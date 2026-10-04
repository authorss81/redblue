# Phase 013 — Bounded loops and interruptible execution

## Reproduction

The finding reproduces on `main`. Two runs before any edit, both against
`target/debug/rb` built from the unmodified tree:

```
$ cat target/tmp/inf.rb
set n to 0
set stop to 0
while stop is 0
    set n to n + 1
end
say "never"

$ timeout 6 ./target/debug/rb run target/tmp/inf.rb
EXIT=124                       # killed by `timeout`; nothing in the language stopped it
```

```
$ cat target/tmp/rep.rb
set t to 0
repeat 500000000 times
    set t to t + 1
end
say t

$ timeout 25 ./target/debug/rb run target/tmp/rep.rb
real  0m25.002s
EXIT=124                       # 500M iterations, still running when the host gave up
```

Exit 124 is `timeout`'s code: the only way to stop either program was an
out-of-band signal. `try ... catch error` cannot see that, no harness could
assert on it, and no limit could be raised because none existed. In source,
`src/vm.rs` looped `for item in items` / `while … is_truthy()` / `for _ in 0..n`
with no counter and no cancellation hook.

## What changed

| File | Lines | What |
|---|---|---|
| `src/vm.rs` | +137 −1 | `MAX_ITERATIONS` / `MAX_STEPS` and their env overrides; `Vm::{steps, max_steps, max_iterations}`; `charge_step` called from `execute_statement`; `charge_iteration` called once per iteration of `ForEach`, `ForRange`, `Repeat` and `While` |
| `src/lib.rs` | +5 −1 | exports `MAX_ITERATIONS`, `MAX_ITERATIONS_ENV`, `MAX_STEPS`, `MAX_STEPS_ENV`, `resolve_max_iterations{,_from}`, `resolve_max_steps{,_from}` |
| `tests/loop_bounds_test.rs` | +535 new | 22 tests |

Both limits resolve from the environment the way `MAX_CALL_DEPTH` already did,
with the same rule that a limit of zero is ignored (it would make every loop
illegal). The policy half is split into `resolve_*_from(Option<usize>)` so the
tests can exercise it without mutating the shared process environment.

Two deliberate choices:

- **The iteration counter is a local of the loop statement**, so the cap is *per
  loop* and nesting does not multiply it. Twenty nested 2-iteration loops are
  fine; `MAX_ITERATIONS` is not a depth-times-anything budget.
- **`steps` is monotonic and is never restored by `try`.** A program cannot
  catch its own step-budget error and continue on a fresh budget.

After the change, the same two commands:

```
$ timeout 20 ./target/debug/rb run target/tmp/inf.rb
Error: RuntimeError: Maximum of 1000000 iterations reached in a 'while' loop
  --> target/tmp/inf.rb:3:1
EXIT=1

$ time ./target/debug/rb run target/tmp/rep.rb
Error: RuntimeError: Maximum of 1000000 iterations reached in a 'repeat' loop
real  0m1.020s
EXIT=1
```

`./target/debug/rb test` still reports 229 passed / 0 failed, and all 6 files in
`examples/` still run.

## Tests added

22 `#[test]` functions in `tests/loop_bounds_test.rs`, 12 of them named `edge_*`,
and 9 that assert a failure is produced. Floor was ≥ 3 tests and ≥ 1 `edge_*`.

| Test | Edge class covered |
|---|---|
| `infinite_while_loop_terminates_with_a_clean_error` | resource_limit — the finding; asserts `Error::Runtime` and that the message names the limit |
| `huge_repeat_count_fails_instead_of_running_forever` | resource_limit — the second reproduction |
| `runaway_loop_process_exits_instead_of_hanging` | resource_limit, cross-process; exit 1, limit in stderr, `say "finished"` after the loop never reached |
| `runaway_loop_is_catchable_from_inside_redblue` | resource_limit — the error is catchable by `try ... catch error`, asserted with `expect caught to be yes` |
| `edge_zero_iterations_is_allowed` | empty — `repeat 0`, `for each` over `[]`, a false `while`, all at cap 1 |
| `edge_exactly_max_iterations_succeeds` | boundary — `max` is allowed, for all three reachable loop forms |
| `edge_max_plus_one_iterations_fails` | boundary — `max + 1` is a clean error naming the limit, for all three forms |
| `edge_iteration_cap_applies_to_each_loop_form` | boundary — a long `for each` list is not exempt |
| `edge_nested_loops_each_get_the_full_cap` | nesting_recursion — nesting does not double-charge the cap |
| `edge_nested_recursion_is_bounded_by_the_step_budget` | nesting_recursion, resource_limit — a loop inside a function inside a loop; the *step budget* is what stops it, which is the case the per-loop cap alone misses |
| `edge_deeply_nested_loops_terminate` | resource_limit — 16 levels, 2^16 inner executions, terminates |
| `step_budget_bounds_a_program_of_many_short_loops` | resource_limit — unboundedness as 100 nested 2-iteration loops, which no per-loop cap can see |
| `step_budget_does_not_fire_under_the_limit` | boundary — a program inside its budget is untouched |
| `edge_step_budget_is_not_spent_by_unentered_loops` | empty — an unentered loop costs only its own statement |
| `step_budget_makes_a_runaway_loop_assertable` | resource_limit — the DoD's "infinite loop is testable": 200 steps is enough to assert the exact error, independent of host speed |
| `edge_count_beyond_i64_is_a_clean_error_not_a_panic` | numeric_boundary — `99999999999999999999` and `1e300`; asserts `Runtime`, not a panic or a hang |
| `edge_fractional_and_negative_counts_run_zero_times` | numeric_boundary — `0.5`, `0`, `-5` |
| `edge_type_mismatch_count_is_not_a_loop` | type_mismatch — a text count is not a loop; pre-existing behaviour preserved, not turned into an error |
| `edge_type_mismatch_iterable_is_not_a_loop` | type_mismatch — `for each x in 5` |
| `published_limits_are_finite_and_positive` | boundary — both resolved defaults are positive and the budget exceeds one loop's cap |
| `a_zero_limit_falls_back_to_the_default` | boundary — a limit of zero is ignored, a positive one honoured, for both limits |
| `shipped_examples_fit_inside_the_published_limits` | resource_limit — the bound must not reject programs the language ships; runs all 6 `examples/*.rb` through a default VM |

Matrix rows not covered, and why:

- **singleton** — N/A. A loop's singleton case *is* its exactly-one-iteration
  case, which is the boundary row; the cap has no separate singleton behaviour
  to assert.
- **out_of_bounds** — N/A. This change adds no index arithmetic. The existing
  `tests/index_bounds_test.rs` covers indexing, which loops do not change.
- **unicode** — N/A. No string is produced, parsed or measured by this change;
  the cap counts iterations and statements, both integers. `redblue` has no
  string-iteration loop, so a unicode loop body cannot change any count.
- **duplicate_missing_keys** — N/A. No record is read or written. A loop
  accumulator's record keys are unchanged behaviour, already covered by
  `tests/record_order_test.rs` and `tests/test_records.rb`.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 245 passed, 0 failed, 0 ignored |
| `./rbops/verify.sh phase-013` | **not run — the script is not in this checkout** |

On the fourth gate: `rbops/` does not exist under the project root
(`ls rbops/verify.sh` → `No such file or directory`), and the task instructions
say the RBOPS pipeline lives outside the working directory and must not be
inspected. I did not run it and I am not claiming a result for it. What I ran
in its place, all green:

```
cargo test --all-targets   245 passed, 0 failed, 0 ignored
./target/debug/rb test    Tests run: 229  Passed: 229  Failed: 0
examples/*.rb              all 6 run with no error output
```

`rb format --check` is red for 22 of 24 `.rb` files and `rb lint` warns
`Unused variable: 'name'`. Both are **pre-existing**: this phase touched
`src/vm.rs`, `src/lib.rs` and one new Rust test file — no `.rb` file and no
formatter code. Recorded in `FINDINGS.md` §4 rather than fixed here.

## Invariants touched

- None. No `.rb` extension change, no `to … end` / `set x to` / `say` change,
  no `Value` variant, no `Error` variant, no grammar change, no parser change.
- New: a runaway loop is now a catchable `Error::Runtime` instead of a process
  that never returns. This is the phase's purpose, and it is the same class of
  change as `MAX_CALL_DEPTH` — a bound that produces a `RuntimeError`.
- Existing tests in `tests/`: 223 before, 223 after, all still passing.

## Known gaps / follow-ups

- `break` and `skip` are still `Ok(Value::Nothing)` (`src/vm.rs:381`,
  `src/vm.rs:385`), so there is still no user-level loop escape. A runaway loop
  is stoppable from outside and testable, but not from inside Redblue other than
  by `try ... catch error` around the whole loop. → `FINDINGS.md` §1
- `Statement::ForRange` is unreachable: no parser arm constructs it, though the
  VM arm is capped. `for each i from 1 to 10` remains unimplemented against
  `SPEC.md:383`. → `FINDINGS.md` §2
- The default step budget (10M) is a fixed number, not a wall-clock deadline. At
  the ~1.1M steps/sec measured on this host it is roughly 9 s in a debug build.
  A deadline would need a clock, which would make test results depend on the
  host — the reason a step count is used instead.
- The step budget does not bound a single long `evaluate` (a deeply nested
  expression), only statement count. Expression recursion is not bounded by
  anything in this phase. → `FINDINGS.md`
- `src/testing/harness.rs:219` builds `Vm::new()` per test block, so the budget
  for every Redblue `test` comes from `REDBLUE_MAX_STEPS` and cannot be set per
  test. `Vm::with_max_steps` exists but nothing in `src/testing/` calls it.
  → `FINDINGS.md` §5
