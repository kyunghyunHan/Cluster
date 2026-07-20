//! Document command boundary.
//!
//! UI code submits an [`EditorCommand`]. Command handlers are the only
//! production code allowed to mutate schematic collections directly.

pub(crate) mod component;
pub(crate) mod context;
pub(crate) mod document;
pub(crate) mod pcb;
pub(crate) mod properties;
pub(crate) mod selection;
pub(crate) mod wiring;

use component::ComponentCommand;
use context::{CommandContext, CommandOutcome};
use document::DocumentCommand;
use pcb::PcbCommand;
use properties::PropertiesCommand;
use selection::SelectionCommand;
use wiring::WiringCommand;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ChangeSet {
    pub(crate) persistence_changed: bool,
    pub(crate) schematic_geometry_changed: bool,
    pub(crate) schematic_connectivity_changed: bool,
    pub(crate) electrical_values_changed: bool,
    pub(crate) simulation_topology_changed: bool,
    pub(crate) simulation_parameters_changed: bool,
    pub(crate) pcb_sync_changed: bool,
    pub(crate) pcb_geometry_changed: bool,
    pub(crate) pcb_rules_changed: bool,
    pub(crate) visual_only: bool,
}

impl ChangeSet {
    pub(crate) const fn schematic() -> Self {
        Self {
            persistence_changed: true,
            schematic_geometry_changed: true,
            schematic_connectivity_changed: true,
            simulation_topology_changed: true,
            pcb_sync_changed: true,
            ..Self::none()
        }
    }

    pub(crate) const fn properties() -> Self {
        Self {
            persistence_changed: true,
            electrical_values_changed: true,
            simulation_parameters_changed: true,
            pcb_sync_changed: true,
            ..Self::none()
        }
    }

    pub(crate) const fn board() -> Self {
        Self {
            persistence_changed: true,
            pcb_geometry_changed: true,
            ..Self::none()
        }
    }

    pub(crate) const fn restored_document() -> Self {
        Self {
            persistence_changed: true,
            schematic_geometry_changed: true,
            schematic_connectivity_changed: true,
            electrical_values_changed: true,
            simulation_topology_changed: true,
            simulation_parameters_changed: true,
            pcb_sync_changed: true,
            pcb_geometry_changed: true,
            pcb_rules_changed: true,
            visual_only: false,
        }
    }

    pub(crate) const fn none() -> Self {
        Self {
            persistence_changed: false,
            schematic_geometry_changed: false,
            schematic_connectivity_changed: false,
            electrical_values_changed: false,
            simulation_topology_changed: false,
            simulation_parameters_changed: false,
            pcb_sync_changed: false,
            pcb_geometry_changed: false,
            pcb_rules_changed: false,
            visual_only: false,
        }
    }

    pub(crate) const fn needs_repaint(self) -> bool {
        self.persistence_changed || self.visual_only
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandMergeKey {
    ComponentMove,
    WireControlPoint(u64),
    ComponentProperties(u64),
    BoardFootprint(u64),
}

#[allow(dead_code)] // Variants are populated as actions are extracted module by module.
pub(crate) enum EditorCommand {
    Component(ComponentCommand),
    Wiring(WiringCommand),
    Selection(SelectionCommand),
    Properties(PropertiesCommand),
    Document(DocumentCommand),
    Pcb(PcbCommand),
}

impl EditorCommand {
    pub(crate) fn pcb_local_analysis_impact(
        &self,
        board: &crate::pcb::board::Board,
    ) -> Option<pcb::PcbAnalysisImpact> {
        match self {
            Self::Pcb(command) => command.local_analysis_impact(board),
            _ => None,
        }
    }

    pub(crate) fn pcb_delta_scope(
        &self,
        board: &crate::pcb::board::Board,
    ) -> Option<pcb::PcbDeltaScope> {
        match self {
            Self::Pcb(command) => Some(command.delta_scope(board)),
            _ => None,
        }
    }

    pub(crate) const fn description(&self) -> &'static str {
        match self {
            Self::Component(ComponentCommand::Place { .. }) => "Place component",
            Self::Component(ComponentCommand::PlaceCustom { .. }) => "Place custom component",
            Self::Component(ComponentCommand::Paste { .. }) => "Paste selection",
            Self::Component(ComponentCommand::Move { .. }) => "Move component",
            Self::Wiring(WiringCommand::Add { .. }) => "Place wire",
            Self::Wiring(WiringCommand::MoveControlPoint { .. }) => "Move wire point",
            Self::Wiring(WiringCommand::InsertControlPoint { .. }) => "Insert wire point",
            Self::Wiring(WiringCommand::Tidy { .. }) => "Tidy wires",
            Self::Selection(SelectionCommand::Delete) => "Delete selection",
            Self::Selection(SelectionCommand::Rotate) => "Rotate component",
            Self::Selection(SelectionCommand::Duplicate) => "Duplicate selection",
            Self::Selection(SelectionCommand::Align(_)) => "Align components",
            Self::Selection(SelectionCommand::Distribute { .. }) => "Distribute components",
            Self::Properties(PropertiesCommand::SetComponentValue { .. }) => "Edit value",
            Self::Properties(PropertiesCommand::SetComponentProperties { .. }) => {
                "Edit component properties"
            }
            Self::Properties(PropertiesCommand::ToggleSwitch { .. }) => "Toggle switch",
            Self::Document(DocumentCommand::Reset) => "New document",
            Self::Pcb(PcbCommand::MoveFootprint { .. }) => "Move PCB footprint",
            Self::Pcb(PcbCommand::MoveFootprints(_)) => "Move PCB footprints",
            Self::Pcb(PcbCommand::RotateFootprint { .. }) => "Rotate PCB footprint",
            Self::Pcb(PcbCommand::RotateFootprints { .. }) => "Rotate PCB footprints",
            Self::Pcb(PcbCommand::FlipFootprints { .. }) => "Flip PCB footprints",
            Self::Pcb(PcbCommand::AddTrack(_)) => "Route PCB track",
            Self::Pcb(PcbCommand::AddRoute { .. }) => "Route PCB connection",
            Self::Pcb(PcbCommand::RemoveTrack { .. }) => "Remove PCB track",
            Self::Pcb(PcbCommand::DeleteTracks { .. }) => "Delete PCB tracks",
            Self::Pcb(PcbCommand::EditTrack(_)) => "Edit PCB track",
            Self::Pcb(PcbCommand::AddVia(_)) => "Place PCB via",
            Self::Pcb(PcbCommand::RemoveVia { .. }) => "Remove PCB via",
            Self::Pcb(PcbCommand::DeleteVias { .. }) => "Delete PCB vias",
            Self::Pcb(PcbCommand::SetOutline(_)) => "Edit board outline",
            Self::Pcb(PcbCommand::SetGeometry { .. }) => "Fit board to geometry",
            Self::Pcb(PcbCommand::ChangeNetClass(_)) => "Change PCB net class",
            Self::Pcb(PcbCommand::ApplyEco { .. }) => "Apply schematic PCB changes",
        }
    }

    pub(crate) const fn merge_key(&self) -> Option<CommandMergeKey> {
        match self {
            Self::Component(ComponentCommand::Move { .. }) => Some(CommandMergeKey::ComponentMove),
            Self::Wiring(WiringCommand::MoveControlPoint { wire_id, .. }) => {
                Some(CommandMergeKey::WireControlPoint(*wire_id))
            }
            Self::Properties(PropertiesCommand::SetComponentValue { component_id, .. })
            | Self::Properties(PropertiesCommand::SetComponentProperties {
                component_id, ..
            }) => Some(CommandMergeKey::ComponentProperties(*component_id)),
            Self::Pcb(PcbCommand::MoveFootprint { footprint_id, .. }) => {
                Some(CommandMergeKey::BoardFootprint(*footprint_id))
            }
            _ => None,
        }
    }

    pub(crate) fn apply(self, context: &mut CommandContext<'_>) -> CommandOutcome {
        match self {
            Self::Component(command) => command.apply(context),
            Self::Wiring(command) => command.apply(context),
            Self::Selection(command) => command.apply(context),
            Self::Properties(command) => command.apply(context),
            Self::Document(command) => command.apply(context),
            Self::Pcb(command) => command.apply(context),
        }
    }
}
