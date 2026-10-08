# Phase 034 — Add the function literal `to (x) ... end` as an expression

## Re-dispatch on 2026-10-08 — the finding is stale; no code changed

This phase was dispatched a second time with `HEAD = 03bfa06`, its own commit.
The finding no longer reproduces: `Expr::FunctionLiteral` is at
`src/parser.rs:67`, and both of the finding's repro programs exit 0 with the
documented output (`42`, and `[2, 4, 6]`). Per the phase prompt's instruction
for a stale finding, this run changed no production code, added no test and did
not edit `src/` — inventing a change to satisfy `must_touch` would be a
fabrication, and `must_touch` is satisfied by the phase's own commit `03bfa06`.
See `FINDINGS.md` § 0, which also asks the auditor why a completed phase was
re-selected.

The gates were re-measured on this `HEAD`, and the numbers differ from the
table below by two tests that were added after the original run wrote it:

| Gate | Result, re-measured 2026-10-08 at `03bfa06` |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff, exit 0 |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings, 0 errors, exit 0 |
| `cargo test --all-targets` | pass — 33 test binaries, **1008 passed, 0 failed, 0 ignored** (the table below says 1006; that count was taken before the last two tests landed) |
| `./rbops/verify.sh phase-034` | **not runnable** — `ls rbops` → `No such file or directory` in this checkout; the pipeline that owns it lives outside the project and was not inspected |

Also re-measured: `tests/closure_capture_test.rs` is 26 passed / 0 failed and
`tests/loop_bounds_test.rs` is 24 passed / 0 failed. Neither file is touched by
`03bfa06` (`git show 03bfa06 --stat -- tests/closure_capture_test.rs
tests/loop_bounds_test.rs` shows an empty diff), so both are unchanged. All 6
`examples/*.rb` and both `modules/*.rb` exit 0. Every `edge_*` test the
definition of done names is present in `tests/function_literal_test.rs` and
passes: zero parameters at line 267, parameter shadowing at 298, three nested
literals at 314, reassigned name at 361, wrong argument count at 413,
unterminated literal at 444, literal nested inside itself at 488.

## Reproducing the finding

Reproduced on `ca6bd7d` (phase-033) before any edit, both with the binary and
through the test harness:

```
$ printf 'set double to to (x) give back x * 2\nsay double(21)\n' > target/tmp_fnlit.rb
$ ./target/debug/rb run target/tmp_fnlit.rb; echo "EXIT=$?"
Error: ParserError: Unexpected token To
  --> target/tmp_fnlit.rb:1:15
1 | set double to to (x) give back x * 2
  |               ^
EXIT=1

$ printf 'say [1,2,3].map(to (x) give back x * 2)\n' > target/tmp_map.rb
$ ./target/debug/rb run target/tmp_map.rb; echo "EXIT=$?"
Error: ParserError: Unexpected token To
  --> target/tmp_map.rb:1:17
1 | say [1,2,3].map(to (x) give back x * 2)
  |                 ^
EXIT=1
```

The second test written, watched fail before any production code changed:

```
$ cargo test --test function_literal_test
test literal_is_an_expression_and_runs ... FAILED
test literal_runs_through_rb_run ... FAILED
---- literal_runs_through_rb_run stdout ----
assertion `left == right` failed: `set double to to (x) ... end` should run,
stderr was:
Error: ParserError: Unexpected token To
  --> …/target/tmp/function-literal/literal_prints_42.rb:1:15
test result: FAILED. 0 passed; 2 failed
```

## What changed

`git diff --numstat`, plus the new test file:

| File | Lines | What |
|---|---|---|
| src/parser.rs | +226 −3 | `Expr::FunctionLiteral { params, body }`; `parse_primary` takes a `to` as a literal (`To => self.parse_function_literal()`); `parse_function_literal` / `_inner` / `parse_literal_body` / `can_begin_statement` / `last_consumed_line`; the parameter list and the block body extracted into `parse_optional_params` / `parse_block_body`, which `parse_callable` now shares; `TokenKind::To` added to `is_expression_start`, so `give back to … end` is the literal `SPEC.md` § Closures writes; 2 unit tests |
| src/vm.rs | +63 −0 | the literal evaluates through the existing `make_function`, so it captures exactly where it was written; `ANONYMOUS_FUNCTION`; `map_builtin` / `apply_map` — the `map` builtin was registered but unimplemented, and a higher-order call needs the walker's captured scopes and call-depth budget, so it is resolved in `Vm::call` and in `call_method` for the `xs.map(f)` spelling |
| src/stdlib.rs | +8 −1 | `list_map` registered and `list` added to `MODULES`, so the `list.map(xs, f)` spelling `SPEC.md` § list writes resolves |
| src/analyzer.rs | +11 −0 | the literal's body is analyzed in a scope of its own, with its parameters declared, as a declaration's body is |
| src/linter.rs | +5 −0 | the literal's body goes through the existing `analyze_callable` |
| src/formatter.rs | +13 −0 | a literal is written as the block it is — `to (x)`, the body indented, `end` at the statement's indentation — and re-formats to itself |
| src/bytecode/codegen.rs | +13 −0 | `rb compile` refuses a literal with a spanned error naming `rb run`, rather than compiling it to something the bytecode VM would read differently from the walker. `expr` has no block pool to put a literal's block in; see FINDINGS #1 and the gap below |
| tests/function_literal_test.rs | +669 | 29 tests, 19 of them `edge_*` |
| SPEC.md | +15 −0 | § First-Class Functions says a literal is an expression, shows the block form beside the one-line form, and states the capture policy |
| docs/GRAMMAR.md | +17 −3 | § 6: the parameter list is optional, and `end` is optional only for a body written entirely on the `to`'s own line |

### Two forms, one meaning

`SPEC.md` and the finding both write the literal on a single line —
`to (x) give back x * 2` — with no `end`, while `docs/GRAMMAR.md` § 6 requires
one. Both work, and the boundary between them is what makes an unterminated
literal reportable:

- The parameters are followed by more of the body on the `to`'s own line → the
  body is what is on that line, and the line closes the literal.
- The parameters end the line → the body is a block, and its `end` is required.

A body that runs onto a second line without its `end` is therefore
`Expected 'end' to close the function literal` with a span, never a panic and
never a file silently swallowed to the end
(`edge_an_unterminated_literal_names_the_missing_end`,
`edge_a_multiline_body_without_its_end_fails_rather_than_eating_the_file`).

### Why `map` is in this diff

The definition of done requires "`rb run` of a file passing a literal to `map`
exits 0 and prints the mapped list". `map` was registered in `stdlib::builtins()`
and had no implementation — `say map([1,2,3], 2)` reported `Unknown function
'map'` on `ca6bd7d` — so a literal had nothing to be passed to. It is
implemented in the three spellings SPEC uses, and each is tested.

### What a global arity check would have cost

Calling a function with the wrong number of arguments binds the missing
parameters to `nothing` and drops the extra ones. A clean refusal is better, was
implemented, and fails three existing tests — `corpus/functions-0015.rb` is
`say wrong()` against `to wrong(n)` with a checked-in expectation of `nothing`.
Re-recording a corpus expectation is a gate-weakening move, so the check was
reverted. FINDINGS #5 has the detail.

## Tests added

| Test | Edge class covered |
|---|---|
| `literal_is_an_expression_and_runs` | the reproduction, in process |
| `literal_runs_through_rb_run` | the reproduction through the `rb` binary: exit 0, stdout `42` |
| `literal_captures_the_bindings_live_where_it_was_written` | capture by value at the point of writing, across a rebinding of the captured local |
| `edge_a_literal_at_the_top_level_reads_globals_at_call_time` | a global is not captured — the policy `tests/closure_capture_test.rs` states, held to by a literal too |
| `literal_inside_a_function_reads_that_functions_locals_not_the_callers` | escaped closure reads its own environment, not the caller's |
| `a_literal_behaves_exactly_as_a_nested_declaration` | the literal and the named declaration of the same body return the same value |
| `edge_a_literal_needs_no_parameters` | empty parameter list, both spellings (`to ()` and a bare `to`) |
| `edge_a_literal_with_an_empty_body_returns_nothing` | empty body — a value, not a parse error |
| `edge_a_parameter_shadows_an_enclosing_binding` | singleton parameter shadowing an enclosing name |
| `edge_three_nested_literals_each_capture_their_own_level` | nesting: four levels, each capturing its own |
| `edge_literals_sit_inside_a_list_and_a_record` | nesting: a literal as a list element and as a record field |
| `edge_a_literal_rebinding_its_capture_leaves_the_enclosing_scope_alone` | a value the literal assigns stays in its call |
| `edge_calling_map_without_a_function_fails` | **failure**: wrong argument count, named |
| `edge_calling_map_on_a_list_without_a_function_fails` | **failure**: wrong argument count through the method spelling |
| `edge_calling_a_literal_with_too_few_arguments_fails` | **failure**: a literal called with too few arguments |
| `edge_calling_a_literal_with_the_wrong_type_fails` | **failure**: type mismatch at the parameter |
| `edge_an_unterminated_literal_names_the_missing_end` | **failure**: malformed input — spanned `ParserError` naming `end`, asserted not to be a panic |
| `edge_a_multiline_body_without_its_end_fails_rather_than_eating_the_file` | **failure**: malformed input — the rest of the file is not swallowed |
| `edge_a_literal_nested_inside_itself_stops_at_the_call_depth_limit` | **failure**: a literal calling itself by name ends at the depth limit, not in an abort |
| `edge_nested_literals_past_the_block_budget_are_reported` | **failure**: resource limit — 65 nested literals is a diagnostic, run in a child process because the test thread is smaller than the one `rb` parses on (FINDINGS #1) |
| `edge_nested_literals_within_the_block_budget_run` | boundary: nesting within the budget runs |
| `a_literal_can_be_passed_to_map` | the SPEC `map` shape |
| `a_literal_maps_in_every_spelling` | `map(xs, f)`, `list.map(xs, f)` and `xs.map(f)` |
| `edge_map_over_an_empty_list_is_an_empty_list` | empty |
| `edge_map_over_a_singleton_is_a_singleton` | singleton |
| `edge_map_hands_each_element_to_the_literal` | unicode — emoji, CJK and combining-free accents through a literal's parameter, plus empty text |
| `a_literal_can_be_stored_in_a_record` | duplicate/missing keys analogue: a field read back out and called |
| `the_formatter_writes_a_literal_as_a_block_and_settles` | `rbfmt` output round-trips and is idempotent |
| `the_linter_does_not_report_a_literal_as_unused` | `rblint` reads the body as a body |

Two unit tests in `src/parser.rs` guard the tables this change added:

| Test | Edge class covered |
|---|---|
| `edge_the_one_line_literal_body_agrees_with_parse_statement_about_starts` | every statement-starting token continues a one-line literal body, and every token that cannot start a statement (`Comma`, `RightParen`, `RightBracket`, `RightBrace`, `End`, `Newline`, `Eof`) ends it |
| `a_literal_parses_in_both_forms_with_the_body_it_was_written_with` | boundary: the one-line and the block form each parse to one `set` |

### Test requirement matrix

- **empty** — covered: `edge_a_literal_needs_no_parameters`, `edge_a_literal_with_an_empty_body_returns_nothing`, `edge_map_over_an_empty_list_is_an_empty_list`, `edge_map_hands_each_element_to_the_literal` (empty text as an argument).
- **singleton** — covered: `edge_a_literal_needs_no_parameters` (one parameter list, `()`), `edge_a_parameter_shadows_an_enclosing_binding` (one parameter), `edge_map_over_a_singleton_is_a_singleton`.
- **boundary** — covered: `edge_nested_literals_within_the_block_budget_run` (nesting at the budget), `edge_nested_literals_past_the_block_budget_are_reported` (one past it), `a_literal_maps_in_every_spelling` (index `0` and the last element of each mapped list).
- **out_of_bounds** — N/A: a literal introduces no index. The rows that would apply are covered for the list it is passed to (`edge_map_over_an_empty_list_is_an_empty_list` is the `len == 0` case); `tests/index_bounds_test.rs` covers indexing itself and is unchanged.
- **type_mismatch** — covered: `edge_calling_a_literal_with_the_wrong_type_fails` (text into a numeric parameter), `edge_calling_map_without_a_function_fails` and `edge_calling_map_on_a_list_without_a_function_fails` (a list where a function was wanted).
- **numeric_boundary** — N/A: no arithmetic is added. The numbers in these tests are small literals; `tests/numeric_edge_test.rs` owns that row and is unchanged.
- **unicode** — covered: `edge_map_hands_each_element_to_the_literal` (`héllo ☃ 日本語 🎉` in, and back out).
- **nesting_recursion** — covered: `edge_three_nested_literals_each_capture_their_own_level` (four levels), `edge_literals_sit_inside_a_list_and_a_record` (a literal inside a list and a record), `edge_a_literal_nested_inside_itself_stops_at_the_call_depth_limit`.
- **duplicate_missing_keys** — N/A: a literal has no keys. `a_literal_can_be_stored_in_a_record` stores one in a record and reads the field back, which is the nearest thing this change can reach; `tests/record_order_test.rs` owns the row and is unchanged.
- **malformed_input** — covered: `edge_an_unterminated_literal_names_the_missing_end`, `edge_a_multiline_body_without_its_end_fails_rather_than_eating_the_file`. Empty file, BOM, CRLF and non-UTF-8 are lexer rows owned by `tests/lexer_robustness_test.rs`, unchanged.
- **resource_limit** — covered: `edge_nested_literals_past_the_block_budget_are_reported` (the block budget), `edge_a_literal_nested_inside_itself_stops_at_the_call_depth_limit` (the call-depth limit).

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | pass — 1006 passed, 0 failed, 0 ignored |
| `./rbops/verify.sh phase-034` | **not run** — there is no `rbops/` directory in this checkout (`ls rbops/` → `No such file or directory`); the pipeline that owns it lives outside the project and was not inspected, per the phase instructions. What that gate would cover beyond the three above was run by hand: all 6 `examples/*.rb` and both `modules/*.rb` exit 0, `tests/closure_capture_test.rs` is 26 passed / 0 failed and `tests/loop_bounds_test.rs` is 24 passed / 0 failed, both unchanged, and no test was ignored, skipped or `#[allow]`ed. |

Pre-existing tests that changed count: **none**. No test was deleted, renamed,
`#[ignore]`d, `.skip`ped or given an `allow(clippy::`. No assertion in an
existing test was loosened, no threshold lowered, no timeout raised.

## Invariants touched

- **None.** `.rb`, `to … end`, `set x to …` and `say` are unchanged. `Value`'s
  variants are unchanged — a literal is an existing `Value::Function`.
  `Expr` **gained** a variant, which is the phase: `Error::{Lexer,Parser,
  Analyzer,Runtime,Io}` is unchanged, and the trailing-comma and `{interp}`
  syntax is untouched.
- A `to` token in *expression* position is now a literal. In *statement*
  position it is still a declaration, so `to name() … end` at the start of a
  line means exactly what it meant before — pinned by the unchanged
  `tests/parser_hardening_test.rs`, whose 65-deep block budget cases and
  `opens_block` list still hold.
- `TokenKind::To` joined `is_expression_start`, which is what makes
  `give back to … end` return a function. A `give back` followed by a
  declaration still needs the declaration on its own line, because the token
  after `give back` is then a newline.
- `map` went from `Unknown function` to working, in three spellings. A program
  that defined its own `map` still wins: the VM resolves a user function before
  the builtin.

## Known gaps / follow-ups

- `rb compile` refuses a function literal (`src/bytecode/codegen.rs`) rather
  than compiling it: `expr` has no block pool, and a literal needs a block. The
  bytecode path has no test for it either way. → FINDINGS #1, and a phase of its
  own for S1/S2.
- The block budget's stack cost is ~30 KB per level and a literal level costs
  more than a block level, so 56+ nested literals abort on a 2 MiB thread. `rb`
  parses on an 8 MiB main thread and reports the budget correctly. →
  FINDINGS #1.
- Calling a function with the wrong argument count is still lenient. → FINDINGS #5.
- `filter` and `reduce` are registered and unimplemented, as `uppercase` and
  seventeen other stdlib functions are. → FINDINGS #2 and #3.
- `add x to y` and `x mod y is 0` do not parse, so `SPEC.md` § Closures'
  `make_counter` still cannot run even though the literal it needs now does.
  → FINDINGS #4.