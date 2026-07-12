//! Single, rectangle, group, and hover selection presentation.

use crate::app::Selection;
use crate::model::{Component, Wire};

pub(crate) fn selection_summary(
    selected: Option<Selection>,
    components: &[Component],
    wires: &[Wire],
) -> String {
    match selected {
        Some(Selection::Component(id)) => components
            .iter()
            .find(|component| component.id == id)
            .map(|component| format!("Selected: {}", component.label))
            .unwrap_or_else(|| "Selected: missing component".to_string()),
        Some(Selection::Wire(id)) => wires
            .iter()
            .find(|wire| wire.id == id)
            .map(|wire| {
                let length: f32 = wire
                    .points
                    .windows(2)
                    .map(|pair| pair[0].distance(pair[1]))
                    .filter(|length| length.is_finite())
                    .sum();
                format!("Selected: wire {length:.0}px")
            })
            .unwrap_or_else(|| "Selected: missing wire".to_string()),
        None => "Selected: none".to_string(),
    }
}
