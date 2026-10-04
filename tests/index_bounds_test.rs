//! Indexing and field access are total: every shape of index reaches a defined
//! outcome, and none of them is a panic or a silently fabricated value.
//!
//! `items[n]` used to answer an index that names no element with
//! `Value::Nothing`, so `items[len]`, `items[999]` and `items[-99]` were
//! indistinguishable from a list that genuinely held `nothing`, and `items[0.5]`
//! silently answered `items[0]`. This file pins the other half of the contract:
//! an index that names no element is a `Runtime` error naming the index, the
//! list length, and the legal positions — so the fix cannot regress into either
//! a silent `nothing` or an unhelpful message.

use redblue::Error;
use redblue::Value;

/// Runs `source` through lexer → parser → VM and returns the value of its last
/// statement.
///
/// Only a bare expression statement carries a value out: `set x to …` and
/// `say …` both evaluate to `nothing`, so a source ending in one of them would
/// make every assertion below compare `nothing` against `nothing` and pass
/// without testing anything. That is rejected here rather than left to the
/// reader of each test to notice.
#[track_caller]
fn eval(source: &str) -> Value {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let ast = redblue::parser::parse(tokens).expect("source should parse");
    assert!(
        matches!(
            ast.statements.last().map(|s| &s.statement),
            Some(redblue::parser::Statement::Expr(_))
        ),
        "`{}` does not end in a bare expression, so it has no value to assert on",
        source
    );
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

/// Asserts `source` fails at runtime with a message starting with `prefix`.
///
/// Used for the two extremes of a double, whose exact decimal expansion is 300
/// digits wide and says nothing a reader needs.
#[track_caller]
fn assert_runtime_error_starts_with(source: &str, prefix: &str) {
    match eval_err(source) {
        Error::Runtime(message, span) => {
            assert!(
                message.starts_with(prefix),
                "`{}` failed with `{}`, which does not start with `{}`",
                source,
                message,
                prefix
            );
            assert!(message.contains("out of bounds"), "`{}` says why", source);
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

#[test]
fn edge_out_of_bounds_index_is_a_runtime_error_not_a_silent_nothing() {
    // One past the end, far out of bounds, and past the start of a list whose
    // negative indexing is otherwise meaningful. Each of these used to answer
    // `nothing`, which is a value a list can legitimately hold.
    assert_runtime_error(
        "[1, 2, 3][3]",
        "Index 3 is out of bounds: length is 3, valid indexes are 0 to 2",
    );
    assert_runtime_error(
        "[1, 2, 3][999]",
        "Index 999 is out of bounds: length is 3, valid indexes are 0 to 2",
    );
    assert_runtime_error(
        "[1, 2, 3][-99]",
        "Index -99 is out of bounds: length is 3, valid indexes are 0 to 2",
    );
    assert_runtime_error(
        "[10, 20, 30][-5]",
        "Index -5 is out of bounds: length is 3, valid indexes are 0 to 2",
    );
    // A singleton reached past its only element.
    assert_runtime_error(
        "[42][1]",
        "Index 1 is out of bounds: length is 1, valid indexes are 0 to 0",
    );
    // The two extremes of a double are out of bounds of every list, including
    // the empty one, and are reported rather than saturating into a position.
    assert_runtime_error_starts_with("[1, 2, 3][1e308]", "Index ");
    assert_runtime_error_starts_with("[1, 2, 3][-1e308]", "Index ");
    assert_runtime_error_starts_with("[][1e308]", "Index ");
    assert_runtime_error_starts_with("[][-1e308]", "Index ");
}

#[test]
fn indexing_into_an_empty_list_is_a_runtime_error() {
    // The empty list has no legal index at all, so both `0` and `-1` are out of
    // bounds. A negative index on an empty list is the case most likely to be
    // mistaken for "the last element", which does not exist.
    assert_runtime_error(
        "[][0]",
        "Index 0 is out of bounds: length is 0, the list is empty, so it has no valid index",
    );
    assert_runtime_error(
        "[][-1]",
        "Index -1 is out of bounds: length is 0, the list is empty, so it has no valid index",
    );
    assert_runtime_error(
        "[][999]",
        "Index 999 is out of bounds: length is 0, the list is empty, so it has no valid index",
    );
}

#[test]
fn an_index_that_is_not_a_whole_number_is_a_runtime_error() {
    // `items[0.5]` used to truncate to `items[0]`, so a fractional index read
    // an element the program never asked for.
    assert_runtime_error(
        "[10, 20, 30][0.5]",
        "Index 0.5 is out of bounds: a list index must be a whole number",
    );
    // Negative zero is zero, so it names the first element either way round.
    assert_eq!(eval("[10, 20, 30][-0.0]"), Value::Number(10.0));
    assert_eq!(eval("[10, 20, 30][0.0]"), Value::Number(10.0));
    assert_eq!(eval("[10, 20, 30][2.0]"), Value::Number(30.0));
    // A negative fraction is the same defect reached from the other side.
    assert_runtime_error(
        "[10, 20, 30][-0.5]",
        "Index -0.5 is out of bounds: a list index must be a whole number",
    );
    assert_runtime_error(
        "[10, 20, 30][-2.5]",
        "Index -2.5 is out of bounds: a list index must be a whole number",
    );
}

#[test]
fn a_legal_index_on_every_boundary_of_the_list_still_answers_the_element() {
    // The fix must not take the legal positions with it: `0` is the first
    // element, `-1` the last, and `len - 1` the same element written positively.
    assert_eq!(eval("[10, 20, 30][0]"), Value::Number(10.0));
    assert_eq!(eval("[10, 20, 30][2]"), Value::Number(30.0));
    assert_eq!(eval("[10, 20, 30][-1]"), Value::Number(30.0));
    assert_eq!(eval("[10, 20, 30][-3]"), Value::Number(10.0));
    assert_eq!(eval("[10, 20, 30][1.0]"), Value::Number(20.0));
    // A singleton list has one legal index, reachable both ways.
    assert_eq!(eval("[42][0]"), Value::Number(42.0));
    assert_eq!(eval("[42][-1]"), Value::Number(42.0));
    // A list element that is genuinely `nothing` still reads back as
    // `nothing` — the error is about the position, never the value stored
    // there, and a two-deep walk through it is legal.
    assert_eq!(eval("[[nothing]][0][0]"), Value::Nothing);
    assert_eq!(eval("[]"), Value::List(vec![]));
}

#[test]
fn edge_an_out_of_bounds_error_names_the_index_it_was_given() {
    // A message that does not say which index failed cannot be acted on. The
    // index is named as the program wrote it, not as the offset it resolved to,
    // so `items[-99]` does not report itself as index -96.
    assert_eq!(
        runtime_message("[1, 2, 3][999]"),
        "Index 999 is out of bounds: length is 3, valid indexes are 0 to 2"
    );
    assert_eq!(
        runtime_message("[1, 2, 3][-99]"),
        "Index -99 is out of bounds: length is 3, valid indexes are 0 to 2"
    );
    assert_eq!(
        runtime_message("[][0]"),
        "Index 0 is out of bounds: length is 0, the list is empty, so it has no valid index"
    );
    // A unicode list is measured in elements, not bytes, so the length named
    // is the number of elements and the index arithmetic is unaffected.
    assert_eq!(
        runtime_message("[\"héllo\", \"日本語\"][5]"),
        "Index 5 is out of bounds: length is 2, valid indexes are 0 to 1"
    );
}

#[test]
fn a_nested_index_out_of_bounds_reports_the_index_that_failed() {
    // The failure belongs to the inner list, so it is the inner index that is
    // named and the inner length that is measured.
    assert_eq!(
        runtime_message("set outer to [[1, 2, 3]]\nsay outer[0][5]"),
        "Index 5 is out of bounds: length is 3, valid indexes are 0 to 2"
    );
    // The outer index is out of bounds here, so the outer length is measured.
    assert_eq!(
        runtime_message("set outer to [[1, 2, 3]]\nsay outer[7][0]"),
        "Index 7 is out of bounds: length is 1, valid indexes are 0 to 0"
    );
    // A list nested directly in a list, indexed straight out of the literal.
    assert_eq!(
        runtime_message("[[1]][0][1]"),
        "Index 1 is out of bounds: length is 1, valid indexes are 0 to 0"
    );
    // Nesting three deep still names the innermost failure, and the two legal
    // levels above it are silent.
    assert_eq!(eval("[[[7]]][0][0][0]"), Value::Number(7.0));
    assert_eq!(
        runtime_message("[[[7]]][0][0][1]"),
        "Index 1 is out of bounds: length is 1, valid indexes are 0 to 0"
    );
}

#[test]
fn indexing_a_non_list_or_a_field_of_a_non_record_is_a_runtime_error() {
    // Every shape a value can take, indexed. None is a panic.
    assert_runtime_error("5[0]", "Cannot index non-list");
    assert_runtime_error("\"hello\"[0]", "Cannot index non-list");
    assert_runtime_error("set r to {a: 1}\nsay r[0]", "Cannot index non-list");
    assert_runtime_error("set y to yes\nsay y[0]", "Cannot index non-list");
    // And the same shapes read as a field. None is a panic.
    assert_runtime_error(
        "set t to \"hello\"\nsay t.length",
        "Cannot access property on non-object",
    );
    assert_runtime_error(
        "set l to [1, 2]\nsay l.first",
        "Cannot access property on non-object",
    );
    assert_runtime_error(
        "set n to 5\nsay n.length",
        "Cannot access property on non-object",
    );
    // A non-number index never reaches the bounds check.
    assert_runtime_error("set l to [1, 2]\nsay l[\"a\"]", "Index must be a number");
    assert_runtime_error("set l to [1, 2]\nsay l[nothing]", "Index must be a number");
    assert_runtime_error("set l to [1, 2]\nsay l[yes]", "Index must be a number");
}

#[test]
fn a_missing_record_key_answers_nothing_and_a_duplicate_key_keeps_the_last() {
    // A missing key is `nothing`: the defined answer, chosen so that reading an
    // absent field is not itself an error and a record can be walked without
    // testing every key first. It is distinguishable from an out-of-bounds
    // list index because the two are different operations with different
    // messages.
    assert_eq!(eval("set r to {a: 1}\nr.missing"), Value::Nothing);
    assert_eq!(eval("set r to {}\nr.a"), Value::Nothing);
    // A key that was never written and one written to `nothing` are the same
    // value, which is the documented consequence of the choice above.
    assert_eq!(eval("set r to {a: nothing}\nr.a"), eval("set r to {}\nr.a"));
    // Repeated keys keep the last value; the record holds exactly one `a`.
    assert_eq!(eval("set r to {a: 1, a: 2}\nr.a"), Value::Number(2.0));
    // A record reached through a legal index, missing a key.
    assert_eq!(eval("set l to [{a: 1}]\nl[0].b"), Value::Nothing);
    // An empty record literal and a field read out of it.
    assert_eq!(eval("set r to {}\nr"), Value::Record(Default::default()));
}
