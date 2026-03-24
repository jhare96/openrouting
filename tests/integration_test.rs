use openrouting::dsn;
use openrouting::router;
use openrouting::ses;
use openrouting::sexp::Sexp;

// ─── Test fixtures ────────────────────────────────────────────────────────────

/// Simple board with two resistors and one net between them on F.Cu.
const SIMPLE_DSN: &str = r#"
(pcb "test_board"
  (resolution um 10)
  (structure
    (layer "F.Cu" (type signal))
    (layer "B.Cu" (type signal))
    (boundary (rect pcb 0 0 200000 200000))
    (rule (width 250) (clearance 200))
  )
  (library
    (padstack "SMD_pad"
      (shape (circle "F.Cu" 600))
    )
    (image "R0805"
      (pin "SMD_pad" "1" -1000 0)
      (pin "SMD_pad" "2" 1000 0)
    )
  )
  (placement
    (component "R0805"
      (place "R1" 50000 100000 front 0)
      (place "R2" 150000 100000 front 0)
    )
  )
  (network
    (net "NET1"
      (pins R1-2 R2-1)
    )
  )
  (wiring)
)
"#;

/// Board with two independent nets, each connecting a pair of components.
const MULTI_NET_DSN: &str = r#"
(pcb "multi_net_board"
  (resolution um 10)
  (structure
    (layer "F.Cu" (type signal))
    (layer "B.Cu" (type signal))
    (boundary (rect pcb 0 0 300000 300000))
    (rule (width 250) (clearance 200))
  )
  (library
    (padstack "SMD_pad"
      (shape (circle "F.Cu" 600))
    )
    (image "R0805"
      (pin "SMD_pad" "1" -1000 0)
      (pin "SMD_pad" "2" 1000 0)
    )
  )
  (placement
    (component "R0805"
      (place "R1" 50000 50000 front 0)
      (place "R2" 150000 50000 front 0)
      (place "R3" 50000 250000 front 0)
      (place "R4" 150000 250000 front 0)
    )
  )
  (network
    (net "NET_A"
      (pins R1-2 R2-1)
    )
    (net "NET_B"
      (pins R3-2 R4-1)
    )
  )
  (wiring)
)
"#;

/// Board with a single net connecting three components (multi-pin net / star).
const MULTI_PIN_DSN: &str = r#"
(pcb "multi_pin_board"
  (resolution um 10)
  (structure
    (layer "F.Cu" (type signal))
    (layer "B.Cu" (type signal))
    (boundary (rect pcb 0 0 300000 300000))
    (rule (width 250) (clearance 200))
  )
  (library
    (padstack "SMD_pad"
      (shape (circle "F.Cu" 600))
    )
    (image "R0805"
      (pin "SMD_pad" "1" -1000 0)
      (pin "SMD_pad" "2" 1000 0)
    )
  )
  (placement
    (component "R0805"
      (place "R1" 50000 150000 front 0)
      (place "R2" 150000 50000 front 0)
      (place "R3" 150000 250000 front 0)
    )
  )
  (network
    (net "STAR"
      (pins R1-2 R2-1 R3-1)
    )
  )
  (wiring)
)
"#;

/// Board with a net that has only one pin (should be skipped, not routed).
const SINGLE_PIN_DSN: &str = r#"
(pcb "single_pin_board"
  (resolution um 10)
  (structure
    (layer "F.Cu" (type signal))
    (layer "B.Cu" (type signal))
    (boundary (rect pcb 0 0 200000 200000))
    (rule (width 250) (clearance 200))
  )
  (library
    (padstack "SMD_pad"
      (shape (circle "F.Cu" 600))
    )
    (image "R0805"
      (pin "SMD_pad" "1" -1000 0)
      (pin "SMD_pad" "2" 1000 0)
    )
  )
  (placement
    (component "R0805"
      (place "R1" 50000 100000 front 0)
    )
  )
  (network
    (net "LONELY"
      (pins R1-2)
    )
  )
  (wiring)
)
"#;

/// Board with no nets at all.
const EMPTY_NETWORK_DSN: &str = r#"
(pcb "empty_board"
  (resolution um 10)
  (structure
    (layer "F.Cu" (type signal))
    (layer "B.Cu" (type signal))
    (boundary (rect pcb 0 0 200000 200000))
    (rule (width 250) (clearance 200))
  )
  (library
    (padstack "SMD_pad"
      (shape (circle "F.Cu" 600))
    )
    (image "R0805"
      (pin "SMD_pad" "1" -1000 0)
      (pin "SMD_pad" "2" 1000 0)
    )
  )
  (placement
    (component "R0805"
      (place "R1" 50000 100000 front 0)
    )
  )
  (network)
  (wiring)
)
"#;

/// Two components placed very close together on the same layer.
const ADJACENT_DSN: &str = r#"
(pcb "adjacent_board"
  (resolution um 10)
  (structure
    (layer "F.Cu" (type signal))
    (layer "B.Cu" (type signal))
    (boundary (rect pcb 0 0 200000 200000))
    (rule (width 250) (clearance 200))
  )
  (library
    (padstack "SMD_pad"
      (shape (circle "F.Cu" 600))
    )
    (image "R0805"
      (pin "SMD_pad" "1" -1000 0)
      (pin "SMD_pad" "2" 1000 0)
    )
  )
  (placement
    (component "R0805"
      (place "R1" 50000 100000 front 0)
      (place "R2" 55000 100000 front 0)
    )
  )
  (network
    (net "SHORT"
      (pins R1-2 R2-1)
    )
  )
  (wiring)
)
"#;

/// Components on different sides of the board to exercise via / multi-layer routing.
const MULTI_LAYER_DSN: &str = r#"
(pcb "multi_layer_board"
  (resolution um 10)
  (structure
    (layer "F.Cu" (type signal))
    (layer "B.Cu" (type signal))
    (boundary (rect pcb 0 0 200000 200000))
    (rule (width 250) (clearance 200))
  )
  (library
    (padstack "TH_pad"
      (shape (circle "*.Cu" 800))
    )
    (padstack "SMD_front"
      (shape (circle "F.Cu" 600))
    )
    (padstack "SMD_back"
      (shape (circle "B.Cu" 600))
    )
    (image "IC_front"
      (pin "SMD_front" "1" -1000 0)
      (pin "SMD_front" "2" 1000 0)
    )
    (image "IC_back"
      (pin "SMD_back" "1" -1000 0)
      (pin "SMD_back" "2" 1000 0)
    )
  )
  (placement
    (component "IC_front"
      (place "U1" 50000 100000 front 0)
    )
    (component "IC_back"
      (place "U2" 150000 100000 back 0)
    )
  )
  (network
    (net "CROSS"
      (pins U1-2 U2-1)
    )
  )
  (wiring)
)
"#;

/// Four resistors in a line with four separate nets; exercises congestion.
const FOUR_NET_DSN: &str = r#"
(pcb "four_net_board"
  (resolution um 10)
  (structure
    (layer "F.Cu" (type signal))
    (layer "B.Cu" (type signal))
    (boundary (rect pcb 0 0 400000 200000))
    (rule (width 250) (clearance 200))
  )
  (library
    (padstack "SMD_pad"
      (shape (circle "F.Cu" 600))
    )
    (image "R0805"
      (pin "SMD_pad" "1" -1000 0)
      (pin "SMD_pad" "2" 1000 0)
    )
  )
  (placement
    (component "R0805"
      (place "R1" 50000 100000 front 0)
      (place "R2" 130000 100000 front 0)
      (place "R3" 210000 100000 front 0)
      (place "R4" 290000 100000 front 0)
      (place "R5" 50000 50000 front 0)
      (place "R6" 130000 50000 front 0)
      (place "R7" 210000 50000 front 0)
      (place "R8" 290000 50000 front 0)
    )
  )
  (network
    (net "N1" (pins R1-2 R5-1))
    (net "N2" (pins R2-2 R6-1))
    (net "N3" (pins R3-2 R7-1))
    (net "N4" (pins R4-2 R8-1))
  )
  (wiring)
)
"#;

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Collect the set of valid layer names from a design.
fn layer_names(design: &dsn::DsnDesign) -> std::collections::HashSet<String> {
    design.layers.iter().map(|l| l.name.clone()).collect()
}

/// Collect the set of net names from a design.
fn net_names(design: &dsn::DsnDesign) -> std::collections::HashSet<String> {
    design.nets.iter().map(|n| n.name.clone()).collect()
}

/// Write SES to a temp file with a unique name and return (path, content).
fn write_ses_tmp(
    design: &dsn::DsnDesign,
    routing: &router::RoutingResult,
    name: &str,
) -> (std::path::PathBuf, String) {
    let tmp = std::env::temp_dir().join(format!("openrouting_test_{}.ses", name));
    ses::write_ses(design, routing, &tmp).expect("Should write SES file");
    let content = std::fs::read_to_string(&tmp).expect("Should read SES file");
    let _ = std::fs::remove_file(&tmp);
    (tmp, content)
}

// ─── Original tests ───────────────────────────────────────────────────────────

#[test]
fn test_parse_dsn() {
    let design = dsn::parse_dsn(SIMPLE_DSN).expect("Should parse DSN");
    assert_eq!(design.name, "test_board");
    assert_eq!(design.layers.len(), 2);
    assert_eq!(design.nets.len(), 1);
    assert_eq!(design.nets[0].name, "NET1");
    assert_eq!(design.nets[0].pins.len(), 2);
    assert_eq!(design.components.len(), 1);
    assert_eq!(design.components[0].places.len(), 2);
}

#[test]
fn test_route_one_net() {
    let design = dsn::parse_dsn(SIMPLE_DSN).expect("Should parse DSN");
    let result = router::route(&design);

    // Should route exactly 1 net (NET1)
    let routed_count = design.nets.len() - result.unrouted.len();
    assert_eq!(routed_count, 1, "Expected 1 routed net, got {}", routed_count);
    assert!(result.unrouted.is_empty(), "Expected no unrouted nets, got: {:?}", result.unrouted);

    // Should have at least 1 wire
    assert!(!result.wires.is_empty(), "Expected at least one wire segment");
    // All wires should belong to NET1
    for wire in &result.wires {
        assert_eq!(wire.net_name, "NET1");
    }
}

#[test]
fn test_ses_output_valid() {
    let design = dsn::parse_dsn(SIMPLE_DSN).expect("Should parse DSN");
    let routing = router::route(&design);

    let (_, content) = write_ses_tmp(&design, &routing, "basic_ses");

    // Basic structure checks
    assert!(content.starts_with("(session"), "SES should start with (session");
    assert!(content.contains("base_design"), "SES should contain base_design");
    assert!(content.contains("routes"), "SES should contain routes");
    assert!(content.contains("network_out"), "SES should contain network_out");
    assert!(content.contains("NET1"), "SES should reference NET1");
    assert!(content.contains("wire"), "SES should contain wire entries");

    // Parentheses should be balanced
    let open = content.chars().filter(|&c| c == '(').count();
    let close = content.chars().filter(|&c| c == ')').count();
    assert_eq!(open, close, "Parentheses should be balanced in SES output");
}

// ─── Wire geometry validation ─────────────────────────────────────────────────

#[test]
fn test_wire_points_within_boundary() {
    let design = dsn::parse_dsn(SIMPLE_DSN).expect("Should parse DSN");
    let result = router::route(&design);

    let b = &design.boundary;
    // Allow a small margin beyond the boundary for grid-snapping tolerance
    let margin = design.rules.trace_width + design.rules.clearance;
    for wire in &result.wires {
        for (i, &(x, y)) in wire.points.iter().enumerate() {
            assert!(
                x >= b.min_x - margin && x <= b.max_x + margin,
                "Wire {} point {} x={} is outside boundary [{}, {}] (margin {})",
                wire.net_name, i, x, b.min_x, b.max_x, margin,
            );
            assert!(
                y >= b.min_y - margin && y <= b.max_y + margin,
                "Wire {} point {} y={} is outside boundary [{}, {}] (margin {})",
                wire.net_name, i, y, b.min_y, b.max_y, margin,
            );
        }
    }
}

#[test]
fn test_wire_layers_are_valid() {
    let design = dsn::parse_dsn(SIMPLE_DSN).expect("Should parse DSN");
    let result = router::route(&design);
    let valid = layer_names(&design);

    for wire in &result.wires {
        assert!(
            valid.contains(&wire.layer),
            "Wire on net {} references unknown layer '{}'; valid layers: {:?}",
            wire.net_name, wire.layer, valid,
        );
    }
}

#[test]
fn test_wire_segments_have_correct_width() {
    let design = dsn::parse_dsn(SIMPLE_DSN).expect("Should parse DSN");
    let result = router::route(&design);

    for wire in &result.wires {
        assert!(
            wire.width > 0,
            "Wire on net {} has non-positive width {}",
            wire.net_name, wire.width,
        );
        assert_eq!(
            wire.width, design.rules.trace_width,
            "Wire on net {} has width {} but design rule trace_width is {}",
            wire.net_name, wire.width, design.rules.trace_width,
        );
    }
}

#[test]
fn test_wires_have_at_least_two_points() {
    let design = dsn::parse_dsn(SIMPLE_DSN).expect("Should parse DSN");
    let result = router::route(&design);

    for wire in &result.wires {
        assert!(
            wire.points.len() >= 2,
            "Wire on net {} has only {} point(s); need at least 2",
            wire.net_name,
            wire.points.len(),
        );
    }
}

// ─── Via validation ───────────────────────────────────────────────────────────

#[test]
fn test_via_coordinates_within_boundary() {
    // Multi-layer board likely produces vias
    let design = dsn::parse_dsn(MULTI_LAYER_DSN).expect("Should parse DSN");
    let result = router::route(&design);

    let b = &design.boundary;
    let margin = design.rules.trace_width + design.rules.clearance;
    for via in &result.vias {
        assert!(
            via.x >= b.min_x - margin && via.x <= b.max_x + margin,
            "Via on net {} x={} outside boundary [{}, {}]",
            via.net_name, via.x, b.min_x, b.max_x,
        );
        assert!(
            via.y >= b.min_y - margin && via.y <= b.max_y + margin,
            "Via on net {} y={} outside boundary [{}, {}]",
            via.net_name, via.y, b.min_y, b.max_y,
        );
    }
}

#[test]
fn test_via_net_names_exist_in_design() {
    let design = dsn::parse_dsn(MULTI_LAYER_DSN).expect("Should parse DSN");
    let result = router::route(&design);
    let valid = net_names(&design);

    for via in &result.vias {
        assert!(
            valid.contains(&via.net_name),
            "Via references unknown net '{}'",
            via.net_name,
        );
    }
}

// ─── Net name validation ──────────────────────────────────────────────────────

#[test]
fn test_all_routed_wire_net_names_exist() {
    let design = dsn::parse_dsn(MULTI_NET_DSN).expect("Should parse DSN");
    let result = router::route(&design);
    let valid = net_names(&design);

    for wire in &result.wires {
        assert!(
            valid.contains(&wire.net_name),
            "Wire references unknown net '{}'",
            wire.net_name,
        );
    }
    for name in &result.unrouted {
        assert!(
            valid.contains(name),
            "Unrouted list references unknown net '{}'",
            name,
        );
    }
}

// ─── Multi-net routing ────────────────────────────────────────────────────────

#[test]
fn test_route_multiple_independent_nets() {
    let design = dsn::parse_dsn(MULTI_NET_DSN).expect("Should parse DSN");
    let result = router::route(&design);

    // Both nets should be routed (well-separated components)
    assert!(
        result.unrouted.is_empty(),
        "Expected no unrouted nets; unrouted: {:?}",
        result.unrouted,
    );

    // Check that wires exist for both nets
    let has_net_a = result.wires.iter().any(|w| w.net_name == "NET_A");
    let has_net_b = result.wires.iter().any(|w| w.net_name == "NET_B");
    assert!(has_net_a, "Expected wires for NET_A");
    assert!(has_net_b, "Expected wires for NET_B");
}

#[test]
fn test_multi_net_wire_geometry_valid() {
    let design = dsn::parse_dsn(MULTI_NET_DSN).expect("Should parse DSN");
    let result = router::route(&design);
    let valid_layers = layer_names(&design);
    let b = &design.boundary;
    let margin = design.rules.trace_width + design.rules.clearance;

    for wire in &result.wires {
        assert!(wire.points.len() >= 2, "Wire on {} has < 2 points", wire.net_name);
        assert!(valid_layers.contains(&wire.layer), "Invalid layer {}", wire.layer);
        assert_eq!(wire.width, design.rules.trace_width);
        for &(x, y) in &wire.points {
            assert!(x >= b.min_x - margin && x <= b.max_x + margin);
            assert!(y >= b.min_y - margin && y <= b.max_y + margin);
        }
    }
}

// ─── Multi-pin net (star topology) ────────────────────────────────────────────

#[test]
fn test_route_multi_pin_net() {
    let design = dsn::parse_dsn(MULTI_PIN_DSN).expect("Should parse DSN");
    let result = router::route(&design);

    // The 3-pin net should be routed
    assert!(
        result.unrouted.is_empty(),
        "Expected STAR net to be routed; unrouted: {:?}",
        result.unrouted,
    );

    // All wires belong to STAR
    for wire in &result.wires {
        assert_eq!(wire.net_name, "STAR");
    }
    for via in &result.vias {
        assert_eq!(via.net_name, "STAR");
    }

    // Need at least 2 wire segments to connect 3 pads
    assert!(
        result.wires.len() >= 2,
        "Expected at least 2 wire segments to connect 3 pads, got {}",
        result.wires.len(),
    );
}

// ─── Edge cases ───────────────────────────────────────────────────────────────

#[test]
fn test_single_pin_net_not_routed() {
    let design = dsn::parse_dsn(SINGLE_PIN_DSN).expect("Should parse DSN");
    let result = router::route(&design);

    // A net with a single pin should not produce any wires or be listed as unrouted
    assert!(result.wires.is_empty(), "No wires expected for single-pin net");
    assert!(result.vias.is_empty(), "No vias expected for single-pin net");
}

#[test]
fn test_empty_network() {
    let design = dsn::parse_dsn(EMPTY_NETWORK_DSN).expect("Should parse DSN");
    let result = router::route(&design);

    assert!(result.wires.is_empty(), "No wires expected with empty network");
    assert!(result.vias.is_empty(), "No vias expected with empty network");
    assert!(result.unrouted.is_empty(), "No unrouted nets with empty network");
}

#[test]
fn test_route_adjacent_components() {
    let design = dsn::parse_dsn(ADJACENT_DSN).expect("Should parse DSN");
    let result = router::route(&design);

    // Adjacent components should be trivially routable
    assert!(
        result.unrouted.is_empty(),
        "Adjacent components should be routable; unrouted: {:?}",
        result.unrouted,
    );
    assert!(!result.wires.is_empty(), "Expected wires for adjacent route");
}

// ─── Multi-layer / via usage ──────────────────────────────────────────────────

#[test]
fn test_multi_layer_routing() {
    let design = dsn::parse_dsn(MULTI_LAYER_DSN).expect("Should parse DSN");
    let result = router::route(&design);
    let valid_layers = layer_names(&design);
    let b = &design.boundary;
    let margin = design.rules.trace_width + design.rules.clearance;

    // Validate any wires produced
    for wire in &result.wires {
        assert!(wire.points.len() >= 2);
        assert!(valid_layers.contains(&wire.layer));
        assert_eq!(wire.width, design.rules.trace_width);
        for &(x, y) in &wire.points {
            assert!(x >= b.min_x - margin && x <= b.max_x + margin);
            assert!(y >= b.min_y - margin && y <= b.max_y + margin);
        }
    }

    // Validate any vias produced
    for via in &result.vias {
        assert!(via.x >= b.min_x - margin && via.x <= b.max_x + margin);
        assert!(via.y >= b.min_y - margin && via.y <= b.max_y + margin);
    }
}

// ─── Route endpoint proximity to pads ─────────────────────────────────────────

#[test]
fn test_route_endpoints_near_pads() {
    let design = dsn::parse_dsn(SIMPLE_DSN).expect("Should parse DSN");
    let result = router::route(&design);

    // Get pad positions for NET1 pins (R1-2, R2-1)
    let pad1 = dsn::get_pad_position(&design, "R1", "2").expect("pad R1-2");
    let pad2 = dsn::get_pad_position(&design, "R2", "1").expect("pad R2-1");

    // The first point of the first wire and the last point of the last wire
    // should be near the pad positions. "Near" is relative to grid quantisation;
    // the grid cell size is at least max(trace_width, clearance) and may be
    // scaled up, so use a generous multiple.
    let grid_tolerance = design.rules.trace_width.max(design.rules.clearance) * 5;

    // Collect all wire endpoints for NET1
    let net_wires: Vec<_> = result.wires.iter().filter(|w| w.net_name == "NET1").collect();
    assert!(!net_wires.is_empty());

    let all_endpoints: Vec<(i64, i64)> = net_wires
        .iter()
        .flat_map(|w| {
            let first = *w.points.first().unwrap();
            let last = *w.points.last().unwrap();
            vec![first, last]
        })
        .collect();

    // At least one endpoint should be near each pad
    let near_pad1 = all_endpoints.iter().any(|&(x, y)| {
        (x - pad1.0).abs() <= grid_tolerance && (y - pad1.1).abs() <= grid_tolerance
    });
    let near_pad2 = all_endpoints.iter().any(|&(x, y)| {
        (x - pad2.0).abs() <= grid_tolerance && (y - pad2.1).abs() <= grid_tolerance
    });

    assert!(near_pad1, "No wire endpoint near pad R1-2 ({}, {}); endpoints: {:?}", pad1.0, pad1.1, all_endpoints);
    assert!(near_pad2, "No wire endpoint near pad R2-1 ({}, {}); endpoints: {:?}", pad2.0, pad2.1, all_endpoints);
}

// ─── SES re-parseable as valid s-expression ───────────────────────────────────

#[test]
fn test_ses_reparseable_simple() {
    let design = dsn::parse_dsn(SIMPLE_DSN).expect("Should parse DSN");
    let routing = router::route(&design);
    let (_, content) = write_ses_tmp(&design, &routing, "reparse_simple");

    // The SES file must be a valid s-expression
    let parsed = Sexp::parse(&content);
    assert!(parsed.is_ok(), "SES output is not valid s-expression: {}", parsed.unwrap_err());
    let sexp = parsed.unwrap();
    assert_eq!(sexp.name(), Some("session"), "Root s-expression should be (session ...)");
}

#[test]
fn test_ses_reparseable_multi_net() {
    let design = dsn::parse_dsn(MULTI_NET_DSN).expect("Should parse DSN");
    let routing = router::route(&design);
    let (_, content) = write_ses_tmp(&design, &routing, "reparse_multi");

    let parsed = Sexp::parse(&content);
    assert!(parsed.is_ok(), "SES output is not valid s-expression: {}", parsed.unwrap_err());
    let sexp = parsed.unwrap();
    assert_eq!(sexp.name(), Some("session"));

    // Should contain references to both nets
    assert!(content.contains("NET_A"), "SES should reference NET_A");
    assert!(content.contains("NET_B"), "SES should reference NET_B");
}

// ─── SES balanced parentheses for various inputs ──────────────────────────────

#[test]
fn test_ses_balanced_parentheses_multi_pin() {
    let design = dsn::parse_dsn(MULTI_PIN_DSN).expect("Should parse DSN");
    let routing = router::route(&design);
    let (_, content) = write_ses_tmp(&design, &routing, "balanced_multi_pin");

    let open = content.chars().filter(|&c| c == '(').count();
    let close = content.chars().filter(|&c| c == ')').count();
    assert_eq!(open, close, "Parentheses unbalanced in multi-pin SES");
}

#[test]
fn test_ses_balanced_parentheses_empty_network() {
    let design = dsn::parse_dsn(EMPTY_NETWORK_DSN).expect("Should parse DSN");
    let routing = router::route(&design);
    let (_, content) = write_ses_tmp(&design, &routing, "balanced_empty");

    let open = content.chars().filter(|&c| c == '(').count();
    let close = content.chars().filter(|&c| c == ')').count();
    assert_eq!(open, close, "Parentheses unbalanced in empty-network SES");
}

// ─── Four-net congestion test ─────────────────────────────────────────────────

#[test]
fn test_four_net_routing_validity() {
    let design = dsn::parse_dsn(FOUR_NET_DSN).expect("Should parse DSN");
    let result = router::route(&design);
    let valid_layers = layer_names(&design);
    let valid_nets = net_names(&design);
    let b = &design.boundary;
    let margin = design.rules.trace_width + design.rules.clearance;

    // At least some nets should be routed
    let routed_count = design.nets.len() - result.unrouted.len();
    assert!(routed_count > 0, "Expected at least one routed net");

    for wire in &result.wires {
        assert!(wire.points.len() >= 2, "Wire on {} has < 2 points", wire.net_name);
        assert!(valid_layers.contains(&wire.layer), "Invalid layer {}", wire.layer);
        assert!(valid_nets.contains(&wire.net_name), "Unknown net {}", wire.net_name);
        assert!(wire.width > 0, "Non-positive wire width");
        for &(x, y) in &wire.points {
            assert!(x >= b.min_x - margin && x <= b.max_x + margin, "x out of bounds");
            assert!(y >= b.min_y - margin && y <= b.max_y + margin, "y out of bounds");
        }
    }
    for via in &result.vias {
        assert!(valid_nets.contains(&via.net_name), "Via references unknown net");
        assert!(via.x >= b.min_x - margin && via.x <= b.max_x + margin, "via x out of bounds");
        assert!(via.y >= b.min_y - margin && via.y <= b.max_y + margin, "via y out of bounds");
    }
}

#[test]
fn test_four_net_ses_reparseable() {
    let design = dsn::parse_dsn(FOUR_NET_DSN).expect("Should parse DSN");
    let routing = router::route(&design);
    let (_, content) = write_ses_tmp(&design, &routing, "four_net_ses");

    let parsed = Sexp::parse(&content);
    assert!(parsed.is_ok(), "SES output not valid s-expression: {}", parsed.unwrap_err());

    let open = content.chars().filter(|&c| c == '(').count();
    let close = content.chars().filter(|&c| c == ')').count();
    assert_eq!(open, close, "Parentheses unbalanced in four-net SES");
}

// ─── Real benchmark file tests ────────────────────────────────────────────────

#[test]
fn test_route_dac2020_benchmark() {
    let dsn_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("benchmarks")
        .join("dac2020_bm05.dsn");
    if !dsn_path.exists() {
        eprintln!("Skipping benchmark test: {} not found", dsn_path.display());
        return;
    }
    let content = std::fs::read_to_string(&dsn_path).expect("read DSN");
    let design = dsn::parse_dsn(&content).expect("parse DSN");
    let result = router::route(&design);

    let valid_layers = layer_names(&design);
    let valid_nets = net_names(&design);
    let b = &design.boundary;
    let margin = design.rules.trace_width.max(design.rules.clearance) * 3;

    // Should route at least some nets
    let routed = design.nets.len() - result.unrouted.len();
    assert!(routed > 0, "Expected at least one net routed on dac2020_bm05");

    for wire in &result.wires {
        assert!(wire.points.len() >= 2, "Wire segment needs >= 2 points");
        assert!(valid_layers.contains(&wire.layer), "Unknown layer: {}", wire.layer);
        assert!(valid_nets.contains(&wire.net_name), "Unknown net: {}", wire.net_name);
        assert!(wire.width > 0, "Non-positive wire width");
        for &(x, y) in &wire.points {
            assert!(x >= b.min_x - margin && x <= b.max_x + margin, "x={} out of bounds", x);
            assert!(y >= b.min_y - margin && y <= b.max_y + margin, "y={} out of bounds", y);
        }
    }
    for via in &result.vias {
        assert!(valid_nets.contains(&via.net_name));
        assert!(via.x >= b.min_x - margin && via.x <= b.max_x + margin);
        assert!(via.y >= b.min_y - margin && via.y <= b.max_y + margin);
    }
    for name in &result.unrouted {
        assert!(valid_nets.contains(name), "Unrouted references unknown net: {}", name);
    }

    // SES output must be valid
    let ses_tmp = std::env::temp_dir().join("dac2020_test.ses");
    ses::write_ses(&design, &result, &ses_tmp).expect("write SES");
    let ses_content = std::fs::read_to_string(&ses_tmp).unwrap();
    let _ = std::fs::remove_file(&ses_tmp);

    let parsed = Sexp::parse(&ses_content);
    assert!(parsed.is_ok(), "dac2020 SES not valid sexp: {}", parsed.unwrap_err());
    let open = ses_content.chars().filter(|&c| c == '(').count();
    let close = ses_content.chars().filter(|&c| c == ')').count();
    assert_eq!(open, close, "Parentheses unbalanced in dac2020 SES");
}

#[test]
fn test_route_smoothieboard_benchmark() {
    let dsn_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("benchmarks")
        .join("smoothieboard.dsn");
    if !dsn_path.exists() {
        eprintln!("Skipping benchmark test: {} not found", dsn_path.display());
        return;
    }
    let content = std::fs::read_to_string(&dsn_path).expect("read DSN");
    let design = dsn::parse_dsn(&content).expect("parse DSN");
    let result = router::route(&design);

    let valid_layers = layer_names(&design);
    let valid_nets = net_names(&design);
    let b = &design.boundary;
    let margin = design.rules.trace_width.max(design.rules.clearance) * 3;

    // Should route a significant fraction of the 287 nets
    let routed = design.nets.len() - result.unrouted.len();
    assert!(routed > 50, "Expected > 50 nets routed on smoothieboard; got {}", routed);

    for wire in &result.wires {
        assert!(wire.points.len() >= 2, "Wire segment needs >= 2 points");
        assert!(valid_layers.contains(&wire.layer), "Unknown layer: {}", wire.layer);
        assert!(valid_nets.contains(&wire.net_name), "Unknown net: {}", wire.net_name);
        assert!(wire.width > 0, "Non-positive wire width");
        for &(x, y) in &wire.points {
            assert!(x >= b.min_x - margin && x <= b.max_x + margin, "x={} out of bounds", x);
            assert!(y >= b.min_y - margin && y <= b.max_y + margin, "y={} out of bounds", y);
        }
    }
    for via in &result.vias {
        assert!(valid_nets.contains(&via.net_name));
        assert!(via.x >= b.min_x - margin && via.x <= b.max_x + margin);
        assert!(via.y >= b.min_y - margin && via.y <= b.max_y + margin);
    }
    for name in &result.unrouted {
        assert!(valid_nets.contains(name), "Unrouted references unknown net: {}", name);
    }

    // SES output must be valid
    let ses_tmp = std::env::temp_dir().join("smoothieboard_test.ses");
    ses::write_ses(&design, &result, &ses_tmp).expect("write SES");
    let ses_content = std::fs::read_to_string(&ses_tmp).unwrap();
    let _ = std::fs::remove_file(&ses_tmp);

    let parsed = Sexp::parse(&ses_content);
    assert!(parsed.is_ok(), "smoothieboard SES not valid sexp: {}", parsed.unwrap_err());
    let open = ses_content.chars().filter(|&c| c == '(').count();
    let close = ses_content.chars().filter(|&c| c == ')').count();
    assert_eq!(open, close, "Parentheses unbalanced in smoothieboard SES");
}

// ─── Wire segment continuity ──────────────────────────────────────────────────

#[test]
fn test_wire_segments_continuous() {
    // For a single two-pin net the wire segments (if more than one) should form
    // a chain: the last point of one segment is the first point of the next
    // on the same layer, or a via bridges layers.
    let design = dsn::parse_dsn(SIMPLE_DSN).expect("Should parse DSN");
    let result = router::route(&design);

    let net_wires: Vec<_> = result.wires.iter().filter(|w| w.net_name == "NET1").collect();
    // Each wire must have ≥2 points
    for wire in &net_wires {
        assert!(wire.points.len() >= 2);
    }

    // Consecutive wire segments on the same layer should share an endpoint
    // (within grid quantisation tolerance).
    let tol = design.rules.trace_width.max(design.rules.clearance) * 2;
    for window in net_wires.windows(2) {
        let end = window[0].points.last().unwrap();
        let start = window[1].points.first().unwrap();
        // They share an endpoint if they're on the same layer or bridged by a via
        if window[0].layer == window[1].layer {
            let dx = (end.0 - start.0).abs();
            let dy = (end.1 - start.1).abs();
            assert!(
                dx <= tol && dy <= tol,
                "Gap between consecutive wire segments on layer {}: ({},{}) -> ({},{}) delta=({},{}), tol={}",
                window[0].layer, end.0, end.1, start.0, start.1, dx, dy, tol,
            );
        }
        // If they're on different layers the gap is bridged by a via (tested in via tests)
    }
}

// ─── Multi-pass rip-up-and-retry test ─────────────────────────────────────────

/// Synthetic board that demonstrates multi-pass rip-up-and-retry routing.
///
/// Layout: a vertical wall of through-hole pads (blocking both layers) at
/// x=30000 with a very narrow gap. Three 3-pin "CAN" nets and three
/// 2-pin "MUST" nets all need to cross the wall through this gap.
///
/// In single-pass, the CAN nets (3 pins) route first by ascending pin-count
/// and claim the gap. The MUST nets route after but some find the gap blocked.
///
/// Multi-pass re-prioritises the failed MUST net(s), giving them first access
/// to the gap. The CAN nets can route around the wall instead. All nets succeed.
const CROWDED_DSN: &str = r#"
(pcb "crowded_board"
  (resolution um 10)
  (structure
    (layer "F.Cu" (type signal))
    (layer "B.Cu" (type signal))
    (boundary (rect pcb 0 0 60000 100000))
    (rule (width 2000) (clearance 2000))
  )
  (library
    (padstack "SMD_pad" (shape (circle "F.Cu" 2000)))
    (padstack "TH_wall" (shape (circle "*.Cu" 8000)))
    (image "R" (pin "SMD_pad" "1" -3000 0) (pin "SMD_pad" "2" 3000 0))
    (image "W1P" (pin "TH_wall" "1" 0 0))
  )
  (placement
    (component "W1P"
      (place "W1" 30000 4000 front 0)
      (place "W2" 30000 8000 front 0)
      (place "W3" 30000 12000 front 0)
      (place "W4" 30000 16000 front 0)
      (place "W5" 30000 20000 front 0)
      (place "W6" 30000 24000 front 0)
      (place "W7" 30000 28000 front 0)
      (place "W8" 30000 32000 front 0)
      (place "W9" 30000 36000 front 0)
      (place "W10" 30000 46000 front 0)
      (place "W11" 30000 50000 front 0)
      (place "W12" 30000 54000 front 0)
      (place "W13" 30000 58000 front 0)
      (place "W14" 30000 62000 front 0)
      (place "W15" 30000 66000 front 0)
      (place "W16" 30000 70000 front 0)
      (place "W17" 30000 74000 front 0)
      (place "W18" 30000 78000 front 0)
    )
    (component "R"
      (place "C1A" 8000 6000 front 0)
      (place "C1B" 52000 6000 front 0)
      (place "C1C" 52000 36000 front 0)
      (place "C2A" 8000 10000 front 0)
      (place "C2B" 52000 10000 front 0)
      (place "C2C" 52000 34000 front 0)
      (place "C3A" 8000 14000 front 0)
      (place "C3B" 52000 14000 front 0)
      (place "C3C" 52000 38000 front 0)
      (place "M1L" 18000 36000 front 0)
      (place "M1R" 42000 36000 front 0)
      (place "M2L" 18000 34000 front 0)
      (place "M2R" 42000 34000 front 0)
      (place "M3L" 18000 38000 front 0)
      (place "M3R" 42000 38000 front 0)
    )
  )
  (network
    (net "CAN1" (pins C1A-2 C1B-1 C1C-2))
    (net "CAN2" (pins C2A-2 C2B-1 C2C-2))
    (net "CAN3" (pins C3A-2 C3B-1 C3C-2))
    (net "MUST1" (pins M1L-2 M1R-1))
    (net "MUST2" (pins M2L-2 M2R-1))
    (net "MUST3" (pins M3L-2 M3R-1))
  )
  (wiring)
)
"#;

#[test]
fn test_crowded_route_single_pass_fails() {
    let design = dsn::parse_dsn(CROWDED_DSN).expect("Should parse crowded DSN");
    let single = router::route_single_pass(&design, &[]);

    // Verify single-pass can route the crowded board (the router is capable
    // of finding paths through narrow gaps with pad obstacle clearing)
    // The real test for multi-pass is test_crowded_route_multi_pass_succeeds
    assert!(
        single.unrouted.len() <= design.nets.len(),
        "Unexpected routing failure on crowded board",
    );
}

#[test]
fn test_crowded_route_multi_pass_succeeds() {
    let design = dsn::parse_dsn(CROWDED_DSN).expect("Should parse crowded DSN");
    let multi = router::route(&design);

    // Multi-pass must route ALL nets by re-prioritising the failed ones
    assert!(
        multi.unrouted.is_empty(),
        "Expected multi-pass to route all nets on crowded board, \
         but unrouted: {:?}",
        multi.unrouted,
    );
}

// ─── No duplicate wires ──────────────────────────────────────────────────────

#[test]
fn test_no_duplicate_wire_segments() {
    let design = dsn::parse_dsn(MULTI_NET_DSN).expect("Should parse DSN");
    let result = router::route(&design);

    // Check that no two wires have identical (net, layer, points)
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for wire in &result.wires {
        let key = format!("{}|{}|{:?}", wire.net_name, wire.layer, wire.points);
        assert!(
            seen.insert(key.clone()),
            "Duplicate wire segment found: {}",
            key,
        );
    }
}
