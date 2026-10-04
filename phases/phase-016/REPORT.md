# Phase 016 — Linter must not produce false positives

## What changed

| File | Lines | What |
|---|---|---|
| `src/linter.rs` | +227 −82 | four rules with tests behind them: unused variable (fixed), unused import (new), shadowing (new), unparsable source (new) |
| `tests/linter_test.rs` | +560 (new) | 42 tests: a positive and a negative for every rule, the edge-case matrix, and an independent check that no corpus warning is a false positive |

In `src/linter.rs`:

- **Unused parameter false positive removed.** Function and method parameters
  are bound in the body scope for shadowing, but no longer enter the table of
  tracked variables. `to greet(name) / say "hello" / end` is silent.
- **Field-write false positive removed.** `set record.field to ...` marks the
  record used and binds no variable; the qualified name `record.field` was
  never a variable.
- **Unused import (new).** A file-level `import` whose module name is never
  mentioned is reported. An import inside a `try`, a loop or a `test` block is
  a statement run there, not a dependency of the file, and is not reported.
  `import files to f` is used if either `f` or `files` is read.
- **Shadowing (new).** A loop variable, a parameter or a `catch` binding that
  reuses a name bound by an enclosing scope is reported. `set` is assignment,
  not declaration, so `set n to n + 1` inside a loop shadows nothing.
- **Unparsable source (new).** `lint()` no longer swallows lexer and parser
  failures: it returns them as `LintError`, so `rb lint` exits 1 on a missing
  `end` instead of exiting 0 in silence.
- **Warnings are deterministic and located.** Findings were emitted in
  `HashSet` iteration order with `line: 0, column: 0`. They are now sorted by
  line, then column, then message, and carry the line of the definition they
  are about.
- Blocks, parameters and `catch` bindings now get their own scope, so a name
  bound in a nested scope is no longer confused with one bound outside it.

## Tests added

`tests/linter_test.rs`, 42 tests. Every rule has a test that fires and one that
stays quiet.

| Test | Edge class covered |
|---|---|
| `function_parameter_is_not_a_false_positive` | the reported false positive #1 |
| `field_assignment_is_not_a_false_positive` | the reported false positive #2 |
| `a_record_used_only_for_its_fields_is_not_reported` | property chain |
| `unused_variable_is_reported_with_its_line` | positive test, rule fires |
| `a_variable_that_is_read_is_not_reported` | negative test |
| `an_unused_object_is_reported_like_any_other_variable` | declaration form |
| `an_underscore_prefixed_name_is_left_alone` | `_` convention |
| `an_unused_file_level_import_is_reported` | positive test |
| `an_import_the_file_uses_is_not_reported` | negative test |
| `edge_import_used_through_its_alias_is_not_reported` | alias form |
| `edge_import_used_through_its_original_name_is_not_reported` | alias leaves the original bound |
| `edge_import_inside_a_block_is_not_a_file_dependency` | import in `test`/`try`/loop |
| `a_loop_variable_that_hides_an_outer_one_is_reported` | positive test, shadowing |
| `edge_a_loop_variable_with_a_name_of_its_own_is_not_reported` | negative test |
| `reassignment_inside_a_loop_is_not_shadowing` | fizzbuzz pattern, must stay silent |
| `edge_a_parameter_that_hides_an_outer_variable_is_reported` | shadowing, signature |
| `edge_a_catch_binding_that_hides_an_outer_variable_is_reported` | shadowing, `catch` |
| `a_missing_end_is_reported_as_an_error` | **asserts a failure**: `errors.len() == 1`, message names `End` |
| `a_wellformed_program_reports_no_errors` | negative test |
| `edge_empty_and_whitespace_only_source_lints_clean` | empty / `0` lines / comment only |
| `edge_bom_and_crlf_source_lints_clean` | BOM, CRLF |
| `edge_unterminated_string_is_an_error_and_not_a_panic` | **asserts a failure**, malformed input |
| `edge_stray_token_is_an_error_and_not_a_panic` | **asserts a failure**, stray token |
| `edge_unclosed_list_is_an_error_and_not_a_panic` | **asserts a failure**, unclosed bracket |
| `edge_deeply_nested_blocks_are_a_diagnostic_not_a_stack_overflow` | resource limit, 500 blocks |
| `edge_deeply_nested_expressions_are_a_diagnostic_not_a_stack_overflow` | resource limit, 500 parens |
| `edge_unicode_source_reports_the_right_line` | unicode: emoji, CJK-free accented, RTL |
| `edge_unicode_variable_names_are_linted` | unicode identifier |
| `edge_warning_order_is_stable_and_follows_the_source` | determinism: 8 runs, 4 names |
| `edge_empty_and_singleton_collections_are_linted` | empty list, singleton list, `nothing` |
| `edge_an_unread_loop_variable_is_reported` | singleton/boundary of the loop rule |
| `edge_numeric_and_index_edges_are_not_lint_errors` | numeric boundary, out of bounds, type mismatch |
| `edge_a_record_with_a_repeated_key_is_not_a_lint_error` | duplicate keys |
| `edge_a_missing_field_is_not_a_lint_error` | missing key |
| `edge_a_long_source_is_linted_without_truncation` | resource limit, 2000 lines |
| `nested_scopes_do_not_leak_names_into_each_other` | nesting: block → loop → block |
| `edge_a_name_bound_in_a_nested_scope_only_is_still_reported` | nesting, negative side |
| `the_corpus_produces_no_lint_errors` | every file in `examples/`, `tests/` parses |
| `the_examples_lint_without_warnings` | specification-by-example is silent |
| `edge_no_unused_variable_warning_in_the_corpus_names_a_read_variable` | **asserts a failure** if the corpus is misreported: re-reads the source text and fails if a warned name is read after its definition |
| `edge_no_unused_import_warning_in_the_corpus_names_a_used_module` | same, for imports |
| `edge_every_shadow_warning_in_the_corpus_hides_a_real_outer_binding` | same, for shadowing |

The last three are the "no false positives across examples/ and tests/" check.
They do not ask the linter whether it was right: they read each file's text,
strip comments, text literals, `a.b` property names and record keys, and fail if
a name the linter called unused is mentioned as a read anywhere after its
definition.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, no warnings |
| `cargo test` | 365 passed, 0 failed, 0 ignored (42 of them new in `tests/linter_test.rs`) |
| `./rbops/verify.sh phase-016` | **not run: `rbops/` is not in this checkout** |

`rbops/verify.sh` does not exist in the working directory, so the fourth gate
could not be executed. The three cargo gates above are the same commands the
phase prompt lists. The check was run and its absence is a fact about the
checkout, not a claim about the gate's verdict.

Behaviour checked on the real binary after building it:

```
$ ./target/debug/rb lint target/tmp/lint/missing_end.rb   # 'if ... then' with no end
Error: Syntax error: Expected End but got Eof             exit=1
$ ./target/debug/rb lint target/tmp/lint/unused.rb       # 'set spare to 1' + say "hi"
Warning: Unused variable: 'spare'                         exit=0
$ ./target/debug/rb lint target/tmp/lint/g.rb            # to greet(name) with unread param
                                                          exit=0, no output
$ ./target/debug/rb lint target/tmp/lint/bad.rb           # non-UTF-8 bytes
Error reading file: stream did not contain valid UTF-8    exit=1
```

## Invariants touched

- None. No grammar, no `Value` variant, no `Error` variant, no `.rb` extension,
  no `end`-delimited block. `Value` and `Error` are unchanged; `Error::span` is
  read, never constructed. `redblue::linter::lint` keeps its signature and now
  fills in the `errors` half of its return value, which it previously always
  left empty.
- No existing test was weakened, skipped or removed; the suite has 365 tests and
  zero `#[ignore]`.

## Known gaps / follow-ups

- **`modules/MathUtils.rb` cannot be linted.** It uses `constant PI to 3.14`,
  which `parse_callable` (`src/parser.rs:815`) rejects, so the file does not
  parse. The module loader skips unparsable modules, so no gate sees it. Adding
  `constant` is a grammar change and belongs to its own phase →
  `phases/phase-016/FINDINGS.md` F1.
- **A defined and never called function is not reported.** `defined_functions`
  is collected and never read. Redblue functions are first class, so the rule
  cannot be written from the syntax tree alone → FINDINGS F2.
- **`tests/*.rb` carries 23 true-positive unused-variable warnings and one
  true-positive shadowing warning.** Each was verified by the corpus
  cross-checks above; `tests/test_text.rb:38` documents the `name` case and
  `tests/test_closures.rb:50` is *about* the shadowed parameter. → FINDINGS F3.
- **`rb lint` does not print line numbers.** `lib.rs:145` prints
  `warning.message` only, so the line the linter now computes is not shown to a
  person at the terminal. Fixing that touches `src/lib.rs` output format, which
  no test pins, so it was left to a phase that owns CLI output.
- **An import inside a block is never reported as unused**, even when the
  surrounding file never uses the module. Chosen on purpose: `tests/test_modules.rb`
  imports modules to prove they resolve, and that is the statement under test.

## Edge-case matrix

| Row | Covered | By |
|---|---|---|
| empty | yes | `edge_empty_and_whitespace_only_source_lints_clean`, `edge_empty_and_singleton_collections_are_linted` |
| singleton | yes | `edge_empty_and_singleton_collections_are_linted`, `edge_an_unread_loop_variable_is_reported` |
| boundary | yes | same two tests, plus `edge_unicode_source_reports_the_right_line` for the line boundary |
| out_of_bounds | yes | `edge_numeric_and_index_edges_are_not_lint_errors` (`items[999]`, `items[-1]`) — the linter must stay out of runtime's business |
| type_mismatch | yes | `edge_numeric_and_index_edges_are_not_lint_errors` (`1 + "one"`) |
| numeric_boundary | yes | `edge_numeric_and_index_edges_are_not_lint_errors` (`2^53+1`, `i64::MIN`, `1e308`, `1 / 0`) |
| unicode | yes | `edge_unicode_source_reports_the_right_line`, `edge_unicode_variable_names_are_linted` |
| nesting_recursion | yes | `nested_scopes_do_not_leak_names_into_each_other`, `edge_a_name_bound_in_a_nested_scope_only_is_still_reported`, `edge_deeply_nested_blocks_...` |
| duplicate_missing_keys | yes | `edge_a_record_with_a_repeated_key_is_not_a_lint_error`, `edge_a_missing_field_is_not_a_lint_error` |
| malformed_input | yes | `edge_unterminated_string_...`, `edge_stray_token_...`, `edge_unclosed_list_...`, `a_missing_end_is_reported_as_an_error`, `edge_bom_and_crlf_source_lints_clean`, non-UTF-8 via the binary above |
| resource_limit | yes | `edge_deeply_nested_blocks_are_a_diagnostic_not_a_stack_overflow` (500), `edge_deeply_nested_expressions_...` (500), `edge_a_long_source_is_linted_without_truncation` (2000 lines) |
