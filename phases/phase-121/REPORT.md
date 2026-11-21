# Phase 121 — Give `filter` and `reduce` an answer, or refuse them by name

## Finding re-verification

The audit's exact reproduction is stale on `main`. Both engines already answer
the flat spelling because `list_operation` is consulted before the generic
builtin fallback:

```bash
$ ./target/debug/rb run target/tmp_filter.rb        # say filter([1,2,3], to (x) give back x > 1)
[2, 3]
$ ./target/debug/rb compile target/tmp_filter.rb -o target/tmp_filter.rbc
$ ./target/debug/rb vm target/tmp_filter.rbc
[2, 3]
```

The phase's definition of done also requires `reduce(xs, init, f)`. That order
did still fail, because `reduce_parts` accepted only `reduce(xs, f)` and
`reduce(xs, f, start)`. That is the gap this phase fixes. See `FINDINGS.md`.

## What changed

| File | Lines | What |
|---|---|---|
| `src/stdlib.rs` | +5 −5 | `reduce_parts` accepts `[start, Function]` as the same call as `[Function, start]`; the doc comment and the REPL signature name both orders. The net line count is zero, so `NATIVE.md`'s pinned `builtins` line stays at 520 |
| `src/interpreter.rs` | +2 −1 | `reduce_builtin`'s doc names the two equivalent orders |
| `src/bytecode/vm.rs` | +4 −4 | `start_reduce`'s doc names the two equivalent orders, re-wrapped to keep `call_function` at line 3792 for `NATIVE.md` |
| `SPEC.md` | +2 −2 | § list writes that the flat `filter(xs, f)` and `reduce(xs, f[, start])` answer like their dotted spellings, and that `reduce(xs, start, f)` is the same call with the start value first |
| `tests/stdlib_modules_test.rs` | +123 −0 | eight tests in `mod list_surface`, covering both `reduce` orders on both engines, empty/singleton lists, a predicate true for nothing, nothing/text items, a failing predicate, a missing function, the preserved wrong-argument message, and TAB completion against `BUILTIN_NAMES` |
| `tests/unreviewed_work_ack_test.rs` | +1 −0 | pins the phase-120 resume commit by SHA now that phase-120 is no longer the phase under review |

The `filter`/`reduce` flat spellings themselves are unchanged: phase-077 already
gave them a driver on both engines. This phase adds the start-before-function
form of `reduce` and the tests that hold the whole surface.

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
| `reduce_accepts_the_start_value_before_the_function` | happy path + both engines + boundary (singleton) + the documented opposite order |
| `edge_reduce_start_before_function_of_an_empty_list_answers_the_start` | empty |
| `edge_filter_keeps_nothing_when_the_predicate_is_true_for_no_item` | predicate true for nothing |
| `edge_filter_predicate_sees_nothing_and_text_items` | type boundary — `nothing` and text items reach the predicate |
| `edge_filter_propagates_a_failure_from_the_predicate` | asserts a failure is produced (`is_err()` plus the named message) |
| `edge_reduce_start_before_function_refuses_a_missing_function` | type mismatch / malformed input |
| `edge_uppercase_still_blames_arguments_it_cannot_use` | the `src/stdlib.rs:1246` message is preserved for a real builtin given a genuinely wrong argument |
| `edge_completion_offers_exactly_the_registered_builtins_that_answer` | TAB completion is reconciled with `BUILTIN_NAMES` and `GLOBAL_CONSTANTS`, not with a copied list |

Existing coverage also runs every registered builtin through a real program:
`edge_no_registered_builtin_is_unreachable_from_a_program` in
`tests/stdlib_module_docs_test.rs` walks `stdlib::builtins()` in both directions,
and its `answerable_builtins` rows include `filter` and `reduce` with their
documented calls. `filter_and_reduce_answer_identically_on_both_engines` already
asserted byte-for-byte agreement between the two engines; the new tests assert
the same for the init-first spelling.

### Mandatory edge-case matrix

| Row | Status |
|---|---|
| empty | covered — `reduce([], 10, f)` answers 10; `filter` false-for-all keeps `[]` |
| singleton | covered — `reduce([5], 10, f)` folds once to 15 |
| boundary | covered — first and last accepted argument orders both answer 6 |
| out_of_bounds | N/A — this change adds no indexing or positional lookup |
| type_mismatch | covered — `reduce([1,2], 0, 5)` and `uppercase(5)` are named refusals |
| numeric_boundary | N/A — no arithmetic, comparison, or float formatting changed |
| unicode | N/A — text is passed through untouched; no encoding or splitting changed |
| nesting_recursion | covered by pre-existing `edge_nested_higher_order_calls_answer_identically_on_both_engines` |
| duplicate_missing_keys | N/A — no records or maps involved |
| malformed_input | covered — a `reduce` with no function after the start value is refused |
| resource_limit | N/A — no new allocation, recursion, or unbounded loop |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 3287 passed, 0 failed |
| `../rbops/verify.sh phase-121` | pass |

## Invariants touched

- None. `reduce(xs, f, start)` keeps its documented answer; `reduce(xs, start, f)`
  is an additive equivalent. `.rb`, `to … end`, `set`, `say`, `Value`, `Error`
  and the test grammar are untouched.

## Known gaps / follow-ups

- The audit's original claim (flat `filter`/`reduce` answer "arguments it cannot
  use") does not reproduce; it was already fixed in phase-077. Recorded in
  `FINDINGS.md` so the ledger can close the stale entry rather than re-fixing it.
- The phase template's "nine functions" does not match the verified two-name
  finding; see `FINDINGS.md`.
