//! The `time` module must be total: a host clock, a timestamp, or a number of
//! seconds that no `SystemTime` can represent is a `Runtime` error a Redblue
//! program can catch, not a panic that unwinds the interpreter thread.
//!
//! The seconds-since-epoch conversion lives behind a function that takes the
//! instant as an argument, so these tests name the instants themselves rather
//! than moving the machine's clock. Nothing here depends on wall-clock state.
//!
//! Two panics of that class lived here and both are reachable from a plain
//! program with no unusual host state: `time.format`'s `SystemTime` addition and
//! `time.sleep`'s `Duration::from_secs_f64`. Every assertion below is on the
//! value the program evaluated to, not merely on whether it ran — `say` returns
//! `Nothing`, so a test that matched `Ok(_)` after a `say` passes whatever the
//! value was, which is how the wrong-date bug this file covers survived a round
//! of review that was otherwise green.

use redblue::{Error, Value};

#[track_caller]
fn eval(source: &str) -> Result<Value, Error> {
    redblue::run_source_value(source)
}

#[track_caller]
fn eval_err(source: &str) -> Error {
    eval(source).expect_err("source should have failed")
}

/// Asserts `source` fails with a `Runtime` error whose message contains `part`.
#[track_caller]
fn assert_runtime_error(source: &str, part: &str) {
    match eval_err(source) {
        Error::Runtime(message, span) => {
            assert!(
                message.contains(part),
                "expected a message containing `{}`, got `{}`",
                part,
                message
            );
            assert!(span.is_known(), "failed without a source span");
        }
        other => panic!("expected a Runtime error, got {:?}", other),
    }
}

/// The text `source` evaluates to, which is what `say` would have printed.
///
/// `say` returns `Nothing`, so a test that evaluates `say <expr>` and matches
/// `Ok(_)` passes whether the expression printed the right date or the wrong
/// one — the value it computed is thrown away before anything is checked. Every
/// assertion here is on the last statement's value for that reason.
#[track_caller]
fn eval_text(source: &str) -> String {
    match eval(source) {
        Ok(Value::Text(text)) => text,
        Ok(other) => panic!("expected text from `{}`, got {:?}", source, other),
        Err(e) => panic!("`{}` should have produced text, got {:?}", source, e),
    }
}

/// A timestamp no `SystemTime` can hold must not panic.
///
/// `SystemTime + Duration` overflows for a duration anywhere near `u64::MAX`
/// seconds, and it does so by panicking from inside `std`, past every
/// `Result` in this crate. `time.format` has to notice the overflow itself.
/// The red test here was a panic at `src/runtime.rs:510` that unwound the
/// interpreter thread and replaced the program's error with "The interpreter
/// thread stopped unexpectedly".
#[test]
fn edge_time_format_of_a_timestamp_no_instant_can_hold_is_an_error() {
    for (timestamp, what) in [
        ("1e300", "a far future timestamp"),
        ("1e30", "a less extreme future timestamp"),
        ("253402300801", "one second past the year 10000"),
    ] {
        match eval_err(&format!("say time.format({}, \"%Y-%m-%d\")", timestamp)) {
            Error::Runtime(message, _) => assert!(
                message.contains("time.format"),
                "{} should be refused as a time.format error, got `{}`",
                what,
                message
            ),
            other => panic!("{} should be a Runtime error, got {:?}", what, other),
        }
    }
}

/// A negative timestamp names a real date, so it is formatted rather than
/// refused. The saturating `as u64` cast this replaced turned `time.format(-1)`
/// into `1970-01-01` — a wrong answer that never said so.
///
/// The assertion is on the formatted date itself. `say` returns `Nothing`, so a
/// test that matched `Ok(_)` passed whether the value was `1969-12-31` or the
/// buggy `1970-01-01`, and could not have caught the bug it exists to pin.
#[test]
fn edge_time_format_of_a_pre_epoch_timestamp_names_that_date() {
    for (timestamp, expected, what) in [
        ("-1", "1969-12-31", "one second before the epoch"),
        ("-2208988800", "1900-01-01", "the start of 1900"),
        ("-86400", "1969-12-31", "the whole day before the epoch"),
    ] {
        assert_eq!(
            eval_text(&format!("time.format({timestamp}, \"%Y-%m-%d\")")),
            expected,
            "{} must be the date it names, not 1970-01-01",
            what
        );
    }
    // A negative timestamp too far back for chrono to name is the same refusal
    // as one too far forward.
    assert_runtime_error("say time.format(-1e300, \"%Y-%m-%d\")", "time.format");
}

/// The epoch itself is a real instant and formats as 1970, not as an error.
/// This is the boundary the pre-epoch test above sits against, and the
/// one-second step is what makes it a boundary rather than a coincidence.
#[test]
fn time_format_at_the_epoch_boundary_is_accepted() {
    // The three instants on either side of the epoch, and the date each names.
    // `0` and `-1` are the whole boundary: a saturating cast made all three of
    // these print `1970-01-01`, so the dates are asserted, not just the success.
    for (timestamp, expected, what) in [
        ("0", "1970-01-01", "the epoch itself"),
        ("1", "1970-01-01", "one second after the epoch"),
        ("-1", "1969-12-31", "one second before the epoch"),
    ] {
        assert_eq!(
            eval_text(&format!("time.format({timestamp}, \"%Y-%m-%d\")")),
            expected,
            "{} is a real instant and must format as the date it names",
            what
        );
    }
}

/// A timestamp is not a number, or a format is not text. Both are refusals,
/// and neither reaches the arithmetic.
#[test]
fn edge_time_format_refuses_arguments_of_the_wrong_type() {
    for (source, what) in [
        ("say time.format(\"0\")", "a text timestamp"),
        ("say time.format([0])", "a list timestamp"),
        ("say time.format({})", "a record timestamp"),
        ("say time.format(0, 5)", "a numeric format"),
        ("say time.format(0, [\"%Y\"])", "a list format"),
    ] {
        match eval_err(source) {
            Error::Runtime(message, _) => assert!(
                message.contains("time.format"),
                "{} should be refused as a time.format error, got `{}`",
                what,
                message
            ),
            other => panic!("{} should be a Runtime error, got {:?}", what, other),
        }
    }
}

/// `time.format` with no arguments at all is the empty case of the same rule.
#[test]
fn edge_time_format_with_no_arguments_is_an_error() {
    assert_runtime_error("say time.format()", "time.format");
}

/// The failure is catchable. A panic through the interpreter thread is not,
/// so this is the assertion that distinguishes an error from a crash.
#[test]
fn edge_a_failing_time_call_is_catchable_by_try_catch() {
    // The program must reach the statement after the `try`, which it cannot do
    // if the panic unwound the thread out from under it. The comparison is last
    // so its `YesNo` is what is asserted: with a `say` the program returned
    // `Nothing` and reached the end whether or not the handler ran.
    for (source, what) in [
        (
            "set caught to 0\ntry\n    say time.format(1e300, \"%Y\")\ncatch error\n    set caught to 1\nend\ncaught > 0\n",
            "time.format",
        ),
        (
            "set caught to 0\ntry\n    say time.sleep(-1)\ncatch error\n    set caught to 1\nend\ncaught > 0\n",
            "time.sleep, whose refusal came from a panic in core",
        ),
        (
            "set caught to 0\ntry\n    say time.unix(\"-99999-01-01 00:00:00\")\ncatch error\n    set caught to 1\nend\ncaught > 0\n",
            "time.unix",
        ),
    ] {
        match eval(source) {
            Ok(Value::YesNo(true)) => {}
            Ok(other) => panic!("the {} handler must have run, got {:?}", what, other),
            Err(e) => panic!("a failing {} call must be catchable, got {:?}", what, e),
        }
    }
}

/// `time.now()` on this host is after the epoch, and must keep working. The
/// pre-epoch path is a new error; this is the assertion that the ordinary path
/// did not become one by accident.
#[test]
fn time_now_on_a_current_clock_is_still_a_record_of_seconds() {
    // The `seconds` field must be readable as a number at all — a record that
    // dropped it, or spelled it differently, would fail here rather than being
    // printed and discarded.
    match eval("set now to time.now()\nnow.seconds + 0\n") {
        Ok(Value::Number(n)) => assert!(
            n > 0.0,
            "time.now().seconds should be a positive number on a current clock, got {}",
            n
        ),
        Ok(other) => panic!("expected a number from now.seconds, got {:?}", other),
        Err(e) => panic!("time.now() must work on a current clock, got {:?}", e),
    }
}

/// The `seconds` field of `time.now()` is a number, and it is positive: a
/// conversion that reported the epoch for a pre-epoch clock, or a negative
/// count, would still be a "number" and the assertion above would pass.
///
/// The comparison is the *last statement*, so its `YesNo` is what `eval` hands
/// back. With a `say` in front of it the program returned `Nothing` and the
/// check passed whatever `seconds` was, which is the assertion this replaces.
#[test]
fn time_now_seconds_is_a_positive_number() {
    for (source, what) in [
        (
            "set now to time.now()\nnow.seconds > 0\n",
            "seconds must be positive",
        ),
        (
            "set now to time.now()\nnow.seconds >= 1600000000\n",
            "seconds must be past 2020, so a zeroed clock is caught",
        ),
        (
            "set now to time.now()\nnow.nanoseconds >= 0\n",
            "nanoseconds is a number, not a placeholder",
        ),
    ] {
        match eval(source) {
            Ok(Value::YesNo(true)) => {}
            Ok(other) => panic!("{}: expected YesNo(true), got {:?}", what, other),
            Err(e) => panic!("{}: got {:?}", what, e),
        }
    }
}

/// `time.sleep` is the same defect class as the `time.format` overflow this
/// phase started on: `Duration::from_secs_f64` panics on a negative, NaN,
/// infinite, or overflowing argument, from inside `core`, past every `Result` in
/// this crate. `say time.sleep(-1)` unwound the interpreter thread and replaced
/// the program's failure with "The interpreter thread stopped unexpectedly" —
/// a message a Redblue `try`/`catch error` cannot catch.
#[test]
fn edge_a_sleep_no_clock_can_wait_is_a_catchable_error() {
    for (source, what) in [
        ("say time.sleep(-1)", "a negative number of seconds"),
        ("say time.sleep(-0.5)", "a negative fraction of a second"),
        (
            "say time.sleep(1e300)",
            "far longer than any clock can wait",
        ),
        ("say time.sleep(1e20)", "longer than any clock can wait"),
        ("say time.sleep(\"soon\")", "a text where a number belongs"),
        ("say time.sleep()", "no argument at all"),
    ] {
        assert_runtime_error(source, "time.sleep");
        assert!(!what.is_empty());
    }
}

/// A sleep the clock can perform is not refused. `time.sleep(0)` is the
/// boundary the refusals above sit against, and refusing it would reject a
/// program that used to run.
#[test]
fn time_sleep_of_zero_seconds_is_accepted() {
    match eval("set slept to time.sleep(0)") {
        Ok(_) => {}
        Err(e) => panic!("a zero-second sleep must be allowed, got {:?}", e),
    }
}

/// `time.unix` parses years `chrono` accepts but `time.format` cannot represent,
/// so the two builtins disagreed about whether the instant existed. A date
/// outside the window both accept is a `Runtime` error naming `time.unix`.
#[test]
fn edge_time_unix_of_a_date_outside_the_shared_window_is_an_error() {
    for (text, what) in [
        ("-99999-01-01 00:00:00", "a year past what a date can name"),
        ("-262143-01-01 00:00:00", "the earliest year chrono parses"),
        (
            "-9999-01-01 00:00:00",
            "before the window time.format accepts",
        ),
    ] {
        let source = format!("say time.unix(\"{text}\")");
        match eval_err(&source) {
            Error::Runtime(message, _) => assert!(
                message.contains("time.unix"),
                "{} should be refused as a time.unix error, got `{}`",
                what,
                message
            ),
            other => panic!("{} should be a Runtime error, got {:?}", what, other),
        }
    }
}

/// The window boundary is the same on both sides: `time.unix` accepts the dates
/// `time.format` accepts, and a timestamp it produced is one `time.format` will
/// read back. This is the round trip that makes them one range rather than two
/// that happen to differ.
#[test]
fn time_unix_and_time_format_agree_on_the_window() {
    for (text, expected, what) in [
        ("1970-01-01 00:00:00", "1970-01-01", "the epoch itself"),
        (
            "1969-12-31 23:59:59",
            "1969-12-31",
            "one second before the epoch",
        ),
        (
            "9999-12-31 23:59:59",
            "9999-12-31",
            "the last year a date can name",
        ),
    ] {
        // `time.unix` reports the timestamp...
        let seconds = match eval(&format!("time.unix(\"{text}\")")) {
            Ok(Value::Number(n)) => n,
            Ok(other) => panic!("{} should be a number, got {:?}", what, other),
            Err(e) => panic!("{} is a real date, got {:?}", what, e),
        };
        // ...and that timestamp is one `time.format` reads back to the same
        // date, so neither builtin answers "no such instant" after the other
        // has said yes.
        assert_eq!(
            eval_text(&format!("time.format({seconds}, \"%Y-%m-%d\")")),
            expected,
            "{} must round trip through both builtins",
            what
        );
    }
}
