//! The app's own preferences (not the keyboard's), kept as JSON in the user's config folder:
//! Windows %APPDATA%\keycandle, macOS ~/Library/Application Support/keycandle,
//! Linux ~/.config/keycandle.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
#[serde(default)] // preferences saved by older versions lack the newer ones
pub struct Prefs {
    /// Locale code, or None to follow the system.
    pub language: Option<String>,
    /// "light" or "dark", or None to follow the system.
    pub appearance: Option<String>,
    /// Sliders show the keyboard's own numbers (e.g. 0-31) instead of percent.
    pub raw_values: bool,
    /// Don't ask GitHub for new versions when the app opens.
    pub no_update_check: bool,
    /// Release tags whose update notice was put off ("Later"): not announced again.
    pub dismissed_updates: Vec<String>,
    /// The firmware version last seen on the keyboard, to tell about updates while it's on Bluetooth.
    pub last_firmware: Option<u32>,
}

impl Prefs {
    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("keycandle").join("prefs.json"))
    }

    /// The saved preferences, or defaults when there are none (or they can't be read).
    pub fn load() -> Self {
        Self::path()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Save; failing to is not worth bothering the user about.
    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}
