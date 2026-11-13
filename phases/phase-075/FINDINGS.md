# Phase 075 — Findings

## Pre-existing gate failures repaired to finish this phase

Both repairs below are outside phase 075's navigation scope. They are retained
because each was a pre-existing failure of a mandatory gate (`cargo test
--all-targets` and `cargo clippy --all-targets -- -D warnings`), and each is
recorded in the master ledger as an owner-phase finding so the owner phases see
the edits:

- `F-073-02` (owner: phase-073) — benchmark worker lookup.
- `F-088-01` (owner: phase-088) — scanner `needless_bool`.

Both ledger rows carry `[RESOLVED — verify in owner phase]`: Phase 075 applied
the minimal repair so the mandatory gates can run, but the owner phases retain
ownership and must verify/re-own the fixes rather than treating them as closed by
this phase.

### `cargo test --all-targets`: `rb-bench` unit tests cannot locate the worker

`tests/bench/mod.rs:408` looked for the runner only beside the current test
harness. When Cargo runs the `rb-bench` binary's own unit tests, that harness
lives under `target/<profile>/build/…`, so `edge_child_measurement_accepts_an_explicit_environment`
failed with `Load("cannot locate the rb-bench worker binary …")`. The lookup now
finds Cargo's target directory and resolves the runner in the current profile
(`target/<profile>/rb-bench`), also covering the cross-target layout
(`target/<triple>/<profile>/rb-bench`), so it cannot silently pick up a stale
binary elsewhere on the ancestor path. The child-process path is still required;
there is no in-process fallback.

Reproduction:

```bash
cargo test --bin rb-bench edge_child_measurement_accepts_an_explicit_environment
```

### `cargo clippy --all-targets -- -D warnings`: unrelated scanner diagnostic

`phases/phase-088/scan.rs:524` returned a bool literal after an `if`, which
clippy rejects as `needless_bool`. The function now returns
`!negates_an_is_ok_call(argument)`; behaviour is unchanged.

Reproduction:

```bash
cargo clippy --all-targets -- -D warnings
```
