# Phase 114 — multiple assignment parses; dotted import marked Not Implemented

## What changed
| File | Lines | What |
|---|---|---|
| src/parser.rs | +98 −0 | `Statement::SetMultiple`; `set a, b to x, y` parses with arity and duplicate-name checks; module declared names include both names; three parser unit tests |
| src/analyzer.rs | +10 −0 | analyses every right-hand side, then declares every bound name |
| src/interpreter.rs | +18 −2 | evaluates all right-hand sides before binding any name, binds in the bytecode VM's reversed store order, and reads multiple-assignment names into the analyzer's module-bound names |
| src/runtime.rs | +19 −8 | module loading evaluates every right-hand side and contributes every name a multiple assignment binds, in the VMs' reverse binding order |
| src/bytecode/codegen.rs | +9 −0 | emits all right-hand sides, then stores in reverse to preserve evaluate-before-bind |
| src/bytecode/vm.rs | +1 −0 | module-body filter keeps a multiple assignment |
| src/formatter.rs | +16 −0 | formats `set a, b to x, y` |
| src/linter.rs | +8 −0 | assigns and analyses every name/value in a multiple assignment |
| src/repl/introspect.rs | +4 −0 | statement name and call-walk support, with both values of a multiple assignment read in source order |
| src/repl/weight.rs | +4 −0 | weight walk support |
| docs/GRAMMAR.md | +0 −2 | removes the dotted `import a.b as c` production from the grammar and keeps its **Not Implemented** note with the working alternatives |
| NATIVE.md, src/bytecode/format.rs | +8 −8 | updates the source-line citations moved by the interpreter and runtime changes |
| tests/doc_blocks_runs.txt, tests/doc_blocks_fragments.txt | +9 −9 | follows the removed grammar line |
| tests/multiple_assignment_test.rs | +293 −0 | new tests, helpers and edge cases |
| tests/unreviewed_work_ack_test.rs | +1 −0 | adds `resume phase-114:` to the in-flight delivery-subject rule so the pipeline's rebuildable delivery commit stays acknowledged |

## Tests added
| Test | Edge class covered |
|---|---|
| multiple_assignment_binds_each_name | singleton / happy path, two names |
| multiple_assignment_three_names | boundary: three names and three values |
| multiple_assignment_evaluates_before_binding | nesting/state: swap reads both right-hand sides first |
| multiple_assignment_agrees_with_the_bytecode_vm | nesting: tree VM and bytecode VM agree on the swap |
| edge_a_constant_target_fails_the_same_way_on_both_vms | failure path: a caught constant refusal leaves the same binding on both VMs |
| edge_a_module_files_multiple_assignment_binds_every_name | module boundary: a file module's `set a, b to 1, 2` binds both names for the importer through the analyzer and loader |
| edge_trailing_comma_in_names_is_refused | malformed input: trailing comma in names |
| edge_trailing_comma_in_values_is_refused | malformed input: trailing comma in values |
| edge_missing_right_hand_side_is_refused | arity mismatch (`set x, y to 1`) |
| edge_extra_right_hand_side_is_refused | arity mismatch (`set x to 1, 2`) |
| edge_duplicate_name_is_refused | duplicate keys: repeated left-hand name |
| edge_multiple_assignment_values_are_full_expressions | boundary: each right-hand side is an expression |
| edge_multiple_assignment_binds_text_and_nothing | type: text and `nothing` values |
| edge_multiple_assignment_missing_value_is_err | failure assertion: `is_err` for a missing value |
| edge_dotted_import_without_alias_is_err | failure assertion: `is_err` for a dotted import |
| single_assignment_still_works | regression: ordinary `set x to 1` |
| existing_import_forms_still_work | regression: `import Mod` and `import a as b` |
| edge_dotted_import_with_alias_is_refused | failure assertion: `import math.sqrt as s` is a Parser error |
| edge_dotted_import_without_alias_is_refused | failure assertion: `import math.sqrt` is a Parser error |
| edge_dotted_import_missing_name_is_refused | failure assertion: nonexistent dotted member is a Parser error |
| parser::tests::multiple_assignment_parses_into_one_statement | singleton: one AST statement holds both names and both values |
| parser::tests::edge_multiple_assignment_arity_mismatch_is_a_parser_error | failure assertion: arity mismatch at the parser |
| parser::tests::edge_dotted_import_is_a_parser_error | failure assertion: dotted import at the parser |
| edge_a_module_files_multiple_assignment_binds_every_name_on_the_bytecode_vm | bytecode import path: a module's multiple assignment survives `compile_module` and binds both names |
| edge_a_modules_multiple_assignment_binds_in_the_vms_reverse_order | module loader boundary: a caught constant collision leaves the same binding as both VMs |
| edge_multiple_assignment_round_trips_and_passes_format_check | formatter round trip: output re-parses and `rb format --check` passes |
| edge_a_multiple_assignment_tracks_every_name_and_every_value (linter) | linter: every left name is tracked and every right expression is read |
| edge_a_question_refuses_a_statement_and_names_the_keyword (introspect) | introspection: `set a, b to …` is named as a `set` statement |
| edge_a_body_is_walked_in_source_order_and_stops_at_its_end (introspect) | introspection: both values of a multiple assignment are walked |
| val_bytes_covers_a_multiple_assignment_function_body (weight) | session weight: both names and both values are counted |

## Review follow-ups (round 2)
- The pre-existing `tests/tooling_grammar_test.rs` binding test is an LSP token
  walk (`src/lsp.rs` `analyze`), not the parser used by `rb run`: it never calls
  `parser::parse`, so it does not show that the execution frontend accepted
  `set a, b to …` before `88849677`. The phase premise concerns that parser; the
  regression that pinned the old failure is `multiple_assignment_binds_each_name`.
- `runtime::module_bindings` now writes a multiple assignment's targets in the
  same reverse order as the tree and bytecode VMs, so all three binding paths
  refuse a constant among the targets at the same point.
- `docs/GRAMMAR.md` now records the duplicate-name restriction beside the
  multiple-assignment production.

## Gates
| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | pass — 3113 passed, 0 failed |
| ../rbops/verify.sh phase-114 | pass |

## Invariants touched
- `set x to <expr>` is unchanged; `set x, y to <a>, <b>` is the newly accepted form the grammar already documented.
- `import a` and `import a as b` are unchanged; the dotted `import a.b as c` form remains a clean `ParserError`, now stated as Not Implemented in `docs/GRAMMAR.md`.

## Known gaps / follow-ups
- Dotted import (`import a.b as c`) is documented as Not Implemented rather than implemented → `FINDINGS.md`.

## Edge-case matrix
| Row | Disposition |
|---|---|
| empty | N/A — the grammar requires at least one name and one value; `set x, y to 1` and `set x to 1, 2` cover the missing side. |
| singleton | covered — `single_assignment_still_works` and `multiple_assignment_binds_each_name`. |
| boundary | covered — two and three names. |
| out_of_bounds | N/A — no index is involved. |
| type_mismatch | N/A — assignment binds any `Value`; no declared types in this form. |
| numeric_boundary | N/A — no arithmetic is performed by the statement. |
| unicode | N/A — identifiers are tokenised by the existing lexer; no new text handling. |
| nesting_recursion | covered — swap evaluation order, bytecode agreement, and a caught constant-refusal agreement test. |
| duplicate_missing_keys | covered — duplicate left-hand name refused; missing right-hand side refused. |
| malformed_input | covered — trailing commas, missing/extra values, dotted imports. |
| resource_limit | N/A — the statement adds no loops or recursion; existing call/loop limits apply. |
