# phase-037 — FINDINGS

Work this phase produced, and what it found next.

## 1. `rbops/` is not present in the checkout

`ls rbops` → `No such file or directory`. `rbops/verify.sh`, `rbops/phases.json`
and `rbops/dispatch.sh` are all absent, so `./rbops/verify.sh phase-037` — the
fourth gate in AGENTS.md section 3.4 — could not be run.

I did not create it (hard rule 1: do not touch `rbops/`). The three cargo gates
were run and are recorded in REPORT.md with what they actually printed. The
fourth gate is **unverified**; a reviewer must run it before this phase is
`.done`.

## 2. A seeded generator is not an unpredictable one

Phase 037 replaced the wall clock in the `random*` builtins with a seeded
SplitMix64, so draws are reproducible — which is what the bootstrap fixed point
needs. But a *seeded* generator is deterministic by construction, so a Redblue
program now has no source of unpredictable data at all.

This is a real capability gap, not a defect: the previous behaviour was
unpredictable and also unreproducible, and this phase was asked for the second
without asking about the first. `src/runtime.rs` has no path to a
cryptographically secure source, and adding one is a language-surface decision
(a new builtin, plus a documented guarantee about what it does and does not
promise). Out of scope here; worth its own phase.

## 3. `random` and `random_number` disagree at the high end

`random(min, max)` is inclusive at both ends. `random_number(min, max)` is
inclusive at the low end and exclusive at the high one. Both readings are
individually defensible — the closed one is what makes `random(5, 5)` answer
`5`, which the phase's Definition of Done requires, and the half-open one is
what `random_number` has always meant — but a reader meeting both spellings will
reasonably expect them to agree.

Not changed here: the phase pinned `random(5, 5) == 5` and preserving
`random_number`'s existing range are both explicit requirements, so reconciling
them is a language decision rather than a bug fix. `tests/numeric_edge_test.rs`
depends on the half-open reading (`random_number(1, 2)` in `1.0..2.0`), so
changing it would touch an existing test.

## 4. The bytecode compiler still refuses function literals

Found while writing `both_engines_agree_on_a_seeded_sequence`: `redblue::compile_source`
rejects `to (x) ... end` with *"`to (x) ... end` as an expression is not compiled
yet; run programs that use one with `rb run`"*.

Not this phase's work, and the test works around it by not using one. It does
mean no seeded-draw test can compare the two engines through `map`, and
`examples/random.rb` cannot either.
