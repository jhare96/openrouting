use std::collections::{BinaryHeap, HashMap, HashSet};
use std::cmp::Reverse;

use crate::dsn::{DsnDesign, PadShape, Side};

#[derive(Debug, Clone)]
pub struct RoutedWire {
    pub net_name: String,
    pub layer: String,
    pub width: i64,
    pub points: Vec<(i64, i64)>,
}

#[derive(Debug, Clone)]
pub struct RoutedVia {
    pub net_name: String,
    pub padstack_name: String,
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone)]
pub struct RoutingResult {
    pub wires: Vec<RoutedWire>,
    pub vias: Vec<RoutedVia>,
    pub unrouted: Vec<String>,
}

// ─── Grid state ───────────────────────────────────────────────────────────────

/// Obstacle grid: one bool grid per layer index.
struct Grid {
    width: usize,
    height: usize,
    /// obstacles[layer][y * width + x]
    obstacles: Vec<Vec<bool>>,
    grid_size: i64,
    offset_x: i64,
    offset_y: i64,
}

impl Grid {
    fn new(
        grid_size: i64,
        offset_x: i64,
        offset_y: i64,
        width: usize,
        height: usize,
        num_layers: usize,
    ) -> Self {
        let obstacles = vec![vec![false; width * height]; num_layers];
        Grid { width, height, obstacles, grid_size, offset_x, offset_y }
    }

    fn dsn_to_grid(&self, x: i64, y: i64) -> (i64, i64) {
        ((x - self.offset_x) / self.grid_size, (y - self.offset_y) / self.grid_size)
    }

    fn grid_to_dsn_center(&self, gx: i64, gy: i64) -> (i64, i64) {
        (
            self.offset_x + gx * self.grid_size + self.grid_size / 2,
            self.offset_y + gy * self.grid_size + self.grid_size / 2,
        )
    }

    fn in_bounds(&self, gx: i64, gy: i64) -> bool {
        gx >= 0 && gy >= 0 && (gx as usize) < self.width && (gy as usize) < self.height
    }

    fn is_obstacle(&self, layer: usize, gx: i64, gy: i64) -> bool {
        if !self.in_bounds(gx, gy) {
            return true;
        }
        self.obstacles[layer][gy as usize * self.width + gx as usize]
    }

    fn set_obstacle(&mut self, layer: usize, gx: i64, gy: i64) {
        if self.in_bounds(gx, gy) {
            self.obstacles[layer][gy as usize * self.width + gx as usize] = true;
        }
    }

    /// Mark a circle of radius `r` grid cells around (cx, cy) as obstacles.
    fn mark_circle(&mut self, layer: usize, cx: i64, cy: i64, r: i64) {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    self.set_obstacle(layer, cx + dx, cy + dy);
                }
            }
        }
    }
}

// ─── BFS maze router ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct State {
    gx: i32,
    gy: i32,
    layer: u8,
}

/// 8-directional moves on the same layer + via transitions.
const DIRS: [(i32, i32); 8] = [
    (1, 0), (-1, 0), (0, 1), (0, -1),
    (1, 1), (1, -1), (-1, 1), (-1, -1),
];

fn bfs(
    grid: &Grid,
    start_cells: &[(i32, i32, usize)],   // (gx, gy, layer)
    target_cells: &HashSet<(i32, i32, usize)>,
    signal_layers: &[usize],
) -> Option<Vec<State>> {
    if start_cells.is_empty() || target_cells.is_empty() {
        return None;
    }

    // Cost: move cost = 10, diagonal = 14, via = 100
    let mut dist: HashMap<State, u32> = HashMap::new();
    let mut prev: HashMap<State, State> = HashMap::new();
    let mut heap: BinaryHeap<Reverse<(u32, State)>> = BinaryHeap::new();

    for &(gx, gy, layer) in start_cells {
        let s = State { gx: gx as i32, gy: gy as i32, layer: layer as u8 };
        dist.insert(s, 0);
        heap.push(Reverse((0, s)));
    }

    while let Some(Reverse((cost, cur))) = heap.pop() {
        if target_cells.contains(&(cur.gx as i32, cur.gy as i32, cur.layer as usize)) {
            // Backtrack
            let mut path = vec![cur];
            let mut node = cur;
            while let Some(&p) = prev.get(&node) {
                path.push(p);
                node = p;
            }
            path.reverse();
            return Some(path);
        }

        if dist.get(&cur).copied().unwrap_or(u32::MAX) < cost {
            continue;
        }

        // Moves on same layer
        for &(dx, dy) in &DIRS {
            let nx = cur.gx + dx;
            let ny = cur.gy + dy;
            if !grid.in_bounds(nx as i64, ny as i64) {
                continue;
            }
            if grid.is_obstacle(cur.layer as usize, nx as i64, ny as i64) {
                // Allow if this is a target cell
                let is_target = target_cells.contains(&(nx as i32, ny as i32, cur.layer as usize));
                if !is_target {
                    continue;
                }
            }
            let move_cost = if dx != 0 && dy != 0 { 14u32 } else { 10u32 };
            let next_cost = cost + move_cost;
            let ns = State { gx: nx, gy: ny, layer: cur.layer };
            if next_cost < dist.get(&ns).copied().unwrap_or(u32::MAX) {
                dist.insert(ns, next_cost);
                prev.insert(ns, cur);
                heap.push(Reverse((next_cost, ns)));
            }
        }

        // Via: change layer
        for &other_layer in signal_layers {
            if other_layer == cur.layer as usize {
                continue;
            }
            if grid.is_obstacle(other_layer, cur.gx as i64, cur.gy as i64) {
                let is_target = target_cells.contains(&(cur.gx as i32, cur.gy as i32, other_layer));
                if !is_target {
                    continue;
                }
            }
            let via_cost = 100u32;
            let next_cost = cost + via_cost;
            let ns = State { gx: cur.gx, gy: cur.gy, layer: other_layer as u8 };
            if next_cost < dist.get(&ns).copied().unwrap_or(u32::MAX) {
                dist.insert(ns, next_cost);
                prev.insert(ns, cur);
                heap.push(Reverse((next_cost, ns)));
            }
        }
    }

    None
}

// ─── Path conversion ──────────────────────────────────────────────────────────

/// Merge collinear grid segments and convert to (wire_segments, via_positions).
fn path_to_wires_and_vias(
    path: &[State],
    grid: &Grid,
    net_name: &str,
    trace_width: i64,
    design: &DsnDesign,
) -> (Vec<RoutedWire>, Vec<RoutedVia>) {
    let mut wires: Vec<RoutedWire> = Vec::new();
    let mut vias: Vec<RoutedVia> = Vec::new();

    if path.is_empty() {
        return (wires, vias);
    }

    let layer_name = |layer_idx: usize| -> String {
        design
            .layers
            .iter()
            .find(|l| l.index == layer_idx)
            .map(|l| l.name.clone())
            .unwrap_or_else(|| format!("Layer{}", layer_idx))
    };

    let mut current_layer = path[0].layer as usize;
    let mut seg_points: Vec<(i64, i64)> = Vec::new();
    let (sx, sy) = grid.grid_to_dsn_center(path[0].gx as i64, path[0].gy as i64);
    seg_points.push((sx, sy));

    for i in 1..path.len() {
        let s = &path[i];
        let (px, py) = grid.grid_to_dsn_center(s.gx as i64, s.gy as i64);

        if s.layer as usize != current_layer {
            // Emit current wire segment
            if seg_points.len() >= 2 {
                wires.push(RoutedWire {
                    net_name: net_name.to_string(),
                    layer: layer_name(current_layer),
                    width: trace_width,
                    points: merge_collinear(seg_points.clone()),
                });
            }
            // Emit via
            let (vx, vy) = grid.grid_to_dsn_center(path[i - 1].gx as i64, path[i - 1].gy as i64);
            vias.push(RoutedVia {
                net_name: net_name.to_string(),
                padstack_name: "via".to_string(),
                x: vx,
                y: vy,
            });
            // Start new segment
            seg_points = vec![(vx, vy), (px, py)];
            current_layer = s.layer as usize;
        } else {
            seg_points.push((px, py));
        }
    }

    if seg_points.len() >= 2 {
        wires.push(RoutedWire {
            net_name: net_name.to_string(),
            layer: layer_name(current_layer),
            width: trace_width,
            points: merge_collinear(seg_points),
        });
    }

    (wires, vias)
}

fn merge_collinear(pts: Vec<(i64, i64)>) -> Vec<(i64, i64)> {
    if pts.len() <= 2 {
        return pts;
    }
    let mut result = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let prev = result.last().copied().unwrap();
        let cur = pts[i];
        let next = pts[i + 1];
        // Check if prev -> cur -> next are collinear (cross product == 0)
        let dx1 = cur.0 - prev.0;
        let dy1 = cur.1 - prev.1;
        let dx2 = next.0 - cur.0;
        let dy2 = next.1 - cur.1;
        if dx1 * dy2 != dy1 * dx2 {
            result.push(cur);
        }
    }
    result.push(*pts.last().unwrap());
    result
}

// ─── Main routing function ────────────────────────────────────────────────────

pub fn route(design: &DsnDesign) -> RoutingResult {
    let trace_width = design.rules.trace_width.max(1);
    let clearance = design.rules.clearance.max(1);

    // Grid size: coarse enough for performance, fine enough for accuracy
    let mut grid_size = trace_width.max(clearance);
    // Ensure we don't make the grid too large
    let board_w = (design.boundary.max_x - design.boundary.min_x).max(1);
    let board_h = (design.boundary.max_y - design.boundary.min_y).max(1);
    // Cap at 500x500 cells
    while board_w / grid_size > 500 || board_h / grid_size > 500 {
        grid_size = (grid_size as f64 * 1.5) as i64;
    }
    grid_size = grid_size.max(1);

    let offset_x = design.boundary.min_x;
    let offset_y = design.boundary.min_y;
    let grid_w = ((board_w + grid_size - 1) / grid_size) as usize + 1;
    let grid_h = ((board_h + grid_size - 1) / grid_size) as usize + 1;

    // Signal layers
    let signal_layers: Vec<usize> = design
        .layers
        .iter()
        .filter(|l| l.layer_type == "signal")
        .map(|l| l.index)
        .collect();

    let num_layers = design.layers.len().max(2);
    let mut grid = Grid::new(grid_size, offset_x, offset_y, grid_w, grid_h, num_layers);

    // Mark pads as obstacles on their respective layers
    mark_pads(&mut grid, design, &signal_layers, clearance);

    // Mark existing wiring as obstacles
    mark_existing_wires(&mut grid, design, &signal_layers, clearance);

    let clearance_cells = (clearance / grid_size).max(1);
    let trace_cells = (trace_width / grid_size / 2).max(0);
    let pad_radius_cells = clearance_cells + trace_cells;

    let mut result = RoutingResult {
        wires: Vec::new(),
        vias: Vec::new(),
        unrouted: Vec::new(),
    };

    // For each net, gather pad positions and route between them
    for net in &design.nets {
        if net.pins.len() < 2 {
            continue;
        }

        let pad_positions: Vec<Option<(i64, i64, String)>> = net
            .pins
            .iter()
            .map(|pin_ref| {
                crate::dsn::get_pad_position(design, &pin_ref.component, &pin_ref.pin)
            })
            .collect();

        let valid_pads: Vec<(i64, i64, usize)> = pad_positions
            .iter()
            .filter_map(|pos| pos.as_ref())
            .filter_map(|(x, y, layer_name)| {
                let layer_idx = layer_index(design, layer_name, &signal_layers);
                Some((*x, *y, layer_idx))
            })
            .collect();

        if valid_pads.len() < 2 {
            result.unrouted.push(net.name.clone());
            continue;
        }

        // Route: connect pads sequentially (first to second, then extend to third, etc.)
        let mut routed_cells: HashSet<(i32, i32, usize)> = HashSet::new();
        // Start from first pad
        let (fx, fy, fl) = valid_pads[0];
        let (fgx, fgy) = grid.dsn_to_grid(fx, fy);
        routed_cells.insert((fgx as i32, fgy as i32, fl));

        let mut net_routed = true;
        let mut net_wires: Vec<RoutedWire> = Vec::new();
        let mut net_vias: Vec<RoutedVia> = Vec::new();

        for pad_idx in 1..valid_pads.len() {
            let (tx, ty, tl) = valid_pads[pad_idx];
            let (tgx, tgy) = grid.dsn_to_grid(tx, ty);
            let target_layer = tl;

            // Build start cells from already-routed positions
            let start_cells: Vec<(i32, i32, usize)> = routed_cells.iter().copied().collect();

            // Target: a small area around the target pad on the target layer
            let mut target_cells: HashSet<(i32, i32, usize)> = HashSet::new();
            for dy in -pad_radius_cells..=pad_radius_cells {
                for dx in -pad_radius_cells..=pad_radius_cells {
                    let nx = tgx + dx;
                    let ny = tgy + dy;
                    if grid.in_bounds(nx, ny) {
                        target_cells.insert((nx as i32, ny as i32, target_layer));
                        // Also accept on any signal layer (via)
                        for &sl in &signal_layers {
                            target_cells.insert((nx as i32, ny as i32, sl));
                        }
                    }
                }
            }

            // BFS
            let path = bfs(&grid, &start_cells, &target_cells, &signal_layers);

            match path {
                Some(p) => {
                    // Convert path to wires/vias
                    let (w, v) = path_to_wires_and_vias(&p, &grid, &net.name, trace_width, design);
                    net_wires.extend(w);
                    net_vias.extend(v);

                    // Mark path as obstacle
                    for state in &p {
                        routed_cells.insert((state.gx, state.gy, state.layer as usize));
                        // Mark with clearance on grid
                        for dy in -pad_radius_cells..=pad_radius_cells {
                            for dx in -pad_radius_cells..=pad_radius_cells {
                                let nx = state.gx as i64 + dx;
                                let ny = state.gy as i64 + dy;
                                if grid.in_bounds(nx, ny) {
                                    grid.set_obstacle(state.layer as usize, nx, ny);
                                }
                            }
                        }
                    }
                    // Also add target cell to routed
                    routed_cells.insert((tgx as i32, tgy as i32, tl));
                }
                None => {
                    net_routed = false;
                }
            }
        }

        if net_routed {
            result.wires.extend(net_wires);
            result.vias.extend(net_vias);
        } else {
            result.unrouted.push(net.name.clone());
        }
    }

    result
}

fn layer_index(design: &DsnDesign, layer_name: &str, signal_layers: &[usize]) -> usize {
    if let Some(l) = design.layers.iter().find(|l| l.name == layer_name) {
        return l.index;
    }
    // Default: first signal layer
    signal_layers.first().copied().unwrap_or(0)
}

fn mark_pads(grid: &mut Grid, design: &DsnDesign, signal_layers: &[usize], clearance: i64) {
    let grid_size = grid.grid_size;
    let clearance_cells = (clearance / grid_size).max(1);

    for comp in &design.components {
        let image = match design.images.get(&comp.image_name) {
            Some(i) => i,
            None => continue,
        };
        for place in &comp.places {
            let comp_rot = place.rotation.to_radians();
            for pin in &image.pins {
                let pin_rot = pin.rotation.to_radians();
                let rx = pin.x as f64 * pin_rot.cos() - pin.y as f64 * pin_rot.sin();
                let ry = pin.x as f64 * pin_rot.sin() + pin.y as f64 * pin_rot.cos();
                let fx = rx * comp_rot.cos() - ry * comp_rot.sin();
                let fy = rx * comp_rot.sin() + ry * comp_rot.cos();
                let fx = if place.side == Side::Back { -fx } else { fx };

                let abs_x = place.x + fx as i64;
                let abs_y = place.y + fy as i64;

                let (gx, gy) = grid.dsn_to_grid(abs_x, abs_y);

                // Determine pad size from padstack
                let pad_radius = design
                    .padstacks
                    .get(&pin.padstack_name)
                    .and_then(|ps| ps.shapes.first())
                    .map(|shape| match shape {
                        PadShape::Circle { diameter, .. } => diameter / 2 / grid_size + clearance_cells,
                        PadShape::Rect { x1, y1, x2, y2, .. } => {
                            let w = (x2 - x1).abs();
                            let h = (y2 - y1).abs();
                            w.max(h) / 2 / grid_size + clearance_cells
                        }
                        PadShape::Oval { width, height, .. } => {
                            width.max(height) / 2 / grid_size + clearance_cells
                        }
                        PadShape::Path { width, .. } => width / 2 / grid_size + clearance_cells,
                    })
                    .unwrap_or(clearance_cells + 1);

                // Mark on all signal layers (through-hole or "*.Cu")
                let pad_layer = design
                    .padstacks
                    .get(&pin.padstack_name)
                    .and_then(|ps| ps.shapes.first())
                    .map(|shape| match shape {
                        PadShape::Circle { layer, .. } => layer.as_str(),
                        PadShape::Rect { layer, .. } => layer.as_str(),
                        PadShape::Oval { layer, .. } => layer.as_str(),
                        PadShape::Path { layer, .. } => layer.as_str(),
                    })
                    .unwrap_or("*.Cu");

                if pad_layer == "*.Cu" {
                    for &sl in signal_layers {
                        grid.mark_circle(sl, gx, gy, pad_radius);
                    }
                } else {
                    let li = layer_index(design, pad_layer, signal_layers);
                    grid.mark_circle(li, gx, gy, pad_radius);
                }
            }
        }
    }
}

fn mark_existing_wires(
    grid: &mut Grid,
    design: &DsnDesign,
    signal_layers: &[usize],
    clearance: i64,
) {
    let grid_size = grid.grid_size;
    let clearance_cells = (clearance / grid_size).max(1);

    for wire in &design.wiring {
        let li = layer_index(design, &wire.layer, signal_layers);
        let trace_r = (wire.width / 2 / grid_size).max(0) + clearance_cells;
        for pt in &wire.points {
            let (gx, gy) = grid.dsn_to_grid(pt.x, pt.y);
            grid.mark_circle(li, gx, gy, trace_r);
        }
    }
}
