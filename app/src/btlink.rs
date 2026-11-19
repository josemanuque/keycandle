//! Settings over Bluetooth, one way (computer -> keyboard), through the keyboard's lock LEDs.
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
use hidapi::{HidApi, HidDevice};

/// The Bluetooth module's id, as the computer sees the keyboard over Bluetooth.
const BT_VID: u16 = 0x05AC;
const BT_PID: u16 = 0x024F;
/// Bluetooth HID (the service class in the device path; the same id over USB is stock firmware).
const BT_HID: &str = "00001124";
const ATTEMPTS: usize = 4;
/// Time per symbol: the LED report has to cross Bluetooth and the module.
const SYMBOL: Duration = Duration::from_millis(30);
const ACK_WAIT: Duration = Duration::from_millis(700);

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
    /// The last setting couldn't be delivered (technical message).
    Failed(String),
}

#[derive(Default)]
struct Shared {
    /// Waiting to go, oldest first; a newer value for the same setting replaces the waiting one, so
    /// a slider drag sends only where it stopped.
    pending: Vec<((u8, u8), Vec<u8>)>,
    sending: bool,
    failed: Option<String>,
    closed: bool,
}

/// Sends settings one by one on a background thread, as the app changes them.
pub struct Queue(std::sync::Arc<(std::sync::Mutex<Shared>, std::sync::Condvar)>);

impl Queue {
    pub fn new() -> Self {
        let shared = std::sync::Arc::new((std::sync::Mutex::new(Shared::default()), std::sync::Condvar::new()));
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
                    s.pending.remove(0)
                };
                let result = send(channel, id, &value);
                let mut s = lock.lock().unwrap();
                s.sending = false;
                s.failed = result.err().map(|e| format!("{e:#}"));
            }
        });
        Self(shared)
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
        let s = self.0 .0.lock().unwrap();
        let busy = s.pending.len() + s.sending as usize;
        match (&s.failed, busy) {
            (_, n) if n > 0 => Status::Sending(n),
            (Some(e), _) => Status::Failed(e.clone()),
            _ => Status::Idle,
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
        let api = HidApi::new()?;
        let devices: Vec<HidDevice> = api
            .device_list()
            .filter(|d| d.vendor_id() == BT_VID && d.product_id() == BT_PID && d.usage_page() == 0x0C)
            .filter(|d| d.path().to_string_lossy().to_lowercase().contains(BT_HID))
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
    use hidapi::{BusType, DeviceInfo, HidApi, HidDevice};

    /// HID LED bits, the order the keyboard sees.
    const CAPS: u8 = 0x02;
    const SCROLL: u8 = 0x04;

    /// The RK71's keyboard collection, over Bluetooth.
    fn keyboard(api: &HidApi) -> Option<&DeviceInfo> {
        api.device_list().find(|d| {
            d.vendor_id() == super::BT_VID
                && d.product_id() == super::BT_PID
                && d.bus_type() == BusType::Bluetooth
                && d.usage_page() == 0x01
                && d.usage() == 0x06
        })
    }

    pub fn find() -> Option<()> {
        let api = HidApi::new().ok()?;
        keyboard(&api).map(|_| ())
    }

    pub fn send(symbols: &[u8]) -> Result<()> {
        let api = HidApi::new()?;
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

    #[cfg(not(windows))]
    #[test]
    fn led_report_id_from_descriptor() {
        // Report 1: a keyboard with 5 LED outputs (page 0x08), as boot keyboards describe them.
        let d = [0x05, 0x01, 0x09, 0x06, 0xA1, 0x01, 0x85, 0x01, 0x05, 0x07, 0x81, 0x02, 0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91, 0x02, 0xC0];
        assert_eq!(imp::led_report_in(&d), Some(1));
    }
}
