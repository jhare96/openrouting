use std::collections::HashMap;
use crate::sexp::Sexp;

#[derive(Debug, Clone)]
pub struct Point {
    pub x: i64,
    pub y: i64,
}

#[derive(Debug, Clone)]
pub struct Resolution {
    pub unit: String,
    pub value: i64,
}

#[derive(Debug, Clone)]
pub struct Layer {
    pub name: String,
    pub layer_type: String,
    pub index: usize,
}

#[derive(Debug, Clone)]
pub struct Boundary {
    pub points: Vec<Point>,
    pub min_x: i64,
    pub min_y: i64,
    pub max_x: i64,
    pub max_y: i64,
}

#[derive(Debug, Clone)]
pub struct DesignRule {
    pub trace_width: i64,
    pub clearance: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Side {
    Front,
    Back,
}

#[derive(Debug, Clone)]
pub struct Place {
    pub reference: String,
    pub x: i64,
    pub y: i64,
    pub side: Side,
    pub rotation: f64,
}

#[derive(Debug, Clone)]
pub struct Component {
    pub image_name: String,
    pub places: Vec<Place>,
}

#[derive(Debug, Clone)]
pub struct ImagePin {
    pub padstack_name: String,
    pub pin_number: String,
    pub x: i64,
    pub y: i64,
    pub rotation: f64,
}

#[derive(Debug, Clone)]
pub struct Image {
    pub name: String,
    pub outlines: Vec<Vec<Point>>,
    pub pins: Vec<ImagePin>,
}

#[derive(Debug, Clone)]
pub enum PadShape {
    Circle { layer: String, diameter: i64 },
    Rect { layer: String, x1: i64, y1: i64, x2: i64, y2: i64 },
    Oval { layer: String, width: i64, height: i64 },
    Path { layer: String, width: i64, points: Vec<Point> },
}

#[derive(Debug, Clone)]
pub struct Padstack {
    pub name: String,
    pub shapes: Vec<PadShape>,
    pub attach: bool,
}

#[derive(Debug, Clone)]
pub struct PinRef {
    pub component: String,
    pub pin: String,
}

#[derive(Debug, Clone)]
pub struct Net {
    pub name: String,
    pub pins: Vec<PinRef>,
}

#[derive(Debug, Clone)]
pub struct Wire {
    pub net_name: String,
    pub layer: String,
    pub width: i64,
    pub points: Vec<Point>,
}

#[derive(Debug, Clone)]
pub struct DsnDesign {
    pub name: String,
    pub resolution: Resolution,
    pub unit: String,
    pub layers: Vec<Layer>,
    pub boundary: Boundary,
    pub rules: DesignRule,
    pub components: Vec<Component>,
    pub images: HashMap<String, Image>,
    pub padstacks: HashMap<String, Padstack>,
    pub nets: Vec<Net>,
    pub wiring: Vec<Wire>,
}

// ─── helpers ──────────────────────────────────────────────────────────────────

fn atom_str<'a>(s: &'a Sexp) -> &'a str {
    s.as_atom().unwrap_or("")
}

fn parse_i64(s: &Sexp) -> i64 {
    atom_str(s)
        .parse::<f64>()
        .map(|f| f as i64)
        .unwrap_or(0)
}

fn parse_f64(s: &Sexp) -> f64 {
    atom_str(s).parse::<f64>().unwrap_or(0.0)
}

/// Parse the `"COMP"-"PIN"` or `COMP-PIN` or `"COMP-PIN"` pin-reference formats.
fn parse_pin_ref(token: &str) -> PinRef {
    // Try split on the last '-' that is preceded by a closing quote or a word char
    // Typical patterns: R1-1  "R1"-"1"  "R1-1"
    // We strip surrounding quotes first, then split on '-'
    let stripped = token.trim_matches('"');

    // Check if there's a `"-"` separator pattern
    // Format: "COMP"-"PIN"  => after trimming outer quotes we get: COMP"-"PIN
    if let Some(idx) = stripped.find("\"-\"") {
        let comp = stripped[..idx].trim_matches('"').to_string();
        let pin = stripped[idx + 3..].trim_matches('"').to_string();
        return PinRef { component: comp, pin };
    }

    // Plain dash split
    if let Some(pos) = stripped.rfind('-') {
        let comp = stripped[..pos].to_string();
        let pin = stripped[pos + 1..].to_string();
        PinRef { component: comp, pin }
    } else {
        PinRef {
            component: stripped.to_string(),
            pin: String::new(),
        }
    }
}

// ─── main parser ──────────────────────────────────────────────────────────────

pub fn parse_dsn(input: &str) -> Result<DsnDesign, String> {
    let root = Sexp::parse(input)?;

    // The root might be wrapped in an extra list if multiple top-level items exist
    let pcb = if root.name() == Some("pcb") {
        &root
    } else if let Some(list) = root.as_list() {
        list.iter()
            .find(|s| s.name() == Some("pcb"))
            .ok_or("No (pcb ...) top-level form found")?
    } else {
        return Err("No (pcb ...) top-level form found".to_string());
    };

    let list = pcb.as_list().ok_or("pcb is not a list")?;

    // Design name (second element after 'pcb')
    let name = list
        .get(1)
        .and_then(|s| s.as_atom())
        .unwrap_or("unnamed")
        .to_string();

    // resolution
    let resolution = if let Some(res) = pcb.find("resolution") {
        let items = res.as_list().unwrap_or(&[]);
        Resolution {
            unit: items.get(1).and_then(|s| s.as_atom()).unwrap_or("um").to_string(),
            value: items.get(2).map(parse_i64).unwrap_or(1),
        }
    } else {
        Resolution { unit: "um".to_string(), value: 1 }
    };

    // unit
    let unit = if let Some(u) = pcb.find("unit") {
        u.as_list()
            .and_then(|l| l.get(1))
            .and_then(|s| s.as_atom())
            .unwrap_or("um")
            .to_string()
    } else {
        resolution.unit.clone()
    };

    // structure: layers, boundary, rules
    let structure = pcb.find("structure");
    let (layers, boundary, rules) = parse_structure(structure);

    // library: images, padstacks
    let library = pcb.find("library");
    let (images, padstacks) = parse_library(library);

    // placement: components
    let placement = pcb.find("placement");
    let components = parse_placement(placement);

    // network: nets
    let network = pcb.find("network");
    let nets = parse_network(network);

    // wiring
    let wiring_node = pcb.find("wiring");
    let wiring = parse_wiring(wiring_node);

    Ok(DsnDesign {
        name,
        resolution,
        unit,
        layers,
        boundary,
        rules,
        components,
        images,
        padstacks,
        nets,
        wiring,
    })
}

fn parse_structure(node: Option<&Sexp>) -> (Vec<Layer>, Boundary, DesignRule) {
    let mut layers = Vec::new();
    let mut boundary = Boundary {
        points: Vec::new(),
        min_x: 0,
        min_y: 0,
        max_x: 100_000,
        max_y: 100_000,
    };
    let mut rules = DesignRule { trace_width: 250, clearance: 200 };

    let node = match node {
        Some(n) => n,
        None => return (layers, boundary, rules),
    };
    let list = match node.as_list() {
        Some(l) => l,
        None => return (layers, boundary, rules),
    };

    let mut layer_idx = 0;
    for item in list.iter().skip(1) {
        match item.name() {
            Some("layer") => {
                let items = item.as_list().unwrap_or(&[]);
                let lname = items.get(1).and_then(|s| s.as_atom()).unwrap_or("").to_string();
                let ltype = item
                    .find("type")
                    .and_then(|t| t.as_list())
                    .and_then(|l| l.get(1))
                    .and_then(|s| s.as_atom())
                    .unwrap_or("signal")
                    .to_string();
                layers.push(Layer { name: lname, layer_type: ltype, index: layer_idx });
                layer_idx += 1;
            }
            Some("boundary") => {
                boundary = parse_boundary(item);
            }
            Some("rule") => {
                rules = parse_rule(item);
            }
            _ => {}
        }
    }

    // Default layers if none found
    if layers.is_empty() {
        layers.push(Layer { name: "F.Cu".to_string(), layer_type: "signal".to_string(), index: 0 });
        layers.push(Layer { name: "B.Cu".to_string(), layer_type: "signal".to_string(), index: 1 });
    }

    (layers, boundary, rules)
}

fn parse_boundary(node: &Sexp) -> Boundary {
    let list = match node.as_list() {
        Some(l) => l,
        None => return default_boundary(),
    };

    // Look for (rect pcb x1 y1 x2 y2) or (polygon pcb 0 x1 y1 x2 y2 ...) children
    for item in list.iter().skip(1) {
        match item.name() {
            Some("rect") => {
                let items = item.as_list().unwrap_or(&[]);
                // (rect pcb x1 y1 x2 y2)
                if items.len() >= 6 {
                    let x1 = parse_i64(&items[2]);
                    let y1 = parse_i64(&items[3]);
                    let x2 = parse_i64(&items[4]);
                    let y2 = parse_i64(&items[5]);
                    let (min_x, max_x) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
                    let (min_y, max_y) = if y1 <= y2 { (y1, y2) } else { (y2, y1) };
                    return Boundary {
                        points: vec![
                            Point { x: x1, y: y1 },
                            Point { x: x2, y: y1 },
                            Point { x: x2, y: y2 },
                            Point { x: x1, y: y2 },
                        ],
                        min_x,
                        min_y,
                        max_x,
                        max_y,
                    };
                }
            }
            Some("polygon") | Some("path") => {
                let items = item.as_list().unwrap_or(&[]);
                // (polygon pcb aperture x1 y1 ...) or (path layer width x1 y1 ...)
                // In both cases coordinate data starts at index 3
                let start = 3;
                let mut points = Vec::new();
                let mut i = start;
                while i + 1 < items.len() {
                    if let (Some(_), Some(_)) = (items[i].as_atom(), items[i + 1].as_atom()) {
                        points.push(Point {
                            x: parse_i64(&items[i]),
                            y: parse_i64(&items[i + 1]),
                        });
                        i += 2;
                    } else {
                        break;
                    }
                }
                if !points.is_empty() {
                    let min_x = points.iter().map(|p| p.x).min().unwrap_or(0);
                    let min_y = points.iter().map(|p| p.y).min().unwrap_or(0);
                    let max_x = points.iter().map(|p| p.x).max().unwrap_or(0);
                    let max_y = points.iter().map(|p| p.y).max().unwrap_or(0);
                    return Boundary { points, min_x, min_y, max_x, max_y };
                }
            }
            _ => {}
        }
    }

    // If boundary has direct rect child
    if let Some(rect) = node.find("rect") {
        let items = rect.as_list().unwrap_or(&[]);
        if items.len() >= 6 {
            let x1 = parse_i64(&items[2]);
            let y1 = parse_i64(&items[3]);
            let x2 = parse_i64(&items[4]);
            let y2 = parse_i64(&items[5]);
            let (min_x, max_x) = if x1 <= x2 { (x1, x2) } else { (x2, x1) };
            let (min_y, max_y) = if y1 <= y2 { (y1, y2) } else { (y2, y1) };
            return Boundary {
                points: vec![
                    Point { x: x1, y: y1 },
                    Point { x: x2, y: y1 },
                    Point { x: x2, y: y2 },
                    Point { x: x1, y: y2 },
                ],
                min_x,
                min_y,
                max_x,
                max_y,
            };
        }
    }

    default_boundary()
}

fn default_boundary() -> Boundary {
    Boundary {
        points: vec![
            Point { x: 0, y: 0 },
            Point { x: 100_000, y: 0 },
            Point { x: 100_000, y: 100_000 },
            Point { x: 0, y: 100_000 },
        ],
        min_x: 0,
        min_y: 0,
        max_x: 100_000,
        max_y: 100_000,
    }
}

fn parse_rule(node: &Sexp) -> DesignRule {
    let mut trace_width = 250i64;
    let mut clearance = 200i64;
    let list = match node.as_list() {
        Some(l) => l,
        None => return DesignRule { trace_width, clearance },
    };
    for item in list.iter().skip(1) {
        match item.name() {
            Some("width") => {
                if let Some(items) = item.as_list() {
                    if let Some(v) = items.get(1) {
                        trace_width = parse_i64(v);
                    }
                }
            }
            Some("clearance") => {
                if let Some(items) = item.as_list() {
                    if let Some(v) = items.get(1) {
                        clearance = parse_i64(v);
                    }
                }
            }
            _ => {}
        }
    }
    DesignRule { trace_width, clearance }
}

fn parse_library(node: Option<&Sexp>) -> (HashMap<String, Image>, HashMap<String, Padstack>) {
    let mut images: HashMap<String, Image> = HashMap::new();
    let mut padstacks: HashMap<String, Padstack> = HashMap::new();

    let list = match node.and_then(|n| n.as_list()) {
        Some(l) => l,
        None => return (images, padstacks),
    };

    for item in list.iter().skip(1) {
        match item.name() {
            Some("image") => {
                if let Some(img) = parse_image(item) {
                    images.insert(img.name.clone(), img);
                }
            }
            Some("padstack") => {
                if let Some(ps) = parse_padstack(item) {
                    padstacks.insert(ps.name.clone(), ps);
                }
            }
            _ => {}
        }
    }

    (images, padstacks)
}

fn parse_image(node: &Sexp) -> Option<Image> {
    let list = node.as_list()?;
    let name = list.get(1)?.as_atom()?.to_string();
    let mut outlines = Vec::new();
    let mut pins = Vec::new();

    for item in list.iter().skip(2) {
        match item.name() {
            Some("outline") => {
                // (outline (path layer width x1 y1 x2 y2 ...))
                if let Some(path) = item.find("path") {
                    let path_list = path.as_list().unwrap_or(&[]);
                    let mut pts = Vec::new();
                    let mut i = 3; // skip 'path', layer, width
                    while i + 1 < path_list.len() {
                        pts.push(Point {
                            x: parse_i64(&path_list[i]),
                            y: parse_i64(&path_list[i + 1]),
                        });
                        i += 2;
                    }
                    if !pts.is_empty() {
                        outlines.push(pts);
                    }
                }
            }
            Some("pin") => {
                // (pin "PADSTACK" "NUM" x y) or (pin "PADSTACK" (rotate ...) "NUM" x y)
                let items = item.as_list().unwrap_or(&[]);
                if items.len() < 4 {
                    continue;
                }
                let padstack_name = items.get(1).and_then(|s| s.as_atom()).unwrap_or("").to_string();

                // Handle optional (rotate angle) child
                let mut rotation = 0.0f64;
                let mut offset = 2usize;
                if let Some(rotate) = item.find("rotate") {
                    rotation = rotate
                        .as_list()
                        .and_then(|l| l.get(1))
                        .map(parse_f64)
                        .unwrap_or(0.0);
                    offset = 3; // skip past the rotate list
                }

                let pin_number = items.get(offset).and_then(|s| s.as_atom()).unwrap_or("").to_string();
                let x = items.get(offset + 1).map(parse_i64).unwrap_or(0);
                let y = items.get(offset + 2).map(parse_i64).unwrap_or(0);

                pins.push(ImagePin { padstack_name, pin_number, x, y, rotation });
            }
            _ => {}
        }
    }

    Some(Image { name, outlines, pins })
}

fn parse_padstack(node: &Sexp) -> Option<Padstack> {
    let list = node.as_list()?;
    let name = list.get(1)?.as_atom()?.to_string();
    let mut shapes = Vec::new();
    let mut attach = false;

    for item in list.iter().skip(2) {
        match item.name() {
            Some("shape") => {
                if let Some(shape_node) = item.as_list().and_then(|l| l.get(1)) {
                    if let Some(shape) = parse_pad_shape(shape_node) {
                        shapes.push(shape);
                    }
                }
            }
            Some("attach") => {
                attach = item
                    .as_list()
                    .and_then(|l| l.get(1))
                    .and_then(|s| s.as_atom())
                    .map(|s| s == "on")
                    .unwrap_or(false);
            }
            _ => {}
        }
    }

    Some(Padstack { name, shapes, attach })
}

fn parse_pad_shape(node: &Sexp) -> Option<PadShape> {
    let list = node.as_list()?;
    let kind = list.first()?.as_atom()?;
    match kind {
        "circle" => {
            let layer = list.get(1)?.as_atom()?.to_string();
            let diameter = list.get(2).map(parse_i64).unwrap_or(100);
            Some(PadShape::Circle { layer, diameter })
        }
        "rect" | "rectangle" => {
            let layer = list.get(1)?.as_atom()?.to_string();
            let x1 = list.get(2).map(parse_i64).unwrap_or(0);
            let y1 = list.get(3).map(parse_i64).unwrap_or(0);
            let x2 = list.get(4).map(parse_i64).unwrap_or(0);
            let y2 = list.get(5).map(parse_i64).unwrap_or(0);
            Some(PadShape::Rect { layer, x1, y1, x2, y2 })
        }
        "oval" => {
            let layer = list.get(1)?.as_atom()?.to_string();
            let width = list.get(2).map(parse_i64).unwrap_or(100);
            let height = list.get(3).map(parse_i64).unwrap_or(100);
            Some(PadShape::Oval { layer, width, height })
        }
        "path" => {
            let layer = list.get(1)?.as_atom()?.to_string();
            let width = list.get(2).map(parse_i64).unwrap_or(0);
            let mut points = Vec::new();
            let mut i = 3;
            while i + 1 < list.len() {
                points.push(Point {
                    x: parse_i64(&list[i]),
                    y: parse_i64(&list[i + 1]),
                });
                i += 2;
            }
            Some(PadShape::Path { layer, width, points })
        }
        _ => None,
    }
}

fn parse_placement(node: Option<&Sexp>) -> Vec<Component> {
    let mut components = Vec::new();
    let list = match node.and_then(|n| n.as_list()) {
        Some(l) => l,
        None => return components,
    };

    for item in list.iter().skip(1) {
        if item.name() == Some("component") {
            let items = item.as_list().unwrap_or(&[]);
            let image_name = items.get(1).and_then(|s| s.as_atom()).unwrap_or("").to_string();
            let mut places = Vec::new();
            for child in items.iter().skip(2) {
                if child.name() == Some("place") {
                    if let Some(place) = parse_place(child) {
                        places.push(place);
                    }
                }
            }
            components.push(Component { image_name, places });
        }
    }

    components
}

fn parse_place(node: &Sexp) -> Option<Place> {
    let list = node.as_list()?;
    // (place "REF" x y front/back rotation)
    let reference = list.get(1)?.as_atom()?.to_string();
    let x = list.get(2).map(parse_i64).unwrap_or(0);
    let y = list.get(3).map(parse_i64).unwrap_or(0);
    let side_str = list.get(4).and_then(|s| s.as_atom()).unwrap_or("front");
    let side = if side_str.eq_ignore_ascii_case("back") { Side::Back } else { Side::Front };
    let rotation = list.get(5).map(parse_f64).unwrap_or(0.0);
    Some(Place { reference, x, y, side, rotation })
}

fn parse_network(node: Option<&Sexp>) -> Vec<Net> {
    let mut nets = Vec::new();
    let list = match node.and_then(|n| n.as_list()) {
        Some(l) => l,
        None => return nets,
    };

    for item in list.iter().skip(1) {
        if item.name() == Some("net") {
            let items = item.as_list().unwrap_or(&[]);
            let name = items.get(1).and_then(|s| s.as_atom()).unwrap_or("").to_string();
            let mut pins = Vec::new();
            if let Some(pins_node) = item.find("pins") {
                let pins_list = pins_node.as_list().unwrap_or(&[]);
                for pin_token in pins_list.iter().skip(1) {
                    if let Some(token) = pin_token.as_atom() {
                        pins.push(parse_pin_ref(token));
                    }
                }
            }
            nets.push(Net { name, pins });
        }
    }

    nets
}

fn parse_wiring(node: Option<&Sexp>) -> Vec<Wire> {
    let mut wires = Vec::new();
    let list = match node.and_then(|n| n.as_list()) {
        Some(l) => l,
        None => return wires,
    };

    for item in list.iter().skip(1) {
        if item.name() == Some("wire") {
            if let Some(wire) = parse_wire(item) {
                wires.push(wire);
            }
        }
    }

    wires
}

fn parse_wire(node: &Sexp) -> Option<Wire> {
    let path = node.find("path")?;
    let path_list = path.as_list()?;
    let layer = path_list.get(1)?.as_atom()?.to_string();
    let width = path_list.get(2).map(parse_i64).unwrap_or(0);
    let mut points = Vec::new();
    let mut i = 3;
    while i + 1 < path_list.len() {
        // stop if we hit a non-atom (e.g. sub-list)
        match (path_list[i].as_atom(), path_list[i + 1].as_atom()) {
            (Some(_), Some(_)) => {
                points.push(Point {
                    x: parse_i64(&path_list[i]),
                    y: parse_i64(&path_list[i + 1]),
                });
                i += 2;
            }
            _ => break,
        }
    }

    // net name from sibling (net "NAME")
    let net_name = node
        .find("net")
        .and_then(|n| n.as_list())
        .and_then(|l| l.get(1))
        .and_then(|s| s.as_atom())
        .unwrap_or("")
        .to_string();

    Some(Wire { net_name, layer, width, points })
}

// ─── pad position helper ──────────────────────────────────────────────────────

/// Returns the absolute (x, y, layer_name) of a pad given a component reference and pin number.
pub fn get_pad_position(
    design: &DsnDesign,
    comp_ref: &str,
    pin_num: &str,
) -> Option<(i64, i64, String)> {
    // Find the component placement
    let place = design.components.iter().find_map(|comp| {
        comp.places.iter().find(|p| p.reference == comp_ref).map(|p| (comp, p))
    });
    let (comp, place) = place?;

    // Look up the image for this component
    let image = design.images.get(&comp.image_name)?;

    // Find the pin in the image
    let pin = image.pins.iter().find(|p| p.pin_number == pin_num)?;

    // Apply pin rotation then component rotation
    let pin_rot = pin.rotation.to_radians();
    let rx = pin.x as f64 * pin_rot.cos() - pin.y as f64 * pin_rot.sin();
    let ry = pin.x as f64 * pin_rot.sin() + pin.y as f64 * pin_rot.cos();

    let comp_rot = place.rotation.to_radians();
    let fx = rx * comp_rot.cos() - ry * comp_rot.sin();
    let fy = rx * comp_rot.sin() + ry * comp_rot.cos();

    // If back side, mirror X
    let fx = if place.side == Side::Back { -fx } else { fx };

    let abs_x = place.x + fx as i64;
    let abs_y = place.y + fy as i64;

    // Determine layer: if back side use B.Cu, else F.Cu
    let layer = design
        .padstacks
        .get(&pin.padstack_name)
        .and_then(|ps| ps.shapes.first())
        .map(|shape| {
            let l = match shape {
                PadShape::Circle { layer, .. } => layer.as_str(),
                PadShape::Rect { layer, .. } => layer.as_str(),
                PadShape::Oval { layer, .. } => layer.as_str(),
                PadShape::Path { layer, .. } => layer.as_str(),
            };
            if l == "*.Cu" {
                if place.side == Side::Back { "B.Cu" } else { "F.Cu" }
            } else {
                l
            }
        })
        .unwrap_or(if place.side == Side::Back { "B.Cu" } else { "F.Cu" })
        .to_string();

    Some((abs_x, abs_y, layer))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_DSN: &str = r#"
(pcb "test"
  (resolution um 10)
  (structure
    (layer "F.Cu" (type signal))
    (layer "B.Cu" (type signal))
    (boundary (rect pcb 0 0 100000 100000))
    (rule (width 250) (clearance 200))
  )
  (library
    (padstack "via"
      (shape (circle "*.Cu" 600))
    )
    (image "R"
      (pin "via" "1" -500 0)
      (pin "via" "2" 500 0)
    )
  )
  (placement
    (component "R"
      (place "R1" 10000 50000 front 0)
      (place "R2" 90000 50000 front 0)
    )
  )
  (network
    (net "NET1"
      (pins R1-1 R2-2)
    )
  )
  (wiring)
)
"#;

    #[test]
    fn test_parse_simple() {
        let design = parse_dsn(SIMPLE_DSN).unwrap();
        assert_eq!(design.name, "test");
        assert_eq!(design.layers.len(), 2);
        assert_eq!(design.nets.len(), 1);
        assert_eq!(design.nets[0].pins.len(), 2);
    }

    #[test]
    fn test_pad_position() {
        let design = parse_dsn(SIMPLE_DSN).unwrap();
        let pos = get_pad_position(&design, "R1", "1").unwrap();
        assert_eq!(pos.0, 10000 - 500);
        assert_eq!(pos.1, 50000);
    }
}
