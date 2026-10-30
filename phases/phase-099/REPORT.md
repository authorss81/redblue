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

The final resume fixed the one part of the deque claim the preserved work did not
actually meet. Every operation used to walk every node of both ends, so a deque
was O(n) per push or pop despite the tree structure. A deque is now an opaque
`Object` that Redblue source cannot construct; its trees are trusted after
construction, while a hand-written record with the same fields is still
accepted and still validated all the way down. That preserves every forged-tree
refusal and removes the per-operation walk from the module's own deques. The
result is measured, not asserted: the 35-test `collections_test` binary ran in
285 s on the previous tree and ran in 4.2 s before the cap-refusal tests were
added; the 38-test binary still finishes in seconds.

The review then closed the budget and representation gaps that resume left. A
deque now enforces `MAX_DEQUE_ITEMS` in the constructor and after every push,
so a list longer than the cap is refused before any tree is built and a push at
the cap cannot produce a deque no operation can use. An over-budget refusal
names the limit and the length that crossed it rather than calling the value a
non-deque, and the trusted object path now requires the same `kind: "deque"`
marker the record path does. `json.stringify` refuses a deque instead of
silently writing `null` for it, and `SPEC.md` names the `kind: "deque"` marker a
hand-written record needs.

| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +72 −28 (resume and review) | `collection_pairs`, the Counter/defaultdict/deque helpers, persistent stack trees for deque ends, named-record validation, the thirteen `collections.*` arms, a linear `stack_from_values` deque constructor, and `deque_rebalance`, which splits a non-empty side across both ends instead of reversing the whole remainder. The resume makes `deque_value` build an opaque `Object` and splits `deque_parts` into a trusted object path (cheap shape and budget checks) and an untrusted record path (full `validate_stack_tree` walk), so module deques no longer re-walk every node per operation while hand-written records are still refused node by node. The review enforces the item budget in the constructor and after each push, splits malformed-value errors from a dedicated budget error that names the limit and supplied length, requires the `kind: "deque"` marker on the trusted object path as well as the record path, corrects the `stack_cons` trust-boundary comment, and makes `json_stringify` refuse a deque rather than write `null` |
| `src/stdlib.rs` | +116 −1 | registers the `collections` module, its 13 members, builtin names, signatures and the module table |
| `tests/collections_test.rs` | +296 −19 (resume and review) | 38 tests, all running the program on both engines and comparing values as data as well as text; the resume adds the opaque-object contract, a malformed-grandchild refusal on the untrusted record path, and a 1,500-item both-ends order test that exercises the trusted path; the review adds the constructor-budget refusal, push-at-cap refusals for both a module deque and a forged record at the cap, the object-path `kind` marker check, and the `json.stringify` refusal |
| `SPEC.md` | +15 −6 (resume and review) | documents the missing-key behaviour of Counter and defaultdict, their lookup cost, the `put` spelling, deque ends, named fields and field errors; the resume changes the deque description from an opaque record to an opaque value and states the trusted-object rule; the review names the `kind: "deque"` marker, the item budget and the JSON refusal |
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

A deque is an opaque object holding `front` and `back`. The two ends are
persistent stacks whose spines hold complete binary trees rather than one
record per item, so a deque of n items nests O(log n) levels instead of n.
Pushing at an empty end moves the other side across once; empty pops answer
`nothing`. Because source cannot construct an `Object`, a value made by this
module is trusted and is not re-walked on every operation. A hand-written record
with `kind: "deque"`, `front` and `back` is still read, but every node is
validated before use, so a bare record, a missing end or a malformed tree is
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
would rebuild it on every other pop and add a second quadratic term. A forged
record still has every node of both ends validated before use, so it is refused
immediately; a deque made by the module is an `Object` source cannot build, so
its trees are trusted and an operation pays only the shape and budget checks.
The
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

The measured cost of `list.shift` today is therefore: `shift` itself is a
constant-time read and is *not* the quadratic spelling; `skip(xs, 1)` is, because
it copies the remaining `n-1` items on every call. The deque's value is not a
comment. On the preserved tree the whole `collections_test` binary took 285 s,
most of it this 8,000-operation comparison and the per-operation deep walks; on
this tree the 38 tests finish in seconds even with the million-item cap-refusal
setup, and the deque half is now dominated by the deliberately quadratic `skip`
side of the comparison rather than by the deque. A loaded runner cannot flip the structural sharing assertion, so the
evidence does not depend on that wall-clock number.

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
| `edge_a_forged_deque_record_with_a_malformed_grandchild_is_refused` | failure: the untrusted-record path still validates every node of both ends, so a malformed grandchild is refused before the first pop |
| `edge_a_valid_deque_tree_may_share_one_child_under_both_branches` | nesting_recursion: a valid DAG reusing one child under both branches pops every logical value rather than hanging or dropping one |
| `edge_a_forged_deque_tree_beyond_the_node_budget_is_refused_before_it_is_walked` | resource_limit: a forged tree claiming more than `MAX_DEQUE_ITEMS` is refused by the budget before the walk starts, and the refusal names the limit and claimed length |
| `edge_an_object_named_deque_still_needs_the_kind_marker` | representation boundary: an object named `deque` without `kind: "deque"`, or with the wrong kind, is refused instead of trusted |
| `edge_a_deque_is_an_opaque_value_rather_than_a_forgeable_record` | representation boundary: a module deque is an `Object` source cannot construct, and both ends still pop in order from it |
| `edge_a_deque_of_many_items_keeps_order_through_both_ends` | resource_limit and order: 1,500 pushes and pops through each end on the trusted path, with both empty pops answering `nothing` |
| `edge_a_deque_constructor_refuses_more_items_than_the_budget` | resource_limit: a list of 1,000,001 items is refused by the constructor instead of building a deque no operation can use, and the refusal names both the limit and the supplied length |
| `edge_a_deque_at_the_cap_refuses_another_push` | resource_limit: a module deque holding exactly `MAX_DEQUE_ITEMS` refuses both `push_left` and `push_right`, naming the limit and the length the push would produce |
| `edge_a_forged_deque_record_at_the_cap_refuses_another_push` | resource_limit: a forged record at the cap, built from structurally valid shared trees, is refused by both pushes after validation rather than only by the constructor |
| `edge_json_stringify_refuses_a_deque_rather_than_writing_null` | representation boundary: a deque, alone or nested in a record, is refused by `json.stringify` rather than silently written as `null` |

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
  `edge_a_forged_deque_tree_with_a_wrong_size_is_refused`,
  `edge_a_forged_deque_tree_with_a_self_consistent_root_but_malformed_children_is_refused`
  and `edge_a_forged_deque_record_with_a_malformed_grandchild_is_refused`.
- resource_limit — covered by the 4,000-item depth test, which measures both a
  pushed deque and one built by `collections.deque(list)`, by the
  8,000-operation shared-node test, by the 1,500-item both-ends test on the
  trusted path, by the constructor-budget refusal at 1,000,001 items, by the
  module deque at exactly the cap refusing both pushes, by the forged record at
  exactly the cap refusing both pushes, and by the forged tree beyond the node
  budget being refused before it is walked; the deque stores one tree node per
  item at most, and Counter/defaultdict append one entry per distinct key. The
  constructor and every push enforce the same `MAX_DEQUE_ITEMS` bound
  `deque_parts` checks, so a deque that exists is one every operation can use.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 2374 passed, 0 failed, 0 ignored |
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
- A deque made by `collections.deque` is an `Object`; Redblue source has no
  literal for one, so it cannot forge a trusted deque. A hand-written record
  with `kind: "deque"`, `front` and `back` is still accepted for compatibility,
  but every operation validates both stacks node by node, so a malformed tree
  anywhere is refused by the first operation. That validation is linear in the
  record's size; module deques skip it and pay only O(log n) spine work plus the
  amortised side reversal, which is why the 8,000-operation test dropped from
  minutes to seconds. The `size` fields still bound every walk, so no forged
  record can make one operation visit more than `MAX_DEQUE_ITEMS` logical items.
  The constructor and every push apply the same bound, so the module cannot
  build a deque that its own operations then refuse. A deque has no JSON form,
  so `json.stringify` refuses one rather than writing `null`, which would say
  it is `nothing`.
