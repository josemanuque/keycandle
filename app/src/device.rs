//! The keyboard over USB: finds the RK71's VIA raw-HID interface and speaks the VIA protocol.
//!
//! VIA packets are 32 bytes: [command, channel, value id, value bytes…]. The keyboard answers each
//! one with a 32-byte packet of the same shape. On the wire a report-ID byte (0) goes in front.

use anyhow::{anyhow, bail, Context, Result};
use hidapi::{HidApi, HidDevice};

const VIA_USAGE_PAGE: u16 = 0xFF60;
const VIA_USAGE: u16 = 0x61;
const PACKET: usize = 32;
const TIMEOUT_MS: i32 = 500;

/// VIA command ids (quantum/via.h).
mod cmd {
    pub const GET_PROTOCOL_VERSION: u8 = 0x01;
    pub const GET_KEYBOARD_VALUE: u8 = 0x02;
    pub const CUSTOM_SET_VALUE: u8 = 0x07;
    pub const CUSTOM_GET_VALUE: u8 = 0x08;
    pub const CUSTOM_SAVE: u8 = 0x09;
    pub const BOOTLOADER_JUMP: u8 = 0x0B;
    pub const UNHANDLED: u8 = 0xFF;
    /// GET_KEYBOARD_VALUE ids
    pub const FIRMWARE_VERSION: u8 = 0x04;
}

/// VIA value channels: 0 = the keyboard's own settings, 3 = QMK RGB matrix.
pub const CHANNEL_KEYBOARD: u8 = 0;
pub const CHANNEL_RGB_MATRIX: u8 = 3;

/// Start HID access and list the devices. One at a time: the app does this from several threads, and
/// hidapi's device search on macOS shares one manager object.
pub fn hid_api() -> hidapi::HidResult<HidApi> {
    static SEARCH: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = SEARCH.lock().unwrap_or_else(|e| e.into_inner());
    HidApi::new()
}

pub struct Keyboard {
    dev: HidDevice,
    pub protocol_version: u16,
}

impl Keyboard {
    /// Open the first RK71 found (QMK firmware, USB).
    pub fn open() -> Result<Self> {
        let kb = crate::keyboard::current();
        let api = hid_api().context("cannot start HID access")?;
        let info = api
            .device_list()
            .find(|d| {
                kb.usb.matches(d) && d.usage_page() == VIA_USAGE_PAGE && d.usage() == VIA_USAGE
            })
            .ok_or_else(|| anyhow!("no {} found on USB ({})", kb.name, kb.usb))?;
        let dev = info.open_device(&api).context(
            "found the RK71 but could not open it. On Linux install tools/udev/50-rk71.rules; close other apps using it",
        )?;
        let mut kb = Keyboard { dev, protocol_version: 0 };
        let r = kb.transfer(&[cmd::GET_PROTOCOL_VERSION])?;
        kb.protocol_version = u16::from_be_bytes([r[1], r[2]]);
        Ok(kb)
    }

    /// Cheap "still there?" check (asks for the protocol version).
    pub fn ping(&self) -> Result<()> {
        self.transfer(&[cmd::GET_PROTOCOL_VERSION]).map(|_| ())
    }

    /// The firmware's version, 0x00MMmmpp (config.h VIA_FIRMWARE_VERSION). 0 = older than v0.7,
    /// which did not report one.
    pub fn firmware_version(&self) -> Result<u32> {
        let r = self.transfer(&[cmd::GET_KEYBOARD_VALUE, cmd::FIRMWARE_VERSION])?;
        Ok(u32::from_be_bytes([r[2], r[3], r[4], r[5]]))
    }

    /// Restart the keyboard into its ROM bootloader (USB 0C45:7040) for a firmware update.
    /// Errors when the firmware is too old to do this; then Fn + Pause does the same by hand.
    pub fn enter_bootloader(self) -> Result<()> {
        let mut p = vec![cmd::BOOTLOADER_JUMP];
        p.extend_from_slice(crate::keyboard::current().bootloader_magic);
        match self.exchange(&p) {
            Ok(answer) if answer[0] == cmd::UNHANDLED => bail!("this firmware cannot restart itself for updates"),
            // Otherwise it is restarting; it may even vanish before its answer is read.
            _ => Ok(()),
        }
    }

    /// Send one VIA packet and return the keyboard's answer.
    fn transfer(&self, data: &[u8]) -> Result<[u8; PACKET]> {
        let answer = self.exchange(data)?;
        if answer[0] == cmd::UNHANDLED {
            bail!("the firmware does not know this command (older firmware?)");
        }
        Ok(answer)
    }

    /// Write a packet and read its answer. VIA answers echo the request (command, and for custom
    /// values the channel and id), so an answer that doesn't match is a stale one left over from
    /// an earlier request: skip it, or values end up filed under the wrong setting.
    fn exchange(&self, data: &[u8]) -> Result<[u8; PACKET]> {
        let mut out = [0u8; PACKET + 1]; // report id 0 + packet
        out[1..1 + data.len()].copy_from_slice(data);
        self.dev.write(&out).context("USB write failed")?;
        let echoed = if matches!(data[0], cmd::CUSTOM_GET_VALUE | cmd::CUSTOM_SET_VALUE) { 3 } else { 1 };
        for _ in 0..4 {
            let mut answer = [0u8; PACKET];
            let n = self.dev.read_timeout(&mut answer, TIMEOUT_MS).context("USB read failed")?;
            if n == 0 {
                bail!("the keyboard did not answer");
            }
            let matches = answer[0] == cmd::UNHANDLED || answer[..echoed] == data[..echoed.min(data.len())];
            if matches {
                return Ok(answer);
            }
        }
        bail!("the keyboard's answers are out of step")
    }

    /// Read a value: returns its bytes (1 for most settings, 2 for colors: hue, saturation).
    pub fn get(&self, channel: u8, id: u8, len: usize) -> Result<Vec<u8>> {
        let r = self.transfer(&[cmd::CUSTOM_GET_VALUE, channel, id])?;
        Ok(r[3..3 + len].to_vec())
    }

    /// Write a value. Takes effect at once; call `save` to keep it across power-off.
    pub fn set(&self, channel: u8, id: u8, value: &[u8]) -> Result<()> {
        let mut p = vec![cmd::CUSTOM_SET_VALUE, channel, id];
        p.extend_from_slice(value);
        self.transfer(&p).map(|_| ())
    }

    /// Ask the keyboard to store the channel's settings in flash. The firmware waits until the
    /// changes settle (2 s) before writing, so this is cheap to call after every change.
    pub fn save(&self, channel: u8) -> Result<()> {
        self.transfer(&[cmd::CUSTOM_SAVE, channel]).map(|_| ())
    }
}
