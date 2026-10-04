//! Numeric edge semantics: total behaviour for every double a Redblue program
//! can reach.
//!
//! `Value::Number` is an `f64`, so every arithmetic operator can leave the
//! reals: `5 % 0` is NaN and `1e308 * 1e308` is infinity. Redblue's policy
//! (see SPEC.md, "Numeric semantics") is that neither may enter `Value::Number`
//! silently — the operation is a `Runtime` error instead. The *display* of a
//! non-finite number is still specified, because `Value` is public API and a
//! Rust caller can build `Value::Number(f64::NAN)` directly.

use redblue::Error;
use redblue::Value;

/// Runs `source` through lexer → parser → VM and returns the value of its last
/// statement.
#[track_caller]
fn eval(source: &str) -> Value {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let ast = redblue::parser::parse(tokens).expect("source should parse");
    let mut vm = redblue::Vm::new();
    vm.run(&ast).expect("source should run")
}

/// Runs `source` and returns the pipeline error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let ast = redblue::parser::parse(tokens).expect("source should parse");
    let mut vm = redblue::Vm::new();
    vm.run(&ast).expect_err("source should have failed")
}

/// Asserts `source` fails at runtime with exactly `expected` as its message.
#[track_caller]
fn assert_runtime_error(source: &str, expected: &str) {
    match eval_err(source) {
        Error::Runtime(message, span) => {
            assert_eq!(
                message, expected,
                "`{}` failed with the wrong runtime message",
                source
            );
            assert!(span.is_known(), "`{}` failed without a source span", source);
        }
        other => panic!(
            "`{}` should fail with a Runtime error, got {:?}",
            source, other
        ),
    }
}

/// The message of the runtime error `source` produced.
#[track_caller]
fn runtime_message(source: &str) -> String {
    match eval_err(source) {
        Error::Runtime(message, _) => message,
        other => panic!(
            "`{}` should fail with a Runtime error, got {:?}",
            source, other
        ),
    }
}

/// What `say` would print for `value`.
fn display(value: &Value) -> String {
    value.to_string()
}

#[test]
fn modulo_by_zero_is_a_runtime_error() {
    assert_runtime_error("set x to 5 % 0", "Modulo by zero");
}

#[test]
fn edge_modulo_by_a_zero_divisor_of_every_shape_is_rejected() {
    // `0.0`, and negative zero reached by arithmetic, are both zero divisors.
    assert_runtime_error("set x to 5 % 0.0", "Modulo by zero");
    assert_runtime_error("set d to 0 * -1\nset x to 5 % d", "Modulo by zero");
    // The empty/zero edge on the left: `0 % 0` is the undefined one.
    assert_runtime_error("set x to 0 % 0", "Modulo by zero");
    // Modulo is otherwise total: a negative dividend and a fractional divisor
    // keep the sign of the dividend, like Rust and C.
    assert_eq!(eval("5 % 3"), Value::Number(2.0));
    assert_eq!(eval("-5 % 3"), Value::Number(-2.0));
    assert_eq!(eval("5 % -3"), Value::Number(2.0));
    assert_eq!(eval("5 % 0.5"), Value::Number(0.0));
    assert_eq!(eval("0 % 7"), Value::Number(0.0));
}

#[test]
fn edge_division_by_zero_still_reports_an_error_for_every_numerator() {
    assert_runtime_error("set x to 1 / 0", "Division by zero");
    assert_runtime_error("set x to 0 / 0", "Division by zero");
    assert_runtime_error("set x to -1 / 0", "Division by zero");
    assert_runtime_error("set x to 1 / -0.0", "Division by zero");
    assert_runtime_error("set d to 0 * -1\nset x to 1 / d", "Division by zero");
    // Division that does not divide evenly still works.
    assert_eq!(eval("7 / 2"), Value::Number(3.5));
    assert_eq!(eval("6 / 3"), Value::Number(2.0));
}

#[test]
fn edge_overflow_to_a_non_finite_number_is_a_runtime_error() {
    // Infinity is not a Redblue number, so overflowing to it is a failure
    // rather than a value that prints as `infinity`.
    assert_runtime_error("set x to 1e308 * 1e308", "infinity is not a finite number");
    assert_runtime_error(
        "set x to -1e308 * 1e308",
        "-infinity is not a finite number",
    );
    assert_runtime_error(
        "set x to 1e308 * 1e308 * 1e308",
        "infinity is not a finite number",
    );
    // Addition and subtraction overflow the same way multiplication does.
    assert_runtime_error("set x to 1e308 + 1e308", "infinity is not a finite number");
    assert_runtime_error(
        "set x to -1e308 - 1e308",
        "-infinity is not a finite number",
    );
    // The boundary below which the operation still fits.
    assert_eq!(eval("1e308 * 1"), Value::Number(1e308));
    assert_eq!(eval("1e308 * 0"), Value::Number(0.0));
    assert_eq!(eval("1e308 + 1e307"), Value::Number(1.1e308));
}

#[test]
fn edge_an_index_at_the_numeric_limit_is_nothing_not_a_panic() {
    // An index is a number, so it can be as wide as a double. Every one of
    // these is a value or `nothing` — the cast to an integer saturates and
    // `len + index` cannot overflow — and none of them is a panic.
    assert_eq!(eval("[1, 2, 3][0]"), Value::Number(1.0));
    // A negative index counts from the end, so -1 is the last element.
    assert_eq!(eval("[1, 2, 3][-1]"), Value::Number(3.0));
    // Empty list, one past the end, far out of bounds, and the two extremes of
    // a double as an index.
    assert_eq!(eval("[][0]"), Value::Nothing);
    assert_eq!(eval("[1, 2, 3][3]"), Value::Nothing);
    assert_eq!(eval("[1, 2, 3][999]"), Value::Nothing);
    assert_eq!(eval("[1, 2, 3][1e308]"), Value::Nothing);
    assert_eq!(eval("[1, 2, 3][-1e308]"), Value::Nothing);
    // An index that is not a number at all never gets that far.
    assert_runtime_error(
        "set xs to [1, 2, 3]\nsay xs[1e400]",
        "infinity is not a finite number",
    );
}

#[test]
fn edge_random_number_refuses_a_range_whose_width_overflows() {
    // `1e308 - -1e308` is `infinity`, so every draw from this range would be a
    // number that does not exist. The result is refused instead. The draw is
    // seeded from the clock, but the refusal does not depend on it: a draw of
    // exactly zero gives `0 * infinity`, which is NaN, also refused.
    let message = runtime_message("random_number(-1e308, 1e308)");
    assert!(
        message.ends_with("is not a finite number"),
        "a range wider than a double should be refused, got {}",
        message
    );
    // An ordinary range is untouched.
    match eval("random_number(1, 2)") {
        Value::Number(n) => assert!((1.0..2.0).contains(&n), "got {}", n),
        other => panic!("random_number(1, 2) should be a number, got {:?}", other),
    }
}

#[test]
fn edge_a_number_literal_out_of_range_is_rejected() {
    // `1e400` is a perfectly good token: it is the value that does not exist.
    assert_runtime_error("set x to 1e400", "infinity is not a finite number");
    // `-1e400` negates the literal, and the literal is what fails.
    assert_runtime_error("set x to -1e400", "infinity is not a finite number");
    // The largest literal that is still finite is accepted.
    assert_eq!(eval("1.7976931348623157e308"), Value::Number(f64::MAX));
}

#[test]
fn edge_json_numbers_out_of_range_are_rejected() {
    // `1e400` is valid JSON syntax that overflows a double. `json.parse`
    // wraps the inner failure in a message of its own.
    assert!(matches!(
        eval_err("set x to json.parse(\"1e400\")"),
        Error::Runtime(..)
    ));
    assert!(
        runtime_message("set x to json.parse(\"1e400\")")
            .contains("infinity is not a finite number"),
        "json.parse should say the number is not finite"
    );
    // The normal path is untouched.
    assert_eq!(eval("set x to json.parse(\"1.5\")"), Value::Nothing);
    assert_eq!(eval("json.parse(\"42\")"), Value::Number(42.0));
}

#[test]
fn edge_negative_zero_equals_zero_and_prints_as_zero() {
    assert_eq!(eval("0 * -1"), Value::Number(-0.0));
    assert_eq!(display(&eval("0 * -1")), "0");
    // Negative zero is not a distinct value in Redblue: it compares equal.
    assert_eq!(eval("0 * -1 is 0"), Value::YesNo(true));
    assert_eq!(eval("0 * -1 is -0.0"), Value::YesNo(true));
    assert_eq!(eval("0 * -1 is not 1"), Value::YesNo(true));
    // A literal typed as negative zero behaves the same way.
    assert_eq!(display(&eval("-0.0")), "0");
    // A zero divisor spelled as negative zero is still a zero divisor.
    assert_runtime_error("set x to 5 % -0.0", "Modulo by zero");
}

#[test]
fn edge_a_number_wider_than_2_53_is_not_printed_as_another_integer() {
    // 2^53 is the last whole number an f64 holds exactly, and it prints as
    // itself.
    assert_eq!(display(&eval("9007199254740992")), "9007199254740992");
    // 2^53 + 1 is rounded on the way in, so it prints as the number it became.
    assert_eq!(display(&eval("9007199254740993")), "9007199254740992");
    // Past the i64 range the old `as i64` display saturated to i64::MAX and
    // reported a whole number the program never held.
    assert_eq!(
        display(&eval("99999999999999999999")),
        "100000000000000000000"
    );
    assert_eq!(
        display(&eval("-99999999999999999999")),
        "-100000000000000000000"
    );
    assert_eq!(display(&eval("0.5")), "0.5");
    assert_eq!(display(&eval("2.5")), "2.5");
    assert_eq!(display(&eval("-2.5")), "-2.5");
}

#[test]
fn edge_non_finite_numbers_have_a_defined_display() {
    // No Redblue program reaches these, but `Value::Number` is public.
    assert_eq!(display(&Value::Number(f64::NAN)), "not a number");
    assert_eq!(display(&Value::Number(f64::INFINITY)), "infinity");
    assert_eq!(
        display(&Value::Number(f64::NEG_INFINITY)),
        "negative infinity"
    );
    // The widest finite numbers are still finite: they must not be mistaken for
    // `-infinity`, and they must keep every digit the double holds.
    for widest in [f64::MAX, f64::MIN, f64::MIN_POSITIVE] {
        let text = display(&Value::Number(widest));
        assert!(
            text != "infinity" && text != "negative infinity" && text != "not a number",
            "{} should print as itself, got {}",
            widest,
            text
        );
    }
    assert_eq!(display(&Value::Number(f64::MIN)), format!("{}", f64::MIN));
}

#[test]
fn edge_json_never_sees_a_non_finite_number() {
    // `json.stringify` has no JSON literal for one, and the language refuses to
    // produce one, so every number it can be handed is finite.
    assert!(matches!(eval_err("set x to 1e400"), Error::Runtime(..)));
    assert_eq!(
        eval("json.stringify(99999999999999999999)"),
        Value::Text("100000000000000000000".to_string())
    );
    assert_eq!(eval("json.stringify(1.5)"), Value::Text("1.5".to_string()));
}

#[test]
fn nan_is_rejected_by_the_only_door_into_value_number() {
    let span = redblue::Span::new(1, 1);
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let error = Value::number(value, span)
            .err()
            .unwrap_or_else(|| panic!("{} should not become a Value::Number", value));
        match error {
            Error::Runtime(message, reported) => {
                assert_eq!(reported, span);
                assert!(
                    message.ends_with("is not a finite number"),
                    "unexpected message for {}: {}",
                    value,
                    message
                );
            }
            other => panic!("expected a Runtime error, got {:?}", other),
        }
    }
    // A finite number is untouched, including the boundaries.
    for value in [0.0, -0.0, 1.0, -1.0, f64::MAX, f64::MIN, f64::MIN_POSITIVE] {
        assert_eq!(
            Value::number(value, span).expect("a finite number is allowed"),
            Value::Number(value)
        );
    }
}

#[test]
fn nan_never_compares_equal_so_it_cannot_be_tested_for() {
    // This is why NaN must not enter `Value::Number`: it is unequal to itself,
    // so `expect x to be x` on a NaN would fail and no comparison could ever
    // settle it.
    assert_ne!(Value::Number(f64::NAN), Value::Number(f64::NAN));
    assert_ne!(Value::Number(f64::NAN), Value::Number(0.0));
    // Every language-level route to a NaN is closed.
    for source in [
        "set x to 5 % 0",
        "set x to 1e308 * 1e308",
        "set x to 1e400",
        "set x to json.parse(\"1e400\")",
        "set x to sqrt(-1)",
    ] {
        assert!(
            matches!(eval_err(source), Error::Runtime(..)),
            "`{}` should not produce a number",
            source
        );
    }
}

#[test]
fn edge_sqrt_of_a_negative_number_has_no_answer() {
    let negative = vec![Value::Number(-1.0)];
    assert_eq!(
        redblue::stdlib::builtin_function("sqrt", negative),
        Some(Value::Nothing),
        "sqrt of a negative number must answer `nothing`, not NaN"
    );
    assert_eq!(
        redblue::stdlib::builtin_function("sqrt", vec![Value::Number(4.0)]),
        Some(Value::Number(2.0))
    );
    // The boundary at zero is still an answer.
    assert_eq!(
        redblue::stdlib::builtin_function("sqrt", vec![Value::Number(0.0)]),
        Some(Value::Number(0.0))
    );
}

#[test]
fn edge_non_finite_results_are_refused_inside_a_list_and_record() {
    // A container is no way round the check: the value is refused where it is
    // computed.
    assert_runtime_error(
        "set xs to [1e308 * 1e308]",
        "infinity is not a finite number",
    );
    assert_runtime_error(
        "set r to {big: 1e308 * 1e308}",
        "infinity is not a finite number",
    );
    // And an index computed from a refused number never runs.
    assert_runtime_error(
        "set xs to [1, 2]\nset i to 1e308 * 1e308\nset x to xs[i]",
        "infinity is not a finite number",
    );
}
