//! The Royal Kludge RK71 (SN32F248BF) with this project's QMK firmware (keyboard/rk71_rgb).

pub mod layout;
pub mod settings;
#[cfg(test)]
mod tests;

use super::{Ids, Profile};

pub static PROFILE: Profile = Profile {
    name: "RK71",
    usb: Ids { vid: 0x0C45, pid: 0x8071 },
    bootloader_magic: b"RK71",
    bootloader: Ids { vid: 0x0C45, pid: 0x7040 },
    chip: (32, 3),
    chip_name: "SN32F24xB",
    flash_size: 64 * 1024,
    ram: 0x2000_0000..=0x2000_2000,
    // Stock firmware V1.12 answers as an Apple keyboard; this report sends it to the bootloader.
    stock: Ids { vid: 0x05AC, pid: 0x024F },
    stock_bootloader_report: [0xAA, 0x42, 0x89, 0x5A, 0xFF, 0x71, 0x62, 0xCC],
    // The Bluetooth module keeps the stock id.
    bluetooth: Ids { vid: 0x05AC, pid: 0x024F },
    state_value: (52, 6),
    bluetooth_reports_since: 0x0900,
    win_lock_value: 56,
    // keyboard/rk71_rgb/rk71_rgb.c, rk71_btcfg_read: brightness, effect, speed, hue, saturation,
    // Win/Mac per device, then the packed state.
    bluetooth_slots: &[
        Some(((3, 1), 0)),
        Some(((3, 2), 0)),
        Some(((3, 3), 0)),
        Some(((3, 4), 0)),
        Some(((3, 4), 1)),
        Some(((0, 55), 0)),
        None,
    ],
    bluetooth_device_keys: &["Q", "W", "E", "R", "T"],
    matrix: (5, 16),
    fn_layer: 3, // keyboard/rk71_rgb/config.h RK71_FN_LAYER
    macros_since: 0x0A00,
    // keyboard/rk71_rgb/keymaps/via/keymap.c (the Fn layer and its comment).
    fn_shortcuts: &[
        ("connection", &[
            ("Fn + Q … T", "bt_device"),
            ("Fn + P", "mode"),
            ("Fn + Backspace", "forget"),
            ("Fn + Space", "battery"),
        ]),
        ("layout", &[
            ("Fn + A", "windows"),
            ("Fn + S", "mac"),
            ("Fn + Win", "win_lock"),
            ("Fn + Ctrl", "frow_lock"),
        ]),
        ("lighting", &[
            ("Fn + Enter", "light_toggle"),
            ("Fn + \\", "next_effect"),
            ("Fn + H", "hue"),
            ("Fn + ↑ / ↓", "brightness"),
            ("Fn + ← / →", "speed"),
        ]),
        ("media", &[
            ("Fn + 1 … =", "fkeys"),
            ("Fn + Esc", "grave"),
            ("Fn + Ins", "prtsc"),
            ("Fn + Home / End", "prev_next"),
            ("Fn + Del", "play"),
            ("Fn + PgUp / PgDn", "volume"),
            ("Fn + M", "mute"),
        ]),
        ("other", &[("Fn + Pause", "boot")]),
    ],
    macro_play_value: 64,
    check_battery_value: 66,
    keys: &layout::KEYS,
};
