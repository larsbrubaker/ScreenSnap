//! The Screen Snap mark, drawn in code at any size: two crop corners framing a
//! translucent selection — the marquee the app is about.
//!
//! Two variants:
//! - [`rgba`] — the full-colour app icon (a blue rounded square with the white
//!   mark), for the window / taskbar icon and the macOS app bundle;
//! - [`template_rgba`] — the bare mark in black with alpha only, for the macOS
//!   menu bar, which recolours "template" images to suit a light or dark bar.
//!
//! Both are straight-alpha RGBA8, top row first. Shapes are described in unit
//! space (0..1, Y down) and rasterised with 4×4 supersampling, so every size is
//! cleanly antialiased. `screensnap-native` (tray icon, window icon) and
//! `scripts/bundle-mac.sh` (the `.icns`, via the `screensnap --write-icon`
//! hook) are the users.

/// An axis-aligned rectangle in unit space with an optional corner radius.
#[derive(Clone, Copy)]
struct Shape {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    radius: f32,
}

impl Shape {
    const fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Self {
        Self {
            x0,
            y0,
            x1,
            y1,
            radius: 0.0,
        }
    }

    /// Whether the unit-space point lies inside.
    fn contains(&self, x: f32, y: f32) -> bool {
        if x < self.x0 || x > self.x1 || y < self.y0 || y > self.y1 {
            return false;
        }
        let r = self.radius;
        if r <= 0.0 {
            return true;
        }
        // Distance into the nearest corner square; only there can the point miss.
        let cx = if x < self.x0 + r {
            self.x0 + r
        } else if x > self.x1 - r {
            self.x1 - r
        } else {
            return true;
        };
        let cy = if y < self.y0 + r {
            self.y0 + r
        } else if y > self.y1 - r {
            self.y1 - r
        } else {
            return true;
        };
        let (dx, dy) = (x - cx, y - cy);
        dx * dx + dy * dy <= r * r
    }
}

/// Samples per pixel side.
const SUPERSAMPLE: u32 = 4;

/// Fraction of pixel `(px, py)` of a `size`-pixel image that `shapes` cover
/// (their union).
fn coverage(shapes: &[Shape], px: u32, py: u32, size: u32) -> f32 {
    let mut hits = 0u32;
    let step = 1.0 / (SUPERSAMPLE as f32 * size as f32);
    for sy in 0..SUPERSAMPLE {
        for sx in 0..SUPERSAMPLE {
            let x = (px * SUPERSAMPLE + sx) as f32 * step + step * 0.5;
            let y = (py * SUPERSAMPLE + sy) as f32 * step + step * 0.5;
            if shapes.iter().any(|s| s.contains(x, y)) {
                hits += 1;
            }
        }
    }
    hits as f32 / (SUPERSAMPLE * SUPERSAMPLE) as f32
}

/// A premultiplied RGBA pixel being composed.
#[derive(Clone, Copy, Default)]
struct Px([f32; 4]);

impl Px {
    /// Source-over a straight `rgb` at `alpha`.
    fn over(&mut self, rgb: [f32; 3], alpha: f32) {
        let keep = 1.0 - alpha;
        for (c, s) in self.0.iter_mut().zip(rgb) {
            *c = s * alpha + *c * keep;
        }
        self.0[3] = alpha + self.0[3] * keep;
    }

    /// Straight-alpha RGBA8.
    fn to_rgba8(self) -> [u8; 4] {
        let a = self.0[3];
        let to8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        if a <= 0.0 {
            return [0, 0, 0, 0];
        }
        [
            to8(self.0[0] / a),
            to8(self.0[1] / a),
            to8(self.0[2] / a),
            to8(a),
        ]
    }
}

/// The two crop corners (top-left and bottom-right "L"s) between `lo` and
/// `hi`, arms `arm` long and `bar` thick.
fn crop_corners(lo: f32, hi: f32, arm: f32, bar: f32) -> [Shape; 4] {
    [
        Shape::rect(lo, lo, lo + arm, lo + bar),
        Shape::rect(lo, lo, lo + bar, lo + arm),
        Shape::rect(hi - arm, hi - bar, hi, hi),
        Shape::rect(hi - bar, hi - arm, hi, hi),
    ]
}

/// The app icon's background tile, inset like a macOS app icon.
const TILE: Shape = Shape {
    x0: 0.08,
    y0: 0.08,
    x1: 0.92,
    y1: 0.92,
    radius: 0.19,
};
/// Tile gradient, top to bottom.
const TILE_TOP: [f32; 3] = [0.33, 0.58, 0.98];
const TILE_BOTTOM: [f32; 3] = [0.13, 0.31, 0.80];
const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
const BLACK: [f32; 3] = [0.0, 0.0, 0.0];

/// The full-colour app icon, `size`×`size` RGBA8 (straight alpha). See the
/// module docs.
pub fn rgba(size: u32) -> Vec<u8> {
    let corners = crop_corners(0.22, 0.78, 0.30, 0.075);
    let selection = [Shape::rect(0.335, 0.335, 0.665, 0.665)];
    render(size, |px, py, out| {
        let tile = coverage(&[TILE], px, py, size);
        if tile > 0.0 {
            let t = (py as f32 + 0.5) / size as f32;
            let rgb = [0, 1, 2].map(|i| TILE_TOP[i] + (TILE_BOTTOM[i] - TILE_TOP[i]) * t);
            out.over(rgb, tile);
        }
        out.over(WHITE, 0.35 * coverage(&selection, px, py, size));
        out.over(WHITE, coverage(&corners, px, py, size));
    })
}

/// The menu-bar mark: black, alpha only (a macOS "template" image), centred with
/// the menu-bar margin built in (tray-icon draws it 22 pt tall; the mark is
/// ~16 pt, as the macOS guidelines suggest). See the module docs.
pub fn template_rgba(size: u32) -> Vec<u8> {
    let corners = crop_corners(0.14, 0.86, 0.36, 0.10);
    let selection = [Shape::rect(0.36, 0.36, 0.64, 0.64)];
    render(size, |px, py, out| {
        out.over(BLACK, 0.55 * coverage(&selection, px, py, size));
        out.over(BLACK, coverage(&corners, px, py, size));
    })
}

/// Run `shade` for every pixel of a `size`×`size` image.
fn render(size: u32, mut shade: impl FnMut(u32, u32, &mut Px)) -> Vec<u8> {
    let mut out = Vec::with_capacity(size as usize * size as usize * 4);
    for py in 0..size {
        for px in 0..size {
            let mut p = Px::default();
            shade(px, py, &mut p);
            out.extend_from_slice(&p.to_rgba8());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(img: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * size + x) * 4) as usize;
        [img[i], img[i + 1], img[i + 2], img[i + 3]]
    }

    #[test]
    fn every_size_has_the_right_length() {
        for size in [0, 1, 16, 22, 44, 256] {
            let n = size as usize * size as usize * 4;
            assert_eq!(rgba(size).len(), n);
            assert_eq!(template_rgba(size).len(), n);
        }
    }

    #[test]
    fn app_icon_is_a_blue_tile_with_a_white_mark() {
        let s = 128;
        let img = rgba(s);
        // Outside the tile: fully transparent.
        assert_eq!(pixel(&img, s, 0, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(&img, s, s - 1, s - 1)[3], 0);
        // Tile background (between the mark and the edge): opaque blue.
        let bg = pixel(&img, s, s / 2, (0.15 * s as f32) as u32);
        assert_eq!(bg[3], 255);
        assert!(bg[2] > bg[0] + 60, "blue tile, got {bg:?}");
        // On the top-left crop corner's horizontal arm: opaque white.
        let arm = pixel(&img, s, (0.35 * s as f32) as u32, (0.255 * s as f32) as u32);
        assert_eq!(arm, [255, 255, 255, 255]);
        // The selection in the middle is lighter than the tile but not white.
        let mid = pixel(&img, s, s / 2, s / 2);
        assert_eq!(mid[3], 255);
        assert!(
            mid[0] > 120 && mid[0] < 250,
            "translucent selection, got {mid:?}"
        );
    }

    #[test]
    fn template_is_black_with_alpha_only() {
        let s = 44;
        let img = template_rgba(s);
        for px in img.chunks_exact(4) {
            assert_eq!(&px[..3], &[0, 0, 0], "a template image carries alpha only");
        }
        // Background between the corners is clear.
        assert_eq!(pixel(&img, s, s - 4, 4)[3], 0);
        // The top-left arm is solid, the selection partial.
        assert_eq!(pixel(&img, s, 12, 8)[3], 255);
        let mid = pixel(&img, s, s / 2, s / 2)[3];
        assert!(mid > 80 && mid < 200, "selection alpha {mid}");
    }

    #[test]
    fn edges_are_antialiased_and_output_is_deterministic() {
        let s = 64;
        let img = rgba(s);
        assert_eq!(img, rgba(s));
        // Some pixel along the tile's rounded corner is partially covered.
        let partial = (0..s / 3).any(|i| {
            let a = pixel(&img, s, i, i)[3];
            a > 0 && a < 255
        });
        assert!(partial, "rounded corners are antialiased");
    }
}
