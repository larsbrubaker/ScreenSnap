//! Selection geometry: the marquee rectangle in capture pixels, and the
//! conversions between agg-gui's widget space and the capture.
//!
//! agg-gui hands widgets **logical points, Y up** from the bottom-left; the
//! capture is **physical pixels, Y down** from the top-left. The overlay widget
//! covers the whole capture, so the conversion is a scale and a flip.
//! Selections snap to whole pixels: a drag from pixel A to pixel B selects
//! both A and B (inclusive), the way Greenshot and macOS do.

/// A rectangle of capture pixels: `x, y` is the top-left pixel, Y down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PxRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PxRect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// The rectangle spanning pixels `a` and `b` inclusive, in either order.
    pub fn spanning(a: PxPoint, b: PxPoint) -> Self {
        let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
        let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
        Self::new(x0, y0, x1 - x0 + 1, y1 - y0 + 1)
    }

    /// Whether pixel `p` lies inside.
    pub fn contains(&self, p: PxPoint) -> bool {
        p.x >= self.x && p.y >= self.y && p.x < self.x + self.width && p.y < self.y + self.height
    }
}

/// One capture pixel, Y down.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PxPoint {
    pub x: u32,
    pub y: u32,
}

impl PxPoint {
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
}

/// Maps between the overlay widget's local space and the capture's pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaptureSpace {
    /// Capture size in physical pixels.
    pub width: u32,
    pub height: u32,
    /// Physical pixels per logical point.
    pub scale: f64,
}

impl CaptureSpace {
    pub fn new(width: u32, height: u32, scale: f64) -> Self {
        Self {
            width,
            height,
            scale,
        }
    }

    /// The capture's size in logical points — the overlay's size.
    pub fn logical_size(&self) -> (f64, f64) {
        (
            f64::from(self.width) / self.scale,
            f64::from(self.height) / self.scale,
        )
    }

    /// The pixel under widget-local point `(x, y)` (logical, Y up), clamped to
    /// the capture so a drag past the edge selects up to the edge.
    pub fn to_px(&self, x: f64, y: f64) -> PxPoint {
        let (_, h) = self.logical_size();
        let px = (x * self.scale).floor();
        let py = ((h - y) * self.scale).floor();
        PxPoint::new(clamp_px(px, self.width), clamp_px(py, self.height))
    }

    /// Widget-local rectangle `(x, y, w, h)` (logical, Y up, bottom-left
    /// origin) covering pixel rectangle `r` exactly.
    pub fn to_local(&self, r: PxRect) -> (f64, f64, f64, f64) {
        let (_, h) = self.logical_size();
        let s = self.scale;
        let x = f64::from(r.x) / s;
        let w = f64::from(r.width) / s;
        let hh = f64::from(r.height) / s;
        let y = h - f64::from(r.y + r.height) / s;
        (x, y, w, hh)
    }

    /// The whole capture as a selection.
    pub fn full(&self) -> PxRect {
        PxRect::new(0, 0, self.width, self.height)
    }
}

fn clamp_px(v: f64, size: u32) -> u32 {
    if !v.is_finite() || v <= 0.0 || size == 0 {
        0
    } else {
        (v as u64).min(u64::from(size - 1)) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spanning_is_inclusive_in_any_direction() {
        let r = PxRect::spanning(PxPoint::new(10, 20), PxPoint::new(4, 5));
        assert_eq!(r, PxRect::new(4, 5, 7, 16));
        let one = PxRect::spanning(PxPoint::new(3, 3), PxPoint::new(3, 3));
        assert_eq!(one, PxRect::new(3, 3, 1, 1));
        assert!(r.contains(PxPoint::new(10, 20)));
        assert!(!r.contains(PxPoint::new(11, 20)));
    }

    #[test]
    fn local_points_flip_and_scale_onto_pixels() {
        // A 2x display: 200x100 px is 100x50 points.
        let space = CaptureSpace::new(200, 100, 2.0);
        assert_eq!(space.logical_size(), (100.0, 50.0));
        // Bottom-left corner of the widget is the bottom-left pixel.
        assert_eq!(space.to_px(0.0, 0.0), PxPoint::new(0, 99));
        // Top-left.
        assert_eq!(space.to_px(0.0, 50.0), PxPoint::new(0, 0));
        assert_eq!(space.to_px(10.25, 40.0), PxPoint::new(20, 20));
        // Past the edges clamps.
        assert_eq!(space.to_px(-5.0, 99.0), PxPoint::new(0, 0));
        assert_eq!(space.to_px(500.0, -3.0), PxPoint::new(199, 99));
    }

    #[test]
    fn to_local_inverts_to_px() {
        let space = CaptureSpace::new(200, 100, 2.0);
        let r = PxRect::new(20, 10, 40, 30);
        let (x, y, w, h) = space.to_local(r);
        assert_eq!((x, y, w, h), (10.0, 30.0, 20.0, 15.0));
        // The rectangle's top-left local corner maps back to its top-left pixel.
        assert_eq!(space.to_px(x + 0.01, y + h - 0.01), PxPoint::new(20, 10));
        assert_eq!(space.full(), PxRect::new(0, 0, 200, 100));
    }
}
