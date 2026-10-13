//! SCAFFOLD — replaced by the settings window implementation (see docs/PLAN.md §Settings).
use agg_gui::{Spacer, Widget};

use crate::fonts::Fonts;
use crate::state::AppState;

pub fn build(_state: &AppState, _fonts: &Fonts) -> Box<dyn Widget> {
    Box::new(Spacer::new())
}
