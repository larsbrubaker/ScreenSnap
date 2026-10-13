//! Where the desktop app keeps what it remembers: `settings.json` (the core's
//! settings blob, `screensnap_core::settings`) in `<OS config dir>/Screen Snap/`
//! (`~/Library/Application Support/Screen Snap` on macOS).
//!
//! `SCREENSNAP_SETTINGS_DIR` points it somewhere else; a screenshot run
//! (`SCREENSNAP_SCREENSHOT`, see `main.rs`) without it uses no file at all, so
//! screenshots start from the defaults and never change the user's settings.
//! The file is written whole through a temporary file and a rename, so a crash
//! mid-write leaves the old file. Read and write failures are ignored: the app
//! then starts from the defaults or forgets the change, and never interrupts a
//! capture for it.

use std::path::{Path, PathBuf};

use screensnap_core::settings::SettingsStore;
use screensnap_core::APP_NAME;

/// Overrides the settings folder.
pub const SETTINGS_DIR_ENV: &str = "SCREENSNAP_SETTINGS_DIR";

/// The folder the settings live in, if any (see the module docs).
pub fn settings_dir(screenshot_run: bool) -> Option<PathBuf> {
    resolve_settings_dir(
        std::env::var_os(SETTINGS_DIR_ENV).map(PathBuf::from),
        screenshot_run,
        dirs::config_dir(),
    )
}

/// [`settings_dir`] with its inputs explicit.
fn resolve_settings_dir(
    override_dir: Option<PathBuf>,
    screenshot_run: bool,
    config_dir: Option<PathBuf>,
) -> Option<PathBuf> {
    if let Some(dir) = override_dir.filter(|d| !d.as_os_str().is_empty()) {
        return Some(dir);
    }
    if screenshot_run {
        return None;
    }
    config_dir.map(|d| d.join(APP_NAME))
}

/// Write `text` to `path` through a temporary file and a rename.
fn write_atomically(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// `settings.json` in a folder.
pub struct FileSettings {
    path: PathBuf,
}

impl FileSettings {
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("settings.json"),
        }
    }
}

impl SettingsStore for FileSettings {
    fn load(&self) -> Option<String> {
        std::fs::read_to_string(&self.path).ok()
    }

    fn save(&self, blob: &str) {
        // Losing a setting is harmless; never interrupt the event loop for it.
        let _ = write_atomically(&self.path, blob);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_through_the_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let nested = dir.path().join("not-yet-created");
        let store = FileSettings::new(&nested);
        assert_eq!(store.load(), None);
        store.save("{\"copy_to_clipboard\": false}");
        assert_eq!(
            store.load().as_deref(),
            Some("{\"copy_to_clipboard\": false}")
        );
        assert!(!nested.join("settings.json.tmp").exists());
    }

    #[test]
    fn an_unwritable_folder_is_not_an_error() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("a-file");
        std::fs::write(&file, "x").expect("write");
        // The "folder" is a file: every write fails, quietly.
        FileSettings::new(&file).save("{}");
        assert_eq!(FileSettings::new(&file).load(), None);
    }

    #[test]
    fn folder_resolution() {
        let config = Some(PathBuf::from("/cfg"));
        assert_eq!(
            resolve_settings_dir(None, false, config.clone()),
            Some(PathBuf::from("/cfg/Screen Snap"))
        );
        assert_eq!(resolve_settings_dir(None, true, config.clone()), None);
        assert_eq!(
            resolve_settings_dir(Some("/over".into()), true, config.clone()),
            Some(PathBuf::from("/over"))
        );
        // An empty override is no override.
        assert_eq!(
            resolve_settings_dir(Some(PathBuf::new()), false, config),
            Some(PathBuf::from("/cfg/Screen Snap"))
        );
        assert_eq!(resolve_settings_dir(None, false, None), None);
    }
}
