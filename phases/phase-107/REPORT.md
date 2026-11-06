# Phase 107 — Add statistics and exact decimal arithmetic

## What changed

| File | Lines | What |
|---|---|---|
| src/decimal.rs | +475 | exact base-10 `Decimal` (`i128` mantissa, 18-place scale), parsing, formatting, exact add/subtract/multiply/remainder, rounded division, comparison, conversion helpers, and eight unit tests |
| src/statistics.rs | +447 | `mean`, `median`, `mode`, `variance`, `standard_deviation`, `percentile` and `correlation` over number lists, shared argument validation, and ten unit tests |
| src/value.rs | +9 | `Value::Decimal`, equality, display, `type_of` name and truthiness |
| src/runtime.rs | +275 −33 | shared arithmetic/comparison/hash/JSON paths for decimals, explicit decimal/number mixing refusal, and decimal/statistics builtin dispatch |
| src/stdlib.rs | +91 −1 | `decimal` module and members; seven math statistics members; builtin names and signatures (162 builtins, 24 modules) |
| src/lib.rs | +2 | registers the `decimal` and `statistics` modules |
| src/repl/weight.rs | +3 | decimal weight for REPL tracing |
| src/testing/runner.rs | +1 | decimal display in the test harness |
| tests/common/vm.rs | +1 | decimal display in shared VM assertions |
| tests/statistics_decimal_test.rs | +392 | eighteen phase tests, including empty/singleton/negative/large-magnitude/2^53/non-terminating-division/large-quotient edges and cross-engine parity |
| tests/stdlib_module_docs_test.rs | +52 | twelve answerable statistics and decimal rows |
| tests/stdlib_module_names_test.rs | +6 | a documented `decimal` module call |
| SPEC.md | +89 −1 | descriptive statistics semantics, decimal arithmetic/conversion rules, and the prominent 2^53 number boundary |
| README.md | +18 −1 | statistics lines, decimal section, mixing and 2^53 guidance |
| SURVEY.md | +29 −28 | re-anchors builtin/module/member counts and citations |
| tests/doc_blocks_runs.txt | +36 −39 | re-anchors SPEC block line keys after the new decimal block and classification |
| tests/doc_blocks_fragments.txt | +4 −4 | re-anchors SPEC fragment keys |
| tests/doc_blocks_known_fail.txt | +1 −1 | re-anchors the known-fail key |

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
| visible decimal/number mixing rule | `edge_decimal_mixing_with_a_number_is_refused_rather_than_silent`; SPEC § decimal |
| 2^53 boundary documented and asserted | SPEC § The `number` boundary is 2^53; `edge_decimal_two_to_the_53_boundary_is_where_integers_stop_being_exact` |
| tree-walker and bytecode agree | `tree_walker_and_bytecode_vm_agree_on_statistics_and_decimals` |
| required edge classes | empty, singleton, negative, very large magnitude, non-terminating division, malformed text, rank/type failures |

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
| edge_decimal_parsing_rejects_malformed_and_too_fine_text | malformed input / precision / failure |
| edge_decimal_two_to_the_53_boundary_is_where_integers_stop_being_exact | numeric boundary |
| edge_decimal_handles_a_very_large_magnitude_exactly | very large magnitude |
| edge_decimal_arithmetic_refuses_a_result_beyond_its_precision_or_range | precision / overflow / failure |
| edge_decimal_comparison_and_equality_are_exact_across_scales | scale / equality |
| tree_walker_and_bytecode_vm_agree_on_statistics_and_decimals | cross-engine parity |
| src/decimal.rs unit tests (8) | parsing, exact addition, rounded division/zero divisor, large representable quotient, overflow, display, cross-scale comparison |
| src/statistics.rs unit tests (10) | hand values, empty, ties, population forms, large finite mean/variance, percentile rank, correlation, type mismatch |

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2629 passed, 0 failed, 0 ignored across 83 test binaries |
| rbops/verify.sh phase-107 | pending |

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
