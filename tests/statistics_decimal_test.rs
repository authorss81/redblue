//! Descriptive statistics and exact decimal arithmetic (phase-107).
//!
//! The expected values here are computed by hand in the test or are the spelled
//! decimal result of the operation; none of them is produced by calling the
//! implementation twice and comparing it with itself.

use redblue::bytecode::vm::BytecodeVm;
use redblue::decimal::Decimal;
use redblue::{Error, Value};

/// Runs `source` through lexer → parser → tree-walking VM and returns the value
/// of its last statement.
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

#[track_caller]
fn number(source: &str) -> f64 {
    match eval(source) {
        Value::Number(number) => number,
        other => panic!("`{source}` should answer a number, got {other:?}"),
    }
}

#[track_caller]
fn decimal(source: &str) -> Decimal {
    match eval(source) {
        Value::Decimal(decimal) => decimal,
        other => panic!("`{source}` should answer a decimal, got {other:?}"),
    }
}

#[track_caller]
fn decimal_text(source: &str, text: &str) -> Decimal {
    let value = decimal(source);
    assert_eq!(
        value,
        Decimal::parse(text).expect("expected decimal should parse"),
        "`{source}` answered the wrong decimal"
    );
    value
}

#[track_caller]
fn runtime_message(source: &str) -> String {
    match eval_err(source) {
        Error::Runtime(message, span) => {
            assert!(span.is_known(), "`{source}` failed without a source span");
            message
        }
        other => panic!("`{source}` should fail with a runtime error, got {other:?}"),
    }
}

#[test]
fn statistics_assert_hand_computed_values() {
    // mean = 10 / 4 = 2.5; median = (2 + 3) / 2 = 2.5.
    assert_eq!(number("mean([1, 2, 3, 4])"), 2.5);
    assert_eq!(number("median([1, 2, 3, 4])"), 2.5);
    // Population variance of [2, 4, 4, 4, 5, 5, 7, 9] is 4, so the standard
    // deviation is 2.
    assert_eq!(number("variance([2, 4, 4, 4, 5, 5, 7, 9])"), 4.0);
    assert_eq!(number("standard_deviation([2, 4, 4, 4, 5, 5, 7, 9])"), 2.0);
    // A perfect rising line has Pearson correlation 1; a falling one -1.
    assert!((number("correlation([1, 2, 3], [2, 4, 6])") - 1.0).abs() < 1e-12);
    assert!((number("correlation([1, 2, 3], [6, 4, 2])") + 1.0).abs() < 1e-12);
}

#[test]
fn median_interpolates_an_even_list_and_refuses_an_empty_one() {
    assert_eq!(number("median([1, 2, 3, 4])"), 2.5);
    assert_eq!(number("median([1, 2, 3, 4, 5])"), 3.0);
    let message = runtime_message("median([])");
    assert!(
        message.contains("at least one number") && message.contains("empty"),
        "empty median should name the empty list, got: {message}"
    );
}

#[test]
fn mode_returns_all_ties_in_ascending_order() {
    let modes = eval("mode([3, 1, 3, 1, 2])");
    assert_eq!(
        modes,
        Value::list(vec![Value::Number(1.0), Value::Number(3.0)]),
        "both tied modes belong in the answer, sorted ascending"
    );
    // Every value tied at one occurrence is still a mode, not "no mode".
    assert_eq!(
        eval("mode([2, 1])"),
        Value::list(vec![Value::Number(1.0), Value::Number(2.0)])
    );
}

#[test]
fn percentile_uses_linear_interpolation_and_literal_expected_values() {
    // Rank position = (4 - 1) * 25 / 100 = 0.75, between 1 and 2.
    assert_eq!(number("percentile([1, 2, 3, 4], 25)"), 1.75);
    assert_eq!(number("percentile([1, 2, 3, 4], 0)"), 1.0);
    assert_eq!(number("percentile([1, 2, 3, 4], 100)"), 4.0);
    assert_eq!(number("percentile([1, 2, 3, 4], 50)"), 2.5);
}

#[test]
fn edge_statistics_reject_empty_lists_and_bad_ranks() {
    for source in [
        "mean([])",
        "median([])",
        "mode([])",
        "variance([])",
        "standard_deviation([])",
    ] {
        let message = runtime_message(source);
        assert!(
            message.contains("empty"),
            "`{source}` should name the empty list, got: {message}"
        );
    }
    assert!(runtime_message("percentile([1, 2], -1)").contains("0 to 100"));
    assert!(runtime_message("percentile([1, 2], 101)").contains("0 to 100"));
    assert!(
        runtime_message("correlation([1, 1, 1], [1, 2, 3])").contains("never varies"),
        "a flat list has no correlation to report"
    );
}

#[test]
fn edge_statistics_handle_a_single_element_and_negative_values() {
    for function in ["mean", "median", "variance", "standard_deviation"] {
        assert_eq!(
            number(&format!("{function}([5])")),
            if function == "mean" || function == "median" {
                5.0
            } else {
                0.0
            }
        );
    }
    assert_eq!(
        eval("mode([5])"),
        Value::list(vec![Value::Number(5.0)]),
        "a single element is its own mode"
    );
    assert_eq!(number("mean([-2, -4])"), -3.0);
    assert_eq!(number("median([-2, -4])"), -3.0);
    assert_eq!(number("variance([-2, -4])"), 1.0);
    assert_eq!(number("percentile([-4, -2], 50)"), -3.0);
}

#[test]
fn edge_statistics_reject_a_list_that_is_not_all_numbers() {
    let message = runtime_message("mean([1, \"two\", 3])");
    assert!(
        message.contains("item 1 is text"),
        "the failing item should be named by kind and index, got: {message}"
    );
    let message = runtime_message("correlation([1, 2], [3, \"four\"])");
    assert!(message.contains("item 1 is text"), "{message}");
    let message = runtime_message("median(\"not a list\")");
    assert!(message.contains("requires a list of numbers"), "{message}");
}

#[test]
fn decimal_addition_and_sum_are_exact_where_f64_is_not() {
    // The binary sum really is not 0.3, which is the defect the decimal type
    // exists to remove.
    assert_ne!(number("0.1 + 0.2"), 0.3);
    assert_eq!(
        decimal_text(
            "decimal.sum([decimal.from_text(\"0.1\"), decimal.from_text(\"0.2\")])",
            "0.3"
        ),
        Decimal::parse("0.3").unwrap()
    );
    // Multiplication and subtraction keep their exact decimal answers too.
    decimal_text(
        "decimal.from_text(\"19.99\") * decimal.from_text(\"3\")",
        "59.97",
    );
    decimal_text(
        "decimal.from_text(\"1.00\") - decimal.from_text(\"0.99\")",
        "0.01",
    );
}

#[test]
fn edge_decimal_mixing_with_a_number_is_refused_rather_than_silent() {
    let message = runtime_message("decimal.from_text(\"0.1\") + 0.2");
    assert!(
        message.contains("Cannot add a decimal with a number")
            && message.contains("decimal.from_number"),
        "mixing should refuse by both kinds and name the conversion, got: {message}"
    );
    let message = runtime_message("0.2 * decimal.from_text(\"0.1\")");
    assert!(
        message.contains("Cannot multiply a number with a decimal"),
        "{message}"
    );
    // The explicit conversion exists, and the explicit loss is a conversion
    // back to f64, not an arithmetic accident.
    assert_eq!(number("decimal.to_number(decimal.from_number(0.1))"), 0.1);
    assert_eq!(number("decimal.to_number(decimal.from_text(\"0.3\"))"), 0.3);
}

#[test]
fn edge_decimal_division_that_does_not_terminate_rounds_to_eighteen_places() {
    decimal_text(
        "decimal.from_text(\"1\") / decimal.from_text(\"3\")",
        "0.333333333333333333",
    );
    decimal_text(
        "decimal.from_text(\"2\") / decimal.from_text(\"3\")",
        "0.666666666666666667",
    );
    decimal_text(
        "decimal.from_text(\"-2\") / decimal.from_text(\"3\")",
        "-0.666666666666666667",
    );
    decimal_text("decimal.from_text(\"1\") / decimal.from_text(\"2\")", "0.5");
    let message = runtime_message("decimal.from_text(\"1\") / decimal.from_text(\"0\")");
    assert_eq!(message, "Division by zero");
}

#[test]
fn edge_decimal_division_of_large_representable_quotients_does_not_overflow() {
    decimal_text(
        "decimal.from_text(\"0.000000000000001001\") / decimal.from_text(\"0.000000000000000001\")",
        "1001",
    );
    decimal_text(
        "decimal.from_text(\"1000\") / decimal.from_text(\"0.000000000000000001\")",
        "1000000000000000000000",
    );
    decimal_text(
        "decimal.from_text(\"123456789012345678\") / decimal.from_text(\"0.000000000000000001\")",
        "123456789012345678000000000000000000",
    );
}

#[test]
fn edge_mean_and_variance_do_not_overflow_on_large_finite_values() {
    assert_eq!(number("mean([1e308, 1e308])"), 1e308);
    assert_eq!(number("variance([1e308, 1e308])"), 0.0);
}

#[test]
fn edge_decimal_parsing_rejects_malformed_and_too_fine_text() {
    for source in [
        "decimal.from_text(\"\")",
        "decimal.from_text(\"1.2.3\")",
        "decimal.from_text(\"1e3\")",
        "decimal.from_text(\"0.0000000000000000001\")",
    ] {
        let message = runtime_message(source);
        assert!(
            message.contains("not a decimal") || message.contains("at most 18"),
            "`{source}` should be refused as decimal text, got: {message}"
        );
    }
    let message = runtime_message("decimal.from_text(0.1)");
    assert!(message.contains("requires text"), "{message}");
}

#[test]
fn edge_decimal_two_to_the_53_boundary_is_where_integers_stop_being_exact() {
    // 2^53 itself is the largest whole number every f64 holds exactly.
    decimal_text("decimal.from_number(9007199254740992)", "9007199254740992");
    assert_eq!(
        eval("decimal.from_number(9007199254740992) is decimal.from_text(\"9007199254740992\")"),
        Value::YesNo(true)
    );
    // 2^53 + 1 is the first whole number that is *not* held exactly; the
    // number literal has already become 2^53 by the time it reaches the
    // conversion, so the two sides must differ.
    assert_eq!(
        eval(
            "decimal.from_number(9007199254740993) is not decimal.from_text(\"9007199254740993\")"
        ),
        Value::YesNo(true),
        "2^53 + 1 must not silently compare equal to an exact decimal"
    );
    let converted = match eval("decimal.from_number(9007199254740993)") {
        Value::Decimal(decimal) => decimal,
        other => panic!("expected a decimal, got {other:?}"),
    };
    assert_eq!(converted, Decimal::parse("9007199254740992").unwrap());
}

#[test]
fn edge_decimal_handles_a_very_large_magnitude_exactly() {
    decimal_text(
        "decimal.from_text(\"123456789012345678.90\") + decimal.from_text(\"0.01\")",
        "123456789012345678.91",
    );
    decimal_text(
        "decimal.sum([decimal.from_text(\"100000000000000000000\"), decimal.from_text(\"0.000000000000000001\")])",
        "100000000000000000000.000000000000000001",
    );
    // Two values at the top of the exact integer range still add exactly.
    decimal_text(
        "decimal.from_text(\"9007199254740992\") + decimal.from_text(\"9007199254740992\")",
        "18014398509481984",
    );
}

#[test]
fn edge_decimal_arithmetic_refuses_a_result_beyond_its_precision_or_range() {
    // 10^-18 squared needs 36 places, which is more than a decimal holds.
    let message = runtime_message(
        "decimal.from_text(\"0.000000000000000001\") * decimal.from_text(\"0.000000000000000001\")",
    );
    assert!(message.contains("at most 18"), "{message}");
    // A mantissa past i128 is refused by name rather than wrapping.
    let message = runtime_message(
        "decimal.from_text(\"100000000000000000000000000000000000000\") + decimal.from_text(\"100000000000000000000000000000000000000\")",
    );
    assert!(
        message.contains("too large") || message.contains("not a decimal"),
        "{message}"
    );
}

#[test]
fn edge_decimal_comparison_and_equality_are_exact_across_scales() {
    assert_eq!(
        eval("decimal.from_text(\"1.0\") is decimal.from_text(\"1\")"),
        Value::YesNo(true)
    );
    assert_eq!(
        eval("decimal.from_text(\"0.10\") is decimal.from_text(\"0.1\")"),
        Value::YesNo(true)
    );
    assert_eq!(
        eval("decimal.from_text(\"1.01\") < decimal.from_text(\"1.1\")"),
        Value::YesNo(true)
    );
    // Comparing a decimal with a number is refused just as arithmetic is.
    assert!(runtime_message("decimal.from_text(\"1\") is 1").contains("decimal with a number"));
    assert!(runtime_message("decimal.from_text(\"1\") < 2").contains("decimal with a number"));
}

/// The same program on the tree-walking VM, as data (or a failure message).
#[track_caller]
fn tree_outcome(source: &str) -> Result<Value, String> {
    let tokens = redblue::lexer::Lexer::tokenize(source).map_err(|e| e.to_string())?;
    let ast = redblue::parser::parse(tokens).map_err(|e| e.to_string())?;
    redblue::analyzer::analyze(&ast).map_err(|e| e.to_string())?;
    let (_, result) = redblue::run_isolated(&ast);
    result.map_err(|e| e.to_string())
}

/// The same program through the bytecode compiler and VM, as data.
#[track_caller]
fn bytecode_outcome(source: &str) -> Result<Value, String> {
    let chunk = redblue::compile_source(source).map_err(|e| e.to_string())?;
    let mut vm = BytecodeVm::new();
    vm.run(&chunk).map_err(|e| e.to_string())
}

#[test]
fn tree_walker_and_bytecode_vm_agree_on_statistics_and_decimals() {
    for source in [
        "mean([1, 2, 3, 4])",
        "median([1, 2, 3, 4])",
        "mode([3, 1, 3, 1, 2])",
        "variance([2, 4, 4, 4, 5, 5, 7, 9])",
        "percentile([1, 2, 3, 4], 25)",
        "correlation([1, 2, 3], [2, 4, 6])",
        "decimal.sum([decimal.from_text(\"0.1\"), decimal.from_text(\"0.2\")])",
        "decimal.from_text(\"1\") / decimal.from_text(\"3\")",
        "decimal.from_text(\"0.1\") + 0.2",
        "mean([])",
        "median([\"a\"])",
    ] {
        let tree = tree_outcome(source);
        let bytecode = bytecode_outcome(source);
        assert_eq!(tree, bytecode, "the two engines disagree about `{source}`");
    }
}
