# Phase 088 — Make the reachability-only tests assert the value they reach for

## What changed

The finding reproduces. Tests were asking two engines whether a program *ran*, and
answering `Ok` was counted as proof the program was right. This diff makes those
tests read what the program answered, through `Vm::take_output` /
`BytecodeVm::take_output` and the value the run hands back, and holds each one to
a value written out literally in the test — and, where the answer must not exist
at all, holds it to a **refusal** that a caller can act on.

No production code changed: `git diff --numstat c6e9837 -- src/` is empty. The
defect was in the assertions, so the fix is in the assertions.

| File | Lines | What |
|---|---|---|
| `tests/stdlib_module_docs_test.rs` | +232 −18 | `edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does` no longer matches `Ok(_) => {}`; it asserts six **literal** expected lines, on the bytecode VM *and* on the walker. `edge_a_builtin_that_answers_the_wrong_value_fails_on_both_engines` pins seven literals per engine. `edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal` holds both engines to a refusal that names the builtin. New: `edge_a_builtin_that_cannot_answer_is_refused_by_the_library_not_answered_wrongly` — the library's own entry point (`run_source`) must come back `Err`, the error must be a `Runtime` error naming the builtin, and neither engine may have printed a line before refusing |
| `tests/loop_bounds_test.rs` | +203 −17 | `edge_type_mismatch_count_is_not_a_loop` and `edge_type_mismatch_iterable_is_not_a_loop` now `take_output()` and assert it is empty; `runaway_loop_is_catchable_from_inside_redblue` ends its program on the flag the `catch` set and asserts the value the run hands back is `YesNo(true)`; `edge_nested_loops_each_get_the_full_cap` asserts `12`; `step_budget_does_not_fire_under_the_limit` asserts `10`; `edge_step_budget_is_not_spent_by_unentered_loops` asserts the **exact** lines printed, and **both** of its fixtures put a `say "never"` in the body, so the shared `never` check discriminates on either row rather than on one; `edge_deeply_nested_loops_terminate` asserts `65536`. Two new tests pin the *positive* direction — a loop that does run prints one line per turn, and a loop whose body says nothing still runs every turn — and that second one pins the loop's own value by ending its program on the loop, so the value the run hands back is the loop's and not the `say` after it |
| `tests/loop_control_test.rs` | +12 −1 | `edge_a_break_stops_the_loop_before_the_iteration_cap` now ends its program on the variable the body changes and asserts the value `1` — the number of turns the loop actually took, not the absence of an error |
| `tests/time_builtin_test.rs` | +12 −2 | `time_sleep_of_zero_seconds_is_accepted` matched `Ok(_) => {}`; it now ends its program on `slept is nothing` and asserts the `YesNo(true)` that comparison answers, which is the shape `time.sleep` is documented to have |
| `tests/redblue_test.rs` | +190 −15 | all five tests now assert the exact lines their program prints, through a `printed()` helper that runs the program **once** and reads `run_isolated` + `take_output`. Four new tests: a value that is a number and not its rendering, both shapes of empty output, a guard that the helper is actually reading output, and a program that fails rather than printing nothing — the last of these now uses `expect_err`, so the refusal is asserted in one line instead of a `match` |
| `tests/text_split_join_test.rs` | +28 −4 | `a_wrongly_typed_split_argument_is_caught_by_try_catch_in_a_redblue_program` now ends its program on the flag the `catch` set and asserts the value the run hands back is `YesNo(true)`, so a `catch` that caught nothing cannot pass. A failed `expect` is a `Runtime` error the run hands back (`src/interpreter.rs:2331`), so the program's own top-level `expect caught to be yes` already fails the run — the value assertion is the second, independent reading of the same run |
| `tests/unreviewed_work_ack_test.rs` | +20 −13 | the delivery rule is re-pointed from `resume phase-084:` to `resume phase-088:`, the phase that owns the file now, and phase-084's delivery commit `1d73e8c` is acknowledged **by SHA** with its numstat — it is an ancestor of HEAD and stays one, so the entry cannot go stale the way a re-created delivery commit's SHA would |
| `phases/phase-088/scan.py` | new | the scanner that counts the reachability-only tests, published so the next audit can re-run it instead of believing a number |
| `phases/phase-088/scanned-files.txt` | new | the file list that scan read — all 68 `tests/*.rs` files |
| `phases/phase-088/REPORT.md` | new | this file |
| `phases/phase-088/FINDINGS.md` | new | the findings that are not this phase's to fix |

Zero tests were deleted. `cargo test --all-targets` reports **1976** passing, 0
failing, 0 ignored, and the number is derived rather than transcribed. This tree
holds 1976 `#[test]` functions, the base commit `c6e9837` holds 1967, and the
nine `#[test]` functions this phase adds are exactly the difference
(1967 + 9 = 1976). The count is checkable without running the suite:

```
$ git grep -hE '^\s*#\[test\]' -- '*.rs' | wc -l
1976
$ git grep -hE '^\s*#\[test\]' c6e9837 -- '*.rs' | wc -l
1967
```

and the suite agrees with it exactly — the base commit's own tree, measured with
the same command, answers 1967 passing, 0 failing, 0 ignored, and this tree
answers 1976 passing, 0 failing, 0 ignored.

## The finding, reproduced

The finding's three named sites, on the base commit, before any change:

```
$ printf 'say abs(-3)\nsay uppercase("hi")\nsay trim("  x  ")\nsay sqrt(9)\nsay text.uppercase("hi")\nsay text.split("a,b", ",")\n' > repro.rb
$ ./target/debug/rb run repro.rb
3
HI
x
3
HI
[a, b]
```

`tests/stdlib_module_docs_test.rs:242` ran exactly those six programs and matched
`Ok(_) => {}`. Every one of the values above was unasserted; `sqrt` answering `1`
would have passed it.

The mutation experiment, run against the **new** tests — `src/stdlib.rs:580`,
`sqrt`'s text arm changed from `_ => None` (refuse) to `_ => Some(Value::Number(0.0))`
(answer wrongly), so `sqrt("nine")` answers `0` instead of refusing:

```
---- edge_a_builtin_that_cannot_answer_is_refused_by_the_library_not_answered_wrongly stdout ----
`say sqrt("nine")` must be refused, not answered: ()

---- edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal stdout ----
`say sqrt("nine")` must be refused by the walker: Nothing
```

Two tests went red, including the two this phase added. Under the old
`Ok(_) => {}` body the same mutation returned `Ok` and passed — that is the
defect, demonstrated rather than asserted. `src/stdlib.rs` was restored from
`target/tmp/stdlib.rs.bak`; `git diff --numstat c6e9837 -- src/` is empty.

## Tests added

| Test | File | Edge class covered |
|---|---|---|
| `edge_a_builtin_that_cannot_answer_is_refused_by_the_library_not_answered_wrongly` | `stdlib_module_docs_test.rs:422` | **failure is produced**; wrong value refused by name on three channels; empty output before the refusal |
| `edge_a_builtin_that_answers_the_wrong_value_fails_on_both_engines` | `stdlib_module_docs_test.rs:299` | wrong value, per engine; empty-string output |
| `edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal` | `stdlib_module_docs_test.rs:352` | type mismatch + failure is produced, on both engines |
| `edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does` (rewritten) | `stdlib_module_docs_test.rs:249` | wrong value, both engines, six builtins |
| `edge_a_loop_that_does_run_prints_one_line_per_turn_in_order` | `loop_bounds_test.rs:719` | loop that **must** run, exact turn count |
| `edge_a_loop_whose_body_says_nothing_still_runs_every_turn` | `loop_bounds_test.rs:742` | the body says nothing; a loop statement is worth `nothing` |
| `edge_an_empty_answer_is_pinned_for_both_shapes_of_empty` | `redblue_test.rs:166` | empty: one empty line vs no lines |
| `edge_the_value_a_program_ends_on_is_a_number_and_not_its_rendering` | `redblue_test.rs:122` | a value read back, not a rendering |
| `edge_a_program_that_says_something_differs_from_one_that_does_not` | `redblue_test.rs:194` | non-empty output; guards the helper itself |
| `edge_a_program_that_cannot_run_reports_a_failure_rather_than_no_output` | `redblue_test.rs:215` | failure is produced, not an empty answer |

### The reachability-only tests — every one, with its reason

Re-verified rather than taken on trust, and the count is **derived**, not
asserted: the scanner is published at `phases/phase-088/scan.py`, and the file
list it read is published at `phases/phase-088/scanned-files.txt`. Run it against
the base commit's `tests/` and it names **17 tests in 5 files**; run it against
this tree's `tests/` and it names **0**.

```
$ git archive c6e9837 tests | tar -x -C /tmp/base
$ python3 phases/phase-088/scan.py /tmp/base/tests
scanned 68 files in /tmp/base/tests
loop_bounds_test.rs:186	edge_zero_iterations_is_allowed
loop_bounds_test.rs:207	edge_exactly_max_iterations_succeeds
loop_bounds_test.rs:272	edge_nested_loops_each_get_the_full_cap
loop_bounds_test.rs:309	step_budget_does_not_fire_under_the_limit
loop_bounds_test.rs:322	edge_step_budget_is_not_spent_by_unentered_loops
loop_bounds_test.rs:570	edge_fractional_and_negative_counts_run_zero_times
loop_bounds_test.rs:586	edge_type_mismatch_count_is_not_a_loop
loop_bounds_test.rs:596	edge_type_mismatch_iterable_is_not_a_loop
loop_bounds_test.rs:637	edge_deeply_nested_loops_terminate
loop_control_test.rs:486	edge_a_break_stops_the_loop_before_the_iteration_cap
redblue_test.rs:4	test_hello_world
redblue_test.rs:10	test_numbers
redblue_test.rs:22	test_math
redblue_test.rs:35	test_list
redblue_test.rs:46	test_function
stdlib_module_docs_test.rs:242	edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does
time_builtin_test.rs:273	time_sleep_of_zero_seconds_is_accepted
vacuous tests: 17

$ python3 phases/phase-088/scan.py tests
scanned 68 files in tests
vacuous tests: 0
```

The scan reads all **68** `tests/*.rs` files — the list is
`phases/phase-088/scanned-files.txt` — and keeps a test that runs a Redblue
program (its own body, or the body of a same-file helper it calls) and asserts
nothing about what the program said, recorded or answered. `assert_eq!`,
`assert_ne!` and `assert_matches!` count, as does an `assert!` whose condition
says more than `is_ok()`; a `panic!` does **not** count, because a test that
panics only on `Err` proves the program ran and nothing else; a Redblue-level
`expect ... to be` does count, because the program checks its own answer and a
failed check is a `Runtime` error the run hands back
(`src/interpreter.rs:2331`), so the Rust test around it fails when it does not.
The scanner's own docstring says the same thing, in step with the code it runs.

All seventeen were given an assertion; none was deleted.

| Test | Given |
|---|---|
| `loop_bounds_test.rs:205` `edge_zero_iterations_is_allowed` | the count of turns the loop took: `0` |
| `loop_bounds_test.rs:242` `edge_exactly_max_iterations_succeeds` | the count of turns: `10` |
| `loop_bounds_test.rs:313` `edge_nested_loops_each_get_the_full_cap` | output `["12"]` |
| `loop_bounds_test.rs:358` `step_budget_does_not_fire_under_the_limit` | output `["10"]` |
| `loop_bounds_test.rs:378` `edge_step_budget_is_not_spent_by_unentered_loops` | the exact lines: `[]` for the `while` row, `["after"]` for the `for each` row, and `never` absent from both — a body that ran would have printed it, because **both** fixtures say `never` in the body |
| `loop_bounds_test.rs:656` `edge_fractional_and_negative_counts_run_zero_times` | the count of turns: `0` |
| `loop_bounds_test.rs:684` `edge_type_mismatch_count_is_not_a_loop` | `take_output()` empty |
| `loop_bounds_test.rs:699` `edge_type_mismatch_iterable_is_not_a_loop` | `take_output()` empty |
| `loop_bounds_test.rs:812` `edge_deeply_nested_loops_terminate` | output `["65536"]` |
| `loop_control_test.rs:491` `edge_a_break_stops_the_loop_before_the_iteration_cap` | the count of turns: `1` |
| `stdlib_module_docs_test.rs:249` `edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does` | six literal lines, bytecode VM and walker |
| `time_builtin_test.rs:279` `time_sleep_of_zero_seconds_is_accepted` | `YesNo(true)`: the sleep was allowed and answered `nothing` |
| `redblue_test.rs:52` `test_hello_world` | the exact line: `["Hello, World!"]` |
| `redblue_test.rs:60` `test_numbers` | the exact line: `["10"]` |
| `redblue_test.rs:74` `test_math` | the exact line: `["30"]` |
| `redblue_test.rs:89` `test_list` | the exact line: `["[1, 2, 3]"]` |
| `redblue_test.rs:102` `test_function` | the exact line: `["Hello, World!"]` |

Two more were strengthened although the scanner does not require it — both carry a
Redblue-level `expect ... to be`, which the scanner counts as an assertion, while
the Rust test around them said nothing about the program's answer:
`loop_bounds_test.rs:159` `runaway_loop_is_catchable_from_inside_redblue` and
`text_split_join_test.rs:759`
`a_wrongly_typed_split_argument_is_caught_by_try_catch_in_a_redblue_program`. Each
of them now ends its program on the flag its own `catch` set and asserts the
value the run hands back is `YesNo(true)`, so the assertion is the program's
answer and not the fact that it ran. Nineteen tests were given an assertion in
all; the scanner names seventeen of them.

Six further candidates were checked by hand and are **not** defects, so the
scanner does not name them: `assertions_test.rs:207`, `:237` and `:332` are three
tests that assert through `ok()` / `fails()`, whose `Result` is `.unwrap()`ed
(`assertions_test.rs:64`, `:68`) — a wrong result panics; `might_fail_test.rs:301`
and `:315` are two more that assert through `both_print`
(`might_fail_test.rs:157`), which compares both engines' output to a literal; and
`might_fail_test.rs:1079` `strip_markers` is not a test at all but a nested helper
inside `a_guard_whose_frame_finished_does_not_swallow_the_next_call_s_failure`.
So the six are **five tests that assert through a helper** and **one nested
helper that is not a test** — and the scanner reaches the same verdict on all
six, because it follows the helper. They are recorded here so the next auditor
does not re-open them.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test --all-targets` | 1976 passed, 0 failed, 0 ignored |
| `cargo test --doc` | 2 passed, 0 failed |
| `../rbops/verify.sh phase-088` | **pass** — `VERIFY PASS — phase-088`, 0 checks failed |

The `cargo test --all-targets` number is **this tree's, measured**, not derived
from a neighbouring report: the run was made in this checkout, in this tree, and
it answers 1976 passed, 0 failed, 0 ignored. It is also the number the tree
itself holds — 1976 `#[test]` functions — and the base commit's tree holds 1967
and measures 1967 with the same command, so the two reports of the same gate
reconcile to the nine tests this phase adds.

`phases/phase-087/REPORT.md:137` records `1766 passed` for the same gate, and
that row does not reconcile with the tree it claims to measure: the base commit
`c6e9837` holds 1967 `#[test]` functions and measures 1967. The 201-test gap is
in phase-087's row, not in this one. This phase's own committed round-1 report
compounded it by adding its eight new tests to the same 1766 — an arithmetic
derivation from a wrong base rather than a measurement — and this round replaces
that derivation with the measured count and with the two commands that produce
it.

`--all-targets` excludes doctests, so this row is smaller than a plain
`cargo test` would report by the two doctests in the row below it.

The fourth gate lives outside the project checkout, at `../rbops/verify.sh`
(relative to the project root) — `rbops/` is not in this tree, and `ls rbops`
answers `No such file or directory`. It was run with
`RBOPS_BASE_REF=c6e9837`. That base ref is the phase's base commit; the pipeline
records it in `../phases/phase-088/.base`, which is **outside this checkout**
(the project's own `phases/` directory holds the phase's documents, not the
pipeline's bookkeeping — `ls phases/phase-088/.base` answers "No such file or
directory" here). Its verdict, verbatim:

```
== test policy
  PASS failure-asserting test present: 7
  PASS test quota met: 9 rust / 0 redblue (need 3 or 2)
  PASS edge case test present: 9
  PASS no newly skipped tests
...
== report honesty
  PASS reported test count (1976) matches actual (1976)
...
╔══════════════════════════════════════╗
║ VERIFY PASS — phase-088                ║
╚══════════════════════════════════════╝
```

The one line that was red on the previous round — `FAIL no test asserts a
failure is produced — mandatory` — is the line the new test in
`tests/stdlib_module_docs_test.rs:422` answers, together with the two
`expect_err` idioms that replaced the two `match`-on-`Result` assertions.

## Edge-case matrix

- **empty** — covered. `edge_an_empty_answer_is_pinned_for_both_shapes_of_empty`
  pins `say ""` (one empty line) against a program with no `say` (no lines) as two
  separate claims; `uppercase("")` → `""` in the wrong-value table.
- **singleton** — covered. `greet("World")` in `test_function` prints exactly one
  line; `text.split("a,b", ",")` is a two-element list against a `split` that did
  not split.
- **boundary** — covered. The loop tests assert exact turn counts at the cap
  (`3`, `10`, `3 × 4 = 12`, `2^16 = 65536`), so one turn early, one turn late, or
  a nesting bug that halved the count all change the printed line.
- **out_of_bounds** — N/A. This phase changes no index handling and no bounds
  check; `tests/index_bounds_test.rs` and `tests/index_words_test.rs` cover it and
  were not touched.
- **type_mismatch** — covered. `edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal`
  (`sqrt("nine")`, `abs("three")`, `text.split(1, ",")`), plus the two
  `edge_type_mismatch_*` loop tests and the new
  `edge_a_builtin_that_cannot_answer_is_refused_by_the_library_not_answered_wrongly`.
- **numeric_boundary** — partial, and stated as partial. `sqrt(9)` is pinned to the
  exact `3` and `abs(-3)` to `3`, and a `repeat 4 times` statement is pinned to
  `Value::Nothing` by a program that **ends on the loop** — so the value the run
  hands back is the loop's own and not the value of the statement after it — but
  `0/0`, `±Infinity`, `NaN` and `2^53±1` are not re-pinned here: they are pinned
  already in `tests/numeric_edge_test.rs` and this phase changes no numeric code.
- **unicode** — covered. `trim("日本語")` is `日本語` in the wrong-value table, on
  both engines — a `trim` that measured bytes would eat half a CJK character.
- **nesting_recursion** — covered. `edge_nested_loops_each_get_the_full_cap`
  (three loops two deep, 12) and `edge_deeply_nested_loops_terminate` (sixteen
  levels, exactly 65536 turns).
- **duplicate_missing_keys** — N/A. No record is read or built by any test this
  phase changed; `tests/record_order_test.rs` owns that row and was not touched.
- **malformed_input** — partial. `edge_a_program_that_cannot_run_reports_a_failure_rather_than_no_output`
  covers a program that names a function that does not exist, and the two
  `type_mismatch` loop tests cover a count and an iterable of the wrong type.
  Unterminated strings, BOMs and CRLF are the business of
  `tests/lexer_robustness_test.rs` and `tests/parser_hardening_test.rs`, which
  this phase did not touch.
- **resource_limit** — covered. Every file this phase changed in
  `loop_bounds_test.rs` is about a limit, and each of those tests now asserts the
  count the limit let through rather than only that it did not fire.

## Invariants touched

- None. No file under `src/` changed; no grammar, no `Value` variant, no `say`
  semantics, no `.rb` extension. The tests read output through the public
  `Vm::take_output` and `BytecodeVm::take_output` that already existed.

## Known gaps / follow-ups

- **The delivery acknowledgement is now written.** The previous attempt recorded
  that `tests/unreviewed_work_ack_test.rs` requires every commit in `HEAD` whose
  subject marks unreviewed work to be acknowledged, and that the pipeline's
  delivery commit did not exist yet, so its SHA could not be pinned. The pipeline
  has since created it — `f0d4a46e8f2cd0e6138a25b329504b626b9aa44e`, subject
  `resume phase-088: adopt preserved work` — and the audit failed on it exactly as
  predicted. It is now acknowledged by the **rule**, not by a SHA: the rule's
  prefix is re-pointed from `resume phase-084:` to `resume phase-088:`, the phase
  that owns the file, which is the design the file itself documents ("the rule
  must acknowledge exactly the commit the pipeline creates for THIS phase"). A
  SHA-pinned entry would have gone stale on the next round, because the pipeline
  rebuilds that commit on the same parent every round — the failure
  `edge_a_rebuilt_delivery_commit_is_neither_unreviewed_nor_stale` exists to
  describe. The commit the rule used to cover, phase-084's `1d73e8c`, is now
  acknowledged **by SHA** in `ACKNOWLEDGED` with its numstat (`+7103/-55`, 7
  files, none under `src/`); it is an ancestor of `HEAD` and stays one, so that
  entry cannot go stale.
- **`redblue::run_source` is `Result<(), Error>` and maps the program's value away at
  `src/lib.rs:110` (`result.map(|_| ())`).** That is what made
  `tests/redblue_test.rs` unable to see a wrong answer, and it is a **public API**
  change, so it is out of scope for a phase whose `must_touch` is `tests/`. The
  tests now route around it with `run_source_value` / `run_isolated`. →
  `phases/phase-088/FINDINGS.md` §1.
- **`Vm::run` has no `set_echo`, unlike `BytecodeVm::run`.** A tree-walking test
  therefore prints its program's lines into the test runner's captured stdout.
  Harmless, and it is why the assertion uses `take_output` rather than stdout. →
  `FINDINGS.md` §2.
- **The auditor's "twenty" is seventeen under the scanner published here** (and
  two more were strengthened anyway, so nineteen tests were given an assertion).
  The scanner is `phases/phase-088/scan.py`, the file list it read is
  `phases/phase-088/scanned-files.txt`, and the exact commands are in the section
  above, so the next audit can re-run it instead of taking the number on trust.
  The discrepancy and the reasoning are in `FINDINGS.md` §3.
- **A failed `expect` fails the run, and this phase's round-1 report said the
  opposite.** `Expr::Expect` answers `Err(Error::Runtime(..))`
  (`src/interpreter.rs:2331`), and nothing catches that at the top level, so
  `run_source` and `Vm::run` come back `Err` — the suite already pins it at
  `tests/expect_test.rs:283` ("expect inside a taken if-branch must fail"). The
  round-1 report claimed the opposite ("its `expect caught to be yes` never made
  `run_source` fail"), and the two tests it strengthened asserted
  `take_expectation_failure().is_none()` *after* the `.expect(...)` that the
  same failure had already turned into a panic — a check that could not fail.
  Both tests now end their program on the flag their own `catch` set and assert
  the value the run hands back, which is the program's **answer** rather than a
  second reading of the same fact. The scanner's docstring said the opposite of
  what the scanner does as well, and is now in step with the code it runs.
- **The previous phase's gate row does not reconcile with its own tree.**
  `phases/phase-087/REPORT.md:137` records `1766 passed` for the same gate; the
  base commit `c6e9837` holds 1967 `#[test]` functions and measures 1967. The
  gap is in that row, and this phase's round-1 report compounded it by adding
  its eight new tests to the same 1766. The count here is measured, and the
  commands that produce it are in the "Gates" section. →
  `phases/phase-088/FINDINGS.md` §6.
- **The fourth gate is run from outside the checkout** and the pipeline copies
  this report into `phases/phase-088/REPORT.md` before it runs, so the verdict
  quoted above is the one the pipeline saw.
