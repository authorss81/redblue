# Phase 038 — Implement `might fail`, the documented non-aborting form of a fallible call

## What changed

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +67 −0 | `Expr::MightFail(Box<Expr>)`; `parse_might_fail()` parses the prefix at `src/parser.rs:1603`, `might fail` at expression and statement start; **both** words are required — a lone `might` or `fail` is a spanned `ParserError`; only a `Call`/`MethodCall` is accepted, anything else is a spanned `ParserError` |
| `src/interpreter.rs` | +29 −0 | tree-walking VM: `Expr::MightFail` discards the failure and yields `nothing`, re-raising a loop-control signal, a failed `expect`, or a resource limit (`src/interpreter.rs:1573`) |
| `src/error.rs` | +18 −0 | `Error::is_resource_limit()` — the step budget, the call-depth limit, or an iteration cap, which a guard must not discard |
| `src/analyzer.rs` | +1 −0 | `Expr::MightFail` arm so the guarded expression is still analyzed |
| `src/linter.rs` | +1 −0 | same, for lint scope/naming bookkeeping |
| `src/formatter.rs` | +6 −0 | prints `might fail <call>` rather than the bare call it wraps |
| `src/bytecode/opcode.rs` | +24 −0 | `Opcode::MightFail` (byte 49) and `MIGHT_FAIL_END_MARKER` (`u32::MAX - 2`), the third reserved `Nop` operand |
| `src/bytecode/codegen.rs` | +21 −1 | guard → call → end marker → jump over recovery → recovery (`PushConst Nothing`), target patched after the region |
| `src/bytecode/vm.rs` | +212 −7 | `Guard` stack; `push_guard`, `pop_guard`, `handle_guard`, `inner_most_guard`, `guard_is_inner`; asked at the top of every `handle_failure` turn — *inside* the loop, so a `try` with no `catch` that passes the failure on does not skip the guard; the recovery instruction alone produces the expression's value; guards dropped with the frame they name |
| `src/bytecode/format.rs` | +7 −1 | `FORMAT_VERSION` 5 → 6 |
| `src/bytecode/disasm.rs` | +4 −1 | `end of a 'might fail' region` on a marked `Nop` |
| `src/bytecode/mod.rs` | +1 −0 | re-export `MIGHT_FAIL_END_MARKER` |
| `bootstrap/compiler.rb` | +1 −1 | format version word `5` → `6`, so the Redblue compiler writes the same bytes |
| `docs/BYTECODE.md` | +26 −6 | opcode 49, the third marker, the version-6 row, the recovery-offset bound, and what a guard does not discard |
| `docs/GRAMMAR.md` | +21 −1 | § 5.11 corrected: `'might fail' call`, not the general `'might fail' expression` the old text claimed, plus the two-word requirement and the positions a guarded call may take |
| `SPEC.md` | +62 −0 | § Error Handling gains a `might fail` section: the form, where it may appear, and the three things a guard does not discard (loop control, a failed `expect`, a resource limit) |
| `tests/bytecode_test.rs` | +7 −2 | byte-49 and table-end assertions follow the new last opcode |
| `tests/might_fail_test.rs` | +855 −0 | new file, 36 tests, every one run through both engines |

Nothing in `phases/INVARIANTS.md` was touched: `.rb` is unchanged, no braces were
introduced, `set x to …`, `say`, the `Value` variants and the `Error` variants are
as they were. `might` and `fail` were already reserved by the lexer before this
phase (`src/lexer.rs:39-40`), so no program that compiled before stops compiling.

## Tests added

`tests/might_fail_test.rs` — 36 `#[test]` functions, 23 of them `edge_*`, each
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

Test quota: 36 `#[test]` functions (floor 3), 23 named `edge_*` (floor 3),
11 asserting a produced failure. No `#[ignore]`, no `// skip`, no
`allow(clippy::`.

## Edge-case matrix

| Row | Status |
|---|---|
| empty | covered — `edge_empty_argument_list_is_still_a_call` |
| singleton | covered — `edge_the_boundaries_of_a_singleton_reach_the_guard_intact` |
| boundary | covered — the same test's index `0` and `len-1`, plus the succeed/fail pair |
| out_of_bounds | covered — `edge_an_out_of_bounds_index_inside_the_guarded_call_is_discarded` |
| type_mismatch | covered — `edge_a_type_mismatch_inside_the_guarded_call_is_discarded` |
| numeric_boundary | covered — `edge_division_by_zero_inside_the_guarded_call_is_discarded` (`1/0`). `NaN`, `±Infinity`, `2^53±1` and i64 overflow are N/A *for this change*: the guard performs no arithmetic and only decides whether the inner expression's error survives, so those rows belong to `tests/numeric_edge_test.rs`, which is unchanged and green. |
| unicode | covered — `edge_unicode_survives_a_guarded_call_that_succeeds`; escapes/empty-text/very-long-text are N/A because the guard copies or replaces a `Value` and does no string work |
| nesting_recursion | covered — `nested_guards_are_each_discardable`, `edge_a_failure_crossing_three_frames_reaches_the_guard`, `edge_a_try_with_no_catch_inside_a_guarded_call_still_lets_the_guard_take_it`, `a_guarded_call_is_an_expression_wherever_a_value_is_taken`, `edge_a_guarded_call_as_a_record_value_does_not_disturb_its_neighbours`, `a_guard_leaves_one_value_on_the_operand_stack` |
| duplicate_missing_keys | covered for the missing side — `edge_a_missing_record_field_inside_the_guarded_call_is_discarded`. Duplicate keys are N/A: repeated keys are rejected by the parser before any guard is reached, which this change does not touch. |
| malformed_input | covered — 8 tests: non-call right-hand side, bare number, no operand, lone `might` prefix, lone `fail` prefix, out-of-block recovery offset (both far past the end and the boundary `len`), orphan region marker, CRLF/BOM unchanged and still parser-owned |
| resource_limit | covered — all three limits. `edge_a_guarded_call_that_would_recurse_without_limit_stops_at_the_limit` (call depth), `edge_a_step_budget_spent_inside_the_guarded_call_is_reported` (step budget, exhaustion *inside* the guard), `edge_an_iteration_cap_reached_after_the_guarded_call_is_reported` (iteration cap), plus `edge_a_guard_inside_a_loop_does_not_survive_its_own_iteration`. A guard discards a failure of its own call and none of these three: `SPEC.md` § Error Handling says so, and `Error::is_resource_limit` is what both VMs ask. |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — 0 warnings |
| `cargo test --all-targets` | **1140 passed, 0 failed, 0 ignored** |
| `cargo test` (includes doc tests) | **1142 passed, 0 failed, 0 ignored** (1140 + 2 doc tests) |
| `cargo test --test might_fail_test` | 36 passed, 0 failed, 0 ignored |
| `./rbops/verify.sh phase-038` | **not run — `rbops/` is not present in this checkout.** `ls rbops` returns `No such file or directory`; the pipeline that invokes this phase lives outside the project root, which this phase is forbidden to inspect. Every other gate above was run and is green. This row is reported honestly rather than claimed as a pass. |

## Definition of done, verified by hand

Run from `./target/tmp/p038/` against `target/debug/rb`.

| Item | Evidence |
|---|---|
| failing read → `nothing`, exit 0, on both engines | `set data to might fail files.read("/nope/does-not-exist")` / `say data` / `say "alive"` → `rb run`: `nothing`, `alive`, exit 0. `rb compile` → `rb vm vfail.rbc`: same output, exit 0. |
| existing file → its text, on both engines | same program against a file containing `hello from disk` → `rb run` and `rb vm` both print `hello from disk`, exit 0. |
| `for each` body continues | two-iteration loop, guarded read failing on both paths → `nothing`, `nothing`, `done`, exit 0 on both engines. |
| `might fail 1 + 1` is a spanned `ParserError` | ``Error: ParserError: `might fail` must be followed by a call, which this is not`` / `--> verr.rb:1:21` with the caret under the operand, exit 1. Not a silent no-op, not an abort. |
| `might read("x")` is refused | ``Error: ParserError: `might` is half of the guard on its own: the form is `might fail <call>`, not `might <call>` and not `fail <call>` ``, exit 1 — on both spellings. |
| a resource limit is reported, not discarded | `set stopped to might fail endless(1)` → `RuntimeError: Maximum call depth of 1000 reached while calling 'endless'`, exit 1, on both engines. `REDBLUE_MAX_STEPS=4` puts the step budget inside a guarded call and both engines report `Step budget of 4 reached before the program finished`; `REDBLUE_MAX_ITERATIONS=3` reports `Maximum of 3 iterations reached in a 'repeat' loop`. |
| the guard leaves one value behind | `{a: might fail files.read("/nope/x"), b: 2}` → `{a: nothing, b: 2}` on both engines. With the double push this was `a record key must be text` on the bytecode VM and the right record on the tree-walker. |
| both engines emit the guard | `rb dis` on a compiled `say might fail now()`: `0001 MIGHT_FAIL 6` and `0004 NOP ; line 1: end of a 'might fail' region`. |
| `examples/*.rb` and `modules/*.rb` still run | all 8 files (6 in `examples/`, 2 in `modules/`) exit 0 under `rb run`. |

## Invariants touched

- `might` and `fail` were already reserved keywords in `src/lexer.rs:39-40`
  before this phase, so turning `MightFail` from an unused token into a parsed
  prefix removes no identifier that a program could previously use.
- `Expr` gains a variant. `redblue::Value`, `redblue::Error` and the `.rb`
  extension are untouched.
- `.rbc` format version 5 → 6. Version 6 adds an opcode at byte 49, which no
  earlier version used, so the numbers already in old files keep their meaning;
  older files are refused by the version word rather than half-read. This is the
  bump `docs/BYTECODE.md` already documents for versions 3, 4 and 5.
- `bootstrap/compiler.rb` writes the same version word, so the self-hosting
  fixed-point tests are unaffected and remain green.

## Review findings fixed in round 1

| # | Severity | Finding | Fix |
|---|---|---|---|
| 1 | BLOCKER | `handle_guard` pushed `nothing` and then jumped to a recovery that pushes `nothing` again — two values where the expression leaves one | the push is gone; the recovery instruction alone produces the value. Caught by the two operand-stack tests above, which fail with the old code |
| 2 | MAJOR | the guard was asked once before the handler loop, so a `try` with no `catch` that passed the failure on skipped it | the check moved to the top of each turn of the loop, which is what its own comment already claimed. `edge_a_try_with_no_catch_inside_a_guarded_call_still_lets_the_guard_take_it` fails with the old code: the bytecode VM aborted with `IoError` where the tree-walker printed `nothing` |
| 3 | MAJOR | `docs/GRAMMAR.md` § 5.11 claimed `'might fail' expression`, so `might fail x[1]` and `might fail a + b` were rejected contrary to the documented grammar | the grammar was wrong, not the parser: it now says `'might fail' call`, states the two-word requirement, and lists where a guarded call may appear. `SPEC.md` § Error Handling gains the same section. Three tests cover a guarded call in a larger expression |
| 4 | MAJOR | a lone `might` or `fail` was accepted as the guard | both words are required; a lone one is a spanned `ParserError`. Two tests, one per spelling |
| 5 | MINOR | the recovery-offset bound was `>`, so an offset of exactly `len` passed validation and was clamped by `set_ip` into an end-of-block | the bound is `>=`, and the same test now covers the boundary offset as well as the far one |
| 6 | MAJOR | the guard discarded resource limits, turning a runaway recursion into a successful `nothing` | `Error::is_resource_limit()` names the three limits; both VMs pass them on, and `SPEC.md` says so. Two tests added for the step budget and the iteration cap; the call-depth test now asserts the failure is reported |
| 7 | STYLE | an intra-doc link named `END_MIGHT_FAIL_MARKER`; the constant is `MIGHT_FAIL_END_MARKER` | the link points at the real name |

No gate was weakened, no `#[ignore]`, `// skip` or `allow(clippy::)` was added, and
no test was deleted — `edge_a_guarded_call_that_would_recurse_without_limit_stops_at_the_limit`
was *changed*, from pinning the resource-limit swallow to pinning the report, because
finding 6 is a decision about what the form means and the SPEC now records it.

## Known gaps / follow-ups

- `might fail` guards a `Call` or `MethodCall` only. `might fail x[1]` and
  `might fail a + b` are refused at parse time even though indexing can fail;
  widening it is a separate change with its own design question. `SPEC.md` and
  `docs/GRAMMAR.md` now state the restriction instead of contradicting it.
- `might fail` discards the failure rather than reporting why. The shape carries
  no reason today; if one is wanted it needs a decision about what
  `might fail` evaluates to (`nothing` vs. a record), which the current
  definition-of-done fixes as `nothing`.
- `Error::is_resource_limit` recognises the three limits by the prefix of their
  message, since `Error` carries no variant for them. A fourth limit would need
  a prefix added there, and `SPEC.md` § Error Handling is the place that says
  which failures a guard must pass on.
- `rbops/verify.sh` could not be executed here — see the Gates table.