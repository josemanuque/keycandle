//! The keyboard this app is for.
//!
//! Everything specific to one keyboard (its USB and Bluetooth ids, chip, the numbers its firmware
//! gives its values, its devices, layout and settings catalogue) is gathered in a profile here. The
//! rest of the app (the VIA protocol in `device`, Sonix flashing in `isp`, the Bluetooth link in
//! `btlink`, the window in `ui`) asks the profile instead of hard-coding the RK71. Supporting
//! another keyboard means writing its profile, next to `rk71`.

pub mod rk71;

/// A USB (or Bluetooth HID) vendor and product id.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Ids {
    pub vid: u16,
    pub pid: u16,
}

impl Ids {
    pub fn matches(&self, d: &hidapi::DeviceInfo) -> bool {
        d.vendor_id() == self.vid && d.product_id() == self.pid
    }
}

impl std::fmt::Display for Ids {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04X}:{:04X}", self.vid, self.pid)
    }
}

pub struct Profile {
    /// As people call it ("RK71").
    pub name: &'static str,
    /// Our QMK firmware with VIA, over USB.
    pub usb: Ids,
    /// The firmware only jumps to its bootloader when this follows VIA's bootloader-jump command.
    pub bootloader_magic: &'static [u8; 4],
    /// The chip's ROM bootloader (flash mode).
    pub bootloader: Ids,
    /// The chip, as its bootloader reports it (family, model).
    pub chip: (u8, u8),
    pub chip_name: &'static str,
    pub flash_size: usize,
    /// Where a valid firmware image's initial stack pointer may point (the chip's RAM).
    pub ram: std::ops::RangeInclusive<u32>,
    /// The original firmware over USB, and the feature report that restarts it into the bootloader.
    pub stock: Ids,
    pub stock_bootloader_report: [u8; 8],
    /// The keyboard over Bluetooth (its Bluetooth module's id).
    pub bluetooth: Ids,
    /// The firmware's state report: VIA value id and length (see `ui::model::KeyboardState`).
    pub state_value: (u8, usize),
    /// The first firmware version (0x00MMmmpp) that reports its values over Bluetooth.
    pub bluetooth_reports_since: u32,
    /// The Win-key lock's VIA value id (channel 0).
    pub win_lock_value: u8,
    /// What the keyboard reports over Bluetooth, slot by slot: the VIA value and which of its bytes.
    /// `None` is the packed state byte (device slot, battery, Win-key lock).
    pub bluetooth_slots: &'static [Option<((u8, u8), usize)>],
    /// The key matrix (rows, columns), as the firmware's key map is laid out.
    pub matrix: (usize, usize),
    /// The first firmware version (0x00MMmmpp) with this app's macros (2 KB, Ctrl/Cmd, play).
    pub macros_since: u32,
    /// VIA value (channel 0) that plays macro n when set to n.
    pub macro_play_value: u8,
    /// What Fn + a key does, by group: (group key, [(keys, description key)]). Translation keys:
    /// `shortcuts.group.<group>` and `shortcuts.<description>`.
    pub fn_shortcuts: &'static [(&'static str, &'static [(&'static str, &'static str)])],
    /// The Fn layer: macros go there, so the key keeps its normal job.
    pub fn_layer: usize,
    /// The physical keys (label, place, matrix position).
    pub keys: &'static [rk71::layout::Key],
    /// The Fn + key that picks each Bluetooth device; their number is the number of devices. The
    /// cable's bit in the Win/Mac value comes right after them.
    pub bluetooth_device_keys: &'static [&'static str],
}

impl Profile {
    /// The bit of the cable in the Win/Mac per device value.
    pub fn wired_device(&self) -> usize {
        self.bluetooth_device_keys.len()
    }
}

/// The keyboard this build of the app is for.
pub fn current() -> &'static Profile {
    &rk71::PROFILE
}
