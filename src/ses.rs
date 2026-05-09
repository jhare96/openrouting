use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::fs::File;

use crate::dsn::DsnDesign;
use crate::router::RoutingResult;

fn needs_quotes(s: &str) -> bool {
    s.is_empty()
        || s.chars()
            .any(|c| c.is_whitespace() || c == '(' || c == ')' || c == '"')
}

fn quoted(s: &str) -> String {
    if needs_quotes(s) {
        format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        s.to_string()
    }
}

pub fn write_ses(
    design: &DsnDesign,
    routing: &RoutingResult,
    output_path: &Path,
) -> Result<(), io::Error> {
    let file = File::create(output_path)?;
    let mut w = BufWriter::new(file);

    let design_name = &design.name;
    let dsn_name = format!("{}.dsn", design_name);

    writeln!(w, "(session {}", quoted(design_name))?;
    writeln!(w, "  (base_design {})", quoted(&dsn_name))?;
    writeln!(w, "  (routes")?;
    writeln!(
        w,
        "    (resolution {} {})",
        design.resolution.unit, design.resolution.value
    )?;
    writeln!(w, "    (parser")?;
    writeln!(w, "      (string_quote \")")?;
    writeln!(w, "      (space_in_quoted_tokens on)")?;
    writeln!(w, "      (host_cad \"openrouting\")")?;
    writeln!(w, "      (host_version \"0.1.0\")")?;
    writeln!(w, "    )")?;
    writeln!(w, "    (network_out")?;

    // Group wires and vias by net name
    let mut net_wires: std::collections::HashMap<&str, Vec<&crate::router::RoutedWire>> =
        std::collections::HashMap::new();
    let mut net_vias: std::collections::HashMap<&str, Vec<&crate::router::RoutedVia>> =
        std::collections::HashMap::new();

    for wire in &routing.wires {
        net_wires.entry(wire.net_name.as_str()).or_default().push(wire);
    }
    for via in &routing.vias {
        net_vias.entry(via.net_name.as_str()).or_default().push(via);
    }

    // Emit nets in the order they appear in the design
    for net in &design.nets {
        let name = net.name.as_str();
        let wires = net_wires.get(name);
        let vias = net_vias.get(name);

        if wires.is_none() && vias.is_none() {
            continue;
        }

        writeln!(w, "      (net {}", quoted(name))?;

        if let Some(ws) = wires {
            for wire in ws {
                if wire.points.len() < 2 {
                    continue;
                }
                write!(
                    w,
                    "        (wire (path {} {}",
                    quoted(&wire.layer),
                    wire.width
                )?;
                for (x, y) in &wire.points {
                    write!(w, " {} {}", x, y)?;
                }
                writeln!(w, "))")?;
            }
        }

        if let Some(vs) = vias {
            for via in vs {
                writeln!(
                    w,
                    "        (via {} {} {})",
                    quoted(&via.padstack_name),
                    via.x,
                    via.y
                )?;
            }
        }

        writeln!(w, "      )")?;
    }

    writeln!(w, "    )")?; // network_out
    writeln!(w, "  )")?;   // routes
    writeln!(w, ")")?;     // session

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::RoutedWire;

    #[test]
    fn test_quoted() {
        assert_eq!(quoted("F.Cu"), "F.Cu");
        assert_eq!(quoted("My Net"), "\"My Net\"");
        assert_eq!(quoted(""), "\"\"");
    }

    #[test]
    fn test_write_ses_no_panic() {
        use std::collections::HashMap;
        use crate::dsn::*;

        let design = DsnDesign {
            name: "test".to_string(),
            resolution: Resolution { unit: "um".to_string(), value: 10 },
            unit: "um".to_string(),
            layers: vec![
                Layer { name: "F.Cu".to_string(), layer_type: "signal".to_string(), index: 0 },
            ],
            boundary: Boundary {
                points: vec![],
                min_x: 0, min_y: 0, max_x: 100000, max_y: 100000,
            },
            rules: DesignRule { trace_width: 250, clearance: 200 },
            components: vec![],
            images: HashMap::new(),
            padstacks: HashMap::new(),
            nets: vec![Net { name: "NET1".to_string(), pins: vec![] }],
            wiring: vec![],
        };

        let routing = RoutingResult {
            wires: vec![RoutedWire {
                net_name: "NET1".to_string(),
                layer: "F.Cu".to_string(),
                width: 250,
                points: vec![(0, 0), (1000, 0)],
            }],
            vias: vec![],
            unrouted: vec![],
        };

        let tmp = std::env::temp_dir().join("test_ses_output.ses");
        write_ses(&design, &routing, &tmp).unwrap();

        let content = std::fs::read_to_string(&tmp).unwrap();
        assert!(content.contains("(session"));
        assert!(content.contains("NET1"));
        assert!(content.contains("F.Cu"));
        let _ = std::fs::remove_file(&tmp);
    }
}
