# Phase 101 — Add the text utilities that formatting and diffing need

## What changed

| File | Lines | What |
|---|---|---|
| src/stdlib.rs | +544 −0 | added `text.wrap`, `text.diff`, `text.dedent`, `text.template`, `text.pad`, `text.align`, their registry/dispatch/signature entries and helpers |
| tests/text_utilities_test.rs | +458 −0 | nine tests, both-engine comparison, catchable failures and an independent unified-diff reader |
| SPEC.md | +43 −0 | documented the six operations, width semantics and template delimiters |
| README.md | +8 −1 | added the six operations to the Text section |
| SURVEY.md | +51 −47 | re-anchored `src/stdlib.rs` citations, registry count 114→120, member count 85→91, and recorded the rank-5 padding change |
| tests/stdlib_module_docs_test.rs | +30 −0 | answerable-builtin rows for the six new registry names |
| tests/doc_blocks_runs.txt, tests/doc_blocks_fragments.txt, tests/doc_blocks_known_fail.txt | +15 −15 | re-anchored SPEC block line keys after the documentation insertions |

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
| edge_a_bad_text_utility_argument_is_a_catchable_error_named_by_the_function | type mismatch / failure asserted |

All nine are `#[test]` functions; five are named `edge_*`. Every test that can
succeed is checked on the tree-walker and the bytecode VM and compared as data.

## Test matrix

- empty — covered: empty wrap input, empty diff inputs, empty pad input.
- singleton — covered: single-line wrap and one-line diff inputs.
- boundary — covered: exact width, word exactly at width, word longer than width, no trailing newline, CRLF.
- out_of_bounds — N/A: these are text transformations, not index lookups; hunk ranges are generated from the inputs and never index outside them.
- type_mismatch — covered: `text.wrap(1, 5)` and the other bad-argument cases produce catchable errors naming the operation.
- numeric_boundary — N/A: widths are non-negative character counts; no floating-point or integer-overflow arithmetic is involved.
- unicode — covered: `pad` and `align` use character counts for `é` and CJK text, where bytes differ.
- nesting_recursion — N/A: the utilities operate on flat text; template substitution is a single pass with no recursive evaluation.
- duplicate_missing_keys — covered: unknown template placeholders are named in a catchable error; repeated placeholders reuse the same field.
- malformed_input — covered: an unclosed template placeholder, an empty placeholder and an empty pad fill are clean errors.
- resource_limit — N/A: the operations are linear in input size and add no unbounded recursion or external resource.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2448 passed, 0 failed |
| rbops/verify.sh | pending |

## Invariants touched

- None. No existing language surface or value/error type changed.

## Known gaps / follow-ups

- None. `text.diff` is a standard unified diff with three context lines; it is
  intentionally not a merge or patch tool.
