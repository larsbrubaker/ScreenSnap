//! The overlay's magnifier ("loupe"): a square next to the pointer showing
//! [`LOUPE_PIXELS`]×[`LOUPE_PIXELS`] capture pixels blown up [`LOUPE_ZOOM`]
//! times, with a pixel grid, the centre pixel outlined and, under it, the
//! pointer's pixel position and colour (`docs/PLAN.md` §Overlay).
//!
//! The magnified image is built here on the CPU by nearest-neighbour
//! repetition ([`magnify`]) at a whole number of physical texels per capture
//! pixel ([`texel_factor`]), so it reaches the screen texel-for-pixel and stays
//! crisp — agg-gui's GPU backend samples images bilinearly, which would blur a
//! 15×15 image stretched by the backend. [`Loupe`] caches the result (as an
//! `Arc`, so the GPU texture is reused) until the pointer moves to another
//! pixel. The pure helpers ([`magnify`], [`sample`], [`hex_color`],
//! [`place`]) carry the pixel math and are unit-tested below; drawing lives in
//! [`Loupe::paint`]. The parent module ([`super`]) decides when to show it.

use std::sync::Arc;

use agg_gui::{Color, DrawCtx};

use crate::selection::PxPoint;

use super::paint::{fill_rect, stroke_rect, TextStyle, OVERLAY_TEXT_SIZE};

/// Capture pixels across (and down) the loupe. Odd, so one pixel is the centre.
pub const LOUPE_PIXELS: u32 = 15;
/// Logical points per capture pixel inside the loupe.
pub const LOUPE_ZOOM: f64 = 8.0;
/// Gap between the pointer and the loupe, in logical points.
pub const LOUPE_OFFSET: f64 = 24.0;
/// Height of the text panel under the loupe (two lines), in logical points.
const PANEL_HEIGHT: f64 = 40.0;
/// Gap between the magnified image and the text panel.
const PANEL_GAP: f64 = 4.0;
/// What the loupe shows for pixels beyond the capture's edge.
const OUTSIDE: [u8; 4] = [24, 24, 24, 255];

/// Physical texels per capture pixel for a capture at `scale` physical pixels
/// per point: [`LOUPE_ZOOM`] points' worth, rounded to a whole number so every
/// capture pixel becomes an identical block (at least 1).
pub fn texel_factor(scale: f64) -> u32 {
    let f = (LOUPE_ZOOM * scale).round();
    if f.is_finite() && f >= 1.0 {
        f as u32
    } else {
        1
    }
}

/// The RGBA of capture pixel `p`, or `None` outside the `width`×`height`
/// image `pixels` (RGBA8, top row first).
pub fn sample(pixels: &[u8], width: u32, height: u32, p: PxPoint) -> Option<[u8; 4]> {
    if p.x >= width || p.y >= height {
        return None;
    }
    let i = (p.y as usize * width as usize + p.x as usize) * 4;
    let px = pixels.get(i..i + 4)?;
    Some([px[0], px[1], px[2], px[3]])
}

/// `#RRGGBB` for a colour (alpha ignored).
pub fn hex_color(rgba: [u8; 4]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgba[0], rgba[1], rgba[2])
}

/// The magnified neighbourhood of `center`: `n`×`n` capture pixels (centred
/// on `center`, `n` odd) each repeated into a `factor`×`factor` block, as an
/// RGBA8 image `n * factor` texels square, top row first. Pixels past the
/// capture's edge are drawn dark and opaque.
pub fn magnify(
    pixels: &[u8],
    width: u32,
    height: u32,
    center: PxPoint,
    n: u32,
    factor: u32,
) -> Vec<u8> {
    let side = (n * factor) as usize;
    let half = i64::from(n / 2);
    let mut out = Vec::with_capacity(side * side * 4);
    for row in 0..n {
        let sy = i64::from(center.y) - half + i64::from(row);
        let mut line = Vec::with_capacity(side * 4);
        for col in 0..n {
            let sx = i64::from(center.x) - half + i64::from(col);
            let rgba = if sx < 0 || sy < 0 {
                OUTSIDE
            } else {
                sample(pixels, width, height, PxPoint::new(sx as u32, sy as u32))
                    .map(|c| [c[0], c[1], c[2], 255])
                    .unwrap_or(OUTSIDE)
            };
            for _ in 0..factor {
                line.extend_from_slice(&rgba);
            }
        }
        for _ in 0..factor {
            out.extend_from_slice(&line);
        }
    }
    out
}

/// Where a `size` box (logical, Y up) goes for a pointer at `pointer` inside
/// an `area` (the widget, origin bottom-left): below and to the right of the
/// pointer, flipped to the left and / or above when it would leave the area,
/// and finally clamped inside it. Returns the box's bottom-left corner.
pub fn place(pointer: (f64, f64), size: (f64, f64), area: (f64, f64)) -> (f64, f64) {
    let (px, py) = pointer;
    let (w, h) = size;
    let (aw, ah) = area;
    let mut x = px + LOUPE_OFFSET;
    if x + w > aw {
        x = px - LOUPE_OFFSET - w;
    }
    let mut y = py - LOUPE_OFFSET - h;
    if y < 0.0 {
        y = py + LOUPE_OFFSET;
    }
    (x.min(aw - w).max(0.0), y.min(ah - h).max(0.0))
}

/// The cached magnified image for one pointer pixel.
struct Magnified {
    center: PxPoint,
    factor: u32,
    image: Arc<Vec<u8>>,
}

/// Where the loupe looks.
#[derive(Clone, Copy, Debug)]
pub struct Spot {
    /// The pointer (local, logical, Y up); the loupe sits beside it.
    pub pointer: (f64, f64),
    /// The capture pixel under it, magnified in the middle.
    pub center: PxPoint,
}

/// The loupe's cache and drawing. See the module docs.
#[derive(Default)]
pub struct Loupe {
    cached: Option<Magnified>,
}

/// What the loupe magnifies: the frozen screen.
pub struct Source<'a> {
    pub pixels: &'a Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    /// Physical pixels per logical point of the capture.
    pub scale: f64,
}

impl Loupe {
    /// The loupe's whole footprint (image plus text panel), logical points.
    pub fn size(scale: f64) -> (f64, f64) {
        let side = Self::image_side(scale);
        (side, side + PANEL_GAP + PANEL_HEIGHT)
    }

    /// The magnified image's side in logical points.
    fn image_side(scale: f64) -> f64 {
        f64::from(LOUPE_PIXELS * texel_factor(scale)) / scale
    }

    /// The magnified image around `center`, rebuilt only when the pointer
    /// moved to another pixel.
    fn image(&mut self, src: &Source<'_>, center: PxPoint) -> Arc<Vec<u8>> {
        let factor = texel_factor(src.scale);
        if let Some(m) = &self.cached {
            if m.center == center && m.factor == factor {
                return Arc::clone(&m.image);
            }
        }
        let image = Arc::new(magnify(
            src.pixels,
            src.width,
            src.height,
            center,
            LOUPE_PIXELS,
            factor,
        ));
        self.cached = Some(Magnified {
            center,
            factor,
            image: Arc::clone(&image),
        });
        image
    }

    /// Draw the loupe for `spot`, inside an `area`-sized widget. `hairline` is
    /// one physical pixel in logical points.
    pub fn paint(
        &mut self,
        ctx: &mut dyn DrawCtx,
        src: &Source<'_>,
        text: &TextStyle,
        spot: Spot,
        area: (f64, f64),
        hairline: f64,
    ) {
        let Spot { pointer, center } = spot;
        let (w, h) = Self::size(src.scale);
        let (x, y) = place(pointer, (w, h), area);
        let side = Self::image_side(src.scale);
        let img_y = y + PANEL_GAP + PANEL_HEIGHT;
        let factor = texel_factor(src.scale);
        let texels = LOUPE_PIXELS * factor;
        let image = self.image(src, center);

        // Frame: a dark rim with a light line inside, readable on any screen.
        fill_rect(
            ctx,
            x - 2.0 * hairline,
            img_y - 2.0 * hairline,
            side + 4.0 * hairline,
            side + 4.0 * hairline,
            Color::rgba(0.0, 0.0, 0.0, 0.85),
        );
        ctx.draw_image_rgba_arc(&image, texels, texels, x, img_y, side, side);

        // Pixel grid.
        let cell = side / f64::from(LOUPE_PIXELS);
        ctx.begin_path();
        for i in 1..LOUPE_PIXELS {
            let o = f64::from(i) * cell;
            ctx.move_to(x + o, img_y);
            ctx.line_to(x + o, img_y + side);
            ctx.move_to(x, img_y + o);
            ctx.line_to(x + side, img_y + o);
        }
        ctx.set_stroke_color(Color::rgba(0.0, 0.0, 0.0, 0.22));
        ctx.set_line_width(hairline);
        ctx.stroke();

        // Centre pixel: dark outside, light inside.
        let mid = f64::from(LOUPE_PIXELS / 2) * cell;
        stroke_rect(
            ctx,
            x + mid - hairline,
            img_y + mid - hairline,
            cell + 2.0 * hairline,
            cell + 2.0 * hairline,
            Color::rgba(0.0, 0.0, 0.0, 0.9),
            hairline * 2.0,
        );
        stroke_rect(
            ctx,
            x + mid,
            img_y + mid,
            cell,
            cell,
            Color::rgba(1.0, 1.0, 1.0, 0.95),
            hairline,
        );
        stroke_rect(
            ctx,
            x - hairline,
            img_y - hairline,
            side + 2.0 * hairline,
            side + 2.0 * hairline,
            Color::rgba(1.0, 1.0, 1.0, 0.7),
            hairline,
        );

        // Text panel: position, then swatch + hex.
        let rgba = sample(src.pixels, src.width, src.height, center).unwrap_or(OUTSIDE);
        ctx.begin_path();
        ctx.rounded_rect(x, y, side, PANEL_HEIGHT, 4.0);
        ctx.set_fill_color(Color::rgba(0.0, 0.0, 0.0, 0.78));
        ctx.fill();
        let line_h = PANEL_HEIGHT / 2.0;
        text.draw(
            ctx,
            &format!("{}, {}", center.x, center.y),
            x + 8.0,
            y + line_h + (line_h - OVERLAY_TEXT_SIZE) / 2.0 + 2.0,
        );
        let swatch = OVERLAY_TEXT_SIZE - 2.0;
        let swatch_y = y + (line_h - swatch) / 2.0 + 1.0;
        fill_rect(
            ctx,
            x + 8.0,
            swatch_y,
            swatch,
            swatch,
            Color::from_rgb8(rgba[0], rgba[1], rgba[2]),
        );
        stroke_rect(
            ctx,
            x + 8.0,
            swatch_y,
            swatch,
            swatch,
            Color::rgba(1.0, 1.0, 1.0, 0.8),
            hairline,
        );
        text.draw(
            ctx,
            &hex_color(rgba),
            x + 14.0 + swatch,
            y + (line_h - OVERLAY_TEXT_SIZE) / 2.0 + 3.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w`×`h` image whose pixel `(x, y)` is `[x, y, 7, 255]`.
    fn ramp(w: u32, h: u32) -> Vec<u8> {
        let mut v = Vec::new();
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&[x as u8, y as u8, 7, 255]);
            }
        }
        v
    }

    fn texel(img: &[u8], side: u32, tx: u32, ty: u32) -> [u8; 4] {
        let i = ((ty * side + tx) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    #[test]
    fn texel_factor_is_whole_and_tracks_scale() {
        assert_eq!(texel_factor(1.0), 8);
        assert_eq!(texel_factor(2.0), 16);
        assert_eq!(texel_factor(1.5), 12);
        assert_eq!(texel_factor(0.01), 1);
        assert_eq!(texel_factor(f64::NAN), 1);
    }

    #[test]
    fn magnify_repeats_each_pixel_into_a_block_around_the_centre() {
        let pixels = ramp(40, 30);
        let img = magnify(&pixels, 40, 30, PxPoint::new(20, 10), 15, 3);
        let side = 15 * 3;
        assert_eq!(img.len(), (side * side * 4) as usize);
        // The centre block (cell 7,7) is the centre pixel, all 3x3 texels.
        for ty in 21..24 {
            for tx in 21..24 {
                assert_eq!(texel(&img, side, tx, ty), [20, 10, 7, 255]);
            }
        }
        // Top-left cell is pixel (13, 3); its neighbour to the right (14, 3).
        assert_eq!(texel(&img, side, 0, 0), [13, 3, 7, 255]);
        assert_eq!(texel(&img, side, 2, 2), [13, 3, 7, 255]);
        assert_eq!(texel(&img, side, 3, 0), [14, 3, 7, 255]);
        // Bottom-right cell is pixel (27, 17).
        assert_eq!(texel(&img, side, side - 1, side - 1), [27, 17, 7, 255]);
    }

    #[test]
    fn magnify_shades_pixels_past_the_edges() {
        let pixels = ramp(10, 10);
        let img = magnify(&pixels, 10, 10, PxPoint::new(0, 9), 15, 1);
        // Cell (7,7) is the corner pixel itself.
        assert_eq!(texel(&img, 15, 7, 7), [0, 9, 7, 255]);
        // Left of x = 0 and below y = 9 are outside.
        assert_eq!(texel(&img, 15, 6, 7), OUTSIDE);
        assert_eq!(texel(&img, 15, 7, 8), OUTSIDE);
        assert_eq!(texel(&img, 15, 8, 6), [1, 8, 7, 255]);
    }

    #[test]
    fn magnify_shows_translucent_pixels_opaque() {
        let pixels = vec![200, 100, 50, 0];
        let img = magnify(&pixels, 1, 1, PxPoint::new(0, 0), 1, 2);
        assert_eq!(img, [200, 100, 50, 255].repeat(4));
    }

    #[test]
    fn sample_and_hex() {
        let pixels = ramp(4, 4);
        assert_eq!(
            sample(&pixels, 4, 4, PxPoint::new(3, 2)),
            Some([3, 2, 7, 255])
        );
        assert_eq!(sample(&pixels, 4, 4, PxPoint::new(4, 0)), None);
        assert_eq!(hex_color([255, 10, 0xAB, 1]), "#FF0AAB");
    }

    #[test]
    fn place_flips_to_stay_inside() {
        let area = (1000.0, 800.0);
        let size = (120.0, 160.0);
        // Room below-right.
        assert_eq!(place((100.0, 500.0), size, area), (124.0, 316.0));
        // Near the right edge: flips left.
        assert_eq!(place((950.0, 500.0), size, area), (806.0, 316.0));
        // Near the bottom (Y up: small y): flips above.
        assert_eq!(place((100.0, 50.0), size, area), (124.0, 74.0));
        // Both, in the bottom-right corner.
        assert_eq!(place((990.0, 10.0), size, area), (846.0, 34.0));
        // An area smaller than the box clamps to its origin.
        assert_eq!(place((5.0, 5.0), size, (50.0, 50.0)), (0.0, 0.0));
    }
}
