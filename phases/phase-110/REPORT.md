# Phase 110 — Let `this` be the operand of `at` and `of`

## What changed

`this` was already a primary (`Parser::parse_primary`), but the separate table
that decides whether the operand of a worded form may start did not name it. The
two tables disagreed, so the same word was a value everywhere except after `at`
and `of`. This phase adds the one missing arm to each engine's operand table.

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +9 −0 | `TokenKind::This` added to `Parser::kind_starts_operand`, with the two failures the omission produced recorded beside the arm, and the operand-table doc updated to say `this` is included although `kind_starts_expression` does not name it |
| `bootstrap/compiler.rb` | +7 −0 | `if is_operand_start_k is "this"` added to `is_operand_start`, the stage-2 copy of the same table |
| `tests/index_words_test.rs` | +250 −0 | 8 tests, all named `edge_*`: the AST shape, the runtime check, both engines, the nested/function-literal scopes, the unchanged spellings, the malformed operand, and the two operand tables read as text and compared |
| `tests/bootstrap_selfhost_test.rs` | +36 −0 | 1 test, `edge_this_is_a_worded_operand_on_both_compilers`, comparing stage 1 and stage 2 byte for byte on five programs |

No other production code moved: no grammar rule was added, no token was reserved,
and `parse_primary` already read `this`.

## Reproduction

Before the change, on this tree (`target/debug/rb`, HEAD `e494fd2`):

```text
$ rb run len.rb
Error: ParserError: Expected something to measure after `length of`
  --> len.rb:4:19
4 |         give back length of this
  |                   ^

$ rb run at.rb
Error: AnalyzerError: Unknown variable 'at'; did you mean 'as'?
  --> at.rb:4:27
4 |         give back this.xs at this
  |                           ^
```

`len.rb` is an object whose method runs `give back length of this`; `at.rb` is
an object whose method runs `give back this.xs at this`. The second is the
phase-080 defect again: `at this` was split into two statements, so `at` stayed a
name.

After the change the same programs reach the runtime checks the spellings stand
for: `length requires a list or text` (the same rule `length of nothing` reaches)
and `Index must be a number`, positioned on the operand. Neither reports
`Unknown variable 'at'`. The already-working `this.xs at 0` still answers `1`.
These outcomes are asserted by the tests below; the bytecode engine is asserted
to produce the same label and message.

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_length_of_this_is_one_worded_length` | AST shape: `length of this` is one `length` call whose operand is `this` |
| `edge_length_of_this_reaches_the_runtime_check_for_zero_one_and_many_fields` | empty / singleton / many — 0, 1 and 3 fields all reach `length requires a list or text`, on both engines |
| `edge_this_is_the_operand_of_a_worded_index` | AST shape plus **asserts a failure**: one `Index` by `this`, a clean `Index must be a number`, and never `Unknown variable` |
| `edge_this_is_the_operand_of_at_in_a_nested_object_and_a_function_literal` | nesting: a nested object's method (both engines) and a function literal written inside a method |
| `edge_the_index_spellings_that_already_worked_keep_their_behaviour` | regression pin: `xs at 0`, `xs[0]`, `length of xs`, `this.xs at 0`, `this.xs at 2` |
| `edge_a_worded_length_with_no_operand_is_still_refused_by_the_parser` | **asserts a failure**: bare `length of` is still `Expected something to measure`, free code and in a method |
| `edge_this_as_an_at_operand_outside_a_method_is_still_an_unknown_variable` | the row that must not move: `xs at this` in free code is still `Unknown variable 'this'` |
| `edge_the_two_operand_tables_name_the_same_token_kinds` | reads `src/parser.rs` and `bootstrap/compiler.rb` as text, compares both operand tables, and requires `this` in each |
| `edge_this_is_a_worded_operand_on_both_compilers` (bootstrap_selfhost_test.rs) | stage 1 and stage 2 byte-identical for `length of this`, `this.xs at this`, and three controls |

Observed: `cargo test --test index_words_test` → **30 passed; 0 failed** (22
pre-existing + 8 new).
Observed: `cargo test --test bootstrap_selfhost_test edge_this_is_a_worded_operand_on_both_compilers`
→ **1 passed; 0 failed**.

## Edge-case matrix

| Row | Status | Where |
|---|---|---|
| empty | covered | zero-field object and empty list in the two runtime tests |
| singleton | covered | one-field object, same test |
| boundary | covered | index `0` and index `len-1` (`this.xs at 0`, `this.xs at 2`) |
| out_of_bounds | N/A | the change adds a token to a parse gate; no index arithmetic moved. The pre-existing `index_bounds_test.rs` still covers indexing |
| type_mismatch | covered | a record used as a length (`length of this`) and as an index (`this.xs at this`) are clean runtime errors |
| numeric_boundary | N/A | no arithmetic, conversion or numeric comparison is touched |
| unicode | N/A | the token is the ASCII keyword `this`; no text is produced, split or compared by the change |
| nesting_recursion | covered | nested object and function literal inside a method |
| duplicate_missing_keys | N/A | no record key is read or written by the change |
| malformed_input | covered | a bare `length of` is still a positioned parser error |
| resource_limit | N/A | the change allocates nothing, recurses nothing and adds no loop; it adds one `matches!` arm and one `if` |

## Gates

Run in this checkout, on this tree, this run.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — exit 0, no output |
| `cargo clippy --all-targets -- -D warnings` | pass — exit 0, no warnings |
| `cargo test --all-targets` | pass — exit 0, 3053 passed, 0 failed |
| `../rbops/verify.sh "phase-110"` | pass — report, diff hygiene, fmt, clippy, test (3053 passed, 0 failed), doc-test sweep and test policy checks all passed. The report-honesty check emitted a warning that the pipeline's parent `phases/phase-110/REPORT.md` claims 16 tests; that file is the previous attempt's and is outside this checkout. This report gives the observed 3053. |

## Invariants touched

- None. `.rb`, `end` terminators, `set x to`, `say`, the `Value` variants, the
  `Error` variants and the trailing-comma / `{interp}` string syntax are
  untouched. `this` was already a primary; the operand table now agrees with it.

## Known gaps / follow-ups

- The operand table now names `this` while `kind_starts_expression` still does
  not. That table decides where a *statement* may begin and how `give back` is
  split, not whether an operand may begin; `this` reaches the parser through the
  fallback expression-statement path there, and this phase deliberately did not
  change it.
- No other gap found.
