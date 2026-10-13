//! The global capture shortcut, through the `global-hotkey` crate (Carbon hot
//! keys on macOS — no Accessibility permission needed — `RegisterHotKey` on
//! Windows, `XGrabKey` on X11).
//!
//! The core's [`Hotkey`] renders itself as the accelerator text this crate
//! parses (`"super+shift+Digit2"`), so registration is parse + register.
//! [`HotkeyRegistrar::sync`] runs on every idle pass and only touches the OS
//! when the wanted shortcut changed: a new one in the settings, or `None`
//! while the settings window's recorder is listening (so the current
//! combination can be typed into it instead of firing a capture).
//!
//! Presses arrive through a callback that only queues
//! [`NativeEvent::HotkeyPressed`] ([`crate::events`]); releases are ignored.

use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use screensnap_core::hotkey::{DisplayStyle, Hotkey};

use crate::events::{self, NativeEvent};

/// No hotkey registered (global-hotkey ids are hashes; 0 stands in for none).
const NO_ID: u32 = 0;

/// Owns the OS registration of the capture shortcut.
pub struct HotkeyRegistrar {
    /// `None` when the OS refused to set up hot keys at all.
    manager: Option<GlobalHotKeyManager>,
    /// Why `manager` is `None`.
    manager_error: Option<String>,
    /// What [`HotkeyRegistrar::sync`] was last asked for.
    wanted: Option<Option<Hotkey>>,
    /// What is registered with the OS right now.
    registered: Option<HotKey>,
    /// The registered hotkey's id, read by the event callback.
    active_id: Arc<AtomicU32>,
    error: Option<String>,
}

impl HotkeyRegistrar {
    /// Set up the OS side. Must run on the main thread (macOS installs a
    /// Carbon event handler on the main event loop).
    pub fn new() -> Self {
        let active_id = Arc::new(AtomicU32::new(NO_ID));
        let (manager, manager_error) = match GlobalHotKeyManager::new() {
            Ok(manager) => (Some(manager), None),
            Err(e) => (None, Some(format!("Global shortcuts are unavailable: {e}"))),
        };
        let id = Arc::clone(&active_id);
        GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
            if event.state == HotKeyState::Pressed && event.id == id.load(Ordering::Relaxed) {
                events::push(NativeEvent::HotkeyPressed);
            }
        }));
        Self {
            manager,
            manager_error,
            wanted: None,
            registered: None,
            active_id,
            error: None,
        }
    }

    /// Make `wanted` the registered shortcut (`None` = none), returning why
    /// the OS refused it, if it did. Cheap when nothing changed.
    pub fn sync(&mut self, wanted: Option<&Hotkey>) -> Option<String> {
        if self.wanted.as_ref().map(Option::as_ref) == Some(wanted) {
            return self.error.clone();
        }
        self.wanted = Some(wanted.cloned());
        self.unregister();
        self.error = match wanted {
            Some(hotkey) => self.register(hotkey).err(),
            None => None,
        };
        self.error.clone()
    }

    fn register(&mut self, hotkey: &Hotkey) -> Result<(), String> {
        let label = hotkey.display(DisplayStyle::native());
        let manager = self.manager.as_ref().ok_or_else(|| {
            self.manager_error
                .clone()
                .unwrap_or_else(|| "Global shortcuts are unavailable".to_string())
        })?;
        let os_hotkey = to_os_hotkey(hotkey)?;
        manager.register(os_hotkey).map_err(|e| {
            format!("{label} could not be registered ({e}). Choose another shortcut.")
        })?;
        self.active_id.store(os_hotkey.id(), Ordering::Relaxed);
        self.registered = Some(os_hotkey);
        Ok(())
    }

    /// Release the registered shortcut, if any.
    pub fn unregister(&mut self) {
        self.active_id.store(NO_ID, Ordering::Relaxed);
        if let (Some(manager), Some(hotkey)) = (&self.manager, self.registered.take()) {
            // Failing to unregister leaves nothing to recover: the id no
            // longer matches, so a stray press is ignored.
            let _ = manager.unregister(hotkey);
        }
    }
}

impl Drop for HotkeyRegistrar {
    fn drop(&mut self) {
        self.unregister();
        GlobalHotKeyEvent::set_event_handler(None::<fn(GlobalHotKeyEvent)>);
    }
}

/// The `global-hotkey` form of a core hotkey.
fn to_os_hotkey(hotkey: &Hotkey) -> Result<HotKey, String> {
    HotKey::from_str(&hotkey.to_accelerator())
        .map_err(|e| format!("Unsupported shortcut {}: {e}", hotkey.to_accelerator()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use screensnap_core::hotkey::{HotkeyMods, KEY_CODES};

    /// Every hotkey the recorder can produce must be one the OS layer accepts.
    #[test]
    fn every_core_hotkey_parses_as_an_os_hotkey() {
        let all = HotkeyMods {
            cmd: true,
            ctrl: true,
            alt: true,
            shift: true,
        };
        for (code, _) in KEY_CODES {
            for mods in [all, HotkeyMods::default()] {
                let hotkey = Hotkey::new(mods, code).expect("known code");
                to_os_hotkey(&hotkey).unwrap_or_else(|e| panic!("{code}: {e}"));
            }
        }
    }

    #[test]
    fn modifiers_survive_the_round_trip() {
        let hotkey = Hotkey::default_for_platform();
        let os = to_os_hotkey(&hotkey).expect("default parses");
        let again = HotKey::from_str(&os.into_string()).expect("canonical form parses");
        assert_eq!(os.id(), again.id());
        assert_ne!(os.id(), NO_ID);
    }
}
