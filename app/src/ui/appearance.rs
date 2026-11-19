//! Light or dark: follow the system, or the user's choice (kept in the app's preferences).

use gpui::{transparent_black, App, Window};
use gpui_component::{Theme, ThemeMode};

use super::chrome::{self, Backdrop};
use crate::prefs::Prefs;
use crate::tr;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];

    fn code(self) -> Option<&'static str> {
        match self {
            Appearance::System => None,
            Appearance::Light => Some("light"),
            Appearance::Dark => Some("dark"),
        }
    }

    pub fn label(self) -> String {
        match self {
            Appearance::System => tr!("settings.appearance_system"),
            Appearance::Light => tr!("settings.appearance_light"),
            Appearance::Dark => tr!("settings.appearance_dark"),
        }
    }

    /// What the user picked (System if nothing).
    pub fn saved() -> Self {
        match Prefs::load().appearance.as_deref() {
            Some("light") => Appearance::Light,
            Some("dark") => Appearance::Dark,
            _ => Appearance::System,
        }
    }

    /// Remember this choice and apply it.
    pub fn choose(self, window: &mut Window, cx: &mut App) {
        let mut prefs = Prefs::load();
        prefs.appearance = self.code().map(str::to_string);
        prefs.save();
        apply(window, cx);
    }
}

/// Apply the saved choice; called at start-up and whenever the system's appearance changes.
pub fn apply(window: &mut Window, cx: &mut App) {
    let saved = Appearance::saved();
    match saved {
        Appearance::System => Theme::sync_system_appearance(Some(window), cx),
        Appearance::Light => Theme::change(ThemeMode::Light, Some(window), cx),
        Appearance::Dark => Theme::change(ThemeMode::Dark, Some(window), cx),
    }
    chrome::set_dark(window, saved.code().map(|code| code == "dark"));
    let backdrop = Backdrop::detect();
    if backdrop.is_translucent() {
        // The window's backdrop shows where nothing is drawn: under the sidebar. The page paints
        // its own (opaque) background.
        let theme = Theme::global_mut(cx);
        theme.tokens.background = transparent_black().into();
        theme.tokens.sidebar = transparent_black().into();
        if backdrop == Backdrop::FloatingGlass {
            theme.colors.sidebar_border = transparent_black();
        }
    }
    super::menu::set_menus(cx);
}
