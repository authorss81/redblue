# Phase 073 — Measure before optimizing: a benchmark harness with a checked-in baseline

## What changed

| File | Lines | What |
|---|---|---|
| `Cargo.toml` | +8 | Added the `rb-bench` binary target; it is a plain bin, not a `[[bench]]`, so `cargo test --all-targets` never runs the measuring code |
| `tests/bench/mod.rs` | +563 | Four workloads (`fibonacci`, `primes`, `concat`, `lex`), reference-answer checks, a 60-second self-enforced budget, table and `BENCH.md` rendering |
| `tests/bench/runner.rs` | +13 | The `rb-bench` entry point, outside the test gate path |
| `tests/bench_test.rs` | +338 | Correctness, table, parsed-baseline, and edge-case tests that do run inside the gate |
| `BENCH.md` | +15 | The measured baseline and hardware note from this landing machine |

## Reproduction of the finding (before the change)

```
$ ls -d benches benchmarks
ls: cannot access 'benches': No such file or directory
ls: cannot access 'benchmarks': No such file or directory
$ grep -n "criterion\|divan\|bench" Cargo.toml
(no output)
$ ls BENCH.md BENCHMARKS.md
ls: cannot access 'BENCH.md': No such file or directory
ls: cannot access 'BENCHMARKS.md': No such file or directory
```

The first failing test was `bench_md_exists_and_names_all_workloads`, which failed because `BENCH.md` did not exist. It now reads the checked-in baseline and requires all four workload names plus the hardware note.

## Tests added

| Test | Edge class covered |
|---|---|
| `bench_md_exists_and_names_all_workloads` | checked-in baseline exists, records hardware, parses every row, and each `(size, answer)` agrees with `run_workload` at its default size |
| `bench_workloads_compute_the_documented_literals` | boundary: fib(20)=6765, primes below 1000=168, concat is exact, lex statement count |
| `bench_table_lists_four_workloads_and_wall_time` | four rows, one header, wall time present but never asserted exact |
| `bench_markdown_shape_names_answers_and_hardware` | generated baseline has the workload names, wall-time column, hardware section |
| `edge_zero_size_completes_with_trivial_answers` | empty / zero: all four workloads at size 0 |
| `edge_singleton_sizes_are_exact` | singleton: fib(1), primes below 2, one concat, one lex statement |
| `edge_a_wrong_answer_is_refused_instead_of_measured` | failure channel: a deliberately wrong reference answer returns `WrongAnswer`, never a `Measurement` |
| `edge_a_workload_error_is_reported_as_load` | failure channel: a workload that returns an error surfaces as `BenchError::Load` |
| `edge_a_workload_that_stops_without_an_answer_is_a_load_error` | failure channel: a disconnected worker surfaces as `BenchError::Load` |
| `edge_empty_workload_name_lists_the_four` | malformed input: empty name is a usage error naming all four |
| `edge_negative_and_malformed_sizes_are_refused` | type mismatch / malformed input: `-1` and `many` are refused |
| `edge_cli_empty_workload_name_is_a_usage_error` | failure channel: CLI exits 2 for an empty name, 0 for `--help` |
| `edge_cli_unknown_workload_with_size_is_a_usage_error` | failure channel: both `bogus` and `bogus 5` exit 2 with usage |
| `edge_a_hung_workload_fails_loudly_at_its_budget` | resource limit: 60-second budget is enforced; a synthetic hung workload returns `BudgetExceeded` |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 2669 passed, 0 failed |
| `../rbops/verify.sh phase-073` | pass — `VERIFY PASS — phase-073`, 2669 tests, 14 rust / 10 edge / 9 failure-asserting, examples green |

## Test requirement matrix

| Row | Result |
|---|---|
| empty | covered — empty workload name, zero-sized workloads, empty concat text |
| singleton | covered — fib(1), primes below 2, one concat iteration, one lex statement |
| boundary | covered — fib(20)=6765, primes below 1000=168, exactly 200 concat iterations |
| out_of_bounds | N/A — the harness indexes no user collection; sizes are the only index-like boundary and are checked |
| type_mismatch | covered — non-numeric size text is refused |
| numeric_boundary | covered — zero and negative sizes; no benchmark asserts floating-point equality beyond the exact integer answers |
| unicode | N/A — workloads use ASCII literals; the harness does not transform text encoding |
| nesting_recursion | N/A — all four workloads are iterative; no recursive call depth is exercised |
| duplicate_missing_keys | N/A — workloads use no records |
| malformed_input | covered — empty workload name, non-numeric size, negative size |
| resource_limit | covered — 60-second budget with a deliberately hung workload; the budget bounds the wait, and the uncancellable worker is detached (see known gaps) |

## Invariants touched

- None. No language syntax, `Value` variant, error kind, example, or module changed.
- The measuring harness is a separate `rb-bench` binary; the test gate only sees the correctness tests.

## Known gaps / follow-ups

- A pre-existing language bug was found while building the workloads: `give back` inside an `if`/`while` block does not return early. The interpreter arm at `src/interpreter.rs:2253` evaluates the value and continues the enclosing block. Reproduced with `to f(n) ... if n < 2 then give back 10 end give back 20 end; say f(1)` printing `20`. Recorded in `FINDINGS.md`; fixing it is out of scope for this test-only phase.
- `BENCH.md` is a debug-build baseline produced by `cargo run --bin rb-bench`; release numbers are not recorded yet.
- The 60-second budget bounds how long the harness waits, not how long the workload runs. The interpreter has no cancellation hook, so a timed-out worker thread is detached and may keep consuming CPU until the test process exits. In the CLI this is masked by `std::process::exit`; a future harness can run measurements in a child process if it needs a hard kill.
- The full gate was run with `GIT_INDEX_FILE=target/tmp/phase073_index` on a copy of the index. This environment resets the main `.git/index` during the test run, so a new test file loses its intent-to-add entry before the test-policy step (the same workaround recorded by phase-094). The work is still ordinary working-tree changes for the pipeline to commit.
