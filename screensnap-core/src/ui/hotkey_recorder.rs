//! `HotkeyRecorder`: the settings window's shortcut field (`docs/PLAN.md`
//! §Settings window).
//!
//! At rest it is a button-like box showing the current capture shortcut
//! ([`Hotkey::display`] in the platform's
//! [`crate::platform::SnapPlatform::hotkey_style`]).
//! A click (or Space / Enter while it has focus) starts recording: the box
//! says "Type a shortcut…" and [`AppState::set_recording_hotkey`] tells the
//! native shell to suspend the global hotkey, so the current combination can
//! be typed too. The next key press that [`Hotkey::from_key_event`] accepts
//! becomes the new shortcut (`AppState::update_settings`) and recording ends.
//! Escape, a second click, or losing keyboard focus (a click elsewhere) ends
//! recording with no change. A combination a global shortcut may not use
//! shows a brief hint to the right of the box ([`invalid_hint`]); modifier
//! keys pressed on their own are just waited through.
//!
//! [`AppState::recording_hotkey`] is the one truth for "recording", so a mode
//! change that clears it (the window closing) also ends recording here. The
//! widget is focusable — a press focuses it — and consumes every key while
//! recording, so Tab, Escape and Enter never reach other widgets meanwhile.
//! Key mapping and validity live in [`crate::hotkey`]; this file only does the
//! interaction and drawing.

use std::sync::Arc;
use std::time::Duration;

use agg_gui::focus::FocusId;
use agg_gui::text::Font;
use agg_gui::{
    DrawCtx, Event, EventResult, HAnchor, Key, MouseButton, Point, Rect, Size, VAnchor, Widget,
    WidgetBase,
};

use crate::hotkey::{code_for_key, DisplayStyle, Hotkey};
use crate::state::AppState;

/// Widget id of the recorder.
pub const HOTKEY_RECORDER_ID: &str = "hotkey-recorder";
/// The recorder's [`Widget::focus_id`], for `agg_gui::focus::request_focus`.
pub const HOTKEY_RECORDER_FOCUS: FocusId = 0x5353_484b; // "SSHK"
/// What the box says while recording.
pub const RECORDING_PROMPT: &str = "Type a shortcut\u{2026}";
/// How long the "not a usable shortcut" hint stays up.
pub const HINT_DURATION: Duration = Duration::from_millis(3000);

const BOX_HEIGHT: f64 = 30.0;
const BOX_MIN_WIDTH: f64 = 170.0;
const BOX_PAD_X: f64 = 14.0;
const FONT_SIZE: f64 = 14.0;
const HINT_FONT_SIZE: f64 = 11.0;
const HINT_GAP: f64 = 12.0;

/// The hint shown when a pressed combination cannot be a global shortcut.
pub fn invalid_hint(style: DisplayStyle) -> &'static str {
    match style {
        DisplayStyle::Mac => "Use \u{2318} or Ctrl with a key, or a function key",
        DisplayStyle::Windows => "Use Ctrl or Win with a key, or a function key",
        DisplayStyle::Linux => "Use Ctrl or Super with a key, or a function key",
    }
}

/// Time on agg-gui's UI clock (virtual in tests), as an offset from its epoch.
fn ui_time() -> Duration {
    agg_gui::clock::since(agg_gui::clock::epoch())
}

/// Split `text` at spaces into lines no wider than `max_width` with the
/// context's current font (a word wider than that gets a line of its own).
fn wrap(ctx: &dyn DrawCtx, text: &str, max_width: f64) -> Vec<String> {
    let width = |s: &str| ctx.measure_text(s).map_or(0.0, |m| m.width);
    let mut lines: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let candidate = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if !line.is_empty() && width(&candidate) > max_width {
            lines.push(std::mem::replace(&mut line, word.to_string()));
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// The shortcut field. See the module docs.
pub struct HotkeyRecorder {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    base: WidgetBase,
    state: AppState,
    font: Arc<Font>,
    style: DisplayStyle,
    hovered: bool,
    focused: bool,
    /// When the invalid-combination hint went up (`ui_time`).
    hint_since: Option<Duration>,
    /// The box's width from the last paint (it fits its text).
    box_width: f64,
}

impl HotkeyRecorder {
    pub fn new(state: &AppState, font: Arc<Font>) -> Self {
        let style = state.platform().hotkey_style();
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            base: WidgetBase::new()
                .with_h_anchor(HAnchor::STRETCH)
                .with_v_anchor(VAnchor::CENTER),
            state: state.clone(),
            font,
            style,
            hovered: false,
            focused: false,
            hint_since: None,
            box_width: BOX_MIN_WIDTH,
        }
        .with_name(HOTKEY_RECORDER_ID)
    }

    /// Whether it is listening for a new shortcut.
    pub fn is_recording(&self) -> bool {
        self.state.recording_hotkey()
    }

    /// The text in the box: the current shortcut, or the prompt.
    pub fn shown_text(&self) -> String {
        if self.is_recording() {
            RECORDING_PROMPT.to_string()
        } else {
            self.state.settings().hotkey.display(self.style)
        }
    }

    /// The hint to the right of the box, while it is up.
    pub fn hint(&self) -> Option<&'static str> {
        let since = self.hint_since?;
        (ui_time().saturating_sub(since) < HINT_DURATION).then(|| invalid_hint(self.style))
    }

    fn start(&mut self) {
        self.hint_since = None;
        self.state.set_recording_hotkey(true);
    }

    fn stop(&mut self) {
        self.hint_since = None;
        if self.state.recording_hotkey() {
            self.state.set_recording_hotkey(false);
        }
        agg_gui::animation::request_draw();
    }

    fn box_rect(&self) -> Rect {
        let h = BOX_HEIGHT.min(self.bounds.height);
        Rect::new(0.0, (self.bounds.height - h) / 2.0, self.box_width, h)
    }

    /// A key while recording: always consumed.
    fn record_key(&mut self, key: &Key, modifiers: agg_gui::Modifiers) {
        if *key == Key::Escape {
            self.stop();
            return;
        }
        if let Some(hotkey) = Hotkey::from_key_event(key, modifiers) {
            self.state.update_settings(|s| s.hotkey = hotkey);
            self.stop();
            return;
        }
        if code_for_key(key).is_some() || matches!(key, Key::Char(_)) {
            self.hint_since = Some(ui_time());
            agg_gui::animation::request_draw_after(HINT_DURATION);
            agg_gui::animation::request_draw();
        }
    }
}

impl Widget for HotkeyRecorder {
    fn type_name(&self) -> &'static str {
        "HotkeyRecorder"
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    fn bounds(&self) -> Rect {
        self.bounds
    }

    fn set_bounds(&mut self, bounds: Rect) {
        self.bounds = bounds;
    }

    fn children(&self) -> &[Box<dyn Widget>] {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn Widget>> {
        &mut self.children
    }

    fn widget_base(&self) -> Option<&WidgetBase> {
        Some(&self.base)
    }

    fn widget_base_mut(&mut self) -> Option<&mut WidgetBase> {
        Some(&mut self.base)
    }

    fn h_anchor(&self) -> HAnchor {
        self.base.h_anchor
    }

    fn v_anchor(&self) -> VAnchor {
        self.base.v_anchor
    }

    fn is_focusable(&self) -> bool {
        true
    }

    fn focus_id(&self) -> Option<FocusId> {
        Some(HOTKEY_RECORDER_FOCUS)
    }

    fn properties(&self) -> Vec<(&'static str, String)> {
        vec![
            ("text", self.shown_text()),
            ("recording", self.is_recording().to_string()),
        ]
    }

    /// Only the box takes the pointer; the hint beside it is just text.
    fn hit_test(&self, local_pos: Point) -> bool {
        let r = self.box_rect();
        local_pos.x >= r.x
            && local_pos.x <= r.x + r.width
            && local_pos.y >= r.y
            && local_pos.y <= r.y + r.height
    }

    fn layout(&mut self, available: Size) -> Size {
        let size = Size::new(available.width.max(BOX_MIN_WIDTH), BOX_HEIGHT);
        self.bounds = Rect::new(0.0, 0.0, size.width, size.height);
        size
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        let v = ctx.visuals();
        let recording = self.is_recording();
        let text = self.shown_text();
        ctx.set_font(Arc::clone(&self.font));
        ctx.set_font_size(FONT_SIZE);
        let metrics = ctx.measure_text(&text);
        let text_w = metrics.as_ref().map_or(0.0, |m| m.width);
        // Wide enough for the prompt, so the box does not jump on a click.
        let prompt_w = ctx.measure_text(RECORDING_PROMPT).map_or(0.0, |m| m.width);
        self.box_width = (text_w.max(prompt_w) + 2.0 * BOX_PAD_X)
            .max(BOX_MIN_WIDTH)
            .min(self.bounds.width);
        let r = self.box_rect();

        let fill = if recording {
            v.accent.with_alpha(0.14)
        } else if self.hovered {
            v.widget_bg_hovered
        } else {
            v.widget_bg
        };
        ctx.begin_path();
        ctx.rounded_rect(r.x, r.y, r.width, r.height, 6.0);
        ctx.set_fill_color(fill);
        ctx.fill();
        let (stroke, width) = if recording || self.focused {
            (v.accent_focus, 2.0)
        } else {
            (v.widget_stroke, 1.0)
        };
        ctx.begin_path();
        ctx.rounded_rect(
            r.x + width / 2.0,
            r.y + width / 2.0,
            r.width - width,
            r.height - width,
            6.0,
        );
        ctx.set_stroke_color(stroke);
        ctx.set_line_width(width);
        ctx.stroke();

        let (ascent, descent) = metrics.map_or((FONT_SIZE * 0.75, FONT_SIZE * 0.25), |m| {
            (m.ascent, m.descent)
        });
        let baseline = r.y + (r.height - (ascent + descent)) / 2.0 + descent;
        ctx.set_fill_color(if recording { v.text_dim } else { v.text_color });
        ctx.fill_text(&text, r.x + (r.width - text_w) / 2.0, baseline);

        if let Some(hint) = self.hint() {
            ctx.set_font_size(HINT_FONT_SIZE);
            ctx.set_fill_color(v.warning_color());
            let x = r.x + r.width + HINT_GAP;
            let lines = wrap(ctx, hint, self.bounds.width - x);
            // One line centred on the box, or two filling its height.
            let line_h = HINT_FONT_SIZE + 2.0;
            let top = r.y + (r.height + line_h * lines.len() as f64) / 2.0;
            for (i, line) in lines.iter().enumerate() {
                let baseline = top - line_h * (i + 1) as f64 + 3.0;
                ctx.fill_text(line, x, baseline);
            }
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        match event {
            Event::MouseMove { pos } => {
                let over = self.hit_test(*pos);
                if over != self.hovered {
                    self.hovered = over;
                    agg_gui::animation::request_draw();
                }
                EventResult::Ignored
            }
            Event::MouseLeave => {
                if self.hovered {
                    self.hovered = false;
                    agg_gui::animation::request_draw();
                }
                EventResult::Ignored
            }
            Event::MouseDown {
                button: MouseButton::Left,
                ..
            } => {
                if self.is_recording() {
                    self.stop();
                } else {
                    self.start();
                }
                EventResult::Consumed
            }
            Event::MouseUp {
                button: MouseButton::Left,
                ..
            } => EventResult::Consumed,
            Event::KeyDown { key, modifiers } if self.is_recording() => {
                self.record_key(key, *modifiers);
                EventResult::Consumed
            }
            Event::KeyDown {
                key: Key::Enter | Key::Char(' '),
                ..
            } => {
                self.start();
                EventResult::Consumed
            }
            Event::KeyUp { .. } if self.is_recording() => EventResult::Consumed,
            Event::FocusGained => {
                self.focused = true;
                agg_gui::animation::request_draw();
                EventResult::Ignored
            }
            Event::FocusLost => {
                self.focused = false;
                self.stop();
                EventResult::Ignored
            }
            _ => EventResult::Ignored,
        }
    }
}
