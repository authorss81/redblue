# Phase 099 — Counter, defaultdict, deque and named records

## What changed

The finding reproduced on `main` before any edit: `collections` was not a
module, so `collections.counter([1, 1])` failed with `Unknown variable
'collections'`, and a missing record field answered `nothing` rather than a
documented default. The new names are builtins reached through the shared
`runtime::builtin` dispatcher, so the tree-walker and the bytecode VM answer
through one implementation. No `Value` variant, error variant or grammar rule
changed.

| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +560 −1 | `collection_pairs`, the Counter/defaultdict/deque helpers, persistent stack trees for deque ends, named-record validation, and the thirteen `collections.*` arms |
| `src/stdlib.rs` | +116 −1 | registers the `collections` module, its 13 members, builtin names, signatures and the module table |
| `tests/collections_test.rs` | +357 −0 | 18 new tests, all running the program on both engines and comparing values as data as well as text |
| `SPEC.md` | +54 −0 | documents the missing-key behaviour of Counter and defaultdict, the `put` spelling, deque ends, named fields and field errors |
| `README.md` | +27 −0 | the user-facing collections section |
| `SURVEY.md` | +50 −49 | builtin count 92 → 105, modules 12 → 13, members 63 → 76, and re-anchored citations |
| `tests/stdlib_module_docs_test.rs` | +69 −0 | one answerable row per new builtin |
| `tests/stdlib_module_names_test.rs` | +5 −0 | the `collections` module row |
| `tests/doc_blocks_runs.txt` | +8 −7 | classifies the new SPEC block and re-anchors the blocks after it |
| `tests/doc_blocks_fragments.txt` | +1 −1 | count 38/107 → 39/108 |
| `tests/doc_blocks_known_fail.txt` | +1 −1 | count 14/107 → 14/108 |
| `tests/unreviewed_work_ack_test.rs` | +2 −1 | acknowledges the `resume phase-099:` delivery subject |

Counter stores first-appearance order as `{counts: [[key, count], ...], missing:
0}`. `collections.count` answers the documented `0` for a key it never saw.
`collections.most_common` sorts by descending count and keeps first-appearance
order for equal counts. Grouping reuses the existing `group_value_hash`, so
counting is linear rather than a nested scan.

`defaultdict` is `{entries: [[key, value], ...], factory: value}`.
`collections.get` appends the factory value on the first read and returns
`{value, dict}`; `collections.put` is the explicit write. The spelling is `put`
because `set` is a reserved word, so `collections.set(...)` is not a legal
property name. The factory is a value, not a callable; that limit is stated in
`SPEC.md` and is a known follow-up below.

A deque is `{kind: "deque", front, back}`. The two ends are persistent stacks
whose spines hold complete binary trees rather than one record per item, so a
deque of n items nests O(log n) levels instead of n. Pushing at an empty end
moves the other side across once; empty pops answer `nothing`. The `kind` field
marks a record the module made, so a bare record or one missing an end is
refused rather than treated as an empty deque.

`collections.namedtuple(names, values)` builds an ordinary record, so fields are
read with `.name`. `collections.field(record, name)` is the explicit accessor
that refuses a missing field with a catchable runtime error naming the field,
instead of answering `nothing`.

### Deque traversal depth

The first implementation stored each end as a chain of `{value, next}` records,
so an n-item deque was n nested records and any recursive consumer could abort
on a large one. The stacks now merge equal-sized trees on push, which keeps both
the spine and every tree logarithmic. The replacement test asserts that a
4,000-item deque nests no more than 64 levels, which is a structural property
rather than a wall-clock measurement. `list.shift` still only reads the first
item; `skip(xs, 1)` is still the spelling that removes it.

## Tests added

| Test | Edge class covered |
|---|---|
| `counter_counts_mixed_value_types_and_reports_missing_as_zero` | mixed key types; literal missing value `0` |
| `most_common_orders_by_count_and_keeps_first_appearance_for_ties` | ordering and stable ties, asserted on a literal list |
| `edge_an_empty_counter_answers_empty_and_zero` | empty collection |
| `counter_counts_unicode_yes_no_nothing_and_records` | Unicode, `yes`, `nothing` and record keys |
| `defaultdict_get_creates_a_missing_entry_and_returns_the_factory_value` | first read creates, later reads reuse the value |
| `defaultdict_hands_each_missing_key_its_own_copy_of_the_factory_value` | per-key entry independence |
| `deque_pops_from_both_ends_in_order` | both ends, singleton and empty pops |
| `deque_interleaves_pushes_and_pops_at_both_ends` | interleaved push/pop at both ends |
| `edge_a_deque_reverses_its_storage_once_when_one_side_empties` | reversal path and order preservation |
| `namedtuple_makes_a_record_with_the_names_it_was_given` | named fields and values |
| `edge_an_empty_namedtuple_is_an_empty_record` | empty record |
| `edge_a_namedtuple_field_name_may_be_any_text` | Unicode field names |
| `edge_a_missing_named_field_is_refused_by_name_rather_than_answering_nothing` | failure: missing field named in a catchable error |
| `edge_a_malformed_counter_is_refused` | type mismatch: non-list and malformed counter |
| `edge_a_malformed_defaultdict_is_refused` | type mismatch: malformed defaultdict |
| `edge_a_malformed_deque_is_refused` | type mismatch: malformed deque and non-deque pop |
| `edge_namedtuple_refuses_mismatched_names_values_and_duplicates` | failure: length mismatch, duplicate name, non-text name |
| `edge_a_large_deque_keeps_its_traversal_depth_bounded` | nesting_recursion: 4,000-item deque nests a bounded number of levels |

Mandatory matrix:

- empty — covered by `edge_an_empty_counter_answers_empty_and_zero`, the empty
  deque pop and `edge_an_empty_namedtuple_is_an_empty_record`.
- singleton — covered by the one-element deque and named-record cases.
- boundary — covered by popping the last item at each deque end and by
  `most_common`'s equal-count ordering.
- out_of_bounds — N/A: these values expose no numeric index; an empty pop answers
  `nothing` instead of indexing, and a missing named field is refused by name.
- type_mismatch — covered by `edge_a_malformed_counter_is_refused`,
  `edge_a_malformed_defaultdict_is_refused`, `edge_a_malformed_deque_is_refused`
  and the named-record refusals.
- numeric_boundary — N/A: counts are non-negative integers and no arithmetic is
  exposed; there is no float, overflow or NaN input to a collection operation.
- unicode — covered by `counter_counts_unicode_yes_no_nothing_and_records` and
  `edge_a_namedtuple_field_name_may_be_any_text`.
- nesting_recursion — covered by Counter over list and record keys, and by
  `edge_a_large_deque_keeps_its_traversal_depth_bounded`, which measures the
  deque value's nesting depth instead of assuming it; no recursive user callback
  is invoked.
- duplicate_missing_keys — covered by Counter equality for repeated values,
  `most_common` ties, missing Counter reads, `defaultdict.get` creation, and
  duplicate named fields being refused.
- malformed_input — covered by the four malformed-input refusal tests.
- resource_limit — covered by the 4,000-item depth test; the deque stores one
  tree node per item at most, and Counter/defaultdict append one entry per
  distinct key. No new limit is needed because these builtins are one VM step
  and do not recurse into user code.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 2354 passed, 0 failed, 0 ignored |
| `./rbops/verify.sh phase-099` | pass |

## Invariants touched

- None. `Value`, `Error`, the grammar, `.rb`, `to … end`, `set … to`, `say`,
  trailing commas, interpolation, examples and modules are unchanged.
- `collections` is a module name; the new names are builtins reached by both
  engines through the existing shared dispatcher.

## Known gaps / follow-ups

- The `defaultdict` factory is a stored value, not a callable. Invoking a
  Redblue function from a native builtin would need a callback path back into
  the interpreter and the VM; that is a separate concern. `collections.put`
  remains the explicit write, and `SPEC.md` states the value-factory rule.
- `list.shift` is a constant-time read in this tree and does not remove the
  first item; the quadratic spelling is `skip(xs, 1)`.
- A deque's `kind` marker makes an ordinary record fail the module's own shape
  check, but a caller who writes the marker by hand can still construct the
  record shape. The operations validate the fields and tree nodes they read, so
  a forged record is refused when it is malformed rather than trusted.
