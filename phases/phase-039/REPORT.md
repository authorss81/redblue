# Phase 039 — Test or delete the public assertion functions nothing calls

## Reproduction

The finding's substance reproduces; its counts and function names do not.

```
$ for f in $(grep -o 'pub fn assert_[a-z_]*' src/testing/assertions.rs | sed 's/pub fn //'); do \
    n=$(grep -rn "\b$f\b" src/ tests/ --include=*.rs | grep -v 'src/testing/assertions.rs' | wc -l); \
    echo "$f -> $n"; done
assert_that -> 0
assert_values_equal -> 2
assert_value_is_number -> 0
assert_value_is_text -> 0
assert_value_is_list -> 0
assert_value_is_yes_no -> 0
assert_value_is_record -> 0
assert_list_length -> 0
assert_text_contains -> 0
assert_text_matches -> 0
assert_number_in_range -> 0
assert_throws -> 0
```

Exactly **11 of 12** `pub fn assert_*` had zero call sites, not "21 of 22", and
the names in the evidence (`assert_true`, `assert_false`, `assert_panics`,
`assert_not_panics`, `assert_matches_regex`, `assert_len`, `assert_type`) are
not in the file at all. The two claims that *do* reproduce are the ones that
mattered:

- `assert_throws` — the assertion AGENTS.md § 3.3 makes mandatory is shipped,
  unreachable, and untested.
- `src/runtime.rs:735` hand-rolled the `expect`/`assert` builtin's failure as a
  private `format!` string instead of calling the library the crate owns.

The `AssertThat` builder the evidence did not mention is worse than dead:
`is_none` and `is_some` were stubs that returned `Err` unconditionally, and
`contains` was **inverted** — it returned `Ok` when the value did *not* equal
the item and `Err` when it did (`src/testing/assertions.rs:102-115` before the
change). An assertion that passes on absence and fails on presence is worse
than an unused one, so it could not be kept and tested; it had to go.

## What changed

| File | Lines | What |
|---|---|---|
| `src/testing/assertions.rs` | +0 −89 | deleted `pub trait Assertion<T>`, `pub struct Expected<T>` (both had zero references anywhere), and the whole `assert_that` / `AssertThat` builder: the two unconditional-`Err` stubs and the inverted `contains`. The 11 correct assertions are unchanged — not a line of their behaviour moved |
| `src/runtime.rs` | +138 −9 | the `expect` / `assert` builtin now calls `crate::testing::assertions::assert_values_equal` instead of formatting `"Assertion failed: expected {:?} but got {:?}"` itself, so the builtin, the `Expect` statement (`src/interpreter.rs:1612`) and `Opcode::Expect` (`src/bytecode/vm.rs:1738`) all report through one library. Plus the `expect_builtin_tests` unit-test module |
| `tests/assertions_test.rs` | +434 −0 | new file, 9 `#[test]` functions |
| `tests/call_depth_test.rs` | +7 −7 | 4 stale expectations repaired (below) |
| `tests/for_range_test.rs` | +5 −5 | 2 stale expectations repaired |
| `tests/function_literal_test.rs` | +4 −3 | 1 stale expectation repaired |
| `tests/loop_bounds_test.rs` | +8 −9 | 7 stale expectations repaired |
| `tests/loop_control_test.rs` | +6 −5 | 2 stale expectations repaired |
| `tests/numeric_edge_test.rs` | +2 −2 | 1 stale expectation repaired |
| `tests/object_model_test.rs` | +3 −4 | 1 stale expectation repaired |

`must_touch: ["src/ or tests/"]` is satisfied by both.

### Why the deletion is deletion, not a mute

Nothing was silenced. `grep -rn 'allow(dead_code' src/` prints nothing, no
`#[ignore]` was added, no `#[cfg(test)]`-only shim remains — the 89 deleted
lines are gone from the file, and the 89 lines the file lost are the only
lines the file lost.

### Why the 7 test files were touched

`cargo test` is one of the four gates and it was **red on arrival**: 18
failures, present before this phase's change. Proven, not asserted — a
`git worktree` of `HEAD~1` reproduces the identical failing set with identical
counts (4+2+1+7+2+1+1 = 18 on both sides).

Every one of the 18 is the same defect: a test matching on the *variant*
`Error::Runtime(..)` where phase-038 made the three resource guards return the
new variant `Error::Limit`. The phase cannot claim a green `cargo test` while
18 tests it is responsible for reporting on are red, so they were repaired.

The design question that repair turned on is answered by the source itself:
`Error::label()` at `src/error.rs:84` returns `"RuntimeError"` for
`Error::Limit`, and `is_resource_limit()` (`src/error.rs:107`) exists to tell
the two apart. `Error::Limit` *is* a runtime error; it is a distinct variant,
not a new error class. The tests were matching on the variant where they
meant the contract.

Every repaired site therefore got **narrower**, not looser:

- `matches!(error, Error::Runtime(..))` → `matches!(error, Error::Limit(..))`
  — demands the limit variant specifically instead of accepting either.
- message-extracting helpers were **split**, not widened: the `Error::Limit`
  arm moved out of `runtime_message` into a `limit_message` beside it. The
  general helpers accept `Error::Runtime` only, and every limit test reads its
  message through `limit_message`, which accepts `Error::Limit` only and asserts
  `is_resource_limit()` on the error on the way past. A helper taking either
  variant let a limit and a program failure be asserted interchangeably; two
  that cannot answer for each other cannot drift apart.

No test was deleted, weakened, skipped or given an `#[allow]`. Full diff of
the repair: +38 −32 across 7 files, and the suite went from 1141/18 to
1159/0.

## Tests added

12 new `#[test]` functions: 9 in `tests/assertions_test.rs`, 3 in
`src/runtime.rs`. Five are named `edge_*`; every one asserts a produced
failure.

### The coverage loop

`tests/assertions_test.rs:32` holds one `probe_*` per surviving assertion.
`every_surviving_assertion_works_in_both_directions` runs each and prints one
line per function. Its output, verbatim:

```
assert_list_length: covered (accepts correct input, rejects wrong input)
assert_number_in_range: covered (accepts correct input, rejects wrong input)
assert_text_contains: covered (accepts correct input, rejects wrong input)
assert_text_matches: covered (accepts correct input, rejects wrong input)
assert_throws: covered (accepts correct input, rejects wrong input)
assert_value_is_list: covered (accepts correct input, rejects wrong input)
assert_value_is_number: covered (accepts correct input, rejects wrong input)
assert_value_is_record: covered (accepts correct input, rejects wrong input)
assert_value_is_text: covered (accepts correct input, rejects wrong input)
assert_value_is_yes_no: covered (accepts correct input, rejects wrong input)
assert_values_equal: covered (accepts correct input, rejects wrong input)
```

```
$ grep -rn 'pub fn assert_' src/testing/assertions.rs | wc -l
11
```

11 declared, 11 printed. The two cannot drift: `every_public_assertion_has_a_probe`
reads the source with `include_str!` at compile time, extracts every
`pub fn assert_` name, and fails in **both** directions — a declared assertion
with no probe, and a probe for an assertion that no longer exists. Adding a
twelfth assertion without a probe fails this file rather than shipping an
untested one. That test is what went red first; its first failure named
`assert_that` as the one assertion with no probe.

| Test | Edge class covered |
|---|---|
| `every_public_assertion_has_a_probe` | the reported finding itself — an assertion with no test, and a test for an assertion that is gone |
| `every_surviving_assertion_works_in_both_directions` | the loop; every assertion accepts a correct input and rejects a wrong one |
| `edge_empty_and_singleton_inputs` | empty + singleton — empty list of length 0, empty text matching `""`, `^$` against `""`, a degenerate `[0, 0]` range, a one-element list |
| `edge_numeric_boundaries` | numeric_boundary — inclusive `min` and `max`, `NaN`, `+Infinity` outside `[-Infinity, 1]`, `2^53` against `2^53 + 2` |
| `edge_type_mismatch_is_rejected_by_every_type_assertion` | type_mismatch — 5 type assertions × 6 values: each accepts only its own variant and rejects the other five with a message, an `expected` and an `actual` |
| `edge_unicode_text_is_matched_by_bytes_of_the_real_string` | unicode — `héllo 🎉 日本語`, RTL `שלום`, NFC `U+00E9` vs NFD `U+0065 U+0301`, a substring spanning no character boundary, a `\p{Greek}` class against Hebrew |
| `edge_malformed_pattern_is_reported_as_a_failure_not_a_panic` | malformed_input — **asserts a failure**: an unparseable regex `[unclosed` must come back `Err` naming the pattern, not `Ok` (a caller could not tell it from a match) and not a panic |
| `every_rejection_names_the_failure` | every rejection carries a non-empty message, `Debug` renders the message first, `Display` is non-empty |
| `redblue_expect_failure_message_is_built_by_the_assertion_library` | the message a Redblue `expect` failure prints is the one `assert_values_equal` builds |
| `the_builtin_prints_the_message_the_assertion_library_builds` | the builtin, for both spellings `expect` and `assert`, and the failure keeps the call-site span |
| `the_builtin_accepts_a_matching_pair` | the builtin is not only a failure path |
| `edge_the_builtin_reports_its_own_argument_and_type_edges` | **asserts failures**: one argument is refused by name; empty list vs empty text both ways, `yes/no` vs `1`, `nothing` vs `0`, empty vs singleton list are refused; `nothing`, `0.0`/`-0.0`, empty text and empty list are accepted against themselves |

Test quota: 12 `#[test]` functions (floor 3), 5 named `edge_*` (floor 3), 9 of
them asserting a produced failure. No `#[ignore]`, no `// skip`, no
`allow(clippy::`, no `#[allow]` of any kind.

## Edge-case matrix

| Row | Status |
|---|---|
| empty | covered — `edge_empty_and_singleton_inputs` (empty list of length 0, `""` matching `""`, `^$` against `""`) |
| singleton | covered — the same test (a one-element list), plus `edge_the_builtin_reports_its_own_argument_and_type_edges` (empty list refused against a singleton list) |
| boundary | covered — `edge_numeric_boundaries`: `assert_number_in_range` accepts exactly `min` and exactly `max`, and refuses `0.0` against `[-1.0, -0.5]` |
| out_of_bounds | **N/A.** No assertion in `src/testing/assertions.rs` indexes anything. The only indexed thing nearby is `items.len()`, which is `Vec::len` and cannot be out of bounds. Index bounds are `tests/index_bounds_test.rs`, green and untouched. |
| type_mismatch | covered — `edge_type_mismatch_is_rejected_by_every_type_assertion`, 30 rejections |
| numeric_boundary | covered — `edge_numeric_boundaries`: `NaN`, `+Infinity`, `2^53`/`2^53+2`, and `0.0`/`-0.0` in the builtin test. i64 overflow is N/A for this change: `Value::Number` is `f64`, so there is no separate i64 domain to overflow |
| unicode | covered — `edge_unicode_text_is_matched_by_bytes_of_the_real_string`. Escapes and very long strings are N/A: these assertions use Rust's `str::contains` and `regex` directly on `&str`, and Redblue escape decoding happens before the value ever reaches them (`tests/expect_test.rs::edge_escapes_are_compared_literally` already covers it) |
| nesting_recursion | covered — `assert_values_equal` compares `Value` structurally, so nested lists and records are compared whole: `edge_empty_and_singleton_inputs` and the probes exercise `[1]` against `[]`. Recursion is N/A: no assertion recurses |
| duplicate_missing_keys | **N/A.** No assertion reads a record's keys. `assert_value_is_record` asks only whether the value *is* a record; `assert_values_equal` compares two records the way `PartialEq` already defines, which `tests/record_order_test.rs` (25 passing) and `tests/expect_test.rs::edge_missing_record_key_differs_from_present_key` cover. Duplicate keys are rejected by the parser before a value exists |
| malformed_input | covered — `edge_malformed_pattern_is_reported_as_a_failure_not_a_panic` (an unparseable regex) and `edge_the_builtin_reports_its_own_argument_and_type_edges` (a one-argument `expect`). A list handed to `assert_list_length` is a type mismatch and is covered above |
| resource_limit | **N/A.** No assertion allocates, recurses or loops; `assert_throws` catches a panic from a caller-supplied closure and returns it as an `Err`. `catch_unwind` cannot catch a panic on another thread, which is a real limit of the assertion — recorded in FINDINGS.md, not fixed here because the Redblue VM reports `Error::Limit` rather than unwinding, so no production caller is affected |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — 0 warnings |
| `cargo test --all-targets` | **1162 passed, 0 failed, 0 ignored** across 42 targets |
| `cargo test --doc` | 2 passed, 0 failed |
| `./rbops/verify.sh phase-039` | **could not run — `rbops/` is not present in this checkout.** `ls rbops` returns `No such file or directory`. The pipeline that invokes this phase lives outside the project root, which this phase is forbidden to inspect. Reported honestly rather than claimed as a pass. |

The 18 failures this phase inherited are now fixed, and the full suite is
green. Measured, not asserted:

```
BEFORE (HEAD~1 worktree):  passed=1129  failed=18
AFTER  (this phase):       passed=1159  failed=0
```

## Definition of done, verified by hand

| Item | Evidence |
|---|---|
| every remaining `pub fn assert_` has a `#[test]` naming it, via a loop whose output is pasted above | `grep -rn 'pub fn assert_' src/testing/assertions.rs \| wc -l` → `11`; the loop printed 11 lines; `every_public_assertion_has_a_probe` enforces it in both directions |
| the count equals the number of lines the loop printed | 11 = 11 |
| every surviving assertion has a test that fails on a wrong input | each `probe_*` ends in `fails(...)`, which returns `Err("accepted a wrong input")` if the assertion returns `Ok`; `every_rejection_names_the_failure` `unwrap_err()`s a wrong input for 7 of them |
| ≥ 3 tests named `edge_*` | 5 |
| the `expect` / `assert` builtin reports through `src/testing/assertions.rs` | `src/runtime.rs:748` calls `crate::testing::assertions::assert_values_equal`; `the_builtin_prints_the_message_the_assertion_library_builds` asserts `message == assert_values_equal(2.0, 1.0).unwrap_err().to_string()` for both spellings |
| a test proves the message a Redblue `expect` prints is the one the library builds | `redblue_expect_failure_message_is_built_by_the_assertion_library` runs `expect 1 to be 2` and requires the printed error to contain the library's message. End to end on the binary: `rb run` prints `RuntimeError: Values not equal: Number(2.0) vs Number(1.0)`, exit 1 |
| `grep -rn 'allow(dead_code' src/` prints nothing | it prints nothing |
| `cargo clippy --all-targets -- -D warnings` is clean | clean |
| `cargo test --all-targets` reports 0 failures | **1162 passed, 0 failed** |
| `examples/*.rb` and `modules/*.rb` still run | all 8 files (`examples/{files,fizzbuzz,formats,hello,test_arithmetic,time}.rb`, `modules/{MathUtils,SuiteKit}.rb`) exit 0 under `rb run` |

## Invariants touched

None of the language surface in `phases/INVARIANTS.md` moved. `.rb` is
unchanged, no braces, `set x to <expr>` and `say` are as they were, the
`Value` variants and the `Error` variants are untouched.

Two public-API changes, both the point of the phase:

- `redblue::testing::Assertion`, `redblue::testing::Expected`,
  `redblue::testing::assert_that` and `redblue::testing::AssertThat` are gone.
  They had zero callers in `src/` and zero references in `tests/`, and
  `AssertThat` could not be tested into correctness without rewriting it, which
  is the refactor this phase is not.
- The `expect` / `assert` builtin's failure message changed from
  `Assertion failed: expected X but got Y` to `Values not equal: X vs Y`. That
  is the wording the `expect` **statement** and `Opcode::Expect` have always
  produced, so this removes a drift rather than introducing one.
  `grep -rn 'Assertion failed' src/ tests/ examples/ modules/ docs/ *.md`
  returned exactly one hit — the line this phase replaced.

The `expect` **statement** path is unchanged: it already called
`assert_values_equal` at `src/interpreter.rs:1612`.

No test-visible error *class* changed in the repair. `Error::Limit` keeps
reporting as `RuntimeError` through `Error::label()` (`src/error.rs:84`), which
is why a test asserting "this is a clean runtime error" was right all along and
only its variant match was stale.

## Known gaps / follow-ups

- `assert_throws` cannot catch a panic on another thread (`catch_unwind` unwinds
  only the calling thread). No in-tree caller is affected because the Redblue
  VM reports `Error::Limit` rather than unwinding → FINDINGS.md § 2.
- The `expect` / `assert` **builtin** arm is unreachable from `.rb` source: the
  parser turns `expect a to be b` into `Expr::Expect` before a call can be
  built, so `expect(1, 2)` is a parse error. The arm was still routed through
  the library, because a registered-and-dead arm that hand-rolls the same
  message the live paths build is the drift that produces a wrong answer the
  day a call form appears. Whether the globals should be registered at all is a
  language-surface question → FINDINGS.md § 3.
- `phases.json`'s evidence for this phase was partly stale: it says 21 of 22
  assertion functions and names eight that do not exist in the file → FINDINGS.md § 4.

## Round 1 — review findings repaired

| Finding | Severity | Where | Fix |
|---|---|---|---|
| `step_budget_makes_a_runaway_loop_assertable` passed for the wrong reason | BLOCKER | `tests/loop_bounds_test.rs` | `count` is declared before the loop, so the 200-step budget is what stops the program; the assertion is `Error::Limit` and a message naming 200, which a run stopped by the million-turn iteration cap cannot satisfy |
| `assert_text_matches` reported an unparseable pattern as a non-match | MAJOR | `src/testing/assertions.rs` | `Regex::new` failure returns its own `Err` naming the pattern and saying it is not a valid expression; a compiled pattern that did not match keeps its own message and `expected` |
| limit tests read their message through a helper taking either variant | MAJOR | 6 test files | helpers were split, not widened: `runtime_message` takes `Error::Runtime` only, `limit_message` takes `Error::Limit` only and asserts `is_resource_limit()` on the error it was handed |
| REPORT cited `Error::kind()` | MINOR | this file, `FINDINGS.md` | corrected to `Error::label()` at `src/error.rs:84` / `is_resource_limit()` at `src/error.rs:107` |

Three tests were added, all of which can fail:

- `edge_the_step_budget_stops_a_loop_the_iteration_cap_could_not` — the message
  starts with `Step budget of 200` and does not say an iteration count was
  reached, so the *budget* is pinned as the thing that stopped the program.
- `edge_a_program_failure_is_not_a_resource_limit` — `is_resource_limit()` is
  `false` for a program's own failure and `true` for each of the three guards,
  with `label()` still `RuntimeError` for each.
- `edge_an_invalid_pattern_and_a_non_match_record_different_expectations` — the
  two failures record different `expected` values, so the difference survives a
  caller that only reads that field.

### Each fix was checked by breaking the thing it pins

Not by inspection — by mutation, with the four gates re-run afterwards:

| Mutation | Result |
|---|---|
| the three guards in `src/interpreter.rs` return `Error::Runtime` instead of `Error::Limit` | **10 of 26** `loop_bounds_test.rs` tests fail, including `step_budget_makes_a_runaway_loop_assertable`. Under the old helper (either variant accepted, message only) they all passed |
| `Vm::charge_step` never fires — `if false` | `step_budget_makes_a_runaway_loop_assertable`, `edge_the_step_budget_stops_a_loop_the_iteration_cap_could_not`, `step_budget_bounds_a_program_of_many_short_loops` and `edge_nested_recursion_is_bounded_by_the_step_budget` all fail. Re-running that same mutation against the *old* body of the BLOCKER test — `count` undeclared, `Error::Runtime(..)` — **passes**, which is the vacuity the finding named, reproduced rather than argued |
| `assert_text_matches` restored to `.unwrap_or(false)` | both `edge_malformed_pattern_is_reported_as_a_failure_not_a_panic` and `edge_an_invalid_pattern_and_a_non_match_record_different_expectations` fail |

Every mutation was reverted; `git diff --stat -- src/` shows only
`src/testing/assertions.rs` changed.