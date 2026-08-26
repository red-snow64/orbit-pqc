#!/usr/bin/env python3
import os
import sys
import time
import subprocess
import argparse
from typing import Dict
from constellation import WalkerDeltaConstellation

def execute_cmd(cmd: str, check: bool = True) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, shell=True, text=True, capture_output=True, check=check)

def set_kernel_link_params(delay_ms: float, loss_pct: float, rate_kbps: int):
    """Executes dynamic link modulation on both namespaces."""
    script_path = os.path.abspath("scripts/modulate_link.sh")
    cmd = f"sudo {script_path} {delay_ms:.2f} {loss_pct:.2f} {rate_kbps}"
    res = execute_cmd(cmd, check=False)
    if res.returncode != 0:
        print(f"[-] Warning: Failed to modulate tc-netem: {res.stderr.strip()}", file=sys.stderr)

def run_live_orbital_pass(
        mode: str,
        sat_a_id: str = "SAT_P1_N1",
        sat_b_id: str = "SAT_P1_N2",
        pass_duration_s: int = 45,
        rate_kbps: int = 128,
        base_loss_pct: float = 1.0,
        output_csv: str = "datasets/live_pass_telemetry.csv"
):
    print(f"[*] Initializing Orbital Link Engine: {sat_a_id} <-> {sat_b_id} (Duration: {pass_duration_s}s)")
    constellation = WalkerDeltaConstellation()
    trajectory = constellation.compute_isl_trajectory(sat_a_id, sat_b_id, duration_sec=pass_duration_s, dt_sec=0.5)

    # Filter to active LOS contact window
    active_points = [p for p in trajectory if p["has_los"]]
    if not active_points:
        print("[-] Error: No Line-of-Sight visibility between selected satellites.")
        sys.exit(1)

    print(f"[+] Active LOS Window: {len(active_points) * 0.5:.1f}s | Mean Distance: {np.mean([p['distance_km'] for p in active_points]):.1f} km")

    node_bin = os.path.abspath("target/release/orbit-node")
    if not os.path.exists(node_bin):
        print(f"[-] Error: Binary {node_bin} not found. Run 'cargo build --release' first.", file=sys.stderr)
        sys.exit(1)

    session_id = 1000
    for point in active_points:
        t_sec = point["time_offset_s"]
        prop_delay_ms = point["prop_delay_ms"]
        distance_km = point["distance_km"]

        # Elevation-dependent dynamic loss modulation
        dynamic_loss = base_loss_pct + (0.5 if distance_km > 3000 else 0.0)

        # 1. Update Linux Traffic Control
        set_kernel_link_params(prop_delay_ms, dynamic_loss, rate_kbps)
        session_id += 1

        print(f"[@ T+{t_sec:04.1f}s] Distance: {distance_km:.1f}km | Delay: {prop_delay_ms:.2f}ms | Loss: {dynamic_loss:.1f}% -> Triggering {mode}")

        # 2. Spawn Responder inside sat_b namespace (non-blocking)
        resp_cmd = f"sudo ip netns exec sat_b {node_bin} --role sat_b --mode {mode} --session-id {session_id} --output-csv {output_csv}"
        responder_proc = subprocess.Popen(resp_cmd, shell=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        time.sleep(0.02)  # 20ms socket setup headroom

        # 3. Spawn Initiator inside sat_a namespace (blocking execution)
        init_cmd = f"sudo ip netns exec sat_a {node_bin} --role sat_a --mode {mode} --session-id {session_id} --output-csv {output_csv}"
        init_res = execute_cmd(init_cmd, check=False)

        responder_proc.wait(timeout=5)

        if init_res.returncode == 0:
            print(f"    [+] Handshake Succeeded (Session {session_id})")
        else:
            print(f"    [-] Handshake Dropped / Timed out (Session {session_id})")

        time.sleep(0.48)  # Maintain 0.5s time step tick

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="ORBIT-PQC Live Orbital Link Driver")
    parser.add_argument("--mode", choices=["classical", "naive_pqc", "orbit_pqc"], default="orbit_pqc")
    parser.add_argument("--duration", type=int, default=30, help="Pass duration in seconds")
    parser.add_argument("--rate", type=int, default=128, help="ISL Channel rate in kbps")
    parser.add_argument("--loss", type=float, default=2.0, help="Base packet loss percentage")
    parser.add_argument("--output", default="datasets/live_pass_telemetry.csv")
    args = parser.parse_args()

    run_live_orbital_pass(
        mode=args.mode,
        pass_duration_s=args.duration,
        rate_kbps=args.rate,
        base_loss_pct=args.loss,
        output_csv=args.output
    )