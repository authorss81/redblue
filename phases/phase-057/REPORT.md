# Phase 057 — Let the REPL measure code: `.time` and `.bench`

## Reproduce (before the change)

```
$ printf 'set x to 3\n.time 1 + 1\n.bench say 1\n' | ./target/debug/rb
>>> Unknown command 'time'. Type :help for available commands.
>>> Unknown command 'bench'. Type :help for available commands.
```

`COMMAND_TABLE` in `src/repl/commands.rs` had no timing word; the only ways to
time anything at the prompt were to shell out to `time` or to hand-roll a loop
over `time.now()`, which reads the wall clock a test may not.

## What changed

| File | Lines | What |
|---|---|---|
| `src/repl/measure.rs` | +320 (new) | The arithmetic and wording of a measurement: run-count parsing, min/median/max/mean, unit selection, report lines. No program running in here. |
| `src/repl/mod.rs` | +415 | `Time`/`Bench` dispatch, `time_answer`/`bench_answer`, `time_runs` (runs the program in the session's own VM), and the `measure` test module. |
| `src/repl/commands.rs` | +25 | `ReplCommand::{Time, Bench}`, two `CommandSpec` rows (`<expression> [n times]`), and `argument_text` covering both. |
| `tests/repl_color_test.rs` | +24 | Drive-by flake fix, flagged below — no assertion changed. |

### The surface

```
>>> .time 1 + 1
time: `1 + 1`, 10 run(s), wall time (figures per run)
  total: 9183.000 ns
  mean:  918.300 ns

>>> .bench say 1
bench: `say 1`, 100 run(s), wall time (figures per run)
  min:   203.000 ns
  median: 208.000 ns
  max:   4718.000 ns
  mean:  256.040 ns
```

* Defaults: `.time` 10 runs, `.bench` 100. Either can be told otherwise by a
  trailing `<n> times` — the language's own shape for the same thing
  (`repeat 3 times`). `1 + 1` therefore stays one expression and is not read as
  `1 +` run once, and a count is never taken out of an expression by accident.
* **The run count is only read from that marked tail**, and only from a bare
  word: `many times` is a mistyped count and is reported as one, while
  `countdown(3) times` is a call with an argument named `times` and stays one.
* Timed code runs in **this session's VM**: `.time x + 1` reads a binding the
  session already holds, and what the timed program binds is still there
  afterwards. `Repl::evaluate_for_question` forks for the opposite reason (a
  question must change nothing), which is the wrong trade for a measurement.
* Output of a timed run is drained every run — `.bench say 1` is a thing a user
  will type, and a measurement is not a request to hear its result 100 times.
* A failing expression is reported **once, by name**, and the loop stops. The
  analysis happens before the clock starts, so an unrunnable program is refused
  by the language's own error rather than N times inside a loop.
* Refusals name the command: `Error: 'bench' takes a whole number of runs:
  'many' is not a number.`, `Error: 'time' cannot run 0 time(s): there is no run
  to measure.`, `Error: 'bench' will not run more than 10000 times; 100000000
  was asked for.`
* One unit per report, chosen from the **mean** (`.time`) and the **min**
  (`.bench`), so no reported figure can print as zero.

## Tests added

| Test | Edge class covered |
|---|---|
| `repl::tests::measure::time_reports_a_wall_time_total_and_a_mean_over_ten_runs` | numeric boundary — total and mean finite, positive, one unit, `mean × runs == total` |
| `repl::tests::measure::bench_reports_min_median_max_and_mean_over_a_hundred_runs` | singleton set of figures — 100 runs, all four labelled, `min <= median <= max`, mean inside the range |
| `repl::tests::measure::timed_code_runs_in_the_session_it_was_typed_in` | session state — reads a session binding, and 5 runs of `set hits to hits + 1` leave 5 |
| `repl::tests::measure::edge_a_failing_expression_reports_its_error_once_and_runs_once` | resource/state — a failing timed expression runs once (counter == 1) and reports its error, through both `time_answer` and the dispatched `.time` command |
| `repl::tests::measure::edge_time_with_no_argument_is_refused_before_anything_runs` | empty — `.time`/`.bench` with no argument is `MissingArgument` under both sigils, and carries the whole argument when given one |
| `repl::tests::measure::edge_a_run_count_that_is_not_a_positive_number_is_refused_by_name` | numeric boundary — `many` refused and quoted, `0` and `-4` refused rather than divided by, a count past `MAX_RUNS` refused |
| `repl::tests::measure::edge_a_broken_expression_is_the_language_error_and_unicode_measures_fine` | malformed input + unicode — `1 +` gives the parser's error; `"héllo " + "🙂"` is timed like any other |
| `repl::measure::tests::edge_a_count_is_only_read_from_the_tail_the_user_marked` | boundary — the count tail vs. `countdown(3) times` |
| `repl::measure::tests::edge_zero_and_a_count_that_is_not_a_number_are_refused_by_name` | numeric boundary — zero, non-numeric, past-the-cap |
| `repl::measure::tests::bench_orders_the_figures_it_reports` | statistics — even count takes the mean of the two middle samples; min/median/max/mean exactly |
| `repl::measure::tests::time_reports_a_total_that_is_the_mean_over_the_runs` | statistics — unit chosen so the mean is ≥ 1 unit |

11 new `#[test]` functions, 6 named `edge_*`, 4 asserting a failure is
produced. No test was weakened, skipped, ignored or suppressed.

## Edge-case matrix

| Row | Covered by |
|---|---|
| empty / zero / nothing | `edge_time_with_no_argument_is_refused_before_anything_runs`; a `0`-run request is refused by name |
| singleton and boundary | `bench_orders_the_figures_it_reports` (exact figures, even and odd counts); the run-count tail parser |
| out of bounds | `MAX_RUNS` cap refuses a 100000000-run request — N/A for list indexing, which is the VM's own guard (untouched by this phase) |
| type mismatch | N/A — the measured program goes through the session's ordinary analyzer/VM pipeline, which already owns that error (`Error: RuntimeError: Unknown function 'bump'` end-to-end); asserting it again here would be a duplicate of `tests/index_bounds_test.rs` |
| type coercion boundaries | N/A — this phase adds no arithmetic to the language; `0/0`, `1/0` etc. remain the VM's existing behaviour |
| unicode and escapes | `edge_a_broken_expression_is_the_language_error_and_unicode_measures_fine` |
| nesting and recursion | N/A — measurement does not transform the program; a recursive function is timed by the same loop as any other (nothing to assert separately) |
| duplicate / missing keys | N/A — records are untouched by this phase |
| malformed input | `edge_a_broken_expression_is_the_language_error_and_unicode_measures_fine` (`1 +`); an unknown function is a clean runtime error end-to-end |
| resource and state | run count capped at `MAX_RUNS` and refused by name; output buffer drained every run; `edge_a_failing_expression_reports_its_error_once_and_runs_once` |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 1720 passed, 0 failed |
| `cargo test --doc` | 2 passed, 0 failed |
| `./rbops/verify.sh phase-057` | **not run — `rbops/` is not in this checkout** (see below) |

`rbops/` and `rbops/verify.sh` are not present in this working tree
(`ls rbops` → *No such file or directory*). I did not modify, inspect or work
around them. The other three gates were run here and their output is in
`target/tmp/test3.log`.

## Drive-by fix, flagged

`tests/repl_color_test.rs` failed once under the full suite during this phase:

```
test edge_banner_is_stable_across_calls ... FAILED
assertion `left == right` failed: the banner must not change between calls
  left:  "                       \u{1b}[1;31m███ …"   (centred)
 right: "\u{1b}[1;31m███ …"                          (not centred)
```

Cause: `edge_banner_is_stable_across_calls` and
`edge_a_narrow_terminal_centres_without_panicking` both `set_var("COLUMNS", …)`
in one process on parallel threads, and the banner reads `COLUMNS` from the
environment (`src/repl/banner.rs:97`). The test's own comment says so; it passed
12/12 in isolation and the clean tree passed once — it is a pre-existing race,
unrelated to this diff, which touches neither `COLUMNS` nor the banner.

Fix: a `static COLUMNS_LOCK: Mutex<()>` in that test file, taken by both tests.
No assertion, threshold, timeout or environment read changed; the environment is
still what the production code reads. Recorded in `FINDINGS.md`.

## Invariants touched

- None. No `Value` variant, `Error` variant, `.rb` extension, keyword, or
  `set x to <expr>` / `say` / `to … end` form was changed. `.time` and `.bench`
  are REPL commands behind the existing sigils (`:time` is the same command), and
  a language program that begins with `.`/`:` is still not a command.

## Known gaps / follow-ups

- `print` and `console.log` write at the point they are reached rather than into
  the VM's buffer, so `.bench print "x"` still writes 100 lines to the terminal.
  Suppressing them would mean refusing or swallowing a language effect; recorded
  in `FINDINGS.md` rather than done here.
- The figures are host wall time and are labelled as such; the clock is read
  with `Instant`, never through `time.now()`, so nothing a test needs to be
  deterministic is written into the language.
- The run count is capped at `MAX_RUNS` (10000) and refused by name past it.