//! Component, pin, and wire hit testing without mutating editor state.

use crate::app::Selection;
use crate::model::{Component, Wire, distance_to_segment};
use crate::ui::app::component_bounds;
use egui::Pos2;

pub(crate) fn hit_test(pos: Pos2, components: &[Component], wires: &[Wire]) -> Option<Selection> {
    hit_test_component(pos, components).or_else(|| hit_test_wire(pos, wires))
}

pub(crate) fn hit_test_component(pos: Pos2, components: &[Component]) -> Option<Selection> {
    components
        .iter()
        .rev()
        .find(|component| component_bounds(component).contains(pos))
        .map(|component| Selection::Component(component.id))
}

pub(crate) fn hit_test_wire(pos: Pos2, wires: &[Wire]) -> Option<Selection> {
    wires
        .iter()
        .rev()
        .find(|wire| {
            wire.points
                .windows(2)
                .any(|segment| distance_to_segment(pos, segment[0], segment[1]) <= 10.0)
        })
        .map(|wire| Selection::Wire(wire.id))
}

pub(crate) fn hit_test_wire_control_point(pos: Pos2, wires: &[Wire]) -> Option<(u64, usize)> {
    wires.iter().rev().find_map(|wire| {
        wire.points
            .iter()
            .enumerate()
            .find(|(_, point)| pos.distance(**point) <= 14.0)
            .map(|(index, _)| (wire.id, index))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn zero_length_wire_is_safe_to_hit_test() {
        let wires = [Wire::new(9, vec![Pos2::ZERO, Pos2::ZERO])];
        assert_eq!(hit_test_wire(Pos2::new(30.0, 30.0), &wires), None);
    }
}
