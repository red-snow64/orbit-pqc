#!/usr/bin/env python3
"""
ORBIT-PQC Complete Experiment Runner
Executes Scenarios A through F2 and custom stress runs:
  - Scenario A: PER Stress-Test (0.0% to 15.0% loss)
  - Scenario B: Bandwidth Bottleneck Sweep (32 to 1024 kbps)
  - Scenario C: Timeout / RTO Sensitivity (250 to 3000 ms)
  - Scenario D: Ablation (Fast-Path Ticket vs Full ML-KEM Ratchet Re-Key)
  - Scenario E: Framing & MTU Sweep (256, 576, 1280, 1500 B)
  - Scenario F1: Live SGP4 Intra-Plane Ring Backbone
  - Scenario F2: Live SGP4 Inter-Plane Crossing Pass
  - Custom: Arbitrary parameter execution with sidecar logging
"""

import os
import sys
import termios
import json
import time
import argparse
import datetime
import subprocess
import numpy as np
import pandas as pd

try:
    from provenance import finalize as finalize_provenance
except ImportError:
    finalize_provenance = None

DATASET_DIR = os.path.abspath("datasets")
NODE_BIN = os.path.abspath("target/release/orbit-node")
SCRIPT_DIR = os.path.abspath("scripts")

RUN_IDS = {
    "A": "scenario_a",
    "B": "scenario_b",
    "C": "scenario_c",
    "D": "scenario_d",
    "E": "scenario_e",
    "F1": "scenario_f1",
    "F2": "scenario_f2",
}


def fix_terminal():
    """Directly restores OPOST and ONLCR flags on the controlling TTY."""
    for stream in (sys.stdout, sys.stdin):
        try:
            if stream.isatty():
                fd = stream.fileno()
                attrs = termios.tcgetattr(fd)
                attrs[1] |= (termios.OPOST | termios.ONLCR)
                termios.tcsetattr(fd, termios.TCSANOW, attrs)
        except Exception:
            pass

    try:
        with open("/dev/tty", "w") as tty:
            subprocess.run(
                ["stty", "sane", "onlcr", "opost"],
                stdin=tty,
                stdout=tty,
                stderr=subprocess.DEVNULL,
            )
    except Exception:
        pass

    sys.stdout.write("\r")
    sys.stdout.flush()


def tprint(*args, **kwargs):
    """Safe print wrapper that always starts at column 0 and cleans TTY."""
    sys.stdout.write("\r")
    print(*args, **kwargs)
    sys.stdout.flush()
    fix_terminal()


def setup_environment():
    os.makedirs(DATASET_DIR, exist_ok=True)
    fix_terminal()
    subprocess.run(f"bash {SCRIPT_DIR}/setup_netns.sh", shell=True, check=True)
    fix_terminal()


def modulate_link(delay_ms: float, loss_pct: float, rate_kbps: int):
    cmd = f"sudo bash {SCRIPT_DIR}/modulate_link.sh {float(delay_ms):.2f} {float(loss_pct):.2f} {int(rate_kbps)}"
    subprocess.run(cmd, shell=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def cleanup_stale_processes():
    subprocess.run(
        "sudo pkill -9 -f orbit-node 2>/dev/null || true",
        shell=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def log_provenance(run_id: str, params: dict):
    meta = {
        "run_id": run_id,
        "timestamp_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
        "git_commit": subprocess.getoutput("git rev-parse --short HEAD 2>/dev/null || echo unknown"),
        "parameters": params,
        "command_line": sys.argv,
        "provenance_status": "pending_finalization",
    }
    with open(os.path.join(DATASET_DIR, f"{run_id}_metadata.json"), "w", encoding="utf-8") as f:
        json.dump(meta, f, indent=2)


def finalize_run_provenance(run_id: str):
    """Finalize a completed dataset sidecar without changing measurements."""
    if finalize_provenance is None:
        tprint(f"[!] Provenance finalizer unavailable for {run_id}; sidecar remains pending.")
        return False
    try:
        path = finalize_provenance(run_id)
        tprint(f"[+] Provenance finalized: {path}")
        return True
    except Exception as exc:
        tprint(f"[!] Provenance finalization failed for {run_id}: {exc}")
        return False


def compute_trial_timeout_ms(mode: str, rate_kbps: int, delay_ms: float) -> int:
    """Calculates the physical worst-case ARQ budget to prevent premature clock cutoffs."""
    if mode == "classical":
        stages = [96, 96, 32]
    elif mode in ["naive_pqc", "naivepqc"]:
        stages = [4861, 4861, 64]
    elif mode in ["orbit_pqc", "orbitpqc"]:
        stages = [48, 32, 16]
    else:
        stages = [1568, 1568, 32]

    total_budget_ms = 0.0
    for s_bytes in stages:
        ser_ms = (s_bytes * 8.0) / float(rate_kbps)
        rto_0 = 2.0 * delay_ms + ser_ms + 50.0
        total_budget_ms += 31.0 * rto_0

    return max(int(total_budget_ms * 1.30), 5000)


def run_single_trial(
    mode: str,
    trial_id: int,
    altitude_km: float,
    pass_phase: str,
    delay_ms: float,
    loss_pct: float,
    rate_kbps: int,
    mtu: int,
    timeout_ms: int,
    output_csv: str,
    enforce_strict_timeout: bool = False,
) -> bool:
    cleanup_stale_processes()
    modulate_link(delay_ms, loss_pct, rate_kbps)

    port = 5000 + (trial_id % 1000)

    if enforce_strict_timeout:
        effective_timeout = timeout_ms
    else:
        calculated_timeout = compute_trial_timeout_ms(mode, rate_kbps, delay_ms)
        effective_timeout = max(timeout_ms, calculated_timeout)

    server_timeout = effective_timeout + 3000

    resp_cmd = (
        f"sudo ip netns exec sat_b {NODE_BIN} "
        f"--role sat_b --mode {mode} "
        f"--bind 10.0.0.2:{port} --peer 10.0.0.1:{port} "
        f"--trial-id {trial_id} --altitude-km {altitude_km} --pass-phase {pass_phase} "
        f"--rate-kbps {rate_kbps} --mtu {mtu} --loss-pct {loss_pct} "
        f"--timeout-ms {server_timeout} --output-csv {output_csv}"
    )
    resp_proc = subprocess.Popen(
        resp_cmd,
        shell=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    time.sleep(0.08)

    init_cmd = (
        f"sudo ip netns exec sat_a {NODE_BIN} "
        f"--role sat_a --mode {mode} "
        f"--bind 10.0.0.1:{port} --peer 10.0.0.2:{port} "
        f"--trial-id {trial_id} --altitude-km {altitude_km} --pass-phase {pass_phase} "
        f"--rate-kbps {rate_kbps} --mtu {mtu} --loss-pct {loss_pct} "
        f"--timeout-ms {effective_timeout} --output-csv {output_csv}"
    )
    init_res = subprocess.run(
        init_cmd,
        shell=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )

    try:
        resp_proc.wait(timeout=(effective_timeout / 1000.0) + 0.5)
    except subprocess.TimeoutExpired:
        resp_proc.kill()

    return init_res.returncode == 0


def get_batch_stats(output_csv: str, mode: str, start_id: int, end_id: int):
    if not os.path.exists(output_csv):
        return None
    try:
        df = pd.read_csv(output_csv, on_bad_lines="skip")
        if df.empty or "mode" not in df.columns:
            return None

        norm_target = mode.lower().replace("_", "").replace("-", "")
        norm_modes = df["mode"].astype(str).str.lower().str.replace("_", "").str.replace("-", "")
        is_success = df["success"].astype(str).str.lower().isin(["true", "1", "t"])

        sub = df[
            (norm_modes == norm_target)
            & (df["trial_id"] >= start_id)
            & (df["trial_id"] <= end_id)
            & is_success
        ]
        lats = pd.to_numeric(sub["handshake_latency_ms"], errors="coerce").dropna().to_numpy()
        bytes_sent = pd.to_numeric(sub["bytes_sent"], errors="coerce").dropna().to_numpy()
        bytes_recv = pd.to_numeric(sub["bytes_recv"], errors="coerce").dropna().to_numpy()

        if len(lats) == 0:
            return None

        return {
            "mean": float(np.mean(lats)),
            "p50": float(np.percentile(lats, 50)),
            "p95": float(np.percentile(lats, 95)),
            "p99": float(np.percentile(lats, 99)),
            "wire_bytes": float(np.mean(bytes_sent + bytes_recv)) if len(bytes_sent) > 0 else 0.0,
        }
    except Exception:
        return None


def run_scenario_a(trials: int = 30):
    fix_terminal()
    tprint("\n" + "=" * 80, flush=True)
    tprint(f"[*] Scenario A: PER Stress-Test ({trials} trials/point @ 128 kbps)", flush=True)
    tprint("=" * 80, flush=True)
    out_csv = os.path.join(DATASET_DIR, "scenario_a_ber_sweep.csv")
    if os.path.exists(out_csv):
        os.remove(out_csv)
    loss_rates = [0.0, 1.0, 2.5, 5.0, 7.5, 10.0, 12.5, 15.0]
    log_provenance("scenario_a", {"trials": trials, "loss_rates": loss_rates, "rate_kbps": 128, "mtu": 1500})

    counter = 10000
    for mode in ["classical", "naive_pqc", "orbit_pqc"]:
        fix_terminal()
        tprint(f"\n--- Protocol Mode: {mode.upper()} ---", flush=True)
        for loss in loss_rates:
            start_id, succ = counter + 1, 0
            for _ in range(trials):
                counter += 1
                if run_single_trial(mode, counter, 550.0, "intra_plane", 6.67, loss, 128, 1500, 3000, out_csv):
                    succ += 1
            stats = get_batch_stats(out_csv, mode, start_id, counter)
            succ_pct = (succ / trials) * 100.0
            if stats:
                tprint(f"  Loss: {loss:4.1f}% | Success: {succ_pct:5.1f}% ({succ:2d}/{trials}) | Mean: {stats['mean']:6.1f} ms | p95: {stats['p95']:6.1f} ms", flush=True)
            else:
                tprint(f"  Loss: {loss:4.1f}% | Success: {succ_pct:5.1f}% ({succ:2d}/{trials}) | TIMEOUT", flush=True)
            fix_terminal()


def run_scenario_b(trials: int = 20):
    fix_terminal()
    tprint("\n" + "=" * 80, flush=True)
    tprint(f"[*] Scenario B: Bandwidth Bottleneck Sweep (32 to 1024 kbps @ 3.0% PER)", flush=True)
    tprint("=" * 80, flush=True)
    out_csv = os.path.join(DATASET_DIR, "scenario_b_bandwidth_sweep.csv")
    if os.path.exists(out_csv):
        os.remove(out_csv)
    rates = [32, 64, 128, 256, 512, 1024]
    log_provenance("scenario_b", {"trials": trials, "rates_kbps": rates, "loss_pct": 3.0, "mtu": 1500})

    counter = 20000
    for mode in ["classical", "naive_pqc", "orbit_pqc"]:
        fix_terminal()
        tprint(f"\n--- Protocol Mode: {mode.upper()} ---", flush=True)
        for rate in rates:
            start_id, succ = counter + 1, 0
            for _ in range(trials):
                counter += 1
                if run_single_trial(mode, counter, 550.0, "intra_plane", 6.67, 3.0, rate, 1500, 4500, out_csv):
                    succ += 1
            stats = get_batch_stats(out_csv, mode, start_id, counter)
            succ_pct = (succ / trials) * 100.0
            if stats:
                eta_30s = max(0.0, (30.0 - (stats["p95"] / 1000.0)) / 30.0 * 100.0)
                tprint(f"  Rate: {rate:4d} kbps | Success: {succ_pct:5.1f}% | Mean: {stats['mean']:6.1f} ms | p95: {stats['p95']:6.1f} ms | eta_win(30s): {eta_30s:5.2f}%", flush=True)
            else:
                tprint(f"  Rate: {rate:4d} kbps | Success: {succ_pct:5.1f}% | TIMEOUT", flush=True)
            fix_terminal()


def run_scenario_c(trials: int = 30):
    tprint("\n" + "=" * 80)
    tprint(f"[*] Scenario C: Timeout / RTO Sensitivity Sweep ({trials} trials/point @ 128 kbps, 5.0% PER)")
    tprint("=" * 80)
    out_csv = os.path.join(DATASET_DIR, "scenario_c_timeout_sweep.csv")
    if os.path.exists(out_csv):
        os.remove(out_csv)
    timeouts = [250, 500, 1000, 1500, 2000, 3000]
    log_provenance("scenario_c", {"trials": trials, "timeouts_ms": timeouts, "rate_kbps": 128, "loss_pct": 5.0})

    counter = 30000
    for mode in ["naive_pqc", "orbit_pqc"]:
        tprint(f"\n--- Protocol Mode: {mode.upper()} ---")
        for t_ms in timeouts:
            start_id, succ = counter + 1, 0
            for _ in range(trials):
                counter += 1
                if run_single_trial(mode, counter, 550.0, "intra_plane", 6.67, 5.0, 128, 1500, t_ms, out_csv, enforce_strict_timeout=True):
                    succ += 1
            stats = get_batch_stats(out_csv, mode, start_id, counter)
            succ_pct = (succ / trials) * 100.0
            if stats:
                tprint(f"  Timeout: {t_ms:4d} ms | Success: {succ_pct:5.1f}% ({succ:2d}/{trials}) | Mean: {stats['mean']:6.1f} ms | p95: {stats['p95']:6.1f} ms")
            else:
                tprint(f"  Timeout: {t_ms:4d} ms | Success: {succ_pct:5.1f}% ({succ:2d}/{trials}) | COMPLETE TIMEOUT")
            fix_terminal()


def run_scenario_d(trials: int = 20):
    fix_terminal()
    tprint("\n" + "=" * 80, flush=True)
    tprint(f"[*] Scenario D: Ablation (Fast-Path Ticket vs Full ML-KEM-1024 Ratchet)", flush=True)
    tprint("=" * 80, flush=True)
    out_csv = os.path.join(DATASET_DIR, "scenario_d_ablation.csv")
    if os.path.exists(out_csv):
        os.remove(out_csv)
    rates = [64, 128, 256, 512, 1024]
    log_provenance("scenario_d", {"trials": trials, "rates_kbps": rates, "loss_pct": 1.0, "mtu": 1500})

    counter = 40000
    for mode in ["orbit_pqc", "orbit_pqc_ratchet"]:
        fix_terminal()
        label = "FAST-PATH TICKET (48 B)" if mode == "orbit_pqc" else "ASYNC RATCHET (ML-KEM-1024)"
        tprint(f"\n--- Mode: {label} ---", flush=True)
        for rate in rates:
            start_id, succ = counter + 1, 0
            for _ in range(trials):
                counter += 1
                if run_single_trial(mode, counter, 550.0, "intra_plane", 6.67, 1.0, rate, 1500, 4000, out_csv):
                    succ += 1
            stats = get_batch_stats(out_csv, mode, start_id, counter)
            succ_pct = (succ / trials) * 100.0
            if stats:
                cost_30s = (stats["mean"] / 30000.0) * 100.0
                tprint(f"  Rate: {rate:4d} kbps | Success: {succ_pct:5.1f}% | Mean: {stats['mean']:6.1f} ms | p95: {stats['p95']:6.1f} ms | 30s Pass Cost: {cost_30s:4.2f}%", flush=True)
            else:
                tprint(f"  Rate: {rate:4d} kbps | Success: {succ_pct:5.1f}% | TIMEOUT", flush=True)
            fix_terminal()


def run_scenario_e(trials: int = 20):
    fix_terminal()
    tprint("\n" + "=" * 80, flush=True)
    tprint(f"[*] Scenario E: Empirical CCSDS Framing & MTU Sweep (256 to 1500 B @ 128 kbps)", flush=True)
    tprint("=" * 80, flush=True)
    out_csv = os.path.join(DATASET_DIR, "scenario_e_sensitivity_sweep.csv")
    if os.path.exists(out_csv):
        os.remove(out_csv)
    mtu_list = [256, 576, 1280, 1500]
    log_provenance("scenario_e", {"trials": trials, "mtu_list": mtu_list, "rate_kbps": 128, "loss_pct": 3.0})

    counter = 50000
    for mode in ["naive_pqc", "orbit_pqc"]:
        fix_terminal()
        tprint(f"\n--- Protocol Mode: {mode.upper()} ---", flush=True)
        for mtu in mtu_list:
            start_id, succ = counter + 1, 0
            for _ in range(trials):
                counter += 1
                if run_single_trial(mode, counter, 550.0, "intra_plane", 6.67, 3.0, 128, mtu, 4500, out_csv):
                    succ += 1
            stats = get_batch_stats(out_csv, mode, start_id, counter)
            succ_pct = (succ / trials) * 100.0
            if stats:
                tprint(f"  MTU: {mtu:4d} B | Success: {succ_pct:5.1f}% | Mean: {stats['mean']:6.1f} ms | p95: {stats['p95']:6.1f} ms | Wire Bytes: {int(stats['wire_bytes']):5d} B", flush=True)
            else:
                tprint(f"  MTU: {mtu:4d} B | Success: {succ_pct:5.1f}% | TIMEOUT", flush=True)
            fix_terminal()


def run_scenario_f1_intra(trials: int = 30):
    fix_terminal()
    tprint("\n" + "=" * 80, flush=True)
    tprint(f"[*] Scenario F1: Live SGP4 Intra-Plane Ring Backbone", flush=True)
    tprint("=" * 80, flush=True)
    trace_path = os.path.join(DATASET_DIR, "sgp4_intra_plane_trace.csv")
    if not os.path.exists(trace_path):
        subprocess.run(f"{sys.executable} python/experiments/sgp4_emulator.py", shell=True, check=True)
    df_trace = pd.read_csv(trace_path)

    out_csv = os.path.join(DATASET_DIR, "scenario_f1_dynamic_intra.csv")
    if os.path.exists(out_csv):
        os.remove(out_csv)
    log_provenance("scenario_f1", {"trials": trials, "trace": "sgp4_intra_plane_trace.csv"})

    counter = 60000
    for mode in ["classical", "naive_pqc", "orbit_pqc"]:
        fix_terminal()
        tprint(f"\n--- Protocol Mode: {mode.upper()} ---", flush=True)
        start_id, succ = counter + 1, 0
        for i in range(trials):
            counter += 1
            idx = int(i * (len(df_trace) - 1) / (trials - 1))
            sample = df_trace.iloc[idx]
            if run_single_trial(mode, counter, float(sample["altitude_km"]), str(sample["pass_phase"]), float(sample["delay_ms"]), float(sample["loss_pct"]), 128, 1500, 3500, out_csv):
                succ += 1
        stats = get_batch_stats(out_csv, mode, start_id, counter)
        succ_pct = (succ / trials) * 100.0
        if stats:
            tprint(f"  Intra Backbone Completion: {succ_pct:5.1f}% ({succ:2d}/{trials}) | Mean: {stats['mean']:6.1f} ms | p95: {stats['p95']:6.1f} ms", flush=True)
        else:
            tprint(f"  Intra Backbone Completion: {succ_pct:5.1f}% | TIMEOUT", flush=True)
        fix_terminal()


def run_scenario_f2_crossing(trials: int = 30):
    fix_terminal()
    tprint("\n" + "=" * 80, flush=True)
    tprint(f"[*] Scenario F2: Live SGP4 Hyper-Velocity Inter-Plane Crossing Pass", flush=True)
    tprint("=" * 80, flush=True)
    trace_path = os.path.join(DATASET_DIR, "sgp4_crossing_pass_trace.csv")
    if not os.path.exists(trace_path):
        subprocess.run(f"{sys.executable} python/experiments/sgp4_emulator.py", shell=True, check=True)
    df_trace = pd.read_csv(trace_path)

    out_csv = os.path.join(DATASET_DIR, "scenario_f2_dynamic_crossing.csv")
    if os.path.exists(out_csv):
        os.remove(out_csv)
    log_provenance("scenario_f2", {"trials": trials, "trace": "sgp4_crossing_pass_trace.csv"})

    counter = 70000
    for mode in ["classical", "naive_pqc", "orbit_pqc"]:
        fix_terminal()
        tprint(f"\n--- Protocol Mode: {mode.upper()} ---", flush=True)
        start_id, succ = counter + 1, 0
        for i in range(trials):
            counter += 1
            idx = int(i * (len(df_trace) - 1) / (trials - 1))
            sample = df_trace.iloc[idx]
            if run_single_trial(mode, counter, float(sample["altitude_km"]), str(sample["pass_phase"]), float(sample["delay_ms"]), float(sample["loss_pct"]), 128, 1500, 3500, out_csv):
                succ += 1
        stats = get_batch_stats(out_csv, mode, start_id, counter)
        succ_pct = (succ / trials) * 100.0
        if stats:
            cost_30s = (stats["mean"] / 30000.0) * 100.0
            eta_win = max(0.0, (30.0 - (stats["p95"] / 1000.0)) / 30.0 * 100.0)
            tprint(f"  Crossing Pass Completion: {succ_pct:5.1f}% ({succ:2d}/{trials}) | Mean: {stats['mean']:6.1f} ms | p95: {stats['p95']:6.1f} ms | 30s Pass Cost: {cost_30s:.2f}% | eta_win: {eta_win:.2f}%", flush=True)
        else:
            tprint(f"  Crossing Pass Completion: {succ_pct:5.1f}% | TIMEOUT", flush=True)
        fix_terminal()


def run_custom_scenario(mode: str, rate: int, loss: float, mtu: int, delay: float, timeout: int, trials: int):
    fix_terminal()
    tprint("\n" + "=" * 80, flush=True)
    tprint(f"[*] Executing Custom Experiment: Mode={mode}, Rate={rate}k, Loss={loss}%, MTU={mtu}B, Delay={delay}ms", flush=True)
    tprint("=" * 80, flush=True)
    out_csv = os.path.join(DATASET_DIR, "custom_experiment.csv")
    if os.path.exists(out_csv):
        os.remove(out_csv)
    log_provenance("custom_run", {"mode": mode, "rate_kbps": rate, "loss_pct": loss, "mtu": mtu, "delay_ms": delay, "timeout_ms": timeout, "trials": trials})

    counter = 80000
    start_id, succ = counter + 1, 0
    for _ in range(trials):
        counter += 1
        if run_single_trial(mode, counter, 550.0, "custom", delay, loss, rate, mtu, timeout, out_csv):
            succ += 1
    stats = get_batch_stats(out_csv, mode, start_id, counter)
    succ_pct = (succ / trials) * 100.0
    if stats:
        tprint(f"  Result: Success={succ_pct:5.1f}% ({succ}/{trials}) | Mean: {stats['mean']:6.1f} ms | p95: {stats['p95']:6.1f} ms | Wire Bytes: {int(stats['wire_bytes'])} B", flush=True)
    else:
        tprint(f"  Result: Success={succ_pct:5.1f}% | ALL TIMED OUT", flush=True)
    fix_terminal()


if __name__ == "__main__":
    if os.geteuid() != 0:
        tprint("[-] Must be run as root.")
        sys.exit(1)

    parser = argparse.ArgumentParser(description="ORBIT-PQC Complete Experiment Runner")
    parser.add_argument("-s", "--scenario", type=str, default="all", help="Target: A, B, C, D, E, F1, F2, all, custom")
    parser.add_argument("--mode", type=str, default="orbit_pqc")
    parser.add_argument("--rate", type=int, default=128)
    parser.add_argument("--loss", type=float, default=3.0)
    parser.add_argument("--mtu", type=int, default=1500)
    parser.add_argument("--delay", type=float, default=6.67)
    parser.add_argument("--timeout", type=int, default=3500)
    parser.add_argument("--trials", type=int, default=20)
    args = parser.parse_args()

    setup_environment()
    target = args.scenario.upper()

    scenario_map = {
        "A": run_scenario_a,
        "B": run_scenario_b,
        "C": run_scenario_c,
        "D": run_scenario_d,
        "E": run_scenario_e,
        "F1": run_scenario_f1_intra,
        "F2": run_scenario_f2_crossing,
    }

    try:
        if target == "ALL":
            for name, func in scenario_map.items():
                func()
                finalize_run_provenance(RUN_IDS[name])
        elif target == "CUSTOM":
            run_custom_scenario(args.mode, args.rate, args.loss, args.mtu, args.delay, args.timeout, args.trials)
            finalize_run_provenance("custom_run")
        elif target in scenario_map:
            scenario_map[target]()
            finalize_run_provenance(RUN_IDS[target])
        else:
            tprint(f"[-] Unknown scenario '{target}'. Choose from: A, B, C, D, E, F1, F2, custom, all")
            sys.exit(1)
    finally:
        fix_terminal()
