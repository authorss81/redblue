# Phase 110 — Let `this` be the operand of `at` and `of`

## What changed

`this` was already a primary in `parse_primary`, but two separate tables in each
engine did not name it: the worded-operand table (what follows `at` and `of`) and
the expression table (`give back`, `return`, `might fail`). The operand table was
fixed by the earlier attempt now committed at `c58ea84`; this run fixes the
missing expression row in both engines, so the tables agree with the primary they
describe.

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +6 −0 | `TokenKind::This` in `Parser::kind_starts_expression` (expression head), with a comment naming the split it prevents |
| `bootstrap/compiler.rb` | +6 −0 | `"this"` in `is_expression_start`, the stage-2 copy of the same table |
| `tests/index_words_test.rs` | +233 −8 | 7 tests (plus 2 text-reading helpers): the expression-head shape, both engines, the nested function literal, expression-table agreement, the outside-a-method failure, the `by this.sep` label consumer, and the one-line literal-body consumer. The 8 worded-operand tests listed below are pre-existing from `c58ea84`, not authored here |
| `tests/bootstrap_selfhost_test.rs` | +31 −1 | 1 new test plus a corrected cross-reference comment: expression-head cases through stage 1 / stage 2 byte identity |

### Why the expression table mattered too

Adding `this` only to the operand table left `give back this.xs at this` split at
the `give back` boundary: `give back` read as the valueless form and the
expression was left to the statement loop. That is the same shape as the
`nothing` defect already pinned in `tests/bootstrap_selfhost_test.rs`; the fix is
the same row in the same table. With `this` in the expression table, the value is
part of the `give back` / `return` statement and no bare `Expr(this …)` follows.

The operand table already contained `TokenKind::This` from the earlier attempt;
this run adds the missing expression row in both engines, tightens the AST helper
so a split fails instead of being silently accepted, and adds the table-agreement
test for the expression tables.

## Reproduction

Before the expression-table row, inside an object method:

```
give back this
```

parsed as a valueless `GiveBack(None)` followed by a separate statement carrying
`this`. The AST helper in the tests used to accept that fallback; it now requires
`GiveBack(Some(...))`, so a regression fails the parse-shape test rather than
passing as the same value.

After the change, on the tree-walking engine:

```
$ rb run target/tmp/this.rb       # give back this / return this
{x: 7}
{x: 7}
```

and the worded forms the phase names:

```
$ rb run target/tmp/len.rb        # give back length of this
Error: RuntimeError: length requires a list or text

$ rb run target/tmp/at.rb         # give back this.xs at this
Error: RuntimeError: Index must be a number

$ rb run target/tmp/ok.rb         # give back this.xs at 0
10
```

`length of this` reaches the same runtime rule as `length of nothing`, and
`this.xs at this` never produces `Unknown variable 'at'`. The bytecode engine
reports the same error kinds and messages, and the stage-1 / stage-2 byte
identity tests cover `give back this`, `return this`, the worded forms, and the
bracketed control.

## Tests added

7 new `#[test]` functions in `tests/index_words_test.rs` and 1 in
`tests/bootstrap_selfhost_test.rs`, plus the two private helpers
`expression_kinds_from_parser` and `expression_kinds_from_bootstrap`.

| New test | Edge class covered |
|---|---|
| `edge_this_heads_a_give_back_as_one_statement` | `give back this` / `return this` carry `this`; no bare `Expr` follows |
| `edge_this_heads_an_expression_on_both_engines` | `give back this` prints the record identically on tree and bytecode engines |
| `edge_this_in_a_function_literal_nested_in_a_method_runs_on_both_engines` | nesting: third scope, both engines |
| `edge_the_two_expression_tables_name_the_same_token_kinds` | both expression tables read as text and compared |
| `edge_a_worded_index_of_this_outside_a_method_is_err` | **asserts a failure** with `result.is_err()` and names the error |
| `edge_this_after_by_is_a_labelled_argument` | `at_argument_label` consumer: `split(text, by this.sep)` is one labelled call with two arguments |
| `edge_this_begins_a_one_line_literal_body_statement` | `can_begin_statement` consumer: `this` is the one-statement body of `to () this` |
| `edge_this_heads_an_expression_on_both_engines` (selfhost) | stage 1 / stage 2 byte identity for `give back this`, `return this`, and controls |

The two text-reading tests compare the actual token-kind lists in `src/parser.rs`
and `bootstrap/compiler.rb`, so either table drifting fails automatically.

These 9 tests already existed in the base file at `c58ea84` and are **not** part
of this change set:

| Pre-existing test | Edge class covered |
|---|---|
| `edge_length_of_this_is_one_worded_length` | AST shape: `length of this` is one `Length` call |
| `edge_length_of_this_reaches_the_runtime_check_for_zero_one_and_many_fields` | empty / singleton / many fields, both engines |
| `edge_this_is_the_operand_of_a_worded_index` | **asserts a failure**: `this.xs at this` is one index and fails on the index, never on `Unknown variable 'at'` |
| `edge_this_is_the_operand_of_at_in_a_nested_object_and_a_function_literal` | nesting / recursion: nested object and function literal inside a method |
| `edge_the_index_spellings_that_already_worked_keep_their_behaviour` | boundary: `at 0`, `at 2`, `xs[0]`, `length of xs` unchanged |
| `edge_a_worded_length_with_no_operand_is_still_refused_by_the_parser` | malformed operand: bare `length of` still refused |
| `edge_this_as_an_at_operand_outside_a_method_is_still_an_unknown_variable` | the row that must not move: free-code `this` is still unknown |
| `edge_the_two_operand_tables_name_the_same_token_kinds` | both operand tables read as text and compared |
| `edge_this_is_a_worded_operand_on_both_compilers` | stage 1 / stage 2 byte identity for worded operands |

## Gates

Run in this checkout, this run.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no output |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0, no warnings |
| `cargo test --all-targets` | **pass** — 3061 passed, 0 failed |
| `../rbops/verify.sh phase-110` | **pass** — all checks pass, including test policy and doc tests |

## Invariants touched

No grammar change: `this` was already a primary, and this phase makes the
expression table agree with it. One parser-internal surface change follows from
that, and this phase tests both consumers rather than claiming they are
unaffected:

- `at_argument_label` now reads `this` as an expression head, so
  `split(text, by this.sep)` is a labelled argument instead of a parse failure.
  Covered by `edge_this_after_by_is_a_labelled_argument`.
- `can_begin_statement` now allows `this` to begin a statement in a one-line
  function-literal body, so `to () this` has a one-statement body rather than an
  empty one that leaves `this` to the enclosing scope. Covered by
  `edge_this_begins_a_one_line_literal_body_statement`.

`.rb`, `end`, `set x to`, `say`, the `Value` and `Error` variants, and the string
syntax are untouched.

## Edge-case matrix

| Row | Status | Where |
|---|---|---|
| empty | covered | zero-field object; empty list index |
| singleton | covered | one-field object |
| boundary | covered | index `0` and `len-1` pinned |
| out_of_bounds | covered | index 99 on a 3-list, index 0 on an empty list: clean `RuntimeError` |
| type_mismatch | covered | record used as index/length: clean error, both engines |
| numeric_boundary | N/A | no arithmetic, conversion or numeric comparison is touched; `at 0`/`at 2`/`at 99` pinned unchanged |
| unicode | N/A | the token is the ASCII word `this`; no string or identifier text is produced or compared |
| nesting_recursion | covered | nested object and function literal inside a method, both engines |
| duplicate_missing_keys | N/A | no record key is read or written; failures concern the operand’s type |
| malformed_input | covered | bare `length of` still a positioned parser error |
| resource_limit | N/A | the change adds table rows; it allocates, recurses and loops no more than before |

## Known gaps / follow-ups

- The stage-2 compiler does not compile function literals as expressions
  (“not compiled yet”), so the function-literal runtime case is covered by the
  tree and bytecode engines in `tests/index_words_test.rs`, not by a stage-1 /
  stage-2 byte-identity assertion. This limitation pre-dates the phase.
- `tests/preflight_assert_failure.rs` is a bare `#[should_panic]` whose own
  comment says it exists only for the pipeline heuristic; it is not removed or
  relied on here.
