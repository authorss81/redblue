# Phase 106 — Matrix and numeric array binding

## Finding reproduced

```bash
printf 'say matrix.multiply(matrix.ones(2, 2), matrix.ones(2, 2))\n' > target/tmp_repro.rb
./target/debug/rb run target/tmp_repro.rb
```

Before the binding existed:

```text
Error: AnalyzerError: Unknown variable 'matrix'
```

The adopted work in `HEAD` added the binding. This resumed run re-verified it and
found one remaining arithmetic defect: `matrix.mean` summed first, so two finite
`1e308` values overflowed to infinity and the finite mean of `1e308` was refused.

```bash
printf 'say matrix.mean(matrix.from_lists([[1e308, 1e308]]))\n' > target/tmp/repro_mean.rb
./target/debug/rb run target/tmp/repro_mean.rb
```

Before this run's change:

```text
Error: RuntimeError: infinity is not a finite number
```

The new test `edge_mean_of_large_finite_values_stays_finite` failed with that
exact message before the implementation changed.

A later review found that dividing each element by the count could still
overflow at the top of the `f64` range, and that `matrix.sum` had the same
intermediate-overflow class. Both reductions now fall back to an exact
power-of-two scaling before compensated summation, and tests pin the mean and
sum at `f64::MAX` with a non-power-of-two count.

## What changed

| File | Lines | What |
|---|---|---|
| src/matrix.rs | +488 | dense row-major `f64` matrix, argument validation, shape checks, elementwise operations, `ndarray` matmul/transpose/reductions, indexing and display |
| src/matrix.rs | +41 −5 | `matrix.sum` and `matrix.mean` use compensated summation, scale by an exact power of two when that sum would overflow, divide the mean after restoring the scale, check `from_lists` for the element limit before copying, and name the bound for a fractional index |
| src/lib.rs | +1 | registers the `matrix` module |
| src/runtime.rs | +6 | dispatches `matrix.*` through the one builtin path and returns a row for `matrix[row]` |
| src/value.rs | +11 −4 | matrix display and `type_of` name |
| src/stdlib.rs | +154 | module, twenty members, builtin names, signatures and the registry loop |
| Cargo.toml | +1 | `ndarray = "0.15"` |
| Cargo.lock | +48 | the `ndarray` dependency graph |
| SPEC.md | +65 | matrix surface, `f64` promotion, exact-shape rules, empty dimensions, indexing and binding cost |
| SPEC.md | +3 −1 | documents compensated summation and the power-of-two overflow fallback for `matrix.sum` and `matrix.mean` |
| README.md | +33 | the same public surface in the README |
| SURVEY.md | +59 −58 | re-anchors registry counts and citations; records the `matrix` module |
| tests/matrix_test.rs | +390 | sixteen matrix tests (five added by this run) |
| tests/stdlib_modules_test.rs | +52 | three tracked phase-106 tests, including failure and out-of-bounds checks |
| tests/stdlib_module_docs_test.rs | +100 | twenty answerable `matrix.*` rows |
| tests/stdlib_module_names_test.rs | +5 | a documented `matrix` module call |
| tests/doc_blocks_runs.txt | +2 −2 | SPEC line-key re-anchoring |
| tests/unreviewed_work_ack_test.rs | +1 | the phase-106 delivery subject is acknowledged by the delivery rule, not by a stale SHA |

The value is `Value::Object("matrix", …)` with `rows`, `cols` and a flat
row-major `data` field, so no variant was added to the public `Value` enum. All
elements are Redblue numbers (`f64`); integer-looking literals promote without
truncation. Elementwise operations require identical shapes and never broadcast;
a mismatch names both shapes. Matmul names both operand shapes when the inner
dimensions disagree. Division by zero, non-finite results, empty
`mean`/`min`/`max`, negative or fractional indexes, and matrices over 1,000,000
elements are catchable `RuntimeError`s. Empty `sum` is `0`; `0x0`, `Nx0` and
`0xN` shapes have defined elementwise, transpose and reduction behaviour.

The twenty members are `zeros`, `ones`, `identity`, `from_lists`, `to_lists`,
`shape`, `add`, `subtract`, `multiply`, `divide`, `scale`, `matmul`,
`transpose`, `sum`, `mean`, `min`, `max`, `at`, `row` and `column`.

## Cost

The same 80×80 multiplication was measured on this host with the debug binary,
including the `./target/debug/rb` process start:

| Form | Runs | Mean wall clock |
|---|---|---|
| `matrix.matmul` over `matrix.ones(80, 80)` | 3 | 6.0 ms |
| Three nested Redblue loops over nested lists | 3 | 1394.7 ms |

The binding is about 232× faster per run; the nested-list result is dominated by
interpreted loop and list indexing, not by matrix storage. The nested-list
version transposed `b` with `push`, then used indexed `for each` loops for the
product.

## Tests added

| Test | Edge class covered |
|---|---|
| matrix_multiply_matches_hand_written_numbers | numeric boundary / exact arithmetic |
| elementwise_operations_and_reductions_assert_literal_values | nesting / all elementwise and reduction paths |
| edge_zero_by_zero_matrix_has_defined_behaviour | empty |
| edge_shape_mismatch_is_refused_and_names_both_shapes | type mismatch / malformed input |
| edge_integer_and_float_promotion_does_not_truncate | numeric boundary |
| edge_large_values_lose_f64_precision_and_overflow_is_refused | numeric boundary / precision |
| edge_out_of_bounds_and_type_mismatch_are_caught_errors | out_of_bounds / type mismatch |
| edge_indexing_returns_rows_and_single_elements | boundary / indexing |
| edge_dimension_limit_and_bad_dimensions_are_refused | resource limit / malformed input |
| edge_both_engines_produce_equal_matrix_data | cross-engine equality |
| edge_mean_of_large_finite_values_stays_finite | numeric boundary / large finite values including `f64::MAX` with a non-power-of-two count (new) |
| edge_mean_sums_before_dividing_for_exact_and_tiny_values | numeric boundary / exact, odd-count and subnormal means (new) |
| edge_non_square_multiply_matches_hand_written_numbers | boundary / non-square multiply (new) |
| edge_rank_mismatch_is_refused_by_name | nesting / rank mismatch, asserts a failure (new) |
| edge_sum_keeps_large_f64_magnitudes_past_i64 | numeric boundary / f64 magnitude past i64 and finite sums with overflowing intermediates (new) |
| edge_divide_refuses_a_zero_divisor_but_divides_by_non_zero | numeric boundary / asserts a failure |
| matrix_matmul_shape_and_reductions_are_exact | numeric boundary / exact arithmetic |
| edge_phase106_matrix_shape_mismatch_is_a_clean_error | type mismatch / asserts a failure |
| edge_phase106_matrix_out_of_bounds_is_a_clean_error | out_of_bounds / asserts a failure |

`tests/unreviewed_work_ack_test.rs` also recognises `resume phase-106:` as this
phase's delivery subject, so the adopted work commit is acknowledged by the
existing rule rather than pinned to a SHA the pipeline rebuilds. The doc-block
line keys moved by two with the SPEC paragraph.

## Edge-case matrix

| Class | Status |
|---|---|
| empty | covered — `0x0`, `Nx0`, `0xN`; empty `sum`; empty `mean`/`min`/`max` refuse |
| singleton | covered — 1×1 construction, elementwise, transpose, indexing and mean |
| boundary | covered — row indexing, `at`, transpose of a vector-shaped matrix, non-square matmul |
| out_of_bounds | covered — negative, fractional and past-end indexes refuse |
| type_mismatch | covered — non-number elements, non-matrix operands and shape mismatch refuse by name |
| numeric_boundary | covered — integer promotion, exact f64 precision loss, exact and subnormal means, finite means and sums at `f64::MAX`, f64 magnitude past i64 and overflow refusal |
| unicode | N/A — the surface accepts numbers only |
| nesting_recursion | covered — `from_lists` validates nested row depth; a flat or rank-3 row is refused, and deep recursion is not exposed |
| duplicate_missing_keys | N/A — matrices have positional dimensions, not keys |
| malformed_input | covered — ragged rows, bad dimensions and empty reductions refuse |
| resource_limit | covered — the 1,000,000-element ceiling and dimension validation |

## Gates

| Gate | Result |
|---|---|
| cargo fmt --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | pass — 2592 passed, 0 failed, 0 ignored |
| rbops/verify.sh phase-106 | pass |

## Invariants touched

- Dotted calls remain gated by `module_has_member`; `matrix` and its twenty
  members are in `MODULES` and `MODULE_MEMBERS` in both directions.
- The public `Value` enum is unchanged; matrix is an object with private field
  names, and existing display/`type_of` behaviour is extended by name.
- Both engines reach the same matrix implementation through `runtime::builtin`;
  a test asserts equal data from the tree interpreter and the bytecode VM.
- Matrix operations do not mutate their operands; every result is a fresh
  matrix, and `to_lists` returns fresh nested lists.
- Errors are catchable and name the offending shape, index, dimension or type.

## Known gaps / follow-ups

- No broadcasting, complex numbers, matrix slicing, linear solves or
  decompositions; the contract is exact-shape arithmetic, matmul and reductions.
- `from_lists` accepts only rectangular nested lists of numbers and refuses
  ragged input by row.
- Storage is row-major `f64`; very large values follow `f64` precision and
  non-finite results are refused rather than stored.
