//! The app's own preferences (not the keyboard's), kept as JSON in the user's config folder:
//! Windows %APPDATA%\rk71-configurator, macOS ~/Library/Application Support/rk71-configurator,
//! Linux ~/.config/rk71-configurator.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Default, Serialize, Deserialize)]
pub struct Prefs {
    /// Locale code, or None to follow the system.
    pub language: Option<String>,
    /// "light" or "dark", or None to follow the system.
    pub appearance: Option<String>,
}

impl Prefs {
    fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("rk71-configurator").join("prefs.json"))
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
