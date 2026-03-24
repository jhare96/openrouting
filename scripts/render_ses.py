#!/usr/bin/env python3
"""
Rendering engine for openrouting: generates PNG images from .dsn and .ses files.

Parses the Specctra DSN (design) and SES (session/routing) S-expression files
and renders the board outline, component pads, routed traces, and vias into
a PNG image.

Usage:
    python3 render_ses.py <input.dsn> <input.ses> [-o output.png] [--dpi 150]
                          [--width 2048] [--layers F.Cu,B.Cu] [--no-pads]
                          [--no-boundary] [--background dark]
"""

from __future__ import annotations

import argparse
import math
import sys
from pathlib import Path
from typing import Optional

from PIL import Image, ImageDraw


# ─── S-expression parser ────────────────────────────────────────────────────

class SExp:
    """Minimal S-expression parser compatible with Specctra DSN/SES format."""

    __slots__ = ("value", "children")

    def __init__(
        self,
        value: Optional[str] = None,
        children: Optional[list["SExp"]] = None,
    ):
        self.value = value  # non-None for atoms
        self.children = children  # non-None for lists

    @property
    def is_atom(self) -> bool:
        return self.value is not None

    @property
    def is_list(self) -> bool:
        return self.children is not None

    @property
    def name(self) -> Optional[str]:
        """Return the first atom in a list (the tag), or None."""
        if self.children and self.children[0].is_atom:
            return self.children[0].value
        return None

    def find(self, key: str) -> Optional["SExp"]:
        """Find first direct child list whose name matches *key*."""
        if self.children:
            for child in self.children:
                if child.name == key:
                    return child
        return None

    def find_all(self, key: str) -> list["SExp"]:
        """Find all direct child lists whose name matches *key*."""
        if self.children:
            return [c for c in self.children if c.name == key]
        return []

    def atom(self, index: int) -> str:
        """Return the atom string at *index* in a list, or empty string."""
        if self.children and index < len(self.children):
            node = self.children[index]
            if node.is_atom:
                return node.value
        return ""

    def num(self, index: int) -> float:
        """Return the numeric value of the atom at *index*, or 0."""
        try:
            return float(self.atom(index))
        except (ValueError, TypeError):
            return 0.0

    def inum(self, index: int) -> int:
        """Return the integer value of the atom at *index*, or 0."""
        return int(self.num(index))

    # ── parsing ──

    @staticmethod
    def parse(text: str) -> "SExp":
        tokens = SExp._tokenize(text)
        items, _ = SExp._parse_tokens(tokens, 0)
        if len(items) == 1:
            return items[0]
        return SExp(children=items)

    @staticmethod
    def _tokenize(text: str) -> list[str]:
        tokens: list[str] = []
        i = 0
        n = len(text)
        while i < n:
            c = text[i]
            # whitespace
            if c in " \t\n\r":
                i += 1
                continue
            # line comments
            if c == "#" or (c == "/" and i + 1 < n and text[i + 1] == "/"):
                while i < n and text[i] != "\n":
                    i += 1
                continue
            # parens
            if c in "()":
                tokens.append(c)
                i += 1
                continue
            # quoted string or bare quote (Specctra string_quote convention)
            if c == '"':
                # Look ahead: if the next non-whitespace char after the quote
                # is ')' or we're at end, treat as bare quote atom.
                j = i + 1
                if j >= n or text[j] in ") \t\n\r":
                    tokens.append('"')
                    i = j
                    continue
                # Otherwise, parse a quoted string
                i += 1  # skip opening quote
                s = []
                while i < n and text[i] != '"':
                    if text[i] == "\\" and i + 1 < n:
                        i += 1
                        s.append(text[i])
                    else:
                        s.append(text[i])
                    i += 1
                if i < n:
                    i += 1  # skip closing quote
                tokens.append("".join(s))
                continue
            # atom
            j = i
            while j < n and text[j] not in " \t\n\r()\"":
                j += 1
            tokens.append(text[i:j])
            i = j
        return tokens

    @staticmethod
    def _parse_tokens(
        tokens: list[str], pos: int
    ) -> tuple[list["SExp"], int]:
        items: list[SExp] = []
        while pos < len(tokens):
            tok = tokens[pos]
            if tok == ")":
                break
            if tok == "(":
                pos += 1
                children, pos = SExp._parse_tokens(tokens, pos)
                if pos < len(tokens) and tokens[pos] == ")":
                    pos += 1
                items.append(SExp(children=children))
            else:
                items.append(SExp(value=tok))
                pos += 1
        return items, pos


# ─── DSN data extraction ────────────────────────────────────────────────────

class BoardData:
    """Extracted board information from the DSN file."""

    def __init__(self):
        self.boundary_points: list[tuple[float, float]] = []
        self.min_x = 0.0
        self.min_y = 0.0
        self.max_x = 100000.0
        self.max_y = 100000.0
        self.layers: list[str] = []
        self.pads: list[dict] = []  # {x, y, shape, layer, ...}
        self.resolution_value = 1
        self.resolution_unit = "um"


def parse_dsn_board(dsn_text: str) -> BoardData:
    """Parse DSN file and extract board geometry for rendering."""
    root = SExp.parse(dsn_text)
    pcb = root if root.name == "pcb" else root.find("pcb")
    if pcb is None:
        raise ValueError("No (pcb ...) top-level form found in DSN file")

    board = BoardData()

    # Resolution
    res = pcb.find("resolution")
    if res:
        board.resolution_unit = res.atom(1) or "um"
        board.resolution_value = res.inum(2) or 1

    # Structure: layers, boundary
    structure = pcb.find("structure")
    if structure:
        for child in structure.children or []:
            if child.name == "layer":
                board.layers.append(child.atom(1))
            elif child.name == "boundary":
                _parse_boundary(child, board)

    if not board.layers:
        board.layers = ["F.Cu", "B.Cu"]

    # Library: padstacks
    padstacks: dict[str, list[dict]] = {}
    library = pcb.find("library")
    if library:
        for ps in library.find_all("padstack"):
            ps_name = ps.atom(1)
            shapes = []
            for shape_node in ps.find_all("shape"):
                shape = _parse_pad_shape(shape_node)
                if shape:
                    shapes.append(shape)
            if shapes:
                padstacks[ps_name] = shapes

        # Images (component footprints)
        images: dict[str, list[dict]] = {}
        for img in library.find_all("image"):
            img_name = img.atom(1)
            pins = []
            for pin in img.find_all("pin"):
                ps_name = pin.atom(1)
                pin_id = pin.atom(2)
                px = pin.num(3)
                py = pin.num(4)
                rotation = 0.0
                if pin.children and len(pin.children) > 5:
                    rotation = pin.num(5)
                pins.append({
                    "padstack": ps_name,
                    "pin_id": pin_id,
                    "x": px,
                    "y": py,
                    "rotation": rotation,
                })
            images[img_name] = pins

    # Placement: resolve pad positions
    placement = pcb.find("placement")
    if placement and library:
        for comp in placement.find_all("component"):
            img_name = comp.atom(1)
            img_pins = images.get(img_name, [])
            for place in comp.find_all("place"):
                ref = place.atom(1)
                cx = place.num(2)
                cy = place.num(3)
                side = place.atom(4)
                rot_deg = place.num(5) if place.children and len(place.children) > 5 else 0.0
                rot = math.radians(rot_deg)

                for pin in img_pins:
                    # Rotate pin around component center
                    px, py = pin["x"], pin["y"]
                    cos_r, sin_r = math.cos(rot), math.sin(rot)
                    rx = px * cos_r - py * sin_r + cx
                    ry = px * sin_r + py * cos_r + cy

                    ps_shapes = padstacks.get(pin["padstack"], [])
                    for shape in ps_shapes:
                        board.pads.append({
                            "x": rx,
                            "y": ry,
                            "shape": shape,
                            "component": ref,
                            "pin": pin["pin_id"],
                        })

    return board


def _parse_boundary(node: SExp, board: BoardData):
    """Parse boundary from DSN structure."""
    if node.children is None:
        return
    for child in node.children[1:]:
        if child.name == "rect":
            x1 = child.num(2)
            y1 = child.num(3)
            x2 = child.num(4)
            y2 = child.num(5)
            board.boundary_points = [(x1, y1), (x2, y1), (x2, y2), (x1, y2)]
            board.min_x = min(x1, x2)
            board.min_y = min(y1, y2)
            board.max_x = max(x1, x2)
            board.max_y = max(y1, y2)
            return
        if child.name in ("polygon", "path"):
            items = child.children or []
            points = []
            i = 3
            while i + 1 < len(items):
                if items[i].is_atom and items[i + 1].is_atom:
                    try:
                        px = float(items[i].value)
                        py = float(items[i + 1].value)
                        points.append((px, py))
                    except ValueError:
                        break
                    i += 2
                else:
                    break
            if points:
                board.boundary_points = points
                xs = [p[0] for p in points]
                ys = [p[1] for p in points]
                board.min_x = min(xs)
                board.min_y = min(ys)
                board.max_x = max(xs)
                board.max_y = max(ys)
                return


def _parse_pad_shape(shape_node: SExp) -> Optional[dict]:
    """Parse a single pad shape from a padstack shape node."""
    if shape_node.children is None or len(shape_node.children) < 2:
        return None
    inner = shape_node.children[1]
    if inner.name == "circle":
        return {
            "type": "circle",
            "layer": inner.atom(1),
            "diameter": inner.num(2),
        }
    elif inner.name == "rect":
        return {
            "type": "rect",
            "layer": inner.atom(1),
            "x1": inner.num(2),
            "y1": inner.num(3),
            "x2": inner.num(4),
            "y2": inner.num(5),
        }
    elif inner.name == "oval":
        return {
            "type": "oval",
            "layer": inner.atom(1),
            "width": inner.num(2),
            "height": inner.num(3),
        }
    elif inner.name == "path":
        return {
            "type": "path",
            "layer": inner.atom(1),
            "width": inner.num(2),
        }
    return None


# ─── SES data extraction ────────────────────────────────────────────────────

class RoutingData:
    """Extracted routing information from the SES file."""

    def __init__(self):
        self.wires: list[dict] = []  # {net, layer, width, points}
        self.vias: list[dict] = []   # {net, padstack, x, y}


def parse_ses_routing(ses_text: str) -> RoutingData:
    """Parse SES file and extract routing data."""
    root = SExp.parse(ses_text)
    session = root if root.name == "session" else root.find("session")
    if session is None:
        raise ValueError("No (session ...) top-level form found in SES file")

    routing = RoutingData()

    routes = session.find("routes")
    if routes is None:
        return routing

    network_out = routes.find("network_out")
    if network_out is None:
        return routing

    for net_node in network_out.find_all("net"):
        net_name = net_node.atom(1)

        for wire_node in net_node.find_all("wire"):
            path_node = wire_node.find("path")
            if path_node is None:
                continue
            layer = path_node.atom(1)
            width = path_node.num(2)
            points = []
            items = path_node.children or []
            i = 3
            while i + 1 < len(items):
                if items[i].is_atom and items[i + 1].is_atom:
                    try:
                        px = float(items[i].value)
                        py = float(items[i + 1].value)
                        points.append((px, py))
                    except ValueError:
                        break
                    i += 2
                else:
                    break
            if len(points) >= 2:
                routing.wires.append({
                    "net": net_name,
                    "layer": layer,
                    "width": width,
                    "points": points,
                })

        for via_node in net_node.find_all("via"):
            padstack = via_node.atom(1)
            vx = via_node.num(2)
            vy = via_node.num(3)
            routing.vias.append({
                "net": net_name,
                "padstack": padstack,
                "x": vx,
                "y": vy,
            })

    return routing


# ─── Rendering ───────────────────────────────────────────────────────────────

# Default layer color palette (RGBA)
LAYER_COLORS: dict[str, tuple[int, int, int, int]] = {
    "F.Cu":   (200,  50,  50, 200),  # red
    "B.Cu":   ( 50,  50, 200, 200),  # blue
    "In1.Cu": ( 50, 180,  50, 200),  # green
    "In2.Cu": (200, 180,  50, 200),  # yellow
    "Top":    (200,  50,  50, 200),  # alias for F.Cu
    "Bottom": ( 50,  50, 200, 200),  # alias for B.Cu
}

BACKGROUND_PRESETS = {
    "dark":  (30, 30, 30),
    "light": (245, 245, 240),
    "black": (0, 0, 0),
    "white": (255, 255, 255),
}

PAD_COLOR = (0, 180, 0, 220)
VIA_OUTER_COLOR = (200, 200, 200, 230)
VIA_INNER_COLOR = (80, 80, 80, 255)
BOUNDARY_COLOR = (180, 180, 60, 255)
UNROUTED_LAYER_COLOR = (150, 150, 150, 150)


def _layer_color(layer: str) -> tuple[int, int, int, int]:
    """Get the RGBA color for a given layer name."""
    if layer in LAYER_COLORS:
        return LAYER_COLORS[layer]
    # Generate a consistent color for unknown layers
    h = hash(layer)
    r = 80 + (h & 0xFF) % 170
    g = 80 + ((h >> 8) & 0xFF) % 170
    b = 80 + ((h >> 16) & 0xFF) % 170
    return (r, g, b, 200)


class Renderer:
    """Renders board and routing data to a PNG image."""

    def __init__(
        self,
        board: BoardData,
        routing: RoutingData,
        image_width: int = 2048,
        dpi: int = 150,
        background: str = "dark",
        show_pads: bool = True,
        show_boundary: bool = True,
        layer_filter: Optional[set[str]] = None,
    ):
        self.board = board
        self.routing = routing
        self.image_width = image_width
        self.dpi = dpi
        self.bg_color = BACKGROUND_PRESETS.get(background, (30, 30, 30))
        self.show_pads = show_pads
        self.show_boundary = show_boundary
        self.layer_filter = layer_filter

        # Calculate board dimensions and scaling
        bw = board.max_x - board.min_x
        bh = board.max_y - board.min_y
        if bw <= 0 or bh <= 0:
            bw = bh = 100000  # fallback

        # Add margin (5% on each side)
        margin_frac = 0.05
        self.margin_x = bw * margin_frac
        self.margin_y = bh * margin_frac

        total_w = bw + 2 * self.margin_x
        total_h = bh + 2 * self.margin_y

        self.scale = image_width / total_w
        self.image_height = max(1, int(total_h * self.scale))
        self.origin_x = board.min_x - self.margin_x
        self.origin_y = board.min_y - self.margin_y

    def _to_px(self, x: float, y: float) -> tuple[int, int]:
        """Convert board coordinates to pixel coordinates."""
        px = int((x - self.origin_x) * self.scale)
        py = int((y - self.origin_y) * self.scale)
        return px, py

    def _width_to_px(self, w: float) -> int:
        """Convert a board-space width to pixel width (at least 1)."""
        return max(1, int(w * self.scale))

    def render(self) -> Image.Image:
        """Render the board and routing to a PIL Image."""
        img = Image.new("RGBA", (self.image_width, self.image_height), self.bg_color + (255,))

        # Draw board outline
        if self.show_boundary:
            self._draw_boundary(img)

        # Draw wires grouped by layer (back layers first, then front)
        self._draw_all_wires(img)

        # Draw vias
        self._draw_vias(img)

        # Draw pads on top
        if self.show_pads:
            self._draw_pads(img)

        return img

    def _draw_boundary(self, img: Image.Image):
        """Draw the board outline."""
        if not self.board.boundary_points:
            return
        overlay = Image.new("RGBA", img.size, (0, 0, 0, 0))
        draw = ImageDraw.Draw(overlay)

        pts = self.board.boundary_points
        pixel_pts = [self._to_px(x, y) for x, y in pts]

        # Close the polygon
        if pixel_pts[0] != pixel_pts[-1]:
            pixel_pts.append(pixel_pts[0])

        line_width = max(2, int(1.5 * self.scale))
        for i in range(len(pixel_pts) - 1):
            draw.line(
                [pixel_pts[i], pixel_pts[i + 1]],
                fill=BOUNDARY_COLOR,
                width=line_width,
            )

        img.alpha_composite(overlay)

    def _draw_all_wires(self, img: Image.Image):
        """Draw all wires, one overlay per layer for performance."""
        # Group wires by layer
        wires_by_layer: dict[str, list[dict]] = {}
        for wire in self.routing.wires:
            layer = wire["layer"]
            if self.layer_filter and layer not in self.layer_filter:
                continue
            wires_by_layer.setdefault(layer, []).append(wire)

        # Determine draw order: known board layers back-to-front, then extras
        layer_order = list(reversed(self.board.layers)) if self.board.layers else []
        ordered_layers = [l for l in layer_order if l in wires_by_layer]
        extra = [l for l in wires_by_layer if l not in set(layer_order)]
        ordered_layers = extra + ordered_layers

        for layer in ordered_layers:
            overlay = Image.new("RGBA", img.size, (0, 0, 0, 0))
            draw = ImageDraw.Draw(overlay)
            color = _layer_color(layer)

            for wire in wires_by_layer[layer]:
                width = self._width_to_px(wire["width"])
                pixel_pts = [self._to_px(x, y) for x, y in wire["points"]]

                for i in range(len(pixel_pts) - 1):
                    draw.line(
                        [pixel_pts[i], pixel_pts[i + 1]],
                        fill=color,
                        width=width,
                    )

                # Draw rounded joints/endpoints
                r = width // 2
                if r >= 1:
                    for px, py in pixel_pts:
                        draw.ellipse(
                            [px - r, py - r, px + r, py + r],
                            fill=color,
                        )

            img.alpha_composite(overlay)

    def _draw_vias(self, img: Image.Image):
        """Draw all vias."""
        overlay = Image.new("RGBA", img.size, (0, 0, 0, 0))
        draw = ImageDraw.Draw(overlay)

        via_radius = max(3, self._width_to_px(300))
        drill_radius = max(1, via_radius // 2)

        for via in self.routing.vias:
            px, py = self._to_px(via["x"], via["y"])
            # Outer ring
            draw.ellipse(
                [px - via_radius, py - via_radius, px + via_radius, py + via_radius],
                fill=VIA_OUTER_COLOR,
            )
            # Inner hole
            draw.ellipse(
                [px - drill_radius, py - drill_radius, px + drill_radius, py + drill_radius],
                fill=VIA_INNER_COLOR,
            )

        img.alpha_composite(overlay)

    def _draw_pads(self, img: Image.Image):
        """Draw component pads."""
        overlay = Image.new("RGBA", img.size, (0, 0, 0, 0))
        draw = ImageDraw.Draw(overlay)

        for pad in self.board.pads:
            shape = pad["shape"]
            px, py = self._to_px(pad["x"], pad["y"])

            if self.layer_filter and shape.get("layer") and shape["layer"] not in self.layer_filter:
                if shape["layer"]:
                    continue

            if shape["type"] == "circle":
                r = max(2, self._width_to_px(shape["diameter"]) // 2)
                draw.ellipse(
                    [px - r, py - r, px + r, py + r],
                    fill=PAD_COLOR,
                )
            elif shape["type"] == "rect":
                hw = max(1, self._width_to_px(abs(shape["x2"] - shape["x1"])) // 2)
                hh = max(1, self._width_to_px(abs(shape["y2"] - shape["y1"])) // 2)
                draw.rectangle(
                    [px - hw, py - hh, px + hw, py + hh],
                    fill=PAD_COLOR,
                )
            elif shape["type"] == "oval":
                hw = max(1, self._width_to_px(shape["width"]) // 2)
                hh = max(1, self._width_to_px(shape["height"]) // 2)
                draw.ellipse(
                    [px - hw, py - hh, px + hw, py + hh],
                    fill=PAD_COLOR,
                )

        img.alpha_composite(overlay)


# ─── CLI ─────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="Render a routed PCB (.dsn + .ses) to a PNG image.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
  %(prog)s board.dsn board.ses
  %(prog)s board.dsn board.ses -o render.png --width 4096
  %(prog)s board.dsn board.ses --background light --dpi 300
  %(prog)s board.dsn board.ses --layers F.Cu
""",
    )
    parser.add_argument("dsn", type=Path, help="Input DSN design file")
    parser.add_argument("ses", type=Path, help="Input SES session/routing file")
    parser.add_argument(
        "-o", "--output",
        type=Path,
        default=None,
        help="Output PNG file (default: <ses_basename>.png)",
    )
    parser.add_argument(
        "--width",
        type=int,
        default=2048,
        help="Image width in pixels (default: 2048)",
    )
    parser.add_argument(
        "--dpi",
        type=int,
        default=150,
        help="Image DPI metadata (default: 150)",
    )
    parser.add_argument(
        "--layers",
        type=str,
        default=None,
        help="Comma-separated list of layers to render (default: all)",
    )
    parser.add_argument(
        "--no-pads",
        action="store_true",
        help="Do not render component pads",
    )
    parser.add_argument(
        "--no-boundary",
        action="store_true",
        help="Do not render board outline",
    )
    parser.add_argument(
        "--background",
        type=str,
        default="dark",
        choices=list(BACKGROUND_PRESETS.keys()),
        help="Background color preset (default: dark)",
    )

    args = parser.parse_args()

    # Validate inputs
    if not args.dsn.exists():
        print(f"Error: DSN file not found: {args.dsn}", file=sys.stderr)
        sys.exit(1)
    if not args.ses.exists():
        print(f"Error: SES file not found: {args.ses}", file=sys.stderr)
        sys.exit(1)

    output = args.output or args.ses.with_suffix(".png")

    # Parse files
    print(f"Parsing DSN: {args.dsn}")
    dsn_text = args.dsn.read_text(encoding="utf-8", errors="replace")
    board = parse_dsn_board(dsn_text)
    print(
        f"  Board: {board.max_x - board.min_x:.0f} x "
        f"{board.max_y - board.min_y:.0f} {board.resolution_unit}, "
        f"{len(board.layers)} layers, {len(board.pads)} pads"
    )

    print(f"Parsing SES: {args.ses}")
    ses_text = args.ses.read_text(encoding="utf-8", errors="replace")
    routing = parse_ses_routing(ses_text)
    print(f"  Routing: {len(routing.wires)} wires, {len(routing.vias)} vias")

    # Layer filter
    layer_filter = None
    if args.layers:
        layer_filter = set(l.strip() for l in args.layers.split(","))
        print(f"  Layer filter: {layer_filter}")

    # Render
    print(f"Rendering {args.width}px wide image...")
    renderer = Renderer(
        board=board,
        routing=routing,
        image_width=args.width,
        dpi=args.dpi,
        background=args.background,
        show_pads=not args.no_pads,
        show_boundary=not args.no_boundary,
        layer_filter=layer_filter,
    )
    img = renderer.render()

    # Save
    img_rgb = img.convert("RGB")
    img_rgb.save(str(output), dpi=(args.dpi, args.dpi))
    print(f"Saved: {output} ({img.width}x{img.height} px)")


if __name__ == "__main__":
    main()
