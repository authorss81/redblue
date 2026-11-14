# Phase 077 — Finish the list surface: `sort`, `reverse`, `zip`, `enumerate`, `filter`, `reduce`

## Finding reproduced

Before this phase the list surface stopped at `map`. The four first-order
builtins were absent from the registry, and the two higher-order ones were
registered but had no driver:

```
$ rb run target/tmp/phase077/sort.rb
RuntimeError: Unknown function 'sort' at 1:1

$ rb run target/tmp/phase077/reverse.rb
RuntimeError: Unknown function 'reverse' at 1:1

$ rb run target/tmp/phase077/filter.rb
RuntimeError: filter was called with arguments it cannot use at 1:1

$ rb run target/tmp/phase077/reduce.rb
RuntimeError: reduce was called with arguments it cannot use at 1:1
```

`zip` and `enumerate` already answered in `src/runtime.rs`; the phase's job was
to give them their tests and their written surface rather than to reimplement
them. `filter` and `reduce` were the two entries left in
`tests/stdlib_module_docs_test.rs::known_gaps()`, deferred there because they
need to call a Redblue function from inside a builtin.

The preserved attempt found the dotted surface still stopped halfway: `zip` and
`enumerate` answered as flat builtins but not as `list.` members:

```
$ rb run target/tmp/t1.rb
RuntimeError: Module 'list' has no function 'zip' at 1:1

$ rb run target/tmp/t2.rb
RuntimeError: Module 'list' has no function 'enumerate' at 1:1
```

The resumed run re-verified the finding and then repaired the three gate
failures the preserved commit left behind: the documentation-block keys were
anchored one line short after `SPEC.md` grew, `sort` answered a non-text field
selector with the generic arity message, and the reported test count was stale.

## What changed

| File | Lines | What |
|---|---|---|
| `src/stdlib.rs` | +84 −13 | `list` module row is `map,length,filter,reduce,sort,reverse,zip,enumerate`; `sort` and `reverse` join the flat globals and `BUILTIN_NAMES`; signatures for both; `list_function`, `reduce_parts` and `list_operation` give both engines one dispatch and one set of refusal messages for the higher-order pair |
| `src/runtime.rs` | +150 −0 | pure `sort` and `reverse`: `sort_key`, `sortable_kind`, `sort_order` and `sorted_list`; `sort` is stable, orders homogeneous values ascending, orders records by a named field with `sort(list, field)`, refuses a mixed-kind list by naming both kinds, and propagates a comparison failure instead of treating it as equal. Resumed run: a non-text field selector is refused by name (`sort with a field requires a field name, but was given a <kind>`) instead of falling through to the generic arity message |
| `src/interpreter.rs` | +76 −15 | tree-walker `filter_builtin` and `reduce_builtin`; `call_user_function` is the same call path `map` uses, so predicate/function closures, arity and errors behave identically; a mid-list failure aborts with no partial list |
| `src/bytecode/vm.rs` | +220 −79 | `PendingMap` grows into `PendingListCall` with `PendingListKind::{Map, Filter, Reduce}`; `start_map`/`start_filter`/`start_reduce` share `advance_list_call` and `record_list_answer`; `call_name` dispatches the higher-order names through `list_operation` |
| `tests/stdlib_modules_test.rs` | +459 | `mod list_surface`: 25 tests, 20 named `edge_*`, diffing both engines on values, dotted spellings and operation-specific refusal text. The resumed run adds `edge_sort_refuses_a_non_text_field_by_name`, `edge_sort_of_mixed_types_is_a_runtime_error` (direct `is_err()` on both engines), `edge_sort_by_field_on_the_dotted_spelling_answers_like_the_flat_one`, `edge_zip_with_three_lists_stops_at_the_shortest`, `edge_filter_of_an_empty_list_never_calls_the_predicate`, and `edge_list_builtins_do_not_mutate_their_input` |
| `tests/stdlib_module_docs_test.rs` | +24 −14 | `filter`, `reduce`, `sort` and `reverse` moved into the answerable rows; `known_gaps()` is now empty while keeping its place for the next gap |
| `tests/handout_claims_test.rs` | +17 −18 | `list.filter` is no longer wrong to write; the “member that does not exist” case uses `list.sum`, so `WRONGLY_DOCUMENTED` is down to `formats` |
| `tests/unreviewed_work_ack_test.rs` | +1 −0 | `resume phase-077:` joins the delivery-subject prefixes, so the preserved attempt's own delivery commit is acknowledged by subject rather than reported as unreviewed work |
| `tests/doc_blocks_{runs,fragments,known_fail}.txt` | +14 −14 | the fourteen block keys below the extended list section are re-anchored to their current lines (they had been written one line short, so `every_documentation_block_is_classified_exactly_once` failed). No block changed bucket |
| `SURVEY.md` | +1 −1 | the regex evidence citation points at `src/stdlib.rs:1780` (`fn regex_builtin`); the previous line number no longer carried the binding the entry makes |
| `SPEC.md` | +22 −12 | § list documents the six builtins, stability, field sort, mixed-kind refusal, the reduce start rule and empty-list refusal; its example block now calls all six plus `list.zip`/`list.enumerate`, and the receiver paragraph records that a variable holding a list answers `xs.map(f)` |
| `README.md` | +6 −0 | the § List example block shows `filter`, `reduce`, `sort`, `reverse`, `list.zip` and `list.enumerate` |

The two engines share the semantics rather than each inventing them: both use
`sort_order`/`sorted_list`, both use `list_operation`/`reduce_parts`, and both
refuse a mixed-kind list with the same sentence naming the two kinds. The
bytecode driver keeps one pending list call per frame, so a nested higher-order
call resumes the outer frame with its own partial results.

## Tests added

| Test | Edge class covered |
|---|---|
| `sort_answers_identically_on_both_engines` | happy path + differential: values diffed across walker and bytecode |
| `reverse_answers_identically_on_both_engines` | happy path + differential: values diffed across walker and bytecode |
| `filter_and_reduce_answer_identically_on_both_engines` | happy path + differential: both higher-order builtins diffed |
| `edge_sort_is_stable_for_records_sharing_a_key` | order stability: records with equal keys keep their input order |
| `edge_sort_refuses_a_mixed_type_list_by_name` | type mismatch: refusal says `sort cannot order a number and a text`, not a silent cross-kind order |
| `edge_sort_of_empty_and_singleton_lists` | empty/singleton: both are valid and unchanged |
| `edge_zip_stops_at_the_shorter_list_and_enumerate_starts_at_zero` | boundary: unequal lengths stop short; the first index is 0 |
| `edge_filter_calls_the_predicate_once_per_item_in_order` | order and count: side effect records each call, asserted exactly |
| `edge_reduce_calls_the_function_in_order_with_an_exact_count` | order and count: a pair-wise fold records each call, asserted exactly |
| `edge_reduce_of_an_empty_list_needs_an_initial_value` | empty: with a start value it answers that value; without one it refuses with `reduce requires a non-empty list when no start value is given` |
| `edge_filter_failure_mid_list_aborts_without_partial_results` | failure: an error inside the predicate surfaces; no partial list is returned |
| `edge_list_builtins_refuse_wrong_types_by_name` | type/arity mismatch: each refusal is pinned to its operation-specific text, including missing functions and extra arguments |
| `edge_nested_higher_order_calls_answer_identically_on_both_engines` | nesting: `filter` inside `map`, `reduce` inside `filter`, and `map` inside a `reduce` function |
| `edge_sort_refuses_a_missing_field_by_name` | duplicate/missing keys: a record with no such field is refused with `sort has no field 'missing'` on both engines |
| `edge_dotted_list_spellings_answer_like_the_flat_ones` | differential surface: `list.sort`, `list.reverse`, `list.filter` and `list.reduce` each answer what the flat spelling answers |
| `edge_dotted_zip_and_enumerate_answer_like_the_flat_ones` | differential surface: `list.zip` and `list.enumerate` answer what the flat spellings answer |
| `edge_dotted_zip_stops_at_the_shorter_and_enumerate_numbers_from_zero` | boundary: `list.zip` stops at the shorter input; `list.enumerate` numbers from zero |
| `edge_dotted_zip_and_enumerate_refuse_what_the_flat_ones_refuse` | failure: the dotted spellings carry the same refusal text as the flat ones |
| `edge_dotted_zip_and_enumerate_name_the_exact_reason_on_both_engines` | failure, directly asserted: both engines answer the exact `Runtime` message and a known span |
| `edge_sort_refuses_a_non_text_field_by_name` | type mismatch (resumed): a non-text field selector is refused by name on both engines instead of getting the generic arity message |
| `edge_sort_of_mixed_types_is_a_runtime_error` | failure, directly asserted (resumed): `run_source_value(...).is_err()` and `vm.run(...).is_err()` for a mixed-kind sort |
| `edge_sort_by_field_on_the_dotted_spelling_answers_like_the_flat_one` | differential surface (resumed): `list.sort(records, field)` answers exactly what the flat field sort answers |
| `edge_zip_with_three_lists_stops_at_the_shortest` | boundary (resumed): more than two inputs still stop at the shortest |
| `edge_filter_of_an_empty_list_never_calls_the_predicate` | empty + side-effect count (resumed): the predicate is called zero times |
| `edge_list_builtins_do_not_mutate_their_input` | value semantics (resumed): `sort`/`reverse` return new lists and leave the input unchanged |

### Mandatory edge-case matrix

| Row | Covered? |
|---|---|
| empty | covered: `edge_sort_of_empty_and_singleton_lists`, `edge_reduce_of_an_empty_list_needs_an_initial_value`, `edge_filter_of_an_empty_list_never_calls_the_predicate` |
| singleton | covered: `edge_sort_of_empty_and_singleton_lists`, `edge_reduce_of_a_singleton_without_a_start_value_never_calls_the_function`, single-item `zip` in `edge_zip_with_three_lists_stops_at_the_shortest` |
| boundary | covered: `edge_zip_stops_at_the_shorter_list_and_enumerate_starts_at_zero`, `edge_zip_with_three_lists_stops_at_the_shortest`; `reduce` with a start value against an empty list vs. without one |
| out_of_bounds | N/A: the builtins walk whole lists; `enumerate` starts at 0 and `reduce` refuses an empty list rather than addressing one |
| type_mismatch | covered: `edge_sort_refuses_a_mixed_type_list_by_name`, `edge_list_builtins_refuse_wrong_types_by_name`, `edge_sort_refuses_a_non_text_field_by_name` |
| numeric_boundary | N/A: no arithmetic limit is introduced; `sort` compares values with the existing ordering (and `-0.0` is exercised in `sort_answers_identically_on_both_engines`) |
| unicode | covered indirectly: existing ordering and text comparison are reused; `reverse` pins `é`, CJK and the empty text |
| nesting_recursion | covered: `edge_nested_higher_order_calls_answer_identically_on_both_engines` nests `filter` inside `map`, `reduce` inside `filter`, and a `map` inside a `reduce` function |
| malformed_input | covered: wrong arity and wrong types are refused by name in `edge_list_builtins_refuse_wrong_types_by_name` and `edge_sort_refuses_a_non_text_field_by_name` |
| duplicate_missing_keys | covered: `edge_sort_refuses_a_missing_field_by_name` pins the dedicated `sort has no field '<field>'` refusal on both engines |
| resource_limit | N/A: all six are bounded by the input list; no new file, allocation loop or recursion is added |

The `list_surface` module adds 25 `#[test]` functions, 20 named `edge_*`. Zero
new `#[ignore]`, `// skip` or `allow(clippy::` suppressions.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2990 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh "phase-077" | pass |

## Invariants touched

- No lexer, parser, grammar or `Error` variant changed. The six names were
  already legal identifiers; this phase gives them answers.
- `filter` and `reduce` keep the `map` calling convention in both engines: a
  Redblue function value is called through the existing user-function path, so
  captured scopes, arity checks and error propagation are the same.
- `sort` is total on homogeneous values and refuses mixed kinds; it never
  silently reorders across kinds. Field sorting is stable, matching value sort.
- `KNOWN_GAPS` is empty, not removed: the test that walks it still runs, and
  the next unimplemented builtin goes back into the same list with its program.

## Known gaps / follow-ups

- `sort` orders by one field name only; a multi-key sort is not invented here.
  A record missing the named field is refused by name with
  `sort has no field '<field>'`, pinned on both engines by
  `edge_sort_refuses_a_missing_field_by_name`.
- The higher-order driver is per-frame in the bytecode VM. A future builtin
  that needs to hold more than one pending list call per frame should extend
  `PendingListCall` rather than add a parallel map-only field.
