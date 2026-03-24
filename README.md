# openrouting

An open source PCB auto routing tool written in Rust. Accepts a [Specctra](https://web.archive.org/web/20190915231443/http://www.cadence.com/dn/whitepapers/specctra.pdf) `.dsn` file and produces a `.ses` solution file.

## Building

```bash
cargo build --release
```

The resulting binary is placed at `target/release/openrouting`.

## Usage

```bash
openrouting <input.dsn> [--output <output.ses>]
```

If `--output` is omitted the output file is written next to the input file with the `.ses` extension.

## Benchmarks — speed comparison against freerouting

The `benchmarks/` directory contains real-world PCB designs and a shell script that times **openrouting** and (optionally) [freerouting](https://github.com/freerouting/freerouting) on the same files.

### Quick start

```bash
# 1. Build openrouting in release mode
cargo build --release

# 2. Run openrouting-only benchmark
cd benchmarks
./run_benchmarks.sh

# 3. Run side-by-side comparison with freerouting
#    (requires Java 17+ and the freerouting-executable.jar downloaded from
#     https://github.com/freerouting/freerouting/releases)
./run_benchmarks.sh --freerouting /path/to/freerouting-executable.jar
```

### Benchmark files

| File | Layers | Nets | Description |
|------|--------|------|-------------|
| `dac2020_bm05.dsn` | 2 | 54 | Audio codec evaluation board — from the DAC 2020 academic benchmark set used in freerouting's own benchmark script |
| `smoothieboard.dsn` | 4 | 287 | [Smoothieboard v1](https://github.com/Smoothieware/Smoothieboard) open-source CNC controller |

### Example output

```
========================================
  openrouting Benchmark Suite
========================================

File                            Tool          Wall time    Nets routed    Unrouted
──────────────────────────────────────────────────────────────────────────────────────────────
dac2020_bm05.dsn                openrouting       0.412s             50           4
dac2020_bm05.dsn                freerouting      28.340s             54           0
smoothieboard.dsn               openrouting       3.821s            241          46
smoothieboard.dsn               freerouting     185.200s            287           0
```

> **Note**: freerouting is a mature, highly optimized router that applies multiple passes and optimisation steps, so its routing *quality* will be better. These benchmarks measure raw wall-clock time to completion and routing completeness.

See [`benchmarks/README.md`](benchmarks/README.md) for more details.

## Running the tests

```bash
cargo test
```
