//! The settings window's shortcut recorder driven through the real app: a
//! click starts recording (and tells the shell), a valid combination is
//! stored, Escape / focus loss leave the shortcut alone, and an unusable
//! combination shows the hint while recording goes on.

mod common;

use agg_gui::{Key, Modifiers};
use common::Harness;
use screensnap_core::hotkey::{DisplayStyle, Hotkey, HotkeyMods};
use screensnap_core::ui::hotkey_recorder::{
    invalid_hint, HotkeyRecorder, HINT_DURATION, HOTKEY_RECORDER_ID, RECORDING_PROMPT,
};
use screensnap_core::ui::settings_view::INCLUDE_CURSOR_ID;

fn cmd_shift() -> Modifiers {
    Modifiers {
        shift: true,
        meta: true,
        ..Modifiers::default()
    }
}

/// Click inside the recorder's box (its left end; the hint sits to its right).
fn click_box(h: &mut Harness) {
    let r = h.rect(HOTKEY_RECORDER_ID);
    let (_, y) = h.center_of(HOTKEY_RECORDER_ID);
    h.click_at((r.x + 20.0) * h.scale, y);
}

fn recorder(h: &Harness) -> &HotkeyRecorder {
    h.widget::<HotkeyRecorder>(HOTKEY_RECORDER_ID)
}

#[test]
fn shows_the_current_shortcut() {
    let mut h = Harness::settings();
    h.state.update_settings(|s| {
        s.hotkey = Hotkey::new(
            HotkeyMods {
                cmd: true,
                shift: true,
                ..HotkeyMods::default()
            },
            "Digit2",
        )
        .unwrap()
    });
    h.frame();
    assert_eq!(recorder(&h).shown_text(), "\u{21e7}\u{2318}2");
    assert!(!recorder(&h).is_recording());
}

#[test]
fn records_shift_cmd_5_into_the_settings() {
    let mut h = Harness::settings();
    click_box(&mut h);
    assert!(
        h.state.recording_hotkey(),
        "the shell is told to stand down"
    );
    assert_eq!(recorder(&h).shown_text(), RECORDING_PROMPT);

    // Modifiers pressed on their own are waited through.
    h.key_with(Key::Other("Shift".into()), cmd_shift());
    assert!(h.state.recording_hotkey());
    assert_eq!(recorder(&h).hint(), None);

    h.key_with(Key::Char('5'), cmd_shift());
    assert!(!h.state.recording_hotkey());
    let hotkey = h.state.settings().hotkey;
    assert_eq!(hotkey.to_accelerator(), "super+shift+Digit5");
    assert_eq!(hotkey.display(DisplayStyle::Mac), "\u{21e7}\u{2318}5");
    assert_eq!(recorder(&h).shown_text(), "\u{21e7}\u{2318}5");
}

#[test]
fn escape_stops_recording_without_a_change() {
    let mut h = Harness::settings();
    let before = h.state.settings().hotkey;
    click_box(&mut h);
    assert!(h.state.recording_hotkey());
    h.key(Key::Escape);
    assert!(!h.state.recording_hotkey());
    assert_eq!(h.state.settings().hotkey, before);
    // Escape went to the recorder, not the window's Close (cancel) button.
    assert_eq!(h.state.mode(), screensnap_core::Mode::Settings);
}

#[test]
fn a_second_click_stops_recording() {
    let mut h = Harness::settings();
    click_box(&mut h);
    assert!(h.state.recording_hotkey());
    click_box(&mut h);
    assert!(!h.state.recording_hotkey());
}

#[test]
fn an_unusable_combination_shows_a_hint_and_keeps_recording() {
    agg_gui::clock::start_virtual();
    let mut h = Harness::settings();
    let before = h.state.settings().hotkey;
    click_box(&mut h);
    h.key(Key::Char('5'));
    assert!(h.state.recording_hotkey());
    assert_eq!(h.state.settings().hotkey, before);
    assert_eq!(recorder(&h).hint(), Some(invalid_hint(DisplayStyle::Mac)));

    agg_gui::clock::advance(HINT_DURATION);
    h.frame();
    assert_eq!(recorder(&h).hint(), None, "the hint is brief");

    // A function key alone is fine.
    h.key(Key::F(5));
    assert!(!h.state.recording_hotkey());
    assert_eq!(h.state.settings().hotkey.to_accelerator(), "F5");
    agg_gui::clock::use_real();
}

#[test]
fn clicking_elsewhere_stops_recording() {
    let mut h = Harness::settings();
    let before = h.state.settings().hotkey;
    click_box(&mut h);
    assert!(h.state.recording_hotkey());
    h.click(INCLUDE_CURSOR_ID);
    assert!(!h.state.recording_hotkey());
    assert_eq!(h.state.settings().hotkey, before);
}

#[test]
fn keyboard_focus_can_start_recording() {
    let mut h = Harness::settings();
    agg_gui::focus::request_focus(screensnap_core::ui::hotkey_recorder::HOTKEY_RECORDER_FOCUS);
    h.frame();
    h.key(Key::Char(' '));
    assert!(h.state.recording_hotkey());
}

#[test]
fn closing_the_window_ends_recording() {
    let mut h = Harness::settings();
    click_box(&mut h);
    h.state.hide_settings();
    h.frame();
    assert!(!h.state.recording_hotkey());
}
