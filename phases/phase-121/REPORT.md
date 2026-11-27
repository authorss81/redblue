# Phase 121 — Give `filter` and `reduce` an answer, or refuse them by name

## Finding re-verification

The audit's exact reproduction is stale on `main`: the flat `filter(xs, f)` and
`reduce(xs, f[, start])` spellings already answer on both engines because
`list_operation` is consulted before the generic builtin fallback.

```bash
$ ./target/debug/rb run target/tmp_filter.rb        # say filter([1,2,3], to (x) give back x > 1)
[2, 3]
$ ./target/debug/rb compile target/tmp_filter.rb -o target/tmp_filter.rbc
$ ./target/debug/rb vm target/tmp_filter.rbc
[2, 3]
```

The phase's definition of done also requires `reduce(xs, init, f)`. That order
did still fail, because `reduce_parts` accepted only `reduce(xs, f)` and
`reduce(xs, f, start)`. The previous attempt fixed that order. See
`FINDINGS.md`.

## What changed

| File | Lines | What |
|---|---|---|
| `src/stdlib.rs` | +5 −5 | `reduce_parts` accepts `[start, Function]` as the same call as `[Function, start]`; two functions after the list keep the documented reducer-first order (`[Function, start]`), so a function seed still works; the doc comment names both orders |
| `src/stdlib.rs` | +6 −0 | `stdlib::builtin` returns `Ok(None)` for a higher-order list name (`map`, `list_map`, `filter`, `reduce`) that `builtin_function` does not answer, so the shared resolver never blames the caller's arguments for a name the engines answer. The guard is on the production path: both engines call this resolver first and dispatch the higher-order driver only after it returns `Ok(None)`. The doc comment says why |
| `src/interpreter.rs` | +10 −8 | the higher-order driver runs after `stdlib::builtin` returns `Ok(None)`, so the resolver is the single dispatch point; `reduce_builtin`'s doc names the two equivalent orders and the reducer-first reading of a function seed |
| `src/bytecode/vm.rs` | +14 −12 | same ordering: the VM asks `stdlib::builtin` first and starts the higher-order driver after it returns `Ok(None)`; `start_reduce`'s doc names the two equivalent orders and the reducer-first reading of a function seed |
| `src/bytecode/format.rs` | +2 −2 | the pinned `NATIVE.md` citations for `builtins` and `call_function` move to the lines they now occupy (`call_function` is now `vm.rs:3796`) |
| `NATIVE.md` | +5 −5 | re-anchors every `src/stdlib.rs:520` `builtins` citation to `:527` and the `src/bytecode/vm.rs` `call_function` citation to `:3796` |
| `SURVEY.md` | +1 −1 | re-anchors the `regex_builtin` citation from `:1870` to `:1867` after the `reduce_parts` and resolver guard edits shifted the line |
| `SPEC.md` | +2 −2 | § list says the flat `filter(xs, f)` and `reduce(xs, f[, start])` answer like their dotted spellings, that `reduce(xs, start, f)` is the same call with the start value first, and that a function start keeps the reducer-first order |
| `src/stdlib.rs` | +117 −0 | a `#[cfg(test)]` module pins the source rule directly: both `reduce_parts` orders, the reducer-first reading of two functions, the named refusal of a call with no function, both spellings in `list_operation`, and the resolver guard |
| `tests/stdlib_modules_test.rs` | +163 −0 | the previous attempt's eight `mod list_surface` tests, kept |
| `tests/stdlib_modules_test.rs` | +61 −0 | four new tests this run: the resolver guard with real arguments, the function-seed reducer-first regression, the function-seed reducer-failure propagation, and the singleton `filter` boundary |
| `tests/stdlib_modules_test.rs` | +52 −0 | four more integration tests this run: a text start, a record start, a singleton that calls the reducer exactly once, and a non-list receiver refusal |
| `tests/stdlib_modules_test.rs` | +38 −0 | a review-round engine test that runs `map`, `filter`, `reduce`, `list_map`, `list.filter` and `list.reduce` through both engines, so removing the resolver guard fails at the engine boundary, not only in the resolver's unit test |

`filter` and `reduce` themselves are unchanged: phase-077 already gave them a
driver on both engines. This phase adds the start-before-function form of
`reduce`, the resolver guard that stops a registered higher-order name being
refused as bad arguments, and the tests that hold the whole surface. The
resolver is the single dispatch point: both engines ask it first, it answers
`Ok(None)` for the higher-order names, and the driver runs after, so the guard
is on the reachable path rather than beside it.

### Resumed work and the merge conflict

This run resumed a parked attempt whose work was merged onto a newer `main`.
`tests/unreviewed_work_ack_test.rs` arrived with conflict markers around the
phase-120 resume entry. Both sides named the same SHA (`e558b56`); the entry
kept is the accurate one for that commit (`+788/-27`, 6 files, `+545/-27` under
`src/`), so the acknowledgement list and `main`'s history agree. There is no
net change to that file against `HEAD`, which is why it is absent from the
table above. The four gates were then re-run from scratch on the resolved tree.

### Second continuation

The phase's substantive change had already been committed as `5ba9fdb`, so
`verify.sh` (which reads the diff against `HEAD`) saw no working-tree changes
and failed the diff, `must_touch` and test-policy checks even though the code
was correct. This run added working-tree coverage on top of that commit: the
six `#[cfg(test)]` unit tests in `src/stdlib.rs` pin the source rule directly,
and the four integration tests in `tests/stdlib_modules_test.rs` cover a text
start, a record start, the singleton call count and a non-list receiver. No
production behaviour was changed by this continuation; the implementation it
exercises is the one in `5ba9fdb`.

### Review round 1

Two findings were fixed without weakening a gate or deleting a test.

- The resolver guard is now reachable. Both engines previously intercepted
  `list_operation` before calling `stdlib::builtin`, so the `Ok(None)` guard
  could never run in production. The interception now happens after the
  resolver returns `Ok(None)`, making `stdlib::builtin` the single dispatch
  point. A new engine-level test
  (`edge_higher_order_names_answer_on_both_engines_without_an_argument_refusal`)
  runs every higher-order spelling through both engines; removing the guard
  turns those calls into `arguments it cannot use` failures.
- The two weak `is_err()` assertions now pin the named refusals:
  `edge_reduce_parts_refuses_a_call_with_no_function` asserts the exact
  `reduce requires a function and an optional start value, but was given 2
  arguments` text, and
  `edge_builtin_resolver_still_refuses_a_real_builtin_with_bad_arguments`
  asserts `uppercase was called with arguments it cannot use`.

## Names written or removed

No builtin name was added and none was removed. The two names the finding cites
are the whole surface:

| Name | Status after this phase |
|---|---|
| `filter` | already answered on both engines; flat and `list.filter` covered by tests |
| `reduce` | answered before; now also accepts `reduce(xs, start, f)` as well as `reduce(xs, f, start)` |

The template asks for nine names; the verified finding is "two names, not a
class" and the diff matches those two. `FINDINGS.md` records the discrepancy.

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_higher_order_names_with_arguments_are_left_to_the_engines` | type/ownership boundary — `map`, `list_map`, `filter`, `reduce` with a real list argument are left to the engines, and stay registered |
| `edge_reduce_with_a_function_start_keeps_the_documented_reducer_first_order` | function seed / regression — two functions after the list read the first as the reducer and the second as the start value, so the pre-existing `reduce(xs, f, start)` keeps answering |
| `edge_reduce_with_a_function_start_propagates_a_reducer_failure` | asserts a failure is produced (`is_err()`) — a failing reducer still fails the whole call when the start value is a function |
| `edge_filter_of_a_singleton_list_keeps_or_drops_the_only_item` | singleton / boundary — a one-item list kept or dropped by the predicate |
| `reduce_accepts_the_start_value_before_the_function` | happy path + both engines + boundary (singleton) + the documented opposite order |
| `edge_reduce_start_before_function_of_an_empty_list_answers_the_start` | empty |
| `edge_filter_keeps_nothing_when_the_predicate_is_true_for_no_item` | predicate true for nothing |
| `edge_filter_predicate_sees_nothing_and_text_items` | type boundary — `nothing` and text items reach the predicate |
| `edge_filter_propagates_a_failure_from_the_predicate` | asserts a failure is produced — a failing predicate fails the whole call |
| `edge_reduce_start_before_function_refuses_a_missing_function` | type mismatch / malformed input |
| `edge_uppercase_still_blames_arguments_it_cannot_use` | the argument-refusal message is preserved for a real builtin given a genuinely wrong argument |
| `edge_completion_offers_exactly_the_registered_builtins_that_answer` | TAB completion is reconciled with `BUILTIN_NAMES` and `GLOBAL_CONSTANTS`, not with a copied list |
| `edge_builtin_resolver_leaves_the_higher_order_list_names_to_the_engines` | type/ownership boundary — `map`, `list_map`, `filter`, `reduce` are not refused by the shared resolver |
| `edge_builtin_resolver_still_blames_a_real_builtin_with_bad_arguments` | asserts a failure is produced — `sqrt` of text keeps the argument refusal |
| `edge_filter_and_reduce_definition_of_done_program_agrees_on_both_engines` | boundary — the phase's exact `filter`/`reduce` program on both engines |
| `edge_reduce_start_before_function_accepts_a_text_start` | type boundary — a text start value folds on both engines |
| `edge_reduce_start_before_function_accepts_a_record_start` | nesting/record — a record start value folds into its field on both engines |
| `edge_reduce_start_before_function_with_a_singleton_calls_the_reducer_once` | singleton — a one-item list with a start calls the reducer exactly once |
| `edge_reduce_start_before_function_refuses_a_non_list_receiver` | asserts a failure is produced — a non-list receiver is refused by name |
| `edge_reduce_parts_accepts_the_start_value_before_the_function` | boundary — `reduce_parts` returns the function as reducer and the value before it as start |
| `edge_reduce_parts_keeps_the_documented_order_when_both_are_functions` | function seed / regression — two functions keep the reducer-first order |
| `edge_reduce_parts_refuses_a_call_with_no_function` | asserts the exact named refusal — `reduce requires a function and an optional start value, but was given 2 arguments` |
| `edge_list_operation_names_both_spellings` | boundary — flat and module spellings reach the same driver; a plain builtin does not |
| `edge_builtin_resolver_leaves_higher_order_names_to_the_engines` | type/ownership boundary — all six higher-order spellings return `Ok(None)` |
| `edge_builtin_resolver_still_refuses_a_real_builtin_with_bad_arguments` | asserts the exact named refusal — `uppercase(5)` answers `uppercase was called with arguments it cannot use` |
| `edge_higher_order_names_answer_on_both_engines_without_an_argument_refusal` | engine boundary — all six higher-order spellings answer on both engines; fails if the resolver guard is removed |

Existing coverage also runs every registered builtin through a real program:
`edge_no_registered_builtin_is_unreachable_from_a_program` in
`tests/stdlib_module_docs_test.rs` walks `stdlib::builtins()` in both directions,
and its `answerable_builtins` rows include `filter` and `reduce` with their
documented calls. `filter_and_reduce_answer_identically_on_both_engines` already
asserted byte-for-byte agreement between the two engines.

### Mandatory edge-case matrix

| Row | Status |
|---|---|
| empty | covered — `reduce([], 10, f)` answers 10; `filter` false-for-all keeps `[]` |
| singleton | covered — `filter([7], …)` keeps or drops the one item; `reduce([5], 10, f)` folds once to 15 |
| boundary | covered — first and last accepted argument orders both answer 6 |
| out_of_bounds | N/A — this change adds no indexing or positional lookup |
| type_mismatch | covered — `reduce([1,2], 0, 5)` and `uppercase(5)` are named refusals; a function start value is read in the documented reducer-first order rather than refused |
| numeric_boundary | N/A — no arithmetic, comparison, or float formatting changed |
| unicode | N/A — text is passed through untouched; no encoding or splitting changed |
| nesting_recursion | covered by pre-existing `edge_nested_higher_order_calls_answer_identically_on_both_engines` |
| duplicate_missing_keys | covered — a record start value is now folded in `edge_reduce_start_before_function_accepts_a_record_start`; the change itself adds no key lookup |
| malformed_input | covered — a `reduce` with no function after the start value is refused |
| resource_limit | N/A — no new allocation, recursion, or unbounded loop |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test` | 3307 passed, 0 failed |
| `cargo test --all-targets` | 3305 passed, 0 failed |
| `../rbops/verify.sh phase-121` | pass (`VERIFY PASS — phase-121`) |

## Invariants touched

- None. `reduce(xs, f, start)` keeps its documented answer for every start value,
  including a function seed; `reduce(xs, start, f)` is an additive equivalent
  whenever the start value is not itself a function. With two functions after
  the list the documented reducer-first order wins. `.rb`, `to … end`, `set`,
  `say`, `Value`, `Error` and the test grammar are untouched.

## Known gaps / follow-ups

- The audit's original claim (flat `filter`/`reduce` answer "arguments it cannot
  use") does not reproduce; it was already fixed in phase-077. Recorded in
  `FINDINGS.md` so the ledger can close the stale entry rather than re-fixing it.
- The phase template's "nine functions" does not match the verified two-name
  finding; see `FINDINGS.md`.
