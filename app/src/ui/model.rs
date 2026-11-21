//! The connected keyboard and its current values. No UI here: the views read from it and write
//! through it.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::btlink;
use crate::macros::Step;
use super::macro_store::Macros;
use crate::keyboard;
use crate::device::{Keyboard, CHANNEL_KEYBOARD};
use crate::preview::effects::{Scene, SideScene};
use crate::settings::{self, value, Setting};

/// Every setting's value, keyed by (channel, id).
pub type Values = HashMap<value::Id, Vec<u8>>;

/// What is on USB (as data; the UI words it in the current language).
#[derive(Clone, Default, PartialEq)]
pub enum Connection {
    /// This firmware, over VIA.
    Usb { protocol: u16 },
    /// Not looked for yet.
    #[default]
    NotFound,
    /// Not found; the reason (a technical message).
    Missing(String),
    /// It was connected and went away.
    Unplugged,
    /// Its bootloader is waiting for firmware.
    FlashMode,
    /// It runs the original Royal Kludge firmware.
    Stock,
    /// Only on Bluetooth: lighting and layouts stay in step; other values are from the last USB connection.
    Bluetooth,
}

/// What the keyboard reports about itself (firmware v0.8+, VIA value 52).
#[derive(Clone, Copy, Default, PartialEq)]
pub struct KeyboardState {
    /// Goes up whenever something changed on the keyboard itself.
    changes: u8,
    pub bluetooth: bool,
    /// Bluetooth device slot, 0..4.
    pub slot: u8,
    pub link: Link,
    pub battery: Battery,
    pub cable: bool,
    pub battery_low: bool,
    pub lighting_on: bool,
    pub mac: bool,
}

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Link {
    #[default]
    Idle,
    Connecting,
    Pairing,
    Connected,
}

#[derive(Clone, Copy, Default, PartialEq)]
pub enum Battery {
    #[default]
    Unknown,
    Low,
    Medium,
    High,
}

impl KeyboardState {
    /// The packed state the keyboard reports over Bluetooth (firmware v0.9+, rk71_btcfg_read): device
    /// slot in bits 0-2, battery level in bits 3-4, battery low in bit 5 (bit 6, the Win-key lock, is
    /// a setting of its own).
    pub fn from_packed(packed: u8) -> Self {
        Self {
            bluetooth: true,
            slot: packed & 7,
            link: Link::Connected,
            battery: battery_level(packed >> 3 & 3),
            battery_low: packed & 0x20 != 0,
            ..Default::default()
        }
    }

    fn from_bytes(v: &[u8]) -> Self {
        Self {
            changes: v[0],
            bluetooth: v[1] != 0,
            slot: v[2],
            link: match v[3] {
                1 => Link::Connecting,
                2 => Link::Pairing,
                3 => Link::Connected,
                _ => Link::Idle,
            },
            battery: battery_level(v[4]),
            cable: v[5] & 1 != 0,
            battery_low: v[5] & 2 != 0,
            lighting_on: v[5] & 4 != 0,
            mac: v[5] & 8 != 0,
        }
    }
}

/// The module's battery level as the firmware numbers it: 0 unknown, 1 low, 2 medium, 3 high.
fn battery_level(level: u8) -> Battery {
    match level {
        1 => Battery::Low,
        2 => Battery::Medium,
        3 => Battery::High,
        _ => Battery::Unknown,
    }
}

/// What a poll found.
pub enum Poll {
    /// Still there, nothing changed.
    Same,
    /// Settings changed on the keyboard (Fn keys, …): these values differ from before.
    Changed(Vec<value::Id>),
    /// Only its state changed (mode, battery, …).
    StateChanged,
    /// It went away.
    Lost,
}

#[derive(Default)]
pub struct Model {
    keyboard: Option<Keyboard>,
    values: Values,
    connection: Connection,
    /// The last failed write, if any (technical message).
    write_error: Option<String>,
    /// 0x00MMmmpp; 0 = older than v0.7 (did not report it).
    firmware_version: Option<u32>,
    /// None for firmware older than v0.8, which doesn't report it.
    state: Option<KeyboardState>,
    /// Sends settings to the keyboard, and hears its changes, while it is only on Bluetooth.
    bluetooth: Option<btlink::Queue>,
    /// Values changed since the cache was last written.
    cache_dirty: bool,
    /// The firmware version last seen over USB (remembered across runs), for while on Bluetooth.
    pub last_firmware: Option<u32>,
    /// The macros and their keys (over USB; editing them needs the cable).
    macros: Option<Macros>,
    /// Over USB: show "saved" / "receiving" until then.
    saved_until: Option<Instant>,
    received_until: Option<Instant>,
    /// When the app last asked the keyboard to check its battery (the Power page).
    battery_asked: Option<Instant>,
    /// Over USB: when the keyboard's state was last read.
    state_read_at: Option<Instant>,
}


/// KEYCANDLE_PREVIEW is set: pages show remembered values without a keyboard (for screenshots and
/// design work).
pub fn preview() -> bool {
    std::env::var_os("KEYCANDLE_PREVIEW").is_some()
}

/// KEYCANDLE_PREVIEW=updates: also pretend version 0.8 is installed, to show the update notices.
pub fn preview_updates() -> bool {
    std::env::var("KEYCANDLE_PREVIEW").is_ok_and(|v| v == "updates")
}

/// KEYCANDLE_PREVIEW=charging or =checking: the battery medium and charging, or being checked.
fn preview_battery(what: &str) -> bool {
    std::env::var("KEYCANDLE_PREVIEW").is_ok_and(|v| v == what)
}

/// How long a finished change stays on show over USB, where changes take no time to send.
const ACTIVITY_SHOWN: Duration = Duration::from_millis(1200);
/// Asking for the battery again sooner than this just repeats the last answer.
const BATTERY_ASK_EVERY: Duration = Duration::from_secs(5);
/// The battery shows as being checked for at most this long (Bluetooth needs a message each way).
const BATTERY_CHECK_TIMEOUT: Duration = Duration::from_secs(8);
/// Over USB the module answers within a few milliseconds; a state read this long after asking has it.
const BATTERY_ANSWER_USB: Duration = Duration::from_millis(400);

impl Model {
    /// Connected over USB (VIA: reads, writes and firmware updates).
    pub fn is_connected(&self) -> bool {
        self.keyboard.is_some() || (preview() && matches!(self.connection, Connection::Usb { .. }))
    }

    /// There are settings to show and change: over USB, or over Bluetooth with values remembered
    /// from the last USB connection.
    pub fn has_settings(&self) -> bool {
        self.is_connected() || ((self.on_bluetooth() || preview()) && !self.values.is_empty())
    }

    pub fn on_bluetooth(&self) -> bool {
        self.connection == Connection::Bluetooth
    }

    /// The Bluetooth sender's progress (None unless only on Bluetooth).
    pub fn bluetooth_status(&self) -> Option<btlink::Status> {
        self.bluetooth.as_ref().map(btlink::Queue::status)
    }

    /// Only on Bluetooth: the keyboard's device slot and battery, once it has reported them
    /// (firmware v0.9+). Over USB, `state` says this and more.
    pub fn bluetooth_state(&self) -> Option<KeyboardState> {
        self.bluetooth.as_ref()?.keyboard_state().map(KeyboardState::from_packed)
    }

    /// Have the keyboard check its battery now, and say when it has (the Power page): over USB a value
    /// write, over Bluetooth a quiet request for its state. Not again within a few seconds.
    pub fn check_battery(&mut self) {
        if self.battery_asked.is_some_and(|at| at.elapsed() < BATTERY_ASK_EVERY) {
            return;
        }
        if let Some(kb) = &self.keyboard {
            if self.state.is_none() {
                return; // older than v0.8: no battery to show
            }
            let _ = kb.set(CHANNEL_KEYBOARD, keyboard::current().check_battery_value, &[1]); // v0.10+
        } else if let Some(queue) = &self.bluetooth {
            queue.ask_state();
        } else {
            return;
        }
        self.battery_asked = Some(Instant::now());
    }

    /// The battery is being checked: asked for (or the keyboard just found over Bluetooth), and no
    /// answer with a level yet.
    pub fn battery_checking(&self) -> bool {
        if preview() {
            return preview_battery("checking");
        }
        if self.keyboard.is_none() && self.bluetooth.is_none() {
            return false;
        }
        let Some(asked) = self.battery_asked.filter(|at| at.elapsed() < BATTERY_CHECK_TIMEOUT) else { return false };
        let known = self.state().copied().or(self.bluetooth_state()).is_some_and(|s| s.battery != Battery::Unknown);
        let answered = match &self.bluetooth {
            Some(queue) => queue.state_at().is_some_and(|at| at > asked),
            None => self.state_read_at.is_some_and(|at| at >= asked + BATTERY_ANSWER_USB),
        };
        !(known && answered)
    }

    /// The values remembered from the last USB connection that the keyboard reports over Bluetooth
    /// (for `--bt-listen N cached`).
    pub fn cached_bluetooth_values() -> Option<Vec<u8>> {
        Self { values: cache::load(), ..Default::default() }.bluetooth_known()
    }

    /// The remembered values the keyboard reports over Bluetooth, in its slot order (all but the
    /// state), if every one is known.
    fn bluetooth_known(&self) -> Option<Vec<u8>> {
        keyboard::current()
            .bluetooth_slots
            .iter()
            .flatten()
            .map(|(id, index)| self.values.get(id).and_then(|v| v.get(*index)).copied())
            .collect()
    }

    /// Only on Bluetooth: the keyboard's firmware turned out not to report its values (older than
    /// v0.9), so changes made on the keyboard don't show here.
    pub fn bluetooth_one_way(&self) -> bool {
        // Firmware seen over USB that reports isn't one-way, however quiet it is (asleep).
        !self.reports_expected() && self.bluetooth.as_ref().and_then(btlink::Queue::keyboard_reports) == Some(false)
    }

    /// The firmware last seen over USB reports its values over Bluetooth.
    fn reports_expected(&self) -> bool {
        self.last_firmware.is_some_and(|v| v >= keyboard::current().bluetooth_reports_since)
    }

    /// The keyboard's macros (over USB).
    pub fn macros(&self) -> Option<&Macros> {
        self.macros.as_ref().filter(|_| self.is_connected())
    }

    /// Over USB with firmware older than the macros (v0.10): they need an update first.
    pub fn macros_need_update(&self) -> bool {
        self.keyboard.is_some() && self.firmware_version.is_none_or(|v| v < keyboard::current().macros_since)
    }

    /// Play macro `n` now, as its key would (it types where the cursor is).
    pub fn play_macro(&self, n: usize) -> anyhow::Result<()> {
        let Some(kb) = &self.keyboard else { anyhow::bail!("connect the keyboard with the USB cable") };
        kb.set(CHANNEL_KEYBOARD, keyboard::current().macro_play_value, &[n as u8])
    }

    /// Store all macros on the keyboard.
    pub fn save_macros(&mut self, all: &[Vec<Step>]) -> anyhow::Result<()> {
        let (Some(kb), Some(m)) = (&self.keyboard, &mut self.macros) else { anyhow::bail!("connect the keyboard with the USB cable") };
        m.save(kb, all)?;
        self.saved_until = Some(Instant::now() + ACTIVITY_SHOWN);
        Ok(())
    }

    /// Run macro `n` with Fn + `key` (or no key).
    pub fn assign_macro(&mut self, n: usize, key: Option<usize>) -> anyhow::Result<()> {
        let (Some(kb), Some(m)) = (&self.keyboard, &mut self.macros) else { anyhow::bail!("connect the keyboard with the USB cable") };
        m.assign(kb, n, key)?;
        self.saved_until = Some(Instant::now() + ACTIVITY_SHOWN);
        Ok(())
    }

    /// Put macros back after a firmware update.
    pub fn restore_macros(&mut self, backup: &Macros) -> anyhow::Result<()> {
        let Some(kb) = &self.keyboard else { anyhow::bail!("not connected") };
        backup.restore(kb)?;
        self.macros = Macros::read(kb).ok();
        Ok(())
    }

    /// What's going on between the app and the keyboard right now, over either connection.
    pub fn activity(&self) -> btlink::Status {
        if let Some(status) = self.bluetooth_status() {
            return status;
        }
        let now = Instant::now();
        if self.received_until.is_some_and(|until| now < until) {
            btlink::Status::Receiving
        } else if self.received_until.is_some_and(|until| now < until + ACTIVITY_SHOWN) {
            btlink::Status::Received
        } else if self.saved_until.is_some_and(|until| now < until) {
            btlink::Status::Saved
        } else if let Some(error) = &self.write_error {
            btlink::Status::Failed(error.clone())
        } else {
            btlink::Status::Idle
        }
    }

    /// Something was on show recently: keep redrawing until it has gone.
    pub fn activity_fading(&self) -> bool {
        let soon = Instant::now().checked_sub(ACTIVITY_SHOWN * 2);
        [self.received_until, self.saved_until].into_iter().flatten().any(|until| soon.is_some_and(|s| s < until))
    }

    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    pub fn write_error(&self) -> Option<&str> {
        self.write_error.as_deref()
    }

    pub fn firmware_version(&self) -> Option<u32> {
        self.firmware_version
    }

    pub fn state(&self) -> Option<&KeyboardState> {
        self.state.as_ref()
    }

    pub fn in_flash_mode(&self) -> bool {
        self.connection == Connection::FlashMode
    }

    /// Firmware can be installed: the keyboard is on the cable (this project's firmware, the
    /// original, or already in flash mode). Never over Bluetooth: the bootloader is USB-only.
    pub fn can_install(&self) -> bool {
        self.is_connected() || self.in_flash_mode() || self.on_stock_firmware()
    }

    pub fn on_stock_firmware(&self) -> bool {
        self.connection == Connection::Stock
    }

    /// Hand the open keyboard over (to the updater, which restarts it into flash mode).
    pub fn take_keyboard(&mut self) -> Option<Keyboard> {
        self.firmware_version = None;
        self.values.clear();
        self.state = None;
        self.keyboard.take()
    }

    /// A copy of every setting, to put back after a firmware update.
    pub fn snapshot(&self) -> Values {
        self.values.clone()
    }

    /// Write saved settings back (after a firmware update reset them), then read them back and retry
    /// any that didn't take. Returns the ones that still differ: (id, wanted, got).
    pub fn restore(&mut self, saved: &Values) -> Vec<(value::Id, Vec<u8>, Vec<u8>)> {
        let mut wrong = Vec::new();
        for attempt in 0..2 {
            for setting in settings::all() {
                if let Some(v) = saved.get(&setting.value_id()) {
                    if attempt > 0 {
                        self.values.remove(&setting.value_id()); // force the write
                    }
                    self.write(setting, v);
                }
            }
            let Some(kb) = &self.keyboard else { break };
            let now = read_all(kb);
            wrong = saved
                .iter()
                .filter_map(|(id, want)| now.get(id).filter(|got| *got != want).map(|got| (*id, want.clone(), got.clone())))
                .collect();
            self.values = now;
            if wrong.is_empty() {
                break;
            }
        }
        wrong
    }

    /// Open the keyboard and read every setting. Returns whether it is connected.
    pub fn connect(&mut self) -> bool {
        self.values.clear();
        self.firmware_version = None;
        // KEYCANDLE_PREVIEW=1: show the pages from the last values seen, as if plugged in (for
        // screenshots and design work without the keyboard; changes go nowhere).
        if preview() {
            self.values = cache::load();
            self.connection = Connection::Usb { protocol: 0 };
            // KEYCANDLE_PREVIEW=updates: an older version, so the update notices show.
            self.firmware_version = Some(if preview_updates() { 0x0800 } else { keyboard::current().macros_since });
            // Wired, charging, battery high, lighting on.
            self.state = Some(KeyboardState::from_bytes(&[0, 0, 0, 3, if preview_battery("charging") { 2 } else { 3 }, 1 | 4]));
            self.macros = Some(Macros::sample());
            return true;
        }
        match Keyboard::open() {
            Ok(kb) => {
                self.firmware_version = kb.firmware_version().ok();
                self.connection = Connection::Usb { protocol: kb.protocol_version };
                self.write_error = None;
                self.state = kb.get(CHANNEL_KEYBOARD, keyboard::current().state_value.0, keyboard::current().state_value.1).ok().map(|v| KeyboardState::from_bytes(&v));
                self.state_read_at = Some(Instant::now());
                self.battery_asked = None;
                self.values = read_all(&kb);
                // Older firmware keeps macros in a tiny space and can't play or match them: no editor.
                let since = keyboard::current().macros_since;
                self.macros = self.firmware_version.filter(|v| *v >= since).and_then(|_| Macros::read(&kb).ok());
                self.keyboard = Some(kb);
                cache::save(&self.values);
            }
            Err(e) => {
                self.keyboard = None;
            self.macros = None;
                self.connection = if crate::isp::is_present() {
                    Connection::FlashMode
                } else if crate::isp::stock_present() {
                    Connection::Stock
                } else if btlink::available() {
                    self.values = cache::load();
                    self.state = None;
                    Connection::Bluetooth
                } else if self.connection == Connection::Unplugged {
                    Connection::Unplugged
                } else {
                    Connection::Missing(format!("{e:#}"))
                };
            }
        }
        if self.on_bluetooth() {
            if self.bluetooth.is_none() {
                // Its first request for the keyboard's values has the battery checked too.
                self.bluetooth = Some(btlink::Queue::new(self.bluetooth_known().as_deref()));
                self.battery_asked = Some(Instant::now());
            }
        } else {
            self.bluetooth = None;
        }
        self.is_connected()
    }

    /// Only on Bluetooth: has the cable come back (connect again), or has the keyboard gone?
    /// Returns whether the connection changed.
    pub fn recheck_bluetooth(&mut self) -> bool {
        self.flush_cache();
        if let Some(queue) = &self.bluetooth {
            queue.ask_again_if_unheard(self.reports_expected());
        }
        let busy = matches!(self.bluetooth_status(), Some(btlink::Status::Sending(_)));
        if !busy && (Keyboard::open().is_ok() || !btlink::available()) {
            self.connection = Connection::NotFound;
            self.bluetooth = None;
            self.values.clear();
            return true;
        }
        false
    }

    /// Only on Bluetooth: take in what the keyboard reported (Fn keys, or its answer to our request
    /// when it was found). Returns the values that changed.
    pub fn bluetooth_updates(&mut self) -> Vec<value::Id> {
        let updates = self.bluetooth.as_ref().map(btlink::Queue::take_updates).unwrap_or_default();
        let mut changed = Vec::new();
        for (id, index, byte) in updates {
            let Some(slot) = self.values.get_mut(&id).and_then(|v| v.get_mut(index)) else { continue };
            if *slot != byte {
                *slot = byte;
                if !changed.contains(&id) {
                    changed.push(id);
                }
            }
        }
        // The Win-key lock comes with the keyboard's state rather than as a value of its own.
        if let Some(packed) = self.bluetooth.as_ref().and_then(btlink::Queue::keyboard_state) {
            let locked = packed >> 6 & 1;
            if let Some(slot) = self.values.get_mut(&(CHANNEL_KEYBOARD, keyboard::current().win_lock_value)).and_then(|v| v.first_mut()) {
                if *slot != locked {
                    *slot = locked;
                    changed.push((CHANNEL_KEYBOARD, keyboard::current().win_lock_value));
                }
            }
        }
        self.cache_dirty |= !changed.is_empty();
        changed
    }

    /// Write the remembered values if they changed (at most once per poll, not per slider step).
    fn flush_cache(&mut self) {
        if std::mem::take(&mut self.cache_dirty) {
            cache::save(&self.values);
        }
    }

    /// Check on the keyboard: still there? Changed anything on its own? Settings are re-read only
    /// when its change counter moved (firmware v0.8+; older firmware is just pinged).
    pub fn poll(&mut self) -> Poll {
        self.flush_cache();
        let Some(kb) = &self.keyboard else { return Poll::Same };
        let fresh = match self.state {
            Some(_) => kb.get(CHANNEL_KEYBOARD, keyboard::current().state_value.0, keyboard::current().state_value.1).map(|v| Some(KeyboardState::from_bytes(&v))),
            None => kb.ping().map(|_| None),
        };
        if fresh.is_ok() {
            self.state_read_at = Some(Instant::now());
        }
        let Ok(fresh) = fresh else {
            self.keyboard = None;
            self.macros = None;
            self.values.clear();
            self.state = None;
            self.connection = Connection::Unplugged;
            return Poll::Lost;
        };
        let (Some(old), Some(new)) = (self.state, fresh) else { return Poll::Same };
        if old == new {
            return Poll::Same;
        }
        self.state = Some(new);
        if old.changes == new.changes {
            return Poll::StateChanged;
        }
        let now = read_all(kb);
        let changed: Vec<value::Id> = now.iter().filter(|(id, v)| self.values.get(id) != Some(v)).map(|(id, _)| *id).collect();
        self.values = now;
        self.cache_dirty |= !changed.is_empty();
        if !changed.is_empty() {
            self.received_until = Some(Instant::now() + ACTIVITY_SHOWN);
        }
        if changed.is_empty() { Poll::StateChanged } else { Poll::Changed(changed) }
    }

    pub fn get(&self, id: value::Id) -> Option<&[u8]> {
        self.values.get(&id).map(Vec::as_slice)
    }

    /// First byte of a value (0 when unknown).
    pub fn byte(&self, id: value::Id) -> u8 {
        self.get(id).map_or(0, |v| v[0])
    }

    /// Hue and saturation of a color value (white when unknown).
    pub fn hue_sat(&self, id: value::Id) -> (u8, u8) {
        self.get(id).filter(|v| v.len() >= 2).map_or((0, 0), |v| (v[0], v[1]))
    }

    /// The option currently chosen for a Choice setting, in the current language.
    pub fn choice_label(&self, id: value::Id) -> String {
        settings::find(id)
            .and_then(|s| s.options().into_iter().nth(self.byte(id) as usize))
            .unwrap_or_else(|| "?".into())
    }

    /// Send a new value to the keyboard and ask it to save once things settle.
    /// Returns false when nothing changed.
    pub fn write(&mut self, setting: &Setting, value: &[u8]) -> bool {
        let id = setting.value_id();
        if self.get(id) == Some(value) {
            return false;
        }
        if let Some(kb) = &self.keyboard {
            if let Err(e) = kb.set(setting.channel, setting.id, value).and_then(|_| kb.save(setting.channel)) {
                self.write_error = Some(format!("{e:#}"));
            } else {
                self.write_error = None;
                self.saved_until = Some(Instant::now() + ACTIVITY_SHOWN);
            }
        } else if let Some(queue) = &self.bluetooth {
            queue.push(setting.channel, setting.id, value);
        }
        self.values.insert(id, value.to_vec());
        self.cache_dirty = true;
        true
    }

    /// Whether a setting applies right now (some only matter for certain values of another).
    pub fn is_shown(&self, setting: &Setting) -> bool {
        match setting.shown_when {
            None => true,
            Some((id, allowed)) => self.get((setting.channel, id)).is_some_and(|v| allowed.contains(&v[0])),
        }
    }

    /// Key lighting as set, for the preview. Brightness is lifted so a dim setting still reads.
    pub fn key_scene(&self, time_ms: f32) -> Scene {
        let (hue, sat) = self.hue_sat(value::KEY_COLOR);
        let brightness = self.byte(value::KEY_BRIGHTNESS) as u16;
        Scene {
            time_ms,
            hue,
            sat,
            val: (96 + brightness * 159 / 255) as u8,
            speed: self.byte(value::KEY_SPEED),
        }
    }

    /// Side lights as set, for the preview.
    pub fn side_scene(&self, time_ms: f32) -> SideScene {
        let mode = self.byte(value::SIDE_MODE);
        let follows_dark_keys = mode == 0 && self.byte(value::KEY_EFFECT) == 0;
        let (hue, sat) = self.hue_sat(if mode == 1 { value::SIDE_COLOR } else { value::KEY_COLOR });
        SideScene {
            time_ms,
            mode: if follows_dark_keys { 2 } else { mode },
            hue,
            sat,
            effect: self.byte(value::SIDE_EFFECT),
            speed: self.byte(value::SIDE_SPEED),
        }
    }
}

/// Every setting's current value.
fn read_all(kb: &Keyboard) -> Values {
    settings::all()
        .filter_map(|setting| kb.get(setting.channel, setting.id, setting.len()).ok().map(|v| (setting.value_id(), v)))
        .collect()
}

/// The keyboard's values from the last USB connection, shown while it is only on Bluetooth (the
/// keyboard can't be read over Bluetooth). Kept next to the app's preferences.
mod cache {
    use super::Values;

    fn path() -> Option<std::path::PathBuf> {
        dirs::config_dir().map(|d| d.join("keycandle").join("keyboard.json"))
    }

    pub fn load() -> Values {
        let entries: Vec<(u8, u8, Vec<u8>)> = path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        entries.into_iter().map(|(channel, id, v)| ((channel, id), v)).collect()
    }

    pub fn save(values: &Values) {
        let Some(path) = path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut entries: Vec<(u8, u8, &Vec<u8>)> = values.iter().map(|((c, i), v)| (*c, *i, v)).collect();
        entries.sort();
        if let Ok(json) = serde_json::to_vec(&entries) {
            let _ = std::fs::write(path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_over_usb() {
        // Changes 7, Bluetooth, device 3, connected, battery medium; cable, lighting on, Mac.
        let s = KeyboardState::from_bytes(&[7, 1, 2, 3, 2, 1 | 4 | 8]);
        assert_eq!(s.changes, 7);
        assert!(s.bluetooth && s.cable && s.lighting_on && s.mac && !s.battery_low);
        assert_eq!(s.slot, 2);
        assert!(s.link == Link::Connected && s.battery == Battery::Medium);
        // Wired, idle, battery unknown, battery low.
        let s = KeyboardState::from_bytes(&[0, 0, 0, 0, 0, 2]);
        assert!(!s.bluetooth && !s.cable && s.battery_low);
        assert!(s.link == Link::Idle && s.battery == Battery::Unknown);
        // A level the firmware doesn't send is unknown, not a panic.
        assert!(KeyboardState::from_bytes(&[0, 0, 0, 9, 7, 0]).battery == Battery::Unknown);
    }

    #[test]
    fn state_over_bluetooth() {
        // Device 3 (slot 2), battery high, low-battery flag, Win-key lock (bit 6, ignored here).
        let s = KeyboardState::from_packed(2 | 3 << 3 | 0x20 | 0x40);
        assert!(s.bluetooth && s.battery_low && !s.cable);
        assert_eq!(s.slot, 2);
        assert!(s.battery == Battery::High && s.link == Link::Connected);
        assert!(KeyboardState::from_packed(1 << 3).battery == Battery::Low);
        assert!(KeyboardState::from_packed(0).battery == Battery::Unknown);
    }
}
