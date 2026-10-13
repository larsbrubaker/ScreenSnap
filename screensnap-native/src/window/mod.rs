//! Reshaping the one window for each mode (PLAN.md "The one-window model"):
//!
//! | Mode     | Window |
//! |----------|--------|
//! | Hidden   | ordered out, app inactive (focus goes back to the previous app) |
//! | Settings | titled, resizable, [`SETTINGS_SIZE`] logical, centred, normal level, activated |
//! | Overlay  | borderless, exactly the captured monitor's frame (menu bar included), above the menu bar and pop-up menus, on every Space, activated and key |
//!
//! [`WindowShaper`] is per OS: `macos.rs` drives the `NSWindow` behind winit's
//! window directly (window level, collection behaviour, alpha, activation
//! policy — none of which winit exposes); `other.rs` is the winit-only
//! fallback for Windows and Linux. Both have the same methods:
//!
//! - `new(window)`; `prepare_start()` in the builder, before the shell shows the
//!   window (macOS: alpha 0, so a start in Hidden never flashes);
//!   `on_launched()` on the first idle pass (macOS: no Dock icon);
//! - `hide()`, `show_settings()`, `show_overlay(&MonitorFrame)`;
//! - `reveal()` — a shown window starts invisible (alpha 0 on macOS) and is
//!   revealed by the host once a frame of the new content has been painted,
//!   so stale pixels from the previous mode never flash.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_constrain;
#[cfg(target_os = "macos")]
pub use macos::WindowShaper;

#[cfg(not(target_os = "macos"))]
mod other;
#[cfg(not(target_os = "macos"))]
pub use other::WindowShaper;

/// The settings window's content size, in logical pixels.
pub const SETTINGS_SIZE: (f64, f64) = (560.0, 520.0);

/// A rectangle in Cocoa screen space (points, origin at the bottom-left of
/// the primary display, Y up) for one given in the global display space
/// (origin at the primary display's top-left, Y down) — the flip needs only
/// the primary display's height. Used when no `NSScreen` matches a display id.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn flip_to_cocoa(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    primary_height: f64,
) -> (f64, f64, f64, f64) {
    (x, primary_height - (y + height), width, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_rects_flip_into_cocoa_space() {
        // The primary display itself maps onto itself.
        assert_eq!(
            flip_to_cocoa(0.0, 0.0, 1512.0, 982.0, 982.0),
            (0.0, 0.0, 1512.0, 982.0)
        );
        // A taller display to the right, top-aligned with the primary one,
        // reaches below Cocoa's y = 0.
        assert_eq!(
            flip_to_cocoa(1512.0, 0.0, 1920.0, 1200.0, 982.0),
            (1512.0, -218.0, 1920.0, 1200.0)
        );
        // A display above the primary one sits above it in Cocoa too.
        assert_eq!(
            flip_to_cocoa(0.0, -1080.0, 1920.0, 1080.0, 982.0),
            (0.0, 982.0, 1920.0, 1080.0)
        );
    }
}
