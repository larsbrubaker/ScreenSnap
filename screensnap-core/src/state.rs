//! `AppState`: the one piece of shared state, a cheap-to-clone handle
//! (`Rc<RefCell<…>>`, UI thread only) that the widgets and the native shell
//! both hold.
//!
//! The app has three [`Mode`]s and one window:
//!
//! - [`Mode::Hidden`] — idle in the menu bar, waiting for the hotkey;
//! - [`Mode::Settings`] — the settings window is up;
//! - [`Mode::Overlay`] — a frozen screen covers the monitor and the user is
//!   drawing a marquee.
//!
//! Widgets change the mode through the methods here; the native shell watches
//! [`AppState::mode_version`] and reshapes the OS window to match (hidden,
//! a normal titled window, or a borderless shield over the monitor). Nothing
//! in the core knows how windows work.
//!
//! The capture flow: the shell grabs the screen and calls
//! [`AppState::begin_capture`]; the overlay shows it and, on release of a
//! marquee, calls [`AppState::finish_capture`], which crops the region, queues
//! a [`Delivery`] and hides; the shell then calls
//! [`AppState::run_pending_delivery`] from its idle hook, after the window is
//! gone.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use chrono::Local;

use crate::capture::Capture;
use crate::output::{self, Delivery, DeliveryReport};
use crate::platform::{Permission, SnapPlatform};
use crate::selection::PxRect;
use crate::settings::{Settings, SettingsStore};

/// The status line when a capture was refused for want of the Screen
/// Recording permission (see [`AppState::ready_to_capture`]).
pub const CAPTURE_NEEDS_PERMISSION: &str =
    "Nothing was captured: Screen Snap needs the Screen Recording permission.";

/// What the single window is showing. See the module docs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Hidden,
    Settings,
    Overlay,
}

/// The frozen screen the overlay shows.
#[derive(Clone, Debug)]
pub struct ActiveCapture {
    pub capture: Rc<Capture>,
    /// The pixels shown and cropped from: the screen, with the pointer drawn
    /// in when the settings include it.
    pub pixels: Arc<Vec<u8>>,
}

struct Inner {
    platform: Rc<dyn SnapPlatform>,
    mode: Mode,
    mode_version: u64,
    settings: Settings,
    settings_version: u64,
    settings_dirty: bool,
    store: Option<Box<dyn SettingsStore>>,
    capture: Option<ActiveCapture>,
    capture_version: u64,
    pending: Option<Delivery>,
    status: Option<String>,
    recording_hotkey: bool,
    hotkey_error: Option<String>,
    quit_requested: bool,
}

/// Shared app state. See the module docs.
#[derive(Clone)]
pub struct AppState(Rc<RefCell<Inner>>);

impl AppState {
    pub fn new(platform: Rc<dyn SnapPlatform>) -> Self {
        Self(Rc::new(RefCell::new(Inner {
            platform,
            mode: Mode::Hidden,
            mode_version: 0,
            settings: Settings::default(),
            settings_version: 0,
            settings_dirty: false,
            store: None,
            capture: None,
            capture_version: 0,
            pending: None,
            status: None,
            recording_hotkey: false,
            hotkey_error: None,
            quit_requested: false,
        })))
    }

    pub fn platform(&self) -> Rc<dyn SnapPlatform> {
        Rc::clone(&self.0.borrow().platform)
    }

    // ---- settings -------------------------------------------------------

    /// Load the settings from `store` and save changes back to it.
    pub fn use_settings_store(&self, store: Box<dyn SettingsStore>) {
        let mut inner = self.0.borrow_mut();
        inner.settings = Settings::from_json(store.load().as_deref());
        inner.store = Some(store);
        inner.settings_version += 1;
    }

    pub fn settings(&self) -> Settings {
        self.0.borrow().settings.clone()
    }

    /// Bumped on every settings change (the shell re-registers the hotkey
    /// when the hotkey in it changed).
    pub fn settings_version(&self) -> u64 {
        self.0.borrow().settings_version
    }

    /// Change the settings. Saved by [`AppState::save_settings`].
    pub fn update_settings(&self, change: impl FnOnce(&mut Settings)) {
        {
            let mut inner = self.0.borrow_mut();
            let before = inner.settings.clone();
            change(&mut inner.settings);
            if inner.settings == before {
                return;
            }
            inner.settings_version += 1;
            inner.settings_dirty = true;
        }
        agg_gui::animation::request_draw();
    }

    /// Write changed settings to the store, if any.
    pub fn save_settings(&self) {
        let mut inner = self.0.borrow_mut();
        if !inner.settings_dirty {
            return;
        }
        inner.settings_dirty = false;
        let blob = inner.settings.to_json();
        if let Some(store) = &inner.store {
            store.save(&blob);
        }
    }

    // ---- mode -----------------------------------------------------------

    pub fn mode(&self) -> Mode {
        self.0.borrow().mode
    }

    /// Bumped on every mode change and every new capture.
    pub fn mode_version(&self) -> u64 {
        self.0.borrow().mode_version
    }

    fn set_mode(&self, mode: Mode) {
        {
            let mut inner = self.0.borrow_mut();
            inner.mode = mode;
            inner.mode_version += 1;
            if mode != Mode::Settings {
                inner.recording_hotkey = false;
            }
        }
        agg_gui::animation::request_layout();
        agg_gui::animation::request_draw();
    }

    /// Open the settings window (menu-bar "Settings…", first run). Ignored
    /// while a capture is on screen.
    pub fn show_settings(&self) {
        if self.mode() == Mode::Overlay {
            return;
        }
        self.update_settings(|s| s.welcomed = true);
        self.set_mode(Mode::Settings);
    }

    /// Close the settings window (its close button, or the window's ×).
    pub fn hide_settings(&self) {
        if self.mode() == Mode::Settings {
            self.set_mode(Mode::Hidden);
        }
    }

    // ---- capture --------------------------------------------------------

    /// Whether the shell may grab the screen now. Without the Screen
    /// Recording permission the OS hands back only the wallpaper, and a
    /// frozen screen of that would look like a broken capture; so instead the
    /// settings window opens, whose warning says how to allow it, with
    /// [`CAPTURE_NEEDS_PERMISSION`] on the status line.
    pub fn ready_to_capture(&self) -> bool {
        if self.platform().screen_capture_permission() != Permission::Denied {
            return true;
        }
        self.set_status(Some(CAPTURE_NEEDS_PERMISSION.to_string()));
        self.show_settings();
        false
    }

    /// Show `capture` in the overlay. Replaces any capture already showing.
    pub fn begin_capture(&self, capture: Capture) {
        {
            let mut inner = self.0.borrow_mut();
            let pixels = capture.composed(inner.settings.include_cursor);
            inner.capture = Some(ActiveCapture {
                capture: Rc::new(capture),
                pixels,
            });
            inner.capture_version += 1;
            inner.recording_hotkey = false;
        }
        self.set_mode(Mode::Overlay);
    }

    /// The frozen screen on show, if any.
    pub fn capture(&self) -> Option<ActiveCapture> {
        self.0.borrow().capture.clone()
    }

    /// Bumped for every new capture (the overlay resets its selection).
    pub fn capture_version(&self) -> u64 {
        self.0.borrow().capture_version
    }

    /// Throw the frozen screen away (Escape).
    pub fn cancel_capture(&self) {
        self.0.borrow_mut().capture = None;
        if self.mode() == Mode::Overlay {
            self.set_mode(Mode::Hidden);
        }
    }

    /// Crop `rect` out of the frozen screen, queue it for delivery and hide.
    /// An empty selection cancels instead.
    pub fn finish_capture(&self, rect: PxRect) {
        let Some(active) = self.0.borrow_mut().capture.take() else {
            return;
        };
        if !rect.is_empty() {
            let cap = &active.capture;
            let image = output::crop(&active.pixels, cap.width, cap.height, rect);
            if !image.pixels.is_empty() {
                self.0.borrow_mut().pending = Some(Delivery {
                    image,
                    scale: cap.scale,
                });
            }
        }
        self.set_mode(Mode::Hidden);
    }

    /// Whether a delivery is waiting for [`AppState::run_pending_delivery`].
    pub fn has_pending_delivery(&self) -> bool {
        self.0.borrow().pending.is_some()
    }

    /// Copy and / or save the queued selection now. The shell calls this once
    /// the overlay window is gone.
    pub fn run_pending_delivery(&self) -> Option<DeliveryReport> {
        let delivery = self.0.borrow_mut().pending.take()?;
        let (settings, platform) = {
            let inner = self.0.borrow();
            (inner.settings.clone(), Rc::clone(&inner.platform))
        };
        let report = output::deliver(&delivery, &settings, platform.as_ref(), Local::now());
        self.set_status(Some(report.message()));
        Some(report)
    }

    // ---- status, hotkey recording, quit --------------------------------

    /// The last capture's outcome (or another notice) for the settings window.
    pub fn status(&self) -> Option<String> {
        self.0.borrow().status.clone()
    }

    pub fn set_status(&self, status: Option<String>) {
        self.0.borrow_mut().status = status;
        agg_gui::animation::request_draw();
    }

    /// Whether the settings window's recorder is listening for a new
    /// shortcut. The shell suspends the global hotkey meanwhile so the
    /// current combination can be typed into the recorder.
    pub fn recording_hotkey(&self) -> bool {
        self.0.borrow().recording_hotkey
    }

    pub fn set_recording_hotkey(&self, recording: bool) {
        self.0.borrow_mut().recording_hotkey = recording;
        agg_gui::animation::request_draw();
    }

    /// Why the OS refused the current hotkey (already taken by another app),
    /// reported by the shell; `None` when it is registered.
    pub fn hotkey_error(&self) -> Option<String> {
        self.0.borrow().hotkey_error.clone()
    }

    pub fn set_hotkey_error(&self, error: Option<String>) {
        self.0.borrow_mut().hotkey_error = error;
        agg_gui::animation::request_draw();
    }

    pub fn request_quit(&self) {
        self.0.borrow_mut().quit_requested = true;
        agg_gui::animation::request_draw();
    }

    pub fn quit_requested(&self) -> bool {
        self.0.borrow().quit_requested
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::{CursorSnapshot, RgbaImage};
    use crate::platform::Permission;
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    struct Fake {
        copied: RefCell<Vec<RgbaImage>>,
        folder: PathBuf,
    }

    impl SnapPlatform for Fake {
        fn copy_image(&self, image: &RgbaImage) -> Result<(), String> {
            self.copied.borrow_mut().push(image.clone());
            Ok(())
        }
        fn default_save_folder(&self) -> PathBuf {
            self.folder.clone()
        }
        fn pick_folder(&self, _: &Path) -> Option<PathBuf> {
            None
        }
        fn reveal(&self, _: &Path) -> Result<(), String> {
            Ok(())
        }
        fn screen_capture_permission(&self) -> Permission {
            Permission::Granted
        }
        fn open_permission_settings(&self) {}
    }

    fn capture_with_cursor() -> Capture {
        let screen = RgbaImage::new(4, 4, [0, 0, 255, 255].repeat(16)).unwrap();
        let cursor = CursorSnapshot {
            image: RgbaImage::new(1, 1, vec![255, 0, 0, 255]).unwrap(),
            x: 1,
            y: 1,
        };
        Capture::new(screen, 1.0, Some(cursor)).unwrap()
    }

    #[test]
    fn capture_flow_crops_composed_pixels_and_delivers_after_hiding() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Rc::new(Fake {
            copied: RefCell::new(Vec::new()),
            folder: dir.path().into(),
        });
        let state = AppState::new(fake.clone());
        state.begin_capture(capture_with_cursor());
        assert_eq!(state.mode(), Mode::Overlay);

        state.finish_capture(PxRect::new(1, 1, 2, 2));
        assert_eq!(state.mode(), Mode::Hidden);
        assert!(state.capture().is_none());
        assert!(
            fake.copied.borrow().is_empty(),
            "nothing is delivered until the shell asks"
        );

        let report = state.run_pending_delivery().unwrap();
        assert!(report.is_ok());
        let copied = fake.copied.borrow();
        assert_eq!(
            copied[0].pixel(0, 0),
            Some([255, 0, 0, 255]),
            "pointer drawn in"
        );
        assert_eq!(copied[0].pixel(1, 1), Some([0, 0, 255, 255]));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        assert!(state.status().unwrap().starts_with("Capture copied"));
    }

    #[test]
    fn cursor_can_be_left_out_and_empty_selections_cancel() {
        let fake = Rc::new(Fake {
            copied: RefCell::new(Vec::new()),
            folder: PathBuf::new(),
        });
        let state = AppState::new(fake.clone());
        state.update_settings(|s| {
            s.include_cursor = false;
            s.save_to_file = false;
        });
        state.begin_capture(capture_with_cursor());
        state.finish_capture(PxRect::new(1, 1, 1, 1));
        state.run_pending_delivery();
        assert_eq!(fake.copied.borrow()[0].pixel(0, 0), Some([0, 0, 255, 255]));

        state.begin_capture(capture_with_cursor());
        state.finish_capture(PxRect::new(1, 1, 0, 3));
        assert!(!state.has_pending_delivery());
        assert_eq!(state.mode(), Mode::Hidden);
    }

    #[test]
    fn settings_window_cannot_open_over_the_overlay() {
        let fake = Rc::new(Fake {
            copied: RefCell::new(Vec::new()),
            folder: PathBuf::new(),
        });
        let state = AppState::new(fake);
        state.show_settings();
        assert_eq!(state.mode(), Mode::Settings);
        assert!(state.settings().welcomed);
        state.begin_capture(capture_with_cursor());
        state.show_settings();
        assert_eq!(state.mode(), Mode::Overlay);
        state.cancel_capture();
        assert_eq!(state.mode(), Mode::Hidden);
    }
}
