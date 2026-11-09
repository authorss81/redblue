# Phase 075 — Teach the language server navigation: goto-definition, references, completion

## What changed

| File | Lines | What |
|---|---|---|
| src/lsp.rs | +1005 −1 | `Location`, `Definition`, `DefinitionKind`; token-level lexical scope walk; `goto_definition`, `find_references`, `completions` and their JSON forms; greedy string-literal and `//` comment context checks; balanced `else if`, multi-name `set`, typed/`of` parameters, catch binders, object-only `has`, range `to` recognition, decorator-position checks, and function-literal frames that close where the parsed one-line body ends |
| src/lib.rs | +3 −2 | re-export the new navigation surface |
| tests/tooling_grammar_test.rs | +663 −2 | 30 navigation tests (26 `edge_*`) including shadowing, builtins, imports, Unicode, comments, malformed source and positions, nested loop/object scopes, `else if`, multi-assignment, typed parameters, catch binders, membership `has`, range bounds, function literals and the 5000-line benchmark |
| tests/unreviewed_work_ack_test.rs | +1 −0 | add `resume phase-075:` to the delivery-subject rule so this phase's rebuildable delivery commit is acknowledged by subject, not by a stale SHA |

No server loop or transport was added: the phase asks for pure functions over source text, and the existing `diagnostics_json` shape is extended per capability.

## Tests added

| Test | Edge class covered |
|---|---|
| goto_definition_on_variable_use_returns_declaration_span | happy path, exact declaration span |
| edge_goto_definition_on_builtin_resolves_to_registry_not_file_span | builtin registry resolution, no file span |
| find_references_distinguishes_shadowed_bindings | nesting / shadowing (inner parameter vs outer `x`) |
| edge_find_references_on_imported_module_spans_the_import | import binding site, not a module definition file |
| edge_empty_source_answers_without_failing | empty source |
| edge_goto_definition_counts_unicode_names_as_characters | Unicode (character columns) |
| edge_completions_at_eof_without_trailing_newline_include_keywords_builtins_and_scope | boundary: EOF with no trailing newline |
| edge_completions_at_eof_after_a_one_line_literal_exclude_its_parameters | boundary: a one-line literal closes at the final line |
| edge_reference_after_a_one_line_literal_resolves_to_the_outer_binding | nesting: a literal parameter does not capture a same-line argument after it |
| edge_completions_inside_string_literal_are_empty | context awareness: inside text |
| edge_completions_inside_line_comment_are_empty | context awareness: inside a `//` comment |
| edge_completions_at_the_end_of_a_line_comment_are_empty | boundary: the terminating newline still belongs to the comment |
| edge_completions_at_eof_inside_an_unterminated_comment_are_empty | boundary: an unterminated comment runs to EOF |
| edge_completions_after_a_line_comment_still_offer_scope | a previous-line comment does not close the scope |
| edge_completions_do_not_treat_slashes_inside_text_as_a_comment | `//` inside a text literal is not a comment |
| edge_navigation_rejects_a_column_past_the_line | out of bounds: column past the line; asserts `is_err()` for all three |
| edge_navigation_handles_five_thousand_lines_quickly | resource: 1000 vs 5000 lines, asserts linear scaling rather than a fixed wall-clock bound |
| edge_loop_and_object_scopes_do_not_pop_the_enclosing_function_scope | nesting: a loop/object `end` closes only its own scope |
| edge_for_loop_variable_does_not_leak_into_the_enclosing_scope | nesting: a loop variable is scoped to the loop body |
| edge_else_if_inside_a_function_keeps_the_function_scope_balanced | nesting: `else if` balances one `If` frame and the function closes its own scope |
| edge_multiple_assignment_declares_every_name_before_to | every name in `set a, b to ...` is declared |
| edge_typed_and_of_parameters_do_not_declare_their_type_names | typed/`of` parameters skip `of`, `list` and `number` |
| edge_catch_binders_are_the_name_after_the_handler_word | plain and typed catch binders, goto and references |
| edge_membership_has_does_not_declare_the_name_after_it | membership `has` is not a property declaration |
| edge_range_with_a_call_lower_bound_does_not_push_a_function_frame | range `to` is not a function declaration |
| edge_range_with_an_lru_cache_lower_bound_does_not_push_a_function_frame | an ordinary `lru_cache` variable before a range's `to` is not a decorator |
| edge_parameterless_function_literal_end_keeps_the_enclosing_scope | a keyword-started literal's `end` closes the literal, not the function around it |
| malformed_positions_are_clean_errors_not_panics | out of bounds / malformed position; asserts `is_err()` |
| edge_malformed_source_returns_errors_not_panics | malformed source: all three surface a lexer error as `Err` |
| navigation_json_extends_the_line_column_shape | JSON `{line, column, ...}` contract for all three |

### Mandatory edge-case matrix

| Row | Covered? |
|---|---|
| empty | covered by `edge_empty_source_answers_without_failing` |
| singleton | covered by the one-use fixtures; a single reference is asserted exactly |
| boundary | covered by EOF-without-newline; cursor at column `len + 1`; end-of-comment and unterminated-comment positions |
| out_of_bounds | covered by `malformed_positions_are_clean_errors_not_panics` (line 0, line past EOF, column 0, column past line) and by `edge_navigation_rejects_a_column_past_the_line` |
| type_mismatch | N/A: navigation consumes source text and positions only, never values |
| numeric_boundary | N/A: no arithmetic or numeric values are evaluated |
| unicode | covered by `edge_goto_definition_counts_unicode_names_as_characters` (span is four characters, not five bytes) |
| nesting_recursion | covered by the shadowing test (function scope nested in top-level scope) and by the loop/object-in-function tests (each block closes only its own scope) |
| duplicate_missing_keys | N/A: no records/maps are navigated; repeated `set` reuses its binding, including the enclosing binding it writes through to |
| malformed_input | covered by `edge_malformed_source_returns_errors_not_panics` (unterminated string surfaces `Err` from all three) and by `malformed_positions_are_clean_errors_not_panics`; comment/string context checks cover `//` inside text |
| resource_limit | covered by the 5000-line benchmark assertion |

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2779 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh "phase-075" | pass |

## Invariants touched

- None. The public `Value`, `Error`, grammar, `.rb` extension and existing tests are unchanged. The only public additions are new `redblue::lsp` items re-exported from `redblue`.

## Known gaps / follow-ups

- The functions implement capabilities only; the LSP transport/server loop remains phase-017 scaffolding by design.
