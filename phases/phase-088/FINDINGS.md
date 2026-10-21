# phase-088 — findings

## 1. `run_source` throws the program's value away, and that is a public API shape

`src/lib.rs:94` — `pub fn run_source(source: &str) -> Result<(), Error>` — reaches
`src/lib.rs:110`, where `run_source_in` ends in `result.map(|_| ())`. A caller
therefore learns whether the program ran and nothing about what it answered.

This is the root cause of the phase's finding: every `assert!(run_source(src).is_ok())`
in the suite is blind to a wrong answer, because the only thing the function can
report is success. `src/lib.rs:120` already offers the other half —
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

One consequence of the shape above is worth naming, because it is the trap this
phase's own tests walked into first: `run_source` has **no channel for the lines a
test needs to read**, so a helper that wants both "did it run" and "what did it
say" has to run the program twice. `tests/redblue_test.rs` did exactly that for a
round, and the assertion it made was about the *second* run's output while the
fixture had been executed twice. The helper now runs the program once, through
`run_isolated`, and reads the lines from the `Vm` that run hands back.

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

## 3. The audit's "twenty" is seventeen under the published scanner

The phase evidence says "Twenty tests across `tests/` have no reachable `assert!` or
`assert_eq!` at all". Re-measured before changing anything — and the scan is now
**published**, so the next audit can re-run it rather than take a number on trust —
the count is **17 tests in 5 files**. All seventeen were given an assertion rather
than deleted, and two more were strengthened anyway, so nineteen tests were given
an assertion in all. The table is in `REPORT.md`.

The scanner is `phases/phase-088/scan.py`, and the file list it read is
`phases/phase-088/scanned-files.txt` — all **68** `tests/*.rs` files. Run it
against the base commit's tree and it names seventeen; run it against this tree's
`tests/` and it names none:

```
$ git archive c6e9837 tests | tar -x -C /tmp/base
$ python3 phases/phase-088/scan.py /tmp/base/tests   # 17
$ python3 phases/phase-088/scan.py tests              # 0
```

What the scan counts, exactly:

- A test is a **candidate** when it runs a Redblue program — its own body, or the
  body of a same-file helper it calls, mentions `.run(`, `run_source`,
  `run_isolated`, `run_chunk`, `tree_walk` or `bytecode`.
- A candidate is **vacuous** when neither its own body nor the body of any
  same-file helper it calls asserts anything about what the program said,
  recorded or answered. `assert_eq!`, `assert_ne!` and `assert_matches!` count, as
  does a match on the program's own answer (`Ok(Value::…)`), and so does an
  `assert!` whose condition says more than `is_ok()`.
- A `panic!` does **not** count. A test that panics only on `Err` proves the
  program ran and proves nothing about what it said — which is the defect.
- A Redblue-level `expect ... to be` **does** count: it lives inside a fixture,
  the program checks its own answer, and the Rust test around it fails when the
  check fails. A failed `expect` is a `Runtime` error the run hands back
  (`src/interpreter.rs:2331`), so the run's own success is the assertion — the
  suite pins it at `tests/expect_test.rs:283` ("expect inside a taken if-branch
  must fail"). This phase's own round-1 report had this backwards, and the two
  tests it strengthened on the strength of that mistake are fixed in
  `REPORT.md`.

The helper step is the one that matters: six of the hits an earlier scan
flagged are not defects — five of them assert through a helper (three through
`ok()` / `fails()`, whose `Result` is `.unwrap()`ed, and two through
`both_print`, which compares both engines' output to a literal) and the sixth
is a nested helper rather than a test — and a scan that cannot see through a
helper reads them as vacuous.

So the likely cause of "twenty" is a scan that did not resolve assertion-bearing
helpers, and one that did not count a fixture's own `expect` as an assertion. The
auditor may want to fold helper resolution into their scanner before the next
count — this one is published, so it can be compared against directly. Not a
defect in the suite; a defect in the measurement, and worth knowing about so the
next phase is not sized from a wrong number.

## 4. `rbops/verify.sh` is outside the checkout, but it is reachable

`ls rbops` → `No such file or directory`; `git ls-files | grep '^rbops/'` matches
nothing. The pipeline lives outside the project checkout — but the gate itself is
one directory up, and it runs:

```
$ RBOPS_BASE_REF=c6e9837 ../rbops/verify.sh phase-088
...
╔══════════════════════════════════════╗
║ VERIFY PASS — phase-088                ║
╚══════════════════════════════════════╝
```

`RBOPS_BASE_REF` must be the phase's base commit — the pipeline records it in
`phases/phase-NNN/.base`, **in the pipeline's own checkout, one directory up**
(`../phases/phase-NNN/.base` relative to the project root; the project's own
`phases/` directory holds the phase's documents, not the pipeline's bookkeeping) —
because the gate's diff-based checks (test quota, `edge_*` count, the
failure-assertion rule) read `git diff <base>`. Without it the base is `HEAD`, the
diff is empty, and the gate reports "no changed files detected" for a phase whose
work is in the working tree.

Same observation `phases/phase-087/REPORT.md` records, and it is worth the next
phase knowing: the gate is not "not run", it is one `../` away.

## 5. The delivery acknowledgement is a per-phase obligation, and it is due
##    *after* the run that wrote the work

`tests/unreviewed_work_ack_test.rs` audits every commit in `HEAD`'s history whose
subject marks unreviewed work (`resume phase-…` or `… checkpoint`) and requires
each one to be acknowledged, either by an entry in `ACKNOWLEDGED` or by the
delivery rule. The pipeline writes the commit that carries a phase's work
**after** the agent's run, so the agent that wrote the work cannot pin its SHA
— and pinning it anyway is what the file's own
`edge_a_rebuilt_delivery_commit_is_neither_unreviewed_nor_stale` exists to
forbid, because the pipeline rebuilds that commit on the same parent every round.

So the obligation belongs to the **next** round: the run that finds the delivery
commit in `HEAD`'s history re-points
`const DELIVERY_SUBJECT_PREFIX` (tests/unreviewed_work_ack_test.rs:242) from the
previous owner to the phase that owns the file now, and acknowledges the commit
the rule used to cover **by SHA** — it is an ancestor of `HEAD` and stays one, so
that entry cannot go stale.

This phase did exactly that: the prefix moved from `resume phase-084:` to
`resume phase-088:`, and phase-084's delivery commit `1d73e8c` was added to
`ACKNOWLEDGED` with its numstat (`+7103/-55`, 7 files, none under `src/`).

The audit failed on the pipeline's commit (`f0d4a46`, subject
`resume phase-088: adopt preserved work`) before this was done, exactly as the
previous attempt's report predicted. Every phase that the pipeline adopts this
way will hit it, and the fix is one constant and one list entry.

## 6. `phases/phase-087/REPORT.md:137` records a gate count the tree does not have

Phase-087's report says `cargo test --all-targets` answers **1766 passed**. The
base commit of this phase, `c6e9837`, holds **1967** `#[test]` functions
(`git grep -hE '^\s*#\[test\]' c6e9837 -- '*.rs' | wc -l`) and measures **1967
passed, 0 failed, 0 ignored** with the same command, in a worktree of that
commit. The 201-test gap is in phase-087's row.

It matters because a report that carries a stale count is a trap for the next
phase: this phase's own round-1 report read phase-087's 1766 as the base and
wrote `1766 + 8 = 1774`, which was an arithmetic derivation from a wrong base
rather than a measurement. The number this report carries — 1976 — is the one
the suite actually answers in this tree, and it is the number the tree holds.

A follow-up phase that reads a gate count out of an earlier report should
re-measure it rather than add to it.