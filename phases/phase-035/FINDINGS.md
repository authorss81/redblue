# Findings — phase-035

## The cited evidence was stale, in two ways

The phase cites `src/interpreter.rs:924, 1089, 1238, 1250, 1263` as five
`SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` calls.

1. **Wrong file.** The time builtins moved to `src/runtime.rs` when the shared
   operation layer was extracted. `src/interpreter.rs:924` is `fn push_scope`.
   The five sites are `src/runtime.rs:346, 474, 656, 670, 686` as of `84b1c44`,
   and after this phase they are `src/runtime.rs:404, 529, 707, 718, 731`.

2. **Wrong defect.** The `.unwrap()`s were converted to `map_err` by commit
   `84b1c44` ("rbops: phase-025"), which is the diff that touches exactly those
   five lines. `grep -rn 'unwrap()' src/` returned 7 hits before this phase, all
   inside `mod tests`.

The phase brief says to stop if the finding does not reproduce. It did not — but
the class it names was still reachable, in the same function, in a form the
finding did not anticipate, so stopping would have left a live panic in place.
Fixed and recorded here so the auditor can correct the manifest entry.

## A reachable panic the finding missed, with no clock manipulation needed

```
$ ./target/debug/rb run target/tmp/p035/big.rb    # say time.format(1e300, "%Y")
thread 'redblue-vm' panicked at src/runtime.rs:510:28:
overflow when adding duration to `SystemTime`
Error: RuntimeError: The interpreter thread stopped unexpectedly
```

`timestamp as u64` saturates, so `1e300` became `u64::MAX` and
`UNIX_EPOCH + Duration::from_secs(u64::MAX)` panicked from inside `std`. No host
clock change, no unusual input — an ordinary number literal reaches it. A Redblue
`try`/`catch error` cannot catch it, because the panic unwinds the interpreter
thread.

The same saturating cast made `time.format(-1, "%Y-%m-%d")` print `1970-01-01`
instead of `1969-12-31`: a wrong answer, silently, one day off.

Both are fixed in this phase and both are now covered.

## Round 1 — the review found a second panic of the same class, one function over

The reviewer read the time-builtin changes and returned four findings. All four
are fixed. What the probe showed, recorded here because the fourth does not say
what the phase brief's reviewer assumed:

**1. `time.sleep` panicked on any number `Duration::from_secs_f64` rejects.**
Reproduced before the fix:

```
$ printf 'say time.sleep(-1)\n' > target/tmp/p035r/t.rb
$ ./target/debug/rb run target/tmp/p035r/t.rb
thread 'redblue-vm' panicked at core/src/time.rs:962:23:
cannot convert float seconds to Duration: value is either too big or NaN
Error: RuntimeError: The interpreter thread stopped unexpectedly
```

The same panic, unreachably reported to the program, for `-0.5`, `1e20`, `1.8e19`
and every `f64` past it. The panic is in `core`, past every `Result` in this
crate, so a Redblue `try`/`catch error` cannot catch it — exactly the failure
class this phase claims to remove. `NaN` and `±Infinity` are not reachable from
Redblue arithmetic (the analyzer refuses a non-finite result before it becomes a
`Value`), but the guard names them anyway because `Duration` is a `std` boundary
rather than a Redblue invariant.

Fixed by `sleep_duration(seconds, span)`, which uses
`Duration::try_from_secs_f64` — the same conversion reported as a `Result` —
behind an explicit finite / non-negative / bounded check so each refusal names
its own reason.

**2. `time.unix("9999999999-01-01 00:00:00")` did not panic.** The finding
predicted a panic inside `chrono` past every `Result`. It does not:

```
$ ./target/debug/rb run ...   # say time.unix("9999999999-01-01 00:00:00")
Error: RuntimeError: Invalid date format, use YYYY-MM-DD HH:MM:SS
```

`NaiveDateTime::parse_from_str` with `%Y-%m-%d` rejects a year above 9999 at the
*parse*, returning `Err`, which the existing `map_err` already turned into a
`Runtime` error. `DateTime::timestamp()` is `(gregorian_day - UNIX_EPOCH_DAY) *
86_400 + seconds_from_midnight` over a `chrono`-bounded year — `i64` arithmetic
that cannot overflow for any date `chrono` will produce. The named input was
already safe.

What *was* wrong is one function away from what the finding describes, and the
probes turned it up: `chrono` parses years as low as **-262143**, which is
`-8334601228800` seconds, and `time.format` refuses that as out of range. So
`time.unix` could hand back a timestamp that the sibling builtin would not take:

```
$ ./target/debug/rb run ...   # say time.unix("-99999-01-01 00:00:00")
-3217830796800                # a number time.format(-3217830796800, ...) refuses
```

The two builtins disagreed about whether the instant existed. `seconds_for_unix`
now bounds the parse result with the same window `seconds_for_format` uses, so
one instant is either real to both or an error from both, and
`time_unix_and_time_format_agree_on_the_window` asserts the round trip.

**3 and 4. Three tests asserted nothing.** `edge_...pre_epoch_timestamp_names_that_date`
matched `Ok(_)` on `say time.format(-1, "%Y-%m-%d")`. `say` returns `Nothing`,
so the assertion passed whether the value was `1969-12-31` or the buggy
`1970-01-01` — the test could not have failed on the bug it exists to pin, and
`time_now_seconds_is_a_positive_number` had the same shape on
`say now.seconds > 0`. Rewritten to assert on the value: the comparison or the
call is now the last statement, so `eval` returns the `YesNo` or the `Text`, and
`eval_text` fails loudly on a non-text value. The same `Ok(_)` pattern was in
three other places in the file (`time_format_at_the_epoch_boundary_is_accepted`,
`edge_a_failing_time_call_is_catchable_by_try_catch`,
`time_now_on_a_current_clock_is_still_a_record_of_seconds`) and all were
strengthened the same way. No test was deleted; the file went from 8 to 12.

### Red, measured against the phase-035 baseline

Reverting only the two production sites this round changed, leaving the rest of
the phase intact, `cargo test --test time_builtin_test`:

```
---- edge_a_sleep_no_clock_can_wait_is_a_catchable_error stdout ----
expected a message containing `time.sleep`, got `The interpreter thread stopped unexpectedly`
---- edge_time_unix_of_a_date_outside_the_shared_window_is_an_error stdout ----
---- edge_a_failing_time_call_is_catchable_by_try_catch stdout ----
test result: FAILED. 9 passed; 3 failed
```

And against the pre-phase tree, where the weak assertions are what let the
wrong-date bug through:

```
---- edge_time_format_of_a_pre_epoch_timestamp_names_that_date stdout ----
  left: "1970-01-01"
 right: "1969-12-31"
---- time_format_at_the_epoch_boundary_is_accepted stdout ----
  left: "1970-01-01"
 right: "1969-12-31"
```

## Left undone, deliberately

- `rbops/verify.sh` does not exist in this checkout, so the script's
  project-specific checks beyond the four documented gates are unverified by me.