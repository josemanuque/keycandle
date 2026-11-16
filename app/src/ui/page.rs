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
}

impl Page {
    pub const ALL: [Page; 5] = [Page::Overview, Page::Lighting, Page::Keys, Page::Power, Page::Firmware];

    pub fn title(self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::Lighting => "Lighting",
            Page::Keys => "Keys & indicators",
            Page::Power => "Power",
            Page::Firmware => "Firmware",
        }
    }

    pub fn subtitle(self) -> &'static str {
        match self {
            Page::Overview => "Everything on the keyboard, in one place. Changes apply live and are saved on the keyboard.",
            Page::Lighting => "Pick an effect to see it here and on the keyboard. Changes are saved once you stop adjusting.",
            Page::Keys => "How locks show and what Fn highlights. Windows/Mac is remembered per device (Fn + A / Fn + S).",
            Page::Power => "Battery saving applies on Bluetooth without the cable. Plugged in, the keyboard never sleeps.",
            Page::Firmware => "Update the keyboard's firmware or go back to the original.",
        }
    }

    /// The page for a command-line name (`--page lighting`).
    pub fn from_name(name: &str) -> Option<Page> {
        Page::ALL.into_iter().find(|p| p.title().to_lowercase().starts_with(&name.to_lowercase()))
    }

    /// Lucide icon file (all of Lucide is embedded via gpui-kit-assets' AllAssets).
    pub fn icon(self) -> Icon {
        let file = match self {
            Page::Overview => "icons/layout-dashboard.svg",
            Page::Lighting => "icons/sun.svg",
            Page::Keys => "icons/keyboard.svg",
            Page::Power => "icons/battery-full.svg",
            Page::Firmware => "icons/download.svg",
        };
        Icon::empty().path(file)
    }

    /// The settings cards on this page.
    pub fn sections(self) -> &'static [Section] {
        match self {
            Page::Lighting => LIGHTING,
            Page::Keys => KEYS,
            Page::Power => POWER,
            Page::Overview | Page::Firmware => &[],
        }
    }

    /// Pages with a live lighting preview, redrawn while shown.
    pub fn is_animated(self) -> bool {
        matches!(self, Page::Overview | Page::Lighting)
    }
}
