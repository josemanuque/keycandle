//! The RK71's settings, mapped to its firmware's VIA values: the pages, their sections and settings,
//! the key-lighting effects, and the values the app reads by name.
//!
//! Channel 0 values are the keyboard's own settings (keyboard/rk71_rgb/rk71_rgb.c,
//! via_custom_value_command_kb). Channel 3 is QMK's RGB matrix (key lighting).

use crate::device::{CHANNEL_KEYBOARD as KB, CHANNEL_RGB_MATRIX as RGB};
use crate::i18n;
use crate::settings::{else_note, inline, ordered, s, section, when, Kind, Section, Setting};

/// Key-lighting effects in the firmware's mode order (0 = lighting off). Only the effects enabled in
/// keyboard/rk71_rgb/keyboard.json exist, in QMK's rgb_matrix_effects.inc order.
pub const KEY_EFFECTS: &[&str] = &[
    "off", "solid", "gradient_up_down", "gradient_left_right", "breathing", "band", "cycle_all",
    "cycle_left_right", "cycle_up_down", "chevron", "pinwheel", "spiral", "raindrops", "jellybean",
    "pixel_rain", "heatmap", "digital_rain", "reactive", "reactive_wide", "splash", "multi_splash",
];

/// An effect's name in the current language.
pub fn effect_name(effect: usize) -> String {
    KEY_EFFECTS.get(effect).map_or_else(|| "?".into(), |k| i18n::text(&format!("effect.{k}")))
}

/// Effects that use the color you pick (solid, or shades and waves of it), in gallery order.
pub const COLOR_EFFECTS: &[u8] = &[1, 4, 5, 2, 3, 12, 17, 18, 19, 20];
/// Effects that make their own colors, in gallery order.
pub const MULTICOLOR_EFFECTS: &[u8] = &[6, 7, 8, 9, 10, 11, 13, 14, 15, 16];
/// Effects that react to typing.
pub const TYPING_EFFECTS: &[u8] = &[15, 17, 18, 19, 20];
/// Effects that move, so their speed matters (all but off and solid).
pub const ANIMATED_EFFECTS: &[u8] = &[2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20];

/// The key-lighting effect. Chosen in the Lighting page's effect gallery rather than in a card.
pub static KEY_EFFECT: Setting = s(RGB, 2, "key_effect", Kind::Choice(KEY_EFFECTS));

const SIDE_MODES: &[&str] = &["follow_keyboard", "fixed_color", "off"];
const SIDE_EFFECTS: &[&str] = &["static", "breathing", "color_cycle", "type_pulse", "wave"];
const CAPS_KEY_MODES: &[&str] = &["auto", "off", "custom_color", "dark"];
const LIGHTS_OFF_TIMES: &[&str] = &["min_1", "min_2", "min_5", "min_10", "min_30", "never"];
const SLEEP_TIMES: &[&str] = &["sec_5", "sec_10", "sec_30", "min_1", "min_5", "never"];

pub const LIGHTING: &[Section] = &[
    section(
        "keys",
        &[
            else_note(when(s(RGB, 4, "key_color", Kind::Color), 2, COLOR_EFFECTS), "color_unused"),
            s(RGB, 1, "key_brightness", Kind::Range { min: 0, max: 255 }),
            else_note(when(s(RGB, 3, "key_speed", Kind::Range { min: 0, max: 255 }), 2, ANIMATED_EFFECTS), "speed_unused"),
        ],
    ),
    section(
        "side_lights",
        &[
            s(KB, 1, "side_mode", Kind::Choice(SIDE_MODES)),
            when(s(KB, 2, "side_color", Kind::Color), 1, &[1]),
            when(s(KB, 17, "side_effect", Kind::Choice(SIDE_EFFECTS)), 1, &[0, 1]),
            when(s(KB, 18, "side_speed", Kind::Range { min: 0, max: 31 }), 1, &[0, 1]),
            s(KB, 9, "side_follow_brightness", Kind::Toggle),
            else_note(when(s(KB, 8, "side_brightness", Kind::Range { min: 0, max: 31 }), 9, &[0]), "side_brightness_follows"),
            s(KB, 5, "side_red", Kind::Range { min: 0, max: 31 }),
            s(KB, 6, "side_green", Kind::Range { min: 0, max: 31 }),
            s(KB, 7, "side_blue", Kind::Range { min: 0, max: 31 }),
        ],
    ),
    section(
        "status",
        &[
            s(KB, 3, "status_show", Kind::Toggle),
            when(s(KB, 10, "status_brightness", Kind::Range { min: 0, max: 31 }), 3, &[1]),
            when(s(KB, 19, "status_wired_color", Kind::Color), 3, &[1]),
            when(s(KB, 20, "status_bluetooth_color", Kind::Color), 3, &[1]),
        ],
    ),
];

/// Caps key modes as shown: Off (no indication) last. Firmware values: 0 auto, 1 off, 2 custom, 3 dark.
const CAPS_KEY_ORDER: &[u8] = &[0, 2, 3, 1];

pub const KEYS: &[Section] = &[
    section(
        "caps_lock",
        &[
            ordered(s(KB, 15, "caps_key", Kind::Choice(CAPS_KEY_MODES)), CAPS_KEY_ORDER),
            when(s(KB, 21, "caps_key_color", Kind::Color), 15, &[2]),
            s(KB, 11, "caps_side", Kind::Toggle),
            inline(s(KB, 12, "caps_side_color", Kind::Color)),
        ],
    ),
    section(
        "other_locks",
        &[
            s(KB, 13, "frow_side", Kind::Toggle),
            inline(s(KB, 14, "frow_side_color", Kind::Color)),
            s(KB, 16, "fn_highlight", Kind::Toggle),
            s(KB, 56, "win_lock", Kind::Toggle),
        ],
    ),
    Section { key: "layout", settings: &[s(KB, 55, "device_layouts", Kind::DeviceLayouts)], wide: true },
];

/// Macros play Ctrl as Cmd on devices set to Mac (firmware v0.10). On the Macros page.
pub static MACRO_MATCH_OS: Setting = s(KB, 57, "macro_match_os", Kind::Toggle);

/// The battery bar toggle sits in the Power page's battery card.
pub static BATTERY_BAR: Setting = s(KB, 24, "battery_bar", Kind::Toggle);

pub const POWER: &[Section] = &[section(
    "on_battery",
    &[
        s(KB, 22, "lights_off_after", Kind::Choice(LIGHTS_OFF_TIMES)),
        s(KB, 23, "sleep_after", Kind::Choice(SLEEP_TIMES)),
        s(KB, 25, "dim_on_battery", Kind::Toggle),
        inline(when(s(KB, 26, "dim_level", Kind::Range { min: 0, max: 31 }), 25, &[1])),
    ],
)];

/// The values the app reads by name (previews and the overview), as (channel, id).
pub mod value {
    use super::{KB, RGB};
    pub type Id = (u8, u8);
    pub const KEY_BRIGHTNESS: Id = (RGB, 1);
    pub const KEY_EFFECT: Id = (RGB, 2);
    pub const KEY_SPEED: Id = (RGB, 3);
    pub const KEY_COLOR: Id = (RGB, 4);
    pub const SIDE_MODE: Id = (KB, 1);
    pub const SIDE_COLOR: Id = (KB, 2);
    pub const SIDE_EFFECT: Id = (KB, 17);
    pub const SIDE_SPEED: Id = (KB, 18);
    pub const CAPS_SIDE_LIGHT: Id = (KB, 11);
    pub const LIGHTS_OFF_AFTER: Id = (KB, 22);
}

