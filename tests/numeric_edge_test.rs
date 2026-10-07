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

/// Lexes `source` and returns the lexer error it produced.
#[track_caller]
fn lex_err(source: &str) -> Error {
    redblue::lexer::Lexer::tokenize(source).expect_err("source should fail to lex")
}

/// Asserts `source` fails to lex, naming `literal` as the malformed literal.
#[track_caller]
fn assert_lexer_error(source: &str, literal: &str) {
    match lex_err(source) {
        Error::Lexer(message, span) => {
            assert!(
                message.contains(literal),
                "`{}` should name the literal it could not read, got {}",
                source,
                message
            );
            assert!(span.is_known(), "`{}` failed without a source span", source);
        }
        other => panic!(
            "`{}` should fail with a Lexer error, got {:?}",
            source, other
        ),
    }
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
fn edge_an_index_at_the_numeric_limit_is_an_error_not_a_panic() {
    // An index is a number, so it can be as wide as a double. Every one of
    // these is a value or a `Runtime` error — the cast to an integer saturates,
    // `len + index` stays in range because a saturated `i64::MIN` plus a
    // non-negative length does not wrap — and none of them is a panic.
    assert_eq!(eval("[1, 2, 3][0]"), Value::Number(1.0));
    // A negative index counts from the end, so -1 is the last element.
    assert_eq!(eval("[1, 2, 3][-1]"), Value::Number(3.0));
    // The whole legal range, at both ends.
    assert_eq!(eval("[1, 2, 3][2]"), Value::Number(3.0));
    assert_eq!(eval("[1, 2, 3][-3]"), Value::Number(1.0));
    // Empty list, one past the end, far out of bounds, and the two extremes of
    // a double as an index: each names no element, so each is an error that
    // says so rather than an index silently answering `nothing`.
    assert_runtime_error(
        "[][0]",
        "Index 0 is out of bounds: length is 0, the list is empty, so it has no valid index",
    );
    assert_runtime_error(
        "[1, 2, 3][3]",
        "Index 3 is out of bounds: length is 3, valid indexes are 0 to 2",
    );
    assert_runtime_error(
        "[1, 2, 3][999]",
        "Index 999 is out of bounds: length is 3, valid indexes are 0 to 2",
    );
    // The two extremes of a double as an index. Both saturate to an integer
    // far outside every list that can exist, so both are errors. The index is
    // printed with the same expansion `say` gives it, which at these magnitudes
    // is 300 digits wide, so only the parts that are stable are asserted.
    for source in ["[1, 2, 3][1e308]", "[1, 2, 3][-1e308]", "[][1e308]"] {
        match eval_err(source) {
            Error::Runtime(message, span) => {
                assert!(message.starts_with("Index "), "unexpected: {}", message);
                assert!(
                    message.ends_with(&format!(
                        "is out of bounds: length is {}, {}",
                        if source.ends_with("[][1e308]") { 0 } else { 3 },
                        if source.ends_with("[][1e308]") {
                            "the list is empty, so it has no valid index"
                        } else {
                            "valid indexes are 0 to 2"
                        }
                    )),
                    "unexpected: {}",
                    message
                );
                assert!(span.is_known());
            }
            other => panic!(
                "`{}` should fail with a Runtime error, got {:?}",
                source, other
            ),
        }
    }
    // An index that is not a number at all never gets that far.
    assert_runtime_error(
        "set xs to [1, 2, 3]\nsay xs[1e400]",
        "infinity is not a finite number",
    );
}

/// A range wider than a double is drawable, and the draw is a number.
///
/// `1e308 - -1e308` is `infinity`, so `min + r * (max - min)` put a number that
/// does not exist into every draw from this range and refused all of them — the
/// one thing this file exists to prevent, and it happened for every value in a
/// perfectly ordinary range. Interpolating between the two ends scales each of
/// them by a number in `[0, 1]` before adding, which cannot overflow, so the
/// range answers. This test was `edge_random_number_refuses_a_range_whose_width_overflows`
/// and asserted the refusal; the refusal was the defect, so the assertion is
/// replaced by the stronger property — a finite number, every time, and one that
/// is really inside the range it was asked for. No entry was removed.
#[test]
fn edge_a_draw_from_a_range_wider_than_a_double_is_still_a_finite_number() {
    // Many draws, because the old arithmetic failed for all of them and the new
    // one must not fail for any: a single draw of exactly zero used to give
    // `0 * infinity`, which is `NaN`, and the seed is the clock.
    for _ in 0..64 {
        match eval("random_number(-1e308, 1e308)") {
            Value::Number(n) => {
                assert!(
                    n.is_finite(),
                    "a draw from -1e308 to 1e308 must be a number, got {}",
                    n
                );
                assert!(
                    (-1e308..=1e308).contains(&n),
                    "a draw must be inside the range it was asked for, got {}",
                    n
                );
            }
            other => panic!(
                "a range wider than a double should still be drawable, got {:?}",
                other
            ),
        }
    }
    // The narrow end of the same range, so the interpolation is not simply
    // answering one end of it every time.
    let mut seen_both = false;
    let mut negative = false;
    for _ in 0..64 {
        match eval("random_number(-1e308, 0)") {
            Value::Number(n) => {
                assert!(n.is_finite() && (-1e308..=0.0).contains(&n), "got {}", n);
                negative |= n < 0.0;
            }
            other => panic!(
                "a draw from -1e308 to 0 should be a number, got {:?}",
                other
            ),
        }
        if negative {
            seen_both = true;
            break;
        }
    }
    assert!(
        seen_both,
        "a draw from -1e308 to 0 must be able to be below zero"
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

#[test]
fn edge_a_sign_right_after_a_number_is_an_operator_not_part_of_the_literal() {
    // `5-2` used to be read as one literal, "5-2", which is not a number, and it
    // silently became 0. A sign only belongs to a literal after its exponent.
    assert_eq!(eval("5-2"), Value::Number(3.0));
    assert_eq!(eval("5+2"), Value::Number(7.0));
    assert_eq!(eval("1-2"), Value::Number(-1.0));
    assert_eq!(eval("10-2-3"), Value::Number(5.0));
    assert_eq!(eval("7/2"), Value::Number(3.5));
    assert_eq!(eval("100-1-1"), Value::Number(98.0));
    // An exponent sign is still part of the literal it belongs to.
    assert_eq!(eval("1e+5"), Value::Number(100000.0));
    assert_eq!(eval("1e-3"), Value::Number(0.001));
    assert_eq!(eval("2E-3"), Value::Number(0.002));
    assert_eq!(eval("1.5e-1"), Value::Number(0.15));
    // A number with no fractional part written out still reads as one number.
    assert_eq!(eval("5."), Value::Number(5.0));
    assert_eq!(eval(".5"), Value::Number(0.5));
}

#[test]
fn edge_a_malformed_number_literal_is_a_lexer_error_not_a_silent_zero() {
    // A literal that is not a number is a mistake in the source. Every one of
    // these used to become the number 0 and the program carried on.
    assert_lexer_error("set x to 1.2.3", "1.2.3");
    assert_lexer_error("set x to 2..3", "2..3");
    assert_lexer_error("set x to 3e", "3e");
    assert_lexer_error("set x to 1e+", "1e+");
    assert_lexer_error("set x to 1.5e", "1.5e");
    assert_lexer_error("say 1.2.3", "1.2.3");
    // The refusal happens in the lexer, so it is the same wherever the literal
    // sits: a list, a record, an index, or on its own.
    for source in [
        "set xs to [1, 2..3]",
        "set r to {a: 3e}",
        "set xs to [1, 2]\nset x to xs[1.2.3]",
        "set x to 1.2.3 + 1",
    ] {
        assert!(
            matches!(lex_err(source), Error::Lexer(..)),
            "`{}` should be refused by the lexer, not read as 0",
            source
        );
    }
}

#[test]
fn edge_a_literal_too_small_to_hold_is_the_number_zero() {
    // Underflow has no infinity to refuse: zero is a number the language has,
    // so the outcome is `0` and it is stated in SPEC.md rather than refused.
    assert_eq!(eval("1e-400"), Value::Number(0.0));
    assert_eq!(display(&eval("1e-400")), "0");
    assert_eq!(eval("1e-324"), Value::Number(0.0));
    assert_eq!(display(&eval("1e-324")), "0");
    // `5e-324` is the smallest positive double, so it does not underflow.
    match eval("5e-324") {
        Value::Number(n) => assert!(n > 0.0, "5e-324 underflowed to {}", n),
        other => panic!("5e-324 should be a number, got {:?}", other),
    }
    // Arithmetic underflows the same way, and only to the same one answer.
    assert_eq!(eval("1e-200 * 1e-200"), Value::Number(0.0));
    // A number that underflowed to zero is a zero divisor like any other.
    assert_runtime_error("set x to 1 / 1e-400", "Division by zero");
    assert_runtime_error("set x to 5 % 1e-400", "Modulo by zero");
}

#[test]
fn edge_a_numeric_for_range_cannot_step_into_a_non_finite_number() {
    // `for each x from 1 to 10` did not parse when this was written, so
    // `Statement::ForRange` was reachable only from a hand-built AST. It parses
    // now (phase-028), and this test still builds the AST by hand because the
    // overflow it covers needs a `from` larger than the grammar would be read
    // as: `for each x from 1e308 to 1.5e308 by 1e308`. The counter goes through
    // the same door as any computed number — a step that overflows it is a
    // runtime error, not an infinity that ends the loop by accident.
    let program = redblue::parser::Program {
        statements: vec![redblue::parser::Stmt {
            span: redblue::Span::new(1, 1),
            statement: redblue::parser::Statement::ForRange {
                variable: "x".to_string(),
                start: redblue::parser::Expr::Number(1e308),
                end: redblue::parser::Expr::Number(1.5e308),
                step: Some(redblue::parser::Expr::Number(1e308)),
                body: Vec::new(),
            },
        }],
    };
    let mut vm = redblue::Vm::new();
    match vm.run(&program) {
        Err(Error::Runtime(message, span)) => {
            assert_eq!(message, "infinity is not a finite number");
            assert!(span.is_known(), "the overflow reported no position");
        }
        Err(other) => panic!(
            "a counter overflow should be a Runtime error, got {:?}",
            other
        ),
        Ok(value) => panic!(
            "a counter overflow should not return a value, got {:?}",
            value
        ),
    }
}
