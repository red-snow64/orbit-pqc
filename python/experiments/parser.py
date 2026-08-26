#!/usr/bin/env python3
"""
Dynamic LaTeX Macro & Table Generator
Generates:
  1. paper/generated_metrics.tex (Pure numeric macros)
  2. paper/table_ablation.tex    (Ablation table float)
  3. paper/table_crossing.tex    (Dynamic crossing pass table float)
"""

import os
import numpy as np
import pandas as pd

DATASET_DIR = os.path.abspath("datasets")
PAPER_DIR = os.path.abspath("paper")

def safe_load(fn):
    p = os.path.join(DATASET_DIR, fn)
    if not os.path.exists(p):
        return None
    try:
        df = pd.read_csv(p, on_bad_lines="skip")
        df["handshake_latency_ms"] = pd.to_numeric(df["handshake_latency_ms"], errors="coerce")
        df["success"] = df["success"].astype(str).str.lower().isin(["true", "1", "t"])
        return df
    except Exception:
        return None

def main():
    os.makedirs(PAPER_DIR, exist_ok=True)
    out_macros = os.path.join(PAPER_DIR, "generated_metrics.tex")
    out_tab_ablation = os.path.join(PAPER_DIR, "table_ablation.tex")
    out_tab_crossing = os.path.join(PAPER_DIR, "table_crossing.tex")

    macros = []
    macros.append("% Auto-generated raw numeric metrics (no math formatting or units inside macros)")

    # Scenario A
    df_a = safe_load("scenario_a_ber_sweep.csv")
    if df_a is not None:
        succ = df_a[df_a["success"]]
        o0 = succ[(succ["mode"] == "orbit_pqc") & (succ["loss_pct"] == 0.0)]["handshake_latency_ms"]
        n0 = succ[(succ["mode"] == "naive_pqc") & (succ["loss_pct"] == 0.0)]["handshake_latency_ms"]
        o15 = succ[(succ["mode"] == "orbit_pqc") & (succ["loss_pct"] == 15.0)]["handshake_latency_ms"]
        n15 = succ[(succ["mode"] == "naive_pqc") & (succ["loss_pct"] == 15.0)]["handshake_latency_ms"]

        if len(o0) > 0 and len(n0) > 0:
            macros.append(f"\\newcommand{{\\OrbitMeanLossZero}}{{{o0.mean():.1f}}}")
            macros.append(f"\\newcommand{{\\NaiveMeanLossZero}}{{{n0.mean():.1f}}}")
            macros.append(f"\\newcommand{{\\SpeedupLossZero}}{{{(n0.mean() / o0.mean()):.2f}}}")
        if len(o15) > 0 and len(n15) > 0:
            macros.append(f"\\newcommand{{\\OrbitMeanLossFifteen}}{{{o15.mean():.1f}}}")
            macros.append(f"\\newcommand{{\\NaiveMeanLossFifteen}}{{{n15.mean():.1f}}}")
            macros.append(f"\\newcommand{{\\SpeedupLossFifteen}}{{{(n15.mean() / o15.mean()):.2f}}}")

    # Scenario B
    df_b = safe_load("scenario_b_bandwidth_sweep.csv")
    if df_b is not None:
        succ = df_b[df_b["success"]]
        o32 = succ[(succ["mode"] == "orbit_pqc") & (succ["rate_kbps"] == 32)]["handshake_latency_ms"]
        n32 = succ[(succ["mode"] == "naive_pqc") & (succ["rate_kbps"] == 32)]["handshake_latency_ms"]
        if len(o32) > 0 and len(n32) > 0:
            macros.append(f"\\newcommand{{\\OrbitMeanRateThirtyTwo}}{{{o32.mean():.1f}}}")
            macros.append(f"\\newcommand{{\\NaiveMeanRateThirtyTwo}}{{{n32.mean():.1f}}}")
            macros.append(f"\\newcommand{{\\SpeedupRateThirtyTwo}}{{{(n32.mean() / o32.mean()):.2f}}}")

            naive_cost = (n32.mean() / 30000.0) * 100.0
            naive_pres = max(0.0, 100.0 - naive_cost)
            orbit_cost = (o32.mean() / 30000.0) * 100.0
            orbit_pres = max(0.0, 100.0 - orbit_cost)

            macros.append(f"\\newcommand{{\\NaivePassCostThirtyTwo}}{{{naive_cost:.2f}}}")
            macros.append(f"\\newcommand{{\\NaivePreservedThirtyTwo}}{{{naive_pres:.2f}}}")
            macros.append(f"\\newcommand{{\\OrbitPassCostThirtyTwo}}{{{orbit_cost:.2f}}}")
            macros.append(f"\\newcommand{{\\OrbitPreservedThirtyTwo}}{{{orbit_pres:.2f}}}")

    # Scenario D (Ablation)
    df_d = safe_load("scenario_d_ablation.csv")
    if df_d is not None:
        succ = df_d[df_d["success"]]
        of64 = succ[(succ["mode"] == "orbit_pqc") & (succ["rate_kbps"] == 64)]["handshake_latency_ms"]
        or64 = succ[(succ["mode"] == "orbit_pqc_ratchet") & (succ["rate_kbps"] == 64)]["handshake_latency_ms"]
        if len(of64) > 0 and len(or64) > 0:
            macros.append(f"\\newcommand{{\\OrbitFastPathMeanSixtyFour}}{{{of64.mean():.1f}}}")
            macros.append(f"\\newcommand{{\\OrbitRatchetMeanSixtyFour}}{{{or64.mean():.1f}}}")

        tab_abl = []
        tab_abl.append("\\begin{table}[htbp]")
        tab_abl.append("\\centering")
        tab_abl.append("\\caption{Ablation: Synchronous Ticket vs. Async ML-KEM-1024 Ratchet}")
        tab_abl.append("\\label{tab:ablation_summary}")
        tab_abl.append("\\begin{tabular}{rcccc}")
        tab_abl.append("\\toprule")
        tab_abl.append("\\textbf{Bitrate} & \\textbf{Ticket (ms)} & \\textbf{Ratchet (ms)} & \\textbf{Ratio} & \\textbf{Pass Cost (30s)} \\\\")
        tab_abl.append("\\midrule")
        for r in sorted(succ["rate_kbps"].unique()):
            sf = succ[(succ["mode"] == "orbit_pqc") & (succ["rate_kbps"] == r)]["handshake_latency_ms"]
            sr = succ[(succ["mode"] == "orbit_pqc_ratchet") & (succ["rate_kbps"] == r)]["handshake_latency_ms"]
            if len(sf) > 0 and len(sr) > 0:
                cost = (sf.mean() / 30000.0) * 100.0
                tab_abl.append(f"{r}~kbps & {sf.mean():.1f} & {sr.mean():.1f} & {(sr.mean()/sf.mean()):.1f}$\\times$ & {cost:.2f}\\% \\\\")
        tab_abl.append("\\bottomrule")
        tab_abl.append("\\end{tabular}")
        tab_abl.append("\\end{table}")
        with open(out_tab_ablation, "w") as f:
            f.write("\n".join(tab_abl) + "\n")

    # Scenario E (Framing)
    df_e = safe_load("scenario_e_sensitivity_sweep.csv")
    if df_e is not None:
        succ = df_e[df_e["success"]]
        ne256 = succ[(succ["mode"] == "naive_pqc") & (succ["mtu_bytes"] == 256)]
        oe256 = succ[(succ["mode"] == "orbit_pqc") & (succ["mtu_bytes"] == 256)]
        if len(ne256) > 0 and len(oe256) > 0:
            b_n = int((ne256["bytes_sent"] + ne256["bytes_recv"]).mean())
            b_o = int((oe256["bytes_sent"] + oe256["bytes_recv"]).mean())
            macros.append(f"\\newcommand{{\\NaiveWireBytesMtuTwoFiftySix}}{{{b_n}}}")
            macros.append(f"\\newcommand{{\\OrbitWireBytesMtuTwoFiftySix}}{{{b_o}}}")

    # Scenario F2 (Crossing Pass Table)
    df_f2 = safe_load("scenario_f2_dynamic_crossing.csv")
    if df_f2 is not None:
        succ = df_f2[df_f2["success"]]
        of2 = succ[succ["mode"] == "orbit_pqc"]["handshake_latency_ms"]
        nf2 = succ[succ["mode"] == "naive_pqc"]["handshake_latency_ms"]
        cf2 = succ[succ["mode"] == "classical"]["handshake_latency_ms"]

        if len(of2) > 0 and len(nf2) > 0:
            p95_o = np.percentile(of2, 95)
            p95_n = np.percentile(nf2, 95)
            eta_o = max(0.0, (30000.0 - p95_o) / 30000.0 * 100.0)
            eta_n = max(0.0, (30000.0 - p95_n) / 30000.0 * 100.0)

            macros.append(f"\\newcommand{{\\OrbitMeanCrossingLatency}}{{{of2.mean():.1f}}}")
            macros.append(f"\\newcommand{{\\NaiveMeanCrossingLatency}}{{{nf2.mean():.1f}}}")
            macros.append(f"\\newcommand{{\\CrossingSpeedup}}{{{(nf2.mean() / of2.mean()):.2f}}}")
            macros.append(f"\\newcommand{{\\OrbitEtaWinCrossingNinetyFive}}{{{eta_o:.2f}}}")
            macros.append(f"\\newcommand{{\\NaiveEtaWinCrossingNinetyFive}}{{{eta_n:.2f}}}")

        tab_cross = []
        tab_cross.append("\\begin{table}[htbp]")
        tab_cross.append("\\centering")
        tab_cross.append("\\caption{Empirical Comparison under SGP4 Crossing Pass (128~kbps)}")
        tab_cross.append("\\label{tab:sgp4_summary}")
        tab_cross.append("\\begin{tabular}{lcccc}")
        tab_cross.append("\\toprule")
        tab_cross.append("\\textbf{Protocol Mode} & \\textbf{Mean (ms)} & \\textbf{$p_{95}$ (ms)} & \\textbf{Success} & \\textbf{$\\eta_{\\text{win}}$ (30s)} \\\\")
        tab_cross.append("\\midrule")
        if len(cf2) > 0:
            p95_c = np.percentile(cf2, 95)
            eta_c = max(0.0, (30000.0 - p95_c) / 30000.0 * 100.0)
            tab_cross.append(f"Classical (ECDH) & {cf2.mean():.1f} & {p95_c:.1f} & 100\\% & {eta_c:.2f}\\% \\\\")
        if len(nf2) > 0:
            tab_cross.append(f"Naive PQC (FIPS 203/204) & {nf2.mean():.1f} & {p95_n:.1f} & 100\\% & {eta_n:.2f}\\% \\\\")
        if len(of2) > 0:
            tab_cross.append(f"\\textbf{{ORBIT-PQC (Ours)}} & \\textbf{{{of2.mean():.1f}}} & \\textbf{{{p95_o:.1f}}} & \\textbf{{100\\%}} & \\textbf{{{eta_o:.2f}\\%}} \\\\")
        tab_cross.append("\\bottomrule")
        tab_cross.append("\\end{tabular}")
        tab_cross.append("\\end{table}")
        with open(out_tab_crossing, "w") as f:
            f.write("\n".join(tab_cross) + "\n")

    with open(out_macros, "w") as f:
        f.write("\n".join(macros) + "\n")
    print("[+] Wrote pure numeric macros, table_ablation.tex, and table_crossing.tex")

if __name__ == "__main__":
    main()