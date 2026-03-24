use openrouting::dsn;
use openrouting::router;
use openrouting::ses;

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

    let tmp = std::env::temp_dir().join("integration_test_output.ses");
    ses::write_ses(&design, &routing, &tmp).expect("Should write SES file");

    let content = std::fs::read_to_string(&tmp).expect("Should read SES file");

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

    let _ = std::fs::remove_file(&tmp);
}
