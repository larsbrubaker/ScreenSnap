//! macOS window shaping through AppKit (see `window/mod.rs` for the table).
//!
//! winit's window is an `NSWindow` subclass; its raw handle gives the content
//! `NSView`, whose `window` is the `NSWindow` to configure. winit's own setters
//! are used where they exist (decorations, resizability, size) so its internal
//! state stays in step; level, collection behaviour, alpha, frame, ordering
//! and app activation go to AppKit directly.
//!
//! The overlay window:
//! - borderless at `NSScreenSaverWindowLevel` (1000), above the menu bar,
//!   the Dock and other apps' open menus — which is the point: the frozen
//!   screen covers the live one exactly;
//! - `CanJoinAllSpaces | FullScreenAuxiliary | Stationary | IgnoresCycle`, so
//!   it appears over a full-screen app's Space and stays out of Exposé and
//!   the window cycle;
//! - framed to the `NSScreen` whose `NSScreenNumber` is the captured display's
//!   `CGDirectDisplayID` (Cocoa coordinates straight from AppKit; the flip in
//!   [`super::flip_to_cocoa`] is only the fallback).
//!
//! The app runs with the `Accessory` activation policy (no Dock icon, no app
//! menu). winit sets `Regular` when it finishes launching an unbundled binary
//! (`cargo run`), so the policy is set again on the first idle pass; a bundle
//! declares `LSUIElement` and never shows a Dock icon at all.

use std::cell::Cell;
use std::sync::Arc;

use agg_gui_shell::winit::dpi::LogicalSize;
use agg_gui_shell::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use agg_gui_shell::winit::window::Window;
use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSNormalWindowLevel, NSScreen,
    NSScreenSaverWindowLevel, NSView, NSWindow, NSWindowCollectionBehavior,
};
use objc2_foundation::{NSNumber, NSPoint, NSRect, NSSize, NSString};

use super::{flip_to_cocoa, SETTINGS_SIZE};
use crate::capture::MonitorFrame;

/// Shapes winit's window for each mode. See the module docs.
pub struct WindowShaper {
    window: Arc<Window>,
    /// The overlay's Cocoa frame while it is up (see `keep_overlay_frame`).
    overlay_frame: Cell<Option<NSRect>>,
}

/// Alpha of a shown window waiting for its first frame: invisible to the eye
/// but not zero, because AppKit reports a fully transparent window as
/// occluded and wgpu's Metal surface then refuses to hand out a drawable
/// (wgpu issue 8309 workaround) — the frame the reveal waits for would never
/// be painted.
const WAITING_ALPHA: f64 = 1.0 / 255.0;

impl WindowShaper {
    pub fn new(window: Arc<Window>) -> Self {
        Self {
            window,
            overlay_frame: Cell::new(None),
        }
    }

    /// The `NSWindow` behind winit's window.
    fn ns_window(&self) -> Option<Retained<NSWindow>> {
        let handle = self.window.window_handle().ok()?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return None;
        };
        // SAFETY: winit's AppKit handle carries a pointer to the window's
        // content NSView, valid while the window lives — and `self.window`
        // keeps it alive for the duration of this borrow. We are on the main
        // thread (the shell's event loop), where AppKit views may be used.
        let view: &NSView = unsafe { appkit.ns_view.cast::<NSView>().as_ref() };
        view.window()
    }

    /// Forget the overlay frame and let AppKit constrain the window again.
    fn leave_overlay(&self) {
        if self.overlay_frame.take().is_some() {
            super::macos_constrain::set_unconstrained(false);
        }
    }

    /// Invisible until the first mode is applied: the shell shows the window
    /// right after the builder returns, and a start in Hidden must not flash.
    pub fn prepare_start(&self) {
        if let Some(w) = self.ns_window() {
            w.setAlphaValue(0.0);
        }
    }

    /// First idle pass, after AppKit finished launching: no Dock icon.
    pub fn on_launched(&self) {
        ensure_accessory();
    }

    /// Order the window out and hand focus back to the app the user was in.
    pub fn hide(&self) {
        self.leave_overlay();
        if let Some(w) = self.ns_window() {
            w.orderOut(None);
            w.setAlphaValue(0.0);
        }
        if let Some(mtm) = MainThreadMarker::new() {
            NSApplication::sharedApplication(mtm).hide(None);
        }
    }

    /// A normal titled window, centred, in front and focused (alpha 0 until
    /// [`WindowShaper::reveal`]).
    pub fn show_settings(&self) {
        self.leave_overlay();
        let Some(w) = self.ns_window() else {
            return;
        };
        w.setAlphaValue(WAITING_ALPHA);
        w.setLevel(NSNormalWindowLevel);
        w.setCollectionBehavior(NSWindowCollectionBehavior::Default);
        w.setHasShadow(true);
        self.window.set_resizable(true);
        self.window.set_decorations(true);
        let _ = self
            .window
            .request_inner_size(LogicalSize::new(SETTINGS_SIZE.0, SETTINGS_SIZE.1));
        w.center();
        activate_and_front(&w);
    }

    /// Cover `monitor` exactly, above everything (alpha 0 until
    /// [`WindowShaper::reveal`]).
    pub fn show_overlay(&self, monitor: &MonitorFrame) {
        let (Some(w), Some(mtm)) = (self.ns_window(), MainThreadMarker::new()) else {
            return;
        };
        super::macos_constrain::install(&w);
        w.setAlphaValue(WAITING_ALPHA);
        self.window.set_decorations(false);
        self.window.set_resizable(false);
        w.setHasShadow(false);
        w.setLevel(NSScreenSaverWindowLevel);
        w.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary
                | NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        let frame = screen_frame(mtm, monitor.id).unwrap_or_else(|| fallback_frame(mtm, monitor));
        w.setFrame_display(frame, true);
        self.overlay_frame.set(Some(frame));
        activate_and_front(&w);
        super::macos_constrain::set_unconstrained(true);
    }

    /// Put the overlay back on its monitor's full frame if anything moved it.
    ///
    /// A safety net behind `macos_constrain`: should AppKit still fit the
    /// window to the screen's visible area (below the menu bar, above the
    /// Dock), the host's next idle pass restores the full frame. Cheap when
    /// nothing moved. Returns whether the frame was corrected.
    pub fn keep_overlay_frame(&self) -> bool {
        let (Some(target), Some(w)) = (self.overlay_frame.get(), self.ns_window()) else {
            return false;
        };
        if w.frame() == target {
            return false;
        }
        w.setFrame_display(target, true);
        true
    }

    /// Make a shown window visible (its new content has been painted).
    pub fn reveal(&self) {
        if let Some(w) = self.ns_window() {
            w.setAlphaValue(1.0);
        }
    }
}

/// Bring the app forward and make `w` the key window, so keys and clicks
/// reach it. Screen Snap is an accessory app that is never frontmost while
/// it waits, so it has to take activation (as winit itself does at launch).
fn activate_and_front(w: &NSWindow) {
    ensure_accessory();
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        app.unhideWithoutActivation();
        #[allow(deprecated)] // `activate` (macOS 14+) does not force; this still does
        app.activateIgnoringOtherApps(true);
    }
    w.makeKeyAndOrderFront(None);
}

/// No Dock icon: the `Accessory` activation policy. Checked whenever the
/// window is shown too, because winit sets `Regular` for an unbundled binary
/// when launching finishes, which can land after the first idle pass.
fn ensure_accessory() {
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        if app.activationPolicy() != NSApplicationActivationPolicy::Accessory {
            app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
        }
    }
}

/// The Cocoa frame of the screen showing display `display_id`.
fn screen_frame(mtm: MainThreadMarker, display_id: u32) -> Option<NSRect> {
    let key = NSString::from_str("NSScreenNumber");
    NSScreen::screens(mtm).iter().find_map(|screen| {
        let number = screen.deviceDescription().objectForKey(&key)?;
        let number = number.downcast::<NSNumber>().ok()?;
        (number.unsignedIntValue() == display_id).then(|| screen.frame())
    })
}

/// The Cocoa frame for `monitor` computed from its global-space rectangle.
fn fallback_frame(mtm: MainThreadMarker, monitor: &MonitorFrame) -> NSRect {
    let primary_height = NSScreen::screens(mtm)
        .firstObject()
        .map(|s| s.frame().size.height)
        .unwrap_or(f64::from(monitor.height));
    let (x, y, w, h) = flip_to_cocoa(
        f64::from(monitor.x),
        f64::from(monitor.y),
        f64::from(monitor.width),
        f64::from(monitor.height),
        primary_height,
    );
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}
