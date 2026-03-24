#!/usr/bin/env bash
# Benchmark script: compare openrouting vs freerouting on real-world DSN files.
#
# Usage:
#   ./run_benchmarks.sh                        # run all benchmarks with openrouting only
#   ./run_benchmarks.sh --freerouting path/to/freerouting-executable.jar
#   ./run_benchmarks.sh --freerouting path/to/jar --timeout 600  # custom timeout (seconds)
#
# Requirements:
#   - openrouting must be built first: cargo build --release (from repo root)
#   - java is required only when --freerouting is supplied
#   - freerouting v2.x may not exit after routing; timeout (default 300s) kills it

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OPENROUTING="$REPO_ROOT/target/release/openrouting"
FREEROUTING_JAR=""
FREEROUTING_TIMEOUT=300   # seconds per file; freerouting may not exit cleanly
OUTPUT_DIR="$(mktemp -d)"

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --freerouting)
            FREEROUTING_JAR="$2"; shift 2 ;;
        --timeout)
            FREEROUTING_TIMEOUT="$2"; shift 2 ;;
        --help|-h)
            head -10 "$0" | tail -9; exit 0 ;;
        *)
            echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

if [[ ! -x "$OPENROUTING" ]]; then
    echo "openrouting binary not found at $OPENROUTING"
    echo "Please run: cargo build --release"
    exit 1
fi

# Benchmark DSN files: (filename, description)
declare -a DSN_FILES=(
    "dac2020_bm05.dsn"
    "smoothieboard.dsn"
)
declare -A DSN_DESC=(
    ["dac2020_bm05.dsn"]="DAC 2020 bm05 (audio codec board, 2 layers, 54 nets)"
    ["smoothieboard.dsn"]="Smoothieboard v1.1 (5-driver CNC, 4 layers, 287 nets)"
)

# Pretty-print width
COL_FILE=30
COL_TOOL=12
COL_TIME=12
COL_ROUTED=14
COL_UNROUTED=12

print_header() {
    printf "%-${COL_FILE}s  %-${COL_TOOL}s  %${COL_TIME}s  %${COL_ROUTED}s  %${COL_UNROUTED}s\n" \
        "File" "Tool" "Wall time" "Nets routed" "Unrouted"
    printf '%s\n' "$(printf '─%.0s' {1..90})"
}

print_row() {
    printf "%-${COL_FILE}s  %-${COL_TOOL}s  %${COL_TIME}s  %${COL_ROUTED}s  %${COL_UNROUTED}s\n" \
        "$1" "$2" "$3" "$4" "$5"
}

echo ""
echo "========================================"
echo "  openrouting Benchmark Suite"
echo "========================================"
echo ""

print_header

for dsn_file in "${DSN_FILES[@]}"; do
    dsn_path="$SCRIPT_DIR/$dsn_file"
    if [[ ! -f "$dsn_path" ]]; then
        echo "  [SKIP] $dsn_file — file not found"
        continue
    fi

    ses_path="$OUTPUT_DIR/$(basename "$dsn_file" .dsn).ses"

    # ─── openrouting ─────────────────────────────────────────────
    start=$(date +%s%3N)
    stderr_out=$("$OPENROUTING" "$dsn_path" --output "$ses_path" 2>&1 || true)
    end=$(date +%s%3N)
    elapsed_ms=$(( end - start ))
    elapsed_fmt="$(( elapsed_ms / 1000 )).$(printf '%03d' $(( elapsed_ms % 1000 )))s"

    routed="?"
    unrouted="?"
    if echo "$stderr_out" | grep -qE "Routed [0-9]+ nets, [0-9]+ unrouted"; then
        routed=$(echo "$stderr_out" | grep -oE "Routed [0-9]+" | grep -oE "[0-9]+")
        unrouted=$(echo "$stderr_out" | grep -oE "[0-9]+ unrouted" | grep -oE "^[0-9]+")
    fi

    print_row "$dsn_file" "openrouting" "$elapsed_fmt" "$routed" "$unrouted"

    # ─── freerouting (optional) ───────────────────────────────────
    # freerouting v2.x may not exit after routing (API server stays alive),
    # so we run it in the background and kill it once the .ses file appears
    # or the timeout expires.
    if [[ -n "$FREEROUTING_JAR" ]] && command -v java &>/dev/null; then
        ses_fr="$OUTPUT_DIR/$(basename "$dsn_file" .dsn)_freerouting.ses"
        fr_log="$OUTPUT_DIR/$(basename "$dsn_file" .dsn)_freerouting.log"

        start=$(date +%s%3N)
        java -jar "$FREEROUTING_JAR" \
            -de "$dsn_path" -do "$ses_fr" \
            -mp 1 -mt 1 \
            -ll INFO </dev/null >"$fr_log" 2>&1 &
        fr_pid=$!

        # Wait for .ses output or timeout
        elapsed_s=0
        while kill -0 "$fr_pid" 2>/dev/null; do
            if [[ -f "$ses_fr" ]]; then
                # Routing done — give a moment for final log output, then kill
                sleep 1
                break
            fi
            sleep 1
            elapsed_s=$(( elapsed_s + 1 ))
            if [[ $elapsed_s -ge $FREEROUTING_TIMEOUT ]]; then
                break
            fi
        done
        kill "$fr_pid" 2>/dev/null || true
        wait "$fr_pid" 2>/dev/null || true

        end=$(date +%s%3N)
        elapsed_ms=$(( end - start ))
        elapsed_fmt="$(( elapsed_ms / 1000 )).$(printf '%03d' $(( elapsed_ms % 1000 )))s"

        java_out=$(cat "$fr_log" 2>/dev/null || true)

        # freerouting doesn't log unrouted counts; count routed nets from .ses
        total_nets=$(grep -c "(net " "$dsn_path" 2>/dev/null || echo "0")
        if [[ -f "$ses_fr" ]]; then
            fr_routed=$(grep -cE '^\s*\(net ' "$ses_fr" 2>/dev/null || echo "0")
            if [[ "$total_nets" =~ ^[0-9]+$ && "$fr_routed" =~ ^[0-9]+$ ]]; then
                fr_unrouted=$(( total_nets - fr_routed ))
                [[ $fr_unrouted -lt 0 ]] && fr_unrouted=0
            else
                fr_unrouted="?"
            fi
        else
            fr_routed="?"
            fr_unrouted="?"
        fi

        if [[ $elapsed_s -ge $FREEROUTING_TIMEOUT ]]; then
            elapsed_fmt="${elapsed_fmt} (timeout)"
        fi

        print_row "$dsn_file" "freerouting" "$elapsed_fmt" "$fr_routed" "$fr_unrouted"
    fi
done

echo ""

# Clean up temp output
rm -rf "$OUTPUT_DIR"

echo "Benchmarks complete."
if [[ -z "$FREEROUTING_JAR" ]]; then
    echo ""
    echo "Tip: To compare against freerouting, pass --freerouting path/to/freerouting-executable.jar"
fi
echo ""
