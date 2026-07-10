use crate::model::PinRef;
use egui::Pos2;
use serde::{Deserialize, Serialize};

pub(crate) type ComponentId = u64;
pub(crate) type PinId = String;
pub(crate) type JunctionId = u64;
pub(crate) type WireSegmentId = u64;
pub(crate) type NetId = usize;

#[derive(Debug, Clone)]
pub(crate) struct Wire {
    pub(crate) id: u64,
    pub(crate) points: Vec<Pos2>,
    pub(crate) start: WireEndpoint,
    pub(crate) end: WireEndpoint,
}

impl Wire {
    pub(crate) fn new(id: u64, points: Vec<Pos2>) -> Self {
        let start = points
            .first()
            .copied()
            .map(WireEndpoint::FreePoint)
            .unwrap_or(WireEndpoint::FreePoint(Pos2::ZERO));
        let end = points
            .last()
            .copied()
            .map(WireEndpoint::FreePoint)
            .unwrap_or(WireEndpoint::FreePoint(Pos2::ZERO));
        Self {
            id,
            points,
            start,
            end,
        }
    }

    pub(crate) fn with_endpoints(
        id: u64,
        points: Vec<Pos2>,
        start: WireEndpoint,
        end: WireEndpoint,
    ) -> Self {
        Self {
            id,
            points,
            start,
            end,
        }
    }

    pub(crate) fn endpoint_at(&self, point_index: usize) -> Option<&WireEndpoint> {
        if point_index == 0 {
            Some(&self.start)
        } else if point_index + 1 == self.points.len() {
            Some(&self.end)
        } else {
            None
        }
    }

    pub(crate) fn endpoint_at_mut(&mut self, point_index: usize) -> Option<&mut WireEndpoint> {
        if point_index == 0 {
            Some(&mut self.start)
        } else if point_index + 1 == self.points.len() {
            Some(&mut self.end)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum SavedWireEndpoint {
    Pin { component_id: u64, pin_name: String },
    Junction { junction_id: u64 },
    FreePoint { x: f32, y: f32 },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WireEndpoint {
    Pin(PinRef),
    Junction(JunctionId),
    FreePoint(Pos2),
}

impl WireEndpoint {
    pub(crate) fn saved(&self) -> SavedWireEndpoint {
        match self {
            Self::Pin(pin) => SavedWireEndpoint::Pin {
                component_id: pin.component_id,
                pin_name: pin.pin_name.clone(),
            },
            Self::Junction(junction_id) => SavedWireEndpoint::Junction {
                junction_id: *junction_id,
            },
            Self::FreePoint(pos) => SavedWireEndpoint::FreePoint { x: pos.x, y: pos.y },
        }
    }

    pub(crate) fn from_saved(saved: SavedWireEndpoint) -> Self {
        match saved {
            SavedWireEndpoint::Pin {
                component_id,
                pin_name,
            } => Self::Pin(PinRef {
                component_id,
                pin_name,
            }),
            SavedWireEndpoint::Junction { junction_id } => Self::Junction(junction_id),
            SavedWireEndpoint::FreePoint { x, y } => Self::FreePoint(Pos2::new(x, y)),
        }
    }
}

pub(crate) fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let ap = p - a;
    let ab_len_sq = ab.x * ab.x + ab.y * ab.y;
    if ab_len_sq == 0.0 {
        return ap.length();
    }
    let t = ((ap.x * ab.x) + (ap.y * ab.y)) / ab_len_sq;
    let t = t.clamp(0.0, 1.0);
    let closest = a + ab * t;
    (p - closest).length()
}

pub(crate) fn point_touches_wire_segment(point: Pos2, a: Pos2, b: Pos2) -> bool {
    distance_to_segment(point, a, b) <= 1.0
}
