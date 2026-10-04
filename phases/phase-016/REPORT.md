# Phase 016 — Linter must not produce false positives

The previous attempt's work is kept and is not redone. This run re-verified the
four gates, then found and fixed one false positive it had missed, removed a
blind spot in the check that was supposed to catch it, and closed the
`rb lint` line-number gap it had listed as a follow-up.

## What changed

| File | Lines | What |
|---|---|---|
| `src/linter.rs` | +7 −1 | `Statement::Object` now reads the name in `extends` instead of dropping it |
| `src/lib.rs` | +6 −2 | `rb lint` prints the line of every warning and every error |
| `tests/linter_test.rs` | +101 −1 | 6 new tests, and `is_binding_mention` no longer calls `extends` a binding |

In `src/linter.rs:278-292`:

```rust
Statement::Object { name, extends, body } => {
    self.declare(name, span);
    if let Some(parent) = extends {
        self.use_name(parent);
    }
    self.analyze_body(body);
}
```

`object Child extends Parent` reads `Parent`: the VM walks `self.objects` when
the declaration runs (`src/vm.rs:1419`) and the analyser rejects an undeclared
parent (`src/analyzer.rs:188`). Before this change `extends: _` threw the name
away and the parent was reported as an unused variable. Reproduced on the
real binary:

```
$ cat target/tmp/lint/extends.rb
object One
end

object Two extends One
end

say "hi"

$ ./target/debug/rb lint target/tmp/lint/extends.rb   # before
Warning: Unused variable: 'One'                       # false positive
Warning: Unused variable: 'Two'                       # true positive
```

That was five warnings in `tests/test_objects.rb` (2) and
`tests/test_object_model.rb` (3). After the fix both files are silent, and the
corpus walk in `tests/linter_test.rs:436` no longer sees them.

In `tests/linter_test.rs:523`, `is_binding_mention` no longer contains
`(Some("extends"), _)`. `extends` binds nothing, and classifying the parent as a
binding is why the corpus cross-check passed while the false positive was live.
With the checker corrected and the fix reverted,
`edge_no_unused_variable_warning_in_the_corpus_names_a_read_variable` fails:

```
tests/test_object_model.rb:74 claims 'One' is unused but it is read on [80]
```

That experiment was run and the fix restored; `cargo test --test linter_test`
is green with both in place.

In `src/lib.rs:142-155`, `rb lint` prints `Warning: line 47: Unused variable:
'past_end'` instead of `Warning: Unused variable: 'past_end'`. The linter has
computed the line since the previous attempt and `run_cli` discarded it; a file
with five findings gave no way to find any of them. The path is not repeated —
the reader named the file. No other output changed and no test pinned the old
string: `grep -rn "Warning:" src tests` now matches the single `eprintln!` at
`src/lib.rs:150` and matched nothing in `tests/` before this run either.

## Tests added

6 tests, all in `tests/linter_test.rs`. `tests/linter_test.rs` now has 48.

| Test | Edge class covered |
|---|---|
| `a_parent_object_named_by_extends_is_not_reported` | the false positive this run found |
| `edge_every_parent_in_a_chain_of_extends_is_read` | nesting: three-level `extends` chain, every parent read |
| `edge_the_leaf_of_an_extends_chain_is_still_reported_when_unread` | negative side: reading the parent must not mark the child used |
| `edge_extends_of_an_undeclared_parent_is_still_a_plain_read` | malformed/unknown parent: only the child is reported |
| `edge_rb_lint_prints_the_line_of_every_warning` | boundary: warnings alone still exit 0, and the first and third warnings name line 1 and line 3 |
| `edge_rb_lint_prints_the_line_of_a_syntax_error_and_exits_nonzero` | **asserts a failure**: exit 1, message names `End` and line 4 |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, no warnings |
| `cargo test --all-targets` | 371 passed, 0 failed, 0 ignored (48 of them in `tests/linter_test.rs`, 6 of those new this run) |
| `./rbops/verify.sh phase-016` | **not run: `rbops/` is not in this checkout** |

`rbops/verify.sh` does not exist in the working directory (`ls rbops` →
`No such file or directory`), so the fourth gate could not be executed. The
three cargo gates are the commands the phase prompt lists, run as written. The
absence of the script is a fact about the checkout, not a claim about its
verdict.

Checked on the real binary after building it:

```
$ for f in examples/*.rb; do ./target/debug/rb lint "$f"; done
(no output — examples/ is silent)

$ ./target/debug/rb lint tests/test_lists.rb
Warning: line 124: Unused variable: 'item'
Warning: line 197: Unused variable: 'bad'

$ ./target/debug/rb lint target/tmp/lint/missing_end.rb
Error: line 4: Syntax error: Expected End but got Eof    exit=1

$ for f in examples/*.rb; do ./target/debug/rb run "$f"; done
(all 6 examples run, exit 0)
```

## Invariants touched

- None. No grammar, no `Value` variant, no `Error` variant, no `.rb` extension,
  no `end`-delimited block. `Statement::Object` is matched more completely, not
  differently. `redblue::linter::lint` keeps its signature and its `(errors,
  warnings)` shape; both were already computed and only the CLI printing
  changed.
- No existing test was weakened, skipped, removed or re-scoped. `grep -rn
  "#\[ignore\]\|allow(clippy::" src tests` returns nothing.

## Definition of done

| Item | Status |
|---|---|
| zero false positives across `examples/` and `tests/` | `examples/` silent. `tests/` reports 17 unused variables and 1 shadowed parameter, all verified true positives — see FINDINGS F3 for the file:line of each group |
| every rule has a positive and a negative test | unused variable, unused import, shadowing, unparsable source each have both, in `tests/linter_test.rs` |
| rule for shadowing, unused variable, unused import, missing end | all four; `missing end` surfaces as a `LintError` from `lint()` (`src/linter.rs:403`), so `rb lint` exits 1 |
| lint never panics on malformed source | `edge_unterminated_string_...`, `edge_stray_token_...`, `edge_unclosed_list_...`, `edge_bom_and_crlf_...`, `edge_deeply_nested_...`; non-UTF-8 bytes are rejected by `run_cli` before the linter sees them |

## Known gaps / follow-ups

- **`modules/MathUtils.rb` cannot be linted.** It uses `constant PI to 3.14`,
  which `parse_callable` (`src/parser.rs:815`) rejects. Adding `constant` is a
  grammar change → FINDINGS F1.
- **A defined and never called function is not reported.**
  `defined_functions` (`src/linter.rs:21`) is collected and never read; the rule
  cannot be written from the syntax tree alone because Redblue functions are
  first class → FINDINGS F2.
- **`rb lint` does not recurse into a directory.** It takes exactly one path, as
  it always has. The corpus walk is done by
  `tests/linter_test.rs:436` instead.
- **An import inside a block is never reported as unused**, even when the
  surrounding file never uses the module. Chosen on purpose:
  `tests/test_modules.rb` imports modules to prove they resolve, and that is the
  statement under test.
- **A shadowed `catch` binding is reported as `Parameter 'x' shadows an outer
  variable`.** It is a binding, not a parameter; the wording came from
  `bind_signature` and was not worth a separate message for one case.

## Edge-case matrix

| Row | Covered | By |
|---|---|---|
| empty | yes | `edge_empty_and_whitespace_only_source_lints_clean`, `edge_empty_and_singleton_collections_are_linted` |
| singleton | yes | `edge_empty_and_singleton_collections_are_linted`, `edge_an_unread_loop_variable_is_reported` |
| boundary | yes | same two, plus `edge_every_parent_in_a_chain_of_extends_is_read` (a chain of three, the parent named at each step) and `edge_unicode_source_reports_the_right_line` |
| out_of_bounds | yes | `edge_numeric_and_index_edges_are_not_lint_errors` (`items[999]`, `items[-1]`) — the linter must stay out of the runtime's business |
| type_mismatch | yes | `edge_numeric_and_index_edges_are_not_lint_errors` (`1 + "one"`) |
| numeric_boundary | yes | `edge_numeric_and_index_edges_are_not_lint_errors` (`2^53+1`, `i64::MIN`, `1e308`, `1 / 0`) |
| unicode | yes | `edge_unicode_source_reports_the_right_line` (emoji, accented, RTL), `edge_unicode_variable_names_are_linted` |
| nesting_recursion | yes | `nested_scopes_do_not_leak_names_into_each_other`, `edge_a_name_bound_in_a_nested_scope_only_is_still_reported`, `edge_deeply_nested_blocks_...`, `edge_every_parent_in_a_chain_of_extends_is_read` |
| duplicate_missing_keys | yes | `edge_a_record_with_a_repeated_key_is_not_a_lint_error`, `edge_a_missing_field_is_not_a_lint_error` |
| malformed_input | yes | `edge_unterminated_string_...`, `edge_stray_token_...`, `edge_unclosed_list_...`, `a_missing_end_is_reported_as_an_error`, `edge_bom_and_crlf_source_lints_clean`, `edge_extends_of_an_undeclared_parent_...`, non-UTF-8 bytes via the binary |
| resource_limit | yes | `edge_deeply_nested_blocks_are_a_diagnostic_not_a_stack_overflow` (500), `edge_deeply_nested_expressions_...` (500), `edge_a_long_source_is_linted_without_truncation` (2000 lines) |
