//! The settings window's content (`docs/PLAN.md` §Settings window), shown in
//! [`crate::state::Mode::Settings`] and built afresh each time the window
//! opens (the root `Rebuilder` in [`crate::ui`]).
//!
//! Top to bottom: the app heading; the capture shortcut
//! ([`HotkeyRecorder`]); the four delivery / overlay checkboxes, each written
//! straight through [`AppState::update_settings`]; the save folder with
//! "Choose…" (the platform's folder picker) and "Show" (reveal in the file
//! manager); warnings for a missing Screen Recording permission (with "Open
//! System Settings") and for a shortcut the OS refused; the last capture's
//! outcome; and the Close / Quit buttons.
//!
//! Texts that change while the window is up — the folder path, the warnings,
//! the status line — sit in small `Rebuilder`s keyed on a hash of exactly what
//! they show (plus the theme epoch, for their colours), so they refresh on
//! the next layout without disturbing the rest of the view (focus, hover,
//! the recorder's state). The permission is read from the platform on each
//! layout, so granting it in System Settings shows up as soon as the window
//! draws again.
//!
//! Key widgets carry the ids below, which the headless tests find them by.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use agg_gui::theme::{current_visuals, current_visuals_epoch};
use agg_gui::{
    Button, Checkbox, FlexColumn, FlexRow, HAnchor, Insets, Label, Rebuilder, Separator, Size,
    Spacer, VAnchor, Widget,
};

use super::hotkey_recorder::HotkeyRecorder;
use crate::fonts::{fa, Fonts};
use crate::platform::Permission;
use crate::settings::Settings;
use crate::state::AppState;
use crate::APP_NAME;

/// Widget id of the whole settings view.
pub const SETTINGS_VIEW_ID: &str = "settings-view";
/// Checkbox ids.
pub const COPY_TO_CLIPBOARD_ID: &str = "settings-copy-to-clipboard";
pub const SAVE_TO_FILE_ID: &str = "settings-save-to-file";
pub const INCLUDE_CURSOR_ID: &str = "settings-include-cursor";
pub const SHOW_MAGNIFIER_ID: &str = "settings-show-magnifier";
/// The label showing the save folder's path.
pub const SAVE_FOLDER_PATH_ID: &str = "settings-save-folder-path";
/// "Choose…" (pick a save folder) and "Show" (reveal it).
pub const CHOOSE_FOLDER_ID: &str = "settings-choose-folder";
pub const SHOW_FOLDER_ID: &str = "settings-show-folder";
/// The Screen Recording permission warning and its button.
pub const PERMISSION_WARNING_ID: &str = "settings-permission-warning";
pub const OPEN_PERMISSION_SETTINGS_ID: &str = "settings-open-permission-settings";
/// The "shortcut is taken" warning.
pub const HOTKEY_ERROR_ID: &str = "settings-hotkey-error";
/// The last capture's outcome.
pub const STATUS_ID: &str = "settings-status";
/// Bottom buttons.
pub const CLOSE_ID: &str = "settings-close";
pub const QUIT_ID: &str = "settings-quit";

/// What the permission warning says.
pub const PERMISSION_TEXT: &str =
    "Screen Recording is not allowed: captures show only the desktop. Allow it, then restart.";

const BODY_SIZE: f64 = 14.0;
const NOTE_SIZE: f64 = 13.0;
/// Width of the left-hand labels in label + control rows.
const ROW_LABEL_WIDTH: f64 = 150.0;
/// Indent that lines the folder row up with the checkbox labels.
const CHECKBOX_INDENT: f64 = 28.0;

fn hash_of(value: &impl Hash) -> u64 {
    let mut h = DefaultHasher::new();
    value.hash(&mut h);
    h.finish()
}

/// The folder captures are saved in: the chosen one or the platform default.
fn save_folder(state: &AppState) -> PathBuf {
    state
        .settings()
        .save_folder
        .unwrap_or_else(|| state.platform().default_save_folder())
}

/// The settings window content. See the module docs.
pub fn build(state: &AppState, fonts: &Fonts) -> Box<dyn Widget> {
    let heading = Label::new(format!("{}  {APP_NAME}", fa::CROP), Arc::clone(&fonts.ui))
        .with_font_size(22.0)
        .with_strong(true);
    let intro = Label::new(
        "Press the shortcut anywhere to freeze the screen, then drag over the part you want.",
        Arc::clone(&fonts.ui),
    )
    .with_font_size(NOTE_SIZE)
    .with_dim(true)
    .with_wrap(true)
    .with_h_anchor(HAnchor::STRETCH);

    let column = FlexColumn::new()
        .with_gap(10.0)
        .with_inner_padding(Insets::from_sides(24.0, 24.0, 20.0, 18.0))
        .with_top_anchor(true)
        .with_panel_bg()
        .with_h_anchor(HAnchor::STRETCH)
        .with_v_anchor(VAnchor::STRETCH)
        .add(Box::new(heading))
        .add(Box::new(intro))
        .add(Box::new(Separator::horizontal()))
        .add(hotkey_row(state, fonts))
        .add(checkbox(
            state,
            fonts,
            COPY_TO_CLIPBOARD_ID,
            format!("{}  Copy to clipboard", fa::CLIPBOARD),
            |s| s.copy_to_clipboard,
            |s, on| s.copy_to_clipboard = on,
        ))
        .add(checkbox(
            state,
            fonts,
            SAVE_TO_FILE_ID,
            format!("{}  Save to folder", fa::SAVE),
            |s| s.save_to_file,
            |s, on| s.save_to_file = on,
        ))
        .add(folder_row(state, fonts))
        .add(checkbox(
            state,
            fonts,
            INCLUDE_CURSOR_ID,
            format!("{}  Include mouse pointer", fa::MOUSE_POINTER),
            |s| s.include_cursor,
            |s, on| s.include_cursor = on,
        ))
        .add(checkbox(
            state,
            fonts,
            SHOW_MAGNIFIER_ID,
            format!("{}  Show magnifier", fa::SEARCH_PLUS),
            |s| s.show_magnifier,
            |s, on| s.show_magnifier = on,
        ))
        .add(notices(state, fonts))
        .add_flex(Box::new(Spacer::new()), 1.0)
        .add(status_line(state, fonts))
        .add(Box::new(Separator::horizontal()))
        .add(bottom_buttons(state, fonts))
        .with_name(SETTINGS_VIEW_ID);
    Box::new(column)
}

/// "Capture shortcut" + the recorder.
fn hotkey_row(state: &AppState, fonts: &Fonts) -> Box<dyn Widget> {
    let label = Label::new(
        format!("{}  Capture shortcut", fa::KEYBOARD),
        Arc::clone(&fonts.ui),
    )
    .with_font_size(BODY_SIZE)
    .with_min_size(Size::new(ROW_LABEL_WIDTH, 0.0))
    .with_v_anchor(VAnchor::CENTER);
    Box::new(
        FlexRow::new()
            .with_gap(12.0)
            .with_h_anchor(HAnchor::STRETCH)
            .add(Box::new(label))
            .add_flex(
                Box::new(HotkeyRecorder::new(state, Arc::clone(&fonts.ui))),
                1.0,
            ),
    )
}

/// A checkbox bound to one boolean setting.
fn checkbox(
    state: &AppState,
    fonts: &Fonts,
    id: &str,
    label: String,
    read: fn(&Settings) -> bool,
    write: fn(&mut Settings, bool),
) -> Box<dyn Widget> {
    let change_state = state.clone();
    Box::new(
        Checkbox::new(label, Arc::clone(&fonts.ui), read(&state.settings()))
            .with_font_size(BODY_SIZE)
            .on_change(move |on| change_state.update_settings(|s| write(s, on)))
            .with_name(id),
    )
}

/// The save folder's path with "Choose…" and "Show".
fn folder_row(state: &AppState, fonts: &Fonts) -> Box<dyn Widget> {
    let version_state = state.clone();
    let build_state = state.clone();
    let build_fonts = fonts.clone();
    let path = Rebuilder::new(
        move || hash_of(&(save_folder(&version_state), current_visuals_epoch())),
        move || {
            Box::new(
                Label::new(
                    save_folder(&build_state).display().to_string(),
                    Arc::clone(&build_fonts.ui),
                )
                .with_font_size(NOTE_SIZE)
                .with_dim(true)
                .with_ellipsis_if_clipped(true)
                .with_h_anchor(HAnchor::STRETCH)
                .with_v_anchor(VAnchor::CENTER)
                .with_name(SAVE_FOLDER_PATH_ID),
            )
        },
    )
    .with_h_anchor(HAnchor::STRETCH)
    .with_v_anchor(VAnchor::CENTER);

    let choose_state = state.clone();
    let choose = Button::new("Choose\u{2026}", Arc::clone(&fonts.ui))
        .with_icon(fa::FOLDER_OPEN, Arc::clone(&fonts.icons))
        .on_click(move || {
            let start = save_folder(&choose_state);
            if let Some(folder) = choose_state.platform().pick_folder(&start) {
                choose_state.update_settings(|s| s.save_folder = Some(folder));
            }
        })
        .with_name(CHOOSE_FOLDER_ID);
    let show_state = state.clone();
    let show = Button::new("Show", Arc::clone(&fonts.ui))
        .on_click(move || {
            let folder = save_folder(&show_state);
            if let Err(e) = show_state.platform().reveal(&folder) {
                show_state.set_status(Some(format!("Could not show {}: {e}", folder.display())));
            }
        })
        .with_name(SHOW_FOLDER_ID);
    Box::new(
        FlexRow::new()
            .with_gap(8.0)
            .with_h_anchor(HAnchor::STRETCH)
            .with_margin(Insets::from_sides(CHECKBOX_INDENT, 0.0, 0.0, 0.0))
            .add_flex(Box::new(path), 1.0)
            .add(Box::new(choose))
            .add(Box::new(show)),
    )
}

/// The permission and hotkey-registration warnings (nothing when all is well).
fn notices(state: &AppState, fonts: &Fonts) -> Box<dyn Widget> {
    let version_state = state.clone();
    let build_state = state.clone();
    let build_fonts = fonts.clone();
    Box::new(
        Rebuilder::new(
            move || {
                let denied =
                    version_state.platform().screen_capture_permission() == Permission::Denied;
                hash_of(&(
                    denied,
                    version_state.hotkey_error(),
                    current_visuals_epoch(),
                ))
            },
            move || build_notices(&build_state, &build_fonts),
        )
        .with_h_anchor(HAnchor::STRETCH),
    )
}

fn build_notices(state: &AppState, fonts: &Fonts) -> Box<dyn Widget> {
    let denied = state.platform().screen_capture_permission() == Permission::Denied;
    let error = state.hotkey_error();
    if !denied && error.is_none() {
        return Box::new(Spacer::new().with_max_size(Size::new(f64::MAX, 0.0)));
    }
    let visuals = current_visuals();
    let mut column = FlexColumn::new()
        .with_gap(10.0)
        .with_top_anchor(true)
        .with_h_anchor(HAnchor::STRETCH);
    if denied {
        let open_state = state.clone();
        let open = Button::new("Open System Settings", Arc::clone(&fonts.ui))
            .on_click(move || open_state.platform().open_permission_settings())
            .with_v_anchor(VAnchor::CENTER)
            .with_name(OPEN_PERMISSION_SETTINGS_ID);
        let text = Label::new(
            format!("{}  {PERMISSION_TEXT}", fa::WARNING),
            Arc::clone(&fonts.ui),
        )
        .with_font_size(NOTE_SIZE)
        .with_color(visuals.warning_color())
        .with_wrap(true)
        .with_h_anchor(HAnchor::STRETCH)
        .with_v_anchor(VAnchor::CENTER);
        column = column.add(Box::new(
            FlexRow::new()
                .with_gap(12.0)
                .with_h_anchor(HAnchor::STRETCH)
                .add_flex(Box::new(text), 1.0)
                .add(Box::new(open))
                .with_name(PERMISSION_WARNING_ID),
        ));
    }
    if let Some(error) = error {
        column = column.add(Box::new(
            Label::new(format!("{}  {error}", fa::WARNING), Arc::clone(&fonts.ui))
                .with_font_size(NOTE_SIZE)
                .with_color(visuals.error_color())
                .with_wrap(true)
                .with_h_anchor(HAnchor::STRETCH)
                .with_name(HOTKEY_ERROR_ID),
        ));
    }
    Box::new(column)
}

/// The last capture's outcome, live.
fn status_line(state: &AppState, fonts: &Fonts) -> Box<dyn Widget> {
    let version_state = state.clone();
    let build_state = state.clone();
    let build_fonts = fonts.clone();
    Box::new(
        Rebuilder::new(
            move || hash_of(&version_state.status()),
            move || {
                let text = build_state.status().unwrap_or_default();
                Box::new(
                    Label::new(text, Arc::clone(&build_fonts.ui))
                        .with_font_size(NOTE_SIZE)
                        .with_dim(true)
                        .with_wrap(true)
                        .with_h_anchor(HAnchor::STRETCH)
                        .with_name(STATUS_ID),
                )
            },
        )
        .with_h_anchor(HAnchor::STRETCH),
    )
}

/// "Quit Screen Snap" on the left, "Close" on the right.
fn bottom_buttons(state: &AppState, fonts: &Fonts) -> Box<dyn Widget> {
    let quit_state = state.clone();
    let quit = Button::new(format!("Quit {APP_NAME}"), Arc::clone(&fonts.ui))
        .with_icon(fa::POWER_OFF, Arc::clone(&fonts.icons))
        .on_click(move || quit_state.request_quit())
        .with_name(QUIT_ID);
    let close_state = state.clone();
    let close = Button::new("Close", Arc::clone(&fonts.ui))
        .with_default_action()
        .with_cancel_action()
        .on_click(move || close_state.hide_settings())
        .with_name(CLOSE_ID);
    Box::new(
        FlexRow::new()
            .with_gap(10.0)
            .with_h_anchor(HAnchor::STRETCH)
            .add(Box::new(quit))
            .add_flex(
                Box::new(Spacer::new().with_max_size(Size::new(f64::MAX, 0.0))),
                1.0,
            )
            .add(Box::new(close)),
    )
}
