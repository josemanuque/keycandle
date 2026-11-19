//! Settings over Bluetooth, both ways. Computer -> keyboard goes through the keyboard's lock LEDs;
//! keyboard -> computer through reserved media-key usages (see `Reports`).
//!
//! Over Bluetooth the RK71 offers only keyboard and media interfaces; the one thing the computer
//! can send is the lock-LED state, which the module passes on to the keyboard. Windows lets an
//! ordinary app set a keyboard's indicator LEDs, without changing the lock state, through the
//! keyboard's own interface (IOCTL_KEYBOARD_SET_INDICATORS). A setting is sent as a sequence of
//! LED states; the firmware decodes it (keyboard/rk71_rgb/rk71_btcfg.c, which documents the format).
//!
//! The CRC makes the keyboard ignore damaged messages. Each one it applies is confirmed on its
//! media-key channel (which apps can read on every OS): a reserved usage, 0x3F0 | (CRC & 0x0F).
//! Without that confirmation the message is sent again.

use std::time::{Duration, Instant};

use anyhow::{bail, Result};
use hidapi::HidDevice;

use crate::device::hid_api;

/// The Bluetooth module's id, as the computer sees the keyboard over Bluetooth.
const BT_VID: u16 = 0x05AC;
const BT_PID: u16 = 0x024F;
/// Bluetooth HID (the service class in Windows' device path; the same id over USB is stock firmware).
#[cfg(windows)]
const BT_HID: &str = "00001124";

/// One of the RK71's collections, over Bluetooth (not the stock firmware over USB, which shares the
/// id). Windows names the transport only in the path; macOS and Linux paths don't carry it.
fn over_bluetooth(d: &hidapi::DeviceInfo) -> bool {
    if d.vendor_id() != BT_VID || d.product_id() != BT_PID {
        return false;
    }
    #[cfg(windows)]
    return d.path().to_string_lossy().to_lowercase().contains(BT_HID);
    #[cfg(not(windows))]
    return matches!(d.bus_type(), hidapi::BusType::Bluetooth);
}
const ATTEMPTS: usize = 4;
/// Time per symbol: the LED report has to cross Bluetooth and the module.
const SYMBOL: Duration = Duration::from_millis(30);
const ACK_WAIT: Duration = Duration::from_millis(700);
/// A message on this channel asks the keyboard to report its values (firmware: BTCFG_SYNC_CHANNEL).
const SYNC_CHANNEL: u8 = 0xF0;
/// The report the keyboard's confirmations and values arrive in. Other reports (typing, on macOS,
/// where they share a device with it) are dropped unread.
const REPORT_ID: u8 = 8;
/// How long "receiving" stays up after a value arrives, and after asking for all of them (the
/// keyboard waits until messages stop, then sends one value every 100 ms).
const RECEIVE_SHOWN: Duration = Duration::from_millis(1200);
const SYNC_ANSWER: Duration = Duration::from_millis(2600);
/// The first value comes about 2 s after the request is confirmed (the keyboard waits for messages to
/// stop), the last about 4 s after: firmware that has sent nothing by then doesn't report.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(6);
/// How long "received" stays up once the keyboard has finished.
const RECEIVED_SHOWN: Duration = Duration::from_millis(1200);
/// How long "saved" stays up after the last change was confirmed.
const SAVED_SHOWN: Duration = Duration::from_millis(1500);
/// How long to wait before trying again to listen to the keyboard.
const LISTEN_RETRY: Duration = Duration::from_secs(1);
/// What each reported slot is: the value's id and which of its bytes (keep in step with the firmware's
/// rk71_btcfg_read). The last one isn't a setting: the keyboard's state, packed (firmware v0.9+):
/// device slot in use (bits 0-2), battery level (3-4), battery low (5), Win-key lock (6).
const SLOTS: [((u8, u8), usize); 7] =
    [((3, 1), 0), ((3, 2), 0), ((3, 3), 0), ((3, 4), 0), ((3, 4), 1), ((0, 55), 0), (KEYBOARD_STATE, 0)];
/// Stands for the keyboard's state among the reported values (no VIA value has this id).
const KEYBOARD_STATE: (u8, u8) = (0xFF, 0);

/// One byte of a value, as the keyboard reported it.
pub type Update = ((u8, u8), usize, u8);

/// Puts together the values the keyboard reports: two usages each, the high and low half.
#[derive(Default)]
struct Reports {
    high: Option<(u16, u8)>,
}

impl Reports {
    fn feed(&mut self, usage: u16) -> Option<Update> {
        match usage {
            0 => None, // the release after every usage
            0x300..=0x36F => {
                self.high = Some(((usage >> 4) & 0xF, (usage & 0xF) as u8));
                None
            }
            0x380..=0x3EF => {
                let slot = (usage - 0x380) >> 4;
                let (_, high) = self.high.take().filter(|(s, _)| *s == slot)?;
                let (id, index) = *SLOTS.get(slot as usize)?;
                Some((id, index, high << 4 | (usage & 0xF) as u8))
            }
            _ => {
                self.high = None;
                None
            }
        }
    }
}

/// The usage in a report from the keyboard's media-key channel, if it is one.
fn usage_in(report: &[u8]) -> Option<u16> {
    match report {
        [REPORT_ID, low, high, ..] => Some(u16::from_le_bytes([*low, *high])),
        _ => None,
    }
}

/// The message: 4 start symbols, then channel, id, two value bytes and a CRC-8, 2 bits per symbol.
pub fn symbols(channel: u8, id: u8, value: &[u8]) -> Vec<u8> {
    let bytes = [channel, id, value.first().copied().unwrap_or(0), value.get(1).copied().unwrap_or(0)];
    let mut out = vec![1, 2, 1, 2];
    for b in bytes.iter().copied().chain([crc8(&bytes)]) {
        for shift in [6, 4, 2, 0] {
            out.push((b >> shift) & 3);
        }
    }
    out
}

fn crc8(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |mut crc, &b| {
        crc ^= b;
        for _ in 0..8 {
            crc = if crc & 0x80 != 0 { (crc << 1) ^ 0x07 } else { crc << 1 };
        }
        crc
    })
}

/// Time per symbol; KEYCANDLE_BT_SYMBOL_MS overrides it (for trying other computers).
fn symbol_time() -> Duration {
    std::env::var("KEYCANDLE_BT_SYMBOL_MS").ok().and_then(|v| v.parse().ok()).map(Duration::from_millis).unwrap_or(SYMBOL)
}

/// The usage the keyboard confirms a message with.
fn ack_usage(symbols: &[u8]) -> u16 {
    let crc = symbols[symbols.len() - 2..].iter().fold(0u8, |b, s| b << 2 | s); // low 4 bits of the CRC
    0x3F0 | crc as u16
}

/// Is an RK71 connected over Bluetooth that we can send to?
pub fn available() -> bool {
    imp::find().is_some()
}

/// Send one setting (a VIA value write) and wait until the keyboard confirms it. About a second
/// per attempt; call it off the UI thread. Returns the number of attempts it took.
pub fn send(channel: u8, id: u8, value: &[u8]) -> Result<usize> {
    imp::ensure_access()?;
    let message = symbols(channel, id, value);
    let ack = Acks::open()?;
    let want = ack_usage(&message);
    for attempt in 1..=ATTEMPTS {
        ack.drain();
        imp::send(&message)?;
        if ack.wait(want, ACK_WAIT) {
            return Ok(attempt);
        }
    }
    bail!("the keyboard didn't confirm the setting ({ATTEMPTS} attempts)")
}

/// Where the background sender is.
#[derive(Clone, PartialEq)]
pub enum Status {
    /// Everything sent and confirmed.
    Idle,
    /// Settings still to send, counting the one on its way.
    Sending(usize),
    /// The keyboard is telling us its values (it was just found, or they changed on it).
    Receiving,
    /// The keyboard's values just arrived.
    Received,
    /// A change just reached the keyboard.
    Saved,
    /// The last setting couldn't be delivered (technical message).
    Failed(String),
}

#[derive(Default)]
struct Shared {
    /// Waiting to go, oldest first; a newer value for the same setting replaces the waiting one, so
    /// a slider drag sends only where it stopped.
    pending: Vec<((u8, u8), Vec<u8>)>,
    sending: bool,
    /// What's on its way is the request for the keyboard's values, not a change of ours.
    asking: bool,
    /// Show "receiving" until then.
    receiving_until: Option<Instant>,
    /// Values did arrive during this "receiving".
    got_values: bool,
    /// Show "saved" until then.
    saved_until: Option<Instant>,
    failed: Option<String>,
    closed: bool,
    /// Reported by the keyboard, not yet picked up by the app.
    updates: Vec<Update>,
    /// The keyboard's packed state (see SLOTS), once it has said.
    keyboard_state: Option<u8>,
    /// The keyboard has reported something since the queue started.
    heard: bool,
    /// Our request for the keyboard's values couldn't be delivered.
    unanswered: bool,
    /// The keyboard confirmed our request for its values; they're due by then.
    answer_due: Option<Instant>,
}

type Link = std::sync::Arc<(std::sync::Mutex<Shared>, std::sync::Condvar)>;

/// Hear what the keyboard reports, for as long as the queue exists (the window is open).
fn listen(link: Link) {
    let closed = || link.0.lock().unwrap().closed;
    while !closed() {
        let Ok(channel) = imp::ensure_access().and_then(|_| Acks::open()) else {
            std::thread::sleep(LISTEN_RETRY);
            continue;
        };
        let mut reports = Reports::default();
        let mut buf = [0u8; 16];
        'open: while !closed() {
            for device in &channel.0 {
                let Ok(n) = device.read_timeout(&mut buf, 50) else { break 'open };
                if let Some(update) = usage_in(&buf[..n]).and_then(|usage| reports.feed(usage)) {
                    let mut s = link.0.lock().unwrap();
                    s.heard = true;
                    if update.0 == KEYBOARD_STATE {
                        s.keyboard_state = Some(update.2);
                        continue;
                    }
                    s.updates.push(update);
                    s.got_values = true;
                    s.receiving_until = Some(Instant::now() + RECEIVE_SHOWN);
                }
            }
        }
    }
}

/// Diagnostics (--bt-listen): ask the keyboard for its values and print what arrives for `seconds`.
pub fn listen_for(seconds: u64) -> Result<()> {
    imp::ensure_access()?;
    let channel = Acks::open()?;
    let asked = std::thread::spawn(|| send(SYNC_CHANNEL, 0, &[0]));
    let start = Instant::now();
    let mut reports = Reports::default();
    let mut buf = [0u8; 16];
    while start.elapsed() < Duration::from_secs(seconds) {
        for device in &channel.0 {
            let n = device.read_timeout(&mut buf, 20)?;
            let Some(usage) = usage_in(&buf[..n]).filter(|u| *u != 0) else { continue };
            let at = start.elapsed().as_millis();
            match reports.feed(usage) {
                Some((id, index, byte)) => println!("{at:>6} ms  {usage:#05x}  value {id:?}[{index}] = {byte}"),
                None => println!("{at:>6} ms  {usage:#05x}"),
            }
        }
    }
    match asked.join() {
        Ok(Ok(attempts)) => println!("request confirmed (attempt {attempts})"),
        Ok(Err(e)) => println!("request not confirmed: {e:#}"),
        Err(_) => println!("request thread failed"),
    }
    Ok(())
}

/// Sends settings one by one on a background thread, as the app changes them.
pub struct Queue(Link);

impl Queue {
    pub fn new() -> Self {
        let shared = std::sync::Arc::new((std::sync::Mutex::new(Shared::default()), std::sync::Condvar::new()));
        let listener = shared.clone();
        std::thread::spawn(move || listen(listener));
        let worker = shared.clone();
        std::thread::spawn(move || {
            let (lock, wake) = &*worker;
            loop {
                let ((channel, id), value) = {
                    let mut s = lock.lock().unwrap();
                    while s.pending.is_empty() && !s.closed {
                        s = wake.wait(s).unwrap();
                    }
                    if s.closed {
                        return;
                    }
                    s.sending = true;
                    let next = s.pending.remove(0);
                    s.asking = next.0 .0 == SYNC_CHANNEL;
                    next
                };
                let result = send(channel, id, &value);
                let mut s = lock.lock().unwrap();
                s.sending = false;
                if result.is_ok() {
                    if s.asking {
                        s.receiving_until = Some(Instant::now() + SYNC_ANSWER);
                        s.answer_due = Some(Instant::now() + ANSWER_TIMEOUT);
                    } else {
                        s.saved_until = Some(Instant::now() + SAVED_SHOWN);
                    }
                }
                // Our request for the keyboard's values failing isn't a lost change: no error on
                // show, just ask again later (ask_again_if_unheard).
                s.unanswered = s.asking && result.is_err();
                if !s.asking {
                    s.failed = result.err().map(|e| format!("{e:#}"));
                }
                s.asking = false;
            }
        });
        let queue = Self(shared);
        queue.push(SYNC_CHANNEL, 0, &[0]); // ask for the keyboard's current values
        queue
    }

    /// What the keyboard reported since the last call.
    pub fn take_updates(&self) -> Vec<Update> {
        std::mem::take(&mut self.0 .0.lock().unwrap().updates)
    }

    /// Ask for the keyboard's values again if they never came because sending failed (it was asleep,
    /// or out of range, when the app found it). Call now and then.
    pub fn ask_again_if_unheard(&self) {
        let ask = {
            let s = self.0 .0.lock().unwrap();
            !s.heard && !s.sending && s.pending.is_empty() && s.unanswered
        };
        if ask {
            self.push(SYNC_CHANNEL, 0, &[0]);
        }
    }

    /// The keyboard's packed state (see SLOTS), if it reported it.
    pub fn keyboard_state(&self) -> Option<u8> {
        self.0 .0.lock().unwrap().keyboard_state
    }

    /// Whether the keyboard reports its values: None until it's known, false for firmware that
    /// confirmed the request but sent nothing back (older than v0.9).
    pub fn keyboard_reports(&self) -> Option<bool> {
        let s = self.0 .0.lock().unwrap();
        if s.heard {
            Some(true)
        } else {
            s.answer_due.filter(|due| Instant::now() >= *due).map(|_| false)
        }
    }

    pub fn push(&self, channel: u8, id: u8, value: &[u8]) {
        let (lock, wake) = &*self.0;
        let mut s = lock.lock().unwrap();
        match s.pending.iter_mut().find(|(key, _)| *key == (channel, id)) {
            Some((_, waiting)) => *waiting = value.to_vec(),
            None => s.pending.push(((channel, id), value.to_vec())),
        }
        wake.notify_one();
    }

    pub fn status(&self) -> Status {
        let mut s = self.0 .0.lock().unwrap();
        let now = Instant::now();
        let ours = s.pending.iter().filter(|((channel, _), _)| *channel != SYNC_CHANNEL).count();
        let busy = ours + (s.sending && !s.asking) as usize;
        let asking = s.asking || ours < s.pending.len();
        if busy > 0 {
            Status::Sending(busy)
        } else if asking || s.receiving_until.is_some_and(|until| now < until) {
            Status::Receiving
        } else if s.got_values && s.receiving_until.is_some_and(|until| now < until + RECEIVED_SHOWN) {
            Status::Received
        } else {
            s.got_values = false;
            if s.saved_until.is_some_and(|until| now < until) {
                Status::Saved
            } else if let Some(e) = &s.failed {
                Status::Failed(e.clone())
            } else {
                Status::Idle
            }
        }
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        let (lock, wake) = &*self.0;
        lock.lock().unwrap().closed = true;
        wake.notify_one();
    }
}

/// The keyboard's Bluetooth media-key collections, where it confirms messages.
struct Acks(Vec<HidDevice>);

impl Acks {
    fn open() -> Result<Self> {
        let api = hid_api()?;
        let devices: Vec<HidDevice> = api
            .device_list()
            .filter(|d| over_bluetooth(d) && d.usage_page() == 0x0C)
            .filter_map(|d| d.open_device(&api).ok())
            .collect();
        if devices.is_empty() {
            bail!("couldn't open the RK71's Bluetooth media keys to hear its confirmation");
        }
        Ok(Self(devices))
    }

    /// Throw away reports from before this attempt.
    fn drain(&self) {
        let mut buf = [0u8; 16];
        for d in &self.0 {
            while d.read_timeout(&mut buf, 0).is_ok_and(|n| n > 0) {}
        }
    }

    /// Wait for a report carrying `usage` (16-bit, little-endian after the report id).
    fn wait(&self, usage: u16, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 16];
        // Either byte order: what the module puts on the air isn't documented.
        let (le, be) = (usage.to_le_bytes(), usage.to_be_bytes());
        while Instant::now() < deadline {
            for d in &self.0 {
                if let Ok(n) = d.read_timeout(&mut buf, 10) {
                    if n >= 3 && buf[1..n].windows(2).any(|w| w == le || w == be) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::thread::sleep;

    use anyhow::{bail, Result};
    use windows_sys::core::GUID;
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
        DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
    use windows_sys::Win32::System::IO::DeviceIoControl;

    /// GUID_DEVINTERFACE_KEYBOARD
    const KEYBOARD: GUID = GUID::from_u128(0x884b96c3_56ef_11d1_bc8c_00a0c91405dd);
    const IOCTL_SET: u32 = (0x0B << 16) | (0x0002 << 2); // IOCTL_KEYBOARD_SET_INDICATORS
    const IOCTL_QUERY: u32 = (0x0B << 16) | (0x0010 << 2); // IOCTL_KEYBOARD_QUERY_INDICATORS
    /// Windows' indicator flags (they differ from the HID order the keyboard sees).
    const SCROLL: u16 = 1;
    const NUM: u16 = 2;
    const CAPS: u16 = 4;

    /// Windows needs no permission for this.
    pub fn ensure_access() -> Result<()> {
        Ok(())
    }

    /// The RK71's Bluetooth keyboard interface (classic HID, its main keyboard collection).
    pub fn find() -> Option<Vec<u16>> {
        unsafe {
            let info = SetupDiGetClassDevsW(&KEYBOARD, std::ptr::null(), std::ptr::null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
            if info as isize == -1 {
                // INVALID_HANDLE_VALUE
                return None;
            }
            let mut found = None;
            let mut i = 0;
            loop {
                let mut iface: SP_DEVICE_INTERFACE_DATA = std::mem::zeroed();
                iface.cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;
                if SetupDiEnumDeviceInterfaces(info, std::ptr::null(), &KEYBOARD, i, &mut iface) == 0 {
                    break;
                }
                i += 1;
                let mut need = 0u32;
                SetupDiGetDeviceInterfaceDetailW(info, &iface, std::ptr::null_mut(), 0, &mut need, std::ptr::null_mut());
                let mut buf = vec![0u8; need as usize];
                let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
                (*detail).cbSize = std::mem::size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
                if SetupDiGetDeviceInterfaceDetailW(info, &iface, detail, need, std::ptr::null_mut(), std::ptr::null_mut()) == 0 {
                    continue;
                }
                let start = std::ptr::addr_of!((*detail).DevicePath) as *const u16;
                let len = (0..).take_while(|&k| *start.add(k) != 0).count();
                let path: Vec<u16> = std::slice::from_raw_parts(start, len).to_vec();
                let text = String::from_utf16_lossy(&path).to_lowercase();
                // Bluetooth HID (0x1124), the module's id, the main keyboard collection.
                if text.contains("00001124") && text.contains("pid&024f") && text.contains("col01") {
                    let mut p = path;
                    p.push(0);
                    found = Some(p);
                    break;
                }
            }
            SetupDiDestroyDeviceInfoList(info);
            found
        }
    }

    struct Device(HANDLE);

    impl Drop for Device {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    impl Device {
        fn leds(&self) -> Result<u32> {
            let (mut out, mut got) = (0u32, 0u32);
            let ok = unsafe { DeviceIoControl(self.0, IOCTL_QUERY, std::ptr::null(), 0, &mut out as *mut _ as *mut c_void, 4, &mut got, std::ptr::null_mut()) };
            if ok == 0 {
                bail!("couldn't read the keyboard's LEDs");
            }
            Ok(out)
        }

        fn set(&self, raw: u32) -> Result<()> {
            let mut got = 0u32;
            let ok = unsafe { DeviceIoControl(self.0, IOCTL_SET, &raw as *const _ as *const c_void, 4, std::ptr::null_mut(), 0, &mut got, std::ptr::null_mut()) };
            if ok == 0 {
                bail!("Windows refused to set the keyboard's LEDs");
            }
            Ok(())
        }
    }

    pub fn send(symbols: &[u8]) -> Result<()> {
        let Some(path) = find() else { bail!("no RK71 connected over Bluetooth") };
        let h = unsafe { CreateFileW(path.as_ptr(), GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut()) };
        if h == INVALID_HANDLE_VALUE {
            bail!("couldn't open the RK71's Bluetooth keyboard");
        }
        let dev = Device(h);
        let original = dev.leds()?; // LED flags in the high 16 bits, unit id in the low ones
        let unit = original & 0xFFFF;
        let mut clock = (original >> 16) as u16 & SCROLL != 0;
        let symbol = super::symbol_time();
        for &d in symbols {
            clock = !clock;
            let flags = if d & 1 != 0 { NUM } else { 0 } | if d & 2 != 0 { CAPS } else { 0 } | if clock { SCROLL } else { 0 };
            dev.set(((flags as u32) << 16) | unit)?;
            sleep(symbol);
        }
        sleep(symbol * 4); // let the keyboard finish before the LEDs go back
        dev.set(original)
    }
}

/// macOS and Linux (untested so far): these don't keep keyboards from apps, so the LED state goes
/// out as an ordinary HID output report. macOS asks for Input Monitoring the first time; Linux needs
/// access to the hidraw device (tools/udev).
#[cfg(not(windows))]
mod imp {
    use std::thread::sleep;

    use anyhow::{Context, Result};
    use hidapi::{DeviceInfo, HidApi, HidDevice};

    /// HID LED bits, the order the keyboard sees.
    const CAPS: u8 = 0x02;
    const SCROLL: u8 = 0x04;

    /// The RK71's keyboard collection, over Bluetooth.
    fn keyboard(api: &HidApi) -> Option<&DeviceInfo> {
        api.device_list().find(|d| super::over_bluetooth(d) && d.usage_page() == 0x01 && d.usage() == 0x06)
    }

    /// macOS: keyboards can only be opened with Input Monitoring. Asks the first time (the system
    /// shows its prompt) and says where the switch is until it's on.
    #[cfg(target_os = "macos")]
    pub fn ensure_access() -> Result<()> {
        #[link(name = "IOKit", kind = "framework")]
        extern "C" {
            fn IOHIDRequestAccess(request_type: u32) -> bool;
        }
        const LISTEN_EVENT: u32 = 1; // kIOHIDRequestTypeListenEvent
        // SAFETY: takes a plain value and returns whether access is granted.
        anyhow::ensure!(
            unsafe { IOHIDRequestAccess(LISTEN_EVENT) },
            "allow Keycandle in System Settings > Privacy & Security > Input Monitoring"
        );
        Ok(())
    }

    #[cfg(not(target_os = "macos"))]
    pub fn ensure_access() -> Result<()> {
        Ok(())
    }

    pub fn find() -> Option<()> {
        let api = super::hid_api().ok()?;
        keyboard(&api).map(|_| ())
    }

    pub fn send(symbols: &[u8]) -> Result<()> {
        let api = super::hid_api()?;
        let info = keyboard(&api).context("no RK71 connected over Bluetooth")?;
        let dev = info.open_device(&api).context(if cfg!(target_os = "macos") {
            "couldn't open the RK71's Bluetooth keyboard (allow Keycandle in System Settings > Privacy & Security > Input Monitoring)"
        } else {
            "couldn't open the RK71's Bluetooth keyboard (no access to its hidraw device: see tools/udev)"
        })?;
        let id = led_report_id(&dev);
        let set = |leds: u8| dev.write(&[id, leds]).context("couldn't set the keyboard's LEDs");
        let symbol = super::symbol_time();
        // The LEDs' state isn't known here: start from Scroll off, so the first symbol is a clock edge.
        set(0)?;
        sleep(symbol);
        let mut clock = false;
        for &d in symbols {
            clock = !clock;
            set(d & 3 | if clock { SCROLL } else { 0 })?;
            sleep(symbol);
        }
        sleep(symbol * 4); // let the keyboard finish before the LEDs go back
        set(if caps_lock_on() { CAPS } else { 0 })?;
        Ok(())
    }

    /// The report id of the keyboard's LED output report, from its report descriptor (1 if it
    /// can't be read).
    fn led_report_id(dev: &HidDevice) -> u8 {
        let mut buf = [0u8; 4096];
        let Ok(n) = dev.get_report_descriptor(&mut buf) else { return 1 };
        led_report_in(&buf[..n]).unwrap_or(1)
    }

    /// Walk a HID report descriptor: the report id in force at the first Output item on the LED page.
    pub(super) fn led_report_in(d: &[u8]) -> Option<u8> {
        let (mut i, mut page, mut id) = (0, 0u32, 0u8);
        while i < d.len() {
            let prefix = d[i];
            if prefix == 0xFE {
                i += 3 + *d.get(i + 1)? as usize; // long item
                continue;
            }
            let size = [0, 1, 2, 4][(prefix & 3) as usize];
            let value = (0..size).fold(0u32, |v, k| v | (*d.get(i + 1 + k).unwrap_or(&0) as u32) << (8 * k));
            match prefix & 0xFC {
                0x04 => page = value,     // Usage Page
                0x84 => id = value as u8, // Report ID
                0x90 if page == 0x08 => return Some(id), // Output, on the LED page
                _ => {}
            }
            i += 1 + size;
        }
        None
    }

    /// Whether Caps Lock is on, so its light is put back right. Linux: unknown here (the light
    /// catches up at the next lock-key press).
    fn caps_lock_on() -> bool {
        #[cfg(target_os = "macos")]
        {
            #[link(name = "CoreGraphics", kind = "framework")]
            extern "C" {
                fn CGEventSourceFlagsState(state: i32) -> u64;
            }
            const HID_SYSTEM_STATE: i32 = 1;
            const ALPHA_SHIFT: u64 = 0x0001_0000;
            // SAFETY: a plain query with no pointers.
            unsafe { CGEventSourceFlagsState(HID_SYSTEM_STATE) & ALPHA_SHIFT != 0 }
        }
        #[cfg(not(target_os = "macos"))]
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_layout() {
        let s = symbols(0, 8, &[2]);
        assert_eq!(s.len(), 4 + 5 * 4);
        assert_eq!(&s[..4], &[1, 2, 1, 2]);
        assert_eq!(&s[4..8], &[0, 0, 0, 0]); // channel 0
        assert_eq!(&s[8..12], &[0, 0, 2, 0]); // id 8 = 0b00_00_10_00
        assert_eq!(crc8(b"123456789"), 0xF4); // the CRC-8 (poly 0x07) check value
        let crc = crc8(&[0, 8, 2, 0]);
        assert_eq!(ack_usage(&s), 0x3F0 | (crc & 0x0F) as u16);
    }

    #[test]
    fn values_reported_by_the_keyboard() {
        let mut r = Reports::default();
        // Brightness (slot 0) = 0xC8, with the releases between the halves.
        assert_eq!(r.feed(0x30C), None);
        assert_eq!(r.feed(0), None);
        assert_eq!(r.feed(0x388), Some(((3, 1), 0, 0xC8)));
        // Layouts (slot 5) = 0x21; saturation is the second byte of the colour.
        assert_eq!(r.feed(0x352), None);
        assert_eq!(r.feed(0x3D1), Some(((0, 55), 0, 0x21)));
        assert_eq!(r.feed(0x34F), None);
        assert_eq!(r.feed(0x3CF), Some(((3, 4), 1, 0xFF)));
        // The keyboard's state (slot 6): device 3, battery high, Win-key lock = 0x5A.
        assert_eq!(r.feed(0x365), None);
        assert_eq!(r.feed(0x3EA), Some((KEYBOARD_STATE, 0, 0x5A)));
        // A low half without its high half, or from another slot, is dropped.
        assert_eq!(r.feed(0x388), None);
        assert_eq!(r.feed(0x301), None);
        assert_eq!(r.feed(0x398), None);
        // A confirmation in between breaks the pair.
        assert_eq!(r.feed(0x301), None);
        assert_eq!(r.feed(0x3F2), None);
        assert_eq!(r.feed(0x381), None);
    }

    #[test]
    fn only_the_report_channel_is_read() {
        assert_eq!(usage_in(&[8, 0xF8, 0x03]), Some(0x3F8));
        assert_eq!(usage_in(&[1, 0, 0, 4, 0, 0, 0, 0, 0]), None);
        assert_eq!(usage_in(&[]), None);
    }

    #[cfg(not(windows))]
    #[test]
    fn led_report_id_from_descriptor() {
        // Report 1: a keyboard with 5 LED outputs (page 0x08), as boot keyboards describe them.
        let d = [0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01, 0x05, 0x07, 0x81, 0x02, 0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91, 0x02, 0xC0];
        assert_eq!(imp::led_report_in(&d), Some(1));
    }
}
