//! `CaptureOverlay`: the frozen screen the user marquees a region on
//! (`docs/PLAN.md` §Overlay). Built afresh by the root `Rebuilder`
//! ([`crate::ui`]) for every capture, so it never inherits an old selection.
//!
//! What it shows, bottom to top:
//! - the capture's pixels ([`crate::state::ActiveCapture::pixels`]) at 1:1
//!   physical pixels, top-left aligned, drawn with `draw_image_rgba_arc` and
//!   the same `Arc` every frame so the GPU backend keeps one texture;
//! - a dimming over everything outside the selection (all of it until a drag
//!   starts), and a light outline around the selection;
//! - crosshair lines through the pixel under the pointer;
//! - a `W × H` pill for the selection;
//! - the magnifier ([`loupe`]) when [`crate::settings::Settings::show_magnifier`]
//!   is on.
//!
//! Input: a left drag selects whole capture pixels, inclusive of the start
//! and end pixel ([`PxRect::spanning`]); releasing a drag that left its first
//! pixel calls [`AppState::finish_capture`]. A click that never leaves its
//! pixel selects nothing and the overlay stays up. Escape (also mid-drag) or
//! a right click calls [`AppState::cancel_capture`]; Space or Enter with no
//! drag in progress captures the whole screen. Keys arrive through
//! `on_unconsumed_key` (the overlay needs no focus: nothing else in the tree
//! can take keys while it is up) and through `on_event` should it ever hold
//! focus. The OS cursor is a crosshair.
//!
//! All conversions between widget space and capture pixels go through
//! [`CaptureSpace`]; the capture's top-left sits at the widget's top-left, so
//! a widget taller than the capture (it should not be) only shifts `y`.

mod loupe;
mod paint;

pub use loupe::{hex_color, magnify, place, sample, texel_factor, LOUPE_PIXELS, LOUPE_ZOOM};

use agg_gui::ux_scale::effective_scale;
use agg_gui::{
    set_cursor_icon, CursorIcon, DrawCtx, Event, EventResult, HAnchor, Key, Modifiers, MouseButton,
    Point, Rect, Size, VAnchor, Widget, WidgetBase,
};

use crate::fonts::Fonts;
use crate::selection::{CaptureSpace, PxPoint, PxRect};
use crate::state::{ActiveCapture, AppState};
use loupe::{Loupe, Source, Spot};
use paint::{crosshair, dim_outside, outline_selection, LocalRect, TextStyle};

/// Widget id of the overlay.
pub const OVERLAY_ID: &str = "capture-overlay";

/// A left-button drag in progress.
#[derive(Clone, Copy, Debug)]
struct Drag {
    start: PxPoint,
    end: PxPoint,
    /// The pointer has left the start pixel: this is a selection, not a click.
    moved: bool,
}

/// The capture overlay. See the module docs.
pub struct CaptureOverlay {
    bounds: Rect,
    children: Vec<Box<dyn Widget>>,
    base: WidgetBase,
    state: AppState,
    active: Option<ActiveCapture>,
    space: Option<CaptureSpace>,
    show_magnifier: bool,
    text: TextStyle,
    loupe: Loupe,
    /// The pointer (local, logical, Y up) while it is over the overlay.
    pointer: Option<Point>,
    drag: Option<Drag>,
    /// Finished or cancelled: ignore input until the root rebuilds.
    done: bool,
    /// The first paint asked agg-gui to re-resolve the cursor.
    cursor_claimed: bool,
}

impl CaptureOverlay {
    /// The overlay for `state`'s current capture (empty if there is none).
    pub fn new(state: &AppState, fonts: &Fonts) -> Self {
        let active = state.capture();
        let space = active.as_ref().map(|a| {
            let c = &a.capture;
            CaptureSpace::new(c.width, c.height, c.scale)
        });
        Self {
            bounds: Rect::default(),
            children: Vec::new(),
            base: WidgetBase::new()
                .with_h_anchor(HAnchor::STRETCH)
                .with_v_anchor(VAnchor::STRETCH),
            state: state.clone(),
            active,
            space,
            show_magnifier: state.settings().show_magnifier,
            text: TextStyle::new(std::sync::Arc::clone(&fonts.ui)),
            loupe: Loupe::default(),
            pointer: None,
            drag: None,
            done: false,
            cursor_claimed: false,
        }
        .with_name(OVERLAY_ID)
    }

    /// The selection so far, in capture pixels (`None` before a drag leaves
    /// its first pixel).
    pub fn selection(&self) -> Option<PxRect> {
        self.drag
            .filter(|d| d.moved)
            .map(|d| PxRect::spanning(d.start, d.end))
    }

    /// The capture pixel under the pointer, if it is over the overlay.
    pub fn pointer_px(&self) -> Option<PxPoint> {
        let pos = self.pointer?;
        self.px_at(pos)
    }

    /// Whether the magnifier is drawn (it is on and the pointer is over the
    /// overlay).
    pub fn magnifier_shown(&self) -> bool {
        self.show_magnifier && self.pointer_px().is_some()
    }

    /// Where the capture's bottom edge sits in local `y`: the capture is
    /// top-aligned in the widget.
    fn y_offset(&self) -> f64 {
        self.space
            .map_or(0.0, |s| self.bounds.height - s.logical_size().1)
    }

    /// The capture pixel under local point `pos`.
    fn px_at(&self, pos: Point) -> Option<PxPoint> {
        let space = self.space?;
        Some(space.to_px(pos.x, pos.y - self.y_offset()))
    }

    /// Pixel rectangle `r` in local coordinates.
    fn local_rect(&self, r: PxRect) -> Option<LocalRect> {
        let space = self.space?;
        let (x, y, w, h) = space.to_local(r);
        Some((x, y + self.y_offset(), w, h))
    }

    /// The local centre of capture pixel `p`.
    fn px_center(&self, p: PxPoint) -> Option<(f64, f64)> {
        let (x, y, w, h) = self.local_rect(PxRect::new(p.x, p.y, 1, 1))?;
        Some((x + w / 2.0, y + h / 2.0))
    }

    fn area(&self) -> (f64, f64) {
        (self.bounds.width, self.bounds.height)
    }

    fn cancel(&mut self) {
        self.done = true;
        self.drag = None;
        self.state.cancel_capture();
    }

    fn finish(&mut self, rect: PxRect) {
        self.done = true;
        self.drag = None;
        self.state.finish_capture(rect);
    }

    fn pointer_moved(&mut self, pos: Point) {
        set_cursor_icon(CursorIcon::Crosshair);
        let before = (self.pointer_px(), self.selection());
        self.pointer = Some(pos);
        if let Some(px) = self.px_at(pos) {
            if let Some(drag) = &mut self.drag {
                drag.end = px;
                drag.moved |= px != drag.start;
            }
        }
        if (self.pointer_px(), self.selection()) != before {
            agg_gui::animation::request_draw();
        }
    }

    fn press(&mut self, pos: Point) {
        self.pointer = Some(pos);
        if let Some(px) = self.px_at(pos) {
            self.drag = Some(Drag {
                start: px,
                end: px,
                moved: false,
            });
        }
        agg_gui::animation::request_draw();
    }

    fn release(&mut self, pos: Point) {
        self.pointer_moved(pos);
        match self.selection() {
            Some(rect) => self.finish(rect),
            None => {
                // A click without a drag selects nothing; keep waiting.
                self.drag = None;
                agg_gui::animation::request_draw();
            }
        }
    }

    fn key(&mut self, key: &Key) -> EventResult {
        if self.done {
            return EventResult::Ignored;
        }
        match key {
            Key::Escape => {
                self.cancel();
                EventResult::Consumed
            }
            Key::Enter | Key::Char(' ') if self.drag.is_none() => {
                match self.space {
                    Some(space) => self.finish(space.full()),
                    None => self.cancel(),
                }
                EventResult::Consumed
            }
            Key::Enter | Key::Char(' ') => EventResult::Consumed,
            _ => EventResult::Ignored,
        }
    }
}

impl Widget for CaptureOverlay {
    fn type_name(&self) -> &'static str {
        "CaptureOverlay"
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

    fn layout(&mut self, available: Size) -> Size {
        let size = Size::new(available.width.max(0.0), available.height.max(0.0));
        self.bounds = Rect::new(0.0, 0.0, size.width, size.height);
        size
    }

    fn paint(&mut self, ctx: &mut dyn DrawCtx) {
        if !self.cursor_claimed {
            // Show the crosshair cursor (and pick up the pointer) without
            // waiting for the first mouse move.
            self.cursor_claimed = true;
            agg_gui::invalidate_cursor();
        }
        let (Some(active), Some(space)) = (&self.active, self.space) else {
            return;
        };
        let area = self.area();
        let (lw, lh) = space.logical_size();
        let oy = self.y_offset();
        let cap = &active.capture;
        ctx.draw_image_rgba_arc(&active.pixels, cap.width, cap.height, 0.0, oy, lw, lh);

        let hairline = 1.0 / effective_scale().max(1e-6);
        let selection = self.selection();
        let hole = selection.and_then(|r| self.local_rect(r));
        dim_outside(ctx, area, hole);
        if let Some(r) = hole {
            outline_selection(ctx, r, hairline);
        }

        let pointer = self.pointer_px();
        if let Some((cx, cy)) = pointer.and_then(|p| self.px_center(p)) {
            crosshair(ctx, area, cx, cy, hairline);
        }

        if let (Some(sel), Some((x, y, _, h))) = (selection, hole) {
            // Above the selection's top-left corner, or just inside it when
            // there is no room above.
            let label = format!("{} \u{d7} {}", sel.width, sel.height);
            let gap = 6.0;
            let above = y + h + gap;
            let py = if above + TextStyle::pill_height() <= area.1 {
                above
            } else {
                (y + h - gap - TextStyle::pill_height()).max(0.0)
            };
            self.text.pill(ctx, &label, x, py, area);
        }

        if self.show_magnifier {
            if let (Some(p), Some(pos)) = (pointer, self.pointer) {
                let src = Source {
                    pixels: &active.pixels,
                    width: cap.width,
                    height: cap.height,
                    scale: cap.scale,
                };
                let at = self.px_center(p).unwrap_or((pos.x, pos.y));
                self.loupe.paint(
                    ctx,
                    &src,
                    &self.text,
                    Spot {
                        pointer: at,
                        center: p,
                    },
                    area,
                    hairline,
                );
            }
        }
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        if self.done {
            return EventResult::Ignored;
        }
        match event {
            Event::MouseMove { pos } => {
                self.pointer_moved(*pos);
                EventResult::Consumed
            }
            Event::MouseLeave if self.drag.is_none() => {
                if self.pointer.take().is_some() {
                    agg_gui::animation::request_draw();
                }
                EventResult::Ignored
            }
            Event::MouseDown {
                pos,
                button: MouseButton::Left,
                ..
            } => {
                self.press(*pos);
                EventResult::Consumed
            }
            Event::MouseUp {
                pos,
                button: MouseButton::Left,
                ..
            } => {
                if self.drag.is_some() {
                    self.release(*pos);
                }
                EventResult::Consumed
            }
            Event::MouseDown {
                button: MouseButton::Right,
                ..
            } => {
                self.cancel();
                EventResult::Consumed
            }
            Event::MouseDown { .. } | Event::MouseUp { .. } => EventResult::Consumed,
            Event::KeyDown { key, .. } => self.key(key),
            _ => EventResult::Ignored,
        }
    }

    fn on_unconsumed_key(&mut self, key: &Key, _modifiers: Modifiers) -> EventResult {
        self.key(key)
    }
}
