# Phase 114 — multiple assignment parses; dotted import marked Not Implemented

## What changed
| File | Lines | What |
|---|---|---|
| src/parser.rs | +145 −1 | `Statement::SetMultiple`; `set a, b to x, y` parses with arity and duplicate-name checks; module declared names include both names; three parser unit tests; a dotted import now fails with a message naming the unsupported form instead of the internal `Dot` token |
| src/analyzer.rs | +10 −0 | analyses every right-hand side, then declares every bound name |
| src/interpreter.rs | +56 −17 | evaluates all right-hand sides before binding any name, binds in the bytecode VM's reversed store order, reads multiple-assignment names into the analyzer's module-bound names, and snapshots the globals, constants and module context around `load_module` so a failed import rolls back every binding |
| src/runtime.rs | +95 −13 | module loading evaluates and binds each declaration in order; `ModuleLoader::check_bind` lets a multiple assignment refuse every target before binding any of them, so the loader is atomic by itself; the trait and function docs describe their separate contracts |
| src/bytecode/codegen.rs | +9 −0 | emits all right-hand sides, then stores in reverse to preserve evaluate-before-bind |
| src/bytecode/vm.rs | +34 −0 | module-body filter keeps a multiple assignment; the import frame hides the caller's handlers while a module runs and restores the module state when it fails |
| src/formatter.rs | +16 −0 | formats `set a, b to x, y` |
| src/linter.rs | +8 −0 | assigns and analyses every name/value in a multiple assignment |
| src/repl/introspect.rs | +4 −0 | statement name and call-walk support, with both values of a multiple assignment read in source order |
| src/repl/weight.rs | +4 −0 | weight walk support |
| docs/GRAMMAR.md | +1 −2 | removes the dotted `import a.b as c` production from the grammar, records the parser's explanatory refusal, and writes the comma-separated multi-import form the parser accepts |
| NATIVE.md, src/bytecode/format.rs | +8 −8 | updates the source-line citations moved by the interpreter and runtime changes |
| tests/doc_blocks_runs.txt, tests/doc_blocks_fragments.txt | +9 −9 | follows the removed grammar line |
| tests/multiple_assignment_test.rs | +526 −0 | new tests, helpers and edge cases, including the explanatory dotted-import refusal, empty-value binding, and the two-constant cross-engine refusal |
| tests/unreviewed_work_ack_test.rs | +40 −2 | the in-flight prefix for the phase under review comes from the pipeline's `RBOPS_PHASE` (falling back to the delivery commit at `HEAD`), and a test refuses a checked-in entry for it, so a phase cannot certify its own delivery |

## Reproduction
`set x, y to 1, 2` (was `ParserError: Expected 'to' but got Comma`) now prints `1` then `2`. `import math.sqrt as s` is refused with `ParserError: Dotted imports are not implemented; use 'import name' or 'import name as alias'`, which is the Not Implemented arm recorded in `docs/GRAMMAR.md` § 3.1.

## Tests added
| Test | Edge class covered |
|---|---|
| multiple_assignment_binds_each_name | singleton / happy path, two names |
| multiple_assignment_three_names | boundary: three names and three values |
| multiple_assignment_evaluates_before_binding | nesting/state: swap reads both right-hand sides first |
| multiple_assignment_agrees_with_the_bytecode_vm | nesting: tree VM and bytecode VM agree on the swap |
| edge_a_constant_target_fails_the_same_way_on_both_vms | failure path: a caught constant refusal leaves the same binding on both VMs |
| edge_two_constant_targets_name_the_same_target_on_both_vms | failure path: with two constant targets, both engines name the same refused target |
| edge_the_phase_under_review_is_not_added_to_this_files_list | audit boundary: the pipeline's in-flight phase is covered without the phase editing its own allowlist |
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
| edge_five_names_bind_in_source_order | boundary: five names keep source order despite the VMs' reverse store |
| edge_a_non_name_target_is_a_parser_error | failure assertion: a literal on the left is refused |
| edge_dotted_import_with_alias_is_a_parser_error | failure assertion: `import math.sqrt as s` is a Parser error via `is_err` |
| parser::tests::multiple_assignment_parses_into_one_statement | singleton: one AST statement holds both names and both values |
| parser::tests::edge_multiple_assignment_arity_mismatch_is_a_parser_error | failure assertion: arity mismatch at the parser, pointing at the separating comma |
| parser::tests::edge_a_duplicate_multiple_assignment_name_points_at_the_repeat | failure assertion: duplicate-name error points at the repeated name |
| parser::tests::edge_dotted_import_is_a_parser_error | failure assertion: dotted import at the parser |
| edge_a_module_files_multiple_assignment_binds_every_name_on_the_bytecode_vm | bytecode import path: a module's multiple assignment survives `compile_module` and binds both names |
| edge_a_modules_multiple_assignment_binds_in_the_vms_reverse_order | module loader boundary: a failed constant collision leaves nothing bound on either VM |
| edge_a_failed_import_binds_nothing | import atomicity: a later declaration's failure rolls back the earlier declaration on both VMs |
| runtime::module_binding_order_tests::edge_module_bindings_try_a_multiple_assignments_targets_in_reverse | module loader boundary: the loader checks `b` and `a` in reverse, refuses before writing, and leaves nothing bound |
| edge_multiple_assignment_round_trips_and_passes_format_check | formatter round trip: output re-parses and `rb format --check` passes |
| edge_a_multiple_assignment_tracks_every_name_and_every_value (linter) | linter: every left name is tracked and every right expression is read |
| edge_a_question_refuses_a_statement_and_names_the_keyword (introspect) | introspection: `set a, b to …` is named as a `set` statement |
| edge_a_body_is_walked_in_source_order_and_stops_at_its_end (introspect) | introspection: both values of a multiple assignment are walked |
| val_bytes_covers_a_multiple_assignment_function_body (weight) | session weight: both names and both values are counted |
| edge_dotted_import_names_the_unsupported_form | failure assertion: dotted import error names the unsupported form and carries a span |
| edge_dotted_import_without_alias_names_the_unsupported_form | failure assertion: no-alias dotted import gets the same explanation |
| edge_a_dotted_import_after_a_plain_one_is_refused | failure assertion: a dotted member in a multi-import is refused where it is written |
| edge_multiple_assignment_binds_empty_values | empty: empty list, `nothing`, and empty text all bind |

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

## Review follow-ups (round 3 — resumed run)
- The resumed tree had `runtime::module_bindings` converted to a `ModuleLoader`
  trait while `Vm::load_module` still expected the old returned `Vec`. That did
  not compile. `VmModuleLoader` now forwards `evaluate`/`bind` to the tree VM,
  so module declarations still bind one at a time through `bind_module_name`
  and `bind_constant` instead of being collected first.
- The line citations in `NATIVE.md` and the `native_study` test moved again when
  `VmModuleLoader` was inserted; both now name the current lines.
- Three further edge tests landed in `tests/multiple_assignment_test.rs` so the
  resumed diff itself carries the quota, the `edge_*` rule and a failure
  assertion.

## Review follow-ups (round 4 — review findings)
- Import atomicity is restored on both VMs: `Vm::load_module` snapshots its
  globals, constants and module context and restores them on every failing path,
  while the bytecode `import` hides the caller's handlers while the module runs
  and restores the same state on failure. `edge_a_failed_import_binds_nothing`
  imports a module whose second declaration fails, catches the failure, and
  shows the first name is not bound.
- The multiple-assignment arity error now points at the nearest separating comma
  and the duplicate-name error points at the repeated name, both captured while
  parsing rather than taken from the token after the statement.
- The `ModuleLoader` docs now describe `evaluate`/`bind`; `module_bindings` has
  its own doc preserving the "file is read once" rationale.
- The reverse-order loader edge is still pinned directly by a runtime unit test,
  while the VM test now asserts that a failed import rolls back instead of
  leaving the earlier target behind.

## Why import atomicity is in this phase
The new statement is a multi-target store, so every path that binds a
module's declarations has to make the same all-or-nothing decision about it. The
tree VM's assignment path, the bytecode `STORE` run, and `module_bindings` are
those paths; leaving the third with a partial binding would make
`set a, b to …` mean one thing in a program and another in the same declarations
written in a module. The import transaction is therefore the mechanism that
keeps this phase's new statement correct at the module boundary, not a general
import refactor added for its own sake. It is narrowly scoped to the writes an
import makes (the `ImportUndo` variants name exactly the tables a module can
touch), and the tree VM keeps its caller-side snapshot as a second line of
defence.

## Review follow-ups (round 5 — review findings)
- The tree VM now pre-checks a multiple assignment's constant targets in the
  same reverse order the bytecode VM stores them, and
  `edge_two_constant_targets_name_the_same_target_on_both_vms` pins that both
  engines name `b` when both targets are constants.
- `module_bindings` now calls a new `ModuleLoader::check_bind` for every target
  before binding any of them, so the loader itself is atomic; the runtime unit
  test asserts the reverse check order and that no bind was attempted. The
  trait and function docs no longer claim atomicity that only the caller's
  snapshot provided.
- The import-atomicity scope is justified above rather than split out of this
  phase, because the phase's own multi-target statement is what exposed the
  module-loader path's partial binding.
- The phase no longer adds its own `resume phase-114:` prefix to the
  acknowledgement list. The audit reads the in-flight phase from the pipeline's
  `RBOPS_PHASE` (or the delivery commit at `HEAD` in a plain `cargo test`), and
  a new test refuses a checked-in entry for the phase under review.
- `docs/GRAMMAR.md` now spells the comma-separated multi-import production
  (`import files, math as m`) instead of only the single-item form.

## Gates
| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | pass — 3128 passed, 0 failed |
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
