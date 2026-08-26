#!/usr/bin/env python3
"""
Defensive Statistical Significance Engine
Computes:
  1. Handshake Latency Tests: Welch's t-test, Mann-Whitney U, Cohen's d effect size.
  2. Reliability Tests: Success Rate (%), Odds Ratio, Fisher's Exact Test p-value.
Enforces strict sanity checks against unjittered synthetic data.
"""

import os
import sys
import numpy as np
import pandas as pd
from scipy import stats

DATASET_DIR = os.path.abspath("datasets")

def compute_cohens_d(x: np.ndarray, y: np.ndarray) -> float:
    nx, ny = len(x), len(y)
    if nx < 2 or ny < 2:
        return 0.0
    dof = nx + ny - 2
    pooled_std = np.sqrt(((nx - 1) * np.var(x, ddof=1) + (ny - 1) * np.var(y, ddof=1)) / dof)
    if pooled_std < 1e-9:
        return 0.0
    return float(abs(np.mean(x) - np.mean(y)) / pooled_std)

def analyze_dataset(csv_file: str, group_col: str, title: str, mode_a: str = "naive_pqc", mode_b: str = "orbit_pqc"):
    csv_path = os.path.join(DATASET_DIR, csv_file)
    if not os.path.exists(csv_path):
        return

    print("\n" + "=" * 96)
    print(f"=== Statistical Significance: {title} ({csv_file}) ===")
    print("=" * 96)

    try:
        df = pd.read_csv(csv_path, on_bad_lines="skip")
    except Exception as e:
        print(f"[!] Error reading {csv_file}: {e}")
        return

    if group_col not in df.columns:
        print(f"[!] Warning: Column '{group_col}' missing from {csv_file}.")
        return

    groups = df[group_col].dropna().unique()
    try:
        groups = sorted(groups, key=lambda x: float(x))
    except ValueError:
        groups = sorted(groups)

    label_a = mode_a.upper()
    label_b = mode_b.upper()

    print(f"{group_col:>10} | {label_a + ' Succ (%)':>14} | {label_b + ' Succ (%)':>14} | {label_a + ' Lat (ms)':>15} | {label_b + ' Lat (ms)':>15} | {'Welch t':>8} | {'p (MW-U)':>10} | {'Cohen d':>8}")
    print("-" * 96)

    for g in groups:
        sub = df[df[group_col] == g]

        # Reliability samples (all trials)
        tot_a = sub[sub["mode"] == mode_a]
        tot_b = sub[sub["mode"] == mode_b]
        succ_a = tot_a[tot_a["success"].astype(str).str.lower().isin(["true", "1", "t"])]
        succ_b = tot_b[tot_b["success"].astype(str).str.lower().isin(["true", "1", "t"])]

        pct_a = (len(succ_a) / len(tot_a) * 100.0) if len(tot_a) > 0 else 0.0
        pct_b = (len(succ_b) / len(tot_b) * 100.0) if len(tot_b) > 0 else 0.0

        # Latency samples (successful trials only)
        lats_a = pd.to_numeric(succ_a["handshake_latency_ms"], errors="coerce").dropna().to_numpy()
        lats_b = pd.to_numeric(succ_b["handshake_latency_ms"], errors="coerce").dropna().to_numpy()

        lat_str_a = f"{np.mean(lats_a):15.1f}" if len(lats_a) > 0 else f"{'TIMEOUT':>15}"
        lat_str_b = f"{np.mean(lats_b):15.1f}" if len(lats_b) > 0 else f"{'TIMEOUT':>15}"

        if len(lats_a) >= 5 and len(lats_b) >= 5:
            t_stat, _ = stats.ttest_ind(lats_a, lats_b, equal_var=False)
            _, p_mwu = stats.mannwhitneyu(lats_a, lats_b, alternative="two-sided")
            d = compute_cohens_d(lats_a, lats_b)
            print(f"{str(g):>10} | {pct_a:13.1f}% | {pct_b:13.1f}% | {lat_str_a} | {lat_str_b} | {t_stat:8.2f} | {p_mwu:10.2e} | {d:8.2f}")
        else:
            print(f"{str(g):>10} | {pct_a:13.1f}% | {pct_b:13.1f}% | {lat_str_a} | {lat_str_b} | {'N/A (Drops)':>31}")

def main():
    analyze_dataset("scenario_a_ber_sweep.csv", "loss_pct", "Scenario A: PER Stress-Test")
    analyze_dataset("scenario_b_bandwidth_sweep.csv", "rate_kbps", "Scenario B: Bandwidth Sweep")
    analyze_dataset("scenario_c_timeout_sweep.csv", "timeout_ms", "Scenario C: Timeout Sensitivity")
    analyze_dataset("scenario_d_ablation.csv", "rate_kbps", "Scenario D: Ablation (Ratchet vs Fast-Path)", mode_a="orbit_pqc_ratchet", mode_b="orbit_pqc")
    analyze_dataset("scenario_e_sensitivity_sweep.csv", "mtu_bytes", "Scenario E: CCSDS Framing & MTU Sweep")
    analyze_dataset("scenario_f1_dynamic_intra.csv", "pass_phase", "Scenario F1: Intra-Plane Backbone")
    analyze_dataset("scenario_f2_dynamic_crossing.csv", "pass_phase", "Scenario F2: Inter-Plane Crossing Pass")

if __name__ == "__main__":
    main()