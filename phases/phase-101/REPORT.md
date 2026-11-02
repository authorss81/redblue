# Phase 101 — Add the text utilities that formatting and diffing need

## What changed

| File | Lines | What |
|---|---|---|
| src/stdlib.rs | +610 −0 | added `text.wrap`, `text.diff`, `text.dedent`, `text.template`, `text.pad`, `text.align`, their dispatch/signature entries and helpers (kept out of the flat registry so the bare names are not advertised); `dedent` now preserves CRLF line endings rather than rewriting them to LF; padded and aligned output is size-capped and allocated with `try_reserve_exact` using the widest fill character rather than the fill's byte length, and `diff` refuses a line table past its cap; the six utilities are reachable only through the `text` module, and a bare `text_wrap(...)`-style name is refused on both engines. On resume, `wrap` tracks its current line width incrementally instead of recounting every word, so a long word-heavy line is linear rather than quadratic |
| tests/text_utilities_test.rs | +740 −0 | twenty-two tests, both-engine comparison, catchable failures and an independent unified-diff reader |
| SPEC.md | +43 −0 | documented the six operations, width semantics, template delimiters and `dedent` line-ending preservation |
| README.md | +8 −1 | added the six operations to the Text section |
| SURVEY.md | +51 −47 | re-anchored `src/stdlib.rs` citations, kept the registry count at 114 with the six module-only utilities outside it, member count 85→91, and recorded the rank-5 padding change |
| tests/stdlib_module_docs_test.rs | +30 −0 | answerable-builtin rows for the six new module-only names |
| tests/doc_blocks_runs.txt, tests/doc_blocks_fragments.txt, tests/doc_blocks_known_fail.txt | +15 −15 | re-anchored SPEC block line keys after the documentation insertions |
| tests/unreviewed_work_ack_test.rs | +1 −0 | added `resume phase-101:` to the delivery-subject prefixes so the resume commit that carries this phase is acknowledged |

The new operations are `text.wrap(text, width)`, `text.diff(old, new)`,
`text.dedent(text)`, `text.template(template, record)`,
`text.pad(text, width, fill)` and `text.align(text, width, side)`. They are
reachable only through the `text` module, so a user declaration such as
`to wrap(prefix)` is not shadowed by a new flat builtin.

## Tests added

| Test | Edge class covered |
|---|---|
| wrap_keeps_words_whole_and_only_breaks_a_word_longer_than_the_width | boundary / word longer than width |
| edge_wrap_handles_empty_single_no_trailing_newline_crlf_and_long_lines | empty / singleton / no trailing newline / CRLF / long line |
| diff_is_empty_for_identical_text_and_pinpoints_one_changed_line | empty result / one changed line |
| edge_diff_handles_empty_and_no_trailing_newline_inputs | empty / no trailing newline |
| edge_diff_round_trips_separate_hunks_through_a_standard_reader | malformed input / standard-reader round trip / multiple hunks |
| dedent_removes_the_common_prefix_and_leaves_an_unaligned_line_alone | common-prefix boundary / unaligned line |
| template_substitutes_a_value_escapes_a_delimiter_and_names_an_unknown_placeholder | duplicate/missing keys / escaped delimiter / catchable failure |
| pad_and_align_produce_exact_character_widths_for_non_ascii_text | unicode / character-vs-byte width |
| edge_a_bad_text_utility_argument_is_a_catchable_error_named_by_the_function | type mismatch / failure asserted / unknown side with text past the width |
| edge_text_utilities_refuse_widths_past_the_documented_limit_on_both_engines | numeric boundary / width past the cap refused on both engines |
| edge_text_diff_refuses_a_line_table_past_the_documented_limit_on_both_engines | resource limit / oversized diff refused on both engines |
| dedent_preserves_crlf_line_endings_while_removing_the_common_prefix | CRLF input / line-ending preservation |
| edge_dedent_handles_blank_lines_tabs_and_no_final_newline | empty/blank / tabs / no trailing newline / CRLF |
| edge_text_utilities_refuse_invalid_widths_and_sides_on_both_engines | boundary (width 0) / failure asserted on both engines |
| edge_text_utilities_refuse_mismatched_argument_types_on_both_engines | type mismatch / failure asserted on both engines |
| edge_text_diff_refuses_a_non_text_argument_on_both_engines | type mismatch / direct `is_err()` failure assertion on both engines |
| edge_wrap_breaks_a_long_word_after_a_partial_line_and_counts_characters | boundary / word longer than width / unicode character-vs-byte |
| edge_pad_and_align_handle_multicharacter_fill_and_exact_widths | boundary / empty text / unicode fill / exact width |
| edge_template_repeats_a_placeholder_and_leaves_a_literal_close_alone | duplicate keys / empty template / literal closing delimiter |
| edge_wrap_collapses_runs_of_spaces_and_keeps_blank_lines | boundary / whitespace runs / blank line |
| edge_pad_with_a_large_multicharacter_fill_reserves_only_the_output | resource limit / long fill / exact width |
| edge_the_six_text_utilities_are_reached_only_through_the_text_module | module-only spelling / bare internal name refused / user `wrap` name |

All twenty-two are `#[test]` functions; sixteen are named `edge_*`. Every test that
can succeed is checked on the tree-walker and the bytecode VM and compared as data.

## Test matrix

- empty — covered: empty wrap input, empty diff inputs, empty pad input, and an empty template.
- singleton — covered: single-line wrap and one-line diff inputs.
- boundary — covered: exact width, word exactly at width, word longer than width, no trailing newline, CRLF, and width `0` refused.
- out_of_bounds — N/A: these are text transformations, not index lookups; hunk ranges are generated from the inputs and never index outside them.
- type_mismatch — covered: `text.wrap(1, 5)`, `text.diff("a", 1)`, `text.dedent(1)`, `text.template("<<name>>", 1)`, a non-numeric width and a non-text fill all produce catchable errors naming the operation on both engines.
- numeric_boundary — covered: a width past 1,000,000 is refused rather than converted to a huge allocation; widths at or below the cap are ordinary whole character counts.
- unicode — covered: `pad`, `align` and `wrap` use character counts for `é` and CJK text, where bytes differ; a CJK word longer than the width is split by characters, and a non-ASCII fill repeats by character.
- nesting_recursion — N/A: the utilities operate on flat text; template substitution is a single pass with no recursive evaluation.
- duplicate_missing_keys — covered: unknown template placeholders are named in a catchable error; repeated placeholders reuse the same field and a literal `>>` is left alone.
- malformed_input — covered: an unclosed template placeholder, an empty placeholder, an empty pad fill and an unknown alignment side are clean errors, including an unknown side when the text is already past the width; `dedent` preserves CRLF.
- resource_limit — covered: `wrap`/`pad`/`align` cap the width at 1,000,000 characters, `pad`/`align` reserve their output with `try_reserve_exact` bounded by the widest emitted character rather than the whole fill, and `diff` refuses a line table past 4,000,000 cells. Each refusal is a catchable `RuntimeError` naming the operation; `wrap` also no longer recounts the current line for every word.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2461 passed, 0 failed |
| rbops/verify.sh | pass |

## Invariants touched

- None. No existing language surface or value/error type changed.

## Known gaps / follow-ups

- `text.diff` is a standard unified diff with three context lines; it is
  intentionally not a merge or patch tool.
- Its line table is the usual quadratic-in-lines LCS table, now bounded at
  4,000,000 cells. That is fine for source-sized texts; a larger pair is refused
  with a catchable error rather than attempting the allocation. There is still no
  linear-space fallback, so the cap is the resource limit rather than a claim
  that the diff is linear.
