# Phase 016 — Linter must not produce false positives

Resumed run. The previous attempt's work is kept and was re-verified, not
redone. This run found the linter correct on every reachable branch of the
implemented grammar, so it changed no production code: what it added is the
coverage that was missing for six statement/expression forms no test reached,
two genuine findings recorded in `FINDINGS.md`, and this report.

## What changed

| File | Lines | What |
|---|---|---|
| `tests/linter_test.rs` | +252 −0 | 13 new tests covering 6 branches of `src/linter.rs` that no test and no corpus file reached |
| `phases/phase-016/FINDINGS.md` | +54 | F4 `import X as Y` is documented and not parsed; F5 the `finally` scope model differs between linter and analyser |

No file under `src/` is changed by this run. The finding the phase was opened
for — an unread parameter, a field write, an object named by `extends`, and a
parse error that never reached `errors` — is fixed in the tree and re-verified
below.

## What was re-verified, not taken on trust

`examples/` is silent under `rb lint` (6 files, no output) and all 6 still run
with exit 0. `rb test tests/suite.rb`: 19 total, 19 passed, 0 failed, 0
skipped. `rb lint tests/suite.rb` reports one warning, `line 88: Unused
variable: 'bad'`, which is the `set bad to 1 / 0` inside a `try` whose right
hand side faults — FINDINGS F3, a true positive.

Every reachable `match` arm of `src/linter.rs` is now reached by a test or by a
corpus file. The two that no source can produce are dead in the parser, not in
the linter: `Statement::ForRange` is never constructed (`src/parser.rs:134` is
its only occurrence) and `Expr::InterpolatedText` has no producer either, so
`{name}` in a text literal is literal text today, as FINDINGS F3 records.

## Tests added

13 tests, all in `tests/linter_test.rs`, which now has 61 (was 48).

Every one of them asserts a non-empty message list at least once, so none of
them can pass against a linter that reported nothing: where a test asserts
silence it is paired in the same body with a positive control that must warn.

| Test | Edge class covered |
|---|---|
| `an_unread_object_method_parameter_is_not_an_unused_variable` | `Statement::Method`: an unread `to can` parameter is a signature; a leftover in the same method body is still reported |
| `edge_a_method_parameter_that_hides_a_file_variable_is_reported` | shadowing, the true-positive side of `to can` |
| `edge_a_field_declaration_is_not_a_variable_binding` | `Statement::Has`: a field name is not a variable name (matches `src/analyzer.rs:176-180`) |
| `edge_a_field_default_reads_the_name_it_defaults_from` | `Has { default: Some }`: the default expression reads its operand |
| `edge_a_literal_field_default_reads_nothing` | `Has { default: Some }` with no name in it — empty default |
| `an_unclosed_object_is_an_error_and_not_a_panic` | **asserts a failure**: missing `end` on a block that is not `if`; one error, no line-by-line warnings |
| `edge_print_reads_what_it_prints` | `Statement::Print`: reads its operand, still reports a name it does not mention |
| `edge_a_unary_operand_counts_as_read` | `Expr::Unary`: both `-n` and `not n` |
| `edge_a_binding_made_in_a_finally_block_is_reported_when_unread` | `Try.finally_body`: unread name reported with the `finally` line |
| `edge_a_finally_binding_does_not_shadow_the_catch_binding` | nesting: `catch` and `finally` are siblings (`src/analyzer.rs:216-218`), plus a real shadow in a `finally` as the control |
| `edge_a_bare_give_back_is_neither_a_read_nor_a_finding` | `Return(None)` / `GiveBack(None)`; a leftover beside the bare `give back` is still reported |
| `edge_mutually_recursive_functions_do_not_leak_names_into_each_other` | nesting/recursion: two functions calling each other, both signs |
| `edge_three_nested_scopes_keep_their_own_names` | nesting: loop → loop → function, and the one name nobody reads |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, no warnings |
| `cargo test --all-targets` | 384 passed, 0 failed, 0 ignored (61 in `tests/linter_test.rs`, 13 of them new this run) |
| `./rbops/verify.sh phase-016` | **not run: `rbops/` is not in this checkout** (`ls rbops/verify.sh` → `No such file or directory`) |

The three cargo gates are the commands the phase prompt lists, run as written
and as reported above. The absence of `verify.sh` is a fact about this
checkout, not a claim about its verdict.

## Invariants touched

- None. No grammar, no `Value` variant, no `Error` variant, no `.rb` extension,
  no `end`-delimited block, no `redblue::` signature. This run edited one test
  file and one findings file.
- No existing test was weakened, skipped, removed or re-scoped.
  `grep -rn "#\[ignore\]\|allow(clippy::" src tests` returns 0 matches.

## Definition of done

| Item | Status |
|---|---|
| zero false positives across `examples/` and `tests/` | `examples/` silent. `tests/` reports 18 warnings, all true positives — FINDINGS F3 gives the file:line of each group, and the two corpus cross-checks read the sources independently of the linter |
| every rule has a positive and a negative test | unused variable, unused import, shadowing (variable), shadowing (parameter), unparsable source each have both, in `tests/linter_test.rs` |
| rule for shadowing, unused variable, unused import, missing end | all four. `missing end` surfaces as a `LintError` from `lint()` (`src/linter.rs:403`), so `rb lint` exits 1 |
| lint never panics on malformed source | 1210 truncated prefixes of every corpus file through the real binary: 0 panics, every exit code 0 or 1. In-process: `edge_unterminated_string_...`, `edge_stray_token_...`, `edge_unclosed_list_...`, `an_unclosed_object_...`, `edge_bom_and_crlf_...`, `edge_deeply_nested_...`; non-UTF-8 bytes are rejected by `run_cli` before the linter sees them |

## Known gaps / follow-ups

- **`import X as Y` is the documented alias and the parser reads `to`
  instead** → FINDINGS F4, with the reproduction. It is the one place where
  `rb lint` output can be true of the tree and wrong for the reader, and it
  cannot be fixed from the linter.
- **`modules/MathUtils.rb` cannot be linted** — `constant PI to 3.14` is not in
  the grammar (`parse_callable`, `src/parser.rs:815`) → FINDINGS F1.
- **A defined and never called function is not reported.** `defined_functions`
  (`src/linter.rs:21`) is collected and never read; Redblue functions are first
  class, so the rule cannot be written from the syntax tree alone →
  FINDINGS F2.
- **`rb lint` does not recurse into a directory.** It takes exactly one path, as
  it always has. The corpus walk is done by `tests/linter_test.rs` instead.
- **An import inside a block is never reported as unused**, even when the
  surrounding file never uses the module. Chosen on purpose: `tests/test_modules.rb`
  imports modules to prove they resolve, and that is the statement under test.
- **A shadowed `catch` binding is reported as `Parameter 'x' shadows an outer
  variable`.** It is a binding, not a parameter; the wording came from
  `bind_signature` and was not worth a separate message for one case.
- **`Error reading file: <io error>` does not name the path** (`src/lib.rs:160`).
  The reader named the file, so this matches the choice made for warnings, but
  it is an inconsistency with `Error: line N:`.

## Edge-case matrix

| Row | Covered | By |
|---|---|---|
| empty | yes | `edge_empty_and_whitespace_only_source_lints_clean`, `edge_empty_and_singleton_collections_are_linted`, `edge_a_literal_field_default_reads_nothing` |
| singleton | yes | `edge_empty_and_singleton_collections_are_linted`, `edge_an_unread_loop_variable_is_reported`, `edge_three_nested_scopes_keep_their_own_names` |
| boundary | yes | the two above, plus `edge_every_parent_in_a_chain_of_extends_is_read` (three-level chain), `edge_unicode_source_reports_the_right_line`, `edge_rb_lint_prints_the_line_of_every_warning` (first and third of three) |
| out_of_bounds | yes | `edge_numeric_and_index_edges_are_not_lint_errors` (`items[999]`, `items[-1]`) — index range is the runtime's business |
| type_mismatch | yes | `edge_numeric_and_index_edges_are_not_lint_errors` (`1 + "one"`) |
| numeric_boundary | yes | `edge_numeric_and_index_edges_are_not_lint_errors` (`2^53+1`, `i64::MIN`, `1e308`, `1 / 0`) |
| unicode | yes | `edge_unicode_source_reports_the_right_line` (emoji, accented, RTL), `edge_unicode_variable_names_are_linted` |
| nesting_recursion | yes | `nested_scopes_do_not_leak_names_into_each_other`, `edge_mutually_recursive_functions_do_not_leak_names_into_each_other`, `edge_three_nested_scopes_keep_their_own_names`, `edge_a_finally_binding_does_not_shadow_the_catch_binding`, `edge_every_parent_in_a_chain_of_extends_is_read` |
| duplicate_missing_keys | yes | `edge_a_record_with_a_repeated_key_is_not_a_lint_error`, `edge_a_missing_field_is_not_a_lint_error` |
| malformed_input | yes | `edge_unterminated_string_...`, `edge_stray_token_...`, `edge_unclosed_list_...`, `an_unclosed_object_...`, `a_missing_end_...`, `edge_bom_and_crlf_...`, `edge_extends_of_an_undeclared_parent_...`, non-UTF-8 bytes via the binary |
| resource_limit | yes | `edge_deeply_nested_blocks_are_a_diagnostic_not_a_stack_overflow` (500), `edge_deeply_nested_expressions_...` (500), `edge_a_long_source_is_linted_without_truncation` (2000 lines), and the linter's own recursion is bounded by `MAX_BLOCK_DEPTH` / `MAX_NESTING_DEPTH` (`src/parser.rs:243,252`), which refuse the source before the linter walks it |
