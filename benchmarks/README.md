# Benchmarks

This directory contains real-world PCB designs in Specctra DSN format used to benchmark `openrouting` against [freerouting](https://github.com/freerouting/freerouting).

## Benchmark Files

| File | Layers | Nets | Size | Description | Source |
|------|--------|------|------|-------------|--------|
| `dac2020_bm05.dsn` | 2 | 54 | 17 KB | Audio codec evaluation board — part of the DAC 2020 PCB routing academic benchmark set | [freerouting/freerouting](https://github.com/freerouting/freerouting/blob/master/tests/Issue508-DAC2020_bm05.dsn) |
| `smoothieboard.dsn` | 4 | 287 | 132 KB | [Smoothieboard v1](https://github.com/Smoothieware/Smoothieboard) 5-driver open-source CNC motion controller | [freerouting/freerouting](https://github.com/freerouting/freerouting/blob/master/tests/Issue145-smoothieboard.dsn) |

### Why these files?

- **dac2020_bm05** is one of two files used in freerouting's own `run_benchmarks.ps1` script, making it the most direct apples-to-apples comparison point.
- **smoothieboard** is a well-known open-source hardware project with a moderate-to-high complexity layout (4 copper layers, 287 nets, dense QFP and passive placement) and is widely used as an informal benchmark in the freerouting community.

## Running the Benchmarks

```bash
# 1. Build openrouting in release mode first
cargo build --release

# 2. Run benchmark (openrouting only)
cd benchmarks
./run_benchmarks.sh

# 3. Run benchmark with freerouting comparison
./run_benchmarks.sh --freerouting /path/to/freerouting-executable.jar
```

### Output example

```
========================================
  openrouting Benchmark Suite
========================================

File                            Tool          Wall time    Nets routed    Unrouted
──────────────────────────────────────────────────────────────────────────────────────────
dac2020_bm05.dsn                openrouting       0.412s             50           4
dac2020_bm05.dsn                freerouting      28.340s             54           0
smoothieboard.dsn               openrouting       3.821s            241          46
smoothieboard.dsn               freerouting     185.200s            287           0
```

> **Note**: freerouting is a mature, highly optimized router that applies multiple routing passes and optimization steps, so its routing quality will be better. These benchmarks measure raw wall-clock time to completion and routing completeness.

## Getting freerouting

Download the latest `freerouting-executable.jar` from the [freerouting releases page](https://github.com/freerouting/freerouting/releases). Java 17+ is required.
