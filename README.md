# openrouting
An open source PCB auto routing tool

## WARNING
This is 100% AI Generated code, exercise with caution.

## Usage

```bash
# Route a DSN file (produces a .ses file)
openrouting board.dsn --output board.ses
```

## Rendering

A Python script is included to generate PNG images of routed boards from the
`.dsn` (design) and `.ses` (session/routing) files.

### Setup

```bash
pip install -r scripts/requirements.txt
```

### Render a board

```bash
python3 scripts/render_ses.py board.dsn board.ses -o board.png
```

### Options

| Flag | Description | Default |
|---|---|---|
| `-o`, `--output` | Output PNG path | `<ses_name>.png` |
| `--width` | Image width in pixels | 2048 |
| `--dpi` | Image DPI metadata | 150 |
| `--layers` | Comma-separated layer filter | all layers |
| `--no-pads` | Hide component pads | show |
| `--no-boundary` | Hide board outline | show |
| `--background` | `dark`, `light`, `black`, `white` | `dark` |

### Examples

```bash
# High-resolution render
python3 scripts/render_ses.py board.dsn board.ses --width 4096 --dpi 300

# Render only front copper layer on a light background
python3 scripts/render_ses.py board.dsn board.ses --layers F.Cu --background light

# Render without pads
python3 scripts/render_ses.py board.dsn board.ses --no-pads
```
