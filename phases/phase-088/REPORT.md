# Phase 088 — Make the reachability-only tests assert the value they reach for

## What changed

The finding reproduces. Tests were asking two engines whether a program *ran*, and
answering `Ok` was counted as proof the program was right. This diff makes those
tests read what the program answered, through `Vm::take_output` /
`BytecodeVm::take_output`, and holds each one to a value written out literally in
the test.

No production code changed: `git status --porcelain src/` is empty. The defect was
in the assertions, so the fix is in the assertions.

| File | Lines | What |
|---|---|---|
| `tests/stdlib_module_docs_test.rs` | +160 −18 | `edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does` no longer matches `Ok(_) => {}`; it asserts six **literal** expected lines, on the bytecode VM *and* on the walker. Two new tests: a builtin that answers the wrong value on each engine, and a builtin given a text that says nothing and names the refusal identically on both engines |
| `tests/loop_bounds_test.rs` | +139 −6 | `edge_type_mismatch_count_is_not_a_loop` and `edge_type_mismatch_iterable_is_not_a_loop` now `take_output()` and assert it is empty; `runaway_loop_is_catchable_from_inside_redblue` asserts `take_expectation_failure().is_none()`; `edge_nested_loops_each_get_the_full_cap` asserts `12`; `step_budget_does_not_fire_under_the_limit` asserts `10`; `edge_step_budget_is_not_spent_by_unentered_loops` asserts `never` was not printed **and** the line after an unentered loop did run; `edge_deeply_nested_loops_terminate` asserts `65536`. Two new tests pin the *positive* direction (a loop that does run prints one line per turn) and the empty-output case |
| `tests/redblue_test.rs` | +186 −15 | all five tests now assert the exact lines their program prints, through a `printed()` helper that runs `run_source` **and** `run_isolated` + `take_output`. Four new tests: a value that is a number and not its rendering, both shapes of empty output, a guard that the helper is actually reading output, and a program that fails rather than printing nothing |
| `tests/text_split_join_test.rs` | +20 −0 | `a_wrongly_typed_split_argument_is_caught_by_try_catch_in_a_redblue_program` now reads `take_expectation_failure()` — its `expect caught to be yes` never made `run_source` fail, so a `catch` that caught nothing passed |
| `phases/phase-088/REPORT.md` | new | this file |

Zero tests were deleted. `cargo test` went **1766 → 1774** (the 8 new tests) and is
**1774 against the 1414 measured at audit time**.

## Reproduction, and proof the new assertions can fail

The finding's three named sites, on `HEAD`, before any change:

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

The mutation experiment, run against the **new** tests — `src/stdlib.rs:566`,
`sqrt`'s `n.sqrt()` changed to `(n.sqrt()).min(1.0f64)`, so `sqrt(9)` answers `1`:

```
---- edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does stdout ----
assertion `left == right` failed: the bytecode VM must answer `say sqrt(9)` with `3`, not merely not fail

---- edge_a_builtin_that_answers_the_wrong_value_fails_on_both_engines stdout ----
assertion `left == right` failed: the walker must answer `say sqrt(9)` with `3`
```

Six tests in the file went red, including the two this phase added. Under the old
body the same mutation returned `Ok` and passed — that is the defect, demonstrated
rather than asserted. `src/stdlib.rs` was restored from `target/tmp/stdlib.rs.bak`;
`git diff src/` is empty.

## Tests added

| Test | File | Edge class covered |
|---|---|---|
| `edge_a_builtin_that_answers_the_wrong_value_fails_on_both_engines` | `stdlib_module_docs_test.rs:299` | wrong value, per engine; empty-string output |
| `edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal` | `stdlib_module_docs_test.rs:352` | type mismatch + failure is produced |
| `edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does` (rewritten) | `stdlib_module_docs_test.rs:249` | wrong value, both engines, six builtins |
| `edge_a_loop_that_does_run_prints_one_line_per_turn_in_order` | `loop_bounds_test.rs:677` | loop that **must** run, exact turn count |
| `edge_a_loop_whose_body_says_nothing_still_runs_every_turn` | `loop_bounds_test.rs:700` | empty output; a loop statement is worth `nothing` |
| `edge_an_empty_answer_is_pinned_for_both_shapes_of_empty` | `redblue_test.rs:157` | empty: one empty line vs no lines |
| `edge_the_value_a_program_ends_on_is_a_number_and_not_its_rendering` | `redblue_test.rs:113` | a value read back, not a rendering |
| `edge_a_program_that_says_something_differs_from_one_that_does_not` | `redblue_test.rs:185` | non-empty output; guards the helper itself |
| `edge_a_program_that_cannot_run_reports_a_failure_rather_than_no_output` | `redblue_test.rs:206` | failure is produced, not an empty answer |

### The "twenty with no reachable assert" — every one, with its reason

Re-verified rather than taken on trust. A scanner over all 63 files in `tests/`
(parsed with comments and string literals stripped, `assert*`/`panic!` detected
transitively through same-file helpers) found **10** tests that run a Redblue
program and never read its output, its recorded expectation, or its returned
value. All ten were given an assertion; none was deleted.

| Test | Given |
|---|---|
| `stdlib_module_docs_test.rs:242` `edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does` | six literal lines, bytecode VM and walker |
| `loop_bounds_test.rs:159` `runaway_loop_is_catchable_from_inside_redblue` | `take_expectation_failure().is_none()` |
| `loop_bounds_test.rs:271` `edge_nested_loops_each_get_the_full_cap` | output `["12"]` |
| `loop_bounds_test.rs:308` `step_budget_does_not_fire_under_the_limit` | output `["10"]` |
| `loop_bounds_test.rs:322` `edge_step_budget_is_not_spent_by_unentered_loops` | `never` absent; the line after the unentered loop printed |
| `loop_bounds_test.rs:586` `edge_type_mismatch_count_is_not_a_loop` | `take_output()` empty |
| `loop_bounds_test.rs:596` `edge_type_mismatch_iterable_is_not_a_loop` | `take_output()` empty |
| `loop_bounds_test.rs:637` `edge_deeply_nested_loops_terminate` | output `["65536"]` |
| `text_split_join_test.rs:759` `a_wrongly_typed_split_argument_is_caught_by_try_catch_in_a_redblue_program` | `take_expectation_failure().is_none()` |
| `redblue_test.rs` × 5 (named separately by the phase) | the exact lines each program prints |

Five further hits were **false positives**, checked by reading the helpers and
recorded here so the next auditor does not re-open them:
`assertions_test.rs:207`, `:237` and `:332` assert through `ok()` / `fails()`,
whose `Result` is `.unwrap()`ed (`assertions_test.rs:64`, `:68`) — a wrong result
panics; `might_fail_test.rs:329` asserts through `both_print`
(`might_fail_test.rs:157`), which compares both engines' output to a literal;
`might_fail_test.rs:1079` `strip_markers` is not a test at all but a nested helper
inside `a_guard_whose_frame_finished_does_not_swallow_the_next_call_s_failure`.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test --all-targets` | **1774 passed, 0 failed, 0 ignored** |
| `cargo test --doc` | 2 passed, 0 failed |
| `./rbops/verify.sh phase-088` | **not run — `rbops/` is not in this checkout** |

On the fourth gate, stated plainly rather than claimed: `rbops/verify.sh` does not
exist in this working tree (`ls rbops` → `No such file or directory`;
`git ls-files | grep '^rbops/'` matches nothing). The RBOPS pipeline lives outside
the project checkout. I ran the gates that exist here and did not fabricate a
result for the one that does not.

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
  `edge_type_mismatch_*` loop tests.
- **numeric_boundary** — partial, and stated as partial. `sqrt(9)` is pinned to the
  exact `3` and `abs(-3)` to `3`, and `repeat 4 times` to `Value::Nothing`, but
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

- **A `resume phase-088:` WIP commit will need an acknowledgement.**
  `tests/unreviewed_work_ack_test.rs` requires every commit in `HEAD` whose subject
  matches the unreviewed-work shape to be on `ACKNOWLEDGED` keyed by its full
  object id, and that id does not exist until the pipeline makes the commit — so
  it cannot be written from here. If the pipeline writes
  `resume phase-088: adopt preserved work`, the entry needs to be added with a
  re-derivable numstat: **`+505/-39`, 4 files, all under `tests/`; `+0/-0` under
  `src/`**. `phases/phase-087/REPORT.md` records the same obligation for its own
  WIP commit.
- `redblue::run_source` is `Result<(), Error>` and maps the program's value away at
  `src/lib.rs:108` (`result.map(|_| ())`). That is what made
  `tests/redblue_test.rs` unable to see a wrong answer, and it is a **public API**
  change, so it is out of scope for a phase whose `must_touch` is `tests/`. The
  tests now route around it with `run_source_value` / `run_isolated`. →
  `phases/phase-088/FINDINGS.md`.
- `Vm::run` has no `set_echo`, unlike `BytecodeVm::run`. A tree-walking test
  therefore prints its program's lines into the test runner's captured stdout.
  Harmless, and it is why the assertion uses `take_output` rather than stdout. →
  `FINDINGS.md`.
- The auditor measured "twenty" tests with no reachable assertion; I found ten
  (plus the five in `redblue_test.rs`). The discrepancy and the exact scan are in
  `FINDINGS.md` so the next audit can re-run it.