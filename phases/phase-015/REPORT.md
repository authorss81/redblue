# Phase 015 — Formatter must be idempotent and lossless

## What changed

| File | Lines | What |
|---|---|---|
| `src/formatter.rs` | +232 −28 | Comment attachment by source line (`//` comments are no longer dropped or migrated onto the wrong keyword), `closing_lines`/`Closing` recording every `else`/`catch`/`finally`/`end` with its line so an empty branch keeps the keyword its author wrote, `format_block` returning the closing keyword, `open_line`/`write_trailing_comment` for comments written at the end of a line, `write_keyword_line` trailing the keyword's own comment, a `match` on `catch_var` so a bare `catch` is written back and a named one keeps its binding and its comment, exact `needs_reformat` comparison, exact trailing-newline emission, text-literal escaping that round-trips, operator-grouping parentheses preserved, no state carried between `format` calls |
| `src/lib.rs` | (unchanged in this diff) | `rb format --check` already calls `formatter::needs_reformat(&source, &formatted)`; verified below |
| `tests/formatter_test.rs` | +1422 −27 | 71 `#[test]` functions, 53 of them `edge_*`: idempotence and losslessness properties over `examples/` + `tests/` + `modules/`, exact `--check` comparison, comment preservation, try/catch/else/finally branch preservation, range loops and the relational operators, module/constant/unless forms, the full edge-case matrix |
| `phases/phase-015/FINDINGS.md` | +72 −55 | F1 and F5 rewritten as RESOLVED with the evidence (both features are grammar now); F2 fixed here |

## The finding, reproduced and fixed

The finding was that `src/lib.rs` compared `source.trim() != formatted.trim()`,
so a file differing from its formatted form only in trailing whitespace or its
trailing newline was reported as already formatted.

**It no longer reproduces on `main`.** `git log -S'trim() != formatted.trim()' --
src/lib.rs` → `329e83d rbops: phase-015`, and `git show HEAD:src/lib.rs:249`
already reads `if formatter::needs_reformat(&source, &formatted)`, where
`src/formatter.rs:956` is `pub fn needs_reformat(source: &str, formatted: &str)
-> bool { source != formatted }`. The previous attempt's fix is in `main`.

Verified against the binary built from this tree — all four cases behave:

```
$ printf 'say "hi"\n   \n' > f.rb ; rb format --check f.rb   # trailing whitespace
File would be reformatted                                              (exit 1)
$ printf 'say "hi"'        > f.rb ; rb format --check f.rb   # no trailing newline
File would be reformatted                                              (exit 1)
$ printf 'say "hi"\r\n'    > f.rb ; rb format --check f.rb   # CRLF
File would be reformatted                                              (exit 1)
$ printf 'say "hi"\n'      > f.rb ; rb format --check f.rb   # already formatted
                                                                     (exit 0)
```

What *did* still reproduce on `main` is the wider loss this phase is named for,
and this is what the resumed work fixes. Built from `main` (`git stash` of this
diff), the formatter drops branches and comments:

```
$ printf 'if 1 is 1 then\n    say 1\nelse // nothing here\n    // nothing either\nend\n' > f.rb
$ rb format f.rb
if 1 is 1 then
    say 1
end
// nothing here
// nothing either
```

The `else` is gone, and the comment the author wrote *on the `else` line* now
sits after the `end`, where it reads as a note about the whole statement. The
same shape loses a `catch`, a `finally`, or their bodies. That is the finding
this diff addresses, in `src/`.

## Tests added

71 `#[test]` functions in `tests/formatter_test.rs`, 53 named `edge_*`.

### Definition of done

| Requirement | Test | Result |
|---|---|---|
| idempotence property over `examples/` + `tests/` | `format_is_idempotent`, `format_is_idempotent_over_the_whole_corpus` (now also `modules/`) | covered |
| format then parse gives an equivalent AST for every example | `format_preserves_the_meaning_of_every_corpus_file`; `same_program` walks the tree with `Span` excluded | covered |
| `--check` compares exactly, including trailing newline | `check_compares_exactly_including_the_trailing_newline`, `formatted_output_ends_with_exactly_one_newline`, `check_rejects_a_file_that_differs_only_in_its_last_byte`, `edge_the_check_flag_reports_a_difference_through_the_exit_code` | covered |
| formatter preserves comments | `format_preserves_comments`, `format_preserves_every_comment_in_every_corpus_file`, `format_preserves_comments_inside_blocks_and_at_the_end`, plus 8 `edge_a_comment_*` tests | covered |

### Edge class covered

| Test | Edge class covered |
|---|---|
| `edge_empty_program_formats_to_nothing`, `edge_empty_text_literal` | empty |
| `edge_singleton_inputs`, `format_writes_index_with_brackets_not_at` | singleton / boundary |
| `check_rejects_a_file_that_differs_only_in_its_last_byte`, `edge_a_comment_that_is_only_slashes_is_kept`, `formatted_output_ends_with_exactly_one_newline` | boundary |
| `edge_numeric_boundaries_round_trip` | numeric_boundary (`-0.0`, `0`, `2^53+1`, `2^52`, `1e308`, `1e-308`, `- -1`) |
| `edge_unicode_text_round_trips`, `edge_text_holding_a_quote_and_a_backslash_round_trips`, `edge_a_bom_does_not_become_part_of_the_first_statement`, `edge_slashes_inside_a_text_literal_are_not_comments` | unicode / escapes |
| `edge_nested_lists_and_records_round_trip`, `edge_a_very_deeply_nested_program_is_handled_or_reported`, `edge_grouping_survives_in_what_the_program_computes`, `format_keeps_the_grouping_the_parentheses_forced`, `format_keeps_right_hand_grouping_of_equal_precedence`, `format_keeps_grouping_for_unary_and_every_operator_level`, `edge_equality_against_a_negation_keeps_its_parentheses`, `edge_a_relational_operator_is_not_reordered_around_its_operands` | nesting_recursion / grouping |
| `edge_duplicate_record_keys_keep_their_order`, `edge_missing_record_field_is_not_invented_by_formatting`, `edge_a_duplicate_key_is_still_overwritten_after_formatting` | duplicate_missing_keys |
| `edge_crlf_source_formats_to_lf_and_is_idempotent`, `edge_a_bom_does_not_become_part_of_the_first_statement`, `edge_unclosed_block_is_a_parser_error`, `edge_a_block_comment_is_still_a_parser_error_when_the_block_is_unclosed`, `edge_a_literal_spanning_lines_keeps_its_slashes_and_later_comments`, `edge_slashes_inside_a_text_literal_are_not_comments` | malformed_input |
| `formatter_rejects_malformed_input_instead_of_guessing` | malformed_input — **asserts a failure is produced** |
| `edge_a_program_that_fails_still_fails_the_same_way_after_formatting`, `edge_a_comparison_of_different_types_still_fails_the_same_way` | failure parity — **asserts a failure** |
| `edge_a_program_deeper_than_the_parser_budget_is_reported_not_truncated`, `edge_blocks_deeper_than_the_parser_budget_are_reported_not_truncated`, `edge_a_very_long_text_literal_round_trips` | resource_limit |
| `edge_bare_catch_body_is_not_deleted`, `edge_bare_catch_program_still_computes_the_same_after_formatting`, `edge_a_catch_without_a_name_keeps_the_branch_it_opens`, `edge_named_catch_keeps_its_binding`, `edge_finally_still_closes_a_bare_catch`, `edge_a_branch_written_on_one_line_keeps_its_keywords`, `edge_an_empty_branch_keeps_its_keyword_and_the_comment_written_on_it` | branch preservation |
| `edge_a_module_and_its_exports_round_trip`, `edge_an_unless_and_a_constant_survive_formatting`, `edge_a_range_loop_is_formatted_and_keeps_its_bounds_and_step`, `edge_a_range_loop_step_is_not_lost_or_invented`, `edge_every_relational_operator_round_trips_in_both_spellings`, `edge_a_spec_only_form_is_never_reported_as_an_error` | statement/operator coverage for forms no corpus file uses |
| `formatter_instance_does_not_carry_state_between_calls` | determinism |
| `formatted_programs_behave_exactly_as_the_originals` | semantics — runs the real `rb` binary on original and formatted source, compares exit code, stdout and stderr |
| `format_covers_every_statement_form` | coverage |

### Edge-case matrix

- **empty** — covered (`edge_empty_program_formats_to_nothing`, `edge_empty_text_literal`, and the empty `else`/`catch`/`finally` branches in `edge_an_empty_branch_keeps_its_keyword_and_the_comment_written_on_it`)
- **singleton** — covered (`edge_singleton_inputs`, `format_writes_index_with_brackets_not_at`)
- **boundary** — covered (`check_rejects_a_file_that_differs_only_in_its_last_byte`, `edge_a_comment_that_is_only_slashes_is_kept`, `formatted_output_ends_with_exactly_one_newline`)
- **out_of_bounds** — N/A. The formatter emits no indexing operation on runtime values; it walks the AST it just parsed and re-renders tokens. Index bounds are a VM concern and are covered by `tests/index_bounds_test.rs`. The nearest formatter analogue is the block-depth bound, covered by `edge_blocks_deeper_than_the_parser_budget_are_reported_not_truncated`.
- **type_mismatch** — covered (`edge_a_comparison_of_different_types_still_fails_the_same_way`: a number compared with text fails identically before and after formatting). Beyond that there is no expression at which a type could be wrong: the formatter never evaluates or converts a value.
- **numeric_boundary** — covered (`edge_numeric_boundaries_round_trip`)
- **unicode** — covered (`edge_unicode_text_round_trips`, `edge_text_holding_a_quote_and_a_backslash_round_trips`, `edge_a_bom_does_not_become_part_of_the_first_statement`, `edge_slashes_inside_a_text_literal_are_not_comments`)
- **nesting_recursion** — covered (8 tests, listed above)
- **duplicate_missing_keys** — covered (3 tests, listed above)
- **malformed_input** — covered (7 tests, listed above)
- **resource_limit** — covered (3 tests, listed above)

### Independent verification beyond the suite

Driven against the built binary, not inferred from the tree:

- Idempotence over every `.rb` file in `examples/`, `modules/` and `tests/` —
  `rb format f > o1; rb format o1 > o2; cmp o1 o2` — 0 non-idempotent.
- All 5 non-clock examples produce byte-identical stdout before and after
  formatting. `examples/time.rb` is excluded from the byte comparison because it
  prints `time.now()` nanoseconds, which differ between any two runs; its parity
  is covered by `formatted_programs_behave_exactly_as_the_originals`, which uses
  deterministic fixtures.
- The four `--check` cases above, verified by exit code.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test --all-targets` | **653 passed, 0 failed, 0 ignored** |
| `./rbops/verify.sh phase-015` | **NOT RUN — `rbops/` does not exist in this checkout** |

### Honest note on the fourth gate

`ls rbops/` → `No such file or directory`; `.opencode/` is likewise absent. The
prompt states the invoking pipeline lives elsewhere and must not be inspected, so
`./rbops/verify.sh` was never executed and no claim is made about its output.
Its content is **unknown to this run**, not assumed-passing. If it enforces a
rule the other three gates do not, only that rule is unverified.

`cargo fmt --check`, `cargo clippy -D warnings` and the full
`cargo test --all-targets` suite were each run directly in this tree and passed
as tabulated above.

## What the resumed merge resolved

The tree arrived with unresolved conflicts in `src/formatter.rs` (one hunk) and
`tests/formatter_test.rs` (two hunks), all in the same `try`/`catch`/`finally`
region. Both sides were kept:

- `src/formatter.rs` — `main` gated the catch clause on
  `catch_var.is_some() || !catch_body.is_empty()`; the parked attempt keyed the
  nameless case on `closed_by == Some(TokenKind::Catch)` and wrote a *named*
  catch as `catch <name>` followed by its trailing comment, because
  `write_keyword_line` ends the line with the comment and a name written after
  it lands inside it (`catch err // note` came back as `catch // noteerr`). The
  resolution is the parked attempt's `match`, with `main`'s `|| !catch_body.is_empty()`
  kept as a second witness in the nameless arm, so a bare `catch` is written back
  whether the closing-keyword lookup or the non-empty body witnesses it. A named
  catch now keeps both its binding and its comment.
- `tests/formatter_test.rs` — both sides' `edge_*` tests are present: `main`'s
  `edge_bare_catch_body_is_not_deleted`,
  `edge_bare_catch_program_still_computes_the_same_after_formatting`,
  `edge_named_catch_keeps_its_binding`,
  `edge_finally_still_closes_a_bare_catch`, and the attempt's
  `edge_a_branch_written_on_one_line_keeps_its_keywords`,
  `edge_an_empty_branch_keeps_its_keyword_and_the_comment_written_on_it`.

Two classes of test in the parked attempt did not survive contact with the newer
`main`, and were rewritten rather than deleted:

- `same_statement` was missing `Unless`, `Constant`, `Module` and `Export`, so
  `format_preserves_the_meaning_of_every_corpus_file` reported a false
  difference on `tests/test_control_flow.rb` and any module file. All four arms
  added.
- Four tests asserted that `for … from … to …` and the relational operators were
  *rejected*. Those forms are grammar now (`Statement::ForRange`,
  `BinaryOp::{Less, LessEqual, Greater, GreaterEqual}`, and a `<` token in the
  lexer), so they were rewritten to pin what the formatter does with them —
  bounds and step preserved, both spellings of each comparison normalising to
  the same text, grouping not reordered — plus
  `edge_a_spec_only_form_is_never_reported_as_an_error`, the counterpart of what
  they used to assert. Verified against the binary before rewriting: `rb run` on
  `for each i from 0 to 10 by 5` prints `0 5 10`, and `say 1 is less than 2` and
  `say 1 < 2` both print `yes`.

## Invariants touched

- None of the language-surface invariants in `phases/INVARIANTS.md` are touched.
  No `TokenKind`, `Value` variant, `Error` variant, keyword, or grammar
  production is added, removed, or renamed. `.rb` remains the source extension,
  `to … end` and `if … end` still use `end`, and `set x to <expr>` is untouched.
- `redblue::formatter::needs_reformat` is a `pub` function added by the earlier
  part of this phase and now in `main`. This diff does not change it.

## Known gaps / follow-ups

- `rb format` prints to stdout rather than rewriting in place
  (`src/lib.rs:179`), so `rb format foo.rb > foo.rb` truncates the file. A CLI
  change, not a formatter one → `FINDINGS.md` F3.
- The corpus-wide property tests do not run `rb` on `modules/` files, because a
  module file is imported, not run — `run_program` would execute it and it
  exports rather than says. `edge_a_module_and_its_exports_round_trip` covers the
  module statement form directly instead.
- Grouping-parentheses preservation is covered for the operator levels the
  formatter special-cases. A future operator added to the grammar must be added
  to `format_keeps_grouping_for_unary_and_every_operator_level`, or that test's
  table needs extending — nothing enforces the link.