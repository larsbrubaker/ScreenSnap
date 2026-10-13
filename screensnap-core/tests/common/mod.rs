//! Shared harness for the headless widget tests: the real app (`build_app`)
//! over a recording fake [`SnapPlatform`], with no window and no GPU. Each
//! frame drains agg-gui's UI queue, lays out and paints into a software
//! framebuffer, as a shell does; input goes through the same `App` entry
//! points a shell uses (Y-down physical pixels). Widgets are found by their
//! ids.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use agg_gui::widget::{find_widget_by_id, find_widget_screen_rect};
use agg_gui::{App, Framebuffer, GfxCtx, Key, Label, Modifiers, MouseButton, Rect, Size};
use screensnap_core::capture::{Capture, RgbaImage};
use screensnap_core::hotkey::DisplayStyle;
use screensnap_core::platform::{Permission, SnapPlatform};
use screensnap_core::{build_app, install_theme_and_fonts, AppState};

/// A scripted OS: records every call in `calls`, keeps copied images, answers
/// the folder picker with `picked` and reports `permission`.
pub struct FakePlatform {
    pub calls: RefCell<Vec<String>>,
    pub copied: RefCell<Vec<RgbaImage>>,
    pub folder: PathBuf,
    pub picked: RefCell<Option<PathBuf>>,
    pub permission: Cell<Permission>,
}

impl FakePlatform {
    pub fn new(folder: &Path) -> Rc<Self> {
        Rc::new(Self {
            calls: RefCell::new(Vec::new()),
            copied: RefCell::new(Vec::new()),
            folder: folder.to_path_buf(),
            picked: RefCell::new(None),
            permission: Cell::new(Permission::Granted),
        })
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.borrow().clone()
    }

    fn record(&self, call: String) {
        self.calls.borrow_mut().push(call);
    }
}

impl SnapPlatform for FakePlatform {
    fn copy_image(&self, image: &RgbaImage) -> Result<(), String> {
        self.record(format!("copy {}x{}", image.width, image.height));
        self.copied.borrow_mut().push(image.clone());
        Ok(())
    }
    fn default_save_folder(&self) -> PathBuf {
        self.folder.clone()
    }
    fn pick_folder(&self, start: &Path) -> Option<PathBuf> {
        self.record(format!("pick_folder {}", start.display()));
        self.picked.borrow().clone()
    }
    fn reveal(&self, path: &Path) -> Result<(), String> {
        self.record(format!("reveal {}", path.display()));
        Ok(())
    }
    fn screen_capture_permission(&self) -> Permission {
        self.permission.get()
    }
    fn open_permission_settings(&self) {
        self.record("open_permission_settings".to_string());
    }
    fn hotkey_style(&self) -> DisplayStyle {
        DisplayStyle::Mac
    }
}

/// A `width`×`height` capture whose pixel `(x, y)` is `[x, y, 200, 255]`
/// (low bytes), at `scale`.
pub fn ramp_capture(width: u32, height: u32, scale: f64) -> Capture {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&[x as u8, y as u8, 200, 255]);
        }
    }
    let image = RgbaImage::new(width, height, pixels).expect("sizes match");
    Capture::new(image, scale, None).expect("valid capture")
}

/// The real app over a fake platform.
pub struct Harness {
    pub state: AppState,
    pub platform: Rc<FakePlatform>,
    pub app: App,
    /// The window size in physical pixels.
    pub size: Size,
    pub scale: f64,
    _dir: tempfile::TempDir,
}

impl Harness {
    /// A window of `width`×`height` logical points at device `scale`.
    pub fn new(width: f64, height: f64, scale: f64) -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let platform = FakePlatform::new(dir.path());
        let state = AppState::new(platform.clone());
        let fonts = install_theme_and_fonts(scale, false).expect("bundled fonts load");
        let app = build_app(state.clone(), fonts);
        let mut h = Self {
            state,
            platform,
            app,
            size: Size::new(width * scale, height * scale),
            scale,
            _dir: dir,
        };
        h.frame();
        h
    }

    /// The settings window, open.
    pub fn settings() -> Self {
        let mut h = Self::new(560.0, 520.0, 1.0);
        h.state.show_settings();
        h.frame();
        h
    }

    /// One shell frame: drain queued UI work, lay out, paint.
    pub fn frame(&mut self) {
        self.snapshot();
    }

    /// Lay out and paint one frame, returning its pixels: RGBA, top row first.
    pub fn snapshot(&mut self) -> Vec<u8> {
        agg_gui::ui_thread::invoke_pending_actions();
        self.app.layout(self.size);
        let mut fb = Framebuffer::new(self.size.width as u32, self.size.height as u32);
        let mut ctx = GfxCtx::new(&mut fb);
        self.app.paint(&mut ctx);
        drop(ctx);
        fb.pixels_flipped()
    }

    pub fn visible(&self, id: &str) -> bool {
        find_widget_by_id(self.app.root(), id).is_some()
            && find_widget_screen_rect(self.app.root(), id).is_some()
    }

    /// The on-screen rectangle of widget `id` (logical, Y up).
    pub fn rect(&self, id: &str) -> Rect {
        find_widget_screen_rect(self.app.root(), id)
            .unwrap_or_else(|| panic!("no visible widget {id:?}"))
    }

    /// The widget `id` downcast to its concrete type.
    pub fn widget<T: 'static>(&self, id: &str) -> &T {
        find_widget_by_id(self.app.root(), id)
            .and_then(|w| w.as_any())
            .and_then(|a| a.downcast_ref::<T>())
            .unwrap_or_else(|| panic!("no {} {id:?}", std::any::type_name::<T>()))
    }

    /// The text of the `Label` with id `id`.
    pub fn label_text(&self, id: &str) -> String {
        self.widget::<Label>(id).text_str().to_string()
    }

    /// The physical, Y-down point at the centre of widget `id`.
    pub fn center_of(&self, id: &str) -> (f64, f64) {
        let r = self.rect(id);
        let logical_h = self.size.height / self.scale;
        (
            (r.x + r.width / 2.0) * self.scale,
            (logical_h - (r.y + r.height / 2.0)) * self.scale,
        )
    }

    /// Click the centre of widget `id`.
    pub fn click(&mut self, id: &str) {
        let (x, y) = self.center_of(id);
        self.click_at(x, y);
    }

    /// Click at a physical, Y-down point.
    pub fn click_at(&mut self, x: f64, y: f64) {
        self.app.on_mouse_move(x, y);
        self.app
            .on_mouse_down(x, y, MouseButton::Left, Modifiers::default());
        self.app
            .on_mouse_up(x, y, MouseButton::Left, Modifiers::default());
        self.frame();
    }

    /// Move to a physical, Y-down point and run a frame.
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.app.on_mouse_move(x, y);
        self.frame();
    }

    /// A left-button drag between physical, Y-down points, in a few steps.
    pub fn drag(&mut self, from: (f64, f64), to: (f64, f64)) {
        self.app.on_mouse_move(from.0, from.1);
        self.app
            .on_mouse_down(from.0, from.1, MouseButton::Left, Modifiers::default());
        self.frame();
        for step in 1..=4 {
            let t = f64::from(step) / 4.0;
            let x = from.0 + (to.0 - from.0) * t;
            let y = from.1 + (to.1 - from.1) * t;
            self.app.on_mouse_move(x, y);
            self.frame();
        }
        self.app
            .on_mouse_up(to.0, to.1, MouseButton::Left, Modifiers::default());
        self.frame();
    }

    /// Press and release `key` with `modifiers`, then run a frame.
    pub fn key_with(&mut self, key: Key, modifiers: Modifiers) {
        self.app.on_key_down(key.clone(), modifiers);
        self.app.on_key_up(key, modifiers);
        self.frame();
    }

    pub fn key(&mut self, key: Key) {
        self.key_with(key, Modifiers::default());
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        agg_gui::ui_thread::unbind_current_thread();
    }
}
