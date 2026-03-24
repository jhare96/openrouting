use openrouting::dsn;
use openrouting::router;

fn main() {
    let content = std::fs::read_to_string("benchmarks/smoothieboard.dsn").unwrap();
    let design = dsn::parse_dsn(&content).unwrap();

    // Check the unrouted nets - are they all multi-pin nets that need through-hole connections?
    let unrouted = vec![
        "Net-(R9-Pad1)", "Net-(IC2-PadP15)", "Net-(IC3-PadP15)", 
        "Net-(IC5-PadP15)", "Net-(IC6-PadP15)", "Net-(R50-Pad2)", 
        "Net-(R61-Pad2)", "Net-(IC12-PadP15)", "TH1", "AGND",
        "/smoothieboard-5driver_3/TD-", "/smoothieboard-5driver_3/TD+",
        "/smoothieboard-5driver_3/LED2/NINTSEL", "Net-(IC7-Pad2)",
    ];

    for net_name in &unrouted {
        if let Some(net) = design.nets.iter().find(|n| n.name == *net_name) {
            println!("=== {} ({} pins) ===", net_name, net.pins.len());
            for pin in &net.pins {
                let pos = dsn::get_pad_position(&design, &pin.component, &pin.pin);
                if let Some((x, y, layer)) = &pos {
                    // Find padstack type
                    let comp = design.components.iter()
                        .find(|c| c.places.iter().any(|p| p.reference == pin.component));
                    let ps_name = comp.and_then(|c| {
                        let image = design.images.get(&c.image_name)?;
                        let p = image.pins.iter().find(|p| p.pin_number == pin.pin)?;
                        Some(p.padstack_name.clone())
                    }).unwrap_or_default();
                    let ps = design.padstacks.get(&ps_name);
                    let num_shapes = ps.map(|p| p.shapes.len()).unwrap_or(0);
                    println!("  {}-{}: ({}, {}) layer={} padstack={} shapes={}", 
                        pin.component, pin.pin, x, y, layer, ps_name, num_shapes);
                } else {
                    println!("  {}-{}: NOT FOUND", pin.component, pin.pin);
                }
            }
        }
    }
}
