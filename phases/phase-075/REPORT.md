# Phase 075 — Teach the language server navigation: goto-definition, references, completion

## What changed

| File | Lines | What |
|---|---|---|
| src/lsp.rs | +918 −1 | `Location`, `Definition`, `DefinitionKind`; token-level lexical scope walk; `goto_definition`, `find_references`, `completions` and their JSON forms; greedy string-literal context check; balanced `else if`, multi-name `set`, typed/`of` parameters, catch binders, object-only `has`, and range `to` recognition |
| src/lib.rs | +3 −2 | re-export the new navigation surface |
| tests/tooling_grammar_test.rs | +536 −2 | 19 navigation tests (16 `edge_*`) including shadowing, builtins, imports, Unicode, malformed source and positions, nested loop/object scopes, `else if`, multi-assignment, typed parameters, catch binders, membership `has`, range bounds and the 5000-line benchmark |

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
| edge_completions_inside_string_literal_are_empty | context awareness: inside text |
| edge_navigation_handles_five_thousand_lines_quickly | resource: 5000 lines, all three under one second |
| edge_loop_and_object_scopes_do_not_pop_the_enclosing_function_scope | nesting: a loop/object `end` closes only its own scope |
| edge_for_loop_variable_does_not_leak_into_the_enclosing_scope | nesting: a loop variable is scoped to the loop body |
| edge_else_if_inside_a_function_keeps_the_function_scope_balanced | nesting: `else if` balances one `If` frame and the function closes its own scope |
| edge_multiple_assignment_declares_every_name_before_to | every name in `set a, b to ...` is declared |
| edge_typed_and_of_parameters_do_not_declare_their_type_names | typed/`of` parameters skip `of`, `list` and `number` |
| edge_catch_binders_are_the_name_after_the_handler_word | plain and typed catch binders, goto and references |
| edge_membership_has_does_not_declare_the_name_after_it | membership `has` is not a property declaration |
| edge_range_with_a_call_lower_bound_does_not_push_a_function_frame | range `to` is not a function declaration |
| malformed_positions_are_clean_errors_not_panics | out of bounds / malformed position; asserts `is_err()` |
| edge_malformed_source_returns_errors_not_panics | malformed source: all three surface a lexer error as `Err` |
| navigation_json_extends_the_line_column_shape | JSON `{line, column, ...}` contract for all three |

### Mandatory edge-case matrix

| Row | Covered? |
|---|---|
| empty | covered by `edge_empty_source_answers_without_failing` |
| singleton | covered by the one-use fixtures; a single reference is asserted exactly |
| boundary | covered by EOF-without-newline; cursor at column `len + 1` |
| out_of_bounds | covered by `malformed_positions_are_clean_errors_not_panics` (line 0, line past EOF, column 0, column past line) |
| type_mismatch | N/A: navigation consumes source text and positions only, never values |
| numeric_boundary | N/A: no arithmetic or numeric values are evaluated |
| unicode | covered by `edge_goto_definition_counts_unicode_names_as_characters` (span is four characters, not five bytes) |
| nesting_recursion | covered by the shadowing test (function scope nested in top-level scope) and by the loop/object-in-function tests (each block closes only its own scope) |
| duplicate_missing_keys | N/A: no records/maps are navigated; repeated `set` in one scope reuses that binding |
| malformed_input | covered by `edge_malformed_source_returns_errors_not_panics` (unterminated string surfaces `Err` from all three) and by `malformed_positions_are_clean_errors_not_panics` |
| resource_limit | covered by the 5000-line benchmark assertion |

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2769 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh "phase-075" | pass |

## Invariants touched

- None. The public `Value`, `Error`, grammar, `.rb` extension and existing tests are unchanged. The only public additions are new `redblue::lsp` items re-exported from `redblue`.

## Known gaps / follow-ups

- The functions implement capabilities only; the LSP transport/server loop remains phase-017 scaffolding by design.
- A parameterless function literal written without parentheses (`to give back 42`) is not given its own static scope; parenthesised literals are. This is a scope-analysis limitation, not a parser change.
- The static resolver models an inner `set` as a new lexical binding in its scope. The interpreter's `set_var` writes through to an enclosing binding when one exists; the two differ only for dynamically-reaching writes, and navigation follows the editor convention of lexical shadowing.
