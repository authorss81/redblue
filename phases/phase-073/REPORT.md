# Phase 073 — Measure before optimizing: a benchmark harness with a checked-in baseline

## What changed

| File | Lines | What |
|---|---|---|
| `Cargo.toml` | +8 −0 | Added the `rb-bench` binary target; it is a plain bin, not a `[[bench]]`, so `cargo test --all-targets` never runs the measuring code |
| `tests/bench/mod.rs` | +1744 −0 | New file: four workloads (`fibonacci`, `primes`, `concat`, `lex`), reference-answer checks, a distinct `SizeTooLarge` refusal, a 60-second self-enforced budget shared across the no-argument run and the `--compare` loop, table and `BENCH.md` rendering, an injectable output path, a checked wall-time cell parser, a manifest-root baseline path for tests, a platform-aware hardware probe, an advisory `--compare` mode, baseline-row validation that refuses unknown, duplicate, oversized, and misspelled rows, a killable child-process measurement mode with an explicit child environment and start-marker handshake, a structured `WRONG` worker line so the child's wrong answer reaches `WrongAnswer` instead of being flattened to `Load`, and unit tests for every branch of the worker protocol parser |
| `tests/bench/runner.rs` | +19 −0 | The `rb-bench` entry point, outside the test gate path, with a hidden `--worker` mode used by the child-process measurement path |
| `tests/bench_test.rs` | +1056 −0 | The earlier correctness, table, parsed-baseline, no-argument runner, `--compare` CLI, numeric-limit, wall-time recording, baseline-parser, comparison-rendering, and edge-case tests that run inside the gate (already in `HEAD`) |
| `tests/bench_test.rs` | +204 −0 | Later attempts add character-safe truncation of long and Unicode answers, header/prose rows ignored by the baseline parser, `check_size_supported` refusing a negative size and an unknown workload, timing-artifact placement, the hidden child-process timeout/kill test, and the baseline-missing-a-workload refusal |
| `tests/bench_test.rs` | +85 −0 | This resume adds the four tests that produce the diff under `tests/` and pin the size-boundary refusal, the no-write single-workload path, the usage contract, and the checked-in baseline's immutability during the correctness run |
| `tests/unreviewed_work_ack_test.rs` | +10 −5 | Puts the in-flight `resume phase-073:` delivery prefix on the audit's delivery rule, so the delivery commit this phase produces is acknowledged by its subject rather than by a SHA that goes stale (landed in the first attempt) |
| `BENCH.md` | +15 −0 | New file: the measured baseline from this landing machine — verified answers **and** wall times, plus the hardware note and the comparison contract |
| `.gitignore` | +6 −1 | The timing-ignore prose now names `BENCH.md` as the checked-in landing-machine baseline instead of claiming only answers are tracked |

The counts above are the full phase diff against the base commit, not per-attempt
deltas. The `Cargo.toml` binary target, the original `tests/bench/mod.rs` and
`tests/bench/runner.rs`, the earlier `tests/bench_test.rs` tests,
`tests/unreviewed_work_ack_test.rs`, `BENCH.md`, and `.gitignore` landed in the
preserved `HEAD` commit; later attempts add the remaining `tests/bench_test.rs`
edge tests, move production measurement into a killable child process, close the
worker-protocol and timeout verification gaps, and update this report.

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
| `edge_cli_too_many_arguments_is_a_usage_error` | malformed input: a third CLI argument exits 2 instead of being silently ignored |
| `edge_unknown_workload_error_names_every_workload` | failure channel: an unknown non-empty name is a usage error that lists all four workloads |
| `edge_size_parsing_boundaries` | numeric boundary: `0` and `i64::MAX` parse; one past `i64::MAX` is a clean usage error |
| `edge_workload_names_are_unique_and_defaults_are_positive` | duplicate keys: workload names are unique and every default size is positive |
| `edge_size_limit_boundary_is_exact_per_workload` | numeric boundary: each workload's `max_size` is accepted by the non-running size check, `max_size + 1` is refused with `SizeTooLarge`, and one small bounded size runs end-to-end so the gate never times a maximum-size measurement |
| `edge_the_size_limit_is_enforced_without_running_the_workload` | resource limit: the size check is exercised directly, so the boundary is pinned without materializing an answer |
| `edge_fibonacci_is_capped_where_f64_is_exact` | numeric boundary: fib(78) is exact and measured; fib(79) is refused before rounding or infinity can pass the check |
| `edge_cli_no_arguments_runs_all_and_writes_a_parseable_baseline` | primary entry point: the no-argument runner writes one parseable row per workload at its default size |
| `edge_cli_no_arguments_reports_a_write_failure` | failure channel: an unwritable baseline path returns exit 1 instead of a silent success, and the timing artifact stays inside the test sandbox |
| `edge_cli_compare_reruns_every_baseline_row` | primary entry point: `--compare` reads a baseline, re-runs every recorded row, exits 0, and renders a current time for each row |
| `edge_a_hung_workload_fails_loudly_at_its_budget` | resource limit: 60-second budget is enforced; a synthetic hung workload returns `BudgetExceeded` |
| `edge_a_hung_child_process_is_killed_at_its_budget` | resource limit: the real child path spawns the hidden sleeping worker with an explicit sentinel environment, waits for a start-marker handshake before the budget begins, returns `BudgetExceeded` at the deadline, and the child (which sleeps ten seconds) never reaches its finish marker, so it cannot outlive the budget |
| `edge_worker_output_parser_decodes_well_formed_lines` | parsed input: the worker protocol decodes both a number answer and a text answer with their durations |
| `edge_worker_output_parser_refuses_an_error_line` | failure channel: an `ERR` line from the worker becomes a `Load` error instead of an answer |
| `edge_worker_output_parser_decodes_a_wrong_answer` | failure channel: the worker's structured `WRONG` line round-trips through the encoder and parser into `BenchError::WrongAnswer`, so the shipped path reports the distinct variant instead of flattening it to `Load` |
| `edge_child_measurement_accepts_an_explicit_environment` | environment coupling: the child measurement entry point passes a non-empty explicit environment to the hidden worker, which writes sentinel markers only when it observes the injected variable, while the process-global environment stays untouched |
| `edge_worker_output_parser_refuses_unreadable_fields` | failure channel / malformed input: a missing field, unreadable duration, unreadable number, and unknown tag are each refused |
| `edge_worker_output_parser_refuses_bad_text_payloads` | failure channel / unicode: non-UTF-8 text, odd-length hex, and a non-hex payload are each refused |
| `edge_the_hidden_hang_workload_is_not_a_listed_workload` | malformed input / isolation: the timeout helper is absent from the four public workload names while its sentinel variable stays a stable contract |
| `edge_cli_compare_refuses_a_baseline_missing_a_workload` | failure channel / coverage: a baseline that omits a workload is refused by name instead of comparing as a smaller baseline |
| `edge_recorded_wall_times_are_present_for_every_workload` | baseline coverage: every checked-in row carries a finite, non-negative wall time |
| `edge_malformed_recorded_wall_time_is_rejected` | failure channel / malformed input: missing unit, non-numeric, negative, `NaN` and infinite wall-time cells are refused |
| `edge_render_markdown_records_each_measurement_wall_time` | boundary: rendered rows carry the measured elapsed time and the hardware note |
| `edge_wall_time_cell_parser_rejects_malformed_text` | failure channel: the wall-time cell parser rejects malformed text and accepts a valid cell |
| `edge_checked_in_baseline_is_resolved_from_the_crate_root` | environment coupling: the baseline test path comes from the manifest root, not the cwd or `RB_BENCH_OUTPUT` |
| `edge_baseline_parser_reads_every_recorded_row` | parsed input: every recorded data row is read with its size, answer, and wall time |
| `edge_baseline_parser_rejects_a_corrupted_known_row` | failure channel / malformed input: a corrupted size or wall time on a known workload is refused instead of skipped |
| `edge_baseline_parser_rejects_unknown_duplicate_and_oversized_rows` | failure channel / duplicate keys / resource limit: an unknown workload is refused even when its size is also malformed, a repeated `(workload, size)` row is refused, and a size past the checkable maximum is refused |
| `edge_comparison_is_advisory_and_handles_a_zero_baseline` | boundary: the advisory comparison prints ratios, says `not run`, and never divides by a zero baseline |
| `edge_table_truncates_long_answers_by_characters` | unicode / boundary: a long CJK answer is shortened without splitting a character, while an answer of exactly the column width is left intact |
| `edge_baseline_parser_ignores_header_separator_and_prose` | empty / malformed input: a baseline containing only a header, separator, and prose parses to zero rows instead of inventing data |
| `edge_check_size_supported_rejects_negative_and_unknown_names` | failure channel: a negative size and an unknown workload are both refused with a usage error that names the bad input and lists all four workloads |
| `edge_timings_path_stays_beside_a_file_or_inside_a_directory` | resource / path: the timing artifact is written beside a file baseline and inside a directory baseline, never outside the sandbox |
| `edge_a_size_past_the_maximum_is_refused_before_a_child_starts` | out of bounds / resource limit: `max_size + 1` is refused with `SizeTooLarge` and the reported workload, size, and maximum are exact |
| `edge_single_workload_measurement_does_not_write_the_baseline` | separation of concerns: measuring one workload prints a table but writes neither the injected path, the default `bench_md_path()`, nor either timing artifact |
| `edge_usage_names_every_workload_and_the_compare_mode` | malformed input / documentation contract: the usage text names all four workloads, `--compare`, `BENCH.md`, and the no-argument behavior |
| `edge_correctness_gate_does_not_rewrite_the_checked_in_baseline` | separation of concerns: running every workload inside the gate leaves the checked-in baseline byte-for-byte unchanged |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 2728 passed, 0 failed (the review fixes keep the round-1 worker-protocol/child-environment tests, compiled into both the integration-test and `rb-bench` unit-test targets; the round-2 fixes make the boundary, hung-child, environment, and cwd-path tests deterministic) |
| `../rbops/verify.sh phase-073` | pass — `VERIFY PASS — phase-073`, 2728 tests, 6 rust / 6 edge / 4 failure-asserting, examples green |

## Test requirement matrix

| Row | Result |
|---|---|
| empty | covered — empty workload name, zero-sized workloads, empty concat text |
| singleton | covered — fib(1), primes below 2, one concat iteration, one lex statement |
| boundary | covered — fib(20)=6765, primes below 1000=168, exactly 200 concat iterations, and an answer of exactly the table column width |
| out_of_bounds | N/A — the harness indexes no user collection; sizes are the only index-like boundary and are checked |
| type_mismatch | covered — non-numeric size text is refused |
| numeric_boundary | covered — zero and negative sizes; no benchmark asserts floating-point equality beyond the exact integer answers. Wall time is recorded and never asserted |
| unicode | covered — a long CJK answer is truncated by characters, not bytes, so the table cannot split a multi-byte character |
| nesting_recursion | N/A — all four workloads are iterative; no recursive call depth is exercised |
| duplicate_missing_keys | N/A — workloads use no records |
| malformed_input | covered — empty workload name, non-numeric size, negative size, a header-only baseline that must parse as zero rows, every malformed worker-protocol line, and a misspelled baseline workload even when its size is malformed too |
| resource_limit | covered — 60-second budget with a deliberately hung workload; production measurements run in a child process that is killed at the deadline and the hidden sleeping worker proves it never reaches its finish marker, while the test-only synthetic helper detaches its short closure (see known gaps) |

## Invariants touched

- None. No language syntax, `Value` variant, error kind, example, or module changed.
- The measuring harness is a separate `rb-bench` binary; the test gate only sees the correctness tests.

## Known gaps / follow-ups

- A pre-existing language bug was found while building the workloads: `give back` inside an `if`/`while` block does not return early. The interpreter arm at `src/interpreter.rs:2253` evaluates the value and continues the enclosing block. Reproduced with `to f(n) ... if n < 2 then give back 10 end give back 20 end; say f(1)` printing `20`. Recorded in `FINDINGS.md`; fixing it is out of scope for this test-only phase.
- `BENCH.md` is a debug-build baseline produced by `cargo run --bin rb-bench`. It records the wall time measured on the landing machine so a later phase can detect a regression, but no test asserts that number: only the verified answer is checked. Release numbers are not recorded yet.
- The resumed attempt corrected the checked-in baseline to record wall time, as the phase goal requires, and made the contract single-sourced: `BENCH.md` is the tracked landing-machine baseline, `BENCH_TIMINGS.md` is the untracked per-run record, and both say so. `rb-bench --compare BENCH.md` re-runs the recorded rows and prints the current wall time next to the baseline; its ratios are advisory, printed only, and never a gate. Timings remain comparable only within the same machine and build, so cross-machine runs should compare the ratio between workloads rather than absolute milliseconds.
- The reviewer's environment-coupling finding is fixed by resolving the test baseline from `CARGO_MANIFEST_DIR` (`checked_in_baseline_path`), so `RB_BENCH_OUTPUT` and the process cwd cannot make a test validate a foreign artifact. The runner keeps its documented cwd/override behavior.
- The no-argument runner writes the untracked timing artifact before the tracked baseline, so a failure on the second write leaves `BENCH.md` untouched. The hardware probe is platform-aware (Linux `/proc/cpuinfo`, macOS `sysctl`, Windows `PROCESSOR_IDENTIFIER`) and says `unknown CPU (<os> <arch> does not expose a model name)` when none is available.
- Fibonacci's checkable maximum is 78: that is the last size whose reference answer is still exactly representable in the interpreter's `f64` numbers. Larger sizes are refused with `SizeTooLarge` before the expected answer can round or reach infinity and pass `inf == inf`.
- The 60-second budget bounds how long the harness waits. `run_all` spends that one budget across all four workloads rather than granting each a fresh 60 seconds. Production measurements now run in a child process, so a timed-out workload is killed and reaped at the deadline instead of being detached. The timeout branch is exercised by a hidden sleeping worker driven through `run_workload`: the test proves the child started, returns `BudgetExceeded`, and finds no finish marker after the child's own sleep would have ended. The `run_with_budget` thread helper remains only for synthetic test closures (a deliberate hang, a load error, a lost worker); those closures are short and bounded by the test, not by a real benchmark.
- The full gate was run without any index workaround during this resumed attempt. The prior attempt's `GIT_INDEX_FILE` bullet is no longer applicable: the resumed change modifies tracked files under `tests/`, so the diff, must_touch and test-policy checks see it directly.
- The second review round added a direct `--compare` CLI test, made the comparison loop spend one shared budget instead of a fresh 60 seconds per row, made the baseline parser refuse unknown-workload rows with a numeric size and duplicate `(workload, size)` rows, and kept the timing artifact inside the output directory when the output path is a directory. The write-failure test now proves the timing write stayed in its sandbox and the failure came from the baseline path.
- This resumed run started from a preserved harness already in `HEAD`, so `verify.sh` saw an empty diff and failed its must_touch and test-quota checks. Four new tests now pin previously untested branches (character-safe truncation, header-only baseline parsing, negative/unknown size-check refusal, and timing-artifact placement) and supply the required `edge_*` and failure-asserting coverage. The same run moved production measurement into a killable child process and tightened the truncation test to assert the exact 20-character cell. No language production code changed.
- The final resume found the whole preserved harness committed in `HEAD`, so every code gate passed while `verify.sh` failed only `no changed files detected`, `diff too small`, `must_touch`, and the test-policy checks. The fix was not to reimplement the harness but to add the four tests above on top of `HEAD`: they pin the size-boundary refusal, prove the single-workload and gate paths never write the baseline, and pin the user-facing usage contract. This produces the required diff under `tests/` without weakening or removing any existing test.
- The review round for this attempt closed the remaining verification gaps: the hidden sleeping worker drives the real child timeout/kill branch and proves no child survives, the worker protocol parser is unit-tested on every malformed/`ERR`/non-UTF-8 line, the read-error branch now kills the child before reaping it, the `DEFAULT_BUDGET` comment describes the child-process design, and `--compare` refuses a baseline that omits any of the four workloads (and the parser refuses a misspelled workload even when its size cell is malformed too). The gate counts in this report are the observed `cargo test --all-targets` and `verify.sh` output, not a decomposition.
- The phase-073 review fixes are in this attempt. The hung-child test no longer mutates the process environment with `set_var`/`remove_var`: `measure_in_child` accepts an explicit environment slice and the test passes the sentinel directly. The child writes a start marker and the parent waits for that handshake before its 500 ms budget begins, so process spawn and dynamic linking cannot consume the budget. The round-2 review removed the remaining wall-clock margin: the child sleeps ten seconds, and the test asserts only that the parent returned `BudgetExceeded` and that no finish marker exists, with no fixed post-return sleep. The size-boundary test now pins each maximum through `check_size_supported` (which does not run the workload) and runs only size 1 end-to-end, so a slower gate machine cannot turn a correctness test into a timeout. The explicit-environment test now drives the hidden worker with a non-empty sentinel and asserts the child wrote both markers, so it fails if `measure_in_child` ignores its `env` argument. The cwd baseline test no longer skips when `RB_BENCH_OUTPUT` is set; it drives the injected accessor directly. The single-workload no-write test snapshots the default `bench_md_path()` and its timing artifact as well as the injected temp path, so a regression that wrote cwd `BENCH.md` would fail it. The worker protocol has a structured `WRONG <expected-hex> <actual-hex>` line; the child emits it for `WrongAnswer`, `measure_in_child` parses before the exit-status check, and a unit test round-trips encoder and parser, so the distinct variant is reachable on the shipped path rather than being flattened to `Load`.
