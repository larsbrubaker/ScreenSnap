//! Keeping AppKit from shrinking the overlay below the menu bar.
//!
//! The problem: on every `Resized`, agg-gui-shell asks winit
//! `Window::is_maximized()`, and winit's macOS `is_zoomed` briefly switches a
//! borderless window to `Titled | Resizable` (because `-isZoomed` ignores
//! borderless windows) and back. Each `-setStyleMask:` makes AppKit fit the
//! window to the screen's visible area as a titled window must be — below
//! the menu bar and notch, above the Dock — so the overlay, which has to cover
//! the whole screen, ends up ~155 pt short and offset.
//!
//! The fix: while an overlay is up, winit's `NSWindow` subclass answers
//! `-constrainFrameRect:toScreen:` with the rectangle unchanged; otherwise it
//! defers to `NSWindow`'s own implementation. The override is added to
//! winit's subclass only (never to `NSWindow` itself), once, and changes
//! behaviour only while [`set_unconstrained`] is on.

use std::ffi::CString;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
use objc2::{ffi, sel, ClassType, Encode};
use objc2_app_kit::NSWindow;
use objc2_foundation::NSRect;

/// Whether the overlay is up (constraints off).
static UNCONSTRAINED: AtomicBool = AtomicBool::new(false);
/// `NSWindow`'s own implementation (set once by [`install`]).
static ORIGINAL: OnceLock<Imp> = OnceLock::new();

/// The C signature of `-constrainFrameRect:toScreen:`.
type ConstrainFn =
    unsafe extern "C-unwind" fn(*mut AnyObject, Sel, NSRect, *mut AnyObject) -> NSRect;

/// The override. See the module docs.
unsafe extern "C-unwind" fn constrain_frame_rect(
    this: *mut AnyObject,
    cmd: Sel,
    rect: NSRect,
    screen: *mut AnyObject,
) -> NSRect {
    if UNCONSTRAINED.load(Ordering::Relaxed) {
        return rect;
    }
    match ORIGINAL.get() {
        Some(&imp) => {
            // SAFETY: `imp` is NSWindow's implementation of this very
            // selector (stored by `install`), so it has this signature; the
            // arguments are passed through unchanged from AppKit's call.
            let original: ConstrainFn = unsafe { std::mem::transmute::<Imp, ConstrainFn>(imp) };
            unsafe { original(this, cmd, rect, screen) }
        }
        None => rect,
    }
}

/// Add the override to `window`'s class (winit's `NSWindow` subclass). Safe
/// to call repeatedly; does nothing for a plain `NSWindow`.
pub fn install(window: &NSWindow) {
    let class: &AnyClass = window.class();
    if class == NSWindow::class() || ORIGINAL.get().is_some() {
        return;
    }
    let selector = sel!(constrainFrameRect:toScreen:);
    let Some(base) = NSWindow::class().instance_method(selector) else {
        return;
    };
    let Ok(types) = CString::new(format!("{}@:{}@", NSRect::ENCODING, NSRect::ENCODING)) else {
        return;
    };
    if ORIGINAL.set(base.implementation()).is_err() {
        return;
    }
    // SAFETY: `constrain_frame_rect` has exactly the C signature the type
    // string describes (NSRect return; self, _cmd, NSRect, id arguments), so
    // the runtime may call it for this selector. It is added to winit's
    // subclass only; `class_addMethod` leaves an existing method alone (it
    // returns NO), in which case nothing changes. Erasing the fn type to `Imp`
    // is how the Objective-C runtime stores every implementation.
    unsafe {
        let imp = std::mem::transmute::<ConstrainFn, Imp>(constrain_frame_rect);
        ffi::class_addMethod(
            class as *const AnyClass as *mut AnyClass,
            selector,
            imp,
            types.as_ptr(),
        );
    }
}

/// Turn the screen constraint off (overlay up) or back on.
pub fn set_unconstrained(on: bool) {
    UNCONSTRAINED.store(on, Ordering::Relaxed);
}
