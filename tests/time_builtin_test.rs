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
            "one second before the year 10000",
        ),
        (
            "10000-01-01 00:00:00",
            "+10000-01-01",
            "the shared window's last instant",
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
fn edge_time_from_unix_floors_a_fractional_second_like_time_format() {
    // Both builtins read a number of seconds the same way: the whole second is
    // the floor, so the part before the decimal point is the instant for a
    // positive fraction and a negative fraction stays in the second before the
    // epoch. Truncating toward zero would make `-1.9` land on 1969-12-31
    // 23:59:59 and `-0.5` land on 1970-01-01, one second after the date.
    assert_eq!(
        eval_text("to_text(time.from_unix(1.9))"),
        "1970-01-01 00:00:01"
    );
    assert_eq!(
        eval_text("to_text(time.from_unix(-1.9))"),
        "1969-12-31 23:59:58"
    );
    assert_eq!(
        eval_text("to_text(time.from_unix(-0.5))"),
        "1969-12-31 23:59:59"
    );
    assert_eq!(
        eval_text("to_text(time.from_unix(0))"),
        "1970-01-01 00:00:00"
    );
    assert_eq!(
        eval_text("time.format(-0.5, \"%Y-%m-%d %H:%M:%S\")"),
        "1969-12-31 23:59:59",
        "the formatter must name the same second as from_unix"
    );
    // The number `time.unix` emits for a pre-epoch fraction must name the
    // second the date value is in, not the one after it.
    assert!(yes(
        "time.from_unix(time.unix(time.parse(\"1969-12-31T23:59:59.5Z\"))) \
         is time.datetime(1969, 12, 31, 23, 59, 59)"
    ));
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

#[test]
fn edge_time_parse_accepts_iso_offsets_against_literal_utc_dates() {
    // A `+02:00` reading is the same instant as the UTC reading two hours
    // earlier, and a `-05:00` reading is three hours later. Both sides are
    // literal `time.datetime` values, so this compares the parser's answer with
    // the calendar rather than with another call to the parser.
    assert!(yes(
        "set zoned to time.parse(\"2024-01-01T12:00:00+02:00\")\n\
         set utc to time.datetime(2024, 1, 1, 10, 0, 0)\n\
         set western to time.parse(\"2023-12-31T22:00:00-05:00\")\n\
         set western_utc to time.datetime(2024, 1, 1, 3, 0, 0)\n\
         zoned is utc and western is western_utc"
    ));
}

#[test]
fn edge_time_parse_keeps_fractions_and_reads_a_missing_zone_as_utc() {
    // A zone-free timestamp is read as UTC, the stance phase-096 documented for
    // every date. A date-only string is midnight on that day, and the fraction
    // is kept to the nanosecond rather than rounded away.
    assert!(yes("set naive to time.parse(\"2024-02-29T23:59:58\")\n\
         set day to time.parse(\"2024-02-29\")\n\
         set frac to time.parse(\"2024-02-29T23:59:58.123456789Z\")\n\
         naive is time.datetime(2024, 2, 29, 23, 59, 58) and \
         day is time.date(2024, 2, 29) and \
         frac.second is 58 and frac.nanosecond is 123456789"));
}

#[test]
fn edge_time_format_of_a_date_pads_and_honours_the_chosen_layout() {
    assert_eq!(
        eval_text(r#"time.format(time.date(2024, 2, 9), "%Y-%m-%d")"#),
        "2024-02-09",
        "a single-digit month and day must be zero-padded"
    );
    assert_eq!(
        eval_text(r#"time.format(time.datetime(2024, 2, 9, 3, 4, 5), "%Y/%m/%d %H:%M:%S")"#),
        "2024/02/09 03:04:05",
        "the chosen layout must be used exactly"
    );
    assert_eq!(
        eval_text("time.format(time.datetime(2024, 2, 9, 3, 4, 5))"),
        "2024-02-09 03:04:05",
        "the default layout stays the one time.format already used"
    );
    // The parsed fraction survives into a caller-chosen output layout.
    assert_eq!(
        eval_text(r#"time.format(time.parse("2024-02-09T03:04:05.123456789Z"), "%S.%f")"#),
        "05.123456789"
    );
    // A layout chrono cannot read is a catchable Runtime error, not a panic.
    assert_runtime_error(r#"time.format(time.date(2024, 1, 1), "%Q")"#, "time.format");
    // `%#z` is not an `Item::Error`: chrono parses it as its parse-only
    // permissive offset, then fails while formatting. That failure must also
    // be a catchable Runtime error rather than a panic from `to_string`.
    assert_runtime_error(
        r#"time.format(time.date(2024, 1, 1), "%#z")"#,
        "time.format",
    );
}

/// The numeric timestamp branch formats a UTC `DateTime`, not a naive value.
/// A naive value has no zone, so `%Z` and `%z` rendered as empty text even
/// though the instant was UTC; the zone-bearing layouts must still name UTC.
#[test]
fn edge_time_format_of_a_numeric_timestamp_keeps_its_utc_zone() {
    assert_eq!(
        eval_text(r#"time.format(0, "%Z")"#),
        "UTC",
        "a numeric timestamp is UTC and must name its zone"
    );
    assert_eq!(
        eval_text(r#"time.format(0, "%z")"#),
        "+0000",
        "a numeric timestamp must render its UTC offset"
    );
}

/// The old `time.unix` layout went through chrono's `%Y`, which reads one to
/// three digit years. Those callers must keep their answers, so the ISO parser
/// accepts the same short years rather than narrowing the old surface.
#[test]
fn edge_time_unix_reads_the_short_years_the_old_layout_accepted() {
    assert!(yes(
        "time.unix(\"999-01-01 00:00:00\") is -30641760000 and \
         time.unix(\"99-01-01 00:00:00\") is -59042995200 and \
         time.unix(\"0999-01-01 00:00:00\") is -30641760000 and \
         time.parse(\"999-01-01\") is time.datetime(999, 1, 1, 0, 0, 0)"
    ));
}

/// The old `time.unix` layout also read unpadded fields and the leap second
/// `:60` through chrono's `%m/%d/%H/%M/%S`. Those callers must keep their
/// answers, so `time.unix` falls back to that layout while `time.parse` stays
/// strict ISO and refuses both forms.
#[test]
fn edge_time_unix_keeps_the_old_layouts_unpadded_fields_and_leap_second() {
    assert!(yes("time.unix(\"2024-1-1 0:0:0\") is 1704067200 and \
         time.unix(\"2024-01-1 0:0:0\") is 1704067200 and \
         time.unix(\"2016-12-31 23:59:60\") is 1483228799"));
    assert_runtime_error(r#"time.parse("2024-1-1 0:0:0")"#, "time.parse");
    assert_runtime_error(r#"time.parse("2016-12-31 23:59:60")"#, "time.parse");
}

#[test]
fn edge_time_unix_reads_a_date_value_and_iso_text_with_an_offset() {
    // The date value and the text both name the epoch second the literal says.
    assert!(yes(
        "time.unix(time.datetime(2024, 1, 1, 0, 0, 0)) is 1704067200 and \
         time.unix(time.parse(\"2024-01-01T02:00:00+02:00\")) is 1704067200 and \
         time.unix(\"2024-01-01T00:00:00Z\") is 1704067200 and \
         time.unix(time.parse(\"1969-12-31T23:59:59Z\")) is -1"
    ));
    // The old fixed layout is still accepted with the value it always had.
    assert!(yes("time.unix(\"2024-01-01 00:00:00\") is 1704067200"));
    assert!(yes(
        "time.format(time.from_unix(1704067200), \"%Y-%m-%d %H:%M:%S\") \
         is \"2024-01-01 00:00:00\""
    ));
}

/// A Unix timestamp carries a fractional second, so `time.unix` must not
/// silently drop the fraction `time.parse` kept. Every fraction here is
/// binary-exact, so the assertion is on the numeric value rather than on a
/// formatting round trip through the same code.
#[test]
fn edge_time_unix_preserves_a_parsed_fraction() {
    assert!(yes(
        "time.unix(time.parse(\"2024-02-29T23:59:58.5Z\")) is 1709251198.5 and \
         time.unix(time.parse(\"2024-02-29T23:59:58.25Z\")) is 1709251198.25 and \
         time.unix(time.parse(\"2024-02-29T23:59:58.75Z\")) is 1709251198.75 and \
         time.unix(time.parse(\"1969-12-31T23:59:59.5Z\")) is -0.5 and \
         time.unix(time.parse(\"2024-01-01T00:00:00.5+02:00\")) is 1704060000.5 and \
         time.unix(time.parse(\"2024-02-29T23:59:58Z\")) is 1709251198"
    ));
}

/// An `f64` cannot hold every nanosecond at 2024's magnitude. A fraction
/// within half an ulp of 1 used to round the addition up to the next whole
/// second, so the timestamp named 23:59:59 while the date value still named
/// 23:59:58.999999999. The answer must stay inside the second it floored to,
/// including at the positive window edge where the rounded value was
/// `+10000-01-01`.
#[test]
fn edge_time_unix_keeps_a_near_one_fraction_inside_its_second() {
    assert!(yes(
        "set stamp to time.unix(time.parse(\"2024-02-29T23:59:58.999999999Z\"))\n\
         stamp is greater than or equal to 1709251198 and \
         stamp is less than 1709251199 and \
         time.from_unix(stamp) is time.datetime(2024, 2, 29, 23, 59, 58)"
    ));
    assert!(yes(
        "set edge to time.unix(time.parse(\"9999-12-31T23:59:59.999999999Z\"))\n\
         edge is greater than or equal to 253402300799 and \
         edge is less than 253402300800 and \
         time.from_unix(edge) is time.datetime(9999, 12, 31, 23, 59, 59)"
    ));
}

#[test]
fn edge_time_unix_fraction_agrees_across_engines_and_text() {
    let source = "\
        set d to time.parse(\"2024-02-29T23:59:58.5Z\")\n\
        set stamp to time.unix(d)\n\
        {stamp: stamp, whole: time.unix(time.parse(\"2024-02-29T23:59:58Z\")), \
         text: to_text(d), formatted: time.format(d, \"%Y-%m-%dT%H:%M:%S%.f\")}\n";
    assert_eq!(
        tree_walk(source),
        bytecode(source),
        "the two engines disagree about a fractional Unix timestamp"
    );
    // The fraction survives both the text round trip and the formatter, so the
    // timestamp, the date text and the chosen layout name the same instant.
    assert!(yes(
        "set d to time.parse(\"2024-02-29T23:59:58.5Z\")\n\
         time.unix(to_text(d)) is time.unix(d) and \
         time.format(d, \"%Y-%m-%dT%H:%M:%S%.f\") is \"2024-02-29T23:59:58.500\""
    ));
}

/// The window ends at `10000-01-01T00:00:00Z`, so the fraction in that final
/// second is past the end. It must be a catchable `time.unix` error rather than
/// a timestamp `time.format` then refuses, while the whole boundary second
/// itself stays accepted.
#[test]
fn edge_time_unix_refuses_a_fraction_past_the_positive_window_edge() {
    assert_runtime_error(
        "time.unix(time.parse(\"+10000-01-01T00:00:00.5Z\"))",
        "time.unix",
    );
    assert!(yes("set caught to 0\n\
         try\n    say time.unix(time.parse(\"+10000-01-01T00:00:00.5Z\"))\n\
         catch error of Runtime\n    set caught to 1\nend\n\
         caught is 1"));
    assert!(yes(
        "time.unix(time.parse(\"10000-01-01T00:00:00Z\")) is 253402300800"
    ));
}

#[test]
fn edge_time_parse_refuses_malformed_text_with_a_position() {
    for text in [
        "",
        "not a date",
        "2024-02-30",
        "2024-02-29T12:00",
        "2024-02-29T23:59:58.",
        "2024-02-29T23:59:58.1234567890Z",
        "2024-02-29T23:59:58+24:00",
        "2024-02-29T23:59:58Zx",
        "2024-02-29T23:59:58Z日本語",
        "2024-02-29T１２:00:00Z",
    ] {
        let message = runtime_error(&format!("time.parse(\"{text}\")"));
        assert!(
            message.contains("time.parse") && message.contains("position"),
            "`{text}` must be refused with a position, got `{message}`"
        );
    }
    // The offending characters and the exact position are named, not merely the
    // fact that something failed. The leap second starts at byte 17 and the
    // trailing `x` is at byte 20.
    let leap = runtime_error(r#"time.parse("2024-02-29T23:59:60Z")"#);
    assert!(
        leap.contains("position 17") && leap.contains("60") && leap.contains("leap second"),
        "a leap second must be refused where it is written, got `{leap}`"
    );
    let trailing = runtime_error(r#"time.parse("2024-02-29T23:59:58Zx")"#);
    assert!(
        trailing.contains("position 20") && trailing.contains("x"),
        "trailing garbage must be named at its position, got `{trailing}`"
    );

    // The failure is catchable and the wrong type is refused by name.
    assert!(yes("set caught to 0\n\
         try\n    say time.parse(\"\")\n\
         catch error of Runtime\n    set caught to 1\nend\n\
         caught is 1"));
    for source in ["time.parse(5)", "time.parse([\"x\"])", "time.parse()"] {
        assert_runtime_error(source, "time.parse");
    }
}

/// The expanded year `%Y` writes at the positive boundary must be text the
/// parser reads back. `10000-01-01` is the window's last instant, but chrono
/// writes it as `+10000-01-01`, so `to_text`, `time.unix`, and `time.parse`
/// all have to accept a leading `+` the ordinary four-digit form does not have.
#[test]
fn edge_the_positive_window_boundary_round_trips_through_text() {
    assert!(yes("set edge to time.parse(\"10000-01-01\")\n\
         set text to to_text(edge)\n\
         text is \"+10000-01-01 00:00:00\" and \
         time.unix(text) is 253402300800 and \
         time.parse(text) is edge and \
         time.format(253402300800, \"%Y-%m-%d\") is \"+10000-01-01\""));
}

/// Every out-of-range field the parser checks has its own branch, so each is
/// pinned to the byte where its digits start. A `Runtime` error is catchable;
/// the loop also proves that with an actual `try`/`catch`, not merely by the
/// error's kind.
#[test]
fn edge_time_parse_refuses_each_out_of_range_field_at_its_position() {
    for (text, position, expected) in [
        ("2024-13-01", 5, "a month from 1 to 12"),
        ("2024-00-01", 5, "a month from 1 to 12"),
        ("2024-01-32", 8, "a day from 1 to 31"),
        ("2024-01-01T24:00:00", 11, "an hour from 0 to 23"),
        ("2024-01-01T00:60:00", 14, "a minute from 0 to 59"),
        (
            "2024-01-01T00:00:00+02:60",
            23,
            "an offset minute from 0 to 59",
        ),
    ] {
        let message = runtime_error(&format!("time.parse(\"{text}\")"));
        assert!(
            message.contains("time.parse")
                && message.contains(&format!("position {position}"))
                && message.contains(expected),
            "`{text}` must be refused at position {position} as `{expected}`, got `{message}`"
        );
        assert!(
            yes(&format!(
                "set caught to 0\n\
                 try\n    say time.parse(\"{text}\")\n\
                 catch error of Runtime\n    set caught to 1\nend\n\
                 caught is 1"
            )),
            "`{text}` must be a catchable Runtime error"
        );
    }
}

/// `chrono` cannot hold a year outside its calendar range, and the refusal
/// must name the year rather than the day field. A real year with an
/// impossible day still blames the day at its own position.
#[test]
fn edge_time_parse_names_the_year_when_the_calendar_cannot_hold_it() {
    for text in ["999999-01-01", "262143-01-01", "-262144-01-01"] {
        let message = runtime_error(&format!("time.parse(\"{text}\")"));
        assert!(
            message.contains("time.parse")
                && message.contains("position 0")
                && message.contains("year"),
            "`{text}` must be refused at the year, got `{message}`"
        );
    }
    let day = runtime_error(r#"time.parse("2024-02-30")"#);
    assert!(
        day.contains("position 8") && day.contains("day"),
        "a real year with an impossible day must still name the day, got `{day}`"
    );
}

#[test]
fn edge_time_parse_keeps_leap_days_and_refuses_leap_seconds() {
    assert!(yes(
        "time.parse(\"2024-02-29\") is time.date(2024, 2, 29) and \
         time.parse(\"2000-02-29\") is time.date(2000, 2, 29)"
    ));
    // A common year and a century that is not a leap year are refused at the
    // day field, which starts at byte 8.
    for text in ["1900-02-29", "2023-02-29"] {
        let message = runtime_error(&format!("time.parse(\"{text}\")"));
        assert!(
            message.contains("position 8"),
            "`{text}` is not a leap day and must be refused at the day, got `{message}`"
        );
    }
    assert!(
        runtime_error(r#"time.parse("2024-02-29T23:59:60Z")"#).contains("leap second"),
        "a leap second must be refused rather than accepted silently"
    );
}

#[test]
fn edge_time_parse_accepts_a_comma_fraction_mark_like_iso_8601() {
    // ISO-8601 permits either `.` or `,` before the fractional second. Log
    // lines and CSVs from locales that write a decimal comma use the latter.
    assert!(yes(
        "set comma to time.parse(\"2024-02-29T23:59:58,123456789Z\")\n\
         set dot to time.parse(\"2024-02-29T23:59:58.123456789Z\")\n\
         comma is dot and comma.nanosecond is 123456789"
    ));
    assert_eq!(
        eval_text(r#"time.format(time.parse("2024-02-29T03:04:05,5Z"), "%S.%f")"#),
        "05.500000000",
        "a comma fraction must reach the formatter with the same value"
    );
}

#[test]
fn edge_time_parse_reads_a_space_separator_and_a_date_only_as_utc() {
    assert!(yes("time.parse(\"2024-02-29 23:59:58\") is \
         time.datetime(2024, 2, 29, 23, 59, 58) and \
         time.parse(\"2024-02-29T23:59:58\") is \
         time.datetime(2024, 2, 29, 23, 59, 58) and \
         time.parse(\"2024-02-29\") is time.date(2024, 2, 29)"));
}

#[test]
fn edge_time_parse_refuses_a_lone_fraction_mark_and_an_offset_without_minutes() {
    let fraction = runtime_error(r#"time.parse("2024-02-29T23:59:58,")"#);
    assert!(
        fraction.contains("position 20") && fraction.contains("decimal mark"),
        "a comma with no digits must be refused where it is written, got `{fraction}`"
    );
    let offset = runtime_error(r#"time.parse("2024-02-29T23:59:58+02")"#);
    assert!(
        offset.contains("position 22") && offset.contains("offset"),
        "an offset with no minutes must be refused at the end, got `{offset}`"
    );
    assert!(yes("set caught to 0\n\
         try\n    say time.parse(\"2024-02-29T23:59:58,\")\n\
         catch error of Runtime\n    set caught to 1\nend\n\
         caught is 1"));
}

#[test]
fn edge_time_unix_refuses_malformed_text_and_wrong_types() {
    for source in [
        "time.unix(\"not a timestamp\")",
        "time.unix(5)",
        "time.unix([\"2024-02-29\"])",
        "time.unix()",
    ] {
        assert_runtime_error(source, "time.unix");
    }
    assert!(yes("set caught to 0\n\
         try\n    say time.unix(\"not a timestamp\")\n\
         catch error of Runtime\n    set caught to 1\nend\n\
         caught is 1"));
}

#[test]
fn edge_the_tree_walker_and_the_bytecode_vm_agree_on_parsing_and_formatting_dates() {
    let source = "\
        set d to time.parse(\"2024-02-29T23:59:58.123456789+02:00\")\n\
        set t to time.format(d, \"%Y-%m-%dT%H:%M:%S\")\n\
        set u to time.unix(d)\n\
        {d: d, t: t, u: u, n: d.nanosecond}\n";
    assert_eq!(
        tree_walk(source),
        bytecode(source),
        "the two engines disagree about parsing or formatting a date"
    );

    let bad = "time.parse(\"2024-02-29T23:59:60Z\")";
    assert_eq!(
        tree_runtime_error(bad),
        bytecode_runtime_error(bad),
        "the two engines disagree about a parse refusal"
    );
}

/// A fraction that `time.parse` kept must still be in the text a program
/// prints, so `to_text` round-trips the instant rather than dropping it.
#[test]
fn edge_date_text_keeps_a_parsed_fraction_and_round_trips() {
    assert_eq!(
        eval_text(r#"to_text(time.parse("2024-02-29T23:59:58.123456789Z"))"#),
        "2024-02-29 23:59:58.123456789"
    );
    assert!(yes(
        "set d to time.parse(\"2024-02-29T23:59:58.123456789Z\")\n\
         time.parse(to_text(d)) is d and \
         time.parse(to_text(d)).nanosecond is d.nanosecond"
    ));
    // A date with no fraction keeps exactly the text it always had.
    assert_eq!(
        eval_text("to_text(time.date(2024, 2, 9))"),
        "2024-02-09 00:00:00"
    );
    // JSON is the same text form, so the two printers cannot disagree.
    assert_eq!(
        eval_text(r#"json.stringify(time.parse("2024-02-29T23:59:58.5Z"))"#),
        "\"2024-02-29 23:59:58.500\""
    );
}

/// The fraction is written with three, six or nine digits, the shortest form
/// that keeps every digit, and a zero fraction writes no mark at all.
#[test]
fn edge_date_text_uses_the_shortest_fraction_without_losing_precision() {
    for (text, expected) in [
        ("2024-02-29T23:59:58.1Z", "2024-02-29 23:59:58.100"),
        ("2024-02-29T23:59:58.123Z", "2024-02-29 23:59:58.123"),
        ("2024-02-29T23:59:58.123456Z", "2024-02-29 23:59:58.123456"),
        (
            "2024-02-29T23:59:58.123456789Z",
            "2024-02-29 23:59:58.123456789",
        ),
        ("2024-02-29T23:59:58.000Z", "2024-02-29 23:59:58"),
    ] {
        assert_eq!(
            eval_text(&format!("to_text(time.parse(\"{text}\"))")),
            expected,
            "`{text}` must print as `{expected}`"
        );
    }
}

/// Appending a second fraction to the text a date printed is trailing garbage,
/// and the refusal names where it starts and stays catchable.
#[test]
fn edge_date_text_round_trip_refuses_trailing_fraction_text() {
    let source = r#"time.parse("2024-02-29 23:59:58.500.250")"#;
    let result = eval(source);
    assert!(
        result.is_err(),
        "a second fraction after a date's text must be refused"
    );
    match result {
        Err(Error::Runtime(message, _)) => assert!(
            message.contains("time.parse") && message.contains("position 23"),
            "the trailing fraction must be named at its position, got `{message}`"
        ),
        other => panic!("expected a Runtime error, got {other:?}"),
    }
    assert!(yes("set caught to 0\n\
         try\n    say time.parse(\"2024-02-29 23:59:58.500.250\")\n\
         catch error of Runtime\n    set caught to 1\nend\n\
         caught is 1"));
}

/// Both engines must print and serialize a fractional date identically, not
/// merely parse it to the same value.
#[test]
fn edge_the_tree_walker_and_the_bytecode_vm_agree_on_fractional_date_text() {
    let source = "\
        set d to time.parse(\"2024-02-29T23:59:58.123456789+02:00\")\n\
        {text: to_text(d), json: json.stringify(d), n: d.nanosecond}\n";
    assert_eq!(
        tree_walk(source),
        bytecode(source),
        "the two engines disagree about printing or serializing a fractional date"
    );
}
