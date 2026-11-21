//! The app's picture of the RK71 agrees with its firmware (keyboard/rk71_rgb) and its VIA definition,
//! and every text the profile and catalogue use exists. The firmware's sources are read as text, so a
//! value renumbered on one side only fails here rather than on the keyboard.

use std::collections::{BTreeMap, BTreeSet};

use super::PROFILE;
use crate::device::CHANNEL_KEYBOARD;
use crate::settings::{self, Kind, Section, KEY_EFFECTS, KEYS as KEYS_PAGE, LIGHTING, POWER};

fn firmware(file: &str) -> String {
    let path = format!("{}/../keyboard/rk71_rgb/{file}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The firmware's VIA value ids (the `ID_…` enums in rk71_rgb.c), numbered as C numbers them.
fn firmware_ids() -> BTreeMap<String, u32> {
    let source = firmware("rk71_rgb.c");
    let mut ids = BTreeMap::new();
    let mut rest = source.as_str();
    while let Some(start) = rest.find("enum {") {
        let body = &rest[start + 6..];
        let end = body.find("};").unwrap();
        let mut next = 0u32;
        for entry in body[..end].lines().map(|l| l.split("/*").next().unwrap().trim()).flat_map(|l| l.split(',')) {
            let entry = entry.trim();
            if !entry.starts_with("ID_") {
                continue;
            }
            let (name, value) = match entry.split_once('=') {
                Some((name, expr)) => {
                    // A number, or another id plus a number.
                    let value = expr.split('+').map(str::trim).fold(0, |sum, term| {
                        sum + term.parse::<u32>().unwrap_or_else(|_| *ids.get(term).unwrap_or_else(|| panic!("unknown id {term}")))
                    });
                    (name.trim(), value)
                }
                None => (entry, next),
            };
            ids.insert(name.to_string(), value);
            next = value + 1;
        }
        rest = &body[end..];
    }
    assert!(ids.len() > 30, "couldn't read the firmware's value ids");
    ids
}

/// Every value number the firmware answers to, ranges (`ID_…_FIRST` to `ID_…_LAST`) included.
fn firmware_values() -> BTreeSet<u8> {
    let ids = firmware_ids();
    let mut values: BTreeSet<u8> = ids.values().map(|v| *v as u8).collect();
    for (name, first) in ids.iter().filter(|(n, _)| n.ends_with("_FIRST")) {
        if let Some(last) = ids.get(&name.replace("_FIRST", "_LAST")) {
            values.extend((*first..=*last).map(|v| v as u8));
        }
    }
    values
}

fn id(name: &str) -> u8 {
    *firmware_ids().get(name).unwrap_or_else(|| panic!("the firmware has no {name}")) as u8
}

fn every_section() -> impl Iterator<Item = &'static Section> {
    [LIGHTING, KEYS_PAGE, POWER].into_iter().flatten()
}

#[test]
fn keyboard_values_exist_in_the_firmware() {
    let known = firmware_values();
    for s in settings::all().filter(|s| s.channel == CHANNEL_KEYBOARD) {
        assert!(known.contains(&s.id), "setting {} uses value {}, which the firmware doesn't have", s.key, s.id);
    }
}

#[test]
fn profile_values_match_the_firmware() {
    assert_eq!(PROFILE.state_value.0, id("ID_STATE"));
    assert_eq!(PROFILE.win_lock_value, id("ID_WIN_LOCK"));
    assert_eq!(PROFILE.macro_play_value, id("ID_MACRO_PLAY"));
    assert_eq!(PROFILE.check_battery_value, id("ID_CHECK_BATTERY"));
    assert_eq!(settings::MACRO_MATCH_OS.id, id("ID_MACRO_MATCH_OS"));
    let by_key = |key: &str| settings::all().find(|s| s.key == key).unwrap_or_else(|| panic!("no setting {key}")).id;
    assert_eq!(by_key("frow_order"), id("ID_FROW_MEDIA_FIRST"));
    assert_eq!(by_key("win_lock"), id("ID_WIN_LOCK"));
    assert_eq!(by_key("side_red"), id("ID_SIDE_RED"));
    assert_eq!(by_key("caps_key"), id("ID_CAPS_KEY_MODE"));
    assert_eq!(by_key("battery_bar"), id("ID_AUTO_BATTERY"));
    assert_eq!(by_key("sleep_after"), id("ID_SLEEP_AFTER"));
}

#[test]
fn every_setting_has_its_own_value() {
    let mut seen = BTreeSet::new();
    for s in settings::all() {
        assert!(seen.insert(s.value_id()), "two settings use {:?} ({})", s.value_id(), s.key);
    }
}

#[test]
fn firmware_constants_match() {
    let config = firmware("config.h");
    let define = |name: &str| -> u32 {
        let line = config.lines().find(|l| l.starts_with(&format!("#define {name} "))).unwrap_or_else(|| panic!("no {name}"));
        let value = line.split_whitespace().nth(2).unwrap();
        match value.strip_prefix("0x") {
            Some(hex) => u32::from_str_radix(hex, 16).unwrap(),
            None => value.parse().unwrap(),
        }
    };
    assert_eq!(PROFILE.fn_layer as u32, define("RK71_FN_LAYER"));
    assert!(PROFILE.macros_since <= define("VIA_FIRMWARE_VERSION"), "the macros need a firmware newer than this one");
    assert!(PROFILE.bluetooth_reports_since <= PROFILE.macros_since);

    let header = firmware("rk71_btcfg.h");
    let slots: usize = header.lines().find_map(|l| l.strip_prefix("#define BTCFG_SLOTS ")).unwrap().trim().parse().unwrap();
    assert_eq!(PROFILE.bluetooth_slots.len(), slots);
    assert!(PROFILE.bluetooth_slots.last().unwrap().is_none(), "the last slot is the packed state");
}

/// The values the keyboard reports over Bluetooth, in the firmware's slot order (rk71_btcfg_read).
#[test]
fn bluetooth_slots_match_the_firmware() {
    let source = firmware("rk71_rgb.c");
    let start = source.find("slots[BTCFG_SLOTS - 1] = {").expect("rk71_btcfg_read's slot table");
    let table = &source[start..start + source[start..].find("};").unwrap()];
    let firmware_slots: Vec<((u8, u8), usize)> = table
        .lines()
        .filter_map(|l| l.trim().strip_prefix('{').and_then(|l| l.split('}').next()))
        .map(|entry| {
            let parts: Vec<&str> = entry.split(',').map(str::trim).collect();
            let channel = match parts[0] {
                "id_qmk_rgb_matrix_channel" => crate::device::CHANNEL_RGB_MATRIX,
                "id_custom_channel" => CHANNEL_KEYBOARD,
                other => panic!("unknown channel {other}"),
            };
            let value = parts[1].parse().unwrap_or_else(|_| id(parts[1]));
            ((channel, value), parts[2].parse().unwrap())
        })
        .collect();
    let app_slots: Vec<_> = PROFILE.bluetooth_slots.iter().flatten().copied().collect();
    assert_eq!(app_slots, firmware_slots);
}

#[test]
fn effects_match_the_firmware() {
    // Off and solid colour come first; then one per effect enabled in keyboard.json.
    let json: serde_json::Value = serde_json::from_str(&firmware("keyboard.json")).unwrap();
    let enabled = json["rgb_matrix"]["animations"].as_object().unwrap().values().filter(|v| v.as_bool() == Some(true)).count();
    assert_eq!(KEY_EFFECTS.len(), enabled + 2);
    for list in [settings::COLOR_EFFECTS, settings::MULTICOLOR_EFFECTS, settings::TYPING_EFFECTS, settings::ANIMATED_EFFECTS] {
        assert!(list.iter().all(|e| (*e as usize) < KEY_EFFECTS.len()));
    }
    // The gallery shows every effect but off exactly once.
    let mut gallery: Vec<u8> = settings::COLOR_EFFECTS.iter().chain(settings::MULTICOLOR_EFFECTS).copied().collect();
    gallery.sort();
    assert_eq!(gallery, (1..KEY_EFFECTS.len() as u8).collect::<Vec<_>>());
}

#[test]
fn keys_match_the_key_matrix() {
    let json: serde_json::Value = serde_json::from_str(&firmware("keyboard.json")).unwrap();
    let (rows, cols) = PROFILE.matrix;
    assert_eq!(json["matrix_pins"]["rows"].as_array().unwrap().len(), rows);
    assert_eq!(json["matrix_pins"]["cols"].as_array().unwrap().len(), cols);
    let layout = json["layouts"].as_object().unwrap().values().next().unwrap()["layout"].as_array().unwrap();
    let firmware_keys: BTreeSet<(usize, usize)> =
        layout.iter().map(|k| (k["matrix"][0].as_u64().unwrap() as usize, k["matrix"][1].as_u64().unwrap() as usize)).collect();
    let app_keys: BTreeSet<(usize, usize)> = PROFILE.keys.iter().map(|k| (k.row, k.col)).collect();
    assert_eq!(app_keys.len(), PROFILE.keys.len(), "two keys share a matrix position");
    assert_eq!(app_keys, firmware_keys);
    for label in PROFILE.bluetooth_device_keys {
        assert!(PROFILE.keys.iter().any(|k| k.label == *label), "no key {label}");
    }
}

/// VIA's menus use values the firmware has (VIA shows nothing for an unknown one).
#[test]
fn via_definition_uses_firmware_values() {
    let path = format!("{}/../via/rk71_rgb_via.json", env!("CARGO_MANIFEST_DIR"));
    let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let known: BTreeSet<u64> = firmware_values().into_iter().map(u64::from).collect();
    let mut found = 0;
    fn walk(v: &serde_json::Value, f: &mut dyn FnMut(&[serde_json::Value])) {
        match v {
            serde_json::Value::Object(map) => {
                if let Some(serde_json::Value::Array(content)) = map.get("content") {
                    if content.len() == 3 && content[0].is_string() {
                        f(content);
                    }
                }
                map.values().for_each(|v| walk(v, f));
            }
            serde_json::Value::Array(items) => items.iter().for_each(|v| walk(v, f)),
            _ => {}
        }
    }
    walk(&json, &mut |content| {
        if content[1].as_u64() == Some(CHANNEL_KEYBOARD as u64) {
            let value = content[2].as_u64().unwrap();
            assert!(known.contains(&value), "VIA's {} uses value {value}, which the firmware doesn't have", content[0]);
            found += 1;
        }
    });
    assert!(found > 20, "found only {found} VIA controls");
}

/// rust-i18n gives back the key itself when a text is missing.
fn assert_text(key: &str) {
    assert_ne!(crate::i18n::text(key), key, "no text for {key} in locales/en.yml");
}

#[test]
fn every_text_exists() {
    for section in every_section() {
        assert_text(&format!("section.{}", section.key));
    }
    for s in settings::all() {
        assert_text(&format!("setting.{}.label", s.key));
        if let Some(note) = s.note_when_hidden {
            assert_text(&format!("note.{note}"));
        }
        if let Kind::Choice(options) = s.kind {
            if options == KEY_EFFECTS {
                options.iter().for_each(|e| assert_text(&format!("effect.{e}")));
            } else {
                options.iter().for_each(|o| assert_text(&format!("option.{o}")));
            }
            // A custom order shows each option exactly once.
            let mut order = s.display_order();
            order.sort();
            assert_eq!(order, (0..options.len() as u8).collect::<Vec<_>>(), "{}'s order", s.key);
        }
    }
    for (group, rows) in PROFILE.fn_shortcuts {
        assert_text(&format!("shortcuts.group.{group}"));
        rows.iter().for_each(|(_, what)| assert_text(&format!("shortcuts.{what}")));
    }
}

/// Shown-when conditions point at a setting in the same section, so both are on screen together.
#[test]
fn conditions_refer_to_their_section() {
    for section in every_section() {
        for s in section.settings {
            if let Some((other, _)) = s.shown_when {
                assert!(
                    section.settings.iter().any(|o| o.channel == s.channel && o.id == other) || (s.channel, other) == settings::KEY_EFFECT.value_id(),
                    "{} depends on value {other}, not in its section",
                    s.key
                );
            }
        }
    }
}
