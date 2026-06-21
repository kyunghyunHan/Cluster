#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum LayerSide {
    Front,
    Back,
    Internal(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) enum BoardLayer {
    FrontCopper,
    BackCopper,
    FrontSilkscreen,
    BackSilkscreen,
    FrontMask,
    BackMask,
    EdgeCuts,
    UserDwgs,
}

impl BoardLayer {
    pub(crate) fn gerber_name(self) -> &'static str {
        match self {
            BoardLayer::FrontCopper => "F.Cu",
            BoardLayer::BackCopper => "B.Cu",
            BoardLayer::FrontSilkscreen => "F.SilkS",
            BoardLayer::BackSilkscreen => "B.SilkS",
            BoardLayer::FrontMask => "F.Mask",
            BoardLayer::BackMask => "B.Mask",
            BoardLayer::EdgeCuts => "Edge.Cuts",
            BoardLayer::UserDwgs => "Dwgs.User",
        }
    }
}

pub(crate) fn default_two_layer_stackup() -> Vec<BoardLayer> {
    vec![
        BoardLayer::FrontCopper,
        BoardLayer::BackCopper,
        BoardLayer::FrontSilkscreen,
        BoardLayer::BackSilkscreen,
        BoardLayer::FrontMask,
        BoardLayer::BackMask,
        BoardLayer::EdgeCuts,
    ]
}
