use redblue::bytecode::vm::BytecodeVm;
use redblue::{Error, Value};

#[track_caller]
fn eval(source: &str) -> Value {
    redblue::run_source_value(source)
        .unwrap_or_else(|error| panic!("should have run, failed with {error:?}\n{source}"))
}

#[track_caller]
fn eval_err(source: &str) -> Error {
    redblue::run_source_value(source).expect_err("source should have failed")
}

fn runtime_message(error: &Error) -> String {
    match error {
        Error::Runtime(message, _) => message.clone(),
        other => panic!("expected a Runtime error, got {other:?}"),
    }
}

fn rows(values: &[&[f64]]) -> Value {
    Value::list(
        values
            .iter()
            .map(|row| Value::list(row.iter().map(|n| Value::Number(*n)).collect()))
            .collect(),
    )
}

fn numbers(values: &[f64]) -> Value {
    Value::list(values.iter().map(|n| Value::Number(*n)).collect())
}

#[test]
fn matrix_multiply_matches_hand_written_numbers() {
    let value = eval(
        r#"
set a to matrix.from_lists([[1, 2], [3, 4]])
set b to matrix.from_lists([[5, 6], [7, 8]])
set product to matrix.matmul(a, b)
expect matrix.to_lists(product) to be [[19, 22], [43, 50]]
matrix.to_lists(product)
"#,
    );
    assert_eq!(
        value,
        rows(&[&[19.0, 22.0], &[43.0, 50.0]]),
        "a hand-written 2x2 matrix product was not reproduced"
    );
}

#[test]
fn elementwise_operations_and_reductions_assert_literal_values() {
    let value = eval(
        r#"
set a to matrix.from_lists([[1, 2], [3, 4]])
set b to matrix.from_lists([[10, 20], [30, 40]])
[matrix.to_lists(matrix.add(a, b)), matrix.to_lists(matrix.subtract(b, a)), matrix.to_lists(matrix.multiply(a, b)), matrix.to_lists(matrix.scale(a, 0.5)), matrix.sum(a), matrix.mean(a), matrix.min(a), matrix.max(a), matrix.to_lists(matrix.transpose(a))]
"#,
    );
    assert_eq!(
        value,
        Value::list(vec![
            rows(&[&[11.0, 22.0], &[33.0, 44.0]]),
            rows(&[&[9.0, 18.0], &[27.0, 36.0]]),
            rows(&[&[10.0, 40.0], &[90.0, 160.0]]),
            rows(&[&[0.5, 1.0], &[1.5, 2.0]]),
            Value::Number(10.0),
            Value::Number(2.5),
            Value::Number(1.0),
            Value::Number(4.0),
            rows(&[&[1.0, 3.0], &[2.0, 4.0]]),
        ]),
        "elementwise operations or reductions did not produce their literal values"
    );
}

#[test]
fn edge_zero_by_zero_matrix_has_defined_behaviour() {
    let value = eval(
        r#"
set z to matrix.zeros(0, 0)
set i to matrix.identity(0)
[matrix.shape(z), matrix.to_lists(z), matrix.sum(z), matrix.to_lists(matrix.add(z, z)), matrix.shape(matrix.matmul(z, z)), matrix.to_lists(matrix.transpose(z)), matrix.to_lists(i), matrix.shape(matrix.from_lists([])), matrix.shape(matrix.from_lists([[], [], []])), matrix.to_lists(matrix.from_lists([[], [], []])), matrix.sum(matrix.from_lists([[], [], []]))]
"#,
    );
    assert_eq!(
        value,
        Value::list(vec![
            numbers(&[0.0, 0.0]),
            Value::list(vec![]),
            Value::Number(0.0),
            Value::list(vec![]),
            numbers(&[0.0, 0.0]),
            Value::list(vec![]),
            Value::list(vec![]),
            numbers(&[0.0, 0.0]),
            numbers(&[3.0, 0.0]),
            Value::list(vec![
                Value::list(vec![]),
                Value::list(vec![]),
                Value::list(vec![])
            ]),
            Value::Number(0.0),
        ]),
        "a zero-dimension matrix was not defined"
    );

    for source in [
        "matrix.mean(matrix.zeros(0, 0))",
        "matrix.min(matrix.zeros(0, 0))",
        "matrix.max(matrix.zeros(0, 0))",
    ] {
        let message = runtime_message(&eval_err(source));
        assert!(
            message.contains("empty matrix"),
            "`{source}` should name the empty matrix, got {message}"
        );
    }

    assert_eq!(
        eval("matrix.to_lists(matrix.matmul(matrix.from_lists([[], []]), matrix.zeros(0, 3)))"),
        rows(&[&[0.0, 0.0, 0.0], &[0.0, 0.0, 0.0]]),
        "a 2x0 times a 0x3 matrix should be a 2x3 matrix of zeros"
    );
}

#[test]
fn edge_divide_refuses_a_zero_divisor_but_divides_by_non_zero() {
    let message = runtime_message(&eval_err(
        "matrix.divide(matrix.ones(2, 2), matrix.zeros(2, 2))",
    ));
    assert!(
        message.contains("zero"),
        "a zero divisor should be refused by name, got {message}"
    );

    assert_eq!(
        eval(
            "matrix.to_lists(matrix.divide(matrix.from_lists([[2, 4], [6, 8]]), matrix.from_lists([[2, 2], [2, 2]])))"
        ),
        rows(&[&[1.0, 2.0], &[3.0, 4.0]]),
        "a non-zero divisor should still divide every element"
    );
}

#[test]
fn edge_shape_mismatch_is_refused_and_names_both_shapes() {
    for source in [
        "matrix.add(matrix.from_lists([[1, 2]]), matrix.from_lists([[1], [2]]))",
        "matrix.subtract(matrix.from_lists([[1, 2]]), matrix.from_lists([[1], [2]]))",
        "matrix.multiply(matrix.from_lists([[1, 2]]), matrix.from_lists([[1], [2]]))",
        "matrix.divide(matrix.from_lists([[1, 2]]), matrix.from_lists([[1], [2]]))",
    ] {
        let message = runtime_message(&eval_err(source));
        assert!(
            message.contains("[1, 2]") && message.contains("[2, 1]"),
            "`{source}` should name both shapes, got {message}"
        );
    }

    let message = runtime_message(&eval_err(
        "matrix.matmul(matrix.from_lists([[1, 2]]), matrix.from_lists([[1, 2]]))",
    ));
    assert!(
        message.contains("[1, 2]") && message.contains("[1, 2]"),
        "a matrix product should name both shapes, got {message}"
    );

    let message = runtime_message(&eval_err(
        "matrix.add(matrix.from_lists([[1, 2]]), matrix.from_lists([[1]]))",
    ));
    assert!(
        message.contains("[1, 2]") && message.contains("[1, 1]"),
        "addition must not broadcast a 1x1 matrix, got {message}"
    );
}

#[test]
fn edge_integer_and_float_promotion_does_not_truncate() {
    let value = eval(
        r#"
set ints to matrix.from_lists([[1, 2], [3, 4]])
set halves to matrix.from_lists([[0.5, 0.5], [0.5, 0.5]])
[matrix.to_lists(matrix.add(ints, halves)), matrix.to_lists(matrix.scale(ints, 0.5)), matrix.to_lists(matrix.multiply(ints, halves)), matrix.sum(matrix.scale(ints, 0.5))]
"#,
    );
    assert_eq!(
        value,
        Value::list(vec![
            rows(&[&[1.5, 2.5], &[3.5, 4.5]]),
            rows(&[&[0.5, 1.0], &[1.5, 2.0]]),
            rows(&[&[0.5, 1.0], &[1.5, 2.0]]),
            Value::Number(5.0),
        ]),
        "integer values were truncated instead of promoted to f64"
    );
}

#[test]
fn edge_large_values_lose_f64_precision_and_overflow_is_refused() {
    assert_eq!(
        eval("matrix.to_lists(matrix.from_lists([[9007199254740993]]))"),
        rows(&[&[9007199254740992.0]]),
        "a value past 2^53 should lose the bit f64 cannot hold"
    );
    assert_eq!(
        eval("matrix.sum(matrix.from_lists([[9007199254740992, 1]]))"),
        Value::Number(9007199254740992.0),
        "summing 1 into 2^53 should keep the f64 answer, not invent a bit"
    );
    assert_eq!(
        eval("matrix.sum(matrix.from_lists([[9223372036854775807, 9223372036854775807]]))"),
        Value::Number(18446744073709551616.0),
        "an integer sum past i64::MAX must not wrap to a negative number"
    );

    let message = runtime_message(&eval_err(
        "matrix.scale(matrix.from_lists([[1e308]]), 1e308)",
    ));
    assert!(
        message.contains("not finite"),
        "an overflowing product should be refused, got {message}"
    );
}

#[test]
fn edge_out_of_bounds_and_type_mismatch_are_caught_errors() {
    for source in [
        "matrix.at(matrix.from_lists([[1, 2], [3, 4]]), -1, 0)",
        "matrix.at(matrix.from_lists([[1, 2], [3, 4]]), 2, 0)",
        "matrix.at(matrix.from_lists([[1, 2], [3, 4]]), 0, 999)",
        "matrix.at(matrix.from_lists([[1, 2], [3, 4]]), 0.5, 0)",
        "set m to matrix.from_lists([[1, 2], [3, 4]])\nm[9]",
    ] {
        let message = runtime_message(&eval_err(source));
        assert!(
            message.contains("out of bounds"),
            "`{source}` should be an out-of-bounds error, got {message}"
        );
    }

    let message = runtime_message(&eval_err(
        "matrix.at(matrix.from_lists([[1, 2], [3, 4]]), 0.5, 0)",
    ));
    assert!(
        message.contains("2 rows") && message.contains("whole number"),
        "a fractional row index must name the bound and the required type, got {message}"
    );

    let message = runtime_message(&eval_err(
        "matrix.add(matrix.from_lists([[1]]), \"not a matrix\")",
    ));
    assert!(
        message.contains("Expected a matrix"),
        "a non-matrix argument should be refused, got {message}"
    );
    let message = runtime_message(&eval_err("matrix.from_lists([1, 2])"));
    assert!(
        message.contains("Each matrix row must be a list"),
        "a flat list is not a matrix, got {message}"
    );
    let message = runtime_message(&eval_err("matrix.from_lists([[1], [2, 3]])"));
    assert!(
        message.contains("same length"),
        "a ragged list is not a matrix, got {message}"
    );
}

#[test]
fn edge_indexing_returns_rows_and_single_elements() {
    let value = eval(
        r#"
set m to matrix.from_lists([[1, 2], [3, 4]])
[matrix.at(m, 0, 1), m[1], matrix.row(m, 1), matrix.column(m, 0)]
"#,
    );
    assert_eq!(
        value,
        Value::list(vec![
            Value::Number(2.0),
            numbers(&[3.0, 4.0]),
            numbers(&[3.0, 4.0]),
            numbers(&[1.0, 3.0]),
        ])
    );
}

#[test]
fn edge_dimension_limit_and_bad_dimensions_are_refused() {
    let message = runtime_message(&eval_err("matrix.zeros(100000, 100000)"));
    assert!(
        message.contains("limit is 1000000"),
        "a huge matrix should be refused before allocation, got {message}"
    );
    for source in ["matrix.zeros(100000000, 0)", "matrix.ones(0, 100000000)"] {
        let message = runtime_message(&eval_err(source));
        assert!(
            message.contains("limit is 1000000"),
            "`{source}` should be refused by its dimension before iteration, got {message}"
        );
    }
    for source in [
        "matrix.zeros(-1, 2)",
        "matrix.zeros(1.5, 2)",
        "matrix.identity(-1)",
    ] {
        let message = runtime_message(&eval_err(source));
        assert!(
            message.contains("whole number"),
            "`{source}` should be refused as a dimension, got {message}"
        );
    }
}

#[test]
fn edge_both_engines_produce_equal_matrix_data() {
    let source = r#"
set a to matrix.from_lists([[1, 2], [3, 4]])
set b to matrix.from_lists([[5, 6], [7, 8]])
set m to matrix.add(matrix.matmul(a, b), matrix.transpose(a))
[matrix.to_lists(m), m[1]]
"#;
    let tree = redblue::run_source_value(source).expect("the tree-walker should run it");
    let chunk = redblue::compile_source(source).expect("the compiler should accept it");
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let byte = vm.run(&chunk).expect("the bytecode VM should run it");
    assert_eq!(
        tree, byte,
        "the tree-walker and bytecode VM disagreed about matrix data"
    );
    assert_eq!(
        tree,
        Value::list(vec![
            rows(&[&[20.0, 25.0], &[45.0, 54.0]]),
            numbers(&[45.0, 54.0]),
        ]),
        "the shared answer should be the hand-written one"
    );
}

#[test]
fn edge_mean_of_large_finite_values_stays_finite() {
    assert_eq!(
        eval("matrix.mean(matrix.from_lists([[1e308, 1e308]]))"),
        Value::Number(1e308),
        "the mean of two finite 1e308 values is finite and must not overflow while summing"
    );
    assert_eq!(
        eval("matrix.mean(matrix.from_lists([[-1e308, -1e308]]))"),
        Value::Number(-1e308),
        "a negative large mean must stay finite too"
    );
    assert_eq!(
        eval("matrix.mean(matrix.from_lists([[7]]))"),
        Value::Number(7.0),
        "the mean of a singleton is the element"
    );
    assert_eq!(
        eval("matrix.mean(matrix.from_lists([[1e308, 1e308, -1e308, -1e308, 1e-308]]))"),
        Value::Number(2e-309),
        "the overflow fallback must keep a representable residual after cancellation"
    );
    assert_eq!(
        eval("matrix.mean(matrix.from_lists([[1e308, 1e308, -1e308]]))"),
        Value::Number(1e308 / 3.0),
        "the overflow fallback must divide by a non-unit count, not just scale"
    );
    assert_eq!(
        eval(
            "matrix.mean(matrix.from_lists([[1.7976931348623157e308, 1.7976931348623157e308, 1.7976931348623157e308]]))"
        ),
        Value::Number(f64::MAX),
        "the mean of three f64::MAX values is f64::MAX, not an overflow to infinity"
    );
    assert_eq!(
        eval(
            "matrix.mean(matrix.from_lists([[-1.7976931348623157e308, -1.7976931348623157e308, -1.7976931348623157e308]]))"
        ),
        Value::Number(-f64::MAX),
        "a negative mean at f64::MAX must stay finite with a non-power-of-two count"
    );
}

#[test]
fn edge_mean_sums_before_dividing_for_exact_and_tiny_values() {
    assert_eq!(
        eval("matrix.mean(matrix.ones(1, 10))"),
        Value::Number(1.0),
        "ten ones have a mean of exactly 1, not 0.9999999999999999"
    );
    assert_eq!(
        eval("matrix.mean(matrix.from_lists([[1, 2, 3, 4, 5]]))"),
        Value::Number(3.0),
        "an odd count must still divide after the exact sum"
    );
    assert_eq!(
        eval("matrix.mean(matrix.from_lists([[1e-320, 1e-320, 1e-320]]))"),
        Value::Number(1e-320),
        "tiny finite elements must not underflow when divided one at a time"
    );
}

#[test]
fn edge_non_square_multiply_matches_hand_written_numbers() {
    let value = eval(
        r#"
set a to matrix.from_lists([[1, 2, 3], [4, 5, 6]])
set b to matrix.from_lists([[7, 8], [9, 10], [11, 12]])
set product to matrix.matmul(a, b)
[matrix.shape(product), matrix.to_lists(product)]
"#,
    );
    assert_eq!(
        value,
        Value::list(vec![
            numbers(&[2.0, 2.0]),
            rows(&[&[58.0, 64.0], &[139.0, 154.0]]),
        ]),
        "a 2x3 times a 3x2 product did not match the hand-written answer"
    );
}

#[test]
fn edge_rank_mismatch_is_refused_by_name() {
    let message = runtime_message(&eval_err("matrix.from_lists([1, 2])"));
    assert!(
        message.contains("row") && message.contains("list"),
        "a flat list is not a rank-2 matrix and should be refused by name, got {message}"
    );
    let message = runtime_message(&eval_err("matrix.from_lists([[[1]]])"));
    assert!(
        message.contains("number") && message.contains("list"),
        "a rank-3 row should be refused rather than flattened, got {message}"
    );
}

#[test]
fn edge_sum_keeps_large_f64_magnitudes_past_i64() {
    assert_eq!(
        eval("matrix.sum(matrix.from_lists([[9223372036854775807, 9223372036854775807]]))"),
        Value::Number(18446744073709551616.0),
        "an f64 sum past i64::MAX must keep its positive magnitude, not wrap"
    );
    assert_eq!(
        eval("matrix.sum(matrix.from_lists([[-9223372036854775807, -9223372036854775807]]))"),
        Value::Number(-18446744073709551616.0),
        "an f64 sum below i64::MIN must keep its negative magnitude, not wrap"
    );
    assert_eq!(
        eval(
            "matrix.sum(matrix.from_lists([[1.7976931348623157e308, 1.7976931348623157e308, -1.7976931348623157e308]]))"
        ),
        Value::Number(f64::MAX),
        "a finite sum whose intermediate overflows must not be refused"
    );
    assert_eq!(
        eval(
            "matrix.sum(matrix.from_lists([[-1.7976931348623157e308, -1.7976931348623157e308, 1.7976931348623157e308]]))"
        ),
        Value::Number(-f64::MAX),
        "a negative finite sum whose intermediate overflows must not be refused"
    );
}

#[test]
fn edge_matmul_over_the_work_limit_is_refused() {
    let message = runtime_message(&eval_err(
        "matrix.matmul(matrix.ones(1000, 1000), matrix.ones(1000, 1000))",
    ));
    assert!(
        message.contains("multiply-adds") && message.contains("limit of 500000000"),
        "an oversized product should be refused for its work, got {message}"
    );
    assert!(
        message.contains("[1000, 1000]"),
        "the refusal should name the operand shape, got {message}"
    );
}

#[test]
fn edge_matmul_work_limit_names_both_shapes_for_a_rectangle() {
    let message = runtime_message(&eval_err(
        "matrix.matmul(matrix.ones(800, 700), matrix.ones(700, 900))",
    ));
    assert!(
        message.contains("[800, 700]") && message.contains("[700, 900]"),
        "the refusal should name both operand shapes, got {message}"
    );
    assert!(
        message.contains("multiply-adds"),
        "the refusal should name the work, got {message}"
    );
}

#[test]
fn edge_matmul_with_a_large_inner_dimension_below_the_limit_runs() {
    let value = eval(
        "set a to matrix.ones(1, 1000)\nset b to matrix.ones(1000, 1)\nset product to matrix.matmul(a, b)\n[matrix.shape(product), matrix.at(product, 0, 0)]",
    );
    assert_eq!(
        value,
        Value::list(vec![numbers(&[1.0, 1.0]), Value::Number(1000.0)]),
        "a product with 1,000 inner steps is small work and must still run"
    );
}
