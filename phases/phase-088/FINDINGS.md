# phase-088 — findings

## 1. `run_source` throws the program's value away, and that is a public API shape

`src/lib.rs:92` — `pub fn run_source(source: &str) -> Result<(), Error>` — reaches
`src/lib.rs:108`, where `run_source_in` ends in `result.map(|_| ())`. A caller
therefore learns whether the program ran and nothing about what it answered.

This is the root cause of the phase's finding: every `assert!(run_source(src).is_ok())`
in the suite is blind to a wrong answer, because the only thing the function can
report is success. `src/lib.rs:118` already offers the other half —
`run_source_value`, documented as returning "the value of its last statement" — and
this phase's tests route around `run_source` with `run_isolated` +
`Vm::take_output`.

Changing `run_source`'s return type is a breaking change to an exported function
(`redblue::run_source` is reachable from outside the crate), so it is not this
phase's to make — `must_touch` for phase-088 is `tests/`. It belongs in a phase that
may change `src/lib.rs`, and it should decide between:

- `run_source` returning `Result<Value, Error>` (a breaking change, and it makes
  every existing `assert!(run_source(..).is_ok())` caller still compile), or
- adding `run_source_output(source) -> Result<Vec<String>, Error>` beside it, so a
  caller that wants the lines does not have to rebuild the pipeline.

Recommendation: the second. It is additive, it is what both engines already offer
through `take_output`, and it is the shape `tests/common/vm.rs::tree_walk` has been
using in-tree since phase-020.

## 2. `Vm::run` prints what the program said; `BytecodeVm::run` can be told not to

`src/bytecode/vm.rs` has `set_echo(bool)` (used by `tests/common/vm.rs:361` so a
test run is not interleaved with program output). The tree-walking `Vm` has no
equivalent, so every Rust test that runs a program through it writes the program's
lines into the test runner's captured stdout.

Harmless today — the lines are captured and only shown on failure — and this phase
does not work around it, because `take_output` is what the assertions read either
way. But it means a `cargo test -- --nocapture` run of `loop_bounds_test.rs` is
full of lines from the fixtures. If `set_echo` is ever added to `Vm`, it should
default to `true` exactly as `BytecodeVm`'s does, so `rb run` is unaffected.

## 3. The audit's "twenty" is ten on this checkout

The phase evidence says "Twenty tests across `tests/` have no reachable `assert!` or
`assert_eq!` at all". Re-measured before changing anything, the number is **10**,
and all ten were fixed rather than deleted (the table is in `REPORT.md`).

The scan that produced it is in `target/tmp/scan5.py` (scratch, not committed).
It parses every `tests/*.rs` with comments and string literals stripped, brace-
matches each `#[test] fn`, and keeps a test whose body runs a program
(`.run(`, `run_source`, `run_isolated`, `run_chunk`, `tree_walk`, `bytecode`) and
whose body mentions none of: `take_output`, `.output`, `stdout`,
`take_expectation_failure`, `expect_err`, `assert*`, or a Redblue-level `expect`.

Five further candidates were checked by hand and are **not** defects:
`tests/assertions_test.rs:207`, `:237` and `:332` assert through `ok()`/`fails()`
whose `Result` is `.unwrap()`ed; `tests/might_fail_test.rs:329` asserts through
`both_print` (`tests/might_fail_test.rs:157`); and
`tests/might_fail_test.rs:1079` `strip_markers` is a nested helper, not a test.

So the likely cause of "twenty" is a scan that did not resolve assertion-bearing
helpers — every one of those five asserts through a helper, and a naive
"no `assert` token in the body" filter reads them as vacuous. The auditor may want
to fold helper resolution into its scanner before the next count. Not a defect in
the suite; a defect in the measurement, and worth knowing about so the next phase
is not sized from a wrong number.

## 4. `rbops/verify.sh` is not in the checkout

`ls rbops` → `No such file or directory`; `git ls-files | grep '^rbops/'` matches
nothing. The pipeline lives outside the project checkout, so gate 4 could not be
run from here and its result is **not** claimed in `REPORT.md`. Same observation
`phases/phase-087/REPORT.md` records. If the intent is that agents run the fourth
gate themselves, `verify.sh` needs to be reachable from the working directory —
otherwise every phase is reporting "not run" and the gate is never exercised
before the reviewer sees the diff.