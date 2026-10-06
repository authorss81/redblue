# Phase 026 — Lex the comparison symbols `<` `>` `=` `!`

## Reproduction of the finding

The finding reproduced on this checkout, unchanged:

```
$ cargo run --bin rb -- run repro.rb     # repro.rb: set x to 20 / if x > 10 then / say "greater" / end
Error: LexerError: Unexpected character '>'
  --> repro.rb:2:6
2 | if x > 10 then
  |      ^
$ echo $?
1
```

`src/lexer.rs:398` is the operator table (`let kind = match c`). Before this
phase it had arms for `+ - * / % ( ) [ ] { } , . :` only; the `_ =>` arm raised
`Unexpected character`. There was no arm for `<`, `>`, `=` or `!`, so
`TokenKind::Equal`/`NotEqual`/`Less`/`LessEqual`/`Greater`/`GreaterEqual` — all
declared at `src/lexer.rs:137-144` — were never constructed by the lexer. The
comparison arm block in `Parser::parse_comparison` (`src/parser.rs:1207-1312`)
was unreachable, and `BinaryOp::Less`/`LessEqual`/`Greater`/`GreaterEqual` were
dead in the VM. The language had **no ordering comparison at all**.

`src/runtime.rs:185-225` already implemented all four ordering operators, and
`src/bytecode/{opcode,codegen,vm}.rs` already had the matching opcodes. Only the
lexer was missing, so the change is confined to the lexer.

## What changed

| File | Lines | What |
|---|---|---|
| `src/lexer.rs` | +43 −0 | Four arms in the operator table: `<` → `Less`/`LessEqual`, `>` → `Greater`/`GreaterEqual`, `=` → `Equal` (consuming a second `=`), `!` → `NotEqual` when an `=` follows, otherwise `Not` |
| `tests/comparison_lex_test.rs` | +790 (new) | 19 `#[test]` functions, 14 of them `edge_*` |
| `tests/test_comparisons.rb` | +186 (new) | 13 Redblue `test` blocks, 7 of them `edge_*` |

No production file other than `src/lexer.rs` was touched. No parser, analyzer,
VM, runtime, formatter or bytecode change was needed or made.

### Two decisions worth stating

**`!` alone is `Not`, not `NotEqual`.** `!=` is consumed as one `NotEqual`
token; a `!` with no `=` after it stays `TokenKind::Not`, which is the prefix
the parser already reads (`src/parser.rs:1226-1233`, for `x is not y`). Making
a bare `!` mean `!=` would have created two spellings of one operator and broken
`is not`. Pinned by `edge_malformed_operator_sequences_are_reported_not_guessed`.

**`=` and `==` are the same token.** `parse_comparison` (`src/parser.rs:1237-1241`)
already treats a bare `Equal` as equality, so the second `=` is consumed and
discarded. The token stream for `a==b` is `Equal`, not `Equal Equal`.

Both are consistent with the grammar `docs/GRAMMAR.md:93-98` and with the
alternative forms `SPEC.md:277-282` documents.

## Tests added

| Test | Edge class covered |
|---|---|
| `the_comparison_symbols_lex_to_their_own_token_kinds` | singleton — each of `<`, `>`, `=`, `==`, `!=` is exactly one token then Eof |
| `edge_less_equal_and_greater_equal_are_one_token_not_two` | boundary — `<=`/`>=` are one token in isolation and in a full statement |
| `each_comparison_operator_selects_the_branch_it_names` | happy path, both directions, all six |
| `edge_x_against_itself_is_strictly_less_and_strictly_greater` | boundary — `x<x` no, `x<=x` yes, `x>x` no, `x>=x` yes, over `5`, `0`, `-3`, `2.5` |
| `edge_reversed_operands_swap_the_answer_for_the_strict_operators` | boundary — reversed operands on all eight ordered pairs |
| `edge_ordering_operands_of_different_types_is_a_runtime_error` | type_mismatch — `1 < "a"`, `yes >= 3`, `nothing > 1`, and the reversed type order |
| `edge_ordering_a_list_or_record_against_a_number_is_a_runtime_error` | type_mismatch — `[1,2] < 3`, `{a:1} <= 1`, `[] > 1`, `[1] < [2]` |
| `edge_equality_of_two_lists_and_two_records_compares_contents` | empty / singleton / nesting — `==` on two lists and two records with equal contents, `[]`, `[7]`, `{}`, three-deep nesting |
| `edge_comparing_an_absent_record_key_orders_nothing_but_equals_it` | duplicate_missing_keys — absent key is `nothing`, unorderable but equal-comparable; duplicate key keeps the last value |
| `edge_text_ordering_is_a_runtime_error_on_both_sides` | unicode / type_mismatch — text ordering refused; text equality on `héllo`, `日本語`, `🎉` |
| `edge_numeric_boundaries_order_without_panicking` | numeric_boundary — signed zero, past `2^53`, `i64` extremes, largest finite double, and that NaN/∞ are refused before a comparison sees them |
| `edge_comparisons_compose_with_lists_loops_functions_and_records` | nesting_recursion — a comparison inside a `for` body, inside a function, on a record field |
| `edge_malformed_operator_sequences_are_reported_not_guessed` | malformed_input — `>>`, `>=>`, `>==`, `!!`, `!=!`, lone `!`/`=`/`<`/`>`, trailing operator, unterminated string |
| `edge_deeply_nested_and_long_comparisons_are_bounded_not_a_panic` | resource_limit — 200-deep nest and a 200-link chain both refused by the parser's nesting guard with a span |
| `edge_ordering_is_total_over_the_extremes` | numeric_boundary — 64 operand pairs over the extremes, cross-checking all six operators for antisymmetry and agreement |
| `edge_a_chained_ordering_comparison_fails_on_the_boolean_left_operand` | type_mismatch — `1 < 2 < 3` is a caught error, proving the chain is not silently flattened |
| `comparison_symbols_do_not_swallow_the_character_after_them` | malformed_input — `x > 5`, `!=x`, `a==b`, `a <`+newline |
| `rb_run_executes_a_file_using_every_comparison_symbol` | end-to-end — a `.rb` file using all six symbols through `rb run`, asserts exit 0 and the branch output |
| `rb_run_exits_zero_when_every_comparison_is_false` | end-to-end — the false side of every symbol exits 0 and prints only `done` |
| `tests/test_comparisons.rb` (13 blocks) | the same surface written in Redblue, run by the project's own harness; the three `try … catch error` blocks assert the error is produced and the branch is not taken |

The three required mandatory tests are present:
`edge_*` — 14 Rust + 7 Redblue. Asserts a failure —
`edge_ordering_operands_of_different_types_is_a_runtime_error`,
`edge_malformed_operator_sequences_are_reported_not_guessed`, and the three
`try … catch error` Redblue blocks. New `#[test]` functions — 19 (floor is 3).
Zero new `#[ignore]`, zero `// skip`, zero `allow(clippy::`.

## Edge-case matrix (AGENTS.md §3.2)

- **empty** — covered. `edge_equality_of_two_lists_and_two_records_compares_contents`
  (`[] == []` yes, `[] == [1]` no, `{} == {}` yes);
  `edge_text_ordering_is_a_runtime_error_on_both_sides` (`"" == ""` yes,
  `"" < ""` refused); `edge_ordering_a_list_or_record_against_a_number…`
  (`[] > 1`).
- **singleton** — covered. `edge_equality…` (`[7] == [7]`); `[42][0]`-style
  singleton reach is `tests/index_bounds_test.rs` and is untouched;
  `the_comparison_symbols_lex_to_their_own_token_kinds` lexes each symbol alone.
- **boundary** — covered. `edge_x_against_itself_is_strictly_less_and_strictly_greater`
  (the `x` vs `x` case the phase names), `edge_reversed_operands…`,
  `edge_less_equal_and_greater_equal_are_one_token_not_two`.
- **out_of_bounds** — N/A. This change adds six lexer arms; it touches no
  indexing path. `tests/index_bounds_test.rs` and `tests/loop_bounds_test.rs`
  cover out-of-bounds index and loop bounds and are unchanged and green.
- **type_mismatch** — covered.
  `edge_ordering_operands_of_different_types_is_a_runtime_error`,
  `edge_ordering_a_list_or_record_against_a_number_is_a_runtime_error`,
  `edge_a_chained_ordering_comparison_fails_on_the_boolean_left_operand`.
- **numeric_boundary** — covered. `edge_numeric_boundaries_order_without_panicking`
  and `edge_ordering_is_total_over_the_extremes`.
- **unicode** — covered. `edge_text_ordering_is_a_runtime_error_on_both_sides`
  on `héllo`, `日本語`, `🎉`, and a Unicode list compared element-wise. The
  lexer arms are ASCII and do not touch the identifier/text paths, which
  `tests/lexer_robustness_test.rs` covers separately.
- **nesting_recursion** — covered.
  `edge_comparisons_compose_with_lists_loops_functions_and_records` and the
  three-deep record/list equalities in `edge_equality…`.
- **duplicate_missing_keys** — covered.
  `edge_comparing_an_absent_record_key_orders_nothing_but_equals_it`.
- **malformed_input** — covered.
  `edge_malformed_operator_sequences_are_reported_not_guessed`,
  `comparison_symbols_do_not_swallow_the_character_after_them`.
- **resource_limit** — covered.
  `edge_deeply_nested_and_long_comparisons_are_bounded_not_a_panic` (the
  parser's existing 64-level guard applies to a comparison chain; both a
  200-deep nest and a 200-link chain are refused with a span rather than
  recursing).

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — zero warnings |
| `cargo test` | **514 passed, 0 failed** (was 495 before this phase) |
| `cargo test --all-targets` | **513 passed, 0 failed, 0 ignored** across 25 suites |
| `./rbops/verify.sh phase-026` | **not run — the file does not exist in this checkout** |

`rbops/` is not present in this working directory (`ls -a` shows `.github`,
`AGENTS.md`, `phases/`, `src/`, `tests/`, … and no `rbops/`), and the task
brief states the pipeline lives elsewhere. `./rbops/verify.sh` returns
`No such file or directory`. Every other gate above was run and is green, and
`tests/redblue_suite_test.rs` — the project's own gate over the Redblue
test suite, which enforces the assertion, skip-marker, edge-name and
failure-assertion rules of AGENTS.md §3.3 — passes 8/8. I am reporting the
missing gate rather than claiming it.

### Backwards compatibility

Every file still exits 0:

```
0 examples/files.rb      0 examples/formats.rb   0 examples/test_arithmetic.rb
0 examples/fizzbuzz.rb   0 examples/hello.rb      0 examples/time.rb
0 modules/MathUtils.rb    0 modules/SuiteKit.rb
```

(`MathUtils.rb` exits 0 on its own; the phase noted it baselined until
phase-024, and that is already the case.)

The bytecode path was exercised too: `rb compile` + `rb vm` on a file using all
six symbols prints `a b c d e f` and exits 0.

`rb format` round-trips the new symbols (`<`, `<=`, `>`, `>=` unchanged; `==`
normalised to `is` and `!=` to `is not`, which are the same operators and the
forms `SPEC.md:271-275` prefers). `rb lint` exits 0 on the same file.

## Invariants touched

- **None.** No entry in `phases/INVARIANTS.md` changed meaning. `.rb` is still
  the extension; `to … end` still closes with `end`; `set x to <expr>` is still
  assignment; `say` still prints; the `Value` variants and `Error` variants are
  untouched; the trailing-comma and `{interp}` string syntaxes are untouched.
- No previously-rejected program is now accepted. The six symbols were
  `LexerError` before and are now parsed; a program that used them could not
  have run before, so there is no acceptance to regress. Every *other*
  malformed program still fails at the same stage with the same message —
  `edge_malformed_operator_sequences_are_reported_not_guessed` pins that `>>`
  and `>==` are parser errors rather than being silently accepted.

## Spec drift

`SPEC.md:277-282` and `docs/GRAMMAR.md:93-98` document `>`, `>=`, `<`, `<=`,
`==`, `!=`. Both files are left exactly as they were: they already describe what
the code now does, and this phase makes the code match them.

One gap is **not** fixed and is not claimed to be: `SPEC.md:271-275` and
`docs/GRAMMAR.md:93-98` also document the word forms `is greater than`,
`is less than or equal to`, `isnt`, `&&` and `||`. None of those parse or lex
today (`if 5 is greater than 3 then` → `ParserError: Expected Then but got
Identifier`; `&&` → `LexerError: Unexpected character '&'`; `isnt` → same
parser error). That is a separate defect in `parse_comparison` and the keyword
table, not in the operator table this phase changed, and fixing it would mean
adding grammar this phase was not scoped to open. It is recorded in
`FINDINGS.md`. `README.md:18` shows `if count is greater than 10` and is
likewise still wrong; per the phase constraints, README is left alone until
that is fixed.

## Known gaps / follow-ups

- The word forms of comparison (`is greater than`, `is less than`,
  `is less than or equal to`, `is greater than or equal to`, `isnt`,
  `is equal to`) do not parse, and neither do `&&` or `||`. → `FINDINGS.md` §1
- `push` is declared as a builtin at `src/stdlib.rs:62` but there is no
  `call_builtin` arm for it, so `push x to list` is a `ParserError`
  (`Unexpected token`). Found while writing tests, out of scope here.
  → `FINDINGS.md` §2
- A one-line `if cond then stmt end` is not idempotent under `rb format`: the
  formatter reports "File would be reformatted" and then writes back identical
  bytes. Pre-existing, unrelated to the symbols (`if x is 3 then say "a" end`
  behaves the same). → `FINDINGS.md` §3