//! `NativePlatform`: screensnap-core's `SnapPlatform` on the real operating
//! system, plus the few OS chores the shell itself runs (asking for the
//! Screen Recording permission at start-up, opening the captures folder).
//!
//! | Service | macOS | Windows | Linux |
//! |---|---|---|---|
//! | Clipboard image | arboard | arboard | arboard |
//! | Default folder | `~/Pictures/Screen Snap` | `Pictures\Screen Snap` | `~/Pictures/Screen Snap` |
//! | Pick folder | rfd | rfd | rfd (xdg desktop portal) |
//! | Reveal | `open -R` | `explorer /select,` | opener on the containing folder |
//! | Permission | `CGPreflightScreenCaptureAccess` | not applicable | not applicable |
//!
//! Without the macOS Screen Recording permission captures show only the
//! wallpaper and Screen Snap's own windows, which is why the settings window
//! warns about it and [`request_capture_permission_once`] asks at launch.

use std::borrow::Cow;
use std::path::{Path, PathBuf};

use screensnap_core::capture::RgbaImage;
use screensnap_core::platform::{Permission, SnapPlatform};
use screensnap_core::settings::Settings;
use screensnap_core::APP_NAME;

/// The real machine.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativePlatform;

impl SnapPlatform for NativePlatform {
    fn copy_image(&self, image: &RgbaImage) -> Result<(), String> {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        clipboard
            .set_image(arboard::ImageData {
                width: image.width as usize,
                height: image.height as usize,
                bytes: Cow::Borrowed(&image.pixels),
            })
            .map_err(|e| e.to_string())
    }

    fn default_save_folder(&self) -> PathBuf {
        default_save_folder(dirs::picture_dir(), dirs::home_dir())
    }

    fn pick_folder(&self, start: &Path) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Choose where to save captures")
            .set_directory(nearest_existing(start))
            .pick_folder()
    }

    fn reveal(&self, path: &Path) -> Result<(), String> {
        reveal_in_file_manager(&nearest_existing(path))
    }

    fn screen_capture_permission(&self) -> Permission {
        screen_capture_permission()
    }

    fn open_permission_settings(&self) {
        open_permission_settings();
    }
}

/// `<Pictures>/Screen Snap`, or `<home>/Screen Snap` on a system without a
/// Pictures folder, or a relative `Screen Snap` as the last resort.
fn default_save_folder(pictures: Option<PathBuf>, home: Option<PathBuf>) -> PathBuf {
    pictures
        .or(home)
        .map(|dir| dir.join(APP_NAME))
        .unwrap_or_else(|| PathBuf::from(APP_NAME))
}

/// `path` if it exists, else its closest existing ancestor (so "Show" on a
/// save folder that has not been created yet opens its parent instead of
/// failing), else `path` unchanged.
fn nearest_existing(path: &Path) -> PathBuf {
    path.ancestors()
        .find(|p| !p.as_os_str().is_empty() && p.exists())
        .unwrap_or(path)
        .to_path_buf()
}

/// The folder captures go to under `settings`.
pub fn captures_folder(settings: &Settings, platform: &dyn SnapPlatform) -> PathBuf {
    settings
        .save_folder
        .clone()
        .unwrap_or_else(|| platform.default_save_folder())
}

/// Menu bar "Open Captures Folder": create the folder if needed and open it in
/// the file manager.
pub fn open_folder(folder: &Path) -> Result<(), String> {
    std::fs::create_dir_all(folder)
        .map_err(|e| format!("Could not create {}: {e}", folder.display()))?;
    opener::open(folder).map_err(|e| format!("Could not open {}: {e}", folder.display()))
}

#[cfg(target_os = "macos")]
fn reveal_in_file_manager(path: &Path) -> Result<(), String> {
    let status = std::process::Command::new("open")
        .arg("-R")
        .arg(path)
        .status()
        .map_err(|e| format!("Could not run open: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Finder could not show {}", path.display()))
    }
}

#[cfg(windows)]
fn reveal_in_file_manager(path: &Path) -> Result<(), String> {
    // Explorer's exit code is meaningless (1 on success), so only a failure to
    // start it is an error.
    let mut arg = std::ffi::OsString::from("/select,");
    arg.push(path);
    std::process::Command::new("explorer")
        .arg(arg)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not run Explorer: {e}"))
}

#[cfg(not(any(target_os = "macos", windows)))]
fn reveal_in_file_manager(path: &Path) -> Result<(), String> {
    // No portable "select this item": open the containing folder.
    let folder = if path.is_dir() {
        path
    } else {
        path.parent().unwrap_or(path)
    };
    opener::open(folder).map_err(|e| format!("Could not open {}: {e}", folder.display()))
}

#[cfg(target_os = "macos")]
fn screen_capture_permission() -> Permission {
    if objc2_core_graphics::CGPreflightScreenCaptureAccess() {
        Permission::Granted
    } else {
        Permission::Denied
    }
}

#[cfg(not(target_os = "macos"))]
fn screen_capture_permission() -> Permission {
    Permission::NotApplicable
}

/// The System Settings pane for Screen Recording.
#[cfg(target_os = "macos")]
const PERMISSION_PANE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

#[cfg(target_os = "macos")]
fn open_permission_settings() {
    // Fire and forget: the settings window re-reads the permission when it
    // paints, so there is nothing to report back.
    let _ = std::process::Command::new("open")
        .arg(PERMISSION_PANE)
        .spawn();
}

/// Other platforms report [`Permission::NotApplicable`], so the settings
/// window never offers this; there is no page to open.
#[cfg(not(target_os = "macos"))]
fn open_permission_settings() {}

/// macOS: if Screen Recording is not granted, ask once (the system shows its
/// prompt the first time and lists Screen Snap in System Settings). Other
/// platforms have no such permission.
pub fn request_capture_permission_once() {
    #[cfg(target_os = "macos")]
    if !objc2_core_graphics::CGPreflightScreenCaptureAccess() {
        // The answer only takes effect after a relaunch; the settings window
        // shows the warning until then.
        let _ = objc2_core_graphics::CGRequestScreenCaptureAccess();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_folder_prefers_pictures_then_home() {
        assert_eq!(
            default_save_folder(Some("/p".into()), Some("/h".into())),
            PathBuf::from("/p/Screen Snap")
        );
        assert_eq!(
            default_save_folder(None, Some("/h".into())),
            PathBuf::from("/h/Screen Snap")
        );
        assert_eq!(
            default_save_folder(None, None),
            PathBuf::from("Screen Snap")
        );
    }

    #[test]
    fn nearest_existing_walks_up_to_a_real_folder() {
        let dir = tempfile::tempdir().expect("temp dir");
        let missing = dir.path().join("a").join("b");
        assert_eq!(nearest_existing(&missing), dir.path());
        assert_eq!(nearest_existing(dir.path()), dir.path());
    }

    #[test]
    fn captures_folder_follows_the_setting() {
        let mut settings = Settings::default();
        let platform = NativePlatform;
        assert_eq!(
            captures_folder(&settings, &platform),
            platform.default_save_folder()
        );
        settings.save_folder = Some("/tmp/caps".into());
        assert_eq!(
            captures_folder(&settings, &platform),
            PathBuf::from("/tmp/caps")
        );
    }

    #[test]
    fn open_folder_creates_it() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("file");
        std::fs::write(&file, "x").expect("write");
        // A folder that cannot be created is reported, not opened.
        let err = open_folder(&file.join("sub")).expect_err("cannot create under a file");
        assert!(err.starts_with("Could not create"), "{err}");
    }
}
