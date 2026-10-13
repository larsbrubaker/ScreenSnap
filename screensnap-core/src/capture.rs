//! A frozen screen: the pixels of one monitor at the moment the hotkey fired,
//! plus the mouse pointer as it looked then.
//!
//! The native shell produces a [`Capture`] (`screensnap-native/src/capture`),
//! the overlay ([`crate::ui::overlay`]) shows it and lets the user marquee a
//! region, and [`crate::output`] crops and delivers that region. Everything
//! here is in **physical pixels, top row first, Y down** — the image's own
//! coordinates — so a selection maps onto the pixels exactly on any display
//! scale.

use std::sync::Arc;

/// An RGBA8 image (straight alpha), top row first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    /// `width * height * 4` bytes.
    pub pixels: Vec<u8>,
}

impl RgbaImage {
    /// An image from raw bytes, or `None` when the length does not match.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        (pixels.len() == width as usize * height as usize * 4).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// The RGBA of pixel `(x, y)`, or `None` outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        Some([
            self.pixels[i],
            self.pixels[i + 1],
            self.pixels[i + 2],
            self.pixels[i + 3],
        ])
    }
}

/// The mouse pointer at capture time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CursorSnapshot {
    /// The pointer image at the capture's pixel scale.
    pub image: RgbaImage,
    /// Where the image's top-left corner falls in the capture, in capture
    /// pixels (already offset by the hotspot). May be negative or past the
    /// edge when the pointer is near one.
    pub x: i32,
    pub y: i32,
}

/// One monitor, frozen. See the module docs.
#[derive(Clone, Debug)]
pub struct Capture {
    /// The screen without the pointer. Shared so the GPU backend keeps one
    /// texture for it (`draw_image_rgba_arc`).
    pub screen: Arc<Vec<u8>>,
    pub width: u32,
    pub height: u32,
    /// Physical pixels per logical point (2.0 on a Retina display).
    pub scale: f64,
    pub cursor: Option<CursorSnapshot>,
}

impl Capture {
    /// A capture of `screen` (RGBA8, `width * height * 4` bytes), or `None`
    /// when the sizes disagree or the scale is not a positive number.
    pub fn new(screen: RgbaImage, scale: f64, cursor: Option<CursorSnapshot>) -> Option<Self> {
        if !(scale.is_finite() && scale > 0.0) || screen.width == 0 || screen.height == 0 {
            return None;
        }
        Some(Self {
            width: screen.width,
            height: screen.height,
            screen: Arc::new(screen.pixels),
            scale,
            cursor,
        })
    }

    /// The image the overlay shows and the selection is cut from: the screen
    /// with the pointer drawn in when `include_cursor` is set (and there is
    /// one), otherwise the plain screen. The plain case shares the buffer.
    pub fn composed(&self, include_cursor: bool) -> Arc<Vec<u8>> {
        match (&self.cursor, include_cursor) {
            (Some(cursor), true) => {
                let mut pixels = self.screen.as_ref().clone();
                blend_over(&mut pixels, self.width, self.height, cursor);
                Arc::new(pixels)
            }
            _ => Arc::clone(&self.screen),
        }
    }
}

/// Source-over blend `cursor` (straight alpha) onto `dst` (opaque RGBA8).
fn blend_over(dst: &mut [u8], width: u32, height: u32, cursor: &CursorSnapshot) {
    let img = &cursor.image;
    for cy in 0..img.height {
        let y = cursor.y + cy as i32;
        if y < 0 || y >= height as i32 {
            continue;
        }
        for cx in 0..img.width {
            let x = cursor.x + cx as i32;
            if x < 0 || x >= width as i32 {
                continue;
            }
            let s = (cy as usize * img.width as usize + cx as usize) * 4;
            let a = u32::from(img.pixels[s + 3]);
            if a == 0 {
                continue;
            }
            let d = (y as usize * width as usize + x as usize) * 4;
            for c in 0..3 {
                let src = u32::from(img.pixels[s + c]);
                let old = u32::from(dst[d + c]);
                dst[d + c] = ((src * a + old * (255 - a) + 127) / 255) as u8;
            }
            let old_a = u32::from(dst[d + 3]);
            dst[d + 3] = (a + (old_a * (255 - a) + 127) / 255).min(255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, rgba: [u8; 4]) -> RgbaImage {
        RgbaImage::new(w, h, rgba.repeat((w * h) as usize)).unwrap()
    }

    #[test]
    fn composed_without_cursor_shares_the_screen() {
        let cap = Capture::new(solid(4, 4, [10, 20, 30, 255]), 2.0, None).unwrap();
        assert!(Arc::ptr_eq(&cap.composed(true), &cap.screen));
    }

    #[test]
    fn cursor_is_blended_and_clipped_at_edges() {
        let mut cursor_px = solid(2, 2, [255, 0, 0, 255]);
        // Top-right pixel of the pointer is transparent.
        cursor_px.pixels[4..8].copy_from_slice(&[0, 0, 0, 0]);
        let cursor = CursorSnapshot {
            image: cursor_px,
            x: -1,
            y: 2,
        };
        let cap = Capture::new(solid(4, 4, [0, 0, 255, 255]), 1.0, Some(cursor)).unwrap();
        let out = RgbaImage::new(4, 4, cap.composed(true).as_ref().clone()).unwrap();
        // Pointer (1,1) lands on (0,3): red. Pointer (1,0) -> (0,2): transparent, stays blue.
        assert_eq!(out.pixel(0, 3), Some([255, 0, 0, 255]));
        assert_eq!(out.pixel(0, 2), Some([0, 0, 255, 255]));
        assert_eq!(out.pixel(1, 3), Some([0, 0, 255, 255]));
        // Excluding the pointer leaves the screen untouched.
        assert!(Arc::ptr_eq(&cap.composed(false), &cap.screen));
    }

    #[test]
    fn rejects_bad_input() {
        assert!(RgbaImage::new(2, 2, vec![0; 15]).is_none());
        assert!(Capture::new(solid(2, 2, [0; 4]), 0.0, None).is_none());
        assert!(Capture::new(solid(2, 2, [0; 4]), f64::NAN, None).is_none());
    }
}
