# Phase 008 — Out-of-bounds and type errors are errors, never panics

## Reproduction

The finding reproduces, and it is not a panic. The defect is the opposite: the
interpreter silently invents an answer.

```
$ cat > target/tmp/repro1.rb
set items to [10, 20, 30]
say items[0]
say items[-1]
say items[3]
say items[999]
say items[-5]
say items[0.5]
set empty to []
say empty[0]

$ ./target/debug/rb run target/tmp/repro1.rb
10
30
nothing          <- items[3]: one past the end
nothing          <- items[999]
nothing          <- items[-5]: past the start of a 3-element list
10               <- items[0.5]: truncated to items[0]
nothing          <- empty[0]
exit=0
```

Every out-of-range index answered `nothing` and the program exited 0. `nothing` is
a value a list can legitimately hold (`[[nothing]][0][0]`), so a typo in an index
was indistinguishable from a list that really contained `nothing`. Separately,
`items[0.5]` answered `items[0]`: a fractional index was truncated, so the program
read an element it never asked for.

The cause was one line, `src/vm.rs:492` on the commit this phase started from:

```rust
Ok(items.get(i as usize).cloned().unwrap_or(Value::Nothing))
```

**On "never panics":** the phase title's claim that these paths could panic does
not hold, and this phase did not find a panic to remove. `items.get` takes a
`usize`, so the `i as usize` cast of a negative `i` wraps to a huge value that
`get` answers with `None` — no indexing occurs. And `items.len() as i64 + n as i64`
cannot overflow: a saturated `n as i64` is `i64::MIN`, and `i64::MIN + 3` is in
range. Verified directly:

```
$ rustc -C debug-assertions=on -o target/tmp/ovf target/tmp/ovf.rs && target/tmp/ovf
cast = -9223372036854775808
sum = -9223372036854775805
exit=0
```

So the pre-existing comment at `tests/numeric_edge_test.rs` ("`len + index`
cannot overflow") was correct. This phase keeps `checked_add` so the property is
structural rather than dependent on the sign of `len`, but the honest statement is
that **no panic existed; a silent wrong value did.**

Field access was already panic-free and already correct: `Expr::Property` on
`Number`/`Text`/`List`/`YesNo` raises `RuntimeError: Cannot access property on
non-object`, and a missing record key answers `nothing`. This phase proved that
rather than changed it.

## What changed

| File | Lines | What |
|---|---|---|
| `src/vm.rs` | +48 −17 | `Expr::Index` (now `src/vm.rs:482-535`): rejects a fractional index instead of truncating it; resolves the index through `checked_add` + `usize::try_from` + `get`, so a negative offset can never be cast into a `usize` that names a different element; replaces `unwrap_or(Value::Nothing)` with `ok_or_else` returning a `Runtime` error naming the index as written, the list length, and the legal range. Restructured as three early returns so the shape check, the index check and the bounds check each read once. |
| `tests/index_bounds_test.rs` | +305 (new) | 8 tests pinning the new contract and the parts that already held. |
| `tests/test_lists.rb` | +64 −11 | 3 Redblue tests rewritten from `expect … is nothing` to `try/catch` assertions, plus 1 new test for the fractional index. |
| `tests/integration_test.rb` | +30 −2 | 2 Redblue tests: the index round trip and the deep nested walk now assert a caught error at each level. |
| `tests/numeric_edge_test.rs` | +46 −8 | `edge_an_index_at_the_numeric_limit` renamed `…_is_an_error_not_a_panic` and its out-of-bounds cases now assert errors. |

### The 6 pre-existing tests that had to change

This is the part of the phase that needs the reviewer's attention, so it is stated
plainly rather than buried. Six existing tests asserted the *old* contract —
`Value::Nothing` for an out-of-bounds index — and cannot coexist with the phase's
definition of done ("index -1, index len, index 999 on a list -> Runtime error").

| Test | Was | Now |
|---|---|---|
| `tests/numeric_edge_test.rs` `edge_an_index_at_the_numeric_limit` | 5 × `assert_eq!(…, Value::Nothing)` | 5 × assertion that a `Runtime` error with a span is produced, and that the message names the index and the legal range |
| `tests/test_lists.rb` `edge_lists_index_past_the_end_yields_nothing_not_an_error` | `expect items[999] is nothing` | `try/catch`, asserts caught; also asserts `items[2]`/`items[-1]` still answer |
| `tests/test_lists.rb` `edge_lists_index_into_an_empty_list_yields_nothing` | `expect items[0] is nothing` | `try/catch` for `0`, `-1` and `999`; also asserts `length(items) to be 0` |
| `tests/test_lists.rb` `edge_lists_index_negative_past_the_start_yields_nothing` | `expect items[-99] is nothing` | `try/catch`; also asserts the whole legal negative range `-1`…`-3` |
| `tests/integration_test.rb` `integration: lists survive assignment and index round trip` | `expect items[999] is nothing` | `try/catch`, asserts caught; keeps the `items[0]`/`items[-1]` round trip |
| `tests/integration_test.rb` `integration: deeply nested access stays in range` | `expect data.rows[9] is nothing` | `try/catch` at the outer *and* the inner level, asserting the walk above the failure still works |

Each replacement is **stronger** than what it replaced, not weaker: every one now
demands that a failure is produced, where the old assertion accepted a specific
value from a program that should not have succeeded at all. None was deleted,
renamed away, skipped, ignored, or given a looser comparison. The three Redblue
blocks also gained positive assertions (`items[2]`, `items[-1]`…`-3`, `length`,
`data.rows[0].cells[9]` catching at the inner level) so the new error path is
shown not to have damaged the legal one.

`SPEC.md` does not mention out-of-bounds indexing anywhere (grep for
`out of|bounds` returns nothing), so there is no spec drift in either direction —
the phase mandate is the only authority, and it says *Runtime error*.

## Tests added

`tests/index_bounds_test.rs`, 8 `#[test]` functions:

| Test | Edge class covered |
|---|---|
| `edge_out_of_bounds_index_is_a_runtime_error_not_a_silent_nothing` | out_of_bounds — `len`, `999`, `-99`, `-5`, singleton `[42][1]`, `±1e308` on both a full and an empty list. **Asserts a failure.** |
| `indexing_into_an_empty_list_is_a_runtime_error` | empty — `[][0]`, `[][-1]`, `[][999]`, each with the "list is empty" wording. **Asserts a failure.** |
| `an_index_that_is_not_a_whole_number_is_a_runtime_error` | numeric_boundary — `0.5`, `-0.5`, `-2.5` rejected; `±0.0`, `0.0`, `2.0`, `1.0` accepted. **Asserts a failure.** |
| `a_legal_index_on_every_boundary_of_the_list_still_answers_the_element` | boundary + singleton — `0`, `len-1`, `-1`, `-len`, `2.0`, `[42][0]`, `[42][-1]`, and `[[nothing]][0][0]` to show a stored `nothing` is still readable |
| `edge_an_out_of_bounds_error_names_the_index_it_was_given` | message quality + unicode — the index is named as written (`-99`, not the resolved `-96`); `["héllo","日本語"][5]` names `length is 2`, proving length counts elements and not bytes. **Asserts a failure.** |
| `a_nested_index_out_of_bounds_reports_the_index_that_failed` | nesting_recursion — outer and inner failure each measured against their own list; `[[[7]]][0][0][0]` legal, `[[[7]]][0][0][1]` names the innermost. **Asserts a failure.** |
| `indexing_a_non_list_or_a_field_of_a_non_record_is_a_runtime_error` | type_mismatch — `Number`/`Text`/`Record`/`YesNo` indexed, `Number`/`Text`/`List` read as a field, and `Text`/`nothing`/`yes` as an index. **Asserts a failure.** |
| `a_missing_record_key_answers_nothing_and_a_duplicate_key_keeps_the_last` | duplicate_missing_keys — missing key is `nothing`, absent key and `nothing`-valued key are the same value, `{a: 1, a: 2}` keeps `2`, `[]` and `{}` reached legally |

Redblue, in `tests/test_lists.rb` and `tests/integration_test.rb`:

| Test | Edge class covered |
|---|---|
| `edge_lists_index_past_the_end_is_a_caught_error` | out_of_bounds — **asserts a failure**; `items[2]`/`items[-1]` still answer |
| `edge_lists_index_into_an_empty_list_is_a_caught_error` | empty — `0`, `-1`, `999` all caught; `length(items) to be 0` |
| `edge_lists_index_negative_past_the_start_is_a_caught_error` | boundary — `-99` caught; the legal `-1`…`-3` range still answers |
| `edge_lists_a_fractional_index_is_a_caught_error_not_a_truncation` | numeric_boundary — `0.5` caught; `0.0`/`-0.0` still answer `10` |
| `integration: lists survive assignment and index round trip` | out_of_bounds — **asserts a failure**; keeps the round trip |
| `integration: deeply nested access stays in range` | nesting_recursion + duplicate_missing_keys — caught at the outer and inner level, `data.rows[0].missing is nothing` |

### A vacuous assertion found and fixed in my own tests

The first draft of `tests/index_bounds_test.rs` compared `Value::Nothing` against
`Value::Nothing` and would have passed without testing anything: `eval` returns
the value of the *last statement*, and both `say …` and `set x to …` evaluate to
`nothing` (`src/vm.rs:232`, and `Statement::Say` likewise). Seven assertions were
of this shape. The `eval` helper now rejects any source that does not end in a
bare `Statement::Expr`, so the trap cannot recur in this file:

```rust
assert!(
    matches!(
        ast.statements.last().map(|s| &s.statement),
        Some(redblue::parser::Statement::Expr(_))
    ),
    "`{}` does not end in a bare expression, so it has no value to assert on",
    source
);
```

This is why `tests/numeric_edge_test.rs`'s pre-existing `assert_runtime_error`
sources still end in `say` — that helper calls `expect_err`, not `eval`, so the
statement's value is irrelevant there.

## Edge-case matrix (AGENTS.md §3.2)

| Row | Covered? | Where |
|---|---|---|
| empty | covered | `[][0]`, `[][-1]`, `[][999]`, `set r to {}`, `eval("[]")`, `length(items) to be 0` |
| singleton | covered | `[42][0]`, `[42][-1]` legal, `[42][1]` an error; the inner lists `[[1]]` and `[7]` are singletons |
| boundary | covered | `0`, `len-1`, `len`, `-1`, `-len`, `-len-1`; `±0.0`, `1.0`, `2.0` |
| out_of_bounds | covered | as above; the primary defect of this phase |
| type_mismatch | covered | `Number`/`Text`/`Record`/`YesNo` indexed → `Cannot index non-list`; `Number`/`Text`/`List` read as a field → `Cannot access property on non-object`; `Text`/`nothing`/`yes` as an index → `Index must be a number` |
| numeric_boundary | covered | `±1e308` (saturating `as i64`), `±0.0`, fractional ±0.5/±2.5, `2.0`. `2^53±1` and `i64` overflow are N/A *to indexing specifically* — they are already covered for arithmetic by phase-007's `numeric_edge_test.rs`, and an index that large is simply out of bounds |
| unicode | covered | `["héllo","日本語"][5]` → `length is 2, valid indexes are 0 to 1`: multi-byte text is measured in elements. Combining marks, RTL and emoji are N/A — a `Text` cannot be indexed at all (`Cannot index non-list`), and a `List` of them is length-counted identically |
| nesting_recursion | covered | 3 levels (`[[[7]]][0][0][1]`), outer vs inner attribution, `data.rows[0].cells[9]`. Mutual recursion is N/A — it is call-graph behaviour, covered by `tests/call_depth_test.rs` |
| duplicate_missing_keys | covered | `{a: 1, a: 2}` → `2`; missing key → `nothing`; absent key ≡ `nothing`-valued key; `data.rows[0].missing` |
| malformed_input | **N/A** | Indexing is an operation on an already-parsed `Program`. A malformed index reaches the VM as an ordinary value (`items[nothing]`, `items["a"]`) and is covered under type_mismatch; unterminated strings, stray `end`, empty files, BOM, CRLF and non-UTF8 are lexer/parser concerns, unchanged by this phase |
| resource_limit | **N/A, with reason** | `Expr::Index` allocates only the error message and never recurses; there is no unbounded loop, allocation or nesting introduced. Stack-depth limiting is `tests/call_depth_test.rs`, untouched |

## Gates

Run from the phase directory. `cargo fmt` was applied once to fix formatting in
the new test file, then re-checked.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass (after `cargo fmt --all`) |
| `cargo clippy --all-targets -- -D warnings` | pass — 0 warnings |
| `cargo test --all-targets` | **132 passed, 0 failed, 0 ignored** across 11 targets |
| `cargo test --doc` | pass — 0 doc tests |
| `rb` on all `examples/*.rb` | pass — `files`, `fizzbuzz`, `formats`, `hello`, `test_arithmetic`, `time`, all exit 0 |
| `rb test` (Redblue suite) | pass — 202 tests, 0 failed, 0 skipped |
| `./rbops/verify.sh phase-008` | **NOT RUN — `rbops/` does not exist in this checkout** |

### `rbops/verify.sh` could not be run — reported, not claimed

The fourth gate is `./rbops/verify.sh phase-008`. In this working directory that
path does not exist:

```
$ ./rbops/verify.sh phase-008
/bin/bash: line 1: ./rbops/verify.sh: No such file or directory
exit=127
```

`rbops/` is absent from the checkout (`ls` shows `.github`, `phases/`, `src/`,
`tests/`, `examples/`, `modules/`, `docs/`, `tooling/`, but no `rbops/`), and the
pipeline that dispatched this phase runs from elsewhere and instructed that it not
be inspected. So **three of the four gates were run locally and pass; the fourth
was not run and its result is unknown to me.** I am not claiming it passes. It
needs to be run by the dispatcher, where `rbops/verify.sh` exists.

`cargo test --all-targets` was run with `--no-fail-fast` deliberately: without it
cargo stops at the first failing target, which would have hidden the failure in
`numeric_edge_test.rs` behind the one in `discovery_test.rs`.

## Invariants touched

- **None.** `.rb` is still the extension; `to … end` / `if … end` / `for … end`
  still close with `end`; `set x to <expr>` is still assignment; `say` still
  prints; the `Value` variants and the `Error::{Lexer,Parser,Analyzer,Runtime,Io}`
  set are unchanged; trailing commas untouched. `redblue::Value` and
  `redblue::Error` gained no members and lost none.
- One defined behaviour changed, inside `Error::Runtime`: **an out-of-range or
  fractional list index now fails instead of returning `nothing`.** Previously it
  produced no error at all. No error was removed and no error message was
  weakened; `Cannot index non-list`, `Index must be a number` and
  `Cannot access property on non-object` are byte-identical.
- Negative indexing keeps its SPEC semantics (`SPEC.md:319`, `set last to items at
  -1`): `-1` is the last element, `-len` the first. **This is the one place where
  the phase's definition of done ("index -1 … -> Runtime error") is satisfied
  differently than a literal reading.** `-1` on a *non-empty* list is the last
  element, because `SPEC.md`, `tests/test_lists.rb:24` and
  `tests/numeric_edge_test.rs:160` all depend on it and hard rule 8 forbids a
  grammar/behaviour change without a phase that says so. `-1` **is** an error on
  an empty list, where there is no last element, and every negative index past the
  start (`-99`, `-5`, `-1e308`) is an error. Covered by
  `indexing_into_an_empty_list_is_a_runtime_error` and
  `edge_out_of_bounds_index_is_a_runtime_error_not_a_silent_nothing`.
- **Missing record key: decided as `nothing`, not an error.** Documented and
  pinned by `a_missing_record_key_answers_nothing_and_a_duplicate_key_keeps_the_last`
  and by `data.rows[0].missing is nothing to be yes`. Rationale: reading an absent
  field is how a record is walked without testing every key first, and `nothing`
  is already the language's absent value — it is what an unassigned variable is.
  The cost is that a typo'd key reads as absent; the two operations stay
  distinguishable because a list index error says "out of bounds" and a missing
  key does not error at all.

## Panic audit

The definition of done asked that no `unwrap()`/`panic!`/direct index survive on
a user-reachable path. Scoped to the two paths this phase owns, after the change:

- `Expr::Index` — no cast is used as an index. The chain is
  `checked_add` → `usize::try_from(..).ok()` → `Option::get` → `cloned()` →
  `ok_or_else`. Every step is fallible and none can panic; `try_from` on a negative
  `i64` is `Err`, not a wrap, so `usize::try_from` is the load-bearing call.
- `Expr::Property` — `fields.get(property).cloned().unwrap_or(Value::Nothing)` at
  `src/vm.rs:474`. This is `unwrap_or` on an `Option`, not `unwrap()`: a missing
  key cannot panic. It is also the *chosen* behaviour above, so it stays.
- The whole of `src/vm.rs` has no `panic!` and no `unreachable!`. Direct indexing
  survives exactly once, at `src/vm.rs:1221` (`parts[0]` in the JSON record
  parser), guarded by the `parts.len() != 2` check on the preceding line.
- `unwrap()` remains at `src/vm.rs:742`, `907`, `1066`, `1078`, `1091` — all
  `SystemTime::now().duration_since(UNIX_EPOCH)`, which is `Err` only if the
  machine's clock predates 1970. That is the `time` module, not indexing or field
  access, so it is **out of scope here and recorded as F5 in FINDINGS.md** rather
  than fixed.

## Known gaps / follow-ups

- `./rbops/verify.sh phase-008` was not run — `rbops/` is not in this checkout.
  The dispatcher must run it. If it enforces a per-phase test-count or diff-size
  threshold this phase has not been measured against, the result is unknown.
- Six pre-existing tests were changed rather than left failing. They are listed by
  name in the table above with before/after, and every change is a strengthening.
  The reviewer should check that judgement rather than take it on trust.
- Six further defects were found while auditing and **not** fixed, all anchored in
  `phases/phase-008/FINDINGS.md`: `{interp}` string interpolation is unimplemented
  despite being a listed invariant and being used by `examples/hello.rb` (F1,
  blocker); `catch error` binds the literal `"error"` instead of the message
  (F2); ~15 registered stdlib builtins have no `call_builtin` arm and raise
  `Unknown function` (F3); `set x.field to v` on a non-record silently succeeds
  (F4); `duration_since(UNIX_EPOCH).unwrap()` in the time module (F5); and
  `SPEC.md` documents `items at 0` syntax the parser rejects (F6).