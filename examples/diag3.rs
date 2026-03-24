use openrouting::dsn;
use openrouting::router;

fn main() {
    let content = std::fs::read_to_string("benchmarks/smoothieboard.dsn").unwrap();
    let design = dsn::parse_dsn(&content).unwrap();

    println!("Design: {} nets, {} layers", design.nets.len(), design.layers.len());
    println!("Rules: trace_width={}, clearance={}", design.rules.trace_width, design.rules.clearance);
    
    let trace_width = design.rules.trace_width.max(1);
    let clearance = design.rules.clearance.max(1);
    let grid_size = trace_width.max(clearance);
    let board_w = (design.boundary.max_x - design.boundary.min_x).max(1);
    let board_h = (design.boundary.max_y - design.boundary.min_y).max(1);
    let grid_w = ((board_w + grid_size - 1) / grid_size) as usize + 1;
    let grid_h = ((board_h + grid_size - 1) / grid_size) as usize + 1;
    println!("Grid: {}x{}, grid_size={}", grid_w, grid_h, grid_size);
    
    // Check how many nets have < 2 valid pads
    let mut insufficient_pads = 0;
    let mut total_routable = 0;
    for net in &design.nets {
        if net.pins.len() < 2 { continue; }
        total_routable += 1;
        
        let valid_count = net.pins.iter().filter(|pin_ref| {
            dsn::get_pad_position(&design, &pin_ref.component, &pin_ref.pin).is_some()
        }).count();
        
        if valid_count < 2 {
            insufficient_pads += 1;
            println!("Insufficient pads for net '{}': {}/{} valid", net.name, valid_count, net.pins.len());
            for pin_ref in &net.pins {
                let pos = dsn::get_pad_position(&design, &pin_ref.component, &pin_ref.pin);
                println!("  {}-{}: {}", pin_ref.component, pin_ref.pin, 
                    if pos.is_some() { "OK" } else { "NOT FOUND" });
            }
        }
    }
    println!("\nTotal nets with >=2 pins: {}", total_routable);
    println!("Nets with insufficient valid pads: {}", insufficient_pads);
    println!("Routable: {}", total_routable - insufficient_pads);
    
    // Run single pass and analyze failures
    let result = router::route_single_pass(&design, &[]);
    let routed = design.nets.len() - result.unrouted.len();
    println!("\nSingle pass: {}/{} routed, {} unrouted", routed, total_routable, result.unrouted.len());
    
    // Run multi-pass
    let result = router::route(&design);
    let routed = design.nets.len() - result.unrouted.len();
    println!("Multi pass: {}/{} routed, {} unrouted", routed, total_routable, result.unrouted.len());
    for name in &result.unrouted {
        if let Some(net) = design.nets.iter().find(|n| &n.name == name) {
            let valid = net.pins.iter().filter(|p| dsn::get_pad_position(&design, &p.component, &p.pin).is_some()).count();
            println!("  {} ({} pins, {} valid)", name, net.pins.len(), valid);
        }
    }
}
