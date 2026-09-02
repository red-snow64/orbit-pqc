#!/usr/bin/env python3
"""Reproducibility provenance utilities for ORBIT-PQC experiment artifacts.

The experiment runner currently creates the initial sidecar before execution.
This module finalizes that sidecar after execution so it records the exact
software/environment state and cryptographic hashes of generated artifacts.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import importlib.metadata
import json
import os
import platform
import subprocess
import sys
from pathlib import Path

DATASET_DIR = Path(__file__).resolve().parents[2] / "datasets"


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def command_output(command: list[str]) -> str | None:
    try:
        return subprocess.check_output(command, stderr=subprocess.DEVNULL, text=True).strip()
    except (OSError, subprocess.CalledProcessError):
        return None


def git_provenance(repo: Path) -> dict[str, object]:
    commit = command_output(["git", "rev-parse", "HEAD"])
    short_commit = command_output(["git", "rev-parse", "--short", "HEAD"])
    status = command_output(["git", "status", "--porcelain"])
    return {
        "commit": commit or "unknown",
        "short_commit": short_commit or "unknown",
        "dirty": bool(status),
    }


def package_versions() -> dict[str, str]:
    versions: dict[str, str] = {}
    for name in ("numpy", "pandas", "scipy", "matplotlib", "sgp4", "skyfield"):
        try:
            versions[name] = importlib.metadata.version(name)
        except importlib.metadata.PackageNotFoundError:
            versions[name] = "not-installed"
    return versions


def environment_provenance(repo: Path) -> dict[str, object]:
    return {
        "python": sys.version.split()[0],
        "platform": platform.platform(),
        "machine": platform.machine(),
        "rustc": command_output(["rustc", "--version"]) or "unavailable",
        "cargo": command_output(["cargo", "--version"]) or "unavailable",
        "packages": package_versions(),
        "environment_variables": {
            "LANG": os.environ.get("LANG"),
            "LC_ALL": os.environ.get("LC_ALL"),
        },
        "git": git_provenance(repo),
    }


def infer_artifacts(run_id: str) -> list[Path]:
    mapping = {
        "scenario_a": "scenario_a_ber_sweep.csv",
        "scenario_b": "scenario_b_bandwidth_sweep.csv",
        "scenario_c": "scenario_c_timeout_sweep.csv",
        "scenario_d": "scenario_d_ablation.csv",
        "scenario_e": "scenario_e_sensitivity_sweep.csv",
        "scenario_f1": "scenario_f1_dynamic_intra.csv",
        "scenario_f2": "scenario_f2_dynamic_crossing.csv",
        "custom_run": "custom_experiment.csv",
    }
    artifact = mapping.get(run_id)
    return [DATASET_DIR / artifact] if artifact else []


def infer_inputs(run_id: str) -> list[Path]:
    if run_id == "scenario_f1":
        return [DATASET_DIR / "sgp4_intra_plane_trace.csv"]
    if run_id == "scenario_f2":
        return [DATASET_DIR / "sgp4_crossing_pass_trace.csv"]
    return []


def finalize(run_id: str, seed: int | None = None) -> Path:
    metadata_path = DATASET_DIR / f"{run_id}_metadata.json"
    if not metadata_path.exists():
        raise FileNotFoundError(metadata_path)

    metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
    repo = Path(__file__).resolve().parents[2]
    artifacts = [p for p in infer_artifacts(run_id) if p.exists()]
    inputs = [p for p in infer_inputs(run_id) if p.exists()]

    metadata["provenance_finalized_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
    metadata["git"] = git_provenance(repo)
    metadata["environment"] = environment_provenance(repo)
    metadata["random_seeds"] = {"status": "not_applicable" if seed is None else "recorded", "seed": seed}
    metadata["input_artifacts"] = [
        {"path": str(p.relative_to(repo)), "sha256": sha256_file(p)} for p in inputs
    ]
    metadata["output_artifacts"] = [
        {"path": str(p.relative_to(repo)), "sha256": sha256_file(p)} for p in artifacts
    ]

    metadata_path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    return metadata_path


def main() -> None:
    parser = argparse.ArgumentParser(description="Finalize an ORBIT-PQC experiment provenance sidecar")
    parser.add_argument("run_id", help="Metadata run identifier, e.g. scenario_a")
    parser.add_argument("--seed", type=int, default=None, help="Random seed if the experiment uses stochastic sampling")
    args = parser.parse_args()
    print(f"[+] Finalized provenance: {finalize(args.run_id, args.seed)}")


if __name__ == "__main__":
    main()
