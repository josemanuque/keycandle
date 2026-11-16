//! The connected keyboard and its current values. No UI here: the views read from it and write
//! through it.

use std::collections::HashMap;

use crate::device::Keyboard;
use crate::preview::effects::{Scene, SideScene};
use crate::settings::{self, value, Kind, Setting};

/// Every setting's value, keyed by (channel, id).
pub type Values = HashMap<value::Id, Vec<u8>>;

#[derive(Default)]
pub struct Model {
    keyboard: Option<Keyboard>,
    values: Values,
    status: String,
    /// 0x00MMmmpp; 0 = older than v0.7 (did not report it).
    firmware_version: Option<u32>,
    /// No keyboard, but its bootloader (flash mode) is on USB.
    in_flash_mode: bool,
}

impl Model {
    pub fn is_connected(&self) -> bool {
        self.keyboard.is_some()
    }

    /// One line about the connection (or why there is none).
    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn firmware_version(&self) -> Option<u32> {
        self.firmware_version
    }

    pub fn in_flash_mode(&self) -> bool {
        self.in_flash_mode
    }

    /// Hand the open keyboard over (to the updater, which restarts it into flash mode).
    pub fn take_keyboard(&mut self) -> Option<Keyboard> {
        self.firmware_version = None;
        self.values.clear();
        self.keyboard.take()
    }

    /// A copy of every setting, to put back after a firmware update.
    pub fn snapshot(&self) -> Values {
        self.values.clone()
    }

    /// Write saved settings back (after a firmware update reset them).
    pub fn restore(&mut self, saved: &Values) {
        for setting in settings::all() {
            if let Some(v) = saved.get(&setting.value_id()) {
                self.write(setting, v);
            }
        }
    }

    /// Open the keyboard and read every setting. Returns whether it is connected.
    pub fn connect(&mut self) -> bool {
        self.values.clear();
        self.firmware_version = None;
        match Keyboard::open() {
            Ok(kb) => {
                self.in_flash_mode = false;
                self.firmware_version = kb.firmware_version().ok();
                self.status = format!("USB · VIA protocol {}", kb.protocol_version);
                for setting in settings::all() {
                    if let Ok(v) = kb.get(setting.channel, setting.id, setting.len()) {
                        self.values.insert(setting.value_id(), v);
                    }
                }
                self.keyboard = Some(kb);
            }
            Err(e) => {
                self.keyboard = None;
                self.in_flash_mode = crate::isp::is_present();
                self.status = if self.in_flash_mode { "In flash mode (bootloader)".into() } else { format!("{e:#}") };
            }
        }
        self.is_connected()
    }

    /// Ping the keyboard; if it stopped answering, forget it. Returns true when it was just lost.
    pub fn check_lost(&mut self) -> bool {
        let lost = self.keyboard.as_ref().is_some_and(|kb| kb.ping().is_err());
        if lost {
            self.keyboard = None;
            self.values.clear();
            self.status = "Unplugged. Waiting for the keyboard…".into();
        }
        lost
    }

    pub fn get(&self, id: value::Id) -> Option<&[u8]> {
        self.values.get(&id).map(Vec::as_slice)
    }

    /// First byte of a value (0 when unknown).
    pub fn byte(&self, id: value::Id) -> u8 {
        self.get(id).map_or(0, |v| v[0])
    }

    /// Hue and saturation of a colour value (white when unknown).
    pub fn hue_sat(&self, id: value::Id) -> (u8, u8) {
        self.get(id).filter(|v| v.len() >= 2).map_or((0, 0), |v| (v[0], v[1]))
    }

    /// The option currently chosen for a Choice setting, as shown in the app.
    pub fn choice_label(&self, id: value::Id) -> &'static str {
        match settings::find(id).map(|s| s.kind) {
            Some(Kind::Choice(options)) => options.get(self.byte(id) as usize).copied().unwrap_or("?"),
            _ => "?",
        }
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
                self.status = format!("Write failed: {e:#}");
            }
        }
        self.values.insert(id, value.to_vec());
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
        let (hue, sat) = self.hue_sat(value::KEY_COLOUR);
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
        let (hue, sat) = self.hue_sat(if mode == 1 { value::SIDE_COLOUR } else { value::KEY_COLOUR });
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
