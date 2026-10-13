//! The settings window driven through the real app: checkboxes write the
//! settings, the folder row picks and reveals, warnings and the status line
//! follow the state live, and Close / Quit do their jobs.

mod common;

use std::path::PathBuf;

use agg_gui::Checkbox;
use common::Harness;
use screensnap_core::platform::Permission;
use screensnap_core::settings::Settings;
use screensnap_core::ui::settings_view::*;
use screensnap_core::Mode;

#[test]
fn checkboxes_start_from_and_toggle_the_settings() {
    let mut h = Harness::settings();
    let defaults = h.state.settings();
    for (id, read) in [
        (
            COPY_TO_CLIPBOARD_ID,
            (|s| s.copy_to_clipboard) as fn(&Settings) -> bool,
        ),
        (SAVE_TO_FILE_ID, |s| s.save_to_file),
        (INCLUDE_CURSOR_ID, |s| s.include_cursor),
        (SHOW_MAGNIFIER_ID, |s| s.show_magnifier),
    ] {
        assert_eq!(h.widget::<Checkbox>(id).checked(), read(&defaults), "{id}");
        h.click(id);
        assert_eq!(read(&h.state.settings()), !read(&defaults), "{id} toggled");
        h.click(id);
        assert_eq!(read(&h.state.settings()), read(&defaults), "{id} back");
    }
}

#[test]
fn folder_row_shows_picks_and_reveals_the_folder() {
    let mut h = Harness::settings();
    let default = h.platform.folder.clone();
    assert_eq!(
        h.label_text(SAVE_FOLDER_PATH_ID),
        default.display().to_string()
    );

    // Cancelled picker: nothing changes.
    h.click(CHOOSE_FOLDER_ID);
    assert_eq!(h.state.settings().save_folder, None);

    let chosen = PathBuf::from("/tmp/elsewhere");
    *h.platform.picked.borrow_mut() = Some(chosen.clone());
    h.click(CHOOSE_FOLDER_ID);
    assert_eq!(h.state.settings().save_folder, Some(chosen.clone()));
    h.frame();
    assert_eq!(
        h.label_text(SAVE_FOLDER_PATH_ID),
        chosen.display().to_string()
    );

    h.click(SHOW_FOLDER_ID);
    assert_eq!(
        h.platform.calls(),
        vec![
            format!("pick_folder {}", default.display()),
            format!("pick_folder {}", default.display()),
            format!("reveal {}", chosen.display()),
        ]
    );
}

#[test]
fn permission_warning_appears_live_and_opens_system_settings() {
    let mut h = Harness::settings();
    assert!(!h.visible(PERMISSION_WARNING_ID));
    h.platform.permission.set(Permission::Denied);
    h.frame();
    assert!(h.visible(PERMISSION_WARNING_ID));
    h.click(OPEN_PERMISSION_SETTINGS_ID);
    assert_eq!(h.platform.calls(), vec!["open_permission_settings"]);
    h.platform.permission.set(Permission::Granted);
    h.frame();
    assert!(!h.visible(PERMISSION_WARNING_ID));
}

#[test]
fn a_capture_without_permission_opens_settings_instead() {
    // Granted: the shell may grab the screen; nothing else changes.
    let mut h = Harness::new(560.0, 520.0, 1.0);
    assert!(h.state.ready_to_capture());
    assert_eq!(h.state.mode(), Mode::Hidden);

    // Denied: the OS would hand back only the wallpaper, so no frozen screen
    // — the settings window opens with the warning and says why.
    h.platform.permission.set(Permission::Denied);
    assert!(!h.state.ready_to_capture());
    h.frame();
    assert_eq!(h.state.mode(), Mode::Settings);
    assert!(h.visible(PERMISSION_WARNING_ID));
    assert_eq!(
        h.label_text(STATUS_ID),
        screensnap_core::state::CAPTURE_NEEDS_PERMISSION
    );
}

#[test]
fn hotkey_error_and_status_update_live() {
    let mut h = Harness::settings();
    assert!(!h.visible(HOTKEY_ERROR_ID));
    h.state
        .set_hotkey_error(Some("\u{21e7}\u{2318}2 is used by another app.".into()));
    h.frame();
    assert!(h
        .label_text(HOTKEY_ERROR_ID)
        .contains("used by another app"));
    h.state.set_hotkey_error(None);
    h.frame();
    assert!(!h.visible(HOTKEY_ERROR_ID));

    assert_eq!(h.label_text(STATUS_ID), "");
    h.state
        .set_status(Some("Capture copied to the clipboard.".into()));
    h.frame();
    assert_eq!(h.label_text(STATUS_ID), "Capture copied to the clipboard.");
}

#[test]
fn close_hides_the_window() {
    let mut h = Harness::settings();
    h.click(CLOSE_ID);
    assert_eq!(h.state.mode(), Mode::Hidden);
    assert!(!h.state.quit_requested());
}

#[test]
fn escape_closes_the_window() {
    let mut h = Harness::settings();
    h.key(agg_gui::Key::Escape);
    assert_eq!(h.state.mode(), Mode::Hidden);
}

#[test]
fn quit_asks_the_shell_to_quit() {
    let mut h = Harness::settings();
    h.click(QUIT_ID);
    assert!(h.state.quit_requested());
}

#[test]
fn every_control_is_on_screen_in_both_looks() {
    for dark in [false, true] {
        let mut h = Harness::settings();
        agg_gui::theme::set_visuals(screensnap_core::fonts::visuals(dark));
        h.platform.permission.set(Permission::Denied);
        h.state.set_hotkey_error(Some("Taken.".into()));
        h.state.set_status(Some(
            "Capture copied to the clipboard and saved as /Users/someone/Pictures/Screen Snap/\
             Screen Snap 2026-10-12 at 18.07.33.png."
                .into(),
        ));
        h.frame();
        // The window itself, not the view (which grows with its content).
        let window = agg_gui::Rect::new(0.0, 0.0, h.size.width / h.scale, h.size.height / h.scale);
        assert!(h.visible(SETTINGS_VIEW_ID));
        for id in [
            screensnap_core::ui::hotkey_recorder::HOTKEY_RECORDER_ID,
            COPY_TO_CLIPBOARD_ID,
            SAVE_TO_FILE_ID,
            SAVE_FOLDER_PATH_ID,
            CHOOSE_FOLDER_ID,
            SHOW_FOLDER_ID,
            INCLUDE_CURSOR_ID,
            SHOW_MAGNIFIER_ID,
            OPEN_PERMISSION_SETTINGS_ID,
            HOTKEY_ERROR_ID,
            STATUS_ID,
            CLOSE_ID,
            QUIT_ID,
        ] {
            let r = h.rect(id);
            assert!(
                r.x >= window.x
                    && r.y >= window.y
                    && r.x + r.width <= window.x + window.width + 0.5
                    && r.y + r.height <= window.y + window.height + 0.5
                    && r.width > 0.0
                    && r.height > 0.0,
                "{id} at {r:?} is outside {window:?} (dark = {dark})"
            );
        }
    }
}
