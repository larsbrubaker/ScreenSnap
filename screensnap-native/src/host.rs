//! The `ShellHost`: everything Screen Snap does around agg-gui-shell's frames,
//! all of it on the idle pass of the event loop (PLAN.md "Events into the
//! loop"):
//!
//! 1. drain the OS events ([`crate::events`]): hotkey and menu-bar picks;
//! 2. take a due capture — first thing, while the window is still hidden, so
//!    the frozen screen shows exactly what was there ([`crate::capture`]);
//! 3. when `AppState::mode_version` moved, reshape the window
//!    ([`crate::window`]) and arm the reveal: a shown window stays invisible
//!    until a frame of the new content, at the window's new size, has been
//!    painted (or a short timeout passes, so a window can never get stuck
//!    invisible while it holds the keyboard);
//! 4. once the window is gone after a capture, deliver it (clipboard / PNG);
//! 5. save changed settings, keep the global hotkey registered as the settings
//!    say (suspended while the recorder listens) and the menu's label in step;
//! 6. quit when asked.
//!
//! The red close button of the settings window only hides it.

use std::sync::Arc;
use std::time::{Duration, Instant};

use agg_gui::App;
use agg_gui_shell::winit::window::Window;
use agg_gui_shell::{Frame, ShellControl, ShellHost};
use screensnap_core::hotkey::Hotkey;
use screensnap_core::{AppState, Mode};

use crate::capture::{self, MonitorFrame};
use crate::events::{self, NativeEvent};
use crate::hotkey::HotkeyRegistrar;
use crate::platform;
use crate::tray::Tray;
use crate::window::WindowShaper;

/// How long a menu-bar "Capture Region" waits for the menu to fade out, so
/// the menu is not in the shot.
const MENU_CAPTURE_DELAY: Duration = Duration::from_millis(250);
/// The longest a shown window stays invisible waiting for its first frame.
const REVEAL_TIMEOUT: Duration = Duration::from_millis(700);

/// How long after the overlay gains focus its pointer is re-asserted.
const POINTER_SETTLE: Duration = Duration::from_millis(300);

/// The pointer position handed to a freshly revealed overlay.
struct PointerSeed {
    /// Capture pixels, Y down.
    at: (f64, f64),
    armed: Instant,
    /// When the window was first seen focused.
    focused: Option<Instant>,
}

/// A shown window waiting for its first painted frame.
struct PendingReveal {
    /// Frames painted before the window was reshaped.
    after_frame: u64,
    since: Instant,
    /// Where to put the pointer once the overlay is laid out (capture px).
    pointer: Option<(f64, f64)>,
}

/// The app side of the shell. See the module docs.
pub struct AppHost {
    pub state: AppState,
    pub window: Arc<Window>,
    pub shaper: WindowShaper,
    pub hotkeys: HotkeyRegistrar,
    pub tray: Option<Tray>,
    /// `--capture-now`: capture once, right after start-up.
    pub capture_at_start: bool,
    launched: bool,
    applied_mode_version: Option<u64>,
    capture_due: Option<Instant>,
    overlay_monitor: Option<MonitorFrame>,
    overlay_pointer: Option<(f64, f64)>,
    reveal: Option<PendingReveal>,
    /// The pointer position being seeded into the overlay (`place_pointer`).
    pointer_seed: Option<PointerSeed>,
    /// Real input arrived since the seed was armed.
    input_seen: bool,
    /// Index and physical size of the last painted frame.
    last_frame: (u64, u32, u32),
    /// The hotkey the registrar and the menu were last synced to.
    synced_hotkey: Option<Option<Hotkey>>,
    reported_hotkey_error: Option<String>,
}

impl AppHost {
    pub fn new(
        state: AppState,
        window: Arc<Window>,
        hotkeys: HotkeyRegistrar,
        tray: Option<Tray>,
        capture_at_start: bool,
    ) -> Self {
        Self {
            shaper: WindowShaper::new(Arc::clone(&window)),
            state,
            window,
            hotkeys,
            tray,
            capture_at_start,
            launched: false,
            applied_mode_version: None,
            capture_due: None,
            overlay_monitor: None,
            overlay_pointer: None,
            reveal: None,
            pointer_seed: None,
            input_seen: false,
            last_frame: (0, 0, 0),
            synced_hotkey: None,
            reported_hotkey_error: None,
        }
    }

    fn handle(&mut self, event: NativeEvent) {
        match event {
            NativeEvent::HotkeyPressed => {
                if !self.state.recording_hotkey() {
                    self.schedule_capture(Duration::ZERO);
                }
            }
            NativeEvent::CaptureFromMenu => self.schedule_capture(MENU_CAPTURE_DELAY),
            NativeEvent::ShowSettings => self.state.show_settings(),
            NativeEvent::OpenCapturesFolder => {
                let settings = self.state.settings();
                let folder = platform::captures_folder(&settings, self.state.platform().as_ref());
                if let Err(message) = platform::open_folder(&folder) {
                    self.state.set_status(Some(message));
                }
            }
            NativeEvent::Quit => self.state.request_quit(),
        }
    }

    fn schedule_capture(&mut self, delay: Duration) {
        // A second press while the frozen screen is up would capture the
        // overlay itself; the first capture stands.
        if self.state.mode() != Mode::Overlay && self.capture_due.is_none() {
            self.capture_due = Some(Instant::now() + delay);
        }
    }

    /// Take the scheduled capture if it is due; otherwise make sure the loop
    /// wakes for it.
    fn run_due_capture(&mut self) {
        let Some(due) = self.capture_due else {
            return;
        };
        let now = Instant::now();
        if now < due {
            agg_gui::animation::request_draw_after(due - now);
            return;
        }
        self.capture_due = None;
        if self.state.mode() == Mode::Overlay {
            return;
        }
        if self.state.mode() == Mode::Settings {
            // Out of the shot; the overlay replaces it.
            self.shaper.hide();
        }
        match capture::grab_screen() {
            Ok(grab) => {
                self.overlay_monitor = Some(grab.monitor);
                self.overlay_pointer = grab.pointer;
                self.state.begin_capture(grab.capture);
            }
            Err(message) => {
                // Say why in the settings window rather than doing nothing.
                eprintln!("screensnap: {message}");
                self.state.set_status(Some(message));
                // Bumps the mode version even from Settings, so a settings
                // window hidden above is shown again.
                self.state.show_settings();
            }
        }
    }

    /// Reshape the window when the mode changed.
    fn apply_mode(&mut self) {
        let version = self.state.mode_version();
        if self.applied_mode_version == Some(version) {
            return;
        }
        self.applied_mode_version = Some(version);
        self.reveal = None;
        match self.state.mode() {
            Mode::Hidden => self.shaper.hide(),
            Mode::Settings => {
                self.shaper.show_settings();
                self.arm_reveal(None);
            }
            Mode::Overlay => match self.overlay_monitor {
                Some(monitor) => {
                    self.shaper.show_overlay(&monitor);
                    self.arm_reveal(self.overlay_pointer);
                }
                // No monitor means no capture of ours: nothing to show.
                None => self.state.cancel_capture(),
            },
        }
        // The new screen must paint even though no input arrived.
        agg_gui::animation::request_layout();
        agg_gui::animation::request_draw();
        self.window.request_redraw();
    }

    fn arm_reveal(&mut self, pointer: Option<(f64, f64)>) {
        self.reveal = Some(PendingReveal {
            after_frame: self.last_frame.0,
            since: Instant::now(),
            pointer,
        });
    }

    /// Reveal a shown window once its new content is on screen.
    fn advance_reveal(&mut self, app: &mut App, painted: bool) {
        let Some(pending) = &self.reveal else {
            return;
        };
        let size = self.window.inner_size();
        let (index, w, h) = self.last_frame;
        let fresh = painted && index > pending.after_frame && (w, h) == (size.width, size.height);
        if !fresh && pending.since.elapsed() < REVEAL_TIMEOUT {
            // Keep frames coming until one matches the new size.
            agg_gui::animation::request_draw();
            return;
        }
        self.pointer_seed = pending.pointer.map(|at| PointerSeed {
            at,
            armed: Instant::now(),
            focused: None,
        });
        self.input_seen = false;
        self.reveal = None;
        self.shaper.reveal();
        self.place_pointer(app);
    }

    /// The overlay learns the pointer from moves; seed it with one so the
    /// crosshair and magnifier show before the mouse moves. Window pixels
    /// are capture pixels (the window covers the monitor 1:1).
    ///
    /// Once is not enough: while the overlay activates, AppKit refreshes the
    /// view's tracking area and winit reports the cursor leaving, which the
    /// overlay takes as "pointer gone". So the position is re-asserted on
    /// every idle pass for [`POINTER_SETTLE`] after the window gains focus,
    /// and seeding stops at once when real input arrives (the user's own
    /// moves win). Re-asserting an unchanged position repaints nothing.
    fn place_pointer(&mut self, app: &mut App) {
        let Some(seed) = &mut self.pointer_seed else {
            return;
        };
        if self.state.mode() != Mode::Overlay || self.input_seen {
            self.pointer_seed = None;
            return;
        }
        let focused = self.window.has_focus() || seed.armed.elapsed() >= REVEAL_TIMEOUT;
        if focused {
            let since = *seed.focused.get_or_insert_with(Instant::now);
            app.on_mouse_move(seed.at.0, seed.at.1);
            if since.elapsed() >= POINTER_SETTLE {
                self.pointer_seed = None;
                return;
            }
        }
        agg_gui::animation::request_draw_after(Duration::from_millis(16));
    }

    /// Register the hotkey the settings ask for (none while recording) and
    /// show it in the menu; report a refusal to the settings window.
    fn sync_hotkey(&mut self) {
        let settings = self.state.settings();
        let wanted = (!self.state.recording_hotkey()).then_some(settings.hotkey);
        if self.synced_hotkey.as_ref() == Some(&wanted) {
            return;
        }
        let error = self.hotkeys.sync(wanted.as_ref());
        if let Some(tray) = &self.tray {
            // While recording, keep showing the configured shortcut.
            if wanted.is_some() {
                tray.set_hotkey(if error.is_none() {
                    wanted.as_ref()
                } else {
                    None
                });
            }
        }
        self.synced_hotkey = Some(wanted);
        if error != self.reported_hotkey_error {
            self.state.set_hotkey_error(error.clone());
            self.reported_hotkey_error = error;
        }
    }
}

impl ShellHost for AppHost {
    fn on_frame(&mut self, _app: &mut App, frame: &Frame) {
        self.last_frame = (frame.index, frame.width, frame.height);
        if frame.input_since_last_frame {
            self.input_seen = true;
        }
    }

    fn on_idle(&mut self, app: &mut App, control: &mut ShellControl<'_>) {
        if !self.launched {
            self.launched = true;
            self.shaper.on_launched();
            if self.capture_at_start {
                self.schedule_capture(Duration::ZERO);
            }
        }
        for event in events::drain() {
            self.handle(event);
        }
        self.run_due_capture();
        self.apply_mode();
        if self.state.mode() == Mode::Overlay && self.shaper.keep_overlay_frame() {
            // A frame painted at the constrained size does not count.
            if let Some(pending) = &mut self.reveal {
                pending.after_frame = self.last_frame.0;
            }
            agg_gui::animation::request_draw();
        }
        self.advance_reveal(app, control.painted());
        self.place_pointer(app);
        // The window is gone once Hidden is applied: deliver now.
        if self.state.mode() == Mode::Hidden && self.state.has_pending_delivery() {
            if let Some(report) = self.state.run_pending_delivery() {
                if !report.is_ok() {
                    eprintln!("screensnap: {}", report.message());
                }
            }
        }
        self.state.save_settings();
        self.sync_hotkey();
        if self.state.quit_requested() {
            control.request_exit();
        }
    }

    fn on_close_requested(&mut self, _app: &mut App) -> bool {
        self.state.hide_settings();
        false
    }

    fn on_exit(&mut self, _app: &mut App) {
        self.state.save_settings();
        self.hotkeys.unregister();
    }
}
