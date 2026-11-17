//! `set x, y to 1, 2` — multiple assignment, as `docs/GRAMMAR.md` § 4.1 writes.
//!
//! The grammar described a declaration that bound several names in one
//! statement, and the parser stopped at the first comma (`ParserError: Expected
//! 'to' but got Comma`). These tests pin the behaviour: every name is bound,
//! all right-hand sides are evaluated before any name is written, and a name
//! count that does not match the value count is refused.

use redblue::{Error, Value};

/// Runs the whole pipeline on `source` and returns the value of its last
/// statement.
#[track_caller]
fn eval(source: &str) -> Value {
    redblue::run_source_value(source)
        .unwrap_or_else(|error| panic!("`{}` should have run, failed with {:?}", source, error))
}

/// [`Value::Number`] for a decimal written as text.
#[track_caller]
fn number(decimal: &str) -> Value {
    Value::Number(
        decimal
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("`{}` should be a decimal number", decimal)),
    )
}

/// The headline claim: two names on the left, two values on the right.
#[test]
fn multiple_assignment_binds_each_name() {
    assert_eq!(
        eval("set x, y to 1, 2\ngive back x + y"),
        number("3"),
        "both names should be bound to their value"
    );
    assert_eq!(
        eval("set x, y to 1, 2\ngive back x"),
        number("1"),
        "the first name should keep the first value"
    );
    assert_eq!(
        eval("set x, y to 1, 2\ngive back y"),
        number("2"),
        "the second name should keep the second value"
    );
}

/// Runs the whole pipeline on `source` and returns the error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    redblue::run_source_value(source).expect_err("`source` should have failed")
}

/// A parse failure is what an unsupported spelling has to produce: not a
/// silently ignored `set`, and not a panic.
#[track_caller]
fn assert_parser_error(source: &str) {
    match eval_err(source) {
        Error::Parser(message, span) => {
            assert!(
                !message.is_empty(),
                "`{}` failed with an empty parser message",
                source
            );
            assert!(span.is_known(), "`{}` failed without a source span", source);
        }
        other => panic!(
            "`{}` should fail with a Parser error, got {:?}",
            source, other
        ),
    }
}

/// Three names and three values, the other size the grammar allows.
#[test]
fn multiple_assignment_three_names() {
    assert_eq!(
        eval("set x, y, z to 1, 2, 3\ngive back x * 100 + y * 10 + z"),
        number("123"),
        "three names should bind in order"
    );
}

/// Every right-hand side is evaluated before any name is written, so a swap
/// exchanges the two values instead of giving both the first one.
#[test]
fn multiple_assignment_evaluates_before_binding() {
    assert_eq!(
        eval("set x, y to 1, 2\nset x, y to y, x\ngive back x * 10 + y"),
        number("21"),
        "the right-hand sides should be read before either name is written"
    );
}

/// A trailing comma after the last name is not in the grammar, so it is refused
/// rather than accepted as an empty extra name.
#[test]
fn edge_trailing_comma_in_names_is_refused() {
    assert_parser_error("set x, y, to 1, 2");
}

/// A trailing comma after the last value has no expression behind it, so it is
/// refused.
#[test]
fn edge_trailing_comma_in_values_is_refused() {
    assert_parser_error("set x, y to 1, 2,");
}

/// More names than values: the missing right-hand side is a parse failure, not
/// a name bound to `nothing`.
#[test]
fn edge_missing_right_hand_side_is_refused() {
    assert_parser_error("set x, y to 1");
}

/// More values than names is the same arity mismatch in the other direction.
#[test]
fn edge_extra_right_hand_side_is_refused() {
    assert_parser_error("set x to 1, 2");
}

/// One name and one value is the ordinary `set`, and still parses as such.
#[test]
fn single_assignment_still_works() {
    assert_eq!(
        eval("set x to 1\ngive back x"),
        number("1"),
        "a `set` with one name should keep its old meaning"
    );
}

/// `import Mod` and `import Mod as Alias` keep working while the dotted form is
/// unsupported.
#[test]
fn existing_import_forms_still_work() {
    assert_eq!(
        eval("import MathUtils as M\nM.circle_area(2)"),
        number("12.56636"),
        "`import a as b` should still bind the alias"
    );
    assert_eq!(
        eval("import MathUtils\nMathUtils.degrees_to_radians(180)"),
        number("3.14159"),
        "`import Mod` should still bind the module's own name"
    );
    assert_eq!(
        eval("import json\njson.stringify(1)"),
        Value::Text("1".to_string()),
        "a builtin namespace should still import without an alias"
    );
}

/// `docs/GRAMMAR.md` § 3.1 marks `import a.b as c` Not Implemented; this is the
/// test that pins the refusal until a phase implements it.
#[test]
fn edge_dotted_import_with_alias_is_refused() {
    assert_parser_error("import math.sqrt as s\nsay s(9)");
}

/// A dotted import with no alias is the same unsupported production.
#[test]
fn edge_dotted_import_without_alias_is_refused() {
    assert_parser_error("import math.sqrt");
}

/// A dotted import naming a member that does not exist is refused by the same
/// parser path, before any module is loaded.
#[test]
fn edge_dotted_import_missing_name_is_refused() {
    assert_parser_error("import math.no_such_member as n\nsay n(9)");
}

/// The bytecode compiler has its own arm for multiple assignment; it has to
/// agree with the tree-walking VM, including the evaluate-before-bind rule.
#[test]
fn multiple_assignment_agrees_with_the_bytecode_vm() {
    let source = "set x, y to 1, 2\nset x, y to y, x\ngive back x * 10 + y\n";
    assert_eq!(eval(source), number("21"));

    let chunk = redblue::bytecode::compile_source(source).expect("source should compile");
    let mut vm = redblue::bytecode::vm::BytecodeVm::new();
    let result = vm.run(&chunk).expect("compiled source should run");
    assert_eq!(
        result,
        number("21"),
        "the bytecode VM should bind multiple names the same way"
    );
}

/// A name repeated on the left has two values that could both win; the grammar
/// does not define that, so the statement is refused.
#[test]
fn edge_duplicate_name_is_refused() {
    assert_parser_error("set x, x to 1, 2");
}
