//! Drawing helpers for the capture overlay ([`super::CaptureOverlay`]): the
//! dimming around the selection, the crosshair, the selection outline and the
//! small dark "pill" labels (`W × H`), plus the rectangle / text primitives the
//! magnifier ([`super::loupe`]) shares.
//!
//! Everything here is in the overlay widget's local space — logical points,
//! Y up, origin bottom-left — and draws over the frozen screen, so it uses
//! fixed dark-and-light colours that read on any screen content rather than
//! the light / dark theme.

use std::sync::Arc;

use agg_gui::text::Font;
use agg_gui::{Color, DrawCtx};

/// Font size of the overlay's labels.
pub const OVERLAY_TEXT_SIZE: f64 = 12.0;
/// The dimming laid over everything outside the selection.
pub const DIM: Color = Color::rgba(0.0, 0.0, 0.0, 0.45);
/// Padding inside a pill label.
const PILL_PAD_X: f64 = 7.0;
const PILL_HEIGHT: f64 = 22.0;

/// A local rectangle `(x, y, w, h)`, Y up.
pub type LocalRect = (f64, f64, f64, f64);

/// Fill an axis-aligned rectangle.
pub fn fill_rect(ctx: &mut dyn DrawCtx, x: f64, y: f64, w: f64, h: f64, color: Color) {
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    ctx.begin_path();
    ctx.rect(x, y, w, h);
    ctx.set_fill_color(color);
    ctx.fill();
}

/// Stroke an axis-aligned rectangle (the line is centred on its edges).
pub fn stroke_rect(
    ctx: &mut dyn DrawCtx,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    color: Color,
    width: f64,
) {
    ctx.begin_path();
    ctx.rect(x, y, w, h);
    ctx.set_stroke_color(color);
    ctx.set_line_width(width);
    ctx.stroke();
}

/// Light text in the overlay's face.
pub struct TextStyle {
    pub font: Arc<Font>,
    pub color: Color,
}

impl TextStyle {
    pub fn new(font: Arc<Font>) -> Self {
        Self {
            font,
            color: Color::rgba(1.0, 1.0, 1.0, 0.95),
        }
    }

    fn select(&self, ctx: &mut dyn DrawCtx) {
        ctx.set_font(Arc::clone(&self.font));
        ctx.set_font_size(OVERLAY_TEXT_SIZE);
    }

    /// Width of `text` in logical points.
    pub fn width(&self, ctx: &mut dyn DrawCtx, text: &str) -> f64 {
        self.select(ctx);
        ctx.measure_text(text).map_or(0.0, |m| m.width)
    }

    /// Draw `text` with its baseline at `(x, y)`.
    pub fn draw(&self, ctx: &mut dyn DrawCtx, text: &str, x: f64, y: f64) {
        self.select(ctx);
        ctx.set_fill_color(self.color);
        ctx.fill_text(text, x, y);
    }

    /// A rounded dark pill holding `text`, its bottom-left at `(x, y)`, kept
    /// inside `area`. Returns the pill's size.
    pub fn pill(
        &self,
        ctx: &mut dyn DrawCtx,
        text: &str,
        x: f64,
        y: f64,
        area: (f64, f64),
    ) -> (f64, f64) {
        let w = self.width(ctx, text) + 2.0 * PILL_PAD_X;
        let x = x.min(area.0 - w).max(0.0);
        let y = y.min(area.1 - PILL_HEIGHT).max(0.0);
        ctx.begin_path();
        ctx.rounded_rect(x, y, w, PILL_HEIGHT, 5.0);
        ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.78));
        ctx.fill();
        self.draw(
            ctx,
            text,
            x + PILL_PAD_X,
            y + (PILL_HEIGHT - OVERLAY_TEXT_SIZE) / 2.0 + 2.0,
        );
        (w, PILL_HEIGHT)
    }

    /// Height of a [`TextStyle::pill`].
    pub fn pill_height() -> f64 {
        PILL_HEIGHT
    }
}

/// Dim everything in `area` (`(w, h)` from the origin) except `hole`.
pub fn dim_outside(ctx: &mut dyn DrawCtx, area: (f64, f64), hole: Option<LocalRect>) {
    let (aw, ah) = area;
    let Some((x, y, w, h)) = hole else {
        fill_rect(ctx, 0.0, 0.0, aw, ah, DIM);
        return;
    };
    // Four bands: below, above, left and right of the hole.
    fill_rect(ctx, 0.0, 0.0, aw, y, DIM);
    fill_rect(ctx, 0.0, y + h, aw, ah - (y + h), DIM);
    fill_rect(ctx, 0.0, y, x, h, DIM);
    fill_rect(ctx, x + w, y, aw - (x + w), h, DIM);
}

/// Outline the selection just outside its pixels: a dark line with a light
/// one inside it, each `hairline` (one physical pixel) wide.
pub fn outline_selection(ctx: &mut dyn DrawCtx, r: LocalRect, hairline: f64) {
    let (x, y, w, h) = r;
    stroke_rect(
        ctx,
        x - 1.5 * hairline,
        y - 1.5 * hairline,
        w + 3.0 * hairline,
        h + 3.0 * hairline,
        Color::rgba(0.0, 0.0, 0.0, 0.7),
        hairline,
    );
    stroke_rect(
        ctx,
        x - 0.5 * hairline,
        y - 0.5 * hairline,
        w + hairline,
        h + hairline,
        Color::rgba(1.0, 1.0, 1.0, 0.95),
        hairline,
    );
}

/// Full-width and full-height lines through `(cx, cy)`: one physical pixel
/// (`hairline`), dark, on a three-pixel light halo.
pub fn crosshair(ctx: &mut dyn DrawCtx, area: (f64, f64), cx: f64, cy: f64, hairline: f64) {
    let (aw, ah) = area;
    for (color, width) in [
        (Color::rgba(1.0, 1.0, 1.0, 0.55), 3.0 * hairline),
        (Color::rgba(0.0, 0.0, 0.0, 0.85), hairline),
    ] {
        ctx.begin_path();
        ctx.move_to(0.0, cy);
        ctx.line_to(aw, cy);
        ctx.move_to(cx, 0.0);
        ctx.line_to(cx, ah);
        ctx.set_stroke_color(color);
        ctx.set_line_width(width);
        ctx.stroke();
    }
}
