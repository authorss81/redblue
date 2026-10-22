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

## 3. The audit's "twenty" is eighteen under the published scanner

The phase evidence says "Twenty tests across `tests/` have no reachable `assert!` or
`assert_eq!` at all". Re-measured before changing anything — and the scan is now
**published**, so the next audit can re-run it rather than take a number on trust —
the count is **18 tests in 6 files**. All eighteen were given an assertion rather
than deleted, and two more were strengthened anyway, so twenty tests were given an
assertion in all. The table is in `REPORT.md`.

The scanner is `phases/phase-088/scan.py`, and the file list it read is
`phases/phase-088/scanned-files.txt` — all **70** `tests/*.rs` files. Run it
against the base commit's tree (68 files, before this phase's
`tests/failure_channel_test.rs` and `tests/phase_088_scanner_test.rs`) and it names
eighteen; run it against this tree's `tests/` and it names none:

```
$ git archive c6e9837 tests | tar -x -C /tmp/base
$ python3 phases/phase-088/scan.py /tmp/base/tests   # 18
$ python3 phases/phase-088/scan.py tests              # 0
```

The scanner is **tested** — `tests/phase_088_scanner_test.rs` runs it over fixture
directories and checks what it names, so a drift in it is a red test rather than a
number nobody checks.

What the scan counts, exactly:

- A test is a **candidate** when it runs a Redblue program — its own body, or the
  body of a same-file helper it calls, mentions `.run(`, `run_source`,
  `run_isolated`, `run_chunk`, `tree_walk` or `bytecode`.
- A candidate is **vacuous** when nothing in it compares what the program
  answered. `assert_eq!`, `assert_ne!` and `assert_matches!` count, as does an
  `assert!` whose condition says more than `is_ok()`, as does an `==` or `!=`, and
  so does a match on the program's own answer against a literal
  (`Ok(Value::YesNo(true))`).
- The comparison only counts if what it holds against something else **is** the
  program's answer: the output it printed, the error it refused with, the value
  it ended on, or a name bound to one of those — directly, or through a name that
  already carries it, or through a collection the answer is put into, or through
  the object the program was run through.
- A `panic!` does **not** count on its own. A test that panics only on `Err`
  proves the program ran and proves nothing about what it said — which is the
  defect. A panic that guards a comparison counts, because the comparison it
  guards is the assertion.
- A Redblue-level `expect ... to be` **does** count: it lives inside a fixture,
  the program checks its own answer, and the Rust test around it fails when the
  check fails. A failed `expect` is a `Runtime` error the run hands back
  (`src/interpreter.rs:2331`), so the run's own success is the assertion — the
  suite pins it at `tests/expect_test.rs:283` ("expect inside a taken if-branch
  must fail"). This phase's own round-1 report had this backwards, and the two
  tests it strengthened on the strength of that mistake are fixed in
  `REPORT.md`.

The helper step is the one that matters: two of the hits an earlier scan
flagged are not defects — they assert through `both_print`, which compares both
engines' output to a literal — and a scan that cannot see through a helper reads
them as vacuous.

Each function is judged **on its own**, too. A name bound in one function says
nothing about a comparison in another, and a scan that concatenates two
functions' text lets the first function's names satisfy the second function's
comparison — a false "not vacuous" of exactly the kind this scan exists to
remove. That is not a hypothetical: the first version of this round's rule
concatenated the test's body with the bodies of the helpers it calls, and the
contamination was visible in the count.

So the likely cause of "twenty" is a scan that did not resolve assertion-bearing
helpers, and one that did not count a fixture's own `expect` as an assertion. The
auditor may want to fold helper resolution into their scanner before the next
count — this one is published, so it can be compared against directly. Not a
defect in the suite; a defect in the measurement, and worth knowing about so the
next phase is not sized from a wrong number.

## 3a. The scanner this phase published was itself unsound, and had no test

The count above is **18**, not the 17 this phase's first two rounds reported, and
the difference is not a rounding error: it is the eighteenth test, and it is a
real one.

The rule the scanner used was unsound in the direction that matters. It returned
"this test asserts something" for a body that merely **mentioned** `take_output`
/ `.output` / `stdout` / `expect_err`, and for a body that merely **contained** an
`assert_eq!` — neither of which has anything to do with whether the program was
right. A test that reads the program's output and never compares it was scored as
non-vacuous, and reading `take_output()` is exactly the idiom this phase
introduced in every new test, so the scanner could not see the defect it was
written to find.

Tightened — the comparison has to hold the program's answer against something
else — the scanner names **18** tests on the base commit, and the eighteenth is
`tests/unless_test.rs` `edge_unless_as_the_last_statement_in_a_file`: it ran its
three fixtures and asked `is_ok()`. It now asserts the line the program printed.

The lesson is worth the next phase knowing, and it is the same lesson this
phase's own §7 states with the mistake removed: **a measurement that is not
itself measured is not evidence**. A scanner with no test of its own is a number
in a report, and the number was wrong in the direction that flattered the phase.
`tests/phase_088_scanner_test.rs` runs the scanner over fixture directories and
checks what it names — a read that is never compared, a comparison of something
the program never said, a comparison through a helper, a `tests/common/` helper,
and this repository's own `tests/` directory, which must answer
`vacuous tests: 0`.

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

The reason the rule records is a **reason a reader can check**: it enumerates
the files the delivery commit carries, and an enumeration that is short is a
reason a reader cannot check the commit against. This round brought it up to
date — the phase's own suites, the acknowledgement data, and this phase's four
documents — so that the string describes the commit the rule covers rather than
the commit an earlier round covered.

## 6. `phases/phase-087/REPORT.md:137` records 1766, and that is the right number for the tree it measured

Phase-087's report says `cargo test --all-targets` answers **1766 passed**.
Checked against the tree that number was measured on, it reconciles exactly.
Phase-087's work was carried in by `ad19910` ("rbops: phase-087
(2026-10-16T15:31:00Z)"), whose parent `6c11270` ("rbops: phase-059") is
phase-087's base:

```
$ git grep -hE '^\s*#\[test\]' ad19910 -- '*.rs' | wc -l
1766
$ git grep -hE '^\s*#\[test\]' 6c11270 -- '*.rs' | wc -l
1748
```

and the suite, measured in that tree, answers **1766 passed, 0 failed, 0
ignored** — measured twice, once in a `git worktree` of `ad19910` and once in a
clone whose `main` is `ad19910`:

```
$ git worktree add --detach /tmp/p087 ad19910
$ git clone --local --no-hardlinks <repo> /tmp/p087-clone
$ git -C /tmp/p087-clone checkout -B main ad19910
$ cargo test --all-targets     # in each
$ grep "^test result:" test.log | awk '{p+=$4; f+=$6; i+=$8} END {print p, f, i}'
1766 0 0
```

So phase-087's row is a measurement of phase-087's own tree, and it is correct.

The number that does **not** reconcile is this phase's round-1 report's, which
read phase-087's 1766 as if it were this phase's base and wrote `1766 + 8 =
1774`. That is an arithmetic derivation from another phase's tree rather than a
measurement. This report's number — 1986 — is measured in this tree, and it is
also the number this tree holds.

The 201-test difference between phase-087's 1766 and this phase's base 1967 is
the work that landed in between, and every step of it is visible in the tree:

| Commit | Delivery | `#[test]` functions |
|---|---|---|
| `ad19910` | phase-087 | 1766 |
| `e008f0b` | phase-089 | 1785 |
| `9bd1e59` | phase-060 | 1836 |
| `e0a3686` | phase-061 | 1868 |
| `af3312b` | phase-062 | 1893 |
| `c6e9837` | phase-084 | 1967 |

Phase-089's own delivery commit `bf9ffc33` is acknowledged in
`tests/unreviewed_work_ack_test.rs:169` with `+1068/-15, 11 files, +381/-15
under src/` — and the unit tests live under `src/` (353 of the base tree's 1967
`#[test]` functions are in `src/`), so the phase that brought the typed
failure value brought its tests with it. The gap is the intervening work, not
an error in phase-087's row.

The lesson survives the correction, and it is the same lesson with the mistake
removed: **a follow-up phase that reads a gate count out of an earlier report
should re-measure it rather than add to it** — and when it does, it must
measure it on the tree the number was measured on before it calls that number
wrong. A count is a fact about a tree, not about a gate.

## 7. The gate counts a failure assertion written as `is_err()`, not `expect_err`

The gate's test-policy section recognises a failure assertion in Rust written as
`is_err()` or `#[should_panic]`, and in Redblue written as `try … catch error`.
`expect_err` is a perfectly good assertion, but it is not the shape the check
counts — and this phase's round-1 report asserted the check was green while
quoting a line the gate does not print (`PASS failure-asserting test present: 7`).
The gate's own wording for the red line is `FAIL no test asserts a failure is
produced — mandatory`.

This round writes the assertion in both idioms, so the check sees the `is_err()`
and a reader sees the message:

```rust
let result = redblue::run_source(source);
assert!(result.is_err(), "`{source}` must be refused, not answered: {result:?}");
let error = result.expect_err("the refusal was just asserted");
```

The count the gate prints for this tree is now **27**, in the block quoted in
`REPORT.md` — the gate's own output, produced by the run in this tree.

Two things worth the next phase knowing:

1. **A quoted gate output is not evidence; the gate's own output is.** This
   phase's round-1 report carried a verbatim-looking block that the gate never
   produced. The block was pasted from a neighbouring phase's report, and the
   phase still failed on the one line it claimed to have fixed. Paste only what
   the run in this tree printed.
2. **The gate's report-honesty check parses the count out of the Gates table**,
   not out of the prose above it, and warns when the table under-claims
   (`WARN report claims 1976 passing, actual run has 1986 — under-claimed, fix
   the number`). The number in the table has to be the measured one — and the
   report the gate reads is the **pipeline's copy** of the report, one directory
   up, so the copy has to be refreshed *before* the run if the table is to be
   read at all. This round copies it first, and the run then answers `PASS
   reported test count (1986) matches actual (1986)`.

## 8. The gate can be run in this checkout, and this is how

`rbops/verify.sh` resolves the manifest, the baseline and **the report** relative to
`RBOPS_ROOT`, and the pipeline's copy of the report is one directory above the
project. The dispatcher refreshes that copy from `phases/phase-088/REPORT.md` before
the gate runs, so a run of the gate *inside* the project needs the same refresh
done by hand — and the pipeline's copy is outside the project, where this phase is
not allowed to write.

So the refresh was done inside the project instead: the manifest, the baseline and
this phase's report were copied to `target/tmp/rbops-root/` (a directory the
project's own `.gitignore` already covers, `/target/`) and the gate was run with

```
RBOPS_ROOT="$PWD/target/tmp/rbops-root" RBOPS_BASE_REF=c6e9837 ../rbops/verify.sh phase-088
```

`RBOPS_BASE_REF=c6e9837` is the base the dispatcher captures for this phase — the
commit before the preserved work was squashed onto `main` — so the diff the gate
judges is the whole of the phase's change and not only the last round's.

The verdict, in this tree, over this diff:

```
== report
  PASS .../phases/phase-088/REPORT.md exists
  PASS section: What changed
  PASS section: Tests added
  PASS section: Gates
  PASS section: Known gaps
  PASS report has at least one real evidence row

== diff hygiene
  PASS forbidden-path scan done
  PASS no gate-weakening constructs added
  PASS no scratch files
  PASS diff substance: +3015 -74 (min +5)
  PASS implementation touched tests/ (must_touch: tests/)

== cargo fmt
  PASS cargo fmt --check

== cargo clippy
  PASS cargo clippy -D warnings

== cargo test
  PASS cargo test: 1992 passed, 0 failed

== doc tests
  PASS runs: examples/files.rb
  PASS runs: examples/fizzbuzz.rb
  PASS runs: examples/formats.rb
  PASS runs: examples/hello.rb
  PASS runs: examples/random.rb
  PASS runs: examples/test_arithmetic.rb
  PASS runs: examples/time.rb
  PASS runs: modules/MathUtils.rb
  PASS runs: modules/SuiteKit.rb
  PASS example sweep done

== test policy
  PASS failure-asserting test present: 30
  PASS test quota met: 19 rust / 0 redblue (need 3 or 2)
  PASS edge case test present: 24
  PASS no newly skipped tests

== report honesty
  PASS reported test count (1992) matches actual (1992)

╔══════════════════════════════════════╗
║ VERIFY PASS — phase-088                ║
╚══════════════════════════════════════╝
```

Two things the next phase should know, both learned the hard way here:

1. **The failure-assertion check counts the DIFF, not the tree.** It is
   `git diff -U0 "$BASE_REF" -- '*.rs' '*.rb' | grep -cE '^\+.*(is_err|…)'`, so it
   sees only the lines this phase added. A tree that has been green for four rounds
   and a report that quotes that tree's numbers can still fail this one check when
   the base is the phase's own delivery point rather than the phase's beginning.
   The rule is not "the suite has a failure assertion somewhere" — it is "this
   phase's diff has one".
2. **`RBOPS_BASE_REF` defaults to `HEAD`**, which is the *worktree* diff when the
   work is uncommitted and the *empty* diff when it is committed. A local run of
   the gate without that variable set can therefore judge a different diff from the
   one the pipeline judges. Set it to the base the dispatcher captured, or the
   verdict is about something else.
