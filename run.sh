#!/usr/bin/env bash
set -euo pipefail

# Ensure controlling TTY is restored to sane mode on exit or interrupt
cleanup_tty() {
    if [ -t 1 ] && [ -e /dev/tty ]; then
        stty sane onlcr opost < /dev/tty 2>/dev/null || true
    else
        stty sane onlcr opost 2>/dev/null || true
    fi
}
trap cleanup_tty EXIT INT TERM

if [ "$EUID" -ne 0 ]; then
    echo "[-] Please run with sudo: sudo ./run.sh [-s <A|B|C|D|E|F1|F2|F|all|custom>]"
    exit 1
fi

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$PROJECT_ROOT"

TARGET="all"
EXTRA_ARGS=()
while [[ $# -gt 0 ]]; do
    case "$1" in
        -s|--scenario)
            TARGET="$2"
            shift 2
            ;;
        --all)
            TARGET="all"
            shift
            ;;
        *)
            EXTRA_ARGS+=("$1")
            shift
            ;;
    esac
done

ORIGINAL_USER="${SUDO_USER:-$USER}"
PYTHON="$PROJECT_ROOT/.venv/bin/python3"

# 1. Build Rust release binary if missing
if [ ! -f "target/release/orbit-node" ]; then
    echo "[*] Step 1: Compiling Rust workspace in release mode..."
    su - "$ORIGINAL_USER" -c "cd '$PROJECT_ROOT' && cargo build --release"
fi

# 2. Virtual Environment Setup
if [ ! -f "$PYTHON" ]; then
    echo "[*] Setting up Python virtual environment..."
    su - "$ORIGINAL_USER" -c "cd '$PROJECT_ROOT' && python3 -m venv .venv && .venv/bin/pip install -r python/requirements.txt"
fi

# 3. Propagate SGP4 Orbits if needed
if [[ "${TARGET^^}" =~ ^(F|F1|F2|ALL)$ ]]; then
    echo "[*] Propagating SGP4 constellation physics..."
    $PYTHON python/experiments/sgp4_emulator.py
fi

# 4. Execute Selected Scenario(s)
echo "[*] Executing testbed scenario: ${TARGET^^}..."
$PYTHON python/experiments/runner.py --scenario "$TARGET" "${EXTRA_ARGS[@]}"

# 5. Run Statistical Significance & LaTeX Macro Generation
if [[ "${TARGET^^}" == "ALL" ]]; then
    echo "[*] Computing statistical significance on raw logs..."
    $PYTHON python/experiments/stats.py

    echo "[*] Parsing summaries and generating publication plots..."
    $PYTHON python/experiments/parser.py
    $PYTHON python/plots/generate_ieee_plots.py 2>/dev/null || true

    if command -v pdflatex &> /dev/null; then
        echo "[*] Compiling IEEE brief paper..."
        cd paper
        pdflatex -interaction=batchmode brief.tex > /dev/null || true
        pdflatex -interaction=batchmode brief.tex > /dev/null || true
        echo "[+] Generated paper/brief.pdf"
    fi
fi

cleanup_tty
echo "======================================================="
echo "[+] Execution complete for Scenario: ${TARGET^^}"
echo "======================================================="