# Phase 098 — Iteration helpers and memoisation

## What changed
| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +281 −0 | `chain`, `zip`, `enumerate`, `product`, `groupby`, `take`, `skip`, the shared `LruCache`, the fractional-count refusal, and the identity-aware cache key |
| `src/stdlib.rs` | +69 −0 | registers the seven helpers and their signatures, plus the hidden `__lru_cache_declare` and `__lru_cache_declare_method` names the bytecode VM emits |
| `src/parser.rs` | +201 −4 | `lru_cache(n) to ...` and `lru_cache(n) to can ...` decorator parsing, the cache capacity on function and method declarations, and `skip(...)` as a call in statement and expression position |
| `src/interpreter.rs` | +73 −13 | registers decorated functions and methods, keys tree-walker memory by function identity, drops a replaced binding's memory, and clears object-method memory between runs |
| `src/bytecode/codegen.rs` | +76 −6 | emits the hidden cache declaration after a decorated function is stored, and the hidden method declaration after the owning object is registered |
| `src/bytecode/vm.rs` | +202 −31 | keys the compiled engine's memory by function identity, carries it on the frame, resolves both hidden declarations, and clears object-method memory between runs |
| `src/value.rs` | +137 −1 | gives `FunctionValue` a unique identity, a constructor that allocates it, and the cache shared by every clone of one value |
| `src/formatter.rs` | +18 −2 | prints the `lru_cache(n)` decorator on both functions and methods |
| `src/analyzer.rs` | +4 −1 | accounts for the cache field on function and method declarations |
| `src/repl/weight.rs` | +17 −13 | accounts for the new declaration fields and the constructor |
| `src/repl/render.rs` | +6 −6 | builds its display-only function through the constructor |
| `src/repl/mod.rs` | +2 −0 | registers the cache-related REPL plumbing |
| `SPEC.md` | +70 −0 | iteration-helper table, zip-shorter rule, group ordering, the memoisation contract (including `to can`), the fractional-count refusal, and that memory belongs to the function value rather than its name |
| `SURVEY.md` | +31 −31 | builtin count 85 → 92 and re-anchored citations |
| `tests/stdlib_module_docs_test.rs` | +958 −0 | iteration, memoisation, cross-engine, identity, capacity, Unicode, type-refusal and method-cache tests |
| `tests/formatter_test.rs` | +74 −2 | cache-field constructors and a formatted `lru_cache` method round trip |
| `tests/bytecode_test.rs` | +1 −0 | constructor update for the cache field |
| `tests/bytecode_vm_test.rs` | +44 −0 | a module cache declaration is not shadowed by an importer local |
| `tests/closure_capture_test.rs` | +6 −6 | hand-built function value goes through the constructor |
| `tests/doc_blocks_runs.txt` | +2 −0 | classifies the new SPEC blocks |
| `tests/unreviewed_work_ack_test.rs` | +1 −0 | acknowledges the `resume phase-098:` delivery subject |
| `phases/phase-098/FINDINGS.md` | +13 −0 | records the remaining cache follow-ups |
| `src/runtime.rs` | +13 −7 | resumed run: `chain` now joins any number of lists (none is the empty list) instead of exactly two |
| `src/stdlib.rs` | +1 −1 | resumed run: `chain` signature says `list, list, ...` |
| `SPEC.md` | +1 −1 | resumed run: `chain` row states the open arity |
| `tests/stdlib_module_docs_test.rs` | +58 −0 | resumed run: three `chain` arity and refusal tests |
| `src/runtime.rs` | +22 −9 | final resumed run: `zip` joins any number of lists, at least two, and stops at the shortest |
| `src/stdlib.rs` | +1 −1 | final resumed run: `zip` signature says `list, list, ...` and describes rows |
| `SPEC.md` | +2 −1 | final resumed run: the `zip` row states the open arity, and the example shows a three-list row |
| `tests/stdlib_module_docs_test.rs` | +81 −1 | final resumed run: four variadic-`zip` tests, and `both` now compares values as data as well as text |
| `tests/doc_blocks_runs.txt` | +1 −1 | final resumed run: re-anchors the memoisation SPEC block after the example line moved |

The helpers materialise lists because Redblue has no iterator value. `zip` takes
at least two lists, produces one row per position with one item from each list,
stops at the shortest input and never pads. `groupby` uses Redblue equality and keeps
first-appearance order, and finds a group through a keyed lookup so its cost
grows with the input rather than comparing every item with every earlier key.
`product` refuses to materialise more than 1,000,000 pairs and reports
`Error::Limit`; a builtin call is one step, so the step budget cannot bound an
answer whose length is the product of two lists. `take` and `skip` clamp both
bounds and refuse a positive fractional count rather than truncating it; a
negative count still clamps.
`lru_cache` is not a reserved word: it is recognised only immediately before a
`to` declaration (plain or `to can`), and a variable named `lru_cache` still
works. The default capacity is 128.

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

The resumed run also closed the method gap. `lru_cache(n) to can f` is now
parsed as a decorated method. The tree-walker attaches the capacity when it
builds the method value; the compiled engine emits a hidden three-argument
declaration after `DEF_OBJECT`/`STORE`, so the object is already registered when
the cache is attached and the `.rbc` format does not change. Both engines clear
object-method memory at the start of a run, the same way they clear function
memory, so a second program on one VM cannot answer from the first run's method
cache. A top-level `to can` declaration is an ordinary binding and uses the
same hidden call a decorated function does. SPEC.md now says the decorator is
also written on a `to can` declaration.

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
| `edge_groupby_of_many_distinct_keys_keeps_first_appearance_order` | resource: 65,536 distinct keys keep first-appearance order through the keyed lookup |
| `edge_product_refuses_a_product_past_the_materialised_limit` | resource limit: a 1,001 × 1,001 product is refused on both engines instead of being reserved |
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
| `edge_lru_cache_refuses_a_capacity_beyond_a_count_it_can_hold` | numeric boundary; failure asserted on both engines for `1e300` and the exact 2^64 `f64` boundary, and a negative capacity is refused by name |
| `edge_iteration_helpers_refuse_nothing` | type mismatch: `nothing` is refused by name by all seven helpers |
| `spec_says_a_cache_matches_function_arguments_by_identity` | the SPEC identity statement is present |
| `edge_lru_cache_map_of_a_large_repeated_value_does_not_recurse` | resource/recursion: 100,000 cache hits in `map` do not overflow the host stack |
| `edge_lru_cache_in_a_loop_keeps_only_escaped_closures_memory` | resource: only escaped closures keep a memory; call count is literal |
| `lru_cache_memoises_an_object_method_on_both_engines` | method memoisation, call count compared across engines |
| `edge_lru_cache_evicts_an_object_methods_entries` | boundary: a method cache of capacity one evicts on the stated bound |
| `edge_lru_cache_refuses_a_bad_capacity_on_a_method` | failure: zero and fractional method capacities are refused by both parser paths |
| `edge_a_method_cache_does_not_survive_a_bytecode_run` | resource/state: method memory is cleared between runs |
| `lru_cache_memoises_a_top_level_can_declaration` | a decorated `to can` outside an object is memoised on both engines |
| `edge_a_cached_method_formats_into_a_program_that_still_runs` | formatter: the method decorator round trips and the formatted program still runs |
| `edge_chain_joins_any_number_of_lists` | empty, singleton and multi-list arity, including Unicode items, on both engines |
| `edge_chain_refuses_a_non_list_among_its_arguments` | type mismatch: a non-list in any position is refused by name on both engines |
| `edge_chain_of_no_or_one_list_is_an_ordinary_answer` | boundary: zero arguments is a real empty answer and one argument is a copy |
| `edge_zip_joins_more_than_two_lists_and_stops_at_the_shortest` | three- and four-list rows, with a different list shortest in each position |
| `edge_zip_covers_singletons_and_many_inputs` | singleton and five-input arity, plus a Unicode row on both engines |
| `edge_zip_refuses_fewer_than_two_lists` | failure: zero and one argument are refused by name on both engines |
| `edge_zip_refuses_a_non_list_among_its_arguments` | type mismatch: a non-list in any position of a longer call is refused by name on both engines |

Mandatory matrix:
- empty — covered by `iteration_helpers_cover_the_empty_list`.
- singleton — covered by `iteration_helpers_cover_singletons_and_many_elements`.
- boundary — covered by `edge_take_and_skip_clamp_at_both_ends`, `edge_zip_stops_at_the_shorter_input` and the variadic `zip` tests, which make each input shortest in turn.
- out_of_bounds — covered by the take/skip clamping tests; `zip` builds only `0..shortest` rows, so it never indexes past an input; the other helpers do not index.
- type_mismatch — covered by `edge_iteration_helpers_refuse_wrong_types`, `edge_iteration_helpers_refuse_nothing`, and the two variadic-`zip` refusal tests.
- numeric_boundary — covered by the ±1e300 count test and the capacity refusals; a non-finite number cannot reach a helper because the language refuses one before it becomes a value.
- unicode — covered by `edge_iteration_helpers_preserve_unicode_items`, which moves emoji, CJK, RTL and a combining mark through every helper.
- nesting_recursion — covered by the recursive Fibonacci test and groupby over equal nested lists.
- duplicate_missing_keys — covered by `edge_groupby_groups_equal_records_and_keeps_missing_fields_distinct` and the structural cache-key test; a produced group always has both `key` and `items`.
- malformed_input — covered by the capacity and wrong-type refusal tests.
- resource_limit — covered by the capacity/eviction tests and the product limit; the cache does not add an unbounded store because the declared bound evicts, and method memory is cleared with the run. The 100,000-hit `map` test covers the cache-hit path without host recursion, and the 65,536-key `groupby` test covers grouping without a quadratic scan.

## Gates
| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | pass — 2336 passed, 0 failed, 0 ignored |
| `./rbops/verify.sh phase-098` | pass |

## Invariants touched
- None to `Value`, `Error` or the grammar. The helpers are new builtin names and
  `lru_cache` is a shape before `to` rather than a keyword.
- Cache keys now match a function argument by identity rather than by the
  language's `is`; `is` itself is unchanged. SPEC.md § Memoisation states the
  distinction.
- `lru_cache` now decorates `to can` as well as plain `to` declarations. The
  bytecode format did not change: the method cache is attached by a hidden
  declaration call after the object is registered.
- `zip` now takes any number of lists (at least two) instead of exactly two.
  A two-list call keeps its old answer, and the shortest-input rule is
  unchanged; this is an additive arity extension, not a second spelling.

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
- An inherited `to can` method shares the parent method value's cache, because a
  cache belongs to the function value and the child inherits that value. A child
  that needs independent memory declares its own decorated method. → FINDINGS.md
- `zip` materialises every row, so a call with two very large lists is bounded
  by the loop and value sizes rather than by its own limit. `product` is the one
  helper with a materialised-answer limit because its length is a product;
  extending that same limit to `zip` would cap a list that a loop could already
  build. No follow-up is needed for this phase.
