//! # screensnap-core
//!
//! The platform-free core of Screen Snap, a Greenshot-style region screenshot
//! tool: press the hotkey, the screen freezes (open menus, tooltips and the
//! pointer included), drag a marquee, and the region goes to the clipboard
//! and / or a PNG file. See `docs/PLAN.md`.
//!
//! `screensnap-native` owns the OS: screen capture, the global hotkey, the
//! menu-bar icon and the window's shape. It builds the app with [`build_app`]
//! over its [`SnapPlatform`] and drives it through [`AppState`].
//!
//! Modules:
//! - [`capture`] — a frozen screen and pointer, and drawing the pointer in.
//! - [`fonts`] — theme + font installation and the Font Awesome code points.
//! - [`hotkey`] — the user-defined shortcut: parsing, display, key mapping.
//! - [`icon`] — the menu-bar / app icon, drawn in code.
//! - [`output`] — crop, PNG encoding, file names, clipboard / file delivery.
//! - [`platform`] — the `SnapPlatform` contract.
//! - [`selection`] — marquee geometry in capture pixels.
//! - [`settings`] — the remembered preferences.
//! - [`state`] — the shared app state: mode, capture, settings.
//! - [`ui`] — the widgets: the capture overlay and the settings window.

pub mod capture;
pub mod fonts;
pub mod hotkey;
pub mod icon;
pub mod output;
pub mod platform;
pub mod selection;
pub mod settings;
pub mod state;
pub mod ui;

use agg_gui::App;

pub use fonts::{install_theme_and_fonts, Fonts};
pub use platform::SnapPlatform;
pub use state::{AppState, Mode};

/// The app's name, as people see it.
pub const APP_NAME: &str = "Screen Snap";

/// Build the whole app over `state`. Call [`install_theme_and_fonts`] first.
pub fn build_app(state: AppState, fonts: Fonts) -> App {
    App::new(ui::build_root(state, fonts))
}
