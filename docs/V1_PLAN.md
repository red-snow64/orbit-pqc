# V1 Development Plan

`V1` is the active development branch for the next repository iteration. `main` remains the stable integration branch until V1 work is complete and approved.

## Working rules

- Develop and commit V1 work on `V1`.
- Keep `main` unchanged during V1 development.
- Prefer small, reviewable commits and focused changes.
- Keep documentation compact; update existing documentation when possible rather than creating redundant files.
- Do not regenerate experimental results until the relevant implementation and validation gates are satisfied.
- Merge `V1` into `main` only after the V1 scope has been reviewed and verified.

## Initial validation gates

1. Establish a reproducible baseline and repository state.
2. Verify cryptographic API correctness and parameter expectations.
3. Complete known-answer-vector/interoperability validation before making conformance claims.
4. Validate networking/node behavior without weakening cryptographic or reproducibility guarantees.
5. Re-run the project validation pipeline and review resulting artifacts before integration.

This plan is intentionally concise and will be updated only when the V1 process or scope materially changes.
