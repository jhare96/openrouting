// Quick diagnostic: what grid parameters does the smoothieboard get?
use std::fs;

fn main() {
    let content = fs::read_to_string("benchmarks/smoothieboard.dsn").unwrap();
    let design = openrouting::dsn::parse_dsn(&content).unwrap();
    
    let trace_width = design.rules.trace_width.max(1);
    let clearance = design.rules.clearance.max(1);
    let mut grid_size = trace_width.max(clearance);
    let board_w = (design.boundary.max_x - design.boundary.min_x).max(1);
    let board_h = (design.boundary.max_y - design.boundary.min_y).max(1);
    while board_w / grid_size > 500 || board_h / grid_size > 500 {
        grid_size = (grid_size as f64 * 1.5) as i64;
    }
    grid_size = grid_size.max(1);
    let grid_w = ((board_w + grid_size - 1) / grid_size) as usize + 1;
    let grid_h = ((board_h + grid_size - 1) / grid_size) as usize + 1;
    
    let clearance_cells = (clearance / grid_size).max(1);
    let trace_cells = (trace_width / grid_size / 2).max(0);
    let pad_radius_cells = clearance_cells + trace_cells;
    
    println!("Board: {}x{} (boundary: ({},{}) to ({},{}))", board_w, board_h, 
        design.boundary.min_x, design.boundary.min_y, design.boundary.max_x, design.boundary.max_y);
    println!("Rules: trace_width={}, clearance={}", trace_width, clearance);
    println!("Grid: {}x{} cells, grid_size={}", grid_w, grid_h, grid_size);
    println!("clearance_cells={}, trace_cells={}, pad_radius_cells={}", 
        clearance_cells, trace_cells, pad_radius_cells);
    
    // Signal layers
    let signal_layers: Vec<_> = design.layers.iter()
        .filter(|l| l.layer_type == "signal")
        .collect();
    println!("\nLayers ({} total):", design.layers.len());
    for l in &design.layers {
        println!("  [{}] {} ({})", l.index, l.name, l.layer_type);
    }
    println!("Signal layers: {:?}", signal_layers.iter().map(|l| &l.name).collect::<Vec<_>>());
    
    // Padstack analysis
    println!("\nPadstack analysis:");
    let mut moat_count = 0;
    for (name, ps) in &design.padstacks {
        if let Some(shape) = ps.shapes.first() {
            let pad_radius = match shape {
                openrouting::dsn::PadShape::Circle { diameter, .. } => diameter / 2 / grid_size + clearance_cells,
                openrouting::dsn::PadShape::Rect { x1, y1, x2, y2, .. } => {
                    let w = (x2 - x1).abs();
                    let h = (y2 - y1).abs();
                    w.max(h) / 2 / grid_size + clearance_cells
                }
                openrouting::dsn::PadShape::Oval { width, height, .. } => {
                    width.max(height) / 2 / grid_size + clearance_cells
                }
                _ => clearance_cells + 1,
            };
            let layer = match shape {
                openrouting::dsn::PadShape::Circle { layer, .. } => layer.as_str(),
                openrouting::dsn::PadShape::Rect { layer, .. } => layer.as_str(),
                openrouting::dsn::PadShape::Oval { layer, .. } => layer.as_str(),
                _ => "?",
            };
            let target_radius = pad_radius_cells;
            let has_moat = layer == "*.Cu" && pad_radius > target_radius + 1;
            if has_moat { moat_count += 1; }
            if pad_radius > 2 || has_moat {
                println!("  {}: pad_radius={}, target_radius={}, layer={} {}", 
                    name, pad_radius, target_radius, layer,
                    if has_moat { "*** MOAT ***" } else { "" });
            }
        }
    }
    println!("\nTotal padstacks with moat: {}", moat_count);

    // Count nets by pin count
    let mut pin_counts: Vec<usize> = design.nets.iter().map(|n| n.pins.len()).collect();
    pin_counts.sort();
    println!("\nNet pin counts: min={}, max={}, median={}", 
        pin_counts.first().unwrap_or(&0),
        pin_counts.last().unwrap_or(&0),
        pin_counts.get(pin_counts.len()/2).unwrap_or(&0));
    let singles = pin_counts.iter().filter(|&&c| c < 2).count();
    println!("Single-pin nets (skipped): {}", singles);
    let routables = pin_counts.iter().filter(|&&c| c >= 2).count();
    println!("Routable nets (>=2 pins): {}", routables);
}
