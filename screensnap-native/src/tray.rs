//! The menu-bar (tray) icon and its menu — Screen Snap's one piece of native
//! UI, because it lives outside the window (see CLAUDE.md):
//!
//! `Capture Region  <hotkey>` · `Settings…` · `Open Captures Folder` ·
//! `Quit Screen Snap`
//!
//! The icon is the core's mark ([`screensnap_core::icon`]): the monochrome
//! template on macOS (the system tints it for a light or dark menu bar), the
//! full-colour icon elsewhere. The Capture item shows the current hotkey as
//! its key equivalent and is updated when the hotkey changes
//! ([`Tray::set_hotkey`]). Picks are queued as [`NativeEvent`]s
//! ([`crate::events`]) and handled on the next idle pass.
//!
//! Must be created on the main thread (macOS status items are AppKit
//! objects); the builder closure in `main.rs` does that.

use screensnap_core::hotkey::Hotkey;
use screensnap_core::APP_NAME;
use tray_icon::menu::accelerator::Accelerator;
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

use crate::events::{self, NativeEvent};

const CAPTURE_ID: &str = "capture";
const SETTINGS_ID: &str = "settings";
const FOLDER_ID: &str = "open-folder";
const QUIT_ID: &str = "quit";

/// Pixel size of the icon handed to the OS (44 = 22 pt at 2×, the macOS menu
/// bar's height on Retina; Windows and Linux scale it down).
const ICON_PX: u32 = 44;

/// The live menu-bar item. Dropping it removes the icon.
pub struct Tray {
    _icon: TrayIcon,
    capture: MenuItem,
}

impl Tray {
    /// Put the icon in the menu bar, its Capture item showing `hotkey`.
    pub fn new(hotkey: Option<&Hotkey>) -> Result<Self, String> {
        let capture = MenuItem::with_id(
            CAPTURE_ID,
            "Capture Region",
            true,
            hotkey.and_then(accelerator),
        );
        let settings = MenuItem::with_id(SETTINGS_ID, "Settings\u{2026}", true, None);
        let folder = MenuItem::with_id(FOLDER_ID, "Open Captures Folder", true, None);
        let quit = MenuItem::with_id(QUIT_ID, format!("Quit {APP_NAME}"), true, None);
        let menu = Menu::new();
        menu.append_items(&[
            &capture,
            &settings,
            &PredefinedMenuItem::separator(),
            &folder,
            &PredefinedMenuItem::separator(),
            &quit,
        ])
        .map_err(|e| format!("Could not build the menu: {e}"))?;

        MenuEvent::set_event_handler(Some(|event: MenuEvent| {
            if let Some(native) = event_for(event.id.as_ref()) {
                events::push(native);
            }
        }));

        let builder = TrayIconBuilder::new();
        // macOS draws the template's alpha in the menu bar's own colour.
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_templated(menu_bar_icon()?);
        #[cfg(not(target_os = "macos"))]
        let builder = builder.with_icon(menu_bar_icon()?);
        let icon = builder
            .with_tooltip(APP_NAME)
            .with_menu(Box::new(menu))
            .build()
            .map_err(|e| format!("Could not create the menu-bar icon: {e}"))?;
        Ok(Self {
            _icon: icon,
            capture,
        })
    }

    /// Show `hotkey` (or nothing, when none is registered) next to "Capture
    /// Region".
    pub fn set_hotkey(&self, hotkey: Option<&Hotkey>) {
        // An accelerator the menu cannot show is only cosmetic: drop it.
        if self
            .capture
            .set_accelerator(hotkey.and_then(accelerator))
            .is_err()
        {
            let _ = self.capture.set_accelerator(None);
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        MenuEvent::set_event_handler(None::<fn(MenuEvent)>);
    }
}

/// The menu-bar icon for this platform (see the module docs).
fn menu_bar_icon() -> Result<Icon, String> {
    let rgba = if cfg!(target_os = "macos") {
        screensnap_core::icon::template_rgba(ICON_PX)
    } else {
        screensnap_core::icon::rgba(ICON_PX)
    };
    Icon::from_rgba(rgba, ICON_PX, ICON_PX).map_err(|e| format!("Bad menu-bar icon: {e}"))
}

/// The menu's key-equivalent form of a hotkey.
fn accelerator(hotkey: &Hotkey) -> Option<Accelerator> {
    hotkey.to_accelerator().parse().ok()
}

/// The event a menu item id stands for.
fn event_for(id: &str) -> Option<NativeEvent> {
    match id {
        CAPTURE_ID => Some(NativeEvent::CaptureFromMenu),
        SETTINGS_ID => Some(NativeEvent::ShowSettings),
        FOLDER_ID => Some(NativeEvent::OpenCapturesFolder),
        QUIT_ID => Some(NativeEvent::Quit),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screensnap_core::hotkey::{HotkeyMods, KEY_CODES};

    #[test]
    fn every_hotkey_can_be_shown_in_the_menu() {
        let mods = HotkeyMods {
            cmd: true,
            ctrl: true,
            alt: true,
            shift: true,
        };
        for (code, _) in KEY_CODES {
            let hotkey = Hotkey::new(mods, code).expect("known code");
            assert!(accelerator(&hotkey).is_some(), "{code}");
        }
    }

    #[test]
    fn menu_ids_map_to_events() {
        assert_eq!(event_for(CAPTURE_ID), Some(NativeEvent::CaptureFromMenu));
        assert_eq!(event_for(SETTINGS_ID), Some(NativeEvent::ShowSettings));
        assert_eq!(event_for(FOLDER_ID), Some(NativeEvent::OpenCapturesFolder));
        assert_eq!(event_for(QUIT_ID), Some(NativeEvent::Quit));
        assert_eq!(event_for("other"), None);
    }

    #[test]
    fn the_icon_is_accepted() {
        assert!(menu_bar_icon().is_ok());
    }
}
