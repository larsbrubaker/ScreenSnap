//! SCAFFOLD — replaced by the capture overlay implementation (see docs/PLAN.md §Overlay).
use agg_gui::Spacer;

use crate::fonts::Fonts;
use crate::state::AppState;

pub struct CaptureOverlay;

impl CaptureOverlay {
    #[allow(clippy::new_ret_no_self)]
    pub fn new(_state: &AppState, _fonts: &Fonts) -> Spacer {
        Spacer::new()
    }
}
