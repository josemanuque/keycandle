//! What a setting is (kind, VIA value, when it shows) and the helpers that build a catalogue. The
//! keyboard's own catalogue lives with its profile (keyboard/rk71/settings.rs).
//!
//! No text lives here: settings, sections, options and notes carry translation keys
//! (locales/*.yml): `setting.<key>.label`, `setting.<key>.help` (optional), `section.<key>`,
//! `option.<key>`, `effect.<key>`, `note.<key>`.

use crate::i18n;

#[derive(Clone, Copy)]
pub enum Kind {
    Toggle,
    Range { min: u8, max: u8 },
    /// Option keys, in the firmware's value order.
    Choice(&'static [&'static str]),
    /// Hue and saturation, as two bytes.
    Color,
    /// Windows or Mac for each device the keyboard connects to: one byte, bit n = Bluetooth device
    /// n + 1 (0..4), bit 5 = the cable.
    DeviceLayouts,
}

#[derive(Clone, Copy)]
pub struct Setting {
    pub channel: u8,
    pub id: u8,
    /// Translation key.
    pub key: &'static str,
    pub kind: Kind,
    /// Shown only while another setting (same channel) has one of these values.
    pub shown_when: Option<(u8, &'static [u8])>,
    /// Note key, shown in the setting's place while it is hidden (says why).
    pub note_when_hidden: Option<&'static str>,
    /// Choice: the order the options are shown in (firmware values), when not 0, 1, 2, …
    pub order: Option<&'static [u8]>,
    /// Drawn inside the previous setting's row, next to its switch (a toggle's color or level).
    pub inline: bool,
}

impl Setting {
    pub fn value_id(&self) -> value::Id {
        (self.channel, self.id)
    }

    pub fn len(&self) -> usize {
        if matches!(self.kind, Kind::Color) { 2 } else { 1 }
    }

    pub fn label(&self) -> String {
        i18n::text(&format!("setting.{}.label", self.key))
    }

    pub fn help(&self) -> Option<String> {
        i18n::optional(&format!("setting.{}.help", self.key))
    }

    pub fn note(&self) -> Option<String> {
        self.note_when_hidden.map(|key| i18n::text(&format!("note.{key}")))
    }

    /// A Choice's firmware values in the order they're shown.
    pub fn display_order(&self) -> Vec<u8> {
        match (self.order, self.kind) {
            (Some(order), _) => order.to_vec(),
            (None, Kind::Choice(keys)) => (0..keys.len() as u8).collect(),
            _ => Vec::new(),
        }
    }

    /// The options of a Choice setting, translated (empty for other kinds).
    pub fn options(&self) -> Vec<String> {
        match self.kind {
            Kind::Choice(keys) if keys == KEY_EFFECTS => (0..keys.len()).map(effect_name).collect(),
            Kind::Choice(keys) => keys.iter().map(|k| i18n::text(&format!("option.{k}"))).collect(),
            _ => Vec::new(),
        }
    }
}

pub struct Section {
    /// Translation key.
    pub key: &'static str,
    pub settings: &'static [Setting],
    /// Takes the page's full width, below the cards that share a row.
    pub wide: bool,
}

impl Section {
    pub fn title(&self) -> String {
        i18n::text(&format!("section.{}", self.key))
    }

    /// A short line beside the title (`section_hint.<key>`), if there is one.
    pub fn hint(&self) -> Option<String> {
        i18n::optional(&format!("section_hint.{}", self.key))
    }
}

pub(crate) const fn section(key: &'static str, settings: &'static [Setting]) -> Section {
    Section { key, settings, wide: false }
}

pub(crate) const fn s(channel: u8, id: u8, key: &'static str, kind: Kind) -> Setting {
    Setting { channel, id, key, kind, shown_when: None, note_when_hidden: None, order: None, inline: false }
}

pub(crate) const fn ordered(mut setting: Setting, order: &'static [u8]) -> Setting {
    setting.order = Some(order);
    setting
}

pub(crate) const fn inline(mut setting: Setting) -> Setting {
    setting.inline = true;
    setting
}

pub(crate) const fn when(mut setting: Setting, id: u8, values: &'static [u8]) -> Setting {
    setting.shown_when = Some((id, values));
    setting
}

pub(crate) const fn else_note(mut setting: Setting, note: &'static str) -> Setting {
    setting.note_when_hidden = Some(note);
    setting
}

// The catalogue itself (pages, settings, effects, value ids) is the keyboard's: keyboard/rk71/settings.rs.
pub use crate::keyboard::rk71::settings::*;

/// Every setting on every page.
pub fn all() -> impl Iterator<Item = &'static Setting> {
    [LIGHTING, KEYS, POWER].into_iter().flatten().flat_map(|s| s.settings.iter()).chain([&KEY_EFFECT, &BATTERY_BAR, &MACRO_MATCH_OS])
}

/// The setting for a value id.
pub fn find(id: value::Id) -> Option<&'static Setting> {
    all().find(|s| (s.channel, s.id) == id)
}
