# Phase 015 — Formatter must be idempotent and lossless

## What changed

Two runs contributed. The first (commit `329e83d`) fixed the finding; this
resume (commit `HEAD`) verified it independently and closed the coverage gap it
left behind.

| File | Lines | What |
|---|---|---|
| `src/formatter.rs` | +384 −147 | **run 1** — comment attachment (`//` comments no longer dropped), exact `needs_reformat` comparison helper, exact trailing-newline emission, text-literal escaping that round-trips, operator-grouping parentheses preserved, no state carried between `format` calls |
| `src/lib.rs` | +1 −1 | **run 1** — `rb format --check` now calls `formatter::needs_reformat(&source, &formatted)` instead of `source.trim() != formatted.trim()` |
| `tests/formatter_test.rs` | +978 −0 | **run 1** — 39 `#[test]` functions, 21 `edge_*` |
| `tests/formatter_test.rs` | +215 −0 | **run 2 (this resume)** — 5 `#[test]` functions, 5 `edge_*`, covering the spec-documented forms no corpus file and no prior test reached (see F5) |

### The finding, reproduced

Before the change, `src/lib.rs:173` compared:

```rust
if source.trim() != formatted.trim() {
```

`trim()` erases trailing whitespace and the trailing newline, so a file
differing from its formatted form *only* in those respects was reported as
already formatted. Re-verified against the current tree in this resume — all
now exit 1, which the old comparison would have reported as exit 0:

```
printf 'say "hi"\n   \n'  > f.rb ; rb format --check f.rb   # trailing whitespace  -> exit 1
printf 'say "hi"'        > f.rb ; rb format --check f.rb   # no trailing newline   -> exit 1
printf 'say "hi"\r\n'    > f.rb ; rb format --check f.rb   # CRLF line ending      -> exit 1
printf 'say "hi"\n\n'    > f.rb ; rb format --check f.rb   # trailing blank line   -> exit 1
printf 'say "hi"\n'      > f.rb ; rb format --check f.rb   # already formatted     -> exit 0
```

## Tests added

44 `#[test]` functions in `tests/formatter_test.rs`, 26 named `edge_*`.
39 from run 1, 5 from this resume. Zero `#[ignore]`, `// skip`, or
`allow(clippy::`.

### Definition of done

| Requirement | Test | Result |
|---|---|---|
| idempotence property over `examples/` + `tests/` | `format_is_idempotent`, `format_is_idempotent_over_the_whole_corpus` | covered |
| format then parse gives an equivalent AST for every example | `format_preserves_the_meaning_of_every_corpus_file`, `same_program` compares the full `Program` debug tree with `Span { … }` stripped | covered |
| `--check` compares exactly, including trailing newline | `check_compares_exactly_including_the_trailing_newline`, `formatted_output_ends_with_exactly_one_newline`, `check_rejects_a_file_that_differs_only_in_its_last_byte` | covered |
| formatter preserves comments | `format_preserves_comments`, `format_preserves_every_comment_in_every_corpus_file`, `format_preserves_comments_inside_blocks_and_at_the_end` | covered |

### Added in this resume — the coverage gap F5 exposed

Auditing which `Stmt`/`BinaryOp` variants the parser can actually produce
showed four spec-documented forms reach no corpus file and no prior test:

| Test | Edge class covered |
|---|---|
| `edge_a_for_from_to_loop_is_reported_not_silently_rewritten` | malformed_input — **asserts failure** (`for … from … to … by …`, `SPEC.md:388`) |
| `edge_a_symbolic_relational_operator_is_reported_not_guessed_at` | malformed_input — **asserts failure** (`<`, `docs/GRAMMAR.md:438`) |
| `edge_a_comparison_containing_or_is_reported_not_silently_rewritten` | malformed_input — **asserts failure** (`is greater than or equal to`; also asserts the CLI exits 1 and prints nothing it cannot stand behind) |
| `edge_a_word_comparison_fails_the_same_way_before_and_after_formatting` | malformed_input — **asserts failure**; exit code, stdout and error *identity* unchanged by formatting |
| `edge_every_spec_only_form_is_either_idempotent_or_a_clean_reported_error` | malformed_input / boundary — umbrella: each form is idempotent or a clean lexer/parser error, never a silent rewrite |

Two of these caught wrong assumptions while being written, which is the evidence
they are not vacuous:

- `edge_a_word_comparison_fails_the_same_way_before_and_after_formatting`
  initially compared whole `stderr` and failed: a diagnostic quotes the source
  line it read, and formatting moved that text. Only the `Error: …` head is
  compared now, via the `error_line` helper. The quoted line is a picture of
  the formatted file, not part of the error.
- `edge_a_comparison_containing_or_is_reported_not_silently_rewritten`
  initially treated both word comparisons alike and failed: `is less than`
  parses into three statements, but `is greater than **or** equal to` does not
  parse at all, because `or` is a real keyword. The two cases are now separate
  tests.

### Edge class covered

| Test | Edge class covered |
|---|---|
| `edge_empty_program_formats_to_nothing` | empty |
| `edge_empty_text_literal` | empty |
| `edge_singleton_inputs` | singleton / boundary |
| `edge_numeric_boundaries_round_trip` | numeric_boundary |
| `edge_unicode_text_round_trips` | unicode |
| `edge_text_holding_a_quote_and_a_backslash_round_trips` | unicode / escapes |
| `edge_a_literal_spanning_lines_keeps_its_slashes_and_later_comments` | unicode / escapes / malformed-adjacent |
| `edge_a_very_long_text_literal_round_trips` | resource_limit |
| `edge_a_very_deeply_nested_program_is_handled_or_reported` | resource_limit / nesting_recursion |
| `edge_nested_lists_and_records_round_trip` | nesting_recursion |
| `edge_duplicate_record_keys_keep_their_order` | duplicate_missing_keys |
| `edge_missing_record_field_is_not_invented_by_formatting` | duplicate_missing_keys |
| `edge_a_duplicate_key_is_still_overwritten_after_formatting` | duplicate_missing_keys |
| `edge_crlf_source_formats_to_lf_and_is_idempotent` | malformed_input |
| `edge_a_bom_does_not_become_part_of_the_first_statement` | malformed_input |
| `edge_unclosed_block_is_a_parser_error` | malformed_input |
| `formatter_rejects_malformed_input_instead_of_guessing` | malformed_input — **asserts failure** |
| `edge_a_program_that_fails_still_fails_the_same_way_after_formatting` | malformed_input — **asserts failure**, failure parity |
| `edge_grouping_survives_in_what_the_program_computes` | nesting_recursion — executes both forms, compares output |
| `format_keeps_the_grouping_the_parentheses_forced` | nesting_recursion |
| `format_keeps_right_hand_grouping_of_equal_precedence` | nesting_recursion |
| `format_keeps_grouping_for_unary_and_every_operator_level` | nesting_recursion |
| `edge_equality_against_a_negation_keeps_its_parentheses` | nesting_recursion |
| `check_rejects_a_file_that_differs_only_in_its_last_byte` | boundary — **asserts failure** (exit code 1) |
| `edge_slashes_inside_a_text_literal_are_not_comments` | malformed_input / unicode |
| `edge_a_comment_that_is_only_slashes_is_kept` | boundary |
| `format_writes_index_with_brackets_not_at` | singleton / boundary |
| `formatter_instance_does_not_carry_state_between_calls` | determinism |
| `formatted_programs_behave_exactly_as_the_originals` | semantics — runs the real `rb` binary on original and formatted source and compares stdout |
| `format_covers_every_statement_form` | coverage |
| the 5 tests in the table above | malformed_input / boundary |

### Edge-case matrix

- **empty** — covered (`edge_empty_program_formats_to_nothing`, `edge_empty_text_literal`)
- **singleton** — covered (`edge_singleton_inputs`, `format_writes_index_with_brackets_not_at`)
- **boundary** — covered (`check_rejects_a_file_that_differs_only_in_its_last_byte`, `edge_a_comment_that_is_only_slashes_is_kept`, `formatted_output_ends_with_exactly_one_newline`, `edge_every_spec_only_form_is_either_idempotent_or_a_clean_reported_error`)
- **out_of_bounds** — N/A. The formatter emits no indexing operation on runtime values; it walks the AST it just parsed, and a grep for `unwrap()`/`panic!`/`[0]`/`unreachable` in `src/formatter.rs` returns nothing. Index bounds are a VM concern, already covered by `tests/index_bounds_test.rs`.
- **type_mismatch** — N/A. The formatter never evaluates or converts a value; it only re-renders tokens. There is no expression at which a type could be wrong. The nearest thing is the spec forms in F5, now covered by 5 tests.
- **numeric_boundary** — covered (`edge_numeric_boundaries_round_trip`) — numeric literals re-render byte-identically.
- **unicode** — covered (`edge_unicode_text_round_trips`, `edge_text_holding_a_quote_and_a_backslash_round_trips`, `edge_a_bom_does_not_become_part_of_the_first_statement`)
- **nesting_recursion** — covered (6 tests, listed above)
- **duplicate_missing_keys** — covered (3 tests, listed above)
- **malformed_input** — covered (10 tests: the 5 listed above plus the 5 added in this resume)
- **resource_limit** — covered (`edge_a_very_deeply_nested_program_is_handled_or_reported`, `edge_a_very_long_text_literal_round_trips`)

### Independent verification beyond the suite

Driven against the built binary, not inferred from the tree. Run 1 recorded the
first three bullets; this resume re-ran all of them and added the last four.

- Idempotence + `format --check` accepted its own output over all 21 `.rb`
  files in `examples/`, `modules/`, `tests/` — 0 non-idempotent, 0 rejected
  by `--check` after formatting. (`modules/MathUtils.rb` is the one file that
  does not parse at all; see F1.)
- All 6 `examples/*.rb` run; `modules/SuiteKit.rb` runs.
- All 6 examples produce byte-identical stdout before and after formatting
  (`examples/time.rb` excluded from the byte comparison — it prints
  `time.now()` nanoseconds, which differ between any two runs; its parity is
  covered instead by `formatted_programs_behave_exactly_as_the_originals`,
  which uses deterministic fixtures).
- **840-mutation fuzz** (this resume): each corpus file mutated 40 ways — line
  deletion, duplication, blank lines, stray tabs/indent, injected comments,
  trailing whitespace, CRLF. 0 non-idempotent, 0 reformat errors, 0 semantic
  drift outside `examples/time.rb`'s wall-clock output.
- **Comment-position sweep** (this resume): a `// MARK` comment injected at
  every line position of 5 block-structured programs — 23 cases. The marker
  survived exactly once in every case and every result was idempotent.
- **Determinism** (this resume): `rb format` run 5× on each of 20 files, all
  byte-identical; a 36-key record formats to a stable key order.
- **Escape round-trip** (this resume): `\n \t \r \\ \"` in a text literal each
  decode and re-encode to the same literal.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test --all-targets` | **328 passed, 0 failed, 0 ignored** (323 after run 1, +5 this resume) |
| `./rbops/verify.sh phase-015` | **NOT RUN — `rbops/` does not exist in this checkout** |

### Honest note on the fourth gate

`rbops/` is absent from the working tree (`ls rbops/verify.sh` →
`No such file or directory`); `.opencode/` is likewise absent. The prompt
states the invoking pipeline lives elsewhere and must not be inspected, so
`./rbops/verify.sh` was never executed and **no claim is made about its
output**. Its content is **unknown to this run**, not assumed-passing. If it
enforces a rule the other three gates do not, only that rule is unverified.

The three runnable gates were each executed directly in this tree, in the order
`AGENTS.md` §3.4 requires, and passed as tabulated above.

## Invariants touched

- None of the language-surface invariants in `phases/INVARIANTS.md` are
  touched. No `TokenKind`, `Value` variant, `Error` variant, keyword, or
  grammar production is added, removed, or renamed. `.rb` remains the source
  extension. This resume changed **tests only** — `src/` is untouched since
  run 1.
- `redblue::formatter::needs_reformat` is a new `pub` function (run 1). This
  is an addition, not a change: `rb format --check` behaviour is the CLI
  surface, and the comparison semantics it encodes are unchanged in intent —
  only the `trim()` that defeated them is gone.

## Known gaps / follow-ups

- `modules/MathUtils.rb` cannot be parsed, so it cannot be formatted. This is
  pre-existing and unrelated to the formatter: the file uses
  `constant PI to 3.14159`, which is not grammar. Confirmed pre-existing by
  this resume — the file is untouched by phase-015, and the version from
  `27d4aeb` (pre-phase-015) fails with the identical error. The formatter
  correctly *rejects* it rather than guessing. → F1 in `FINDINGS.md`.
- The corpus used by `corpus()` covers `examples/` and `tests/` only. It does
  not include `modules/`, because `modules/MathUtils.rb` does not parse and
  would fail the corpus-wide property tests for a reason that is not the
  formatter's. Deliberately **not** worked around by skipping unparseable
  files in `corpus()` — that would hide exactly the failure worth surfacing.
  The change is one string once F1 is fixed. → F2.
- `SPEC.md` and `docs/GRAMMAR.md` document `for … from … to … [by …]` and the
  relational operators, none of which the parser builds. Six branches of
  `src/formatter.rs` are consequently unreachable and are now pinned by 5
  tests that record the current behaviour instead of asserting dead arms.
  → F5.
- Grouping-parentheses preservation is covered for the operator levels the
  formatter special-cases. A future operator added to the grammar must be
  added to `format_keeps_grouping_for_unary_and_every_operator_level`, or
  that test's table needs extending — nothing enforces the link.
- `rb format` prints to stdout rather than rewriting in place, so
  `rb format foo.rb > foo.rb` truncates the file first. → F3.
