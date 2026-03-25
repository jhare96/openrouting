use std::collections::{BinaryHeap, HashSet};
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

    /// Clear obstacles in a circle of radius `r` around (cx, cy).
    fn clear_circle(&mut self, layer: usize, cx: i64, cy: i64, r: i64) {
        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    let gx = cx + dx;
                    let gy = cy + dy;
                    if self.in_bounds(gx, gy) {
                        self.obstacles[layer][gy as usize * self.width + gx as usize] = false;
                    }
                }
            }
        }
    }
}

// ─── BFS workspace (flat arrays with generation counting) ─────────────────────

/// Reusable workspace for BFS/A* searches. Uses flat arrays indexed by
/// (layer * width * height + gy * width + gx) for O(1) lookups instead of
/// HashMap. Generation counting avoids expensive array resets between searches.
struct BfsWorkspace {
    dist: Vec<u32>,
    prev: Vec<u32>,
    generation: Vec<u16>,
    current_gen: u16,
    target: Vec<bool>,
    target_indices: Vec<usize>,
    width: usize,
    height: usize,
}

impl BfsWorkspace {
    fn new(width: usize, height: usize, num_layers: usize) -> Self {
        let size = width * height * num_layers;
        BfsWorkspace {
            dist: vec![u32::MAX; size],
            prev: vec![u32::MAX; size],
            generation: vec![0; size],
            current_gen: 0,
            target: vec![false; size],
            target_indices: Vec::new(),
            width,
            height,
        }
    }

    /// Increment generation to invalidate all previous dist/prev entries.
    fn new_search(&mut self) {
        self.current_gen = self.current_gen.wrapping_add(1);
        if self.current_gen == 0 {
            self.generation.fill(0);
            self.current_gen = 1;
        }
    }

    fn mark_target(&mut self, gx: i32, gy: i32, layer: usize) {
        let idx = self.idx(gx, gy, layer);
        if !self.target[idx] {
            self.target[idx] = true;
            self.target_indices.push(idx);
        }
    }

    fn clear_targets(&mut self) {
        for &idx in &self.target_indices {
            self.target[idx] = false;
        }
        self.target_indices.clear();
    }

    #[inline(always)]
    fn idx(&self, gx: i32, gy: i32, layer: usize) -> usize {
        layer * self.width * self.height + (gy as usize) * self.width + (gx as usize)
    }

    #[inline(always)]
    fn get_dist(&self, idx: usize) -> u32 {
        if self.generation[idx] == self.current_gen {
            self.dist[idx]
        } else {
            u32::MAX
        }
    }

    #[inline(always)]
    fn get_prev(&self, idx: usize) -> u32 {
        if self.generation[idx] == self.current_gen {
            self.prev[idx]
        } else {
            u32::MAX
        }
    }

    #[inline(always)]
    fn set_dist_prev(&mut self, idx: usize, dist: u32, prev: u32) {
        self.dist[idx] = dist;
        self.prev[idx] = prev;
        self.generation[idx] = self.current_gen;
    }

    fn decode_index(&self, idx: usize) -> State {
        let layer_size = self.width * self.height;
        let layer = (idx / layer_size) as u8;
        let rem = idx % layer_size;
        let gy = (rem / self.width) as i32;
        let gx = (rem % self.width) as i32;
        State { gx, gy, layer }
    }
}

// ─── A* maze router ──────────────────────────────────────────────────────────

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

/// Octile distance heuristic for A* (admissible with costs 10/14).
#[inline(always)]
fn heuristic(gx: i32, gy: i32, tcx: i32, tcy: i32) -> u32 {
    let dx = (gx - tcx).unsigned_abs();
    let dy = (gy - tcy).unsigned_abs();
    let diag = dx.min(dy);
    let straight = dx.max(dy) - diag;
    straight * 10 + diag * 14
}

fn bfs(
    grid: &Grid,
    start_cells: &[(i32, i32, usize)],
    target_center: (i32, i32),
    signal_layers: &[usize],
    ws: &mut BfsWorkspace,
    heap: &mut BinaryHeap<Reverse<(u32, u32, State)>>,
    via_cost: u32,
) -> Option<Vec<State>> {
    if start_cells.is_empty() || ws.target_indices.is_empty() {
        return None;
    }

    heap.clear();
    let (tcx, tcy) = target_center;

    for &(gx, gy, layer) in start_cells {
        let s = State { gx, gy, layer: layer as u8 };
        let idx = ws.idx(gx, gy, layer);
        ws.set_dist_prev(idx, 0, u32::MAX);
        let h = heuristic(gx, gy, tcx, tcy);
        heap.push(Reverse((h, 0, s)));
    }

    while let Some(Reverse((_f_cost, g_cost, cur))) = heap.pop() {
        let cur_idx = ws.idx(cur.gx, cur.gy, cur.layer as usize);

        // Skip stale heap entries (a shorter path was already found)
        if ws.get_dist(cur_idx) < g_cost {
            continue;
        }

        if ws.target[cur_idx] {
            // Backtrack path
            let mut path = vec![cur];
            let mut idx = cur_idx;
            loop {
                let p = ws.get_prev(idx);
                if p == u32::MAX { break; }
                path.push(ws.decode_index(p as usize));
                idx = p as usize;
            }
            path.reverse();
            return Some(path);
        }

        // Moves on same layer
        for &(dx, dy) in &DIRS {
            let nx = cur.gx + dx;
            let ny = cur.gy + dy;
            if !grid.in_bounds(nx as i64, ny as i64) {
                continue;
            }
            let ns_idx = ws.idx(nx, ny, cur.layer as usize);
            if grid.is_obstacle(cur.layer as usize, nx as i64, ny as i64) && !ws.target[ns_idx] {
                continue;
            }
            let move_cost = if dx != 0 && dy != 0 { 14u32 } else { 10u32 };
            let new_g = g_cost + move_cost;
            if new_g < ws.get_dist(ns_idx) {
                ws.set_dist_prev(ns_idx, new_g, cur_idx as u32);
                let h = heuristic(nx, ny, tcx, tcy);
                heap.push(Reverse((new_g + h, new_g, State { gx: nx, gy: ny, layer: cur.layer })));
            }
        }

        // Via: change layer
        for &other_layer in signal_layers {
            if other_layer == cur.layer as usize {
                continue;
            }
            let ns_idx = ws.idx(cur.gx, cur.gy, other_layer);
            if grid.is_obstacle(other_layer, cur.gx as i64, cur.gy as i64) && !ws.target[ns_idx] {
                continue;
            }
            let new_g = g_cost + via_cost;
            if new_g < ws.get_dist(ns_idx) {
                ws.set_dist_prev(ns_idx, new_g, cur_idx as u32);
                let h = heuristic(cur.gx, cur.gy, tcx, tcy);
                heap.push(Reverse((new_g + h, new_g, State { gx: cur.gx, gy: cur.gy, layer: other_layer as u8 })));
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

/// Maximum number of rip-up-and-retry passes before giving up.
const MAX_ROUTING_PASSES: usize = 20;

/// Maximum grid dimension (width or height) in cells. Limits memory usage
/// while still allowing fine resolution for most board sizes.
const MAX_GRID_DIM: i64 = 1500;

pub fn route(design: &DsnDesign) -> RoutingResult {
    // Try multiple initial via costs to find the best starting point
    let initial_via_costs = [50, 40, 60, 30, 70];
    let mut best = route_single_pass_with_via_cost(design, &[], initial_via_costs[0]);

    for &vc in &initial_via_costs[1..] {
        if best.unrouted.is_empty() {
            break;
        }
        let candidate = route_single_pass_with_via_cost(design, &[], vc);
        if candidate.unrouted.len() < best.unrouted.len() {
            best = candidate;
        }
    }

    // Phase 1: Standard rip-up-and-retry with spatial neighbors
    let via_costs = [50, 40, 60, 30, 70, 20, 80, 45, 55, 35, 25, 65, 15, 75];
    let mut stalled_count = 0;
    for pass in 0..MAX_ROUTING_PASSES {
        if best.unrouted.is_empty() {
            break;
        }

        let mut priority_nets: Vec<String> = best.unrouted.clone();
        if stalled_count >= 1 {
            let neighbor_nets = find_spatial_neighbors(design, &best.unrouted, stalled_count);
            for n in &neighbor_nets {
                if !priority_nets.contains(n) {
                    priority_nets.push(n.clone());
                }
            }
        }

        let vc = via_costs[pass % via_costs.len()];
        #[cfg(feature = "verbose")]
        eprintln!(
            "Pass {}: {} unrouted net(s), via_cost={}, retrying with {} priority nets",
            pass + 2,
            best.unrouted.len(),
            vc,
            priority_nets.len(),
        );
        let candidate = route_single_pass_with_via_cost(design, &priority_nets, vc);

        if candidate.unrouted.len() < best.unrouted.len() {
            best = candidate;
            stalled_count = 0;
        } else if candidate.unrouted.len() == best.unrouted.len()
            && candidate.unrouted != best.unrouted
        {
            best = candidate;
            stalled_count += 1;
        } else {
            stalled_count += 1;
        }

        if stalled_count >= 4 {
            break;
        }
    }

    // Phase 2: Transitive rip-up – discover the conflict chain.
    // When nets A fail, route them first → nets B fail. Add B to priority.
    // Route A+B first → nets C fail. Add C. Repeat until all route or we loop.
    if !best.unrouted.is_empty() {
        for vc in &via_costs {
            if best.unrouted.is_empty() {
                break;
            }
            let chain = build_conflict_chain(design, &best.unrouted, *vc);
            if !chain.is_empty() {
                #[cfg(feature = "verbose")]
                eprintln!(
                    "Transitive rip-up: {} conflict chain nets, via_cost={}",
                    chain.len(),
                    vc,
                );
                let candidate = route_single_pass_with_via_cost(design, &chain, *vc);
                if candidate.unrouted.len() < best.unrouted.len() {
                    best = candidate;
                }
                if best.unrouted.is_empty() {
                    break;
                }
            }
        }
    }

    best
}

/// Build a transitive conflict chain: start with unrouted nets, route them first,
/// collect any newly-unrouted nets, add them to the priority chain, repeat.
fn build_conflict_chain(design: &DsnDesign, initial_unrouted: &[String], via_cost: u32) -> Vec<String> {
    let mut chain: Vec<String> = initial_unrouted.to_vec();
    let mut seen_sets: Vec<Vec<String>> = Vec::new();

    for _iter in 0..10 {
        let result = route_single_pass_with_via_cost(design, &chain, via_cost);
        if result.unrouted.is_empty() {
            return chain; // Found a working chain!
        }
        // Add newly-unrouted nets to the chain
        let mut changed = false;
        for net in &result.unrouted {
            if !chain.contains(net) {
                chain.push(net.clone());
                changed = true;
            }
        }
        if !changed {
            // Same nets still unrouted – chain can't help
            break;
        }
        // Check for cycles
        let mut sorted_unrouted = result.unrouted.clone();
        sorted_unrouted.sort();
        if seen_sets.contains(&sorted_unrouted) {
            break;
        }
        seen_sets.push(sorted_unrouted);
    }

    chain
}

/// Find nets whose pins are spatially close to the unrouted nets' pins.
/// These "neighbor" nets are likely competing for the same routing channels.
/// `radius_factor` controls how wide the search area is (increases each stall).
fn find_spatial_neighbors(
    design: &DsnDesign,
    unrouted: &[String],
    radius_factor: usize,
) -> Vec<String> {
    let trace_width = design.rules.trace_width.max(1);
    let clearance = design.rules.clearance.max(1);
    // Search radius: a few trace widths, expanding with each stall
    let radius = (trace_width + clearance) * (5 + radius_factor as i64 * 3);

    // Collect positions of all unrouted net pins
    let mut unrouted_positions: Vec<(i64, i64)> = Vec::new();
    for net in &design.nets {
        if unrouted.contains(&net.name) {
            for pin in &net.pins {
                if let Some((x, y, _)) = crate::dsn::get_pad_position(design, &pin.component, &pin.pin) {
                    unrouted_positions.push((x, y));
                }
            }
        }
    }

    // Find other nets with pins near any unrouted pin
    let mut neighbors: Vec<String> = Vec::new();
    for net in &design.nets {
        if unrouted.contains(&net.name) || neighbors.contains(&net.name) {
            continue;
        }
        'pin_loop: for pin in &net.pins {
            if let Some((x, y, _)) = crate::dsn::get_pad_position(design, &pin.component, &pin.pin) {
                for &(ux, uy) in &unrouted_positions {
                    let dist = ((x - ux).abs()).max((y - uy).abs());
                    if dist <= radius {
                        neighbors.push(net.name.clone());
                        break 'pin_loop;
                    }
                }
            }
        }
    }

    neighbors
}

/// Run a single routing pass over the design.
///
/// Nets whose names appear in `priority_nets` are routed first; the remaining
/// nets are routed in descending pin-count order (the same heuristic used by
/// the original single-pass router).
pub fn route_single_pass(design: &DsnDesign, priority_nets: &[String]) -> RoutingResult {
    route_single_pass_with_via_cost(design, priority_nets, 50)
}

/// Run a single routing pass with a configurable via cost.
fn route_single_pass_with_via_cost(design: &DsnDesign, priority_nets: &[String], via_cost: u32) -> RoutingResult {
    let trace_width = design.rules.trace_width.max(1);
    let clearance = design.rules.clearance.max(1);

    // Compute the bounding box that covers ALL pad positions plus the board boundary.
    // Some pads may be placed outside the board outline (e.g., edge connectors).
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        design.boundary.min_x,
        design.boundary.min_y,
        design.boundary.max_x,
        design.boundary.max_y,
    );
    for net in &design.nets {
        for pin in &net.pins {
            if let Some((x, y, _)) = crate::dsn::get_pad_position(design, &pin.component, &pin.pin) {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    // Add margin for clearance around edge pads
    let margin = clearance * 2;
    min_x -= margin;
    min_y -= margin;
    max_x += margin;
    max_y += margin;

    // Grid size: use half the trace width or clearance for finer resolution.
    // The +1 ensures the result is at least 1 even when both values are 1
    // (integer division of 2/2 would give 1, but 1/2 would give 0 without it).
    let mut grid_size = (trace_width.max(clearance) + 1) / 2;
    let board_w = (max_x - min_x).max(1);
    let board_h = (max_y - min_y).max(1);
    // Increase grid_size until the grid fits within MAX_GRID_DIM cells per axis
    while board_w / grid_size > MAX_GRID_DIM || board_h / grid_size > MAX_GRID_DIM {
        grid_size = (grid_size as f64 * 1.5) as i64;
    }
    grid_size = grid_size.max(1);

    let offset_x = min_x;
    let offset_y = min_y;
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

    // Reusable BFS workspace and heap (avoids per-search allocations)
    let mut ws = BfsWorkspace::new(grid_w, grid_h, num_layers);
    let mut heap: BinaryHeap<Reverse<(u32, u32, State)>> = BinaryHeap::new();

    let mut result = RoutingResult {
        wires: Vec::new(),
        vias: Vec::new(),
        unrouted: Vec::new(),
    };

    // Build the priority set for O(1) lookups
    let priority_set: HashSet<&str> = priority_nets.iter().map(|s| s.as_str()).collect();

    // Sort nets: priority nets first (in the order given), then remaining nets by
    // descending pin count. Routing large nets first gives them access to more
    // channels, while small 2-pin nets are more flexible and can route around.
    let mut sorted_nets: Vec<&crate::dsn::Net> = design.nets.iter().collect();
    sorted_nets.sort_by(|a, b| {
        let a_pri = priority_set.contains(a.name.as_str());
        let b_pri = priority_set.contains(b.name.as_str());
        b_pri.cmp(&a_pri).then_with(|| {
            if a_pri && b_pri {
                // Among priority nets: preserve the order from priority_nets
                let a_idx = priority_nets.iter().position(|n| n == &a.name).unwrap_or(usize::MAX);
                let b_idx = priority_nets.iter().position(|n| n == &b.name).unwrap_or(usize::MAX);
                a_idx.cmp(&b_idx)
            } else {
                // Non-priority: descending pin count (large nets first)
                b.pins.len().cmp(&a.pins.len())
            }
        })
    });

    // For each net, gather pad positions and route between them
    for net in &sorted_nets {
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
            .map(|(x, y, layer_name)| {
                let layer_idx = layer_index(design, layer_name, &signal_layers);
                (*x, *y, layer_idx)
            })
            .collect();

        if valid_pads.len() < 2 {
            result.unrouted.push(net.name.clone());
            continue;
        }

        // Collect pad obstacle info for this net: (grid_x, grid_y, layer, obstacle_radius)
        let net_pad_obstacles: Vec<(i64, i64, Vec<usize>, i64)> = net
            .pins
            .iter()
            .filter_map(|pin_ref| {
                let pos = crate::dsn::get_pad_position(design, &pin_ref.component, &pin_ref.pin)?;
                let (gx, gy) = grid.dsn_to_grid(pos.0, pos.1);

                // Look up padstack to get obstacle radius and layers
                let comp = design.components.iter().find(|c| c.places.iter().any(|p| p.reference == pin_ref.component))?;
                let image = design.images.get(&comp.image_name)?;
                let pin = image.pins.iter().find(|p| p.pin_number == pin_ref.pin)?;
                let padstack = design.padstacks.get(&pin.padstack_name)?;

                let pad_radius = max_pad_radius(&padstack.shapes, grid_size, clearance_cells);

                // Determine which layers this pad exists on
                let mut layers = Vec::new();
                for shape in &padstack.shapes {
                    let layer_name = match shape {
                        PadShape::Circle { layer, .. } => layer.as_str(),
                        PadShape::Rect { layer, .. } => layer.as_str(),
                        PadShape::Oval { layer, .. } => layer.as_str(),
                        PadShape::Polygon { layer, .. } => layer.as_str(),
                        PadShape::Path { layer, .. } => layer.as_str(),
                    };
                    if layer_name == "*.Cu" {
                        layers.extend(signal_layers.iter().copied());
                        break;
                    } else {
                        let li = layer_index(design, layer_name, &signal_layers);
                        if !layers.contains(&li) {
                            layers.push(li);
                        }
                    }
                }
                if layers.is_empty() {
                    layers.extend(signal_layers.iter().copied());
                }

                Some((gx, gy, layers, pad_radius))
            })
            .collect();

        // Clear own-net pad obstacles so BFS can start/end at pads
        for &(gx, gy, ref layers, r) in &net_pad_obstacles {
            for &layer in layers {
                grid.clear_circle(layer, gx, gy, r);
            }
        }

        // Determine which pads are through-hole (multi-layer)
        let pad_is_through_hole: Vec<bool> = net
            .pins
            .iter()
            .map(|pin_ref| {
                let comp = design.components.iter().find(|c| c.places.iter().any(|p| p.reference == pin_ref.component));
                let is_th = comp.and_then(|c| {
                    let image = design.images.get(&c.image_name)?;
                    let pin = image.pins.iter().find(|p| p.pin_number == pin_ref.pin)?;
                    let ps = design.padstacks.get(&pin.padstack_name)?;
                    // Through-hole if pad has shapes on more than one layer,
                    // or if it has a single shape on the special "*.Cu" layer (all copper layers).
                    let multi_layer = ps.shapes.len() > 1;
                    let wildcard_cu = ps.shapes.iter().any(|shape| {
                        let layer_name = match shape {
                            PadShape::Circle { layer, .. } => layer.as_str(),
                            PadShape::Rect { layer, .. } => layer.as_str(),
                            PadShape::Oval { layer, .. } => layer.as_str(),
                            PadShape::Polygon { layer, .. } => layer.as_str(),
                            PadShape::Path { layer, .. } => layer.as_str(),
                        };
                        layer_name == "*.Cu"
                    });
                    Some(multi_layer || wildcard_cu)
                });
                is_th.unwrap_or(false)
            })
            .collect();

        // Route: connect pads sequentially (first to second, then extend to third, etc.)
        let mut routed_cells: HashSet<(i32, i32, usize)> = HashSet::new();
        // Start from first pad – for TH pads, add start cells on all signal layers
        let (fx, fy, fl) = valid_pads[0];
        let (fgx, fgy) = grid.dsn_to_grid(fx, fy);
        if pad_is_through_hole.first().copied().unwrap_or(false) {
            for &sl in &signal_layers {
                routed_cells.insert((fgx as i32, fgy as i32, sl));
            }
        } else {
            routed_cells.insert((fgx as i32, fgy as i32, fl));
        }

        let mut net_routed = true;
        let mut net_wires: Vec<RoutedWire> = Vec::new();
        let mut net_vias: Vec<RoutedVia> = Vec::new();

        // Track valid_pads index for through-hole check (skip first, so i=0 → second pad)
        for (i, &(tx, ty, tl)) in valid_pads.iter().skip(1).enumerate() {
            let (tgx, tgy) = grid.dsn_to_grid(tx, ty);
            let target_layer = tl;

            // Build start cells from already-routed positions
            let start_cells: Vec<(i32, i32, usize)> = routed_cells.iter().copied().collect();

            // Target: a small area around the target pad on all signal layers
            ws.new_search();
            for dy in -pad_radius_cells..=pad_radius_cells {
                for dx in -pad_radius_cells..=pad_radius_cells {
                    let nx = tgx + dx;
                    let ny = tgy + dy;
                    if grid.in_bounds(nx, ny) {
                        ws.mark_target(nx as i32, ny as i32, target_layer);
                        // Also accept on any signal layer (via)
                        for &sl in &signal_layers {
                            ws.mark_target(nx as i32, ny as i32, sl);
                        }
                    }
                }
            }

            // A* search
            let path = bfs(&grid, &start_cells, (tgx as i32, tgy as i32), &signal_layers, &mut ws, &mut heap, via_cost);
            ws.clear_targets();

            match path {
                Some(p) => {
                    // Convert path to wires/vias
                    let (w, v) = path_to_wires_and_vias(&p, &grid, &net.name, trace_width, design);
                    net_wires.extend(w);
                    net_vias.extend(v);

                    // Mark path as obstacle (circular clearance zone)
                    for state in &p {
                        routed_cells.insert((state.gx, state.gy, state.layer as usize));
                        let clearance_r2 = clearance_cells * clearance_cells;
                        for dy in -clearance_cells..=clearance_cells {
                            for dx in -clearance_cells..=clearance_cells {
                                if dx * dx + dy * dy <= clearance_r2 {
                                    let nx = state.gx as i64 + dx;
                                    let ny = state.gy as i64 + dy;
                                    if grid.in_bounds(nx, ny) {
                                        grid.set_obstacle(state.layer as usize, nx, ny);
                                    }
                                }
                            }
                        }
                    }
                    // Also add target cell to routed (on all signal layers for TH pads)
                    let target_is_th = pad_is_through_hole.get(i + 1).copied().unwrap_or(false);
                    if target_is_th {
                        for &sl in &signal_layers {
                            routed_cells.insert((tgx as i32, tgy as i32, sl));
                        }
                    } else {
                        routed_cells.insert((tgx as i32, tgy as i32, tl));
                    }
                }
                None => {
                    net_routed = false;
                }
            }
        }

        // Always keep successfully routed segments (even if some pins failed).
        // This is critical for large multi-pin nets (e.g., AGND with 211 pins)
        // where most pins connect but a few fail.
        result.wires.extend(net_wires);
        result.vias.extend(net_vias);

        if !net_routed {
            result.unrouted.push(net.name.clone());
        }

        // Restore pad obstacles for other nets
        for &(gx, gy, ref layers, r) in &net_pad_obstacles {
            for &layer in layers {
                grid.mark_circle(layer, gx, gy, r);
            }
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

/// Compute the maximum obstacle radius (in grid cells) across all shapes in a padstack.
fn max_pad_radius(shapes: &[PadShape], grid_size: i64, clearance_cells: i64) -> i64 {
    shapes
        .iter()
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
            PadShape::Polygon { points, .. } => {
                let max_extent = points.iter().map(|p| p.x.abs().max(p.y.abs())).max().unwrap_or(0);
                max_extent / grid_size + clearance_cells
            }
        })
        .max()
        .unwrap_or(clearance_cells + 1)
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

                // Determine pad size from padstack (max across all shapes)
                let padstack = design.padstacks.get(&pin.padstack_name);
                let pad_radius = padstack
                    .map(|ps| max_pad_radius(&ps.shapes, grid_size, clearance_cells))
                    .unwrap_or(clearance_cells + 1);

                // Mark obstacles on each layer where the pad has a shape.
                // Iterate over ALL shapes to handle multi-layer (through-hole) pads.
                let mut marked_any = false;
                if let Some(ps) = padstack {
                    for shape in &ps.shapes {
                        let layer_name = match shape {
                            PadShape::Circle { layer, .. } => layer.as_str(),
                            PadShape::Rect { layer, .. } => layer.as_str(),
                            PadShape::Oval { layer, .. } => layer.as_str(),
                            PadShape::Polygon { layer, .. } => layer.as_str(),
                            PadShape::Path { layer, .. } => layer.as_str(),
                        };
                        if layer_name == "*.Cu" {
                            for &sl in signal_layers {
                                grid.mark_circle(sl, gx, gy, pad_radius);
                            }
                            marked_any = true;
                            break;
                        } else {
                            let li = layer_index(design, layer_name, signal_layers);
                            grid.mark_circle(li, gx, gy, pad_radius);
                            marked_any = true;
                        }
                    }
                }
                if !marked_any {
                    // Fallback: mark on all signal layers
                    for &sl in signal_layers {
                        grid.mark_circle(sl, gx, gy, pad_radius);
                    }
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
