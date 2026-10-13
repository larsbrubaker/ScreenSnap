//! The capture overlay driven through the real app: drags select whole capture
//! pixels (inclusive) on 1x and 2x captures and deliver exactly those pixels;
//! Escape / right click cancel; Space / Enter capture the whole screen; a bare
//! click selects nothing; the frame paints the dimming, crosshair cursor and
//! magnifier.

mod common;

use agg_gui::{current_cursor_icon, CursorIcon, Key, Modifiers, MouseButton};
use common::{ramp_capture, Harness};
use screensnap_core::selection::{PxPoint, PxRect};
use screensnap_core::ui::overlay::{CaptureOverlay, OVERLAY_ID};
use screensnap_core::Mode;

/// An overlay over a `w`×`h` px capture at `scale`, in a window exactly its size.
fn overlay(w: u32, h: u32, scale: f64) -> Harness {
    let mut h_ = Harness::new(f64::from(w) / scale, f64::from(h) / scale, scale);
    h_.state.begin_capture(ramp_capture(w, h, scale));
    h_.frame();
    assert_eq!(h_.state.mode(), Mode::Overlay);
    assert!(h_.visible(OVERLAY_ID));
    h_
}

/// Deliver the queued selection and return the copied image's size and
/// top-left / bottom-right pixels.
fn delivered(h: &Harness) -> (u32, u32, [u8; 4], [u8; 4]) {
    assert!(h.state.has_pending_delivery(), "nothing was queued");
    let report = h.state.run_pending_delivery().expect("a delivery");
    assert!(report.is_ok(), "{report:?}");
    let copied = h.platform.copied.borrow();
    let img = copied.last().expect("an image was copied");
    (
        img.width,
        img.height,
        img.pixel(0, 0).unwrap(),
        img.pixel(img.width - 1, img.height - 1).unwrap(),
    )
}

#[test]
fn drag_selects_whole_pixels_inclusive_and_delivers_them() {
    let mut h = overlay(200, 120, 1.0);
    h.drag((10.5, 20.5), (40.5, 50.5));
    assert_eq!(h.state.mode(), Mode::Hidden);
    let (w, hh, first, last) = delivered(&h);
    assert_eq!((w, hh), (31, 31));
    assert_eq!(first, [10, 20, 200, 255]);
    assert_eq!(last, [40, 50, 200, 255]);
}

#[test]
fn drag_works_in_any_direction() {
    let mut h = overlay(200, 120, 1.0);
    h.drag((150.2, 100.7), (100.9, 60.1));
    let (w, hh, first, last) = delivered(&h);
    assert_eq!((w, hh), (51, 41));
    assert_eq!(first, [100, 60, 200, 255]);
    assert_eq!(last, [150, 100, 200, 255]);
}

#[test]
fn drag_on_a_retina_capture_selects_physical_pixels() {
    // 200x120 px at 2x is a 100x60-point overlay; the shell reports the
    // pointer in physical pixels, which are the capture's pixels.
    let mut h = overlay(200, 120, 2.0);
    let r = h.rect(OVERLAY_ID);
    assert_eq!((r.width, r.height), (100.0, 60.0));
    h.drag((11.0, 21.0), (40.0, 51.0));
    let (w, hh, first, last) = delivered(&h);
    assert_eq!((w, hh), (30, 31));
    assert_eq!(first, [11, 21, 200, 255]);
    assert_eq!(last, [40, 51, 200, 255]);
    // Retina density travels with the delivery's PNG; the crop is physical.
}

#[test]
fn a_drag_past_the_edge_stops_at_the_edge() {
    let mut h = overlay(100, 80, 1.0);
    h.drag((90.5, 70.5), (400.0, 300.0));
    let (w, hh, _, last) = delivered(&h);
    assert_eq!((w, hh), (10, 10));
    assert_eq!(last, [99, 79, 200, 255]);
}

#[test]
fn escape_cancels_even_mid_drag() {
    let mut h = overlay(100, 80, 1.0);
    h.key(Key::Escape);
    assert_eq!(h.state.mode(), Mode::Hidden);
    assert!(h.state.capture().is_none());
    assert!(!h.state.has_pending_delivery());

    h.state.begin_capture(ramp_capture(100, 80, 1.0));
    h.frame();
    h.app.on_mouse_move(10.0, 10.0);
    h.app
        .on_mouse_down(10.0, 10.0, MouseButton::Left, Modifiers::default());
    h.app.on_mouse_move(30.0, 30.0);
    h.frame();
    assert_eq!(
        h.widget::<CaptureOverlay>(OVERLAY_ID).selection(),
        Some(PxRect::new(10, 10, 21, 21))
    );
    h.key(Key::Escape);
    assert_eq!(h.state.mode(), Mode::Hidden);
    h.app
        .on_mouse_up(30.0, 30.0, MouseButton::Left, Modifiers::default());
    h.frame();
    assert!(!h.state.has_pending_delivery());
}

#[test]
fn right_click_cancels() {
    let mut h = overlay(100, 80, 1.0);
    h.app.on_mouse_move(50.0, 50.0);
    h.app
        .on_mouse_down(50.0, 50.0, MouseButton::Right, Modifiers::default());
    h.app
        .on_mouse_up(50.0, 50.0, MouseButton::Right, Modifiers::default());
    h.frame();
    assert_eq!(h.state.mode(), Mode::Hidden);
    assert!(!h.state.has_pending_delivery());
}

#[test]
fn space_and_enter_capture_the_whole_screen() {
    let mut h = overlay(64, 48, 2.0);
    h.move_to(5.0, 5.0);
    h.key(Key::Char(' '));
    assert_eq!(h.state.mode(), Mode::Hidden);
    let (w, hh, first, last) = delivered(&h);
    assert_eq!((w, hh), (64, 48));
    assert_eq!(first, [0, 0, 200, 255]);
    assert_eq!(last, [63, 47, 200, 255]);

    h.state.begin_capture(ramp_capture(64, 48, 2.0));
    h.frame();
    h.key(Key::Enter);
    let (w, hh, _, _) = delivered(&h);
    assert_eq!((w, hh), (64, 48));
}

#[test]
fn space_during_a_drag_does_not_capture() {
    let mut h = overlay(100, 80, 1.0);
    h.app.on_mouse_move(10.0, 10.0);
    h.app
        .on_mouse_down(10.0, 10.0, MouseButton::Left, Modifiers::default());
    h.app.on_mouse_move(20.0, 20.0);
    h.key(Key::Char(' '));
    assert_eq!(h.state.mode(), Mode::Overlay);
    h.app
        .on_mouse_up(20.0, 20.0, MouseButton::Left, Modifiers::default());
    h.frame();
    let (w, hh, _, _) = delivered(&h);
    assert_eq!((w, hh), (11, 11));
}

#[test]
fn a_click_without_a_drag_selects_nothing() {
    let mut h = overlay(100, 80, 1.0);
    h.click_at(30.5, 30.5);
    assert_eq!(h.state.mode(), Mode::Overlay);
    assert!(!h.state.has_pending_delivery());
    assert_eq!(h.widget::<CaptureOverlay>(OVERLAY_ID).selection(), None);
    // The overlay still works afterwards.
    h.drag((30.5, 30.5), (32.5, 33.5));
    let (w, hh, _, _) = delivered(&h);
    assert_eq!((w, hh), (3, 4));
}

#[test]
fn pointer_tracks_pixels_and_sets_a_crosshair_cursor() {
    let mut h = overlay(200, 120, 2.0);
    h.move_to(77.0, 33.0);
    let ov = h.widget::<CaptureOverlay>(OVERLAY_ID);
    assert_eq!(ov.pointer_px(), Some(PxPoint::new(77, 33)));
    assert!(ov.magnifier_shown());
    assert_eq!(current_cursor_icon(), CursorIcon::Crosshair);
}

#[test]
fn the_magnifier_follows_the_setting() {
    let mut h = Harness::new(100.0, 80.0, 1.0);
    h.state.update_settings(|s| s.show_magnifier = false);
    h.state.begin_capture(ramp_capture(100, 80, 1.0));
    h.frame();
    h.move_to(50.0, 40.0);
    assert!(!h.widget::<CaptureOverlay>(OVERLAY_ID).magnifier_shown());
}

/// The frame shows the capture at 1:1: the selection's pixels untouched,
/// everything else dimmed.
#[test]
fn painting_dims_outside_the_selection_only() {
    let mut h = Harness::new(300.0, 200.0, 1.0);
    h.state.update_settings(|s| s.show_magnifier = true);
    h.state.begin_capture(ramp_capture(300, 200, 1.0));
    h.frame();
    // Before any drag the whole screen is dimmed.
    let px = |frame: &[u8], x: usize, y: usize| {
        let i = (y * 300 + x) * 4;
        [frame[i], frame[i + 1], frame[i + 2]]
    };
    let frame = h.snapshot();
    assert!(
        px(&frame, 250, 20)[2] < 160,
        "dimmed: {:?}",
        px(&frame, 250, 20)
    );

    h.app.on_mouse_move(20.0, 150.0);
    h.app
        .on_mouse_down(20.0, 150.0, MouseButton::Left, Modifiers::default());
    h.app.on_mouse_move(120.0, 180.0);
    let frame = h.snapshot();
    // Inside the selection (away from the crosshair, label and loupe).
    assert_eq!(px(&frame, 60, 170), [60, 170, 200]);
    // Outside: dimmed.
    let outside = px(&frame, 290, 100);
    assert!(outside[2] < 160, "dimmed: {outside:?}");
}
