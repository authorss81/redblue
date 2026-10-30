# Phase 099 — Counter, defaultdict, deque and named records

## What changed

The finding reproduced on the phase base before any edit: `collections` was not
a module, so `collections.counter([1, 1])` failed with `Unknown variable
'collections'`, and a missing record field answered `nothing` rather than a
documented default. The new names are builtins reached through the shared
`runtime::builtin` dispatcher, so the tree-walker and the bytecode VM answer
through one implementation. No `Value` variant, error variant or grammar rule
changed.

This resume kept the preserved implementation and finished the evidence the
definition of done still lacked: a structural cost test for `list.shift` against
a deque work queue, a single-entry Counter test, a linear deque constructor, and
balanced deque rebalancing that keeps alternating pops from rebuilding the untouched side.

| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +698 −1 | `collection_pairs`, the Counter/defaultdict/deque helpers, persistent stack trees for deque ends, named-record validation, the thirteen `collections.*` arms, a linear `stack_from_values` deque constructor, and `deque_rebalance`, which splits a non-empty side across both ends instead of reversing the whole remainder; deque ends are validated recursively down every branch; the resume replaced a manual `div_ceil` spelling with the standard method so clippy stays warning-free |
| `src/stdlib.rs` | +116 −1 | registers the `collections` module, its 13 members, builtin names, signatures and the module table |
| `tests/collections_test.rs` | +753 −0 | 28 tests, all running the program on both engines and comparing values as data as well as text; includes the shared-node deque/list comparison, the linear-construction boundary sweep, a counter's own missing value, signed-zero key equality, list and record defaultdict keys, and forged deque trees whose size disagrees with their children or whose children are malformed |
| `SPEC.md` | +56 −0 | documents the missing-key behaviour of Counter and defaultdict, their lookup cost, the `put` spelling, deque ends, named fields and field errors |
| `README.md` | +28 −0 | the user-facing collections section |
| `SURVEY.md` | +50 −49 | builtin count 92 → 105, modules 12 → 13, members 63 → 76, and re-anchored citations |
| `tests/stdlib_module_docs_test.rs` | +69 −0 | one answerable row per new builtin |
| `tests/stdlib_module_names_test.rs` | +5 −0 | the `collections` module row |
| `tests/doc_blocks_runs.txt` | +8 −7 | classifies the new SPEC block and re-anchors the blocks after it |
| `tests/doc_blocks_fragments.txt` | +1 −1 | recounted to 39 of 110 blocks |
| `tests/doc_blocks_known_fail.txt` | +2 −2 | recounted to 12 of 110 blocks |
| `tests/unreviewed_work_ack_test.rs` | +2 −1 | acknowledges the `resume phase-099:` delivery subject |

Counter stores first-appearance order as `{counts: [[key, count], ...], missing:
0}`. `collections.count` answers the documented `0` for a key it never saw.
`collections.most_common` sorts by descending count and keeps first-appearance
order for equal counts. Grouping reuses the existing `group_value_hash`, so
grouping is linear rather than a nested scan; lookups scan the stored pairs, as
`SPEC.md` now states.

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

### Deque construction and traversal depth

The first implementation stored each end as a chain of `{value, next}` records,
so an n-item deque was n nested records and any recursive consumer could abort
on a large one. The stacks now merge equal-sized trees on push, which keeps both
the spine and every tree logarithmic. The replacement test asserts that a
4,000-item deque nests no more than 64 levels both when the deque is built by
repeated `push_right` and when it is built by `collections.deque(list)`, which is
a structural property rather than a wall-clock measurement.

This resume also builds `collections.deque(list)` in one pass with
`stack_from_values`: it takes the largest perfect tree that fits at each step,
so the constructor is linear instead of paying a spine copy per item. When one
end empties, `deque_rebalance` reads the other side's pop order once and splits
it so both ends retain roughly half the items. Alternating pops at opposite ends
then share the trees at the end they did not touch; moving the whole remainder
would rebuild it on every other pop and add a second quadratic term. Validation
walks every node of both ends on each operation so a forged tree is refused
immediately, which is a separate linear cost per operation. The
boundary sweep pops both ends of deques built from every length in
`{0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 127}` and
checks the literal order against `0..n` and its reverse.

### Deterministic cost of `list.shift`

`shift(list)` is a constant-time read: it answers the first item and leaves the
list unchanged. The removal is `skip(xs, 1)`, which builds a new list over the
remaining items once per call. The test
`edge_a_deque_beats_a_list_work_queue_at_the_front` pins this without a clock:
it counts the tree nodes a deque keeps when it pops the front and asserts every
remaining node is shared with the deque it came from, while `skip(xs, 1)`
allocates new list storage. A loaded or differently optimized runner cannot
flip that comparison, and the same test still runs 8,000 operations through both
engines and checks the answers as data.

## Tests added

| Test | Edge class covered |
|---|---|
| `counter_counts_mixed_value_types_and_reports_missing_as_zero` | mixed key types; literal missing value `0` |
| `most_common_orders_by_count_and_keeps_first_appearance_for_ties` | ordering and stable ties, asserted on a literal list |
| `edge_an_empty_counter_answers_empty_and_zero` | empty collection |
| `counter_counts_unicode_yes_no_nothing_and_records` | Unicode, `yes`, `nothing` and record keys |
| `defaultdict_get_creates_a_missing_entry_and_returns_the_factory_value` | first read creates, later reads reuse the value |
| `defaultdict_writes_one_missing_key_without_changing_the_others` | each missing key receives the factory value; writing one key leaves the other entry unchanged |
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
| `edge_a_large_deque_keeps_its_traversal_depth_bounded` | nesting_recursion: 4,000-item deque nests a bounded number of levels whether it is pushed one item at a time or built by `collections.deque(list)` |
| `edge_a_single_entry_counter_counts_it_once` | singleton: one item counts once, an unseen key answers `0` literally |
| `edge_a_counter_refuses_a_negative_forged_count` | failure: negative, fractional and non-numeric forged counts are refused by both `count` and `most_common` |
| `edge_a_deque_built_from_a_list_pops_both_ends_for_every_length` | empty, singleton, boundary and power-of-two-minus-one construction lengths |
| `edge_a_deque_beats_a_list_work_queue_at_the_front` | resource_limit and deterministic storage sharing: 8,000 deque operations against `shift` + `skip`, plus a node-identity assertion that a front pop shares the remainder while `skip` copies it |
| `edge_alternating_pops_at_both_ends_share_the_deque_trees` | resource_limit and deterministic storage sharing: 2,048 alternating `pop_left`/`pop_right` calls, each checked for the correct value, with a bound proving the tree nodes rebuilt across all pops stay linear rather than reversing the whole remainder every other pop |
| `edge_a_counter_answers_its_own_missing_value_for_an_unseen_key` | duplicate_missing_keys: a forged counter's `missing` value `42` is answered literally for an unseen key, and `most_common` does not invent an entry |
| `edge_counter_treats_zero_and_negative_zero_as_the_same_key` | numeric_boundary: `0`, `-0.0`, `0.0` and `0` share one entry with the literal count `4` and one `most_common` row |
| `edge_a_defaultdict_uses_list_and_record_keys_without_collapsing_them` | nesting_recursion and mixed key types: list and record keys create separate entries and an equal list key is found on a later read |
| `edge_a_forged_deque_tree_with_a_wrong_size_is_refused` | failure: a deque tree whose `size` disagrees with its children is refused with the documented catchable deque error |
| `edge_a_forged_deque_tree_with_a_self_consistent_root_but_malformed_children_is_refused` | failure: a tree whose root agrees with its children's claimed sizes but whose children are leaves claiming size 3 is refused by the first `pop_left`, not only by a later operation |

Mandatory matrix:

- empty — covered by `edge_an_empty_counter_answers_empty_and_zero`, the empty
  deque pop and `edge_an_empty_namedtuple_is_an_empty_record`.
- singleton — covered by `edge_a_single_entry_counter_counts_it_once`, the
  one-element deque and named-record cases.
- boundary — covered by popping the last item at each deque end,
  `most_common`'s equal-count ordering, and the `2^k - 1` lengths in the
  construction sweep.
- out_of_bounds — N/A: these values expose no numeric index or argument order;
  an empty pop answers `nothing` instead of indexing, and a missing named field
  is refused by name.
- type_mismatch — covered by `edge_a_malformed_counter_is_refused`,
  `edge_a_malformed_defaultdict_is_refused`, `edge_a_malformed_deque_is_refused`
  and the named-record refusals.
- numeric_boundary — covered by `edge_a_counter_refuses_a_negative_forged_count`
  and by `counter_number`'s finite, non-negative and whole-number checks.
  Counts are non-negative integers, so a fractional, negative, non-numeric,
  `NaN` or infinite forged count is refused rather than returned; no arithmetic
  is exposed through the public constructors. `edge_counter_treats_zero_and_negative_zero_as_the_same_key`
  pins that `0`, `-0.0` and `0.0` are one key, not three.
- unicode — covered by `counter_counts_unicode_yes_no_nothing_and_records` and
  `edge_a_namedtuple_field_name_may_be_any_text`.
- nesting_recursion — covered by Counter over list and record keys,
  `edge_a_defaultdict_uses_list_and_record_keys_without_collapsing_them`, and by
  `edge_a_large_deque_keeps_its_traversal_depth_bounded`, which measures the
  deque value's nesting depth instead of assuming it; no recursive user callback
  is invoked.
- duplicate_missing_keys — covered by Counter equality for repeated values,
  `most_common` ties, missing Counter reads,
  `edge_a_counter_answers_its_own_missing_value_for_an_unseen_key`,
  `defaultdict.get` creation, and duplicate named fields being refused.
- malformed_input — covered by the four malformed-input refusal tests, the
  forged-count refusal, and
  `edge_a_forged_deque_tree_with_a_wrong_size_is_refused` and
  `edge_a_forged_deque_tree_with_a_self_consistent_root_but_malformed_children_is_refused`.
- resource_limit — covered by the 4,000-item depth test, which measures both a
  pushed deque and one built by `collections.deque(list)`, and by the
  8,000-operation shared-node test; the deque stores one tree node per item at
  most, and
  Counter/defaultdict append one entry per distinct key. No new limit is needed
  because these builtins are one VM step and do not recurse into user code.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 2364 passed, 0 failed, 0 ignored |
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
- `list.shift` is a constant-time read and does not remove the first item; the
  quadratic spelling is `skip(xs, 1)`. The test pins the storage-sharing
  relationship, not a wall-clock time.
- A deque's `kind` marker makes an ordinary record fail the module's own shape
  check, but a caller who writes the marker by hand can still construct the
  record shape. Every operation validates every node of both ends recursively
  before returning a deque, so a forged record with a malformed node anywhere is
  refused by the first operation rather than trusted. That validation makes an
  operation linear in the deque's size; the structural sharing still keeps a
  pop from rebuilding the untouched side, which the resource tests pin.
