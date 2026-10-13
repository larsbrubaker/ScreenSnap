//! Window shaping on Windows and Linux with winit alone (see `window/mod.rs`
//! for the table). winit has no window alpha, so a shown window is visible at
//! once and [`WindowShaper::reveal`] has nothing to do; `AlwaysOnTop` stands in
//! for macOS's screen-saver level. On these platforms `xcap` reports monitors
//! in physical pixels, which is what the overlay is positioned and sized in.

use std::sync::Arc;

use agg_gui_shell::winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use agg_gui_shell::winit::window::{Window, WindowLevel};

use super::SETTINGS_SIZE;
use crate::capture::MonitorFrame;

/// Shapes winit's window for each mode. See the module docs.
pub struct WindowShaper {
    window: Arc<Window>,
}

impl WindowShaper {
    pub fn new(window: Arc<Window>) -> Self {
        Self { window }
    }

    /// Nothing to prepare: the first idle pass hides the window if needed.
    pub fn prepare_start(&self) {}

    /// Nothing to do once launched (no Dock to stay out of).
    pub fn on_launched(&self) {}

    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    /// A normal decorated window, centred on its monitor, in front.
    pub fn show_settings(&self) {
        let w = &self.window;
        w.set_window_level(WindowLevel::Normal);
        w.set_decorations(true);
        w.set_resizable(true);
        let _ = w.request_inner_size(LogicalSize::new(SETTINGS_SIZE.0, SETTINGS_SIZE.1));
        if let Some(monitor) = w.current_monitor().or_else(|| w.primary_monitor()) {
            let (m_pos, m_size, size) = (monitor.position(), monitor.size(), w.outer_size());
            let x = m_pos.x + (m_size.width as i32 - size.width as i32) / 2;
            let y = m_pos.y + (m_size.height as i32 - size.height as i32) / 2;
            w.set_outer_position(PhysicalPosition::new(x, y));
        }
        w.set_visible(true);
        w.focus_window();
    }

    /// Cover `monitor` exactly, on top.
    pub fn show_overlay(&self, monitor: &MonitorFrame) {
        let w = &self.window;
        w.set_decorations(false);
        w.set_resizable(false);
        w.set_window_level(WindowLevel::AlwaysOnTop);
        w.set_outer_position(PhysicalPosition::new(monitor.x, monitor.y));
        let _ = w.request_inner_size(PhysicalSize::new(monitor.pixel_width, monitor.pixel_height));
        w.set_visible(true);
        w.focus_window();
    }

    /// winit positions the overlay itself; nothing moves it afterwards.
    pub fn keep_overlay_frame(&self) -> bool {
        false
    }

    /// Shown windows are already visible here.
    pub fn reveal(&self) {}
}
