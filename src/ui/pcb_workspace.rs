use crate::commands::pcb::PcbCommand;
use crate::model::cad::{CadNet, Point2};
use crate::pcb::board::Board;
use crate::pcb::layer::BoardLayer;
use crate::pcb::track::TrackSegment;
use crate::pcb::via::Via;
use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum PcbTool {
    #[default]
    Select,
    Route,
    Via,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum CornerMode {
    #[default]
    FortyFive,
    Ninety,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) enum RoutingState {
    #[default]
    Idle,
    Armed {
        source_footprint: u64,
        net: usize,
    },
    Routing {
        net: usize,
        layer: BoardLayer,
        width_mm: f32,
        anchors: Vec<Point2>,
        cursor: Point2,
        corner_mode: CornerMode,
        vias: Vec<Via>,
    },
    ViaPreview {
        net: usize,
        position: Point2,
        from_layer: BoardLayer,
        to_layer: BoardLayer,
    },
}

pub(crate) struct PcbWorkspaceState {
    pub(crate) zoom: f32,
    pub(crate) pan: Vec2,
    pub(crate) tool: PcbTool,
    pub(crate) selected_footprints: HashSet<u64>,
    pub(crate) selected_track: Option<u64>,
    pub(crate) selected_via: Option<u64>,
    pub(crate) active_layer: BoardLayer,
    pub(crate) corner_mode: CornerMode,
    pub(crate) highlighted_net: Option<usize>,
    pub(crate) show_front_copper: bool,
    pub(crate) show_back_copper: bool,
    pub(crate) show_silkscreen: bool,
    pub(crate) show_edges: bool,
    pub(crate) routing: RoutingState,
    drag_start: Option<(Pos2, Vec<(u64, Point2)>)>,
    box_start: Option<Point2>,
}

impl Default for PcbWorkspaceState {
    fn default() -> Self {
        Self::initialized()
    }
}

impl PcbWorkspaceState {
    pub(crate) fn initialized() -> Self {
        Self {
            zoom: 12.0,
            active_layer: BoardLayer::FrontCopper,
            corner_mode: CornerMode::FortyFive,
            show_front_copper: true,
            show_back_copper: true,
            show_silkscreen: true,
            show_edges: true,
            pan: Vec2::new(20.0, 20.0),
            tool: PcbTool::Select,
            selected_footprints: HashSet::new(),
            selected_track: None,
            selected_via: None,
            highlighted_net: None,
            routing: RoutingState::Idle,
            drag_start: None,
            box_start: None,
        }
    }
}

pub(crate) fn render_pcb_workspace(
    ui: &mut egui::Ui,
    board: &Board,
    nets: &[CadNet],
    state: &mut PcbWorkspaceState,
) -> Vec<PcbCommand> {
    let mut commands = Vec::new();
    let selected = state
        .selected_footprints
        .iter()
        .copied()
        .collect::<Vec<_>>();
    ui.horizontal_wrapped(|ui| {
        for (tool, label) in [
            (PcbTool::Select, "Select"),
            (PcbTool::Route, "Route"),
            (PcbTool::Via, "Via"),
        ] {
            if ui.selectable_label(state.tool == tool, label).clicked() {
                state.tool = tool;
            }
        }
        ui.separator();
        if ui.button("Rotate").clicked() && !selected.is_empty() {
            commands.push(PcbCommand::RotateFootprints {
                footprint_ids: selected.clone(),
                delta_deg: 90.0,
            });
        }
        if ui.button("Flip").clicked() && !selected.is_empty() {
            commands.push(PcbCommand::FlipFootprints {
                footprint_ids: selected.clone(),
            });
        }
        if ui.button("Delete copper").clicked() {
            if let Some(track_id) = state.selected_track.take() {
                commands.push(PcbCommand::DeleteTracks {
                    track_ids: vec![track_id],
                });
            }
            if let Some(via_id) = state.selected_via.take() {
                commands.push(PcbCommand::DeleteVias {
                    via_ids: vec![via_id],
                });
            }
        }
        ui.separator();
        ui.label("Layer");
        ui.selectable_value(&mut state.active_layer, BoardLayer::FrontCopper, "F.Cu");
        ui.selectable_value(&mut state.active_layer, BoardLayer::BackCopper, "B.Cu");
        ui.separator();
        ui.selectable_value(&mut state.corner_mode, CornerMode::FortyFive, "45°");
        ui.selectable_value(&mut state.corner_mode, CornerMode::Ninety, "90°");
        ui.separator();
        ui.checkbox(&mut state.show_front_copper, "F.Cu");
        ui.checkbox(&mut state.show_back_copper, "B.Cu");
        ui.checkbox(&mut state.show_silkscreen, "Silk");
        ui.checkbox(&mut state.show_edges, "Edge.Cuts");
    });

    let available = ui.available_size();
    ui.horizontal_top(|ui| {
        let side_width = 170.0_f32.min(available.x * 0.28);
        ui.allocate_ui_with_layout(
            Vec2::new(side_width, available.y),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.heading("PCB");
                ui.label(format!(
                    "{} footprints · {} tracks · {} vias",
                    board.footprints.len(),
                    board.tracks.len(),
                    board.vias.len()
                ));
                ui.separator();
                ui.label("Nets");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for net in nets {
                        let selected = state.highlighted_net == Some(net.net_id);
                        if ui
                            .selectable_label(
                                selected,
                                format!("{}  ·  {}", net.name, net.connected_pins.len()),
                            )
                            .clicked()
                        {
                            state.highlighted_net = (!selected).then_some(net.net_id);
                        }
                    }
                });
                ui.separator();
                ui.label(match &state.routing {
                    RoutingState::Idle => "Routing idle".to_string(),
                    RoutingState::Armed { net, .. } => format!("Net {net} armed"),
                    RoutingState::Routing {
                        net,
                        anchors,
                        width_mm,
                        ..
                    } => format!(
                        "Routing net {net} · {:.2} mm · {} anchor(s)",
                        width_mm,
                        anchors.len()
                    ),
                    RoutingState::ViaPreview { net, .. } => format!("Via preview · net {net}"),
                });
                ui.label("Esc cancel · Backspace previous anchor");
            },
        );
        ui.separator();
        let canvas_size = ui.available_size();
        let (response, painter) = ui.allocate_painter(canvas_size, Sense::click_and_drag());
        let rect = response.rect;
        painter.rect_filled(rect, 0.0, Color32::from_rgb(13, 18, 23));
        handle_view(ui.ctx(), &response, rect, state);
        let pan = state.pan;
        let zoom = state.zoom;
        let map = |point: Point2| rect.min + pan + Vec2::new(point.x * zoom, point.y * zoom);
        let unmap = |point: Pos2| {
            let local = point - rect.min - pan;
            Point2::new(local.x / zoom, local.y / zoom)
        };
        draw_board(
            &painter,
            rect,
            board,
            nets,
            state,
            Point2::new((-state.pan.x) / state.zoom, (-state.pan.y) / state.zoom),
            Point2::new(
                (rect.width() - state.pan.x) / state.zoom,
                (rect.height() - state.pan.y) / state.zoom,
            ),
            map,
        );

        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            state.routing = RoutingState::Idle;
            state.tool = PcbTool::Select;
        }
        if ui.input(|input| input.key_pressed(egui::Key::Backspace))
            && let RoutingState::Routing { anchors, .. } = &mut state.routing
        {
            anchors.pop();
            if anchors.is_empty() {
                state.routing = RoutingState::Idle;
            }
        }
        if let Some(pointer) = response.hover_pos()
            && rect.contains(pointer)
        {
            update_interaction(
                PointerInput {
                    screen: pointer,
                    world: unmap(pointer),
                    shift: ui.input(|input| input.modifiers.shift),
                    released: ui.input(|input| input.pointer.any_released()),
                },
                &response,
                board,
                nets,
                state,
                &mut commands,
            );
        }
    });
    commands
}

fn handle_view(
    ctx: &egui::Context,
    response: &egui::Response,
    rect: Rect,
    state: &mut PcbWorkspaceState,
) {
    let scroll = ctx.input(|input| input.smooth_scroll_delta.y);
    if scroll != 0.0
        && response
            .hover_pos()
            .is_some_and(|position| rect.contains(position))
    {
        state.zoom = (state.zoom * if scroll > 0.0 { 1.08 } else { 1.0 / 1.08 }).clamp(2.0, 80.0);
    }
    if ctx.input(|input| input.pointer.middle_down())
        || (response.dragged() && ctx.input(|input| input.modifiers.alt))
    {
        state.pan += ctx.input(|input| input.pointer.delta());
    }
}

#[allow(clippy::too_many_arguments)] // Rendering inputs are explicit and borrowed for one frame.
fn draw_board(
    painter: &egui::Painter,
    rect: Rect,
    board: &Board,
    nets: &[CadNet],
    state: &PcbWorkspaceState,
    viewport_min: Point2,
    viewport_max: Point2,
    map: impl Fn(Point2) -> Pos2 + Copy,
) {
    let grid = (board.grid.grid_mm * state.zoom).max(4.0);
    let mut x = rect.left() + state.pan.x.rem_euclid(grid);
    while x < rect.right() {
        painter.line_segment(
            [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
            Stroke::new(0.5_f32, Color32::from_rgb(31, 38, 45)),
        );
        x += grid;
    }
    let mut y = rect.top() + state.pan.y.rem_euclid(grid);
    while y < rect.bottom() {
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(0.5_f32, Color32::from_rgb(31, 38, 45)),
        );
        y += grid;
    }
    if state.show_edges {
        for edge in board.edges_in_rect(viewport_min, viewport_max) {
            if let Some(segment) = board.outline.points.get(edge..=edge + 1) {
                painter.line_segment(
                    [map(segment[0]), map(segment[1])],
                    Stroke::new(2.0_f32, Color32::from_rgb(210, 210, 190)),
                );
            }
        }
    }
    for track in board
        .track_candidates_in_bounds(viewport_min, viewport_max)
        .into_iter()
        .filter_map(|id| board.track(id))
    {
        let visible = match track.layer {
            BoardLayer::FrontCopper => state.show_front_copper,
            BoardLayer::BackCopper => state.show_back_copper,
            _ => false,
        };
        if !visible {
            continue;
        }
        let highlighted = state.highlighted_net == Some(track.net_id);
        let color = match track.layer {
            BoardLayer::FrontCopper => Color32::from_rgb(225, 90, 70),
            _ => Color32::from_rgb(75, 135, 225),
        };
        painter.line_segment(
            [map(track.start), map(track.end)],
            Stroke::new(
                (track.width_mm * state.zoom).max(if highlighted { 4.0 } else { 2.0 }),
                color,
            ),
        );
    }
    for via in board
        .via_candidates_in_bounds(viewport_min, viewport_max)
        .into_iter()
        .filter_map(|id| board.via(id))
    {
        painter.circle_filled(
            map(via.position),
            (via.diameter_mm * state.zoom * 0.5).max(3.0),
            Color32::from_rgb(205, 175, 85),
        );
        painter.circle_filled(
            map(via.position),
            (via.drill_mm * state.zoom * 0.5).max(1.0),
            Color32::from_rgb(20, 24, 28),
        );
    }
    for edge in board.ratsnest_edges(nets) {
        let from = board.footprint(edge.from_footprint_id);
        let to = board.footprint(edge.to_footprint_id);
        if let Some((from, to)) = from.zip(to) {
            painter.line_segment(
                [map(from.position), map(to.position)],
                Stroke::new(1.0_f32, Color32::from_rgb(220, 205, 85)),
            );
        }
    }
    for footprint in board
        .footprints_in_rect(viewport_min, viewport_max)
        .into_iter()
        .filter_map(|id| board.footprint(id))
    {
        let center = map(footprint.position);
        let bounds = Rect::from_center_size(center, Vec2::new(9.0, 6.0) * state.zoom.min(3.0));
        let selected = state.selected_footprints.contains(&footprint.id);
        painter.rect_filled(
            bounds,
            2.0,
            if footprint.flipped {
                Color32::from_rgb(80, 125, 165)
            } else {
                Color32::from_rgb(180, 165, 105)
            },
        );
        painter.rect_stroke(
            bounds,
            2.0,
            Stroke::new(
                if selected { 2.5_f32 } else { 1.0_f32 },
                if selected {
                    Color32::from_rgb(80, 220, 130)
                } else {
                    Color32::from_rgb(235, 225, 180)
                },
            ),
            StrokeKind::Outside,
        );
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            &footprint.reference,
            egui::FontId::proportional(10.0),
            Color32::BLACK,
        );
    }
    if let RoutingState::Routing {
        anchors,
        cursor,
        corner_mode,
        ..
    } = &state.routing
        && let Some(start) = anchors.last().copied()
    {
        for (a, b) in route_segments(start, *cursor, *corner_mode) {
            painter.line_segment(
                [map(a), map(b)],
                Stroke::new(2.0_f32, Color32::from_rgb(100, 240, 170)),
            );
        }
    }
}

#[derive(Clone, Copy)]
struct PointerInput {
    screen: Pos2,
    world: Point2,
    shift: bool,
    released: bool,
}

fn update_interaction(
    pointer: PointerInput,
    response: &egui::Response,
    board: &Board,
    nets: &[CadNet],
    state: &mut PcbWorkspaceState,
    commands: &mut Vec<PcbCommand>,
) {
    let world = if matches!(state.tool, PcbTool::Route | PcbTool::Via) {
        snap_routing_point(pointer.world, board)
    } else {
        snap_point(pointer.world, board.grid.grid_mm)
    };
    if let RoutingState::Routing { cursor, .. } = &mut state.routing {
        *cursor = world;
    }
    match state.tool {
        PcbTool::Select => {
            if response.clicked() {
                let hit = hit_footprint(board, world);
                if !pointer.shift {
                    state.selected_footprints.clear();
                }
                if let Some(id) = hit {
                    state.selected_footprints.insert(id);
                    state.selected_track = None;
                    state.selected_via = None;
                } else if let Some(id) = hit_via(board, world) {
                    state.selected_via = Some(id);
                    state.selected_track = None;
                } else {
                    state.selected_track = hit_track(board, world);
                    state.selected_via = None;
                }
            }
            if response.drag_started() {
                let hit = hit_footprint(board, world);
                if let Some(id) = hit {
                    if !state.selected_footprints.contains(&id) {
                        state.selected_footprints.clear();
                        state.selected_footprints.insert(id);
                    }
                    state.drag_start = Some((
                        pointer.screen,
                        board
                            .footprints
                            .iter()
                            .filter(|footprint| state.selected_footprints.contains(&footprint.id))
                            .map(|footprint| (footprint.id, footprint.position))
                            .collect(),
                    ));
                } else {
                    state.box_start = Some(world);
                }
            }
            if pointer.released
                && let Some((start, origins)) = state.drag_start.take()
            {
                let delta = (pointer.screen - start) / state.zoom;
                if delta.length_sq() > 0.0001 {
                    commands.push(PcbCommand::MoveFootprints(
                        origins
                            .into_iter()
                            .map(|(id, origin)| {
                                (
                                    id,
                                    snap_point(
                                        Point2::new(origin.x + delta.x, origin.y + delta.y),
                                        board.grid.grid_mm,
                                    ),
                                )
                            })
                            .collect(),
                    ));
                }
            }
            if pointer.released
                && let Some(start) = state.box_start.take()
            {
                let min_x = start.x.min(world.x);
                let max_x = start.x.max(world.x);
                let min_y = start.y.min(world.y);
                let max_y = start.y.max(world.y);
                state.selected_footprints = board
                    .footprints_in_rect(Point2::new(min_x, min_y), Point2::new(max_x, max_y))
                    .iter()
                    .filter_map(|id| board.footprint(*id))
                    .filter(|footprint| {
                        footprint.position.x >= min_x
                            && footprint.position.x <= max_x
                            && footprint.position.y >= min_y
                            && footprint.position.y <= max_y
                    })
                    .map(|footprint| footprint.id)
                    .collect();
            }
        }
        PcbTool::Route => {
            if response.clicked() {
                route_click(board, nets, state, world, commands);
            }
        }
        PcbTool::Via => {
            if let Some(net_id) = state.highlighted_net {
                let other_layer = if state.active_layer == BoardLayer::FrontCopper {
                    BoardLayer::BackCopper
                } else {
                    BoardLayer::FrontCopper
                };
                state.routing = RoutingState::ViaPreview {
                    net: net_id,
                    position: world,
                    from_layer: state.active_layer,
                    to_layer: other_layer,
                };
                if response.clicked() {
                    let next_id = board.vias.iter().map(|via| via.id).max().unwrap_or(0) + 1;
                    commands.push(PcbCommand::AddVia(Via {
                        id: next_id,
                        net_id,
                        position: world,
                        diameter_mm: board.design_rules.min_via_diameter_mm.max(0.6),
                        drill_mm: board.design_rules.min_via_drill_mm.max(0.3),
                    }));
                    state.active_layer = other_layer;
                    state.routing = RoutingState::Idle;
                }
            }
        }
    }
}

fn route_click(
    board: &Board,
    nets: &[CadNet],
    state: &mut PcbWorkspaceState,
    world: Point2,
    commands: &mut Vec<PcbCommand>,
) {
    let hit = hit_footprint(board, world);
    match &mut state.routing {
        RoutingState::Idle => {
            let Some(source_id) = hit else {
                return;
            };
            let Some(net) = footprint_net(board, nets, source_id) else {
                return;
            };
            state.highlighted_net = Some(net);
            state.routing = RoutingState::Armed {
                source_footprint: source_id,
                net,
            };
        }
        RoutingState::Armed {
            source_footprint,
            net,
        } => {
            let start = board
                .footprint(*source_footprint)
                .map_or(world, |footprint| footprint.position);
            state.routing = RoutingState::Routing {
                net: *net,
                layer: state.active_layer,
                width_mm: board.design_rules.min_track_width_mm.max(0.25),
                anchors: vec![start, world],
                cursor: world,
                corner_mode: state.corner_mode,
                vias: Vec::new(),
            };
        }
        RoutingState::Routing {
            net,
            layer,
            width_mm,
            anchors,
            corner_mode,
            vias,
            ..
        } => {
            if hit.is_some_and(|id| footprint_net(board, nets, id) == Some(*net)) {
                anchors.push(world);
                let mut next_id = board.tracks.iter().map(|track| track.id).max().unwrap_or(0) + 1;
                let mut tracks = Vec::new();
                for points in anchors.windows(2) {
                    for (start, end) in route_segments(points[0], points[1], *corner_mode) {
                        if start != end {
                            tracks.push(TrackSegment {
                                id: next_id,
                                net_id: *net,
                                layer: *layer,
                                start,
                                end,
                                width_mm: *width_mm,
                            });
                            next_id += 1;
                        }
                    }
                }
                commands.push(PcbCommand::AddRoute {
                    tracks,
                    vias: std::mem::take(vias),
                });
                state.routing = RoutingState::Idle;
            } else {
                anchors.push(world);
            }
        }
        RoutingState::ViaPreview { .. } => {}
    }
}

fn hit_footprint(board: &Board, point: Point2) -> Option<u64> {
    board
        .footprint_candidates(point)
        .iter()
        .rev()
        .filter_map(|id| board.footprint(*id))
        .find(|footprint| {
            (footprint.position.x - point.x).abs() <= 5.0
                && (footprint.position.y - point.y).abs() <= 3.5
        })
        .map(|footprint| footprint.id)
}

fn hit_via(board: &Board, point: Point2) -> Option<u64> {
    board
        .via_candidates(point)
        .iter()
        .rev()
        .filter_map(|id| board.via(*id))
        .find(|via| {
            (via.position.x - point.x).powi(2) + (via.position.y - point.y).powi(2)
                <= (via.diameter_mm * 0.75).powi(2)
        })
        .map(|via| via.id)
}

fn hit_track(board: &Board, point: Point2) -> Option<u64> {
    board
        .track_candidates(point)
        .iter()
        .rev()
        .filter_map(|id| board.track(*id))
        .find(|track| {
            point_segment_distance(point, track.start, track.end) <= track.width_mm * 0.5 + 0.3
        })
        .map(|track| track.id)
}

fn point_segment_distance(point: Point2, start: Point2, end: Point2) -> f32 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared <= f32::EPSILON {
        return ((point.x - start.x).powi(2) + (point.y - start.y).powi(2)).sqrt();
    }
    let t =
        (((point.x - start.x) * dx + (point.y - start.y) * dy) / length_squared).clamp(0.0, 1.0);
    let closest = Point2::new(start.x + dx * t, start.y + dy * t);
    ((point.x - closest.x).powi(2) + (point.y - closest.y).powi(2)).sqrt()
}

fn footprint_net(board: &Board, nets: &[CadNet], footprint_id: u64) -> Option<usize> {
    let symbol_id = board.footprint(footprint_id)?.symbol_instance_id?;
    nets.iter()
        .find(|net| {
            net.connected_pins
                .iter()
                .any(|pin| pin.component_id == symbol_id)
        })
        .map(|net| net.net_id)
}

fn snap_point(point: Point2, grid: f32) -> Point2 {
    if grid <= f32::EPSILON {
        point
    } else {
        Point2::new(
            (point.x / grid).round() * grid,
            (point.y / grid).round() * grid,
        )
    }
}

fn snap_routing_point(point: Point2, board: &Board) -> Point2 {
    board
        .pad_candidates(point)
        .into_iter()
        .filter_map(|pad| board.pad_position(&pad))
        .filter(|position| {
            (position.x - point.x).powi(2) + (position.y - point.y).powi(2) <= 2.0_f32.powi(2)
        })
        .min_by(|left, right| {
            let left_distance = (left.x - point.x).powi(2) + (left.y - point.y).powi(2);
            let right_distance = (right.x - point.x).powi(2) + (right.y - point.y).powi(2);
            left_distance.total_cmp(&right_distance)
        })
        .unwrap_or_else(|| snap_point(point, board.grid.grid_mm))
}

fn route_segments(start: Point2, end: Point2, mode: CornerMode) -> Vec<(Point2, Point2)> {
    let corner = match mode {
        CornerMode::Ninety => Point2::new(end.x, start.y),
        CornerMode::FortyFive => {
            let diagonal = (end.x - start.x).abs().min((end.y - start.y).abs());
            Point2::new(
                start.x + (end.x - start.x).signum() * diagonal,
                start.y + (end.y - start.y).signum() * diagonal,
            )
        }
    };
    vec![(start, corner), (corner, end)]
}
