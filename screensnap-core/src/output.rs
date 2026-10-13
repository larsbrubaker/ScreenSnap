//! Turning a selection into results: crop the frozen screen, encode a PNG,
//! name the file, and deliver to the clipboard and / or disk as the settings
//! say.
//!
//! [`crate::state::AppState::finish_capture`] queues a [`Delivery`]; the
//! native shell runs it on its next idle pass, after the overlay window has
//! gone, so the screen is back in the user's hands before the PNG is written.

use std::io;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};

use crate::capture::RgbaImage;
use crate::platform::SnapPlatform;
use crate::selection::PxRect;
use crate::settings::Settings;

/// Copy `rect` out of an RGBA8 image `width` pixels wide. The rectangle is
/// clipped to the image; an empty result is a 0×0 image.
pub fn crop(pixels: &[u8], width: u32, height: u32, rect: PxRect) -> RgbaImage {
    let x0 = rect.x.min(width);
    let y0 = rect.y.min(height);
    let x1 = rect.x.saturating_add(rect.width).min(width);
    let y1 = rect.y.saturating_add(rect.height).min(height);
    let (w, h) = (x1 - x0, y1 - y0);
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for y in y0..y1 {
        let start = (y as usize * width as usize + x0 as usize) * 4;
        out.extend_from_slice(&pixels[start..start + w as usize * 4]);
    }
    RgbaImage {
        width: w,
        height: h,
        pixels: out,
    }
}

/// Encode `image` as a PNG. `scale` (physical pixels per point) is written as
/// the pixel density — 144 dpi for a 2× capture — so viewers that honour it
/// show a Retina capture at the size it had on screen.
pub fn encode_png(image: &RgbaImage, scale: f64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, image.width, image.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        let ppm = (72.0 * scale / 0.0254).round() as u32;
        encoder.set_pixel_dims(Some(png::PixelDimensions {
            xppu: ppm,
            yppu: ppm,
            unit: png::Unit::Meter,
        }));
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(&image.pixels)
            .map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}

/// `"Screen Snap 2026-10-12 at 18.07.33.png"` — sortable, and free of the
/// characters Finder and Explorer reject.
pub fn file_name(now: DateTime<Local>) -> String {
    format!("Screen Snap {}.png", now.format("%Y-%m-%d at %H.%M.%S"))
}

/// `folder/name`, or `folder/name (2).png`, `(3)` … when taken.
pub fn unique_path(folder: &Path, name: &str) -> PathBuf {
    let first = folder.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s, format!(".{e}")),
        None => (name, String::new()),
    };
    (2u32..)
        .map(|n| folder.join(format!("{stem} ({n}){ext}")))
        .find(|p| !p.exists())
        .unwrap_or(first)
}

/// Write `png` into `folder` (created when missing) under a fresh name.
pub fn save_png(folder: &Path, png: &[u8], now: DateTime<Local>) -> io::Result<PathBuf> {
    std::fs::create_dir_all(folder)?;
    let path = unique_path(folder, &file_name(now));
    std::fs::write(&path, png)?;
    Ok(path)
}

/// A cropped selection waiting to be delivered.
#[derive(Clone, Debug)]
pub struct Delivery {
    pub image: RgbaImage,
    pub scale: f64,
}

/// What happened to a delivery, for the status line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeliveryReport {
    /// `Some(Ok)` when copied, `Some(Err)` when copying failed, `None` when
    /// the settings did not ask for it.
    pub copied: Option<Result<(), String>>,
    pub saved: Option<Result<PathBuf, String>>,
}

impl DeliveryReport {
    /// One line for people: "Copied to the clipboard and saved as …".
    pub fn message(&self) -> String {
        let mut parts = Vec::new();
        let mut errors = Vec::new();
        match &self.copied {
            Some(Ok(())) => parts.push("copied to the clipboard".to_string()),
            Some(Err(e)) => errors.push(format!("could not copy to the clipboard ({e})")),
            None => {}
        }
        match &self.saved {
            Some(Ok(path)) => parts.push(format!("saved as {}", path.display())),
            Some(Err(e)) => errors.push(format!("could not save ({e})")),
            None => {}
        }
        let mut text = if parts.is_empty() {
            String::new()
        } else {
            format!("Capture {}", parts.join(" and "))
        };
        if !errors.is_empty() {
            if !text.is_empty() {
                text.push_str("; ");
                text.push_str(&errors.join("; "));
            } else {
                let joined = errors.join("; ");
                let mut chars = joined.chars();
                text = chars
                    .next()
                    .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
                    .unwrap_or_default();
            }
        }
        if text.is_empty() {
            "Capture discarded: copying and saving are both turned off".to_string()
        } else {
            text.push('.');
            text
        }
    }

    pub fn is_ok(&self) -> bool {
        !matches!(self.copied, Some(Err(_))) && !matches!(self.saved, Some(Err(_)))
    }
}

/// Deliver `delivery` as `settings` ask: clipboard first (fast, and what
/// people paste right away), then the file.
pub fn deliver(
    delivery: &Delivery,
    settings: &Settings,
    platform: &dyn SnapPlatform,
    now: DateTime<Local>,
) -> DeliveryReport {
    let copied = settings
        .copy_to_clipboard
        .then(|| platform.copy_image(&delivery.image));
    let saved = settings.save_to_file.then(|| {
        let folder = settings
            .save_folder
            .clone()
            .unwrap_or_else(|| platform.default_save_folder());
        encode_png(&delivery.image, delivery.scale)
            .and_then(|png| save_png(&folder, &png, now).map_err(|e| e.to_string()))
    });
    DeliveryReport { copied, saved }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ramp(w: u32, h: u32) -> Vec<u8> {
        (0..w * h)
            .flat_map(|i| [i as u8, (i >> 8) as u8, 0, 255])
            .collect()
    }

    #[test]
    fn crop_copies_rows_and_clips() {
        let px = ramp(4, 3);
        let c = crop(&px, 4, 3, PxRect::new(1, 1, 2, 2));
        assert_eq!((c.width, c.height), (2, 2));
        assert_eq!(c.pixel(0, 0), Some([5, 0, 0, 255]));
        assert_eq!(c.pixel(1, 1), Some([10, 0, 0, 255]));
        let clipped = crop(&px, 4, 3, PxRect::new(3, 2, 10, 10));
        assert_eq!((clipped.width, clipped.height), (1, 1));
        assert_eq!(clipped.pixel(0, 0), Some([11, 0, 0, 255]));
        let none = crop(&px, 4, 3, PxRect::new(9, 9, 2, 2));
        assert_eq!((none.width, none.height, none.pixels.len()), (0, 0, 0));
    }

    #[test]
    fn png_round_trips_with_retina_density() {
        let img = RgbaImage::new(3, 2, ramp(3, 2)).unwrap();
        let bytes = encode_png(&img, 2.0).unwrap();
        let decoder = png::Decoder::new(bytes.as_slice());
        let mut reader = decoder.read_info().unwrap();
        let dims = reader.info().pixel_dims.unwrap();
        assert_eq!(dims.xppu, 5669); // 144 dpi
        let mut buf = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut buf).unwrap();
        assert_eq!(&buf[..frame.buffer_size()], img.pixels.as_slice());
    }

    #[test]
    fn names_are_unique_within_a_second() {
        let dir = tempfile::tempdir().unwrap();
        let now = Local.with_ymd_and_hms(2026, 10, 12, 18, 7, 33).unwrap();
        assert_eq!(file_name(now), "Screen Snap 2026-10-12 at 18.07.33.png");
        let a = save_png(dir.path(), b"a", now).unwrap();
        let b = save_png(dir.path(), b"b", now).unwrap();
        assert_eq!(
            a.file_name().unwrap(),
            "Screen Snap 2026-10-12 at 18.07.33.png"
        );
        assert_eq!(
            b.file_name().unwrap(),
            "Screen Snap 2026-10-12 at 18.07.33 (2).png"
        );
    }

    #[test]
    fn report_messages() {
        let both = DeliveryReport {
            copied: Some(Ok(())),
            saved: Some(Ok(PathBuf::from("/x/a.png"))),
        };
        assert_eq!(
            both.message(),
            "Capture copied to the clipboard and saved as /x/a.png."
        );
        let failed = DeliveryReport {
            copied: Some(Err("busy".into())),
            saved: None,
        };
        assert_eq!(failed.message(), "Could not copy to the clipboard (busy).");
        assert!(!failed.is_ok());
        assert!(DeliveryReport::default()
            .message()
            .starts_with("Capture discarded"));
    }
}
