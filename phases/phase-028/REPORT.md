# Phase 028 — Implement the range loop `for each i from A to B [by C]`

## What changed

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +63 −22 | `parse_for` gains a `from A to B [by C]` arm producing `Statement::ForRange`; the shared `{ statement } 'end'` tail extracted to `parse_loop_body`; `at_range_step_marker` reads `by` positionally |
| `src/value.rs` | +52 | `expect_range_number` (a non-number bound is a `RuntimeError` naming the argument), `range_has_next` (the step's sign picks the direction), `Value::type_name` |
| `src/vm.rs` | +26 −18 | `ForRange` arm uses both helpers instead of an `if let` that silently skipped non-numeric bounds and an `i <= end` test that could only count up |
| `src/bytecode/vm.rs` | +21 −16 | same two helpers in `Sequence::Range`, so both VMs agree |
| `src/runtime.rs` | +1 −12 | the `type_of` builtin now calls `Value::type_name` instead of repeating its own table (byte-identical output) |
| `SPEC.md` | +17 | the two rules the phase introduces — the step's sign is the direction, and a non-number bound is an error — written next to the example they govern |
| `tests/for_range_test.rs` | +521 (new) | 31 tests |
| `tests/numeric_edge_test.rs` | +6 −5 | comment only: it claimed `Statement::ForRange` was unreachable from source |

## Reproduction

```
$ printf 'for each i from 1 to 3\n    say i\nend\n' > target/tmp/range_repro.rb
$ ./target/debug/rb run target/tmp/range_repro.rb
Error: ParserError: Expected In but got From
  --> target/tmp/range_repro.rb:1:12
1 | for each i from 1 to 3
  |            ^
exit=1
```

The finding held on `main`. Reproduced before any edit.

## Tests added

31 new `#[test]` functions in `tests/for_range_test.rs` (floor is 3); 18 are
named `edge_*` (floor is 1); 14 assert a failure is produced (floor is 1). No
new `#[ignore]`, no `// skip`, no `allow(clippy::`, no existing test weakened.

| Test | Edge class covered |
|---|---|
| `a_range_loop_visits_every_value_in_order` | boundary — the documented `from 1 to 5` |
| `a_step_of_five_visits_only_the_multiples_of_five` | boundary — `by 5` over 0..10, nothing between |
| `a_step_of_one_and_an_omitted_step_are_the_same_loop` | boundary — `by 1` ≡ no `by`, asserted as equal output |
| `a_step_that_does_not_land_on_the_end_still_stops_at_or_before_it` | boundary — `by 3` over 0..10 stops at 9 |
| `a_range_loop_nests_and_the_inner_variable_is_its_own` | nesting_recursion — same name nested, shadow then restore |
| `the_loop_variable_is_a_local_that_does_not_leak` | resource/state — 4 iterations whatever the body writes; outer `i` still 99 |
| `edge_the_loop_variable_is_out_of_scope_after_end` | resource/state — `AnalyzerError`, not a silent global |
| `edge_an_empty_range_visits_nothing_and_does_not_hang` | empty — `from 5 to 1` body runs 0 times |
| `edge_a_range_of_one_visits_exactly_one_value` | singleton — `from 3 to 3`, inclusive |
| `edge_a_descending_step_counts_down` | boundary — `from 5 to 1 by -1` visits 5,4,3,2,1 |
| `edge_a_descending_step_that_starts_below_its_end_is_empty` | empty — `from 1 to 5 by -1` visits nothing |
| `edge_a_fractional_step_on_a_whole_number_range` | numeric_boundary — `by 0.25` over 0..1, exact |
| `edge_a_fractional_step_that_would_overshoot_still_terminates` | numeric_boundary / resource_limit — accumulated `by 0.1` cannot loop forever |
| `edge_a_non_number_from_is_a_runtime_error_naming_from` | type_mismatch |
| `edge_a_non_number_to_is_a_runtime_error_naming_to` | type_mismatch — names `to`, not `from` |
| `edge_a_non_number_by_is_a_runtime_error_naming_by` | type_mismatch — names `by` |
| `edge_a_list_as_a_bound_is_refused_rather_than_skipped` | type_mismatch — list, record and `nothing`, each by type name |
| `edge_a_step_missing_its_end_of_the_range_is_a_parser_error` | malformed_input — `to` with nothing after it |
| `edge_a_by_before_the_to_is_a_parser_error` | malformed_input — the marker is only read after `to` |
| `edge_a_for_each_with_neither_in_nor_from_is_a_parser_error` | malformed_input — `for each i 3` |
| `edge_a_zero_step_stops_at_the_iteration_guard_rather_than_hanging` | resource_limit — **asserts a failure**: the exact `MAX_ITERATIONS` message |
| `edge_a_range_longer_than_the_iteration_limit_stops_like_a_repeat_does` | resource_limit — **asserts a failure**: same limit as `repeat` at the same cap |
| `edge_a_range_at_exactly_the_iteration_limit_still_finishes` | resource_limit — the guard's off-by-one boundary, `cap` allowed |
| `edge_a_step_that_overflows_the_counter_is_a_runtime_error` | numeric_boundary / failure — `infinity is not a finite number` |
| `the_formatter_prints_a_range_loop_and_the_result_still_runs` | malformed_input — formatter round-trip is a fixed point and still visits 0,5,10 |
| `the_linter_analyzes_a_range_loop_body` | resource/state — the body is not skipped |
| `the_analyzer_accepts_a_range_loop_and_its_step` | boundary |
| `an_unanalysable_range_bound_is_reported_by_the_analyzer` | malformed_input / failure |
| `a_range_loop_is_deterministic_across_runs` | determinism |
| `a_range_variable_does_not_collide_with_an_outer_binding` | determinism / state |
| `an_integral_counter_renders_without_a_fraction` | numeric_boundary |

**Mutation-checked.** Reverting `range_has_next` to `current <= end` and making
`expect_range_number` accept text failed exactly
`edge_a_descending_step_counts_down`,
`edge_a_descending_step_that_starts_below_its_end_is_empty`,
`edge_a_non_number_from_…`, `edge_a_non_number_to_…` and
`edge_a_non_number_by_…` — 5 tests, then reverted. The suite bites.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, no warnings |
| `cargo test --all-targets` | pass — **557 passed, 0 failed, 0 ignored** across 26 test binaries. Baseline on stashed `main` was **526**; +31 is exactly the 31 new tests in `tests/for_range_test.rs`, so nothing pre-existing was added or lost |
| `cargo test --doc` | pass, 1 passed |
| `./rbops/verify.sh phase-028` | **not run — `rbops/` is not in this checkout.** See below. |
| every file in `examples/` and `modules/` via `rb run` | pass — 8/8 exit 0. `MathUtils.rb` passes too, so no exception was needed. |

`ls rbops` → `No such file or directory`. The pipeline that dispatched this
phase lives elsewhere and was not inspected. As the closest honest substitute I
ran what `.github/workflows/ci.yml` runs (`cargo build`, `cargo test`,
`cargo clippy -- -D warnings`) — all pass — plus the examples/modules sweep the
project contract names, plus the two documented examples run verbatim:

```
$ ./target/debug/rb run spec_example.rb   # SPEC.md:383-391, verbatim
1 2 3 4 5 6 7 8 9 10 0 5 10 15 20 25 30 35 40 45 50 55 60 65 70 75 80 85 90 95 100
$ ./target/debug/rb run readme_example.rb  # README.md:101-103, verbatim
1 2 3 4 5
```

This gate row is the one claim in this report I could not execute. The phase is
not `.done` until `rbops/verify.sh phase-028` is run by the pipeline.

## Definition of done

| Requirement | Status | Evidence |
|---|---|---|
| `for each i from 1 to 5` exits 0, prints 1 2 3 4 5 once each in order | met | `a_range_loop_visits_every_value_in_order`; `exit=0` from the binary |
| `from 0 to 10 by 5` visits 0, 5, 10 and nothing else; `by 1` ≡ omitted `by` | met | `a_step_of_five_visits_only_the_multiples_of_five`, `a_step_of_one_and_an_omitted_step_are_the_same_loop` |
| the loop variable is a local; out of scope after `end` is an `AnalyzerError` | met | `the_loop_variable_is_a_local_that_does_not_leak`, `edge_the_loop_variable_is_out_of_scope_after_end` |
| `edge_*` for empty range, singleton, descending `by`, fractional `by`, non-number `from`/`to`/`by` | met | five `edge_*` tests, one per clause |
| `edge_*` asserting a FAILURE for `by 0`: `RuntimeError` naming `MAX_ITERATIONS`, no hang, no abort | met | `edge_a_zero_step_stops_at_the_iteration_guard_rather_than_hanging`, exact message |
| the existing guard is reused, not bypassed: an over-long range gives the same error `repeat` gives | met | `edge_a_range_longer_than_the_iteration_limit_stops_like_a_repeat_does`, plus `edge_a_range_at_exactly_the_iteration_limit_still_finishes` for the off-by-one |
| `cargo test --all-targets` 0 failures; `examples/` and `modules/` all exit 0 | met | see Gates |

## Mandatory edge-case matrix

| Row | Status |
|---|---|
| empty / zero / "nothing" | covered — `edge_an_empty_range_visits_nothing_and_does_not_hang`, `edge_a_descending_step_that_starts_below_its_end_is_empty`, `edge_a_zero_step_…`, and `by nothing` in `edge_a_list_as_a_bound_is_refused_rather_than_skipped` |
| singleton and boundary | covered — `edge_a_range_of_one_visits_exactly_one_value` (`from 3 to 3`, inclusive at both ends), `a_step_of_five_…`, `a_step_of_one_and_an_omitted_step_…`, `edge_a_range_at_exactly_the_iteration_limit_still_finishes` (cap vs cap+1) |
| out of bounds | N/A as an index — a range has no index. The analogous bound (a counter past `end`) is covered: `a_step_that_does_not_land_on_the_end_still_stops_at_or_before_it`, `edge_a_fractional_step_that_would_overshoot_still_terminates`. Index bounds are phase-026/earlier phases' `tests/index_bounds_test.rs` |
| type mismatch | covered — `edge_a_non_number_from_…`, `edge_a_non_number_to_…`, `edge_a_non_number_by_…`, `edge_a_list_as_a_bound_is_refused_rather_than_skipped` (text, list, record, `nothing`) |
| type coercion boundaries | covered — `edge_a_fractional_step_on_a_whole_number_range`, `edge_a_fractional_step_that_would_overshoot_still_terminates`, `edge_a_step_that_overflows_the_counter_is_a_runtime_error` (`1e308` + `1e308`), `edge_the_loop_variable_is_out_of_scope_after_end`. `-0.0` is `0.0` in `f64`, so `by -0.0` takes the up branch; `NaN`/`±Infinity` cannot enter `Value::Number` at all (`Value::number`, `src/value.rs:221`) and the counter goes through `finite_number` |
| unicode / escapes | N/A — no text crosses the range machinery. `from`/`to`/`by` take numbers, so the unicode rows (empty text, `"`, `\`, newline, emoji/CJK/RTL, combining marks, very long strings) cannot fail or pass differently here. The only text this change produces is the 4 ASCII tokens `for each … from … to … by`, asserted byte-for-byte in `the_formatter_prints_a_range_loop_and_the_result_still_runs`. `tests/lexer_robustness_test.rs` and `tests/test_text.rb` own the unicode row |
| nesting / recursion | covered — `a_range_loop_nests_and_the_inner_variable_is_its_own`. Mutual recursion through a range loop is a function-call concern owned by `tests/call_depth_test.rs` (`MAX_CALL_DEPTH`) |
| duplicate / missing keys | N/A — a range loop has no record literal and reads no field. Record keys are `tests/record_order_test.rs`; the nearest thing here is a missing *name* (`to nope`), covered by `an_unanalysable_range_bound_is_reported_by_the_analyzer` |
| malformed input | covered — `edge_a_step_missing_its_end_of_the_range_is_a_parser_error`, `edge_a_by_before_the_to_is_a_parser_error`, `edge_a_for_each_with_neither_in_nor_from_is_a_parser_error`, `the_formatter_prints_a_range_loop_and_the_result_still_runs`. An empty file, a BOM and CRLF are lexer-level and unchanged by this phase (`tests/lexer_robustness_test.rs`); an unclosed `end` cannot reach a range loop's body loop, which stops at `Eof` and then fails `expect(End)` (`src/parser.rs` `parse_loop_body`) |
| resource / state | covered — `edge_a_zero_step_…`, `edge_a_range_longer_than_the_iteration_limit_stops_like_a_repeat_does`, `edge_a_range_at_exactly_the_iteration_limit_still_finishes`, `edge_a_step_that_overflows_the_counter_is_a_runtime_error`, plus the scope pair `the_loop_variable_is_a_local_that_does_not_leak` / `edge_the_loop_variable_is_out_of_scope_after_end` |

## Invariants touched

- None of the language invariants in `AGENTS.md` §2. `.rb`, `to … end`, `set x
  to <expr>`, `say`, the `Value` variants and the `Error` variants are all
  unchanged. The range loop the grammar already documented now runs; nothing new
  was added to the language's surface.
- One invariant was **deliberately not** taken: `by` was **not** added to
  `KEYWORDS`. Reserving it would have refused `to can grow(by)` / `say by`
  (`tests/bytecode_test.rs:602`), a parameter name the language has always
  allowed, and phase rules forbid weakening that test. `by` is read positionally
  in the one place that means "step" (`Parser::at_range_step_marker`,
  `src/parser.rs:759`) and nothing else changed.
- Two **behaviour changes to previously-unreachable code**, both required by the
  phase's own definition of done and both about code no source could reach
  before:
  - a non-numeric bound was a silent zero-iteration loop and is now a
    `RuntimeError` naming the argument;
  - a negative step could never advance the counter and visited nothing; the
    step's sign is now the direction.

## Notes for the reviewer

- **Why both VMs changed.** `src/bytecode/vm.rs` had a working
  `Sequence::Range` all along, and `tests/bytecode_vm_test.rs` already carried
  four range corpus entries (`shape/range-with-a-step`, `shape/range-backwards`,
  `shape/range-with-non-numeric-bounds`, `singleton/range-of-one`) that
  `compile_source` was skipping with `let Ok(chunk) = … else { continue }`
  because the shared parser refused them. They now compile and run, so the
  differential test compares the two VMs on range loops for the first time.
  Leaving the bytecode VM alone would have made that comparison fail, so both
  use `expect_range_number` and `range_has_next` from `src/value.rs` — one
  definition, two callers.
- **Zero step is not special-cased.** `range_has_next` does not look for
  `step == 0.0`; `by 0` simply never leaves `start` and is stopped by
  `charge_iteration` (`src/vm.rs:333`), the guard `repeat` and `while` share.
  That is what the phase required, and it means there is no second way for a
  range loop to escape.
- **No float comparison was weakened.** `range_has_next` is still a plain `<=`
  / `>=` on `f64`; an epsilon was deliberately not introduced, because it would
  let a range stop one or more values early without saying so. A fractional
  step that accumulates past its end therefore ends the loop early rather than
  hanging — asserted in
  `edge_a_fractional_step_that_would_overshoot_still_terminates`.
- **`src/runtime.rs` shrank by 11 lines.** The `type_of` builtin had its own
  copy of the type-name table; it now calls `Value::type_name`. Output is
  byte-identical for every variant and for no argument (`"nothing"`), and
  `tests/constant_test.rs` plus the `type_of` cases in `tests/` cover it. This
  is deduplication forced by needing the same names in an error message, not a
  refactor smuggled in.
- **`README.md` was not touched.** Its `for each number from 1 to 5` example is
  now true (run above), so there is nothing to correct. `ROADMAP.md` was not
  touched. `SPEC.md` gained only the two rules this phase introduces.

## Known gaps / follow-ups

- `break` and `skip` are silent no-ops in **every** loop form, including the
  range loop. Pre-existing, verified on `main`, reproduced in `FINDINGS.md` §1
  as MAJOR. Not fixed here: it needs a signal out of the loop body in both VMs,
  which is a phase of its own.
- `by` gets no editor highlighting, because it is deliberately not a keyword.
  `FINDINGS.md` §2.
- The analyzer reports an out-of-scope read on the line *after* it.
  `FINDINGS.md` §3.