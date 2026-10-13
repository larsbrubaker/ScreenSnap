//! The mouse pointer on Windows and Linux — not read yet.
//!
//! winit only reports the pointer inside its own window, so there is no
//! portable way to find it while Screen Snap is hidden. Until the per-OS
//! readers land (Windows `GetCursorInfo` + `DrawIconEx`, X11 XFixes; see
//! PLAN.md "Platform roadmap"), captures there cover the primary monitor and
//! leave the pointer out.

use super::PointerImage;

/// The pointer's global position: unknown on this platform.
pub fn location() -> Option<(f64, f64)> {
    None
}

/// The pointer image: unknown on this platform.
pub fn image(_scale: f64) -> Option<PointerImage> {
    None
}
