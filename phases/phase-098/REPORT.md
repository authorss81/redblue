# Phase 098 — Iteration helpers and memoisation

## What changed
| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +200 −5 | `chain`, `zip`, `enumerate`, `product`, `groupby`, `take`, `skip`, the shared `LruCache`, and the resumed run's fractional-count refusal |
| `src/stdlib.rs` | +62 −2 | registers the seven helpers, their signatures (now saying the `take`/`skip` count is whole), and the hidden `__lru_cache_declare` name the bytecode VM emits |
| `src/parser.rs` | +181 | `lru_cache(n) to ...` decorator parsing, `skip(...)` as a call in statement and expression position, and the cache capacity on a function declaration |
| `src/interpreter.rs` | +39 | registers decorated declarations and performs tree-walker cache lookup/insertion |
| `src/bytecode/codegen.rs` | +30 | emits the hidden cache declaration after a decorated function is stored |
| `src/bytecode/vm.rs` | +71 −7 | registers decorated declarations and carries a cache on the frame; this resumed run moved the hit check into `call_function` so `map`, method calls and direct calls all consult the same declaration cache |
| `src/formatter.rs` | +10 | prints the `lru_cache(n)` decorator |
| `src/analyzer.rs` | +4 | accounts for the new function declaration field |
| `src/repl/weight.rs` | +4 | accounts for the new function declaration field |
| `SPEC.md` | +58 −4 | iteration-helper table, zip-shorter rule, group ordering, the memoisation contract, the fractional-count refusal, and that a cache belongs to the declaration rather than one call spelling |
| `SURVEY.md` | +36 −34 | builtin count 85 → 92 and re-anchored citations |
| `tests/stdlib_module_docs_test.rs` | +497 +70 | iteration/memoisation tests, including the cross-engine map hit, failed-call, far-past-the-end, fractional-count, Unicode, record-grouping and structural cache-key tests |
| `tests/formatter_test.rs` | +37 −? | constructor update for the cache field |
| `tests/bytecode_test.rs` | +1 | constructor update for the cache field |
| `tests/doc_blocks_runs.txt` | +2 | classifies the two new SPEC blocks |
| `tests/unreviewed_work_ack_test.rs` | +1 | acknowledges the `resume phase-098:` delivery subject |

The helpers materialise lists because Redblue has no iterator value. `zip` stops
at the shorter input and never pads. `groupby` uses Redblue equality and keeps
first-appearance order. `take` and `skip` clamp both bounds and refuse a positive
fractional count rather than truncating it; a negative count still clamps.
`lru_cache` is not a reserved word: it is recognised only immediately before a `to` declaration, and a
variable named `lru_cache` still works. The default capacity is 128.

A cache belongs to the declaration. The resumed run fixed a cross-engine
disagreement: the bytecode VM looked for a hit only on a direct `CALL`, so a
decorated function passed to `map` ran its body once per element there while the
tree-walker answered from memory. The lookup now lives in `call_function`, which
every call path reaches, and a hit inside `map` is routed through `advance_map`
exactly as a returned frame would be. Failed calls are not cached on either
engine.

Phase-077 is not done in this tree (`.done` is absent), so it had not added `zip`
or `enumerate` to the list surface. This phase added no list members; the
helpers are flat builtins only, which avoids creating a second spelling.

## Tests added
| Test | Edge class covered |
|---|---|
| `iteration_helpers_cover_the_empty_list` | every helper on empty input |
| `iteration_helpers_cover_singletons_and_many_elements` | single-element and multi-element literal answers |
| `edge_zip_stops_at_the_shorter_input` | zip shorter-input rule, both directions |
| `edge_take_and_skip_clamp_at_both_ends` | negative, zero and past-end counts |
| `edge_take_and_skip_saturate_a_count_far_past_the_end` | numeric boundary: ±1e300 counts |
| `edge_take_and_skip_refuse_a_fractional_count` | type coercion boundary; failure asserted on both engines |
| `edge_iteration_helpers_preserve_unicode_items` | Unicode: emoji, CJK, RTL and a combining mark |
| `edge_groupby_groups_equal_records_and_keeps_missing_fields_distinct` | duplicate/missing keys: equal records group, differing field sets do not |
| `edge_lru_cache_keys_on_structural_argument_equality` | cache keys compare list and record contents by equality, asserted by call count |
| `edge_groupby_keeps_first_appearance_order_and_groups_by_equality` | group order and equality, including lists |
| `edge_iteration_helpers_refuse_wrong_types` | every helper's type-refusal channel |
| `edge_lru_cache_evicts_at_its_capacity` | capacity bound and call-count rise after eviction |
| `edge_lru_cache_requires_a_positive_capacity` | zero, fractional and non-numeric capacities |
| `edge_lru_cache_does_not_record_a_failed_call` | failure assertion on both engines; retry runs the body again |
| `edge_lru_cache_hits_inside_map_on_both_engines` | memoisation on a non-direct call path, compared across engines |
| `lru_cache_makes_fewer_recursive_calls_than_uncached` | observable memoisation by call count |
| `lru_cache_defaults_to_a_capacity_of_128` | default capacity and eviction |
| `edge_lru_cache_memoises_a_body_that_ends_in_an_expression` | implicit final answer is cached |
| `edge_redeclaring_a_cached_function_undecorated_drops_its_memory` | redeclaration drops the old cache |
| `edge_lru_cache_does_not_survive_a_bytecode_run` | cache is per run |
| `iteration_helpers_agree_across_both_engines` | tree-walker and bytecode values compared as data |
| `spec_says_each_iteration_helper_materialises` | SPEC table is present, complete and says "materialises" |

Mandatory matrix:
- empty — covered by `iteration_helpers_cover_the_empty_list`.
- singleton — covered by `iteration_helpers_cover_singletons_and_many_elements`.
- boundary — covered by `edge_take_and_skip_clamp_at_both_ends` and `edge_zip_stops_at_the_shorter_input`.
- out_of_bounds — covered by the take/skip clamping tests; the helpers do not index.
- type_mismatch — covered by `edge_iteration_helpers_refuse_wrong_types`.
- numeric_boundary — covered by the ±1e300 count test and the capacity refusals; a non-finite number cannot reach a helper because the language refuses one before it becomes a value.
- unicode — covered by `edge_iteration_helpers_preserve_unicode_items`, which moves emoji, CJK, RTL and a combining mark through every helper.
- nesting_recursion — covered by the recursive Fibonacci test and groupby over equal nested lists.
- duplicate_missing_keys — covered by `edge_groupby_groups_equal_records_and_keeps_missing_fields_distinct` and the structural cache-key test; a produced group always has both `key` and `items`.
- malformed_input — covered by the capacity and wrong-type refusal tests.
- resource_limit — covered by the capacity/eviction tests; the cache does not add an unbounded store because the declared bound evicts.

## Gates
| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | pass — 2308 passed, 0 failed, 0 ignored |
| `./rbops/verify.sh phase-098` | pass |

## Invariants touched
- None. The helpers are new builtin names, `lru_cache` is a shape before `to`
  rather than a keyword, and no existing `Value`, `Error`, or grammar spelling
  changed.

## Known gaps / follow-ups
- The helpers are eager, and `SPEC.md` says so for every row. A lazy form would
  need an iterator value, which the language does not have.
- `lru_cache` capacity is fixed at declaration time; there is no runtime resize
  or inspection builtin.
- The cache is keyed by declaration name. Two same-named declarations in
  different scopes are not distinguished, matching the language's name-based
  function lookup.
- The bytecode file format did not change. Decorated declarations emit a hidden
  call after the store, so older `.rbc` files still load.
- Phase-077 has not landed, so there was no list-surface `zip`/`enumerate` to
  extend; none was added here.
