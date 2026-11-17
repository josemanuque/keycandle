//! The SN32F248B's ROM bootloader (USB 0C45:7040): writes a firmware image.
//!
//! The bootloader lives in mask ROM, so it is always there: a failed or interrupted write is fixed
//! by entering it again (Fn + Pause on this firmware, or the BOOT pad, see stock/README.md) and
//! writing again.
//!
//! Protocol (Sonix ISP, as also used by SonixQMK's SonixFlasherC): 64-byte HID feature reports.
//! A command is [cmd, 0xAA, 0x55, 0, args…]; the reply starts with u32 LE (0x55AA << 8 | cmd)
//! then u32 LE status 0xFAFAFAFA. Checksums are the 16-bit sum of the image's LE halfwords.

use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use hidapi::{HidApi, HidDevice};

pub const VID: u16 = 0x0C45;
pub const PID: u16 = 0x7040;
const REPORT: usize = 64;
const BASE: u16 = 0x55AA;
const ACK: u32 = 0xFAFA_FAFA;

mod cmd {
    pub const GET_VERSION: u8 = 0x01;
    pub const SET_SECURITY: u8 = 0x03;
    pub const PROGRAM: u8 = 0x05;
    pub const CHECKSUM: u8 = 0x06;
    pub const USER_MODE: u8 = 0x07;
}

/// SN32F24xB: family 32, chip 3; 64 KB of flash; code security level 0 reads as 0x0000.
const FAMILY: u8 = 32;
const CHIP_SN32F24XB: u8 = 3;
pub const FLASH_SIZE: usize = 64 * 1024;
const SECURITY_OFF: u16 = 0x0000;
const SECURITY_OFF_ALT: u16 = 0xFFFF;

/// The original Royal Kludge firmware shows up with an Apple keyboard's id.
const STOCK_VID: u16 = 0x05AC;
const STOCK_PID: u16 = 0x024F;
/// The stock firmware jumps to the bootloader when it gets this feature report (what the vendor's
/// updater sends; see tools/stock_enter_isp.py).
const STOCK_ISP_MAGIC: [u8; 8] = [0xAA, 0x42, 0x89, 0x5A, 0xFF, 0x71, 0x62, 0xCC];

/// Is an RK71 running the stock firmware connected? A real Apple keyboard shares the id, so this also
/// requires the stock firmware's vendor-defined collection on interface 1.
pub fn stock_present() -> bool {
    HidApi::new().is_ok_and(|api| {
        let mut devices = api.device_list().filter(|d| d.vendor_id() == STOCK_VID && d.product_id() == STOCK_PID);
        devices.any(|d| d.interface_number() == 1 && d.usage_page() >= 0xFF00)
    })
}

/// Restart a stock-firmware RK71 into its bootloader.
pub fn enter_from_stock() -> Result<()> {
    let api = HidApi::new().context("cannot start HID access")?;
    let info = api
        .device_list()
        .find(|d| d.vendor_id() == STOCK_VID && d.product_id() == STOCK_PID && d.interface_number() == 0)
        .context("no RK71 with the original firmware found on USB")?;
    // hidapi falls back to opening without read/write access, which is all a feature report needs
    // (Windows keeps keyboards open exclusively).
    let dev = info.open_device(&api).context("could not open the keyboard")?;
    let mut report = [0u8; REPORT + 1];
    report[1..1 + STOCK_ISP_MAGIC.len()].copy_from_slice(&STOCK_ISP_MAGIC);
    // The firmware jumps away during the transfer, so an error here is normal.
    let _ = dev.send_feature_report(&report);
    Ok(())
}

/// Is the keyboard in its bootloader right now?
pub fn is_present() -> bool {
    HidApi::new().is_ok_and(|api| api.device_list().any(|d| d.vendor_id() == VID && d.product_id() == PID))
}

/// Wait for the bootloader to appear (after the keyboard was told to restart into it).
pub fn wait_for(timeout: Duration) -> bool {
    let end = Instant::now() + timeout;
    while Instant::now() < end {
        if is_present() {
            return true;
        }
        sleep(Duration::from_millis(250));
    }
    false
}

/// Check that a file looks like firmware for this keyboard before anything is written.
pub fn check_image(image: &[u8]) -> Result<()> {
    if image.len() < 0x100 || image.len() > FLASH_SIZE {
        bail!("the file is {} bytes; firmware for this keyboard is 256 bytes to 64 KB", image.len());
    }
    // An Arm Cortex-M0 image starts with its stack pointer (in RAM) and reset address (in flash).
    let word = |i: usize| u32::from_le_bytes(image[i..i + 4].try_into().unwrap());
    let (stack, reset) = (word(0), word(4));
    if !(0x2000_0000..=0x2000_2000).contains(&stack) || reset as usize >= FLASH_SIZE || reset & 1 == 0 {
        bail!("this does not look like SN32 keyboard firmware (stack {stack:#x}, reset {reset:#x})");
    }
    Ok(())
}

/// 16-bit sum of little-endian halfwords, as the bootloader computes it.
pub fn checksum(data: &[u8]) -> u16 {
    data.chunks(2).fold(0u16, |sum, c| sum.wrapping_add(u16::from_le_bytes([c[0], *c.get(1).unwrap_or(&0)])))
}

pub struct Bootloader {
    dev: HidDevice,
    pub security: u16,
}

impl Bootloader {
    /// Open the bootloader and check it is the RK71's chip.
    pub fn open() -> Result<Self> {
        let api = HidApi::new().context("cannot start HID access")?;
        let dev = api.open(VID, PID).context(
            "found the bootloader but could not open it. On Linux install tools/udev/50-rk71.rules",
        )?;
        let mut bl = Bootloader { dev, security: 0 };
        let r = bl.command(cmd::GET_VERSION, &[])?;
        if r[8] != FAMILY || r[9] != CHIP_SN32F24XB {
            bail!("unexpected chip {}.{} in the bootloader (expected the RK71's SN32F24xB)", r[8], r[9]);
        }
        bl.security = u16::from_le_bytes([r[14], r[15]]);
        Ok(bl)
    }

    /// Checksum of the whole flash as the bootloader sees it.
    pub fn flash_checksum(&self) -> Result<u16> {
        let r = self.command(cmd::CHECKSUM, &[])?;
        Ok(u16::from_le_bytes([r[8], r[9]]))
    }

    /// Write `image` at address 0 and verify it. `progress` gets 0.0..=1.0.
    pub fn write(&self, image: &[u8], mut progress: impl FnMut(f32)) -> Result<()> {
        check_image(image)?;
        let mut padded = image.to_vec();
        padded.resize(image.len().div_ceil(REPORT) * REPORT, 0);
        let chunks = padded.len() / REPORT;

        if self.security != SECURITY_OFF && self.security != SECURITY_OFF_ALT {
            self.command(cmd::SET_SECURITY, &[0, 0, SECURITY_OFF as u8, (SECURITY_OFF >> 8) as u8])
                .context("could not turn off code security")?;
        }

        // Start programming at offset 0 (the SN32F24xB erases as it goes).
        let mut args = [0u8; 8];
        args[4..8].copy_from_slice(&(chunks as u32).to_le_bytes());
        self.command(cmd::PROGRAM, &args).context("the bootloader refused to start writing")?;

        for (i, chunk) in padded.chunks(REPORT).enumerate() {
            self.send(chunk).with_context(|| format!("writing block {} of {chunks}", i + 1))?;
            progress((i + 1) as f32 / chunks as f32);
        }

        let r = self.reply(cmd::PROGRAM).context("the bootloader did not confirm the write")?;
        let last = &padded[padded.len() - 4..];
        if &r[REPORT - 4..] != last {
            bail!("the bootloader received a different last block; nothing is lost, try again");
        }
        let expected = checksum(&padded);
        let written = u16::from_le_bytes([r[8], r[9]]);
        if written != expected {
            bail!("checksum mismatch after writing ({written:#06x}, expected {expected:#06x}); try again");
        }
        if self.flash_checksum()? != expected {
            bail!("the flash does not read back as written; try again");
        }
        Ok(())
    }

    /// Leave the bootloader and start the firmware.
    pub fn start_firmware(&self) -> Result<()> {
        self.send(&command_packet(cmd::USER_MODE, &[]))
    }

    fn command(&self, id: u8, args: &[u8]) -> Result<[u8; REPORT]> {
        self.send(&command_packet(id, args))?;
        self.reply(id)
    }

    fn send(&self, data: &[u8]) -> Result<()> {
        let mut buf = [0u8; REPORT + 1]; // report id 0 + report
        buf[1..1 + data.len()].copy_from_slice(data);
        self.dev.send_feature_report(&buf).context("USB write to the bootloader failed")
    }

    fn reply(&self, id: u8) -> Result<[u8; REPORT]> {
        let mut last_err = None;
        for _ in 0..5 {
            let mut buf = [0u8; REPORT + 1];
            match self.dev.get_feature_report(&mut buf) {
                Ok(n) if n == REPORT + 1 => {
                    let mut r = [0u8; REPORT];
                    r.copy_from_slice(&buf[1..]);
                    let answer = u32::from_le_bytes(r[0..4].try_into().unwrap());
                    let status = u32::from_le_bytes(r[4..8].try_into().unwrap());
                    if answer != (BASE as u32) << 8 | id as u32 {
                        bail!("unexpected answer {answer:#010x} from the bootloader");
                    }
                    if status != ACK {
                        bail!("the bootloader reported an error ({status:#010x})");
                    }
                    return Ok(r);
                }
                Ok(n) => bail!("short answer from the bootloader ({n} bytes)"),
                Err(e) => last_err = Some(e), // busy: try again shortly
            }
            sleep(Duration::from_millis(100));
        }
        Err(last_err.map(anyhow::Error::from).unwrap_or_else(|| anyhow::anyhow!("no answer")))
            .context("the bootloader did not answer")
    }
}

fn command_packet(id: u8, args: &[u8]) -> [u8; REPORT] {
    let mut p = [0u8; REPORT];
    p[0] = id;
    p[1..3].copy_from_slice(&BASE.to_le_bytes());
    p[4..4 + args.len()].copy_from_slice(args);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_is_sum_of_halfwords() {
        assert_eq!(checksum(&[0x01, 0x02, 0x03, 0x04]), 0x0201 + 0x0403);
        assert_eq!(checksum(&[0xFF, 0xFF, 0x02, 0x00]), 0x0001);
    }

    #[test]
    fn rejects_non_firmware() {
        assert!(check_image(&[0u8; 0x400]).is_err());
        let mut fw = vec![0u8; 0x400];
        fw[0..4].copy_from_slice(&0x2000_0800u32.to_le_bytes());
        fw[4..8].copy_from_slice(&0x0000_0101u32.to_le_bytes());
        assert!(check_image(&fw).is_ok());
    }
}
