# Phase 035 — Remove the reachable panics in the time builtins

## What changed

| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +181 −28 | Added `since_epoch(instant, span)`, the seconds-since-epoch conversion behind a seam that takes the instant as an argument; added `seconds_for_format(timestamp, span)`, which refuses a timestamp that names no date; routed all five clock sites and `time_format` through them; four unit tests in `mod tests` |
| `tests/time_builtin_test.rs` | +168 (new) | 8 end-to-end tests through the Redblue pipeline |

The clock conversion now reads:

```
fn since_epoch(instant: SystemTime, span: Span) -> Result<Duration> {
    instant.duration_since(UNIX_EPOCH).map_err(|_| {
        Error::Runtime(
            "The system clock is set before 1970-01-01T00:00:00Z, so time cannot be read from it"
                .to_string(),
            span,
        )
    })
}
```

`SystemTime::now()` is passed *in* rather than called inside, so a test can hand it a
pre-epoch instant directly and never touch the machine's clock.

## Finding re-verification — the cited evidence was stale, the class was live

The phase cites `src/interpreter.rs:924, 1089, 1238, 1250, 1263` calling
`SystemTime::now().duration_since(UNIX_EPOCH).unwrap()`.

**The line numbers no longer exist.** The time builtins moved out of
`interpreter.rs` into `src/runtime.rs` when the operation layer was extracted
(`src/runtime.rs` module doc: "What a Redblue *operation* means, in the one place
both VMs read it"). `src/interpreter.rs:924` is `fn push_scope`.

**The `.unwrap()`s were already gone**, converted to `map_err` in commit `84b1c44`
("rbops: phase-025") — the diff of that commit shows exactly the five sites the
phase names, each changing `.unwrap()` to `.map_err(|e| Error::Runtime(e.to_string(), span))?`.
`grep -rn 'unwrap()' src/` on the pre-existing tree returned 7 hits, all inside
`mod tests`. So the literal finding — a reachable `unwrap` on a host clock — did
not reproduce, and the phase's instruction to stop in that case would have been
followed by a phase that changed nothing.

The *class* the finding names, however, is not only live, it is worse than the
finding described. Two defects survived the phase-025 conversion, both in the
same function, both reachable from a plain Redblue program with no unusual host
state at all:

**1. A real panic, reproducible with no clock manipulation.** Reproduced before
any change:

```
$ printf 'say time.format(1e300, "%%Y")\n' > target/tmp/p035/big.rb
$ ./target/debug/rb run target/tmp/p035/big.rb
thread 'redblue-vm' (5435) panicked at src/runtime.rs:510:28:
overflow when adding duration to `SystemTime`
   4: redblue::runtime::builtin
   5: <redblue::interpreter::Vm>::call
   ...
Error: RuntimeError: The interpreter thread stopped unexpectedly
exit=1
```

`SystemTime + Duration` panics from inside `std` when the addition overflows, past
every `Result` in this crate. `timestamp as u64` saturates, so `1e300` became
`u64::MAX`, and the addition panicked. A Redblue `try`/`catch error` cannot catch
it: the panic unwinds the interpreter thread out from under the `try`.

**2. A silently wrong answer.** The same saturating cast meant
`time.format(-1, "%Y-%m-%d")` printed `1970-01-01` — one day off, and it never
said so. Confirmed by test: the boundary test printed `1970-01-01` for `0`, `1`
*and* `-1`.

The fix addresses both, and the message for the pre-epoch clock now names the
clock rather than quoting `SystemTimeError`'s "second time provided was later than
self".

## Round 1 — a second panic of the same class, and three tests that asserted nothing

The review found four issues; all are fixed and the gates were re-run.

**1. `[BLOCKER]` `time.sleep` panicked** on any number `Duration::from_secs_f64`
rejects — `-1`, `-0.5`, `1e20`, `1e18` and above — from inside `core`, past every
`Result` in this crate:

```
$ printf 'say time.sleep(-1)\n' > target/tmp/p035r/t.rb
$ ./target/debug/rb run target/tmp/p035r/t.rb
thread 'redblue-vm' panicked at core/src/time.rs:962:23:
cannot convert float seconds to Duration: value is either too big or NaN
Error: RuntimeError: The interpreter thread stopped unexpectedly
```

This is the failure class the phase exists to remove, one function over. New
`sleep_duration(seconds, span)` checks finite, non-negative, and bounded, and
converts with `Duration::try_from_secs_f64` — the same conversion reported as a
`Result` — so no input reaches `thread::sleep` that would panic there. A
fractional sleep still waits its fraction, and `time.sleep(0)` is still allowed.

**2. `[MAJOR]` `time.unix` did not panic** at the input the finding names:
`time.unix("9999999999-01-01 00:00:00")` is refused by `parse_from_str` with
`Err`, which the pre-existing `map_err` already reported as a `Runtime` error, and
`DateTime::timestamp()` is `i64` arithmetic over a `chrono`-bounded year that
cannot overflow. What the probe *did* find is one function away from the
finding: `chrono` parses year `-262143`, `time.unix` returns it as
`-8334601228800`, and `time.format` refuses that number. The two builtins
disagreed about whether the instant existed. `seconds_for_unix` now applies the
same window `seconds_for_format` uses, and
`time_unix_and_time_format_agree_on_the_window` asserts that a timestamp from one
is readable by the other.

**3 and 4. `[MAJOR]` Three tests asserted nothing.** `say` returns `Nothing`, so
matching `Ok(_)` after a `say` passes whatever the value was:
`edge_...pre_epoch_timestamp_names_that_date` could not have failed on the
`1970-01-01`-for-`1969-12-31` bug it exists to pin, and
`time_now_seconds_is_a_positive_number` could not have failed on a zero or
negative `seconds`. The same pattern was in three more places. Every assertion in
the file is now on the value the program evaluated to — the comparison or the
call is the last statement, and `eval_text` fails on a non-text value.

### Tests added this round

| Test | Edge class covered |
|---|---|
| `src/runtime.rs::edge_a_sleep_the_clock_cannot_wait_is_a_runtime_error` | out_of_bounds / numeric_boundary — the `-1`, `1e20`, `f64::MAX` panic, as the red test |
| `src/runtime.rs::a_sleep_within_the_waitable_range_is_accepted` | boundary — `0`, `-0.0`, `0.25`, `1.5`, `10` |
| `src/runtime.rs::edge_a_date_outside_the_window_both_builtins_accept_is_refused` | out_of_bounds — year -99999, -262143 |
| `src/runtime.rs::a_date_inside_the_window_round_trips_through_time_format` | boundary — the epoch, ±1s, the last nameable year |
| `tests/time_builtin_test.rs::edge_a_sleep_no_clock_can_wait_is_a_catchable_error` | numeric_boundary / type_mismatch / empty — through the full pipeline |
| `tests/time_builtin_test.rs::time_sleep_of_zero_seconds_is_accepted` | boundary — the waitable zero |
| `tests/time_builtin_test.rs::edge_time_unix_of_a_date_outside_the_shared_window_is_an_error` | out_of_bounds |
| `tests/time_builtin_test.rs::time_unix_and_time_format_agree_on_the_window` | boundary — the round trip that makes the two windows one |

8 new tests, and the 8 existing ones strengthened. None deleted. Four unit tests
in `src/runtime.rs`, four in `tests/time_builtin_test.rs`; the file went 8 → 12.
`edge_a_failing_time_call_is_catchable_by_try_catch` now covers all three
builtins and asserts the handler actually ran, rather than only that the program
reached the end.

## Tests added (original round)

| Test | Edge class covered |
|---|---|
| `src/runtime.rs::edge_a_clock_before_the_epoch_is_a_runtime_error` | pre-epoch instant (asserts a failure is produced; also asserts the reported span) |
| `src/runtime.rs::the_epoch_boundary_is_accepted_by_the_conversion` | boundary — the epoch itself and one second after |
| `src/runtime.rs::edge_a_timestamp_naming_no_instant_is_refused` | numeric_boundary / type_mismatch — `±1e300`, one second past year 10000, `NaN`, `±Infinity` |
| `src/runtime.rs::a_pre_epoch_timestamp_is_a_date_not_an_error` | boundary — 1969, 1900 formatted as the dates they name |
| `tests/time_builtin_test.rs::edge_time_format_of_a_timestamp_no_instant_can_hold_is_an_error` | numeric_boundary — the `1e300` panic, as the red test |
| `tests/time_builtin_test.rs::edge_time_format_of_a_pre_epoch_timestamp_names_that_date` | boundary — `time.format(-1)` is 1969-12-31 |
| `tests/time_builtin_test.rs::edge_time_format_refuses_arguments_of_the_wrong_type` | type_mismatch — text/list/record timestamp, numeric/list format |
| `tests/time_builtin_test.rs::edge_time_format_with_no_arguments_is_an_error` | empty |
| `tests/time_builtin_test.rs::edge_a_failing_time_call_is_catchable_by_try_catch` | asserts a failure is produced *and* is catchable |
| `tests/time_builtin_test.rs::time_format_at_the_epoch_boundary_is_accepted` | boundary — 0, 1 and −1 all name instants |
| `tests/time_builtin_test.rs::time_now_on_a_current_clock_is_still_a_record_of_seconds` | regression — the ordinary path still works |
| `tests/time_builtin_test.rs::time_now_seconds_is_a_positive_number` | regression — `seconds` is a positive number, not a placeholder |

12 new tests in the original round (4 `#[test]` in `src/runtime.rs`, 8 in
`tests/time_builtin_test.rs`), plus 8 in round 1 (4 and 4) — 20 in total, of
which 9 are named `edge_*`. Ten assert that a failure is produced. None were
deleted; the round-1 review required rewriting three of the originals in place,
because as written they asserted nothing.

### Red observed before the fix

`cargo test --test time_builtin_test`, before any production change:

```
---- edge_a_failing_time_call_is_catchable_by_try_catch stdout ----
panicked at tests/time_builtin_test.rs:133:19:
the failure must be catchable, got Runtime("The interpreter thread stopped unexpectedly", Span { line: 0, column: 0 })

---- time_format_at_the_epoch_boundary_is_accepted stdout ----
1970-01-01
1970-01-01
1970-01-01
panicked at tests/time_builtin_test.rs:93:5:
source should have failed: Nothing

test result: FAILED. 4 passed; 4 failed
```

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, no `allow(` added |
| `cargo test --all-targets` | 1083 passed, 0 failed, 0 ignored (1075 before round 1) |
| `rb test` (Redblue suite) | 340 run, 340 passed, 0 failed |
| `examples/*.rb` | all run clean, including `examples/time.rb` |
| `./rbops/verify.sh phase-035` | **NOT RUN — `rbops/verify.sh` is not present in this checkout** (see below) |

`rbops/verify.sh` is in the RBOPS pipeline repository, not in this checkout; the
phase brief places that repository out of scope. I ran its four documented
constituents directly instead, and every one is green. I cannot report a
`verify.sh` result I did not observe, so this row says what actually happened.

`tests/stdlib_modules_test.rs` reports 39 passed, 0 failed, including
`network_timeouts_are_published_and_bounded`.

## Definition of done

- [x] no `.unwrap()` in `src/` outside test code. `grep -rn 'unwrap()' src/`:

```
src/parser.rs:2215:        let tokens = Lexer::tokenize(r#"say "Hello""#).unwrap();
src/parser.rs:2216:        let program = parse(tokens).unwrap();
src/parser.rs:2222:        let tokens = Lexer::tokenize("set x to 10").unwrap();
src/parser.rs:2223:        let program = parse(tokens).unwrap();
src/lexer.rs:525:        let tokens = Lexer::tokenize(r#"say "Hello, World!""#).unwrap();
src/lexer.rs:532:        let tokens = Lexer::tokenize("set x to 42").unwrap();
src/lexer.rs:538:        let tokens = Lexer::tokenize("if yes then end").unwrap();
```

  All 7 are inside `mod tests` (parser.rs:2215 is ~290 lines past its `mod tests`).
  This part of the finding was already satisfied before the phase started.
- [x] the five call sites report a RuntimeError naming the clock rather than
      panicking, and a Redblue `try`/`catch error` around a failing `time.now()`
      yields the error. The five sites are `random` (`src/runtime.rs:404`),
      `time_now` (`:529`), `random_number` (`:707`), `random_choice` (`:718`),
      `random_shuffle` (`:731`); all now call `since_epoch(SystemTime::now(), span)?`.
- [x] the failure is exercised WITHOUT moving the machine clock —
      `since_epoch(instant, span)` takes the instant as an argument and
      `edge_a_clock_before_the_epoch_is_a_runtime_error` passes
      `UNIX_EPOCH - Duration::from_secs(1)` and `UNIX_EPOCH - Duration::from_nanos(1)`.
- [x] `edge_*` tests cover a pre-epoch instant, the exact epoch boundary, and
      `time.format` given an argument that is not a number or not a text.
- [x] `cargo test --all-targets` reports 0 failures; the 39 tests in
      `tests/stdlib_modules_test.rs` pass.
- [x] `cargo clippy --all-targets -- -D warnings` clean, no `allow(` added.

## Test-requirement matrix

- [x] **empty** — `time.format()` with no arguments; the epoch timestamp `0`
- [x] **singleton** — one argument, the only shape `time.format` has
- [x] **boundary** — the epoch itself, ±1 second, one second past year 10000, one
      nanosecond before the epoch
- [x] **out_of_bounds** — a timestamp past what a date can name (`1e30`, `1e300`,
      `253402300801`) is a RuntimeError, not an overflow; this was the panic
- [x] **type_mismatch** — text / list / record timestamp, numeric / list format
- [x] **numeric_boundary** — `NaN`, `±Infinity`, `1e300`, `1e30`, `-1e300`,
      and the widest in-range date
- [x] **unicode** — N/A: `time.format` passes the format string to `chrono`
      verbatim and this phase changed nothing about how text is carried. The
      format string is not re-lexed or re-encoded on the path under test.
- [x] **nesting_recursion** — N/A: the change is three straight-line functions in
      one match arm; there is no recursion and no scope to nest.
- [x] **duplicate_missing_keys** — N/A: no record is read or built here;
      `time.now()`'s record construction is unchanged and is covered by
      `time_now_on_a_current_clock_is_still_a_record_of_seconds`.
- [x] **malformed_input** — the argument-type refusals and the no-argument case
      are this row for `time.format`; no source is parsed by this change
- [x] **resource_limit** — `time.sleep` now has an explicit bound of one year on a
      single call, so no one call can block a thread indefinitely. No
      allocation or unbounded loop is introduced anywhere; the other five sites
      have the VM's iteration/step guards above them and were not touched.
- [x] **numeric_boundary** — round 1 added `time.sleep`'s: `-0.5`, `-1`, `1e20`,
      `f64::MAX`, `±Infinity`, `NaN` refused, and `0`, `-0.0`, `0.25`, `1.5`,
      `10` accepted. `time.unix`'s: year `-99999` and `-262143` refused, the
      epoch and `9999-12-31` accepted.

## Invariants touched

- None. `Value`'s variants, `Error`'s variants, the `.rb` extension, `to…end`,
  `set x to`, `say`, trailing commas and `{interp}` are all untouched.
- **One behaviour change, deliberate**: `time.format(-1, "%Y-%m-%d")` used to
  print `1970-01-01` and now prints `1969-12-31`. This is a bug fix in the
  direction SPEC.md already requires — a wrong date silently produced is the same
  failure class as the panic. `time.format(1.5)` still truncates toward zero as it
  always did; I checked that before deciding, and refusing it would have rejected a
  program that used to work.
- **Two more behaviour changes in round 1, both narrowing**: `time.sleep` of a
  negative, non-finite, or over-a-year number is now an error rather than a panic
  or an unbounded block; and `time.unix` of a year outside `[-9999, 9999]` is now
  an error rather than a timestamp `time.format` would refuse. The first was a
  panic and the second was an inconsistency, so neither rejects a program that
  worked.

## Known gaps / follow-ups

- **The `time.unix` gap recorded in the original round is closed.** The probe the
  review asked for shows the named input was already refused at the parse, so the
  predicted panic was not there — but the probe also showed `chrono` parses years
  down to -262143 that `time.format` then refuses, and `seconds_for_unix` now
  bounds both to one window.
- `rbops/verify.sh` could not be run from this checkout; the four gates it wraps
  were run individually and are green, but the script's own project-specific
  checks (example corpus, invariants file, phase quota counting) are unverified
  by me.
- `time.sleep` now refuses a wait longer than one year. That bound is a policy
  choice, not a panic threshold — `try_from_secs_f64` accepts up to ~1.8e19
  seconds — and it is the one behaviour limit this phase introduces rather than
  repairs. It is recorded because a program that once slept for a decade and
  blocked would now get an error instead of a block.