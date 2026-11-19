# Phase 119 — Expose the last value as `$_` and name sessions

## What changed
| File | Lines | What |
|---|---|---|
| src/lexer.rs | +25 −0 | reads `$_` as one identifier; a bare `$` is still a lexer error, and lexer tests pin both |
| src/interpreter.rs | +21 −1 | `set_global_binding` lets the REPL point `$_` at `_` without re-pointing the whole VM; `user_names` holds `$_` out of the session's own names so listings do not gain a second row |
| src/repl/mod.rs | +437 −12 | `$_` is kept equal to `_`, `:last` prints that value, `:session <name>` labels a sitting, `:sessions` lists labels plus the existing checkpoints, the bytecode path is seeded with `$_`, and tests across the phase pin the behaviour |
| src/repl/debug_repl.rs | +83 −2 | the debug sub-REPL keeps its own `$_` equal to its own `_`, never inheriting the parent's alias, with tests pinning both rules |
| src/repl/commands.rs | +25 −3 | command variants and table rows for `:last` and `:session`; `:sessions` description now names both halves |
| tests/repl_test.rs | +43 −0 | end-to-end stdout tests for `:last` and for the labels-plus-checkpoints listing |
| tests/unreviewed_work_ack_test.rs | +39 −0 | pins phase-118's delivery commit by SHA, as the phase-after-delivery handoff requires, and refuses its prefix on the in-flight rule |
| src/bytecode/format.rs, NATIVE.md | +2 −2 each | the interpreter line citations move with the added lines, so the study still points at `run` and `captured` |

## Tests added
| Test | Edge class covered |
|---|---|
| last_value_is_usable_as_a_name | happy path |
| last_value_is_usable_as_a_name_on_the_bytecode_engine | engine boundary — `$_` crosses into the fresh bytecode VM |
| last_value_after_an_error_is_the_last_successful_value | failure does not replace the last successful value |
| last_command_reports_the_same_value_as_the_name | `:last` and `$_` cannot disagree |
| edge_first_line_last_value_is_a_named_error | singleton / boundary — first line |
| edge_last_value_after_only_a_nothing_line_is_a_named_error | empty / nothing — a successful line that produced no value |
| edge_last_before_any_success_raises_a_named_error | empty — no last result at all |
| edge_labels_are_listed_once_in_first_use_order | duplicate labels / deterministic order |
| edge_a_label_keeps_spaces_and_command_characters | spaces and characters a command word cannot carry |
| edge_a_session_label_cannot_be_empty | malformed input — empty label |
| labelling_a_session_does_not_change_a_program | labelling is observation only |
| the_existing_underscore_binding_is_unchanged | pre-existing `_` contract |
| test_last_value_is_one_identifier | lexer — `$_` is a single identifier |
| test_bare_dollar_is_still_a_lexer_error | lexer — a bare `$` is refused |
| edge_first_line_last_value_is_a_named_error_on_the_bytecode_engine | engine boundary / empty — first line on the bytecode VM |
| edge_labels_and_checkpoints_are_two_separated_sections | combined listing format |
| last_command_prints_the_same_value_as_the_name | end-to-end — `:last` emits the same `= value` line |
| edge_sessions_lists_labels_and_checkpoints_as_separate_sections | end-to-end combined listing |
| edge_a_session_command_with_no_name_shows_usage_and_labels_nothing | end-to-end malformed input — missing label argument |
| last_value_alias_is_not_listed_as_a_second_session_binding | `$_` is a view of `_`, not a duplicate `:vars` row |
| edge_a_session_label_survives_a_reset_that_forgets_bindings_and_checkpoints | `:reset` clears bindings and checkpoints but keeps the label |
| a_sub_repl_last_value_is_named_by_the_same_alias_as_the_parent | debug sub-REPL — `$_` is its own last value and survives a failed line |
| the_parents_last_value_alias_does_not_leak_into_the_sub_repl | debug sub-REPL — its `_` starts empty, so the parent's `$_` is not inherited |

The pre-existing `_` contract is also pinned by the tests that predate this phase and were not modified, including `edge_a_block_that_outgrows_the_buffer_is_dropped_not_kept` and the checkpoint tests that read `_`.

## Test requirement matrix
| Row | Status |
|---|---|
| empty | covered — empty label and empty last value |
| singleton | covered — first line of a fresh session |
| boundary | covered — first successful value vs. before any success |
| out_of_bounds | N/A — no indexing or ordered lookup is added |
| type_mismatch | N/A — `$_` carries an already-typed `Value`; no new coercion |
| numeric_boundary | N/A — no arithmetic or numeric conversion is added; `$_` returns the stored value |
| unicode | N/A — no new text transcoding; labels are stored as the argument string and the lexer's string path is unchanged |
| nesting_recursion | N/A — no recursive or nested structure is walked |
| duplicate_missing_keys | N/A — no keyed container is added; duplicate labels are covered above |
| malformed_input | covered — empty label, and a missing `$_`/`:last` value is a clean error |
| resource_limit | N/A — only an interactive command adds a label, so a program cannot grow the list; labels are one string per user action, like aliases |

## Gates
| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 3207 passed, 0 failed |
| ../rbops/verify.sh "phase-119" | PASS |

## Invariants touched
- None. `_` is still set only by a successful non-`Nothing` line and is still not a program name; `$_` is additive.

## Known gaps / follow-ups
- `:session` labels live for the process and are not written into `:save` / `:restore` files. The phase asks for labelling and listing, not persistence; saving labels would be a separate change.
- The full suite's `tests/unreviewed_work_ack_test.rs` was red before this phase because the `resume phase-118: adopt preserved work` delivery commit had no acknowledgement. It is pinned by its SHA in `ACKNOWLEDGED`, the same handoff phase-118 made for phase-117; its prefix is deliberately absent from `DELIVERY_SUBJECT_PREFIXES` so the in-flight rule is not widened.
