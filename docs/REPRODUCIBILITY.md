# ORBIT-PQC Reproducibility Protocol

## Baseline

The Phase 1 baseline is the repository state at commit `294973d9987c48835fc5c42e398460d8beaf9579` (`Fix: Paper Formatting Issue`). Work for later phases should branch from this state rather than modifying the baseline in place.

## Software inputs

- Rust workspace is pinned by `Cargo.lock`.
- Python dependencies are declared in `python/requirements.txt`.
- CI uses Python 3.11 and the stable Rust toolchain.
- IEEE paper compilation uses TeX Live with `texlive-publishers` for `IEEEtran.cls`.

## Experiment provenance

Each experiment should produce a sidecar metadata file in `datasets/` containing:

- unique run ID;
- UTC timestamp;
- Git commit, short commit, and dirty-worktree state;
- experiment parameters;
- Python, Rust/Cargo, platform, and relevant Python package versions;
- random seed(s), when applicable, or an explicit `not_applicable` marker;
- input trace identifiers and SHA-256 hashes when traces are used;
- output dataset identifiers and SHA-256 hashes after execution.

The initial runner metadata is created before a run. After the run, provenance should be finalized with `python/experiments/provenance.py <run_id> [--seed N]`. Finalization is separate from measurement generation so hashes describe completed artifacts.

Historical datasets whose metadata records `git_commit: "unknown"` are retained as historical results and must not be relabeled retroactively.

## Reproduction order

1. Install the pinned Rust and Python dependencies.
2. Build the release protocol binary with `cargo build --release`.
3. Generate SGP4 traces with `python/experiments/sgp4_emulator.py`.
4. Execute an explicitly selected scenario with `python/experiments/runner.py` on a Linux host with the required network-namespace privileges.
5. Finalize the run provenance sidecar with `python/experiments/provenance.py <run_id>`; provide `--seed N` only when stochastic sampling is used.
6. Run `python/experiments/stats.py` against the generated datasets.
7. Regenerate LaTeX metrics/tables with `python/experiments/parser.py`.
8. Regenerate the IEEE figure with `python/plots/generate_ieee_plots.py`.
9. Compile `paper/brief.tex` twice with `pdflatex`.

## Measurement policy

- Never edit generated CSV measurements manually.
- Never replace an existing result merely to make a statistic or figure look conventional.
- Keep raw trial-level observations separate from derived statistics.
- Record failures/timeouts as experimental outcomes.
- Distinguish measured results, modeled results, and derived metrics in the paper.
- When a result changes, regenerate downstream artifacts from the raw dataset rather than editing LaTeX numbers by hand.
- Do not modify historical provenance metadata solely to make it appear reproducible.

## V1 cryptographic correctness gate

The V1 crypto layer currently wraps `pqcrypto-kyber` 0.8.1 and `pqcrypto-dilithium` 0.5.0. The implementation exposes ML-KEM-768 and ML-DSA-65 parameter sizes and now includes API-level tests for valid operation, tampered inputs, wrong-key behavior, message binding, public-key binding, and parameter sizes.

These tests are correctness checks for the wrapper API; they are not sufficient to establish FIPS 203/204 conformance or interoperability. Known-answer-vector (KAT) and interoperability validation must be completed before making conformance claims or regenerating experimental results based on those claims.

## Baseline limitations

The baseline metadata predates this protocol and some historical sidecars contain `git_commit: "unknown"`. The baseline also represents the current implementation and is not a declaration that the cryptographic construction, orbital model, or statistical methodology has already been fully validated. Those are targets for subsequent research phases.
