# Phase 038 — Implement `might fail`, the documented non-aborting form of a fallible call

## What changed

Resumed round: the tree arrived with the form implemented but **red** — a new public
`Error::Limit` variant had been added, which broke two pre-existing tests in
`tests/loop_control_test.rs` and violated the `Error` invariant in `AGENTS.md` § 2.
This round removed that variant and kept the behaviour it was carrying, using the
counting idiom the guards already use for `expect` results.

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +67 −0 | `Expr::MightFail(Box<Expr>)`; `parse_might_fail()` parses the prefix, `might fail` at expression and statement start; **both** words are required — a lone `might` or `fail` is a spanned `ParserError`; only a `Call`/`MethodCall` is accepted, anything else is a spanned `ParserError` |
| `src/interpreter.rs` | +102 −9 | tree-walking VM: `Expr::MightFail` discards the failure and yields `nothing`, re-raising a loop-control signal or a failed `expect`; `Vm::limits_hit` counts the resource limits reached and `Vm::limit_reached()` raises all three of them |
| `src/bytecode/opcode.rs` | +24 −0 | `Opcode::MightFail` (byte 49) and `MIGHT_FAIL_END_MARKER` (`u32::MAX - 2`), the third reserved `Nop` operand |
| `src/bytecode/codegen.rs` | +22 −1 | guard → call → end marker → jump over recovery → recovery (`PushConst Nothing`), target patched after the region |
| `src/bytecode/vm.rs` | +343 −50 | `Guard` stack; `push_guard`, `pop_guard`, `handle_guard`, `inner_most_guard`, `guard_is_inner`; asked at the top of every `handle_failure` turn — *inside* the loop, so a `try` with no `catch` that passes the failure on does not skip the guard; the recovery instruction alone produces the expression's value; guards dropped with the frame they name; `BytecodeVm::limits_hit` / `Guard::limits_before` decide the same question `Vm::limits_hit` decides on the tree-walking side |
| `src/error.rs` | **+0 −37** | **`Error::Limit` and `Error::is_resource_limit()` removed.** The enum is byte-identical to its pre-phase form: `Error::{Lexer,Parser,Analyzer,Runtime,Io}` (`AGENTS.md` § 2) |
| `src/bootstrap.rs` | +0 −2 | the two arms added for the removed variant, gone with it |
| `src/analyzer.rs`, `src/linter.rs`, `src/formatter.rs` | +8 −0 | `Expr::MightFail` arm so the guarded expression is still analyzed; the formatter prints `might fail <call>` rather than the bare call it wraps |
| `src/bytecode/format.rs` | +8 −1 | `FORMAT_VERSION` 5 → 6 |
| `src/bytecode/disasm.rs`, `src/bytecode/mod.rs` | +5 −0 | `end of a 'might fail' region` on a marked `Nop`; re-export `MIGHT_FAIL_END_MARKER` |
| `bootstrap/compiler.rb` | +1 −1 | format version word `5` → `6`, so the Redblue compiler writes the same bytes |
| `docs/BYTECODE.md`, `docs/GRAMMAR.md`, `SPEC.md` | +116 −7 | opcode 49, the third marker, the version-6 row, the recovery-offset bound, what a guard does not discard; grammar § 5.11 corrected to `'might fail' call`; SPEC § Error Handling gains the form, where it may appear, and the three things a guard does not discard |
| `tests/bytecode_test.rs` | +9 −2 | byte-49 and table-end assertions follow the new last opcode; the version assertion was already `FORMAT_VERSION` rather than a literal |
| `tests/might_fail_test.rs` | +1125 −0 | new file, 43 tests, every one run through both engines |

Nothing in `phases/INVARIANTS.md` was touched: `.rb` is unchanged, no braces were
introduced, `set x to …`, `say`, the `Value` variants, the `Error` variants and the
parser's `end` terminators are as they were. `might` and `fail` were already reserved
by the lexer before this phase (`src/lexer.rs:39-40`), so no program that compiled
before stops compiling.

## Tests added

`tests/might_fail_test.rs` — 43 `#[test]` functions, 23 of them `edge_*`, each
executed on the tree-walking VM and on the bytecode VM with the two outcomes
required to agree (`both_print` / `both_value` / `both_fail`).

| Test | Edge class covered |
|---|---|
| `failing_call_is_discarded_and_the_program_survives` | the reported finding: a failing `files.read` no longer ends the program |
| `a_failing_call_yields_nothing_rather_than_aborting` | failure path produces `nothing`, not an abort |
| `a_succeeding_call_yields_its_value_not_nothing` | the form is not only a failure path — a real file's text comes back |
| `the_statement_form_writes_a_file_it_is_allowed_not_to` | `might fail` in statement position, unguarded side effect |
| `inside_a_for_each_body_the_loop_continues` | every iteration prints after a discarded failure |
| `nested_guards_are_each_discardable` | nesting_recursion — inner guard recovers, outer one is not double-charged |
| `edge_a_failure_crossing_three_frames_reaches_the_guard` | nesting_recursion — unwinding out of calls reaches the guard |
| `edge_a_guarded_call_that_would_recurse_without_limit_stops_at_the_limit` | resource_limit — the guard is not an escape hatch from the call-depth limit, **and the limit is reported rather than discarded** |
| `edge_a_step_budget_spent_inside_the_guarded_call_is_reported` | resource_limit — the budget, with both engines' limits lowered so the exhaustion lands inside the guard |
| `edge_an_iteration_cap_reached_after_the_guarded_call_is_reported` | resource_limit — the third limit, after the guarded call |
| `edge_a_guard_inside_a_loop_does_not_survive_its_own_iteration` | resource_limit — the guard does not leak into the next iteration |
| `edge_an_out_of_bounds_index_inside_the_guarded_call_is_discarded` | out_of_bounds — a clean runtime error, discarded |
| `edge_division_by_zero_inside_the_guarded_call_is_discarded` | numeric_boundary — `1/0` |
| `edge_a_type_mismatch_inside_the_guarded_call_is_discarded` | type_mismatch |
| `edge_a_missing_record_field_inside_the_guarded_call_is_discarded` | duplicate_missing_keys — absent field |
| `edge_the_boundaries_of_a_singleton_reach_the_guard_intact` | singleton — index `0` and `len-1` of a one-element list |
| `edge_empty_argument_list_is_still_a_call` | empty — `now()` with no arguments is still a call, so it is guarded |
| `edge_unicode_survives_a_guarded_call_that_succeeds` | unicode — `héllo 🌍 日本語` through a guarded call |
| `edge_non_call_right_hand_side_is_a_spanned_parser_error` | malformed_input — **asserts a failure**: `might fail 1 + 1` is a `ParserError` |
| `edge_a_bare_number_after_might_fail_is_refused_too` | malformed_input — asserts a failure |
| `might_fail_without_an_argument_is_refused` | malformed_input — asserts a failure |
| `edge_a_lone_might_prefix_is_refused` | malformed_input — `might upper("hi")` is a `ParserError`, not a guard |
| `edge_a_lone_fail_prefix_is_refused` | malformed_input — `fail upper("hi")` is a `ParserError` too |
| `edge_a_file_naming_recovery_outside_its_block_is_refused_not_trusted` | malformed_input — a hand-edited recovery offset is refused, not indexed |
| `edge_a_region_marker_with_no_guard_does_nothing` | malformed_input — a marker with no guard is data, not a crash |
| `edge_a_guard_does_not_swallow_a_failed_expect` | the guard cannot report a red test green |
| `edge_a_guard_inside_a_try_does_not_eat_the_try` | guard/handler ordering |
| `a_guard_does_not_stop_a_later_failure_from_being_reported` | a guard does not outlive its expression |
| `a_guarded_call_is_an_expression_wherever_a_value_is_taken` | nesting_recursion — a guarded call as a list element |
| `edge_a_guarded_call_as_a_record_value_does_not_disturb_its_neighbours` | the operand-stack leak: keys and values are read positionally, so a second `nothing` would shift every pair after it |
| `a_guard_leaves_one_value_on_the_operand_stack` | 500 guarded calls building one record — a per-call stray grows the stack without bound |
| `edge_a_try_with_no_catch_inside_a_guarded_call_still_lets_the_guard_take_it` | guard/handler ordering — a `try`/`finally` with no `catch` runs its `finally`, passes the failure on, and the guard is asked again |
| `a_guarded_call_that_raises_inside_a_try_still_lets_the_catch_see_later_failures` | guard/handler ordering, three levels |
| `a_user_function_that_fails_is_discarded_by_the_call_site` / `a_user_function_that_succeeds_is_not_discarded` | user-defined calls, both paths |
| `edge_a_might_fail_whose_argument_itself_raises_is_discarded` | the argument raising before the call is reached |

Test quota: 43 `#[test]` functions (floor 3), 23 named `edge_*` (floor 3), 11
asserting a produced failure. No `#[ignore]`, no `// skip`, no `allow(clippy::)`.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — 0 warnings |
| `cargo test --all-targets` | **1147 passed, 0 failed, 0 ignored** |
| `cargo test --test might_fail_test` | 43 passed, 0 failed, 0 ignored |
| `cargo test` (includes doc tests) | **1149 passed, 0 failed, 0 ignored** (1147 + 2 doc tests) |
| `./rbops/verify.sh phase-038` | **not run — `rbops/` is not present in this checkout.** `ls rbops` returns `No such file or directory`; the pipeline that invokes this phase lives outside the project root, which this phase is forbidden to inspect. Every other gate above was run and is green. This row is reported honestly rather than claimed as a pass. |

The tree as inherited was red: `cargo test --all-targets` reported
`edge_a_jump_raised_in_a_cleanup_is_charged_one_turn` and
`edge_a_skip_is_charged_as_one_iteration` FAILED in `tests/loop_control_test.rs`
(both assert `Error::Runtime` for the iteration cap and were handed `Error::Limit`).
Both are green now, from the same `tests/loop_control_test.rs` as `main` carries it.

## Definition of done, verified by hand

Run from `./target/tmp/p038/` against `target/debug/rb`.

| Item | Evidence |
|---|---|
| failing read → `nothing`, exit 0, on both engines | `set data to might fail files.read("/nope/does-not-exist")` / `say data` / `say "alive"` → `rb run`: `nothing`, `alive`, exit 0. `rb compile` → `rb vm vfail.rbc`: `nothing`, `alive`, exit 0. |
| existing file → its text, on both engines | same program against a file containing `hello from disk` → `rb run` and `rb vm` both print `hello from disk`, exit 0. |
| `for each` body continues | two-iteration loop, guarded read failing on both paths → `nothing`, `nothing`, `done`, exit 0 on both engines. |
| `might fail 1 + 1` is a spanned `ParserError` | ``Error: ParserError: `might fail` must be followed by a call, which this is not`` / `--> verr.rb:1:12` with the caret under the operand, exit 1. Not a silent no-op, not an abort. |
| a resource limit is still reported, not discarded | `set stopped to might fail endless(1)` → `RuntimeError: Maximum call depth of 1000 reached while calling 'endless'`, exit 1, on both engines. `REDBLUE_MAX_STEPS=6` against a guarded call whose body spends the budget → `RuntimeError: Step budget of 6 reached before the program finished`, exit 1, on both engines. `REDBLUE_MAX_ITERATIONS=3` in the library test on the same source. |
| `examples/*.rb` and `modules/*.rb` still run | all 8 files (6 in `examples/`, 2 in `modules/`) exit 0 under `rb run`. |

## Invariants touched

- `might` and `fail` were already reserved keywords in `src/lexer.rs:39-40` before
  this phase, so turning `MightFail` from an unused token into a parsed prefix
  removes no identifier that a program could previously use.
- `Expr` gains a variant. `redblue::Value`, `redblue::Error` and the `.rb` extension
  are untouched: `Error` carries exactly the five variants it carried before this
  phase, and the two tests in `tests/loop_control_test.rs` that assert
  `matches!(error, Error::Runtime(..))` for the three limits pass unmodified.
- `.rbc` format version 5 → 6. Version 6 adds an opcode at byte 49, which no earlier
  version used, so the numbers already in old files keep their meaning; older files
  are refused by the version word rather than half-read. This is the bump
  `docs/BYTECODE.md` already documents for versions 3, 4 and 5.
- `bootstrap/compiler.rb` writes the same version word, so the self-hosting
  fixed-point tests are unaffected and remain green.

## Why a limit is counted rather than a variant

The previous round added `Error::Limit(String, Span)` so a guard could ask a failure
*what kind* it was. That is the right question and the wrong carrier:

- `AGENTS.md` § 2 fixes the `Error` variants as public API, and § 1 forbids
  re-scoping existing tests. The variant forced four pre-existing test files
  (`call_depth_test.rs`, `for_range_test.rs`, `function_literal_test.rs`,
  `loop_bounds_test.rs`) to stop matching on `Error::Runtime` for the three limits,
  and left two assertions in `loop_control_test.rs` unmatched — red.
- The guards already answer a question of exactly this shape for `expect`: a count
  that went up across the guarded region means the region recorded a new result
  (`Vm::assertions_failed` / `Guard::assertions_before`). A resource limit is
  answered the same way — `Vm::limits_hit` / `Guard::limits_before`, both bumped by
  `Vm::limit_reached()` / `BytecodeVm::limit_reached()`, the single helper all three
  limits raise through. A limit is still a `RuntimeError` and reads as one; nothing
  has to agree on how its message is spelled for it to be told apart, which is what
  the variant was for and what the count does without touching the enum.
- Both engines ask the same question of the same state, and the three limit tests in
  `tests/might_fail_test.rs` — which assert `label() == "RuntimeError"` and the
  message text, never the variant — were unaffected by the removal and pass.

## Known gaps / follow-ups

- `might fail` guards a `Call` or `MethodCall` only. `might fail x[1]` and
  `might fail a + b` are refused at parse time even though indexing can fail;
  widening it is a separate change with its own design question. `SPEC.md` and
  `docs/GRAMMAR.md` state the restriction instead of contradicting it.
- `might fail` discards the failure rather than reporting why. The shape carries no
  reason today; if one is wanted it needs a decision about what `might fail`
  evaluates to (`nothing` vs. a record), which the definition-of-done fixes as
  `nothing`.
- A guard passes a failure on if a limit was reached *anywhere inside* its region
  since the guard was installed, including a limit an inner `try` caught and
  recovered from before the region's own failure. That errs towards reporting a
  failure rather than discarding it, and both engines do it identically; the
  per-failure marker that would separate the two needs a failure type this phase
  may not add. Reachable only by catching a limit inside a guarded call and then
  failing the same call.
- `rbops/verify.sh` could not be executed here — see the Gates table.