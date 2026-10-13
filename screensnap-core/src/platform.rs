//! What the core needs from the operating system, as a trait.
//!
//! `screensnap-native` implements [`SnapPlatform`] with the real OS (arboard
//! clipboard, rfd folder picker, Finder / Explorer reveal, the macOS Screen
//! Recording permission). Tests use a recording fake. Screen capture itself
//! and the global hotkey are not here: they are driven by the native shell,
//! which hands the core a finished [`crate::capture::Capture`].

use std::path::{Path, PathBuf};

use crate::capture::RgbaImage;
use crate::hotkey::DisplayStyle;

/// Whether the OS lets this process read other apps' windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Granted,
    /// Captures will show only the desktop and Screen Snap's own windows.
    Denied,
    /// The platform has no such permission, or cannot say.
    NotApplicable,
}

/// OS services the core calls. See the module docs.
pub trait SnapPlatform {
    /// Put `image` on the system clipboard.
    fn copy_image(&self, image: &RgbaImage) -> Result<(), String>;
    /// Where captures go when the user has not chosen a folder.
    fn default_save_folder(&self) -> PathBuf;
    /// Ask the user for a folder (a native dialog), starting at `start`.
    fn pick_folder(&self, start: &Path) -> Option<PathBuf>;
    /// Show `path` in Finder / Explorer / the file manager.
    fn reveal(&self, path: &Path) -> Result<(), String>;
    /// The screen-recording permission state, read fresh each call.
    fn screen_capture_permission(&self) -> Permission;
    /// Open the OS page where the permission is granted.
    fn open_permission_settings(&self);
    /// How shortcuts are spelled on this OS.
    fn hotkey_style(&self) -> DisplayStyle {
        DisplayStyle::native()
    }
}
