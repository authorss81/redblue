# Phase 119 — Expose the last value as `$_` and name sessions

## What changed
| File | Lines | What |
|---|---|---|
| src/lexer.rs | +47 −0 | reads only the exact `$_` as one identifier; `$_foo` is still the bare `$` error, a bare `$` is still a lexer error, and lexer tests pin all three |
| src/analyzer.rs | +88 −8 | refuses `$_` everywhere, and `_` as a declaration or assignment target only in a session analysis, so the alias cannot be shadowed and the VM's `_` cannot drift from the session's last value without breaking standalone programs that use `_`; reads of both are still allowed |
| src/interpreter.rs | +21 −1 | `set_global_binding` lets the REPL point `$_` at `_` without re-pointing the whole VM; `user_names` holds `$_` out of the session's own names so listings do not gain a second row |
| src/repl/mod.rs | +1035 −14 | `$_` is kept equal to `_`, `:last` prints that value, `:session <name>` labels a sitting, `:sessions` lists labels plus the existing checkpoints, the bytecode path is seeded with `$_`, labels stop at `MAX_SESSION_LABELS` so a piped `:session` cannot grow the list without bound, labels reject control and bidirectional formatting characters so a newline or a right-to-left override cannot forge or reorder listing rows, and every state restore (`:undo`, `:redo`, `:restore`, `:reset`, `:load`) re-points the alias at the restored `_`; the resumed pass adds tests for a unicode last value, a type-mismatch error, and a call-time lookup across function calls |
| src/repl/debug_repl.rs | +141 −6 | the debug sub-REPL clears both inherited halves of the parent's last-value pair at the fork, then keeps its own `_` in its VM and its own `$_` equal to it after every successful line, never inheriting the parent's alias or binding, with tests pinning every rule |
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
| the_underscore_binding_cannot_be_reassigned_out_from_under_the_alias | `_` is the session's binding, so a program cannot desync it from `$_` and `:last` |
| edge_the_underscore_binding_cannot_be_assigned | analyzer — `_` may be read but never bound in a session, just as `$_` may not |
| edge_a_standalone_program_may_still_bind_underscore | analyzer — `_` remains an ordinary name for a standalone program |
| edge_a_session_refuses_underscore_even_before_it_is_bound | analyzer — a session owns `_` from its first line, so supplying other names does not leave the refusal off |
| test_last_value_is_one_identifier | lexer — `$_` is a single identifier |
| test_bare_dollar_is_still_a_lexer_error | lexer — a bare `$` is refused |
| test_last_value_name_does_not_swallow_a_suffix | lexer — only the exact `$_` is a name, not every name it prefixes |
| edge_first_line_last_value_is_a_named_error_on_the_bytecode_engine | engine boundary / empty — first line on the bytecode VM |
| edge_labels_and_checkpoints_are_two_separated_sections | combined listing format |
| last_command_prints_the_same_value_as_the_name | end-to-end — `:last` emits the same `= value` line |
| edge_sessions_lists_labels_and_checkpoints_as_separate_sections | end-to-end combined listing |
| edge_a_session_command_with_no_name_shows_usage_and_labels_nothing | end-to-end malformed input — missing label argument |
| last_value_alias_is_not_listed_as_a_second_session_binding | `$_` is a view of `_`, not a duplicate `:vars` row |
| edge_a_session_label_survives_a_reset_that_forgets_bindings_and_checkpoints | `:reset` clears bindings and checkpoints but keeps the label |
| a_sub_repl_last_value_is_named_by_the_same_alias_as_the_parent | debug sub-REPL — `$_` is its own last value and survives a failed line |
| the_parents_last_value_alias_does_not_leak_into_the_sub_repl | debug sub-REPL — its `_` starts empty, so the parent's `$_` is not inherited |
| the_parents_last_value_binding_does_not_leak_into_the_sub_repl | debug sub-REPL — both `_` and `$_` start empty, so neither half of the parent's pair leaks |
| last_value_composes_in_a_following_expression | `$_` is a value in a larger expression, not only a line of its own |
| edge_a_unicode_session_label_is_kept_whole | unicode — a label with emoji, CJK and RTL text is stored as written |
| edge_session_labels_stop_at_their_capacity | resource limit / boundary — the label list stops at its cap |
| edge_session_label_capacity_is_a_named_error | resource limit — a label past the cap is a clean named error |
| declaring_last_value_is_refused_so_it_stays_a_view_of_underscore | malformed input / declaration target — `$_` may be read but never bound, so it cannot shadow `_` |
| state_restores_keep_the_last_value_alias_equal_to_underscore | state boundary — undo, redo, checkpoint restore, reset and load all keep `$_` a view of `_` |
| edge_a_session_label_with_a_newline_is_refused | malformed input — a multi-line label cannot forge rows in the listing |
| edge_a_session_label_with_a_bidi_control_is_refused | malformed input — a bidirectional override or isolate cannot reorder or forge a listing row |
| edge_a_whitespace_only_session_label_is_refused | malformed input / empty — a label of only whitespace names nothing |
| edge_a_zero_width_only_session_label_is_refused | malformed input / empty — a label of invisible zero-width characters names nothing |
| edge_a_session_label_is_trimmed_at_both_ends | boundary — surrounding padding is not carried into the listing |
| edge_last_command_after_a_failed_line_names_the_last_successful_value | failure does not replace the value `:last` prints, matching `$_` |
| edge_last_value_is_readable_inside_a_function_body | nesting — the alias resolves in a function body, not only at top level |
| edge_last_value_carries_unicode_text_whole | unicode — `$_` returns a text value with emoji and CJK as written |
| edge_last_value_type_mismatch_is_a_clean_runtime_error | type mismatch — adding a number to the last value's text is a named runtime error, not a panic |
| edge_last_value_is_the_latest_success_across_function_calls | nesting / state — `$_` is resolved at call time, not frozen when a function was defined |
| `the_parents_last_value_binding_does_not_leak_into_the_sub_repl` (strengthened) | debug sub-REPL — after `2 + 3`, `_` reads back exactly `= 5` rather than merely not answering the parent's 99 |

The pre-existing `_` contract is also pinned by the tests that predate this phase and were not modified, including `edge_a_block_that_outgrows_the_buffer_is_dropped_not_kept` and the checkpoint tests that read `_`.

## Test requirement matrix
| Row | Status |
|---|---|
| empty | covered — empty label and empty last value |
| singleton | covered — first line of a fresh session |
| boundary | covered — first successful value vs. before any success |
| out_of_bounds | N/A — no indexing or ordered lookup is added |
| type_mismatch | covered — `$_` carrying text is a clean `Cannot add` runtime error when added to a number |
| numeric_boundary | N/A — no arithmetic or numeric conversion is added; `$_` returns the stored value |
| unicode | covered — a label with emoji, CJK and RTL text is stored as written, and a unicode last value is carried whole |
| nesting_recursion | covered — `$_` resolves inside a function body, at call time, as well as at top level |
| duplicate_missing_keys | N/A — no keyed container is added; duplicate labels are covered above |
| malformed_input | covered — empty label, whitespace-only label, zero-width-only label, a label with a newline/control character, a label with a bidirectional formatting control, and a missing `$_`/`:last` value are clean errors |
| resource_limit | covered — labels stop at `MAX_SESSION_LABELS` and the refusal is a named error |

## Gates
| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 3243 passed, 0 failed |
| ../rbops/verify.sh "phase-119" | PASS |

## Invariants touched
- `_` is still set only by a successful non-`Nothing` line. It is refused as a binding target only inside a session that supplies it, so standalone `_` stays legal; `$_` is additive, readable but never a binding a program may declare or assign.

## Known gaps / follow-ups
- `:session` labels live for the process and are not written into `:save` / `:restore` files. The phase asks for labelling and listing, not persistence; saving labels would be a separate change.
- The full suite's `tests/unreviewed_work_ack_test.rs` was red before this phase because the `resume phase-118: adopt preserved work` delivery commit had no acknowledgement. It is pinned by its SHA in `ACKNOWLEDGED`, the same handoff phase-118 made for phase-117; its prefix is deliberately absent from `DELIVERY_SUBJECT_PREFIXES` so the in-flight rule is not widened.
