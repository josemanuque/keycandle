//! The keyboard over USB: finds the RK71's VIA raw-HID interface and speaks the VIA protocol.
//!
//! VIA packets are 32 bytes: [command, channel, value id, value bytes…]. The keyboard answers each
//! one with a 32-byte packet of the same shape. On the wire a report-ID byte (0) goes in front.

use anyhow::{anyhow, bail, Context, Result};
use hidapi::{HidApi, HidDevice};

pub const VID: u16 = 0x0C45;
pub const PID: u16 = 0x8071;
const VIA_USAGE_PAGE: u16 = 0xFF60;
const VIA_USAGE: u16 = 0x61;
const PACKET: usize = 32;
const TIMEOUT_MS: i32 = 500;

/// VIA command ids (quantum/via.h).
mod cmd {
    pub const GET_PROTOCOL_VERSION: u8 = 0x01;
    pub const CUSTOM_SET_VALUE: u8 = 0x07;
    pub const CUSTOM_GET_VALUE: u8 = 0x08;
    pub const CUSTOM_SAVE: u8 = 0x09;
    pub const UNHANDLED: u8 = 0xFF;
}

/// VIA value channels: 0 = the keyboard's own settings, 3 = QMK RGB matrix.
pub const CHANNEL_KEYBOARD: u8 = 0;
pub const CHANNEL_RGB_MATRIX: u8 = 3;

pub struct Keyboard {
    dev: HidDevice,
    pub protocol_version: u16,
}

impl Keyboard {
    /// Open the first RK71 found (QMK firmware, USB).
    pub fn open() -> Result<Self> {
        let api = HidApi::new().context("cannot start HID access")?;
        let info = api
            .device_list()
            .find(|d| {
                d.vendor_id() == VID && d.product_id() == PID && d.usage_page() == VIA_USAGE_PAGE && d.usage() == VIA_USAGE
            })
            .ok_or_else(|| anyhow!("no RK71 found on USB (VID 0C45, PID 8071)"))?;
        let dev = info.open_device(&api).context(
            "found the RK71 but could not open it. On Linux install tools/udev/50-rk71.rules; close other apps using it",
        )?;
        let mut kb = Keyboard { dev, protocol_version: 0 };
        let r = kb.transfer(&[cmd::GET_PROTOCOL_VERSION])?;
        kb.protocol_version = u16::from_be_bytes([r[1], r[2]]);
        Ok(kb)
    }

    /// Send one VIA packet and return the keyboard's answer.
    fn transfer(&self, data: &[u8]) -> Result<[u8; PACKET]> {
        let mut out = [0u8; PACKET + 1]; // report id 0 + packet
        out[1..1 + data.len()].copy_from_slice(data);
        self.dev.write(&out).context("USB write failed")?;
        let mut answer = [0u8; PACKET];
        let n = self.dev.read_timeout(&mut answer, TIMEOUT_MS).context("USB read failed")?;
        if n == 0 {
            bail!("the keyboard did not answer");
        }
        if answer[0] == cmd::UNHANDLED {
            bail!("the firmware does not know this command (older firmware?)");
        }
        Ok(answer)
    }

    /// Read a value: returns its bytes (1 for most settings, 2 for colours: hue, saturation).
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
