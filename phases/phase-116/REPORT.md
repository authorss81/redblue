# Phase 116 — Fail an attempt fast when it has written no implementation

## Status: BLOCKED — not implemented, honestly reported

The phase asks for a change that lands entirely in `rbops/` dispatch logic. This
checkout does not contain `rbops/`, the phase's own Constraints forbid touching
`rbops/`, and its `must_touch: ["rbops/"]` requires a changed file there. Those
three facts cannot be satisfied at once, and `./rbops/verify.sh` — the gate that
would adjudicate them — is absent. Full evidence and the design I recommend for
whoever can edit the dispatcher are in `FINDINGS.md`.

**No implementation was written, because any I could write here would be
unreachable code that no gate and no dispatcher ever executes.**

## What changed

| File | Lines | What |
|---|---|---|
| `phases/phase-116/FINDINGS.md` | new | blockers B1–B4 with commands and outputs, plus the recommended watchdog design |
| `phases/phase-116/REPORT.md` | new | this file |

No source, test, workflow or pipeline file was touched. `git status` shows only
these two additions.

## Tests added

| Test | Edge class covered |
|---|---|
| none | — see "Test requirements" below. The change under test does not exist in this checkout, so there is nothing to assert against. |

The quota (≥3 `#[test]` or ≥2 Redblue `test` blocks, ≥1 `edge_*`, ≥1 asserting a
failure) is **not met, and cannot be met honestly here**. The unit tests the
phase requires — "reads only, gets killed" and "writes first, survives" — are
tests of `rbops` dispatch behaviour. Satisfying the count with Rust tests under
`tests/` would require inventing a Rust module that does not exist and asserting
against it: a test that cannot fail, which §3.1 and §5 of the contract both
treat as a BLOCKER finding. I would rather hand back a red phase than a green
one built on a fake.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no diff |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0 |
| `cargo test --all-targets` | **pass** — 299 passed, 0 failed, 0 ignored (unchanged baseline; this phase changed no Rust) |
| `./rbops/verify.sh phase-116` | **NOT RUN — the script does not exist here.** `./rbops/verify.sh phase-116` → `bash: ./rbops/verify.sh: No such file or directory`, exit 127 |

The last gate is the project's only authority on whether work is real, and it
could not be executed. That alone disqualifies this phase from `.done`; the
self-contradictory `must_touch` disqualifies it a second time.

## Invariants touched

- None. No `.rb` grammar, `Value` variant, `Error` variant, `set x to …`, `say`,
  `to … end` behaviour, or `tests/` file was read for modification or modified.

## Test requirements — row by row

Not applicable to every row: there is no behaviour change in this checkout. I am
recording that rather than claiming coverage I do not have.

| Row | Status | Why |
|---|---|---|
| empty | N/A | No behaviour implemented. The watchdog's "no work" state is its whole subject, and it lives in `rbops/`. |
| singleton | N/A | Same. |
| boundary | N/A | Same — this row is the fraction-of-budget boundary and it needs the dispatcher's budget constant, which is not in this checkout. |
| out_of_bounds | N/A | Same. |
| type_mismatch | N/A | Shell/dispatch logic has no type system; the row belongs to the interpreter. |
| numeric_boundary | N/A | Same. |
| unicode | N/A | Same. |
| nesting_recursion | N/A | Same. |
| duplicate_missing_keys | N/A | Same. |
| malformed_input | N/A | Partially relevant (a watchdog run against an unreadable workspace) but untestable here — see `FINDINGS.md` item 3. |
| resource_limit | N/A | This row *is* the phase, and it is the one row that cannot be satisfied from this checkout. |

Additional checkboxes: **0** new `#[ignore]` / `// skip` / `allow(clippy::`
suppressions. **0** newly-failing pre-existing tests — the 299-test suite is green
and unmodified.

## Known gaps / follow-ups

- The phase is unstarted, not partially done. Watchdog design, marker semantics
  and the two required fixture tests are specified in `FINDINGS.md` so the work
  can be executed the moment someone with write access to `rbops/` picks it up.
- The manifest bug is recorded for the auditor: `must_touch: ["rbops/"]` plus a
  Constraint forbidding `rbops/` means this class of phase can never pass as
  written. See the "Manifest recommendation" section of `FINDINGS.md`.
- phase-053's 0-byte artifacts could not be verified from this checkout. I am
  not claiming the finding is stale; only that I could not re-verify it, which
  the phase instructed me to do before changing anything.