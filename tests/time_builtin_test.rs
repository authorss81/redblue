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

use redblue::bytecode::vm::BytecodeVm;
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
///
/// The comparison is the *last statement*, so the run's answer is the `YesNo`
/// it produced: the sleep was allowed, and it answered `nothing` — the shape
/// `time.sleep` is documented to have. A sleep that was refused would answer an
/// error instead, and one that began answering a number would answer
/// `YesNo(false)` here.
#[test]
fn time_sleep_of_zero_seconds_is_accepted() {
    match eval("set slept to time.sleep(0)\nslept is nothing\n") {
        Ok(Value::YesNo(true)) => {}
        Ok(other) => panic!(
            "a zero-second sleep must be allowed and answer nothing, got {:?}",
            other
        ),
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

// ---------------------------------------------------------------------------
// Dates and durations (phase-096)
// ---------------------------------------------------------------------------

#[track_caller]
fn yes(source: &str) -> bool {
    match eval(source) {
        Ok(Value::YesNo(answer)) => answer,
        Ok(other) => panic!("expected yes/no from `{source}`, got {other:?}"),
        Err(error) => panic!("`{source}` should run, got {error:?}"),
    }
}

#[track_caller]
fn runtime_error(source: &str) -> String {
    match eval(source) {
        Err(Error::Runtime(message, _)) => message,
        Err(other) => panic!("expected a Runtime error from `{source}`, got {other:?}"),
        Ok(value) => panic!("`{source}` should fail, got {value:?}"),
    }
}

#[test]
fn edge_date_constructs_and_compares() {
    let value = eval(
        "set earlier to time.date(2024, 2, 29)\n\
         set later to time.date(2024, 3, 1)\n\
         earlier < later",
    )
    .expect("a date must be constructible and comparable");
    assert_eq!(value, Value::YesNo(true));

    assert!(yes("set d to time.datetime(2024, 2, 29, 13, 45, 59)\n\
         d.year is 2024 and d.month is 2 and d.day is 29 and \
         d.hour is 13 and d.minute is 45 and d.second is 59"));

    // Monday is 1 and Sunday is 7, so the week starts on Monday.
    assert!(yes("time.date(2024, 1, 1).weekday is 1 and \
         time.date(2024, 1, 7).weekday is 7"));

    // An unknown property is a visible `nothing`, not an error, for both kinds.
    assert!(yes("set d to time.date(2024, 1, 1)\n\
         set span to time.duration(60)\n\
         d.bogus is nothing and span.bogus is nothing"));
}

#[test]
fn edge_subtracting_dates_spans_a_month_and_a_leap_day() {
    // January 31 to March 1 in a leap year is 30 days: 1 day into February and
    // all 29 days of it. The same span in a non-leap year is one day shorter.
    assert!(yes(
        "set span to time.date(2024, 3, 1) - time.date(2024, 1, 31)\n\
         span.days is 30 and span.total_seconds is 2592000"
    ));
    assert!(yes(
        "set leap to time.date(2024, 3, 1) - time.date(2024, 2, 28)\n\
         set plain to time.date(2023, 3, 1) - time.date(2023, 2, 28)\n\
         leap.days is 2 and plain.days is 1"
    ));
}

#[test]
fn edge_adding_months_clamps_at_the_end_of_the_month_both_ways() {
    // Forward: 31 January plus one month is 29 February in a leap year and
    // 28 February in a non-leap year. Backward: 31 March minus one month is
    // 29 February, not 3 March.
    assert!(yes(
        "set leap to time.add_months(time.date(2024, 1, 31), 1)\n\
         set plain to time.add_months(time.date(2023, 1, 31), 1)\n\
         set earlier to time.add_months(time.date(2024, 3, 31), -1)\n\
         leap.month is 2 and leap.day is 29 and \
         plain.month is 2 and plain.day is 28 and \
         earlier.month is 2 and earlier.day is 29"
    ));
}

#[test]
fn edge_adding_a_year_to_a_leap_day_lands_on_the_last_day_of_february() {
    assert!(yes(
        "set next to time.add_years(time.date(2024, 2, 29), 1)\n\
         set before to time.add_years(time.date(2024, 2, 29), -1)\n\
         next.year is 2025 and next.month is 2 and next.day is 28 and \
         before.year is 2023 and before.month is 2 and before.day is 28"
    ));
}

#[test]
fn edge_duration_components_and_negative_and_zero_spans() {
    assert!(yes("set span to time.duration(90000)\n\
         span.days is 1 and span.hours is 1 and span.minutes is 0 and \
         span.seconds is 0 and span.total_seconds is 90000"));
    assert!(yes(
        "set forward to time.date(2024, 1, 1) - time.date(2024, 1, 2)\n\
         set zero to time.date(2024, 1, 1) - time.date(2024, 1, 1)\n\
         forward.days is -1 and forward.total_seconds is -86400 and \
         zero.total_seconds is 0 and to_text(zero) is \"0 seconds\""
    ));
}

#[test]
fn edge_adding_a_duration_is_fixed_24_hour_days_with_no_dst() {
    // 2024-03-10 and 2024-11-03 are the two American daylight-saving dates.
    // A naive date has no zone, so neither day is 23 or 25 hours long: adding
    // 24 hours always moves exactly one calendar day.
    assert!(yes(
        "set spring to time.date(2024, 3, 10) + time.duration(86400)\n\
         set fall to time.add_hours(time.date(2024, 11, 3), 24)\n\
         spring.year is 2024 and spring.month is 3 and spring.day is 11 and \
         spring.hour is 0 and fall.day is 4 and fall.hour is 0"
    ));
}

#[test]
fn edge_a_date_is_first_class_in_records_lists_and_functions() {
    assert!(yes("to stamp()\n\
             give back time.date(2024, 5, 17)\n\
         end\n\
         set r to {when: stamp(), history: [stamp()]}\n\
         set s to r.when\n\
         type_of(s) is \"date\" and type_of(r.history[0]) is \"date\" and \
         s.year is 2024 and to_text(s) is \"2024-05-17 00:00:00\""));
}

#[test]
fn edge_comparing_a_date_with_a_number_is_a_catchable_type_error() {
    for (source, first, second) in [
        ("time.date(2024, 1, 1) < 5", "date", "number"),
        ("5 < time.date(2024, 1, 1)", "number", "date"),
        (
            "time.duration(1) < time.date(2024, 1, 1)",
            "duration",
            "date",
        ),
    ] {
        let message = runtime_error(source);
        assert!(
            message.contains(first) && message.contains(second),
            "`{source}` must name both {first} and {second}, got `{message}`"
        );
    }

    // Adding or subtracting a date and a number is refused too; it never
    // silently becomes the epoch or a count of seconds.
    assert!(runtime_error("time.date(2024, 1, 1) + 1").contains("non-numbers"));
    assert!(runtime_error("time.date(2024, 1, 1) - 1").contains("non-numbers"));
}

#[test]
fn edge_malformed_dates_and_arithmetic_are_refused_by_name() {
    for (source, needle) in [
        ("time.date(2024, 13, 1)", "not a real date"),
        ("time.date(2024, 2, 30)", "not a real date"),
        ("time.date(2024, 0, 1)", "not a real date"),
        ("time.date(2024, 1, 0)", "not a real date"),
        ("time.date(2024, 1.5, 1)", "whole number"),
        (r#"time.date("2024", 1, 1)"#, "whole number"),
        ("time.date(2024, 1)", "requires 3 numbers"),
        ("time.datetime(2024, 1, 1, 24, 0, 0)", "not a real time"),
        (r#"time.duration("1")"#, "time.duration"),
        ("time.duration(1e300)", "cannot represent"),
        ("time.add_years(5, 1)", "requires a date"),
        (
            "time.add_years(time.date(2024, 1, 1), 300000)",
            "cannot move the date that far",
        ),
        (
            "time.add_days(time.date(2024, 1, 1), 1e300)",
            "cannot represent",
        ),
    ] {
        let message = runtime_error(source);
        assert!(
            message.contains(needle),
            "`{source}` should mention `{needle}`, got `{message}`"
        );
    }
}

/// The lines a tree-walking run printed, and the value it ended on.
#[track_caller]
fn tree_walk(source: &str) -> (Vec<String>, Value) {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("the source lexes");
    let program = redblue::parser::parse(tokens).expect("the source parses");
    redblue::analyzer::analyze(&program).expect("the source analyzes");
    let (mut vm, result) = redblue::run_isolated(&program);
    let lines = vm.take_output();
    let value = result.expect("the source runs on the tree-walker");
    (lines, value)
}

/// The lines a bytecode run printed, and the value it ended on.
#[track_caller]
fn bytecode(source: &str) -> (Vec<String>, Value) {
    let chunk = redblue::compile_source(source).expect("the source compiles");
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let result = vm.run(&chunk);
    let lines = vm.take_output();
    let value = result.expect("the source runs on the bytecode VM");
    (lines, value)
}

#[test]
fn edge_the_tree_walker_and_the_bytecode_vm_agree_on_dates() {
    let source = "\
        set a to time.datetime(2024, 2, 29, 23, 30, 0)\n\
        set b to time.add_months(a, 1)\n\
        set span to b - a\n\
        set neg to time.duration(-90000)\n\
        {a: a, b: b, span: span, neg: neg}\n";

    let tree = tree_walk(source);
    let compiled = bytecode(source);
    assert_eq!(
        tree, compiled,
        "the two engines disagree about a date value"
    );
}

#[test]
fn edge_equality_between_a_date_or_duration_and_a_number_is_a_catchable_type_error() {
    // SPEC.md says a date or a duration compared with a number is a runtime
    // error, not a coercion. Ordering already refused it; equality used to
    // answer a silent `no`, which hid a program that compared the wrong kinds.
    for source in [
        "time.date(2024, 1, 1) is 5",
        "5 is time.date(2024, 1, 1)",
        "time.date(2024, 1, 1) is not 5",
        "time.duration(1) is 5",
        "5 is time.duration(1)",
        "time.duration(1) is not 5",
    ] {
        let message = runtime_error(source);
        assert!(
            message.contains("number")
                && (message.contains("date") || message.contains("duration")),
            "`{source}` must name both kinds it mixed, got `{message}`"
        );
    }

    // Equality within a kind is still defined and still answers yes/no.
    assert!(yes("time.date(2024, 1, 1) is time.date(2024, 1, 1)"));
    assert!(yes("time.date(2024, 1, 1) is not time.date(2024, 1, 2)"));
    assert!(yes("time.duration(60) is time.duration(60)"));
    assert!(yes("time.duration(60) is not time.duration(61)"));
}

#[test]
fn edge_negative_duration_components_are_signed_and_sum_to_the_total() {
    // A span before an instant reads with negative components, and the total
    // still matches the components the program can see.
    assert!(yes("set span to time.duration(-90000)\n\
         span.days is -1 and span.hours is -1 and span.minutes is 0 and \
         span.seconds is 0 and span.total_seconds is -90000"));
    assert!(yes("set span to time.duration(-1.5)\n\
         span.days is 0 and span.hours is 0 and span.minutes is 0 and \
         span.seconds is -1 and span.nanoseconds is -500000000 and \
         span.total_seconds is -1.5"));
    // Zero is the boundary between the two signs and has no negative zero.
    assert!(yes(
        "set zero to time.duration(0)\n\
         zero.days is 0 and zero.hours is 0 and zero.minutes is 0 and \
         zero.seconds is 0 and zero.nanoseconds is 0 and zero.total_seconds is 0"
    ));
    // Fractional seconds are rounded to the nearest millisecond, half away
    // from zero, so the 0.0004/0.0006 boundary is pinned in both signs.
    assert!(yes("time.duration(0.0004).nanoseconds is 0 and \
         time.duration(0.0006).nanoseconds is 1000000 and \
         time.duration(-0.0004).nanoseconds is 0 and \
         time.duration(-0.0006).nanoseconds is -1000000"));
}

#[test]
fn edge_month_end_overflow_backwards_in_leap_and_non_leap_years() {
    // The same backward move clamps to 29 February in a leap year and to
    // 28 February in a common year.
    assert!(yes(
        "set leap to time.add_months(time.date(2024, 3, 31), -1)\n\
         set plain to time.add_months(time.date(2023, 3, 31), -1)\n\
         leap.month is 2 and leap.day is 29 and \
         plain.month is 2 and plain.day is 28"
    ));
    // Thirteen months is a year plus one month, so the clamp applies after
    // the year rollover too.
    assert!(yes(
        "set moved to time.add_months(time.date(2024, 1, 31), 13)\n\
         moved.year is 2025 and moved.month is 2 and moved.day is 28"
    ));
}

#[track_caller]
fn tree_runtime_error(source: &str) -> String {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("the source lexes");
    let program = redblue::parser::parse(tokens).expect("the source parses");
    redblue::analyzer::analyze(&program).expect("the source analyzes");
    let (_vm, result) = redblue::run_isolated(&program);
    match result {
        Err(Error::Runtime(message, _)) => message,
        other => panic!("expected a Runtime error from `{source}`, got {other:?}"),
    }
}

#[track_caller]
fn bytecode_runtime_error(source: &str) -> String {
    let chunk = redblue::compile_source(source).expect("the source compiles");
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    match vm.run(&chunk) {
        Err(Error::Runtime(message, _)) => message,
        other => panic!("expected a Runtime error from `{source}`, got {other:?}"),
    }
}

#[test]
fn edge_the_tree_walker_and_the_bytecode_vm_agree_on_time_operations_and_errors() {
    let source = "\
        set a to time.datetime(2024, 1, 31, 23, 59, 59)\n\
        set b to time.add_months(a, 1)\n\
        set c to time.add_years(b, 1)\n\
        set d to time.add_days(c, 2)\n\
        set e to time.add_hours(d, 3)\n\
        set f to time.add_minutes(e, 4)\n\
        set g to time.add_seconds(f, 5)\n\
        set h to time.duration(-1.5)\n\
        {a: a, b: b, c: c, d: d, e: e, f: f, g: g, h: h, \
         earlier: a < b, same: a is a, span: b - a}\n";

    assert_eq!(
        tree_walk(source),
        bytecode(source),
        "the two engines disagree about time arithmetic"
    );

    // Both engines must refuse the same mixed-kind equality, with the same
    // message, not one silently answering `no`.
    let bad = "time.date(2024, 1, 1) is 5";
    assert_eq!(
        tree_runtime_error(bad),
        bytecode_runtime_error(bad),
        "the two engines disagree about a date/number comparison"
    );
}

#[test]
fn edge_time_from_unix_agrees_with_time_unix_on_literal_instants() {
    for (seconds, expected) in [
        ("0", "1970-01-01 00:00:00"),
        ("1704067200", "2024-01-01 00:00:00"),
        ("-1", "1969-12-31 23:59:59"),
        ("1709164800", "2024-02-29 00:00:00"),
    ] {
        assert_eq!(
            eval_text(&format!("to_text(time.from_unix({seconds}))")),
            expected,
            "time.from_unix({seconds}) must be the instant it names"
        );
    }
    // The two directions are one window: what `time.unix` reads, `from_unix`
    // must construct, including the leap day that has no 29 February next year.
    assert!(yes(
        "set leap to time.from_unix(time.unix(\"2024-02-29 00:00:00\"))\n\
         leap is time.date(2024, 2, 29)"
    ));
    assert!(yes(
        "set leap to time.from_unix(time.unix(\"2024-02-29 00:00:00\"))\n\
         time.unix(to_text(leap)) is 1709164800"
    ));
    assert!(yes(
        "set next to time.from_unix(1709164800) + time.duration(86400)\n\
         next is time.date(2024, 3, 1)"
    ));
}

#[test]
fn edge_time_from_unix_truncates_a_fractional_second_like_time_format() {
    // Both builtins read a number of seconds the same way: `as i64` truncates
    // toward zero, so the part before the decimal point is the instant. A
    // negative fraction must not round down to the previous second.
    assert_eq!(
        eval_text("to_text(time.from_unix(1.9))"),
        "1970-01-01 00:00:01"
    );
    assert_eq!(
        eval_text("to_text(time.from_unix(-1.9))"),
        "1969-12-31 23:59:59"
    );
    assert_eq!(
        eval_text("to_text(time.from_unix(0))"),
        "1970-01-01 00:00:00"
    );
}

#[test]
fn edge_time_from_unix_of_an_instant_outside_the_shared_window_is_an_error() {
    for source in [
        "time.from_unix(253402300801)",
        "time.from_unix(1e300)",
        "time.from_unix(-1e300)",
    ] {
        assert_runtime_error(source, "time.from_unix");
    }
    // A number is the only thing a caller may pass, and the empty call is the
    // empty case of the same rule rather than a panic or a silent zero.
    for source in [
        "time.from_unix(\"0\")",
        "time.from_unix([0])",
        "time.from_unix()",
    ] {
        assert_runtime_error(source, "time.from_unix");
    }
}

#[test]
fn edge_the_tree_walker_and_the_bytecode_vm_agree_on_time_from_unix() {
    let source = "\
        set d to time.from_unix(1709164800)\n\
        set span to d - time.date(2024, 2, 28)\n\
        {d: d, span: span, text: to_text(d), next: d + time.duration(86400)}\n";
    assert_eq!(
        tree_walk(source),
        bytecode(source),
        "the two engines disagree about time.from_unix"
    );

    let bad = "time.from_unix(1e300)";
    assert_eq!(
        tree_runtime_error(bad),
        bytecode_runtime_error(bad),
        "the two engines disagree about a time.from_unix refusal"
    );
}
