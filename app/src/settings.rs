//! Every setting the app shows, mapped to the firmware's VIA values.
//!
//! Channel 0 values are the keyboard's own settings (keyboard/rk71_rgb/rk71_rgb.c,
//! via_custom_value_command_kb). Channel 3 is QMK's RGB matrix (key lighting).

use crate::device::{CHANNEL_KEYBOARD as KB, CHANNEL_RGB_MATRIX as RGB};

#[derive(Clone, Copy)]
pub enum Kind {
    Toggle,
    Range { min: u8, max: u8 },
    Choice(&'static [&'static str]),
    /// Hue and saturation, as two bytes.
    Colour,
}

#[derive(Clone, Copy)]
pub struct Setting {
    pub channel: u8,
    pub id: u8,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: Kind,
    /// Shown only while another setting (same channel) has one of these values.
    pub shown_when: Option<(u8, &'static [u8])>,
}

impl Setting {
    pub fn len(&self) -> usize {
        if matches!(self.kind, Kind::Colour) { 2 } else { 1 }
    }
}

pub struct Section {
    pub title: &'static str,
    pub settings: &'static [Setting],
}

const fn s(channel: u8, id: u8, label: &'static str, help: &'static str, kind: Kind) -> Setting {
    Setting { channel, id, label, help, kind, shown_when: None }
}

const fn when(mut setting: Setting, id: u8, values: &'static [u8]) -> Setting {
    setting.shown_when = Some((id, values));
    setting
}

/// Key-lighting effects in the firmware's mode order (0 = lighting off). Only the effects enabled in
/// keyboard/rk71_rgb/keyboard.json exist, in QMK's rgb_matrix_effects.inc order.
const KEY_EFFECTS: &[&str] = &[
    "Off", "Solid colour", "Gradient up/down", "Gradient left/right", "Breathing", "Band brightness",
    "Cycle all", "Cycle left/right", "Cycle up/down", "Rainbow chevron", "Cycle pinwheel", "Cycle spiral",
    "Raindrops", "Jellybean raindrops", "Pixel rain", "Typing heatmap", "Digital rain", "Reactive",
    "Reactive (wide, multi)", "Splash", "Multi splash",
];

pub const LIGHTING: &[Section] = &[
    Section {
        title: "Keys",
        settings: &[
            s(RGB, 2, "Effect", "", Kind::Choice(KEY_EFFECTS)),
            s(RGB, 4, "Colour", "", Kind::Colour),
            s(RGB, 1, "Brightness", "", Kind::Range { min: 0, max: 255 }),
            s(RGB, 3, "Effect speed", "", Kind::Range { min: 0, max: 255 }),
        ],
    },
    Section {
        title: "Side lights",
        settings: &[
            s(KB, 1, "Colour", "", Kind::Choice(&["Follow keyboard", "Fixed colour", "Off"])),
            when(s(KB, 2, "Fixed colour", "", Kind::Colour), 1, &[1]),
            when(s(KB, 17, "Effect", "", Kind::Choice(&["Static", "Breathing", "Colour cycle", "Type pulse", "Wave"])), 1, &[0, 1]),
            when(s(KB, 18, "Effect speed", "", Kind::Range { min: 0, max: 31 }), 1, &[0, 1]),
            s(KB, 8, "Brightness", "", Kind::Range { min: 0, max: 31 }),
            s(KB, 9, "Follow the keyboard's brightness", "Off: the side lights keep their own brightness", Kind::Toggle),
            s(KB, 5, "Red balance", "Evens out the bluish diffuser", Kind::Range { min: 0, max: 31 }),
            s(KB, 6, "Green balance", "", Kind::Range { min: 0, max: 31 }),
            s(KB, 7, "Blue balance", "", Kind::Range { min: 0, max: 31 }),
        ],
    },
    Section {
        title: "Status on the side lights",
        settings: &[
            s(KB, 3, "Show status", "Pairing, wired/Bluetooth switches, battery", Kind::Toggle),
            when(s(KB, 10, "Status brightness", "", Kind::Range { min: 0, max: 31 }), 3, &[1]),
            when(s(KB, 19, "Wired colour", "", Kind::Colour), 3, &[1]),
            when(s(KB, 20, "Bluetooth colour", "", Kind::Colour), 3, &[1]),
        ],
    },
];

pub const KEYS: &[Section] = &[
    Section {
        title: "Caps Lock",
        settings: &[
            s(KB, 15, "Caps key", "Auto: cool white, or a dark key when the lighting is white", Kind::Choice(&["Auto", "Off", "Custom colour"])),
            when(s(KB, 21, "Caps key colour", "", Kind::Colour), 15, &[2]),
            s(KB, 11, "Show on the nearest side light", "Only while the lighting is on or you're typing", Kind::Toggle),
            when(s(KB, 12, "Side light colour", "", Kind::Colour), 11, &[1]),
        ],
    },
    Section {
        title: "Other indicators",
        settings: &[
            s(KB, 13, "F-row lock on the farthest side light", "Fn + Ctrl turns 1…= into F1…F12", Kind::Toggle),
            when(s(KB, 14, "F-row lock colour", "", Kind::Colour), 13, &[1]),
            s(KB, 16, "Fn highlight", "While Fn is held, only keys with an Fn function stay lit", Kind::Toggle),
        ],
    },
];

pub const POWER: &[Section] = &[Section {
    title: "On battery",
    settings: &[
        s(KB, 22, "Lighting off after", "", Kind::Choice(&["1 minute", "2 minutes", "5 minutes", "10 minutes", "30 minutes", "Never"])),
        s(KB, 23, "Deep sleep once dark, after", "Any key wakes it at once and is still typed", Kind::Choice(&["5 seconds", "10 seconds", "30 seconds", "1 minute", "5 minutes", "Never"])),
        s(KB, 24, "Battery bar when unplugging or low", "", Kind::Toggle),
        s(KB, 25, "Dim the lighting on battery", "Full brightness returns when plugged in", Kind::Toggle),
        when(s(KB, 26, "Brightness on battery", "", Kind::Range { min: 0, max: 31 }), 25, &[1]),
    ],
}];
