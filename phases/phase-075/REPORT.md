# Phase 075 — Teach the language server navigation: goto-definition, references, completion

## What changed

| File | Lines | What |
|---|---|---|
| src/lsp.rs | +1005 −1 | `Location`, `Definition`, `DefinitionKind`; token-level lexical scope walk; `goto_definition`, `find_references`, `completions` and their JSON forms; greedy string-literal and `//` comment context checks; balanced `else if`, multi-name `set`, typed/`of` parameters, catch binders, object-only `has`, range `to` recognition, decorator-position checks, and function-literal frames that close where the parsed one-line body ends |
| src/lsp.rs | +115 −31 | resumed run: `DeclarationSink` removes the clippy `too_many_arguments` failure without an `allow` suppression, then the review fixes: an assignment `to` is no longer a literal opener, object members resolve in the deferred pass, lexer errors keep their span, string/comment delimiters count as inside, builtin references exclude member/type names, goto excludes member names, CR/CRLF line starts and comment endings are handled, and an `unless` body opens its own lexical scope |
| src/lexer.rs | +6 −1 | a lone CR advances the line counter and ends a `//` comment, matching the line breaks the lexer already emits |
| src/lib.rs | +3 −2 | re-export the new navigation surface |
| tests/tooling_grammar_test.rs | +583 −37 | 48 navigation tests (44 `edge_*`) including shadowing, builtins, imports, Unicode, CR/CRLF endings, comments, malformed source and positions, nested `unless`/loop/repeat/module/test/try-catch/object scopes, `else if`, multi-assignment, typed parameters, catch binders, membership `has`, object member access (forward, `this`, ordinary), range bounds, function literals and deterministic navigation over 5000 declarations |
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
| edge_navigation_resolves_a_binding_after_five_thousand_lines | resource: a deterministic goto/references/completion answer at the last of 5000 declarations, with no wall-clock ratio |
| edge_loop_and_object_scopes_do_not_pop_the_enclosing_function_scope | nesting: a loop/object `end` closes only its own scope |
| edge_for_loop_variable_does_not_leak_into_the_enclosing_scope | nesting: a loop variable is scoped to the loop body |
| edge_unless_scope_does_not_leak_into_the_enclosing_scope | nesting: an `unless` body's declaration stays in its own scope and the enclosing binding survives its `end` |
| edge_while_scope_does_not_leak_into_the_enclosing_scope | nesting: a `while` body's declaration stays in its own scope and the enclosing binding survives its `end` |
| edge_repeat_scopes_do_not_leak_into_the_enclosing_scope | nesting: counted `repeat`/`end` and `repeat`/`until` both close only their own scope |
| edge_module_scope_does_not_leak_into_the_enclosing_scope | nesting: a module body's declaration stays inside it while the module name survives its `end` |
| edge_test_block_keeps_the_enclosing_scope_balanced | nesting: a `test` body reads the enclosing scope, its inner block does not leak, and the enclosing binding survives its `end` |
| edge_try_catch_scope_does_not_leak_into_the_enclosing_scope | nesting: a catch binder stays in its own scope and the enclosing binding survives the try's `end` |
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
| edge_goto_definition_on_a_declaration_returns_its_own_span | singleton: goto on the declaration names itself |
| edge_completions_on_an_empty_line_offer_keywords_builtins_and_scope | empty prefix on a blank line offers keywords, builtins and scope |
| edge_completions_after_a_function_end_exclude_its_parameters | nesting: a parameter does not leak past its `end` |
| edge_navigation_rejects_an_enormous_line_number | out of bounds / numeric boundary: `usize::MAX` line is a clean error; asserts `is_err()` |
| edge_assignment_to_inside_an_inline_block_keeps_the_function_scope_balanced | nesting: an assignment's `to` does not open a literal frame that steals the block `end` |
| edge_object_member_access_navigates_a_forward_reference | forward reference: `p.name` above `object p ... has name` resolves and joins the field's references |
| edge_object_member_access_navigates_this_and_ordinary_reference | object navigation: both `this.name` and `Person.name` resolve to `has name` and appear in its references |
| edge_builtin_references_exclude_member_and_type_names | context awareness: `holder.abs` and the type name in `items of abs` are not uses of the `abs` builtin, while the plain `abs` use still resolves to the registry |
| edge_goto_definition_on_a_member_name_does_not_resolve_to_a_builtin | context awareness: goto on `holder.abs` answers `None` while a plain `abs` call still answers the builtin |
| edge_navigation_handles_cr_and_crlf_line_endings | line endings: lone CR and CRLF line starts, comment termination, goto, references and completion after the comment |
| edge_completions_at_the_opening_quote_and_comment_marker_are_empty | boundary: a caret on the opening quote or on the first `/` of `//` is inside text/comment |
| edge_malformed_source_error_carries_the_lexer_span | malformed source: the error keeps both `Unterminated string` and its `1:5` span |

### Mandatory edge-case matrix

| Row | Covered? |
|---|---|
| empty | covered by `edge_empty_source_answers_without_failing` |
| singleton | covered by the one-use fixtures; a single reference is asserted exactly |
| boundary | covered by EOF-without-newline; cursor at column `len + 1`; end-of-comment and unterminated-comment positions; lone-CR and CRLF line endings |
| out_of_bounds | covered by `malformed_positions_are_clean_errors_not_panics` (line 0, line past EOF, column 0, column past line), by `edge_navigation_rejects_a_column_past_the_line`, and by `edge_navigation_rejects_an_enormous_line_number` (`usize::MAX`) |
| type_mismatch | N/A: navigation consumes source text and positions only, never values |
| numeric_boundary | N/A: no arithmetic or numeric values are evaluated |
| unicode | covered by `edge_goto_definition_counts_unicode_names_as_characters` (span is four characters, not five bytes) |
| nesting_recursion | covered by the shadowing test (function scope nested in top-level scope), by the loop/object-in-function tests, and by dedicated `unless`, `while`, counted/until `repeat`, `module`, `test` and `try`/`catch` fixtures: each block closes only its own frame/scope and the binding after it still resolves |
| duplicate_missing_keys | N/A: no records/maps are navigated; repeated `set` reuses its binding, including the enclosing binding it writes through to |
| malformed_input | covered by `edge_malformed_source_returns_errors_not_panics` (unterminated string surfaces `Err` from all three) and by `malformed_positions_are_clean_errors_not_panics`; comment/string context checks cover `//` inside text |
| resource_limit | covered by `edge_navigation_resolves_a_binding_after_five_thousand_lines`: a deterministic, fixed 5000-line source answers goto/references/completion at its last declaration; no wall-clock ratio is asserted |

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2797 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh "phase-075" | pass |

## Invariants touched

- None. The public `Value`, `Error`, grammar, `.rb` extension and existing tests are unchanged. The only public additions are new `redblue::lsp` items re-exported from `redblue`.

## Known gaps / follow-ups

- The functions implement capabilities only; the LSP transport/server loop remains phase-017 scaffolding by design.
