# Phase 027 — Parse the English comparison forms `is greater than` / `is less than` / `is equal to`

## Reproduction of the finding

The finding reproduced on this checkout, unchanged. Five of the six documented
spellings failed; only `is not` parsed:

```
$ printf 'set x to 1\nif x %s then\n    say "yes"\nend\n' "$form" > target/tmp/t.rb
$ cargo run --quiet --bin rb -- run target/tmp/t.rb

is equal to 1                 => Error: ParserError: Expected Then but got To
is less than 1                => Error: ParserError: Expected Then but got Identifier("than")
is greater than or equal to 1 => Error: ParserError: Expected Then but got Identifier("than")
is less than or equal to 1    => Error: ParserError: Expected Then but got Identifier("than")
is greater than 1             => Error: ParserError: Expected Then but got Identifier("than")
is not 1                      => yes
```

`Parser::parse_comparison` handled `TokenKind::Is` by advancing past it and
then looking only for `TokenKind::Equal` or `TokenKind::Not`, falling through to
`BinaryOp::Equal` otherwise. `greater`, `less`, `than` and `equal` are not in
the `KEYWORDS` table (`src/lexer.rs:14-68`), so they lex as
`TokenKind::Identifier` and became the right operand of a bare `is`; `to` is
`TokenKind::To`, which is why `is equal to` failed differently. The rest of the
line then died on the missing `then`. Same defect as phase 026 `FINDINGS.md` §1.

## What changed

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +94 −18 | New `Parser::parse_is_operator`, `is_word_ahead`, `or_equal_to_ahead`, `current_at`; the `Is` arm of `parse_comparison` now delegates to `parse_is_operator` instead of looking for `=`/`not` inline |
| `tests/comparison_words_test.rs` | +700 (new) | 13 `#[test]` functions, 10 of them `edge_*` |
| `tests/test_comparisons.rb` | +102 | 6 Redblue `test` blocks, 4 of them `edge_*` |

No file other than `src/parser.rs` was changed in `src/`. The lexer, analyzer,
VM, runtime, bytecode and formatter were not touched, and no grammar in
`SPEC.md` or `docs/GRAMMAR.md` was edited.

### The change, in one paragraph

`greater`, `less`, `than` and `equal` are plain identifiers, so they are
matched **by name** in `parse_is_operator` (`src/parser.rs:1250-1294`), the
same way `parse_has` already matched `default` by name at
`src/parser.rs:830-841`. After `is`, the parser tries, in order: `equal to`;
`greater than [or equal to]`; `less than [or equal to]`; the symbolic `=`; the
prefix `not`; and bare `is` = equality. The `or equal to` tail is matched as
one phrase by `or_equal_to_ahead` (`src/parser.rs:1225-1241`) and consumed
three tokens, so it binds inside the comparison rather than being left for
`parse_or`.

### Two decisions worth stating

**A word is an operator only when the whole phrase is there.** `parse_is_operator`
checks the complete phrase with `is_word_ahead` *before* consuming anything. So
`if x is greater then` still compares `x` for equality against a variable named
`greater`, exactly as it did before this phase — the alternative would have made
`greater`, `less`, `equal` and `than` reserved words and silently broken any
program that uses them as identifiers. Pinned by
`edge_a_partial_word_phrase_still_compares_for_equality`.

**`or equal to` is consumed here, not by `parse_or`.** The alternative —
letting `or` bind loosely — is what the phase names as the bug to avoid:
`x is greater than or equal to y` would become `(x is greater than y) or equal
to y`, which does not parse at all. Pinned by
`edge_or_equal_to_binds_as_one_comparison_not_as_a_logical_or`, which also
checks that a *genuine* `or`/`and` still binds looser than the word comparison.

## Tests added

| Test | Edge class covered |
|---|---|
| `the_six_word_comparison_forms_parse_and_select_the_branch_they_name` | happy path — all six word forms in one program, each selecting its own branch |
| `rb_run_executes_a_file_using_every_word_comparison_form` | end-to-end — a `.rb` file through `rb run`, asserts exit 0 and the exact printed branch lines |
| `word_and_symbolic_forms_agree_on_the_same_operands` | cross-check — 8 operand pairs × 6 forms against the phase-025 symbols, plus 6 non-comparable pairs that must fail identically |
| `edge_or_equal_to_binds_as_one_comparison_not_as_a_logical_or` | boundary — the inclusive boundary over 5 operand pairs, the mirror identity, and real `or`/`and` still binding looser |
| `edge_word_comparison_at_the_equality_boundary_in_both_directions` | boundary — `x` against itself, all six forms, over 5 values |
| `edge_word_comparison_with_the_operands_reversed` | boundary — 4 pairs reversed; strict forms swap, equality does not |
| `edge_word_ordering_a_number_against_text_is_a_runtime_error` | type_mismatch — number vs text in both directions, `""` included; text equality still works |
| `edge_word_ordering_a_record_or_list_against_a_number_is_a_runtime_error` | type_mismatch — record vs number both directions, `[]`, `[1,2]`, list vs list, empty containers |
| `edge_a_partial_word_phrase_still_compares_for_equality` | malformed_input / back-compat — `greater`, `less`, `equal`, `than` usable as variables; a bare `is greater` with no such variable is a runtime error |
| `edge_malformed_word_comparisons_are_reported_not_guessed` | malformed_input — 10 phrases with a missing operand or a half-phrase, every refusal carrying a span |
| `edge_word_comparison_orders_the_numeric_boundaries` | numeric_boundary — signed zero, past `2^53`, `i64` extremes, largest finite double, and `1/0`, `0/0`, `1e400` refused before a comparison sees them |
| `edge_word_comparisons_of_records_containers_and_nested_values` | duplicate_missing_keys / nesting_recursion — duplicate key keeps the last value, key order is not content, missing key is `nothing` (equal to `nothing`, unorderable), three-deep nesting, inside loops, functions and `and` chains |
| `edge_a_long_word_comparison_chain_is_bounded_by_the_nesting_guard` | resource_limit — a 200-link chain is refused by the existing 64-level nesting guard with a span; a 20-link chain through `rb run` exits 0 |
| `tests/test_comparisons.rb` (6 blocks) | the same surface in Redblue, run by the project's own harness; two `try … catch error` blocks assert the error is produced and the branch is not taken |

The three mandatory tests are present. `edge_*` — 10 Rust + 4 Redblue. Asserts
a failure — `edge_word_ordering_a_number_against_text_is_a_runtime_error`,
`edge_word_ordering_a_record_or_list_against_a_number_is_a_runtime_error`,
`edge_malformed_word_comparisons_are_reported_not_guessed`,
`edge_a_partial_word_phrase_still_compares_for_equality`, and the two
`try … catch error` Redblue blocks. New `#[test]` functions — 13 (floor is 3).
Zero new `#[ignore]`, zero `// skip`, zero `allow(clippy::`.

### The tests can fail

Not asserted from a green run; measured by mutation:

| Mutation to `src/parser.rs` | Result |
|---|---|
| `or_equal_to_ahead` returns `false` unconditionally (the loose-binding bug the phase names) | 10 of 13 tests FAIL |
| the adjective/op pair `("greater", Greater), ("less", Less)` swapped | 7 of 13 tests FAIL |

The third test in this file, `the_six_word_comparison_forms_parse_and_select_the_branch_they_name`,
was written before any production change and was watched fail with
`Parser("Expected Then but got Identifier(\"than\")", Span { line: 3, column: 17 })`.

## Edge-case matrix (AGENTS.md §3.2)

- **empty** — covered. `edge_word_ordering_a_record_or_list_against_a_number…`
  (`{}` and `[]` compared with `is equal to`, and each refused when ordered);
  `edge_word_ordering_a_number_against_text…` (`"" is equal to ""` yes,
  `"" is less than "a"` refused); `edge_a_malformed…` — the file
  `set x to 1\nif x is greater than` with no operand at all.
- **singleton** — covered. `edge_word_ordering_a_record_or_list…` (`[7] is
  equal to [7]`, `[] is equal to [1]`); the singleton *phrase* — one operator
  with no `or equal to` tail — is `is greater than` / `is less than` in
  `the_six_word_comparison_forms…` and in every boundary test.
- **boundary** — covered. `edge_or_equal_to_binds_as_one_comparison…` (the
  inclusive boundary where strict and inclusive disagree, both signs),
  `edge_word_comparison_at_the_equality_boundary_in_both_directions`,
  `edge_word_comparison_with_the_operands_reversed`.
- **out_of_bounds** — N/A. This change adds six match arms to one function in
  `parse_comparison`; it introduces no indexing and touches no subscript
  expression. Out-of-bounds index and loop bounds are pinned by
  `tests/index_bounds_test.rs` and `tests/loop_bounds_test.rs`, both unchanged
  and green. What the word forms *can* reach wrongly — a record against a
  number, a list against a number — is covered under type_mismatch.
- **type_mismatch** — covered. `edge_word_ordering_a_number_against_text_is_a_runtime_error`,
  `edge_word_ordering_a_record_or_list_against_a_number_is_a_runtime_error`,
  and the non-comparable half of `word_and_symbolic_forms_agree_on_the_same_operands`.
  All are `Error::Runtime` with a span; none is a `yes`, a `no` or a panic.
- **numeric_boundary** — covered. `edge_word_comparison_orders_the_numeric_boundaries`:
  signed zero in all six directions, `2^53+1` rounding back onto `2^53`, the
  `i64` extremes, the largest finite double, and `1/0`, `0/0`, `1e400` and
  `1e308 * 1e308` refused before any comparison sees them.
- **unicode** — covered. `edge_word_ordering_a_number_against_text_is_a_runtime_error`
  on `héllo`, `日本語`, `🎉` — equal to themselves through `is equal to`,
  unequal to a longer string, and still refused when ordered. The new code
  matches ASCII words on `Identifier` tokens only; it does not touch text
  literals, and `tests/lexer_robustness_test.rs` covers those separately.
- **nesting_recursion** — covered. `edge_word_comparisons_of_records_containers_and_nested_values`
  — a comparison inside a `for` body over the loop variable, against the result
  of a function call, against a record field, inside an `and` chain written the
  way `SPEC.md:287` writes it, and three-deep nested records and lists compared
  for equality.
- **duplicate_missing_keys** — covered. Same test: `{a: 1, a: 2}` keeps the
  last value and the comparison sees it; `{a: 1, b: 2} is equal to {b: 2, a: 1}`
  because key order is not content; `r.missing` is `nothing`, equal to
  `nothing`, not equal to `0`, and unorderable.
- **malformed_input** — covered. `edge_malformed_word_comparisons_are_reported_not_guessed`
  — 9 phrases with a missing operand or a half-written phrase, plus a phrase
  split across a newline (not joined) and an operator at end of file; all are
  parser errors carrying a span, none silently accepted.
  `edge_a_partial_word_phrase_still_compares_for_equality` covers the other
  direction: a half-phrase must not be guessed into an operator.
- **resource_limit** — covered. `edge_a_long_word_comparison_chain_is_bounded_by_the_nesting_guard`
  — the parser's existing 64-level `enter_nesting` guard applies to the word
  forms, because `parse_comparison` still calls `enter_nesting` once per link;
  a 200-link chain is a clean `ParserError` naming the limit, a 60-link chain
  parses and reaches the runtime, and a 20-link chain through `rb run` exits 0.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — zero warnings |
| `cargo test --all-targets` | **526 passed, 0 failed, 0 ignored** across 25 suites |
| `cargo test` | **527 passed, 0 failed** — the same 25 suites plus 1 doc-test |
| `./rbops/verify.sh phase-027` | **not run — the file does not exist in this checkout** |

`rbops/` is not present in this working directory (`ls -a` shows `.github`,
`AGENTS.md`, `phases/`, `src/`, `tests/`, … and no `rbops/`), and the task
brief states the pipeline lives elsewhere. `./rbops/verify.sh` returns
`No such file or directory`. Every other gate was run and is green, and
`tests/redblue_suite_test.rs` — the project's own gate over the Redblue test
suite, which enforces the assertion, skip-marker, edge-name and
failure-assertion rules of AGENTS.md §3.3 — passes 8/8. Phase 026's `REPORT.md`
recorded the same absence. I am reporting the missing gate rather than
claiming it.

### Backwards compatibility

Every file still exits 0:

```
0 examples/files.rb      0 examples/formats.rb   0 examples/test_arithmetic.rb
0 examples/fizzbuzz.rb   0 examples/hello.rb      0 examples/time.rb
0 modules/MathUtils.rb    0 modules/SuiteKit.rb
```

`MathUtils.rb` exits 0 on its own, so the phase-024 baselining is not needed
here either. The bytecode path was exercised too: `rb compile` + `rb vm` on a
file using five of the six word forms prints `greater ge eq ne` and exits 0 —
so the compiler and the bytecode VM read the new operators, not just the
tree-walker. `rb lint` on the same file exits 0 with no findings.

## Invariants touched

- **None.** No entry in the invariants table changed meaning. `.rb` is still
  the extension; `to … end`, `if … end` and `for … end` still close with `end`;
  `set x to <expr>` is still assignment; `say` still prints; the `Value` and
  `Error` variants are untouched; trailing-comma and `{interp}` string syntax
  are untouched.
- No previously-rejected program is newly accepted *as a different program*.
  Five spellings that could not parse now parse to the operator
  `SPEC.md:277-282` says they mean, and `tests/comparison_words_test.rs`
  cross-checks every one of them against the phase-025 symbol form on the same
  operands. Every malformed phrase is still refused, with a span —
  `edge_malformed_word_comparisons_are_reported_not_guessed` — and a half
  phrase is still equality, not a guess —
  `edge_a_partial_word_phrase_still_compares_for_equality`.
- No `panic!`, `unwrap` or index was added. `is_word_ahead`, `current_at` and
  `or_equal_to_ahead` are all `self.tokens.get(...)`-based, so they return
  `None`/`false` at end of input rather than reading past the end.

## Spec drift

None introduced. `SPEC.md:273-274`, `SPEC.md:277-282`, `docs/GRAMMAR.md:93-98`,
`docs/GRAMMAR.md:375-380`, `README.md:18` and `ROADMAP.md:10` already promise
these forms and are left exactly as they were; this phase makes the code match
them. No file outside `src/parser.rs` was edited in `src/`, and no
documentation was changed to claim anything.

One of those promises was re-checked by running it, not by reading it:

```
$ printf 'set score to 90\nif score is greater than 80 then\n    say "pass"\nend\n' > target/tmp/rm.rb
$ cargo run --quiet --bin rb -- run target/tmp/rm.rb
pass
```

`ROADMAP.md:672` parses and prints. The example at `SPEC.md:735-737` uses
`if b is equal to 0` with no `then`, so it still fails — but on the missing
`then` (`Expected Then but got GiveBack` at `SPEC.md:737`'s body), which is a
different error from the one this phase fixed and a property of the example, not
of the comparison.

`docs/GRAMMAR.md:94` lists a seventh spelling, `isnt`, which still does not
parse. It is not named in this phase's goal, so it is recorded in
`FINDINGS.md` §1 rather than smuggled in.

## Known gaps / follow-ups

- `isnt`, the seventh spelling at `docs/GRAMMAR.md:94`, still does not parse.
  → `FINDINGS.md` §1
- `expect x is equal to be <value>` cannot be written: `is equal to` and
  `expect … to be …` both want a `to`. A grammar question, not a parser bug.
  → `FINDINGS.md` §2
- `rb format` now rewrites `is greater than` as `>`, because the formatter
  prints the AST rather than the spelling, and both spellings now build the
  same AST. Nothing regressed — the file was a parse error before — but a
  formatter that rewrites `README.md:18`'s example into a symbol is arguably
  wrong for this language. → `FINDINGS.md` §3
- The doc comment above `Formatter::format_binary_op`
  (`src/formatter.rs:629-633`) is now stale: it says `<`, `<=`, `>`, `>=` and
  `in` "have no token at all". Four of those five do. → `FINDINGS.md` §5
- A `test` block cannot see a variable set above it. Pre-existing and
  symbol-independent. → `FINDINGS.md` §4