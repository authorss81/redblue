# phase-039 findings

Work that belongs to a future phase. Recorded here so the auditor can promote
it with file:line evidence, per AGENTS.md § 1.7.

## 1. 18 pre-existing test failures in the resource-limit family

**Severity: major. Not caused by phase-039, and not fixed by it.**

Measured with the full suite on both sides of phase-039 (`git stash` for the
"before"), the failing set is byte-identical:

```
BEFORE (main):   passed=1129  failed=18  ignored=0
AFTER  (p039):   passed=1141  failed=18  ignored=0
```

All 18 are one kind of failure: a test that asserts `Error::Runtime` where the
code now returns `Error::Limit`.

| File | Count | Representative assertion |
|---|---|---|
| `tests/call_depth_test.rs` | 4 | `:85` — `expected RuntimeError, got Limit("Maximum call depth of 1000 reached while calling 'boom'", Span { line: 2, column: 5 })` |
| `tests/loop_bounds_test.rs` | 7 | `edge_a_skip_is_charged_as_one_iteration`, `edge_max_plus_one_iterations_fails`, … |
| `tests/for_range_test.rs` | 2 | `huge_repeat_count_fails_instead_of_running_forever`, … |
| `tests/loop_control_test.rs` | 2 | `infinite_while_loop_terminates_with_a_clean_error`, … |
| `tests/function_literal_test.rs` | 1 | `edge_a_literal_nested_inside_itself_stops_at_the_call_depth_limit` |
| `tests/numeric_edge_test.rs` | 1 | `edge_nested_recursion_is_bounded_by_the_step_budget` |
| `tests/object_model_test.rs` | 1 | `edge_mutually_recursive_methods_hit_the_call_depth_limit` |

Phase-038 introduced `Error::is_resource_limit()` and the three limits now
report `Error::Limit` rather than `Error::Runtime`. The tests were not
updated. Fixing them means deciding whether `Error::Limit` should match a
`matches!(err, Error::Runtime(..))` arm or whether the tests should be widened
to `Error::Runtime | Error::Limit` — that is a design question about the
error surface, and phase-039's mandate is the assertion library. Not done
here.

## 2. `assert_throws` cannot catch a panic on another thread

`src/testing/assertions.rs:156` wraps its closure in
`std::panic::catch_unwind`, which unwinds only the calling thread. A Redblue
program run on a spawned thread (or any embedder that hosts the VM on one)
would lose the process instead of getting an `Err`.

No in-tree caller is affected: the Redblue VM reports `Error::Limit` at the
call-depth and step-budget guards rather than unwinding, so nothing this
assertion is meant to catch actually panics. Phase-039 added the first test
that calls it at all (`probe_assert_throws` in
`tests/assertions_test.rs`, plus `every_rejection_names_the_failure`), and both
pass. A closure that panics is caught; a closure that spawns a panicking
thread is not, and documenting that limit is the honest fix rather than
pretending the assertion is total.

## 3. The `expect` / `assert` builtin arm is unreachable from Redblue source

`src/stdlib.rs:188-189` registers `expect` and `assert` as
`Value::Builtin`, and `src/runtime.rs:735` handles them. But the parser turns
`expect a to be b` into `Expr::Expect` before a call can be built, so the
builtin arm is dead from `.rb`:

```
$ ./target/debug/rb run target/tmp/t1.rb      # set x to expect(1, 2)
Error: ParserError: Unexpected token Expect
  --> target/tmp/t1.rb:1:10
```

Phase-039 still routed the arm through `assert_values_equal`, because a
registered-and-dead arm that hand-rolls the same message the live paths build
is exactly the drift that produces a wrong answer the day a call form appears.
The three unit tests in `src/runtime.rs::expect_builtin_tests` drive it
through `runtime::builtin` directly, which is the only way to reach it today.

The real question this raises is separate: should `expect` and `assert` be
registered as globals at all, given that the parser owns the `expect` keyword?
Either the registration is removed, or a call form (`expect(a, b)`) is given
a grammar. That is a language-surface decision and needs its own phase.

## 4. The finding's own evidence was partly stale

`phases.json` for phase-039 says "the 21 public assertion functions nothing
calls", names `assert_true`, `assert_false`, `assert_panics`,
`assert_not_panics`, `assert_contains`, `assert_matches_regex`, `assert_len`,
`assert_type`, and cites `src/runtime.rs:559-576`. On `main` there were **12**
`pub fn assert_*` and 11 uncalled, none of those eight names existed, and the
builtin arm is at `src/runtime.rs:735`. The substance held (see REPORT.md) but
the numbers did not, so the phase prompt should not be trusted as a count.
