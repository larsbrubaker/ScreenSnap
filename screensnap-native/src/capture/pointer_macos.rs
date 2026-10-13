//! The mouse pointer on macOS: where it is (CoreGraphics) and what it looks
//! like (AppKit's `NSCursor.currentSystemCursor`, whichever app set it).
//!
//! - [`location`] reads `CGEventGetLocation` of a fresh event: global display
//!   points, origin at the top-left of the primary display, Y down — the same
//!   space `xcap` / `CGDisplayBounds` describe monitors in. (`NSEvent
//!   mouseLocation` would be Cocoa's bottom-left space and need flipping.)
//! - [`image`] rasterises the cursor's `NSImage` into an RGBA buffer at the
//!   capture's scale (so a Retina capture gets the 2× pointer artwork) by
//!   drawing it through an `NSGraphicsContext` wrapped around a
//!   `CGBitmapContext` over our own buffer, then un-premultiplies it.
//!
//! Either returning `None` just means the capture goes ahead without the
//! pointer. Apple has deprecated `currentSystemCursor` in favour of
//! ScreenCaptureKit's cursor option; it still works on current macOS.

use std::ffi::c_void;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSCursor, NSGraphicsContext};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGEvent, CGImageAlphaInfo, CGImageByteOrderInfo,
};
use screensnap_core::capture::RgbaImage;

use super::PointerImage;

/// The largest pointer image accepted, in pixels per side (macOS pointers are
/// at most 64 pt even with accessibility enlargement; this is a sanity cap).
const MAX_SIDE: f64 = 1024.0;

/// The pointer's global position in display points (see the module docs).
pub fn location() -> Option<(f64, f64)> {
    let event = CGEvent::new(None)?;
    let p: CGPoint = CGEvent::location(Some(&event));
    (p.x.is_finite() && p.y.is_finite()).then_some((p.x, p.y))
}

/// The current system pointer at `scale` pixels per point.
pub fn image(scale: f64) -> Option<PointerImage> {
    // NSCursor and NSGraphicsContext are main-thread AppKit; the shell calls
    // this from the event loop's idle hook, which is the main thread.
    MainThreadMarker::new()?;
    #[allow(deprecated)] // see the module docs
    let cursor = NSCursor::currentSystemCursor()?;
    let ns_image = cursor.image();
    let size = ns_image.size();
    let hot = cursor.hotSpot();
    let (pw, ph) = ((size.width * scale).ceil(), (size.height * scale).ceil());
    if !(pw >= 1.0 && ph >= 1.0 && pw <= MAX_SIDE && ph <= MAX_SIDE) {
        return None;
    }
    let (pw, ph) = (pw as usize, ph as usize);
    let mut pixels = vec![0u8; pw * ph * 4];
    {
        let space = CGColorSpace::new_device_rgb()?;
        let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
        // SAFETY: `pixels` is a live, writable buffer of exactly
        // `ph` rows of `pw * 4` bytes (8-bit RGBA), as described to CoreGraphics
        // here, and it outlives the context: the context is dropped at the end
        // of this block, before `pixels` is read or moved.
        let ctx = unsafe {
            CGBitmapContextCreate(
                pixels.as_mut_ptr().cast::<c_void>(),
                pw,
                ph,
                8,
                pw * 4,
                Some(&space),
                info,
            )
        }?;
        draw_cursor(&ctx, &ns_image, size, scale);
    }
    unpremultiply(&mut pixels);
    Some(PointerImage {
        image: RgbaImage::new(pw as u32, ph as u32, pixels)?,
        // `hotSpot` is in points from the image's top-left corner.
        hotspot: (hot.x * scale, hot.y * scale),
    })
}

/// Draw `image` (`size` points) into `ctx`, scaled to pixels. CoreGraphics'
/// origin is bottom-left, but a bitmap context's memory holds the top row
/// first, so drawing unflipped yields a top-row-first buffer.
fn draw_cursor(ctx: &CGContext, image: &objc2_app_kit::NSImage, size: CGSize, scale: f64) {
    CGContext::scale_ctm(Some(ctx), scale, scale);
    let graphics = NSGraphicsContext::graphicsContextWithCGContext_flipped(ctx, false);
    NSGraphicsContext::saveGraphicsState_class();
    NSGraphicsContext::setCurrentContext(Some(&graphics));
    image.drawInRect(CGRect::new(CGPoint::new(0.0, 0.0), size));
    NSGraphicsContext::restoreGraphicsState_class();
}

/// Premultiplied RGBA8 to straight alpha, in place.
fn unpremultiply(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        let a = u32::from(px[3]);
        if a == 0 || a == 255 {
            continue;
        }
        for c in &mut px[..3] {
            *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpremultiply_restores_straight_colour() {
        let mut px = [64, 0, 128, 128, 10, 20, 30, 255, 0, 0, 0, 0];
        unpremultiply(&mut px);
        assert_eq!(px, [128, 0, 255, 128, 10, 20, 30, 255, 0, 0, 0, 0]);
    }
}
