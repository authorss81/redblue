# Phase 006 — Bounded call depth in the VM

## Reproduction

The finding's first claim reproduces exactly; its second does not, on this commit.

```
$ grep -n "depth|stack|limit" src/vm.rs
1114:    let mut depth = 0;          # split_json_pairs bracket counting, not call depth
```

No frame or call counter existed. But the claimed abort did **not** reproduce,
because a call to a user function never ran the body at all — `src/vm.rs:1004`
matched `Value::Function` and returned `Value::Nothing`, and the body was
dropped at the declaration (`src/vm.rs:264`, `body: _`). Unbounded recursion was
therefore not reachable, only silent no-ops:

```
$ printf 'to boom(n)\n  boom(n + 1)\nend\nsay "before"\nboom(0)\nsay "after"\n' > deep.rb
$ ./target/debug/rb run deep.rb
before
after          # exit 0 — boom(0) printed nothing and recursed zero times
```

So the counter could not be added as a guard on a live path: it would have been
dead code, and 3 of the 5 definition-of-done items (infinite recursion errors,
depth 999 succeeds, mutual recursion bounded) would have been untestable.
Retaining the body is the prerequisite, and it is where the real stack risk
lives — with bodies retained and no counter, the abort appears immediately:

```
$ printf 'set reached to 0\nto countdown(n)\n  if n is 0 then\n    set reached to 0\n  else\n    set reached to countdown(n - 1)\n  end\n  give back reached\nend\nsay countdown(998)\n' > deep.rb
$ ./target/debug/rb run deep.rb
thread 'main' has overflowed its stack
fatal runtime error: stack overflow, aborting
$ echo $?
134
```

Body retention is filed as `phases/phase-003/FINDINGS.md` §3. It is done here
**without** changing the public `Value::Function` variant: the body is kept in
the VM's own `functions` map (`HashMap<String, Vec<Stmt>>`), so the deferred
public-API phase stays open.

The other reproducible abort, which is *not* this phase's concern, is in the
parser: `set x to ((((1))))` nested 20 000 deep aborts the same way. See
`FINDINGS.md` §1.

## What changed

| File | Lines | What |
|---|---|---|
| `src/vm.rs` | +129 −20 | `MAX_CALL_DEPTH` (1000), `MAX_CALL_DEPTH_ENV` (`REDBLUE_MAX_CALL_DEPTH`), `resolve_max_call_depth`, `call_depth`/`max_call_depth` on `Vm`, `with_max_call_depth`, `call_user_function` (check → push scope → bind params → run body → pop scope → release), body retention for `to` declarations, `run_isolated` |
| `src/lib.rs` | +5 −5 | exports `MAX_CALL_DEPTH`, `MAX_CALL_DEPTH_ENV`, `run_isolated`; `run_source` runs through `run_isolated` |
| `src/testing/harness.rs` | +2 −4 | test blocks run through `run_isolated` so `rb test` gets the same bound |
| `tests/test_functions.rb` | +66 −2 | 5 Redblue blocks for calls, caught depth errors, mutual recursion; header comment corrected |
| `tests/call_depth_test.rs` | +241 new | 11 Rust tests |

### Why `run_isolated`

A call frame costs about 58 KiB in an unoptimised build (six nested Rust frames
per Redblue call). Measured on this machine, the default 16 MiB main-thread
stack aborts at depth ~280 — so a 1000 limit on the main stack would still abort
before it was reached, i.e. the counter would be a promise the process cannot
keep. `run_isolated` runs the program on a thread whose stack is
`limit × 256 KiB` (`STACK_BYTES_PER_CALL`), which is what makes "depth 999
succeeds, depth 1001 errors" true rather than nominal. A spawn failure is
reported as `Error::Io`, never as an abort.

`Vm` therefore keeps `Send` (no `Rc` in the body store), so `run_isolated` can
hand the `Vm` back and `take_expectation_failure` keeps working in the harness.

## Tests added

11 `#[test]` functions in `tests/call_depth_test.rs` and 5 Redblue blocks in
`tests/test_functions.rb`.

| Test | Edge class covered |
|---|---|
| `infinite_recursion_is_a_runtime_error_not_a_stack_overflow` | resource limit; asserts `Error::Runtime` + names function and limit |
| `infinite_recursion_exits_one_in_a_child_process` | resource limit; exit code 1, not 134; stderr free of `stack overflow` |
| `mutual_recursion_across_two_functions_is_bounded` | nesting/recursion; `ping`↔`pong` |
| `depth_just_below_the_limit_still_succeeds` | boundary; depth 998 returns `0` |
| `edge_the_limit_itself_is_the_boundary` | boundary + out_of_bounds; 1000 frames succeed, 1001 error |
| `depth_error_is_catchable_by_try_catch` | failure asserted; `try/catch` recovers and stdout is exactly `2\n` |
| `env_var_overrides_the_call_depth_limit` | configuration; limit 5 rejects depth 10 and accepts depth 3 |
| `edge_zero_limit_falls_back_to_the_default` | empty/zero; `REDBLUE_MAX_CALL_DEPTH=0` → default, `double(21)` is `42` |
| `edge_invalid_limits_fall_back_to_the_default` | malformed input; `"abc"`, `"-1"`, `""`, `"1.5"` → default |
| `edge_three_nested_calls_are_not_recursion` | nesting; 3 nested calls are legal |
| `with_max_call_depth_bounds_a_program_without_the_environment` | configuration in process, no env |
| `functions: a call runs the body and yields its last value` | Redblue: `add(2, 3)` is `5` |
| `functions: a call sees only its own parameters` | Redblue: scoping |
| `edge_functions_unbounded_recursion_is_a_caught_error` | Redblue: failure is produced and caught |
| `edge_functions_mutual_recursion_is_bounded` | Redblue: mutual recursion |
| `edge_functions_a_caught_depth_error_leaves_the_program_usable` | Redblue: state after a caught error |

## Edge-case matrix

| Row | Status |
|---|---|
| empty / zero / nothing | covered — `edge_zero_limit_falls_back_to_the_default`; a limit of `0` is not honoured |
| singleton and boundary | covered — `edge_the_limit_itself_is_the_boundary`, `with_max_call_depth…`; exactly one frame and exactly the limit |
| out of bounds | covered — depth `limit + 1` is a clean `RuntimeError`, not a panic (`edge_the_limit_itself_is_the_boundary`) |
| type mismatch | N/A — this change never converts a value; the pre-existing argument-binding rule (missing → `nothing`, extra ignored) is recorded in `FINDINGS.md` §3 |
| numeric boundary | N/A — no arithmetic added. The one number parsed here is the limit itself, covered for `"0"`, `"-1"`, `"1.5"`, `""`, `"abc"` |
| unicode | N/A — no string handling added; the error message is ASCII by construction and a non-ASCII function name is not expressible in the grammar today |
| nesting / recursion | covered — self recursion, mutual recursion, 3 nested calls, and depth 998 |
| duplicate / missing keys | N/A — no records or fields added |
| malformed input | covered — invalid `REDBLUE_MAX_CALL_DEPTH` values; a malformed *program* never reaches the counter (the parser rejects it first) |
| resource and state | covered — the whole point of the phase: the stack is sized from the limit and an unbounded `try`/`catch` cycle cannot accumulate depth (`call_depth` is released on the error path, asserted by `depth_error_is_catchable_by_try_catch`) |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test --all-targets` | 103 passed, 0 failed, 0 ignored (92 before, 11 added) |
| `rb test` (Redblue suite) | 192 passed, 0 failed (187 before, 5 added) |
| `examples/*.rb` | all exit 0; `modules/MathUtils.rb` still fails to parse — pre-existing, `phases/phase-003/FINDINGS.md` §6, verified unchanged by `git stash` |
| `./rbops/verify.sh phase-006` | **could not run — `rbops/` is not in this checkout** (same as `phases/phase-004/FINDINGS.md` §1; only `.github/` exists). Nothing was authored under `rbops/` to work around it. |

## Invariants touched

- **None of the language-surface invariants.** `.rb`, `to … end`, `set x to`,
  `say`, the `Value` variants and the `Error` variants are unchanged; the
  public `Value::Function(name, params)` shape is untouched.
- **Behaviour change: a call to a user function now runs its body.** It used to
  return `nothing` unconditionally. This is required by the definition of done
  (a depth limit is meaningless if calls do not recurse) and is the body
  retention filed as `phases/phase-003/FINDINGS.md` §3. Visible effect:
  `examples/hello.rb` prints a third line, `Hello, {name}!`, where it printed
  nothing before. The braces are literal because `{interp}` is still
  unimplemented (`phases/phase-003/FINDINGS.md` §2) — not a regression here.
- A depth violation is a catchable `Error::Runtime`, i.e. the error surface
  AGENTS.md §2 fixes; the program exits 1 instead of aborting with 134.

## Known gaps / follow-ups

- The REPL still runs on the main thread (`src/repl/mod.rs:240` calls
  `vm.run` directly), so unbounded recursion typed at the prompt still aborts.
  It keeps a persistent `Vm` across lines, so wrapping each line would mean
  moving the VM in and out of a sized thread. → `FINDINGS.md` §2
- Deeply nested *source* still aborts in the parser before the VM is reached.
  → `FINDINGS.md` §1
- Arity is not checked when binding arguments. → `FINDINGS.md` §3
- An unbounded `while`/`repeat` loop is still unbounded; this phase bounds call
  depth only. → `FINDINGS.md` §4
