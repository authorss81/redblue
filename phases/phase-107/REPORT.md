# Phase 107 — Add statistics and exact decimal arithmetic

## What changed

| File | Lines | What |
|---|---|---|
| src/decimal.rs | +737 | exact base-10 `Decimal` (`i128` mantissa, 18-place scale), parsing, formatting, exact add/subtract/multiply/remainder, cancelling list sums, rounded division, comparison, conversion helpers, and fourteen unit tests |
| src/statistics.rs | +455 | `mean`, `median`, `mode`, `variance`, `standard_deviation`, `percentile` and `correlation` over number lists, shared argument validation, variance as the mean of squared deviations (so a finite variance cannot overflow on the way), and eleven unit tests |
| src/value.rs | +9 | `Value::Decimal`, equality, display, `type_of` name and truthiness |
| src/runtime.rs | +279 −37 | shared arithmetic/comparison/hash/JSON paths for decimals, explicit decimal/number mixing refusal, and decimal/statistics builtin dispatch |
| src/stdlib.rs | +91 −1 | `decimal` module and members; seven math statistics members; builtin names and signatures (162 builtins, 24 modules) |
| src/lib.rs | +2 | registers the `decimal` and `statistics` modules |
| src/repl/weight.rs | +3 | decimal weight for REPL tracing |
| src/testing/runner.rs | +1 | decimal display in the test harness |
| tests/common/vm.rs | +1 | decimal display in shared VM assertions |
| tests/statistics_decimal_test.rs | +467 | twenty-nine phase tests, including empty/singleton/negative/large-magnitude/2^53/non-terminating-division/large-quotient/remainder/cancelling-sum edges, a finite variance whose squared-deviation sum would overflow, and cross-engine parity |
| tests/stdlib_module_docs_test.rs | +52 | twelve answerable statistics and decimal rows |
| tests/stdlib_module_names_test.rs | +6 | a documented `decimal` module call |
| SPEC.md | +93 −2 | descriptive statistics semantics, decimal arithmetic/conversion rules, the non-finite variance refusal, and the prominent 2^53 number boundary |
| README.md | +18 −1 | statistics lines, decimal section, mixing and 2^53 guidance |
| SURVEY.md | +29 −28 | re-anchors builtin/module/member counts and citations |
| tests/doc_blocks_runs.txt | +36 −39 | re-anchors SPEC block line keys after the new decimal block and classification |
| tests/doc_blocks_fragments.txt | +4 −4 | re-anchors SPEC fragment keys |
| tests/doc_blocks_known_fail.txt | +1 −1 | re-anchors the known-fail key |
| tests/unreviewed_work_ack_test.rs | +1 | adds `resume phase-107:` to the delivery-subject acknowledgement rule so the phase's own delivery commit is covered |

Statistics are population forms; an even median interpolates the two central values,
mode returns every tie in ascending order, percentile interpolates at `(n-1)*p/100`,
and correlation is Pearson. A decimal never mixes silently with a number: the
mixed operation is a catchable runtime error, and `decimal.from_number` /
`decimal.to_number` are the named, documented lossy conversions. Decimal addition,
subtraction, multiplication and remainder are exact while the value fits 18 places
and the integer range; non-terminating division rounds half away from zero.

## Acceptance evidence

| Acceptance item | Evidence |
|---|---|
| mean/median/mode/variance/stdev literal values | `statistics_assert_hand_computed_values` |
| even median interpolation; empty median error | `median_interpolates_an_even_list_and_refuses_an_empty_one` |
| all tied modes in documented order | `mode_returns_all_ties_in_ascending_order` |
| literal percentile and interpolation in SPEC.md | `percentile_uses_linear_interpolation_and_literal_expected_values`; SPEC § Descriptive statistics |
| exact decimal sum not representable in f64 | `decimal_addition_and_sum_are_exact_where_f64_is_not` |
| cross-scale add/subtract/remainder cancel before alignment overflow | `edge_decimal_cross_scale_arithmetic_cancels_before_overflowing`; src/decimal.rs unit tests |
| visible decimal/number mixing rule | `edge_decimal_mixing_with_a_number_is_refused_rather_than_silent`; `edge_decimal_mixing_is_refused_inside_lists_and_membership`; SPEC § decimal |
| 2^53 boundary documented and asserted | SPEC § The `number` boundary is 2^53; `edge_decimal_two_to_the_53_boundary_is_where_integers_stop_being_exact` |
| tree-walker and bytecode agree | `tree_walker_and_bytecode_vm_agree_on_statistics_and_decimals` |
| required edge classes | empty, singleton, negative, very large magnitude, non-terminating division, malformed text, rank/type failures |
| a finite variance is not lost to a sum-of-squares overflow | `edge_variance_survives_a_sum_of_squares_that_would_overflow` |

## Tests added

| Test | Edge class covered |
|---|---|
| statistics_assert_hand_computed_values | literal values |
| median_interpolates_an_even_list_and_refuses_an_empty_one | empty / interpolation / failure |
| mode_returns_all_ties_in_ascending_order | duplicate values / order |
| percentile_uses_linear_interpolation_and_literal_expected_values | boundary / interpolation |
| edge_statistics_reject_empty_lists_and_bad_ranks | empty / out of bounds / failure |
| edge_statistics_handle_a_single_element_and_negative_values | singleton / negative |
| edge_statistics_reject_a_list_that_is_not_all_numbers | type mismatch / failure |
| decimal_addition_and_sum_are_exact_where_f64_is_not | exact arithmetic |
| edge_decimal_mixing_with_a_number_is_refused_rather_than_silent | type mismatch / failure |
| edge_decimal_division_that_does_not_terminate_rounds_to_eighteen_places | non-terminating division |
| edge_decimal_division_of_large_representable_quotients_does_not_overflow | cancelled scales / large representable quotient |
| edge_mean_and_variance_do_not_overflow_on_large_finite_values | very large finite magnitude / cancellation |
| edge_variance_of_opposing_large_finite_values_is_refused_as_non_finite | very large finite magnitude / non-finite refusal |
| edge_variance_survives_a_sum_of_squares_that_would_overflow | very large finite magnitude / finite mean of overflowing sum of squares |
| edge_correlation_refuses_mismatched_lists_with_a_runtime_failure | malformed input / failure |
| edge_mode_treats_negative_zero_as_the_same_value_as_zero | duplicate values / signed zero |
| edge_decimal_from_number_below_the_scale_is_refused_rather_than_zero | precision boundary / failure |
| edge_decimal_product_normalizes_trailing_zeros_before_the_scale_check | precision / cancelled trailing zeros |
| edge_decimal_remainder_is_exact_and_handles_the_minimum_without_panicking | remainder / minimum i128 / zero divisor |
| edge_decimal_cross_scale_arithmetic_cancels_before_overflowing | cross-scale add/subtract/remainder / cancellation before alignment overflow |
| edge_decimal_mixing_is_refused_inside_lists_and_membership | nested decimal/number mixing / unequal-length list / membership failure |
| edge_membership_keeps_a_yes_or_no_answer_for_non_decimal_kinds | non-decimal mixed-kind membership answers yes/no rather than failing |
| edge_decimal_sum_cancels_intermediate_overflow | intermediate overflow / cancellation |
| edge_decimal_parsing_rejects_malformed_and_too_fine_text | malformed input / precision / failure |
| edge_decimal_two_to_the_53_boundary_is_where_integers_stop_being_exact | numeric boundary |
| edge_decimal_handles_a_very_large_magnitude_exactly | very large magnitude |
| edge_decimal_arithmetic_refuses_a_result_beyond_its_precision_or_range | precision / overflow / failure |
| edge_decimal_comparison_and_equality_are_exact_across_scales | scale / equality |
| tree_walker_and_bytecode_vm_agree_on_statistics_and_decimals | cross-engine parity |
| src/decimal.rs unit tests (14) | parsing, negative minimum text and quotient, exact addition, remainder/minimum i128, product normalization, cancelling list sum, cross-scale cancellation, rounded division/zero divisor, large representable quotient, overflow, display, cross-scale comparison |
| src/statistics.rs unit tests (11) | hand values, empty, ties, population forms, large finite mean/variance, percentile rank, correlation, type mismatch |

## Test matrix coverage

| Row | Status |
|---|---|
| empty | covered — every statistic refuses an empty list; `decimal.sum([])` is zero |
| singleton | covered — singleton mean/median/mode/variance/stdev |
| boundary | covered — percentile 0 and 100, 2^53 and 2^53+1, 18-place scale |
| out_of_bounds | covered — percentile ranks -1 and 101 are clean runtime errors |
| type_mismatch | covered — non-number list items, non-list arguments, decimal/number mixing |
| numeric_boundary | covered — 2^53 boundary, i128 mantissa overflow, non-terminating division |
| unicode | N/A — the change consumes numbers and ASCII decimal text; it adds no text-indexing or Unicode-sensitive behaviour |
| nesting_recursion | N/A — statistics are flat reductions and decimal arithmetic is non-recursive; no nesting or call-depth change |
| duplicate_missing_keys | N/A — no record or map key is read or written by this change |
| malformed_input | covered — empty, multi-point, exponent and over-precise decimal text are refused |
| resource_limit | covered — a result past 18 places or the i128 mantissa is refused, never wrapped; list sums cancel before the range check; large finite means do not overflow, while a variance whose squared deviation leaves the finite range is refused as non-finite |

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2651 passed, 0 failed, 0 ignored across 83 test binaries |
| rbops/verify.sh phase-107 | pass (0 failed) |

## Known gaps

- Statistics operate on `number` (`f64`) lists only; there is no statistics over
  `decimal` lists. A decimal reduction is available through `decimal.sum`, which
  is exact.
- `decimal` supports `+`, `-`, `*`, `/`, `mod`, unary negation, equality and
  ordering. There is no decimal square root, power, trigonometry or logarithm;
  those still convert to `number` explicitly with `decimal.to_number`.
- Decimal values are not accepted by every numeric builtin (for example `abs`,
  `floor`, `round` and the matrix functions take numbers). This phase keeps the
  type's conversion boundary explicit rather than broadening every call site.
- Decimal text is parsed from ordinary `1.23` / `-1.23` decimal notation, not
  exponent notation or grouped text such as `1,234.56`.
- No JSON schema change: `json.stringify` writes a decimal as an unquoted exact
  JSON number, and `json.parse` still reads JSON numbers as `number`, because
  JSON has no exact-decimal type. A program that needs a decimal round trip uses
  `decimal.from_text` on text.
