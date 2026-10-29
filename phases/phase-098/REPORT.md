# Phase 098 — Iteration helpers and memoisation

## What changed
| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +200 −5 | `chain`, `zip`, `enumerate`, `product`, `groupby`, `take`, `skip`, the shared `LruCache`, and the resumed run's fractional-count refusal |
| `src/runtime.rs` (resumed run 2) | +85 −4 | cache keys match function arguments by identity, including a function nested in a list or record, so one declaration can never answer from another's body |
| `src/stdlib.rs` | +62 −2 | registers the seven helpers, their signatures (now saying the `take`/`skip` count is whole), and the hidden `__lru_cache_declare` name the bytecode VM emits |
| `src/parser.rs` | +181 | `lru_cache(n) to ...` decorator parsing, `skip(...)` as a call in statement and expression position, and the cache capacity on a function declaration |
| `src/interpreter.rs` | +58 −10 | registers decorated declarations, keys tree-walker memory by function identity, and drops the replaced binding's memory on rebinding |
| `src/bytecode/codegen.rs` | +30 | emits the hidden cache declaration after a decorated function is stored |
| `src/bytecode/vm.rs` | +106 −9 | keys the compiled engine's memory by function identity, carries it on the frame, and resolves the hidden declaration through the value the `STORE` just bound |
| `src/bytecode/vm.rs` (resumed run) | +23 −15 | binds a function declaration in the declaring frame's own scope, so a sibling closure declaration no longer replaces the earlier closure's cache |
| `src/formatter.rs` | +10 | prints the `lru_cache(n)` decorator |
| `src/value.rs` | +35 −0 | gives `FunctionValue` a unique identity and a constructor that allocates it, so clones share memory and distinct closures never do |
| `src/analyzer.rs` | +4 | accounts for the new function declaration field |
| `src/repl/weight.rs` | +10 −4 | accounts for the new function declaration field and the constructor |
| `src/repl/render.rs` | +6 −5 | builds its display-only function through the constructor |
| `SPEC.md` | +65 −4 | iteration-helper table, zip-shorter rule, group ordering, the memoisation contract, the fractional-count refusal, and that memory belongs to the function value rather than its name; the resumed run also says the decorator is on a plain `to` declaration, not a `to can` method |
| `SPEC.md` (resumed run 2) | +4 | says a function argument is matched by identity, not by the language's `is`, and that the rule reaches a function nested in a list or record |
| `SURVEY.md` | +36 −34 | builtin count 85 → 92 and re-anchored citations |
| `tests/stdlib_module_docs_test.rs` | +632 +70 | iteration/memoisation tests, including the cross-engine map hit, failed-call, far-past-the-end, fractional-count, Unicode, record-grouping, structural cache-key and same-named-closure tests |
| `src/runtime.rs` (tests, resumed run 2) | +38 | unit test that a function argument is keyed by identity and that the rule reaches a list-nested function |
| `tests/stdlib_module_docs_test.rs` (resumed run 2) | +70 | capacity-boundary refusal, `nothing` refusals for the seven helpers, and the SPEC identity-rule check |
| `tests/formatter_test.rs` | +37 −? | constructor update for the cache field |
| `tests/bytecode_test.rs` | +1 | constructor update for the cache field |
| `tests/closure_capture_test.rs` | +6 −5 | hand-built function value goes through the constructor |
| `tests/doc_blocks_runs.txt` | +2 | classifies the two new SPEC blocks |
| `tests/unreviewed_work_ack_test.rs` | +1 | acknowledges the `resume phase-098:` delivery subject |

The helpers materialise lists because Redblue has no iterator value. `zip` stops
at the shorter input and never pads. `groupby` uses Redblue equality and keeps
first-appearance order. `take` and `skip` clamp both bounds and refuse a positive
fractional count rather than truncating it; a negative count still clamps.
`lru_cache` is not a reserved word: it is recognised only immediately before a `to` declaration, and a
variable named `lru_cache` still works. The default capacity is 128.

A cache belongs to the function value the declaration produces. The resumed run
fixed two cross-engine disagreements. First, the bytecode VM looked for a hit
only on a direct `CALL`, so a decorated function passed to `map` ran its body
once per element there while the tree-walker answered from memory; the lookup now
lives in `call_function`, which every call path reaches, and a hit inside `map`
is routed through `advance_map` exactly as a returned frame would be. Second,
both engines keyed the memory by declaration *name*, so two closures built by two
calls to one factory shared one memory: the second closure could answer with the
first closure's captured value, and a cache hit could hide a failure the second
closure would have produced. The key is now `FunctionValue::id`, unique per value
and preserved by cloning, so aliases and recursive reads share their own memory
while same-named closures stay apart. A declaration that replaces the binding of
a name drops that replaced value's memory, which keeps a cached declaration in a
loop from accumulating unreachable caches. Failed calls are not cached on either
engine.

The second resumed run made the key safe for function arguments. The language's
`is` compares two functions by name, parameters and captured bindings, and that
equality cannot be a cache key: two declarations can produce same-shaped
functions with different bodies, so one could answer from the other's entry. A
function argument is now matched by `FunctionValue::id`, and the rule reaches a
function nested in a list or record. Every other value keeps the language's
equality, so the structural cache-key test is unchanged.

The resumed run fixed a third cross-engine disagreement. A function declaration
inside a call was bound by the bytecode VM's `STORE` in the global scope (the
tree-walker binds it in the call's own scope), and `DEF_FUNCTION` resolved the
binding it replaced by walking every live local scope. Declaring a same-named
closure from a second factory call therefore found the first closure through the
global binding and dropped its cache, while the first closure was still
reachable. `DEF_FUNCTION` now resolves the replaced binding exactly as the
tree-walker does — the innermost scope, or the global scope only when there is no
local one — and declares the new name in the current scope before the `STORE`.
Two sibling factory calls each keep their own memory, and the call-count test
now agrees across both engines.

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
| `edge_lru_cache_is_not_shared_by_two_same_named_closures` | identity: two closures from one factory over different captured values answer independently |
| `edge_lru_cache_same_named_closures_fill_independently` | identity plus call count: each closure's second call is a hit of its own memory |
| `edge_lru_cache_does_not_let_one_closure_answer_for_another` | identity and failure: a cached success in one closure cannot hide another's division by zero |
| `edge_lru_cache_survives_sibling_closure_declarations` | identity: a second factory call keeps the first closure's memory; call count compared across engines |
| `edge_lru_cache_hit_refreshes_recency` | boundary: a hit moves its entry to the recent end of the LRU order, proved by eviction and call count |
| `edge_take_and_skip_refuse_a_text_count` | type mismatch: both helpers refuse a text count by name on both engines |
| `edge_lru_cache_keys_function_arguments_by_identity` | identity: two same-shaped function values are equal to `is` but are separate cache keys, including a list-nested function |
| `edge_lru_cache_refuses_a_capacity_beyond_a_count_it_can_hold` | numeric boundary; failure asserted on both engines for `1e300`, and a negative capacity is refused by name |
| `edge_iteration_helpers_refuse_nothing` | type mismatch: `nothing` is refused by name by all seven helpers |
| `spec_says_a_cache_matches_function_arguments_by_identity` | the SPEC identity statement is present |

Mandatory matrix:
- empty — covered by `iteration_helpers_cover_the_empty_list`.
- singleton — covered by `iteration_helpers_cover_singletons_and_many_elements`.
- boundary — covered by `edge_take_and_skip_clamp_at_both_ends` and `edge_zip_stops_at_the_shorter_input`.
- out_of_bounds — covered by the take/skip clamping tests; the helpers do not index.
- type_mismatch — covered by `edge_iteration_helpers_refuse_wrong_types` and `edge_iteration_helpers_refuse_nothing`.
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
| `cargo test --all-targets` | pass — 2318 passed, 0 failed, 0 ignored |
| `./rbops/verify.sh phase-098` | pass |

## Invariants touched
- None to `Value`, `Error` or the grammar. The helpers are new builtin names and
  `lru_cache` is a shape before `to` rather than a keyword.
- Cache keys now match a function argument by identity rather than by the
  language's `is`; `is` itself is unchanged. SPEC.md § Memoisation states the
  distinction.

## Known gaps / follow-ups
- The helpers are eager, and `SPEC.md` says so for every row. A lazy form would
  need an iterator value, which the language does not have.
- `lru_cache` capacity is fixed at declaration time; there is no runtime resize
  or inspection builtin.
- The cache is keyed by `FunctionValue::id`. Identities come from a
  process-wide counter, so a value built by one process cannot be compared to
  one built by another; nothing observable depends on the number itself.
- The bytecode file format did not change. Decorated declarations emit a hidden
  call after the store, so older `.rbc` files still load.
- Phase-077 has not landed, so there was no list-surface `zip`/`enumerate` to
  extend; none was added here.
- A decorated `to can` method still cannot be written: `lru_cache to can f`
  is refused because the decorator parser recognises a plain function name.
  The memoisation contract therefore covers function declarations and calls
  through those values, not object methods; adding method caching would need
  the method's capacity carried into the compiled block format. → FINDINGS.md
