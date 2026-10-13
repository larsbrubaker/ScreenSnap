//! Freezing the screen: the monitor under the pointer, captured with `xcap`
//! the instant the hotkey fires (Screen Snap's window is still hidden, so
//! open menus and tooltips of other apps are in the shot), plus the mouse
//! pointer as it looked then.
//!
//! The pixels become a core [`Capture`] (physical pixels, top row first);
//! the pointer becomes a [`CursorSnapshot`] positioned in those pixels, which
//! the core draws in when "Include mouse pointer" is on. [`Grab::monitor`]
//! tells the window module ([`crate::window`]) exactly which monitor to cover.
//!
//! Coordinates: `xcap` (and CoreGraphics underneath it on macOS) describe
//! monitors in the *global display space* — on macOS that is points with the
//! origin at the top-left of the primary display, Y down; on Windows and X11 it
//! is physical pixels, also Y down. The pointer location comes from the same
//! space ([`pointer`]), so placing it in the capture is a subtraction and a
//! scale ([`pointer_in_capture`]).
//!
//! The pointer image itself is per OS: macOS reads `NSCursor` (`pointer_macos`);
//! other platforms capture without it for now (PLAN.md "Platform roadmap").

#[cfg(target_os = "macos")]
#[path = "pointer_macos.rs"]
mod pointer;
#[cfg(not(target_os = "macos"))]
#[path = "pointer_other.rs"]
mod pointer;

use std::time::Instant;

use screensnap_core::capture::{Capture, CursorSnapshot, RgbaImage};
use xcap::Monitor;

/// Where a captured monitor sits, in the global display space (see the module
/// docs), and its size in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MonitorFrame {
    /// The OS's display id (`CGDirectDisplayID` on macOS).
    pub id: u32,
    pub x: i32,
    pub y: i32,
    /// Size in global display units (points on macOS, pixels elsewhere).
    pub width: u32,
    pub height: u32,
    /// Size of the captured image in physical pixels.
    pub pixel_width: u32,
    pub pixel_height: u32,
    /// Physical pixels per logical point.
    pub scale: f64,
}

/// One frozen monitor.
pub struct Grab {
    pub capture: Capture,
    pub monitor: MonitorFrame,
    /// The pointer's hotspot in capture pixels (Y down), when known — where
    /// the overlay's crosshair starts before the mouse moves.
    pub pointer: Option<(f64, f64)>,
}

/// The pointer image as the OS draws it.
pub struct PointerImage {
    /// The pointer image, rasterised at the capture's scale.
    pub image: RgbaImage,
    /// The hotspot inside `image`, in its pixels.
    pub hotspot: (f64, f64),
}

/// Capture the monitor under the pointer (or the primary one when the
/// pointer cannot be located).
pub fn grab_screen() -> Result<Grab, String> {
    let started = Instant::now();
    let location = pointer::location();
    let monitor = pick_monitor(location)?;
    let err = |e: xcap::XCapError| e.to_string();
    let (x, y) = (monitor.x().map_err(err)?, monitor.y().map_err(err)?);
    let (width, height) = (
        monitor.width().map_err(err)?,
        monitor.height().map_err(err)?,
    );
    let reported_scale = monitor.scale_factor().map_err(err)? as f64;

    let image = monitor
        .capture_image()
        .map_err(|e| format!("Could not capture the screen: {e}"))?;
    let (pixel_width, pixel_height) = image.dimensions();
    let scale = capture_scale(pixel_width, width, reported_scale);
    let frame = MonitorFrame {
        id: monitor.id().map_err(err)?,
        x,
        y,
        width,
        height,
        pixel_width,
        pixel_height,
        scale,
    };
    // The buffer moves into the core's image as is: no copy of a 20+ MB frame.
    let screen = RgbaImage::new(pixel_width, pixel_height, image.into_raw())
        .ok_or("The captured image has an unexpected size")?;

    let pointer_px = location.map(|loc| pointer_in_capture(loc, &frame));
    let cursor = pointer_px.and_then(|at| {
        let img = pointer::image(scale)?;
        Some(CursorSnapshot {
            x: (at.0 - img.hotspot.0).round() as i32,
            y: (at.1 - img.hotspot.1).round() as i32,
            image: img.image,
        })
    });
    let capture = Capture::new(screen, scale, cursor).ok_or("The captured screen is empty")?;
    log_timing(started, &frame);
    Ok(Grab {
        capture,
        monitor: frame,
        pointer: pointer_px,
    })
}

/// The monitor containing `location`, else the primary one, else the first.
fn pick_monitor(location: Option<(f64, f64)>) -> Result<Monitor, String> {
    if let Some((x, y)) = location {
        if let Ok(monitor) = Monitor::from_point(x.floor() as i32, y.floor() as i32) {
            return Ok(monitor);
        }
    }
    let monitors = Monitor::all().map_err(|e| format!("Could not list the displays: {e}"))?;
    let primary = monitors
        .iter()
        .position(|m| m.is_primary().unwrap_or(false))
        .unwrap_or(0);
    monitors
        .into_iter()
        .nth(primary)
        .ok_or_else(|| "No display found".to_string())
}

/// Physical pixels per display unit. On macOS the monitor is measured in
/// points, so the ratio of the image to it is exact for the shot taken; on
/// the other platforms the monitor is already in pixels and the OS's DPI
/// scale is the logical scale.
fn capture_scale(pixel_width: u32, monitor_width: u32, reported: f64) -> f64 {
    if cfg!(target_os = "macos") && monitor_width > 0 {
        f64::from(pixel_width) / f64::from(monitor_width)
    } else if reported.is_finite() && reported > 0.0 {
        reported
    } else {
        1.0
    }
}

/// A global-display-space point as capture pixels of `monitor` (Y down).
pub fn pointer_in_capture(location: (f64, f64), monitor: &MonitorFrame) -> (f64, f64) {
    let units_to_px = if cfg!(target_os = "macos") {
        monitor.scale
    } else {
        1.0
    };
    (
        (location.0 - f64::from(monitor.x)) * units_to_px,
        (location.1 - f64::from(monitor.y)) * units_to_px,
    )
}

/// `SCREENSNAP_LOG_TIMING=1` prints how long the grab took (capture speed is
/// what makes the freeze feel instant, so it is worth watching).
fn log_timing(started: Instant, frame: &MonitorFrame) {
    if std::env::var_os("SCREENSNAP_LOG_TIMING").is_some() {
        eprintln!(
            "screensnap: captured display {} ({}x{} px, scale {}) in {:.0} ms",
            frame.id,
            frame.pixel_width,
            frame.pixel_height,
            frame.scale,
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(x: i32, y: i32, scale: f64) -> MonitorFrame {
        MonitorFrame {
            id: 1,
            x,
            y,
            width: 1512,
            height: 982,
            pixel_width: (1512.0 * scale) as u32,
            pixel_height: (982.0 * scale) as u32,
            scale,
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn pointer_maps_points_to_capture_pixels() {
        // A Retina display to the left of the primary one.
        let m = frame(-1512, 0, 2.0);
        assert_eq!(pointer_in_capture((-1512.0, 0.0), &m), (0.0, 0.0));
        assert_eq!(pointer_in_capture((-12.5, 100.0), &m), (2999.0, 200.0));
        assert_eq!(capture_scale(3024, 1512, 1.0), 2.0);
    }

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn pointer_maps_pixels_to_capture_pixels() {
        let m = frame(1920, -200, 1.5);
        assert_eq!(pointer_in_capture((1930.0, -100.0), &m), (10.0, 100.0));
        assert_eq!(capture_scale(2880, 2880, 1.5), 1.5);
        assert_eq!(capture_scale(2880, 2880, f64::NAN), 1.0);
    }
}
