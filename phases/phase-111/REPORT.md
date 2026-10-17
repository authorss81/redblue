# Phase 111 — Stop dispatch from marking a phase done while its reviewer still has open MAJOR findings

## Status: BLOCKED. No implementation change was made.

This phase declares `must_touch: ["rbops/dispatch.sh"]`. That file is not in this
repository, and I may only read and write inside this checkout. Full evidence, the
exact commands and their exact output, are in `FINDINGS.md`.

Summary of what I verified:

- `ls rbops/` → `No such file or directory`; `git ls-files | grep -c '^rbops/'` → `0`
- `find . -name dispatch.sh` / `-name smoke.sh` / `-name verify.sh` → no output
- `logs/` (which holds the cited `phase-080.review.3.log` evidence) → does not exist
- `phases/` contains phase-002 … phase-093 only — no `phase-110`, no `phase-111`
- `grep -rn "REVIEW VERDICT"` → no match anywhere in the tree

Two independent reasons the mandated re-verification could not be performed:
the change target is absent, and the evidence the phase asks me to re-verify
(`logs/phase-080.review.3.log`) is absent. The prompt's own instruction for that
case is "a stale phase must never be 'fixed' by inventing a change", so I
stopped rather than inventing one.

## What changed

| File | Lines | What |
|---|---|---|
| `phases/phase-111/FINDINGS.md` | +104 | This block record: commands, output, and what is required |
| `phases/phase-111/REPORT.md` | new | This file |

No file under `src/`, `tests/`, `modules/`, `examples/`, `bootstrap/` or
`.github/` was touched. No file under `rbops/` was touched — it does not exist here.

**`must_touch` is NOT satisfied.** This phase fails that check, correctly: I could
not make the required change. It must be applied by hand in the RBOPS repository.

## Tests added

| Test | Edge class covered |
|---|---|
| none | — |

No test was added, because the two locations this phase's definition of done
requires tests (`rbops/smoke.sh`, and the dispatch code they would exercise) do not
exist in this checkout. Writing a Rust test in `tests/` would test something other
than the thing the phase is about — a `#[test]` that reimplements dispatch's
severity regex in Rust would pass while dispatch went on shipping `[MAJOR]` phases.
That is a test that cannot fail about the real defect, which AGENTS.md §3.1
forbids. The honest options were a real test or no test, and only one of those
exists here.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | not run — no source change to format; the toolchain is present (`cargo 1.99.1`) |
| `cargo clippy --all-targets -- -D warnings` | not run — no Rust change |
| `cargo test --all-targets` | not run — no Rust change |
| `./rbops/verify.sh phase-111` | **cannot run — `rbops/verify.sh` does not exist in this checkout** |

None of these is claimed as passing. `cargo` is installed and working (verified:
`cargo 1.99.1`, `rustc 1.99.1`); I did not run the Rust gates because I changed no
Rust, and AGENTS.md hard rule 4 forbids spending the budget on long local builds
for nothing. The fourth gate — the one that decides this phase — is unobtainable
from here, which is the core of the block.

## Test-requirement matrix

The matrix in AGENTS.md §3.2 describes runtime behaviour of the interpreter
(empty lists, out-of-bounds indices, `0/0`, unicode strings, recursion limits).
This phase changes no interpreter behaviour and ships no runtime path, so all
eleven rows are N/A — **not** because they were skipped, but because there is no
code here to exercise them. The applicable verification for this phase is the two
`rbops/smoke.sh` cases named in its definition of done, which is exactly what
cannot be written in this repository.

| Row | Status |
|---|---|
| empty | N/A — no interpreter code changed |
| singleton | N/A — no interpreter code changed |
| boundary | N/A — no interpreter code changed |
| out_of_bounds | N/A — no interpreter code changed |
| type_mismatch | N/A — no interpreter code changed |
| numeric_boundary | N/A — no interpreter code changed |
| unicode | N/A — no interpreter code changed |
| nesting_recursion | N/A — no interpreter code changed |
| duplicate_missing_keys | N/A — no interpreter code changed |
| malformed_input | N/A — no interpreter code changed |
| resource_limit | N/A — no interpreter code changed |

Quota rows are likewise unmet and I am not going to dress that up: 0 new
`#[test]` functions, 0 new Redblue `test` blocks, 0 `edge_*` tests, 0 tests
asserting a failure. Zero new `#[ignore]`, `// skip` or `allow(clippy::`
suppressions, and zero pre-existing tests broken — because I changed nothing at
all. Every count here is zero honestly rather than three by writing tests of the
wrong subject.

## Invariants touched

None. No language surface, `Value` variant, `Error` variant, grammar rule or
existing test was touched. `examples/*.rb` and `modules/*.rb` are untouched.

## Known gaps / follow-ups

- **This phase is unapplied.** The `[MAJOR]`-ships-as-`.done` defect in dispatch
  is still live. It must be applied by hand in the RBOPS repository.
- **`manual: true` was not honoured.** This phase's own Evidence section says an
  agent cannot land it, and phase-092/093 were both applied by hand with the same
  note — yet phase-111 was dispatched to an agent anyway. **That eligibility bug
  is in the RBOPS repo's dispatcher, and it will keep producing this phase
  forever.** It is the defect worth fixing first, because unlike phase-111's
  subject it is reproducible every time.
- **The parser defect phase-080 shipped may be fixable from here.** If the
  `[MAJOR]` at `src/parser.rs:1813` is a real Redblue bug, that half needs no
  special permission — it is an ordinary `src/` phase. `phases/phase-110/` does
  not exist in this checkout; confirm it was dispatched and landed rather than
  assuming the defect is covered.
- I have **not** read `rbops/dispatch.sh:912` or `:923-925`. Those line numbers
  come from the phase prompt, not from a line I verified in this run. Whoever
  applies the fix must re-verify them against current `main` before editing.
