# ORBIT-PQC Reproducibility Protocol

## Baseline

The Phase 1 baseline is the repository state at commit `294973d9987c48835fc5c42e398460d8beaf9579` (`Fix: Paper Formatting Issue`). Work for later phases should branch from this state rather than modifying the baseline in place.

## Software inputs

- Rust workspace is pinned by `Cargo.lock`.
- Python dependencies are declared in `python/requirements.txt`.
- CI uses Python 3.11 and the stable Rust toolchain.
- IEEE paper compilation uses TeX Live with `texlive-publishers` for `IEEEtran.cls`.
- The cryptographic layer currently uses `pqcrypto-kyber` 0.8.1 for the Kyber-768 implementation and `pqcrypto-dilithium` 0.5.0 for Dilithium3. These are retained as explicit dependencies until the Phase 2 interoperability and known-answer-vector checks are complete.

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

## Cryptographic correctness gate

Phase 2 begins with the existing cryptographic bindings rather than replacing them prematurely. The current test suite now checks ML-KEM-768 round-trip agreement, ciphertext tampering behavior, wrong-key behavior, ML-DSA-65 message/signature/public-key binding, and standardized parameter sizes.

These tests establish API-level correctness properties but are not, by themselves, proof of FIPS 203/204 conformance. Before claiming standards conformance, Phase 2 must add interoperability or known-answer-vector validation against the standardized algorithms and record the exact implementation provenance. No experimental results should be regenerated from cryptographic changes until that gate is satisfied.

## Baseline limitations

The baseline metadata predates this protocol and some historical sidecars contain `git_commit: "unknown"`. The baseline also represents the current implementation and is not a declaration that the cryptographic construction, orbital model, or statistical methodology has already been fully validated. Those are targets for subsequent research phases.
