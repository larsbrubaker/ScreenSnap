//! The widget tree. One window shows one of two screens, chosen by
//! [`crate::state::Mode`]:
//!
//! - [`overlay`] — the frozen screen with the marquee, crosshair and
//!   magnifier ([`Mode::Overlay`]);
//! - [`settings_view`] — the settings window ([`Mode::Settings`]), with the
//!   shortcut [`hotkey_recorder`].
//!
//! The root is a `Rebuilder` keyed on [`AppState::mode_version`], so each
//! mode change (and each new capture) builds that screen afresh: an overlay
//! never inherits the last capture's selection. While hidden it shows an
//! empty widget.

pub mod hotkey_recorder;
pub mod overlay;
pub mod settings_view;

use agg_gui::{HAnchor, Rebuilder, Spacer, VAnchor, Widget};

use crate::fonts::Fonts;
use crate::state::{AppState, Mode};

/// The root widget. See the module docs.
pub fn build_root(state: AppState, fonts: Fonts) -> Box<dyn Widget> {
    let version_state = state.clone();
    Box::new(
        Rebuilder::new(
            move || version_state.mode_version(),
            move || match state.mode() {
                Mode::Overlay => Box::new(overlay::CaptureOverlay::new(&state, &fonts)),
                Mode::Settings => settings_view::build(&state, &fonts),
                Mode::Hidden => Box::new(Spacer::new()),
            },
        )
        .with_h_anchor(HAnchor::STRETCH)
        .with_v_anchor(VAnchor::STRETCH),
    )
}
