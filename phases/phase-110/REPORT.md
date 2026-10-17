# Phase 110 — Let `this` be the operand of `at` and `of`

## What changed

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +12 −0 | `TokenKind::This` added to the `matches!` arm list of `Parser::kind_starts_operand` (`src/parser.rs:1876`), with the reason the table and `parse_primary` disagreed |
| `bootstrap/compiler.rb` | +7 −0 | `"this"` added to `is_operand_start` (`bootstrap/compiler.rb:1189`), the stage-2 copy of the same table |
| `tests/this_operand_test.rs` | +804 (new) | 16 tests: the AST shape, the runtime answers, and the two operand tables read as text and compared |
| `tests/bootstrap_selfhost_test.rs` | +54 −0 | 1 test: `edge_this_is_a_worded_operand_on_both_compilers`, five programs through stage 1 / stage 2 byte identity, two of them written to fail on pre-fix stage 2 |
| `tests/unreviewed_work_ack_test.rs` | +1 −0 | the `8a5587d` resume-preserved-work commit acknowledged with its measured reason (this run: `unreviewed_work_in_head_history_is_acknowledged` was **FAILING** until this line was added) |

The whole fix is one token per table. `this` was already a primary —
`Parser::parse_primary` reads it as the ordinary variable it is
(`src/parser.rs:2687`) — so the operand table was simply missing an entry that
`parse_primary` already had. Nothing else in the pipeline moved.

## Reproduction, on the parent commit

Built `rb` from `HEAD~1` (`git show HEAD~1:src/parser.rs`, then `cargo build
--bin rb`) and ran the two shapes:

```
$ target/debug/rb run target/tmp/p110/len.rb      # object B / has a / to can size() / give back length of this
Error: ParserError: Expected something to measure after `length of`
  --> target/tmp/p110/len.rb:4:19
4 |         give back length of this
  |                   ^

$ target/debug/rb run target/tmp/p110/at.rb       # give back this.xs at this
Error: AnalyzerError: Unknown variable 'at'; did you mean 'as'?
  --> target/tmp/p110/at.rb:4:27
4 |         give back this.xs at this
  |                           ^
```

The first is the parser refusing the operand. The second is the misparse: `at`
read as a variable name because `at_index_ahead` saw no operand after it, so
`give back this.xs at this` became four statements. Both were confirmed by
reading the parent source as well — `TokenKind::This` appears zero times in the
parent's `kind_starts_operand`, and `is_operand_start` in the parent's
`bootstrap/compiler.rb:1146` tests only `num text yes no nothing id lparen
lbrack lbrace not minus to`.

## The same two programs after the fix

```
$ target/debug/rb run target/tmp/p110/len.rb
Error: RuntimeError: length requires a list or text
  --> target/tmp/p110/len.rb:4:14
4 |         give back length of this
  |              ^

$ target/debug/rb run target/tmp/p110/at.rb
Error: RuntimeError: Index must be a number
  --> target/tmp/p110/at.rb:4:19
4 |         give back this.xs at this
  |                   ^

$ target/debug/rb compile target/tmp/p110/at.rb -o target/tmp/p110/at.rbc
$ target/debug/rb vm target/tmp/p110/at.rbc
Error: RuntimeError: Index must be a number
  --> 4:1
```

`length of this` now reaches the *same runtime check* `length of nothing`
reaches — `this` is the prototype record, and `length` takes a list or a text —
which is the shape the finding asked for: the parser stops being the thing that
decides. `this.xs at this` is now an index, so it fails on `xs[k]`'s check and
never on `Unknown variable 'at'`. The bytecode engine agrees.

## Tests added

17 new `#[test]` functions: 16 in `tests/this_operand_test.rs` (13 named
`edge_*`) and 1 in `tests/bootstrap_selfhost_test.rs`.

| Test | Edge class covered |
|---|---|
| `a_worded_length_of_this_is_the_call_length_this` | AST shape: `length of this` is one `Length` call, not a refusal |
| `a_worded_index_of_this_is_one_index_expression` | AST shape: `xs at this` is one `Index` node |
| `a_worded_index_of_this_in_free_code_is_parsed_and_refused_by_the_analyzer` | the parse is unconditional; a wrong operand is refused by the *analyzer*, naming `this` |
| `edge_length_of_this_reaches_the_runtime_check_for_zero_one_and_many_fields` | empty / singleton / many — 0, 1 and 3 fields all reach the same runtime check, on both engines |
| `edge_length_of_this_and_length_of_nothing_are_refused_by_the_same_rule` | asserts a failure: the two words are refused by *one* rule, not two |
| `edge_this_as_the_operand_of_at_is_an_index_with_a_bad_index` | asserts a failure: `Index must be a number`, and the message never contains `Unknown variable` |
| `edge_this_as_the_operand_of_at_reports_a_position` | the failure is positioned on the line the writer wrote, not unpositioned |
| `edge_this_is_the_operand_of_at_in_a_nested_object_and_a_function_literal` | nesting / recursion: nested object, and a function literal written inside a method |
| `edge_the_index_spellings_that_already_worked_keep_their_behaviour` | regression pin: `this.xs at 0`, `xs at 0`, `xs[0]`, `length of xs`, and `this.xs at 2` (last element) |
| `edge_this_xs_at_this_reports_index_must_be_a_number_on_both_engines` | empty list + both engines |
| `edge_this_as_an_at_operand_outside_a_method_is_still_an_unknown_variable` | the row that must not move: `xs at this` in free code is still `Unknown variable 'this'` |
| `edge_the_two_operand_tables_name_the_same_token_kinds` | **the test that could have caught this finding** — both tables read as text and compared |
| `edge_a_worded_index_off_this_past_the_end_is_a_clean_runtime_error` | out of bounds: index 99 on a 3-list, and index 0 on an empty list |
| `edge_a_worded_length_with_no_operand_is_still_refused_by_the_parser` | malformed input: a bare `length of` is still `Expected something to measure`, free code and in a method |
| `edge_this_is_in_both_operand_tables` | `this` named explicitly in each table, so the answer is a name and not a list |
| `edge_the_operand_table_is_still_the_expression_table_less_might_fail` | the phase-080 mistake stays fixed: the operand table did not become the expression table |
| `edge_this_is_a_worded_operand_on_both_compilers` (bootstrap_selfhost_test.rs) | both engines write the same bytes for `length of this`, `this.xs at this`, and three controls |

Every "both engines" assertion goes through `both_engines_agree`, which compares
tree-walk output/failure against `rb compile` + `rb vm` output/failure.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — exit 0, no output |
| `cargo clippy --all-targets -- -D warnings` | pass — no errors, no warnings |
| `cargo test --all-targets` | pass — **1802 passed, 0 failed, 0 ignored**, across 65 test binaries (`EXIT=0`) |
| `./rbops/verify.sh phase-110` | **NOT RUN — `rbops/` is not in this checkout.** `ls rbops` → `No such file or directory`; running it prints `bash: ./rbops/verify.sh: No such file or directory`. The pipeline that invokes this agent lives outside the working tree and this agent may not write outside it, so gate 4 was not executed locally. The first three gates above were run here and are reported as observed. |

Suite detail: the S3 fixed-point tests are in `bootstrap_selfhost_test` and pass —
`edge_the_self_hosted_compiler_compiles_itself_byte_identically`,
`edge_stage3_is_byte_identical_on_every_corpus_program`,
`edge_three_consecutive_self_compilations_are_byte_identical` (24 passed, 360 s).

Note on run time: `cargo test --all-targets` takes roughly 12 minutes locally
because three bootstrap binaries each do multiple full self-compilations. A 15
minute timeout on the whole command kills it; that is why the number above is
from a run to completion.

## Invariants touched

None. No file under `phases/INVARIANTS.md` (that file does not exist in this
checkout) and nothing in the AGENTS.md §2 table moved: `.rb` extension, `end`
terminators, `set x to`, `say`, the `Value` variants, the `Error` variants and
the trailing-comma / `{interp}` string syntax are all untouched. No public type
was renamed and no grammar rule was added — `this` was already a primary, and
this phase makes the operand table agree with the primary table instead of
disagreeing with it.

## Edge-case matrix

| Row | Status | Where |
|---|---|---|
| empty | covered | zero-field object (`edge_length_of_this_reaches_the_runtime_check_for_zero_one_and_many_fields`), empty list (`edge_this_xs_at_this_reports_index_must_be_a_number_on_both_engines`, `edge_a_worded_index_off_this_past_the_end_is_a_clean_runtime_error`) |
| singleton | covered | one-field object, same test |
| boundary | covered | index `0` and index `len-1` — `edge_the_index_spellings_that_already_worked_keep_their_behaviour` pins `at 0` and `at 2` on a 3-list |
| out_of_bounds | covered | `edge_a_worded_index_off_this_past_the_end_is_a_clean_runtime_error`: index 99 on length 3, index 0 on an empty list; both clean `RuntimeError`s naming the length |
| type_mismatch | covered | `edge_this_as_the_operand_of_at_is_an_index_with_a_bad_index` — a record used as an index is `Index must be a number`; `edge_length_of_this_reaches_the_runtime_check_for_zero_one_and_many_fields` — a record used as a measure is `length requires a list or text` |
| numeric_boundary | N/A | this phase adds a *token* to a parse table. It cannot introduce or move `0/0`, `1/0`, `-0.0`, `NaN`, `±Infinity`, `2^53±1` or `i64` overflow, because no arithmetic, no conversion and no numeric comparison is touched. `edge_the_index_spellings_that_already_worked_keep_their_behaviour` pins that the numeric operand path (`at 0`, `at 2`, `at 99`) is byte-for-byte what it was |
| unicode | N/A | the token is the ASCII keyword `this`; no string, encoding or identifier text is produced, split or compared anywhere in the change. The operand tables are read as Rust/Redblue *source text* and compared as `String` lists (`edge_the_two_operand_tables_name_the_same_token_kinds`), which is ASCII-only by construction — a Unicode operand row would be testing the harness, not the change |
| nesting_recursion | covered | `edge_this_is_the_operand_of_at_in_a_nested_object_and_a_function_literal` — nested object (2 scopes) and a function literal inside a method (a third scope), on both engines |
| duplicate_missing_keys | N/A | this phase reads no record keys and writes none. The records involved are objects with declared fields, and the failure asserted is about the operand's *type*, not about a missing or duplicated key |
| malformed_input | covered | `edge_a_worded_length_with_no_operand_is_still_refused_by_the_parser` — `length of` with nothing after it is still `Expected something to measure after \`length of\``, free code and inside a method, on both engines. That is the malformed-operand row for a worded form |
| resource_limit | N/A | the change adds one `matches!` arm to a token-kind test and one `is` test. It allocates nothing, recurses nothing and does not run anything on a loop; no new resource is introduced, so there is no new limit to cover. The self-compilation step budget is separately pinned by the pre-existing `edge_the_self_compilation_still_obeys_the_step_budget`, which passes |

## Known gaps / follow-ups

- **`./rbops/verify.sh phase-110` was never executed.** `rbops/` is absent from
  this checkout and is outside the directory this agent is permitted to write in.
  Gate 4 is the only gate not covered by evidence here. Nothing in this report
  claims it passed.
- **Two statements still split, not one.** The operand table covers `at` and
  `of`. If any other wording guard consults a *different* table to decide whether
  a word is an index, it can still split. `edge_this_as_an_at_operand_outside_a_method_is_still_an_unknown_variable`
  and `edge_a_worded_length_with_no_operand_is_still_refused_by_the_parser` bound
  the two shapes this phase touched; a third wording guard is untested here.
- **`length of this` is a runtime failure, not a value.** That is deliberate and
  is what the finding asked for (`this` is the prototype record; `length` takes a
  list or a text), but it means the phrase reads as valid source and only fails
  on execution. If SPEC.md is later tightened about `this`, this is the line to
  revisit.
- **The table-parity test reads source text, not the compiled tables.** It finds
  a token added to one table and not the other; it will not find a token that is
  *missing from both*, which is exactly the shape this phase fixed. Nothing in
  the suite covers that direction today — a differential test cannot, because
  both engines omit it identically. → candidate for `FINDINGS.md`.
