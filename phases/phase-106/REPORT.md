# Phase 106 — Matrix and numeric array binding

## Finding reproduced

```bash
printf 'say matrix.multiply(matrix.ones(2, 2), matrix.ones(2, 2))\n' > target/tmp_repro.rb
./target/debug/rb run target/tmp_repro.rb
```

Before the change:

```text
Error: AnalyzerError: Unknown variable 'matrix'
```

The first matrix test, `matrix_multiply_matches_hand_written_numbers`, was
written before the implementation and failed with the same missing module.

## What changed

| File | Lines | What |
|---|---|---|
| src/matrix.rs | +488 | dense row-major `f64` matrix, argument validation, shape checks, elementwise operations, `ndarray` matmul/transpose/reductions, indexing and display |
| src/lib.rs | +1 | registers the `matrix` module |
| src/runtime.rs | +6 | dispatches `matrix.*` through the one builtin path and returns a row for `matrix[row]` |
| src/value.rs | +11 −4 | matrix display and `type_of` name |
| src/stdlib.rs | +154 | module, twenty members, builtin names, signatures and the registry loop |
| Cargo.toml | +1 | `ndarray = "0.15"` |
| Cargo.lock | +48 | the `ndarray` dependency graph |
| SPEC.md | +65 | matrix surface, `f64` promotion, exact-shape rules, empty dimensions, indexing and binding cost |
| README.md | +33 | the same public surface in the README |
| SURVEY.md | +59 −58 | re-anchors registry counts and citations; records the `matrix` module |
| tests/matrix_test.rs | +304 | ten matrix tests |
| tests/stdlib_modules_test.rs | +52 | three tracked phase-106 tests, including failure and out-of-bounds checks |
| tests/stdlib_module_docs_test.rs | +100 | twenty answerable `matrix.*` rows |
| tests/stdlib_module_names_test.rs | +5 | a documented `matrix` module call |
| tests/doc_blocks_runs.txt | +2 −2 | SPEC line-key re-anchoring |

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

The same 80×80 multiplication was measured on this host, including the
`./target/debug/rb` process start (about 4.5 ms per run):

| Form | Runs | Mean wall clock |
|---|---|---|
| `matrix.matmul` over `matrix.ones(80, 80)` | 20 | 4.7 ms |
| Three nested Redblue loops over nested lists | 3 | 1483.9 ms |

The binding is about 316× faster per run; the nested-list result is dominated by
interpreted loop and list indexing, not by matrix storage.

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
| matrix_matmul_shape_and_reductions_are_exact | numeric boundary / exact arithmetic |
| edge_phase106_matrix_shape_mismatch_is_a_clean_error | type mismatch / asserts a failure |
| edge_phase106_matrix_out_of_bounds_is_a_clean_error | out_of_bounds / asserts a failure |

Three tracked tests also grew: the two stdlib documentation checks now carry the
new module and builtin rows, and `tests/stdlib_modules_test.rs` carries three
phase-106 tests for the tracked-tree quota and failure/edge coverage. The doc-block
line keys moved with SPEC.md.

## Edge-case matrix

| Class | Status |
|---|---|
| empty | covered — `0x0`, `Nx0`, `0xN`; empty `sum`; empty `mean`/`min`/`max` refuse |
| singleton | covered — 1×1 construction, elementwise, transpose and indexing |
| boundary | covered — row indexing, `at`, transpose of a vector-shaped matrix |
| out_of_bounds | covered — negative, fractional and past-end indexes refuse |
| type_mismatch | covered — non-number elements, non-matrix operands and shape mismatch refuse by name |
| numeric_boundary | covered — integer promotion, exact f64 precision loss and overflow refusal |
| unicode | N/A — the surface accepts numbers only |
| nesting_recursion | covered — `from_lists` validates nested row depth; deep recursion is not exposed |
| duplicate_missing_keys | N/A — matrices have positional dimensions, not keys |
| malformed_input | covered — ragged rows, bad dimensions and empty reductions refuse |
| resource_limit | covered — the 1,000,000-element ceiling and dimension validation |

## Gates

| Gate | Result |
|---|---|
| cargo fmt --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | pass — 2586 passed, 0 failed, 0 ignored |
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
