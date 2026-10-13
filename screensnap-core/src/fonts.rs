//! Theme and fonts, installed once at startup by the native shell (and by the
//! headless tests) so every surface renders with the same setup.
//!
//! The UI face is agg-gui's bundled Noto Sans with its standard fallback
//! chain, which puts Font Awesome 4.7 behind it: an icon is just its code
//! point drawn as ordinary text. The code points the app uses are named in
//! [`fa`].

use std::sync::Arc;

use agg_gui::fonts::{standard_ui_font, FONT_AWESOME_4_7};
use agg_gui::text::Font;
use agg_gui::theme::{set_visuals, Visuals};
use agg_gui::{font_settings, set_device_scale};

/// Font Awesome 4.7 code points used by the app.
pub mod fa {
    /// fa-crop — the app mark and "Capture Region".
    pub const CROP: char = '\u{f125}';
    /// fa-keyboard-o — the hotkey row.
    pub const KEYBOARD: char = '\u{f11c}';
    /// fa-clipboard — copy to clipboard.
    pub const CLIPBOARD: char = '\u{f0ea}';
    /// fa-floppy-o — save to folder.
    pub const SAVE: char = '\u{f0c7}';
    /// fa-folder-open — choose a folder.
    pub const FOLDER_OPEN: char = '\u{f07c}';
    /// fa-mouse-pointer — include the pointer.
    pub const MOUSE_POINTER: char = '\u{f245}';
    /// fa-search-plus — the magnifier.
    pub const SEARCH_PLUS: char = '\u{f00e}';
    /// fa-exclamation-triangle — a warning (permission, hotkey taken).
    pub const WARNING: char = '\u{f071}';
    /// fa-check-circle — a capture went fine.
    pub const CHECK_CIRCLE: char = '\u{f058}';
    /// fa-cog — open settings.
    pub const COG: char = '\u{f013}';
    /// fa-power-off — quit.
    pub const POWER_OFF: char = '\u{f011}';
    /// fa-times — close.
    pub const TIMES: char = '\u{f00d}';
}

/// The faces the widgets draw with.
#[derive(Clone)]
pub struct Fonts {
    /// Noto Sans with Font Awesome as fallback; used for all text.
    pub ui: Arc<Font>,
    /// Font Awesome alone, for `Button::with_icon`.
    pub icons: Arc<Font>,
}

/// The look for a light or dark desktop.
pub fn visuals(dark: bool) -> Visuals {
    if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    }
}

/// Install the theme (following the OS's light / dark appearance) and the UI
/// font for a display at `device_scale`, returning the fonts the widget tree is
/// built with.
pub fn install_theme_and_fonts(device_scale: f64, dark: bool) -> Result<Fonts, &'static str> {
    set_device_scale(device_scale);
    set_visuals(visuals(dark));
    let ui = Arc::new(standard_ui_font());
    let icons = Arc::new(Font::from_slice(FONT_AWESOME_4_7)?);
    font_settings::set_system_font(Some(Arc::clone(&ui)));
    Ok(Fonts { ui, icons })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every label `Hotkey::display` can produce must be drawable.
    #[test]
    fn hotkey_labels_are_drawable() {
        use crate::hotkey::{DisplayStyle, Hotkey, HotkeyMods, KEY_CODES};
        let fonts = install_theme_and_fonts(1.0, false).expect("bundled fonts load");
        let mods = HotkeyMods {
            cmd: true,
            ctrl: true,
            alt: true,
            shift: true,
        };
        for (code, _) in KEY_CODES {
            let text = Hotkey::new(mods, code).unwrap().display(DisplayStyle::Mac);
            for ch in text.chars() {
                assert!(fonts.ui.has_glyph(ch), "{code}: UI font is missing {ch:?}");
            }
        }
    }

    #[test]
    fn fonts_cover_the_text_and_icons_the_app_draws() {
        let fonts = install_theme_and_fonts(1.0, false).expect("bundled fonts load");
        for ch in "Screen Snap 0123456789 \u{d7} \u{2318}\u{21e7}\u{2325}".chars() {
            assert!(fonts.ui.has_glyph(ch), "UI font is missing {ch:?}");
        }
        for icon in [
            fa::CROP,
            fa::KEYBOARD,
            fa::CLIPBOARD,
            fa::SAVE,
            fa::FOLDER_OPEN,
            fa::MOUSE_POINTER,
            fa::SEARCH_PLUS,
            fa::WARNING,
            fa::CHECK_CIRCLE,
            fa::COG,
            fa::POWER_OFF,
            fa::TIMES,
        ] {
            assert!(
                fonts.icons.has_glyph(icon),
                "Font Awesome is missing U+{:04X}",
                icon as u32
            );
            assert!(fonts.ui.has_glyph(icon), "UI fallback chain misses an icon");
        }
    }
}
