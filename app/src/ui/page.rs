//! The app's sections, as listed in the sidebar.

use gpui_component::Icon;

use crate::settings::{Section, KEYS, LIGHTING, POWER};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Lighting,
    Keys,
    Power,
    Firmware,
    /// The app's own settings (language, appearance).
    Settings,
}

impl Page {
    pub const ALL: [Page; 6] = [Page::Overview, Page::Lighting, Page::Keys, Page::Power, Page::Firmware, Page::Settings];

    /// Stable name: the translation key part and the `--page` argument.
    pub fn name(self) -> &'static str {
        match self {
            Page::Overview => "overview",
            Page::Lighting => "lighting",
            Page::Keys => "keys",
            Page::Power => "power",
            Page::Firmware => "firmware",
            Page::Settings => "settings",
        }
    }

    pub fn title(self) -> String {
        crate::i18n::text(&format!("page.{}.title", self.name()))
    }

    pub fn subtitle(self) -> String {
        crate::i18n::text(&format!("page.{}.subtitle", self.name()))
    }

    /// The page for a command-line name (`--page lighting`).
    pub fn from_name(name: &str) -> Option<Page> {
        Page::ALL.into_iter().find(|p| p.name().starts_with(&name.to_lowercase()))
    }

    /// Lucide icon file (all of Lucide is embedded via gpui-kit-assets' AllAssets).
    pub fn icon(self) -> Icon {
        let file = match self {
            Page::Overview => "icons/layout-dashboard.svg",
            Page::Lighting => "icons/sun.svg",
            Page::Keys => "icons/keyboard.svg",
            Page::Power => "icons/battery-full.svg",
            Page::Firmware => "icons/download.svg",
            Page::Settings => "icons/settings.svg",
        };
        Icon::empty().path(file)
    }

    /// The settings cards on this page.
    pub fn sections(self) -> &'static [Section] {
        match self {
            Page::Lighting => LIGHTING,
            Page::Keys => KEYS,
            Page::Power => POWER,
            Page::Overview | Page::Firmware | Page::Settings => &[],
        }
    }

    /// Pages with a live lighting preview, redrawn while shown.
    pub fn is_animated(self) -> bool {
        matches!(self, Page::Overview | Page::Lighting)
    }
}
