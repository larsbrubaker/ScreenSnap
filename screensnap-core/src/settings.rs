//! What Screen Snap remembers between runs, as a JSON blob.
//!
//! The core never touches the file system for this: a [`SettingsStore`]
//! (a file in the OS config directory in `screensnap-native`, an in-memory
//! one in tests) loads and saves the text. Unknown or missing fields fall back
//! to their defaults, so an older or hand-edited file still loads.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::hotkey::Hotkey;

/// The user's preferences. See the module docs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The global shortcut that freezes the screen.
    pub hotkey: Hotkey,
    /// Put each capture on the clipboard.
    pub copy_to_clipboard: bool,
    /// Write each capture to [`Settings::save_folder`] as a PNG.
    pub save_to_file: bool,
    /// Where captures are saved; `None` uses the platform default
    /// (`~/Pictures/Screen Snap`).
    pub save_folder: Option<PathBuf>,
    /// Draw the mouse pointer into the frozen screen (so it can be captured).
    pub include_cursor: bool,
    /// Show the pixel magnifier next to the pointer while selecting.
    pub show_magnifier: bool,
    /// The settings window has been shown once (first-run welcome).
    pub welcomed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: Hotkey::default_for_platform(),
            copy_to_clipboard: true,
            save_to_file: true,
            save_folder: None,
            include_cursor: true,
            show_magnifier: true,
            welcomed: false,
        }
    }
}

impl Settings {
    /// Read a blob, falling back to the defaults when it is missing or broken.
    pub fn from_json(text: Option<&str>) -> Self {
        text.and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Where the settings blob lives.
pub trait SettingsStore {
    fn load(&self) -> Option<String>;
    /// Persist `blob`. Failures are the store's to swallow: losing a setting
    /// must never interrupt a capture.
    fn save(&self, blob: &str);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_missing_fields() {
        let mut s = Settings::default();
        s.save_folder = Some(PathBuf::from("/tmp/caps"));
        s.include_cursor = false;
        assert_eq!(Settings::from_json(Some(&s.to_json())), s);

        let partial = Settings::from_json(Some(r#"{"copy_to_clipboard": false}"#));
        assert!(!partial.copy_to_clipboard);
        assert!(partial.save_to_file);
        assert_eq!(partial.hotkey, Hotkey::default_for_platform());

        assert_eq!(Settings::from_json(Some("not json")), Settings::default());
        assert_eq!(Settings::from_json(None), Settings::default());
    }
}
