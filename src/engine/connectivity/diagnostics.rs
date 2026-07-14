use crate::model::{ConnectivityDiagnostic, Wire};

pub(in crate::engine) fn geometry_diagnostics(wires: &[Wire]) -> Vec<ConnectivityDiagnostic> {
    wires
        .iter()
        .filter(|wire| {
            wire.points.len() < 2
                || wire
                    .points
                    .windows(2)
                    .all(|segment| segment[0].distance(segment[1]) <= f32::EPSILON)
        })
        .map(|wire| ConnectivityDiagnostic::DegenerateWire { wire_id: wire.id })
        .collect()
}
