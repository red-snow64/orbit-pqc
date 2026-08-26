#!/usr/bin/env python3
"""
Publication-Grade IEEE Plot Generator
Renders 3-panel vector graphic from empirical testbed CSV datasets.
Output: figures/fig_combined_empirical_ieee.pdf
"""

import os
import numpy as np
import pandas as pd
import matplotlib.pyplot as plt
from matplotlib.ticker import ScalarFormatter

DATASET_DIR = os.path.abspath("datasets")
FIGURE_DIR = os.path.abspath("figures")
os.makedirs(FIGURE_DIR, exist_ok=True)

plt.rcParams.update({
    "font.family": "serif",
    "font.serif": ["Times New Roman", "Times", "DejaVu Serif"],
    "font.size": 8,
    "axes.labelsize": 8,
    "axes.titlesize": 8.5,
    "xtick.labelsize": 7.5,
    "ytick.labelsize": 7.5,
    "legend.fontsize": 7,
    "mathtext.fontset": "stix",
    "lines.linewidth": 1.2,
    "lines.markersize": 4,
    "grid.alpha": 0.3,
    "grid.linestyle": "--",
})

C_CLASSICAL = "#1f77b4"
C_NAIVE     = "#d95f02"
C_ORBIT     = "#1b9e77"
C_RATCHET   = "#7570b3"

def safe_load_csv(filename: str):
    p = os.path.join(DATASET_DIR, filename)
    if not os.path.exists(p):
        return None
    try:
        df = pd.read_csv(p, on_bad_lines="skip")
        df["handshake_latency_ms"] = pd.to_numeric(df["handshake_latency_ms"], errors="coerce")
        df["success"] = df["success"].astype(str).str.lower().isin(["true", "1", "t"])
        return df[df["success"] & df["handshake_latency_ms"].notna()]
    except Exception:
        return None

def main():
    fig, axes = plt.subplots(1, 3, figsize=(7.0, 2.15))
    plt.subplots_adjust(top=0.86, bottom=0.20, left=0.08, right=0.98, wspace=0.32)

    # Panel (a): Scenario A
    ax = axes[0]
    df_a = safe_load_csv("scenario_a_ber_sweep.csv")
    if df_a is not None and not df_a.empty:
        for mode, label, color, marker in [
            ("classical", "Classical (ECDH)", C_CLASSICAL, "o"),
            ("naive_pqc", "Naive PQC (ML-KEM/DSA)", C_NAIVE, "s"),
            ("orbit_pqc", "ORBIT-PQC (Ours)", C_ORBIT, "D"),
        ]:
            sub = df_a[df_a["mode"] == mode]
            if sub.empty:
                continue
            losses = sorted(sub["loss_pct"].unique())
            means, p95s = [], []
            for l in losses:
                lats = sub[sub["loss_pct"] == l]["handshake_latency_ms"].values
                means.append(np.mean(lats))
                p95s.append(np.percentile(lats, 95))

            err_up = np.array(p95s) - np.array(means)
            ax.errorbar(losses, means, yerr=[np.zeros_like(err_up), err_up],
                        label=label, color=color, marker=marker, capsize=2, capthick=0.8)

    ax.set_title("(a) Latency vs. Loss (128 kbps)")
    ax.set_xlabel("Packet Loss Rate $P_{\\mathrm{loss}}$ (%)")
    ax.set_ylabel("Latency (ms) [Mean + $p_{95}$]")
    ax.grid(True)
    ax.legend(loc="upper left", framealpha=0.85)

    # Panel (b): Scenario B
    ax = axes[1]
    df_b = safe_load_csv("scenario_b_bandwidth_sweep.csv")
    if df_b is not None and not df_b.empty:
        rates = sorted(df_b["rate_kbps"].unique())
        for mode, label, color, marker, ls in [
            ("classical", "Classical", C_CLASSICAL, "o", ":"),
            ("naive_pqc", "Naive PQC", C_NAIVE, "s", "--"),
            ("orbit_pqc", "ORBIT-PQC", C_ORBIT, "D", "-"),
        ]:
            sub = df_b[df_b["mode"] == mode]
            if sub.empty:
                continue
            etas = []
            for r in rates:
                lats = sub[sub["rate_kbps"] == r]["handshake_latency_ms"].values
                if len(lats) > 0:
                    p95 = np.percentile(lats, 95)
                    eta = max(0.0, (30000.0 - p95) / 30000.0 * 100.0)
                    etas.append(eta)
                else:
                    etas.append(0.0)

            ax.plot(rates, etas, label=label, color=color, marker=marker, linestyle=ls)

    ax.set_title(r"(b) Pass Efficiency $\eta_{\mathrm{win}}$ ($\tau_{\mathrm{los}}=30$s, $p_{95}$)")
    ax.set_xlabel("Channel Bitrate (kbps)")
    ax.set_ylabel(r"Preserved Window $\eta_{\mathrm{win}}$ (%)")
    ax.set_xscale("log", base=2)
    ax.set_xticks([32, 64, 128, 256, 512, 1024])
    ax.get_xaxis().set_major_formatter(ScalarFormatter())
    ax.grid(True)
    ax.legend(loc="lower right", framealpha=0.85)

    # Panel (c): Scenario D
    ax = axes[2]
    df_d = safe_load_csv("scenario_d_ablation.csv")
    if df_d is not None and not df_d.empty:
        rates_d = sorted(df_d["rate_kbps"].unique())
        for mode, label, color, marker in [
            ("orbit_pqc", "Fast-Path (48B)", C_ORBIT, "D"),
            ("orbit_pqc_ratchet", "Async Ratchet", C_RATCHET, "^"),
        ]:
            sub = df_d[df_d["mode"] == mode]
            if sub.empty:
                continue
            means = [sub[sub["rate_kbps"] == r]["handshake_latency_ms"].mean() for r in rates_d]
            ax.plot(rates_d, means, label=label, color=color, marker=marker)

    ax.set_title("(c) Ticket vs. Ratchet Re-Key")
    ax.set_xlabel("Channel Bitrate (kbps)")
    ax.set_ylabel("Latency (ms) [Log Scale]")
    ax.set_xscale("log", base=2)
    ax.set_yscale("log")
    ax.set_xticks([64, 128, 256, 512, 1024])
    ax.get_xaxis().set_major_formatter(ScalarFormatter())
    ax.grid(True, which="both")
    ax.legend(loc="upper right", framealpha=0.85)

    out_pdf = os.path.join(FIGURE_DIR, "fig_combined_empirical_ieee.pdf")
    plt.savefig(out_pdf, dpi=300)
    plt.close()
    print(f"[+] Saved IEEE vector plot to: {out_pdf}")

if __name__ == "__main__":
    main()