# Phase 072 — Let the session switch engines: `.vm` toggles tree-walker and bytecode VM

## What changed
| File | Lines | What |
|---|---|---|
| src/repl/commands.rs | +15 −0 | `.vm [walker|bytecode]` command variant, table row, optional argument, help text |
| src/repl/mod.rs | +698 −10 | `Engine` state, `.vm` dispatch/refusal, per-run bytecode execution, binding and context hand-off, prompt naming, save/restore persistence, tests |
| src/bytecode/vm.rs | +135 −15 | exposed session bindings (`user_bindings`) while filtering builtins and qualified member names; context seed/adopt (`session_context`, `observe_context`) and module programs retained for the hand-off |
| src/interpreter.rs | +21 −0 | published-member bindings handed to the bytecode VM so a qualified call resolves after a switch |

The default remains `Engine::Walker`. `.vm` with no argument reports the current engine without changing it; `.vm bytecode` switches, `.vm walker` switches back. A third word is refused with both valid names. After the first `.vm`, the rendered prompt names the engine (the default `>>> ` is unchanged for sessions that never switch). `:save` writes an `engine` record for every state section, including the default engine, and `:restore` puts back the saved engine; a file with no engine record restores the Walker default.

## Tests added
| Test | Edge class covered |
|---|---|
| repl::tests::engines::vm_switches_engine_and_the_next_program_runs_there | engine marker proves the bytecode VM ran; singleton switch |
| repl::tests::engines::bindings_survive_both_directions_of_a_switch | state round-trip walker→bytecode→walker→bytecode |
| repl::tests::engines::edge_vm_refuses_a_third_engine_name_and_names_the_two | malformed input / asserted failure |
| repl::tests::engines::edge_engine_specific_divergence_is_reported_with_the_engine_named | type mismatch: the same failing call is placed `2:10` by the walker and `2:1` by bytecode, each failure naming its engine |
| repl::tests::engines::edge_vm_switch_inside_a_multiline_block_applies_at_execution | nesting / buffered block; the block survives the command and runs on the new engine |
| repl::tests::engines::edge_vm_choice_survives_save_and_restore | state persistence through `:save`/`:restore` |
| repl::tests::engines::edge_a_saved_walker_session_returns_a_bytecode_session_to_the_walker | a file with no engine record restores the Walker default and replays there |
| repl::tests::engines::edge_a_walker_state_section_names_the_engine_it_saved | a Walker session with aliases writes `engine walker` and restores both halves |
| repl::tests::engines::edge_vm_rapid_double_switch_is_idempotent | boundary / duplicate switch; bindings, last result, history and transcript unchanged |
| repl::tests::engines::edge_a_bare_vm_asks_without_switching | empty / query form: bare `.vm` reports the engine and changes neither the engine nor the prompt |
| repl::tests::engines::edge_an_import_alias_survives_a_second_bytecode_line | persistence: an `import ... as` on one bytecode line still names its module on the next |
| repl::tests::engines::edge_a_bytecode_import_joins_the_walker_context_on_a_switch | context round-trip: the imported module, its alias and its members cross back to the walker |
| repl::tests::engines::edge_a_bytecode_object_joins_the_walker_context_on_a_switch | context round-trip: an object declaration made on bytecode is a declared type after switching back |
| repl::tests::engines::edge_restoring_a_walker_session_with_aliases_keeps_the_default_prompt | state restoration: a Walker session's engine record does not turn the default prompt into an engine-named one |
| repl::tests::surface::a_vm_switch_is_named_by_the_next_prompt | prompt names the engine after either switch |

Edge-case matrix:
- empty — covered: bare `.vm` is the query form and changes no state.
- singleton — covered: switching once before any binding (`vm_switches_engine_*`).
- boundary — covered: rapid double switch is idempotent.
- out_of_bounds — N/A: no index or length argument was added.
- type_mismatch — N/A: the argument is a closed set of words, not a typed value; the divergence test exercises a real runtime type mismatch instead.
- numeric_boundary — N/A: no arithmetic or numeric conversion changed.
- unicode — N/A: engine words are ASCII and no text transformation was added.
- nesting_recursion — covered: switch inside a multiline block.
- duplicate_missing_keys — N/A: no record fields were added to the language.
- malformed_input — covered: a third engine word is refused by name.
- resource_limit — N/A: a switch allocates no persistent second VM; bytecode runs use a fresh VM per line.

## Gates
| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2208 passed, 0 failed |
| ../rbops/verify.sh phase-072 | pass (after this report was added) |

## Invariants touched
- None: the default engine and the documented tree-walking semantics are unchanged; `.rb`, `end`, `set`, `say`, `Value` and `Error` variants are untouched. `.vm` is a REPL command, not a language construct.

## Known gaps / follow-ups
- A function declared while the bytecode engine is active carries a compiled body; switching back to the tree-walker and calling it produces the existing clean "only `rb vm` can run" error. Full function-value portability across engines is not claimed here.
- Modules, aliases and object declarations do cross between engines as context, but a method or function carries the body it was declared with: a compiled body is not callable by the tree-walker, matching the function-value gap above.
- The debugger remains a tree-walker facility; `.vm bytecode` does not arm breakpoints for bytecode runs.
