use openrouting::dsn;
use openrouting::router;

fn main() {
    let dsn_str = r#"
(pcb "test"
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
      (place "W7" 30000 48000 front 0)
      (place "W8" 30000 52000 front 0)
      (place "W9" 30000 56000 front 0)
      (place "W10" 30000 60000 front 0)
      (place "W11" 30000 64000 front 0)
      (place "W12" 30000 68000 front 0)
    )
    (component "R"
      (place "C1A" 8000 6000 front 0)
      (place "C1B" 52000 6000 front 0)
      (place "C1C" 52000 36000 front 0)
      (place "C2A" 8000 10000 front 0)
      (place "C2B" 52000 10000 front 0)
      (place "C2C" 52000 34000 front 0)
      (place "M1L" 18000 36000 front 0)
      (place "M1R" 42000 36000 front 0)
      (place "M2L" 18000 34000 front 0)
      (place "M2R" 42000 34000 front 0)
    )
  )
  (network
    (net "CAN1" (pins C1A-2 C1B-1 C1C-2))
    (net "CAN2" (pins C2A-2 C2B-1 C2C-2))
    (net "MUST1" (pins M1L-2 M1R-1))
    (net "MUST2" (pins M2L-2 M2R-1))
  )
  (wiring)
)
"#;

    let design = dsn::parse_dsn(dsn_str).expect("parse");
    
    // Run 10 times to check determinism
    println!("Testing determinism (10 runs):");
    for i in 0..10 {
        let single = router::route_single_pass(&design, &[]);
        let multi = router::route(&design);
        let total = design.nets.len();
        let s_ok = total - single.unrouted.len();
        let m_ok = total - multi.unrouted.len();
        println!("  Run {}: single={}/{} unrouted={:?} | multi={}/{} unrouted={:?}",
            i+1, s_ok, total, single.unrouted, m_ok, total, multi.unrouted);
    }
}
