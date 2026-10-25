//! The `regex` module: the crate bound into the language, on both engines.
//!
//! `regex` is already a dependency and already used by the test harness, but
//! before this module a Redblue program parsed, split and validated text with
//! literal separators only. These tests pin the Redblue surface — not the
//! crate's — so a match record, a capture reference and a split are asserted on
//! the value the program receives.

use std::path::Path;

use redblue::bytecode::vm::BytecodeVm;
use redblue::{Error, Value};

#[track_caller]
fn eval(source: &str) -> Value {
    redblue::run_source_value(source)
        .unwrap_or_else(|error| panic!("should have run, failed with {error:?}\n{source}"))
}

#[track_caller]
fn eval_err(source: &str) -> Error {
    redblue::run_source(source).expect_err("source should have failed")
}

#[track_caller]
fn eval_compiled(source: &str) -> Result<Value, Error> {
    let tokens = redblue::lexer::Lexer::tokenize(source)?;
    let program = redblue::parser::parse(tokens)?;
    let chunk = redblue::bytecode::compile_program(&program)?;
    BytecodeVm::new().run(&chunk)
}

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

fn list(items: Vec<Value>) -> Value {
    Value::list(items)
}

/// A match is a record of ordinary data, not an opaque handle: `groups[0]` is
/// the whole match, `groups[1]` the first capture, and `start`/`end` count
/// characters like `length` does.
#[test]
fn edge_regex_match_exposes_the_whole_match_and_its_captures() {
    let value = eval(
        r#"set m to regex.match("héllo id=42", "id=([0-9]+)")
[m.text, m.start, m.finish, m.groups[0], m.groups[1]]"#,
    );
    assert_eq!(
        value,
        list(vec![
            text("id=42"),
            Value::Number(6.0),
            Value::Number(11.0),
            text("id=42"),
            text("42"),
        ])
    );
}

/// A pattern that matches at position zero has `start` 0, and the record is a
/// value rather than an identity: two matches with the same captures compare
/// equal, and a different capture makes them differ.
#[test]
fn edge_regex_match_records_compare_by_their_captures() {
    let value = eval(
        r#"set a to regex.match("id=42", "id=([0-9]+)")
set b to regex.match("id=42", "id=([0-9]+)")
set c to regex.match("id=7", "id=([0-9]+)")
set zero to regex.match("id=42", "^id")
[a is b, a is c, zero.start]"#,
    );
    assert_eq!(
        value,
        list(vec![
            Value::YesNo(true),
            Value::YesNo(false),
            Value::Number(0.0),
        ])
    );
}

/// `regex.split` splits where the pattern matches; a pattern that does not
/// match leaves the text whole. `text.split` keeps its own literal meaning at
/// the same time, which is the reason the two spellings exist.
#[test]
fn edge_regex_split_uses_a_pattern_and_text_split_stays_literal() {
    let value = eval(
        r#"[regex.split("a1b22c", "[0-9]+"), regex.split("abc", ","), text.split("a1b22c", "2")]"#,
    );
    assert_eq!(
        value,
        list(vec![
            list(vec![text("a"), text("b"), text("c")]),
            list(vec![text("abc")]),
            list(vec![text("a1b"), text(""), text("c")]),
        ])
    );
}

/// An empty pattern is valid for `regex.split`: it splits at every character
/// boundary, including the empty text before the first and after the last.
#[test]
fn edge_regex_split_with_an_empty_pattern_keeps_the_empty_parts() {
    assert_eq!(
        eval(r#"regex.split("abc", "")"#),
        list(vec![text(""), text("a"), text("b"), text("c"), text(""),])
    );
}

/// `$1` and `${name}` are capture references, and both spell the captured text
/// rather than the reference.
#[test]
fn edge_regex_replace_expands_numbered_and_named_captures() {
    let value = eval(
        r#"[regex.replace("id=42", "id=([0-9]+)", "user-$1"), regex.replace("id=42", "id=(?P<num>[0-9]+)", "user-$num")]"#,
    );
    assert_eq!(value, list(vec![text("user-42"), text("user-42")]));
}

/// A replacement that names a group the pattern does not have is a catchable
/// `Runtime` failure, not a silently deleted piece of text.
#[test]
fn edge_regex_invalid_group_reference_is_a_catchable_runtime_error() {
    let value = eval(
        r#"set caught to ""
try
    set ignored to regex.replace("id=42", "id=([0-9]+)", "$2")
catch error of Runtime problem
    set caught to problem.message
end
caught"#,
    );
    let Value::Text(message) = value else {
        panic!("the handler did not answer a message: {value:?}");
    };
    assert!(
        message.contains("no capture group") && message.contains('2'),
        "the refusal does not name the missing group: {message}"
    );

    let named = eval(
        r#"set caught to ""
try
    set ignored to regex.replace("id=42", "id=(?P<num>[0-9]+)", "$other")
catch error of Runtime problem
    set caught to problem.message
end
caught"#,
    );
    let Value::Text(message) = named else {
        panic!("the handler did not answer a message: {named:?}");
    };
    assert!(
        message.contains("other"),
        "the refusal does not name the missing capture: {message}"
    );
}

/// A pattern that does not compile is a catchable `Runtime` failure carrying
/// the reason, never a Rust panic.
#[test]
fn edge_regex_invalid_pattern_is_a_catchable_runtime_error() {
    let value = eval(
        r#"set caught to ""
try
    set ignored to regex.is_match("x", "(")
catch error of Runtime problem
    set caught to problem.message
end
caught"#,
    );
    let Value::Text(message) = value else {
        panic!("the handler did not answer a message: {value:?}");
    };
    assert!(
        message.contains("regex pattern") && message.contains("not valid"),
        "the refusal does not say the pattern is invalid: {message}"
    );

    assert!(matches!(
        eval_err(r#"say regex.is_match("x", "(")"#),
        Error::Runtime(_, _)
    ));
}

/// Reading a capture past the pattern's group count is a runtime failure, and a
/// group that did not participate is `nothing` rather than an empty text.
#[test]
fn edge_regex_capture_out_of_bounds_and_non_participating_groups() {
    assert!(matches!(
        eval_err(r#"say regex.capture("id=42", "id=([0-9]+)", 2)"#),
        Error::Runtime(message, _) if message.contains("no capture group 2")
    ));

    assert!(matches!(
        eval_err(r#"say regex.capture("id=42", "id=([0-9]+)", 1e20)"#),
        Error::Runtime(message, _) if message.contains("no capture group")
    ));

    let value = eval(
        r#"set m to regex.match("a", "(a)|(b)")
[m.groups[1], m.groups[2], regex.capture("a", "(a)|(b)", 2)]"#,
    );
    assert_eq!(value, list(vec![text("a"), Value::Nothing, Value::Nothing]));
}

/// A pattern that occurs nowhere answers `nothing`, not a match record with an
/// empty match and not a failure.
#[test]
fn edge_regex_match_answers_nothing_when_the_pattern_does_not_match() {
    let value = eval(
        r#"set m to regex.match("id=42", "user=[0-9]+")
[m, m is nothing, regex.match("id=42", "user=[0-9]+") is nothing]"#,
    );
    assert_eq!(
        value,
        list(vec![Value::Nothing, Value::YesNo(true), Value::YesNo(true),])
    );
}

/// The empty pattern matches at position zero and captures the empty text.
#[test]
fn edge_regex_empty_pattern_matches_at_zero() {
    let value = eval(
        r#"set m to regex.match("abc", "")
[m.text, m.start, m.finish, m.groups[0]]"#,
    );
    assert_eq!(
        value,
        list(vec![
            text(""),
            Value::Number(0.0),
            Value::Number(0.0),
            text(""),
        ])
    );
}

/// Matches do not overlap: `aa` in `aaa` is found once, at the start, so the
/// replacement is `Xa` rather than `XaX`.
#[test]
fn edge_regex_overlapping_matches_are_not_double_counted() {
    let value = eval(r#"[regex.replace("aaa", "aa", "X"), regex.match("aaa", "aa").start]"#);
    assert_eq!(value, list(vec![text("Xa"), Value::Number(0.0)]));
}

/// A non-ASCII pattern and subject are matched by characters, so the record's
/// positions agree with `length`.
#[test]
fn edge_regex_non_ascii_pattern_and_subject() {
    let value = eval(
        r#"set m to regex.match("héllo 世界", "世(界)")
[m.text, m.start, m.finish, m.groups[1], length(m.text)]"#,
    );
    assert_eq!(
        value,
        list(vec![
            text("世界"),
            Value::Number(6.0),
            Value::Number(8.0),
            text("界"),
            Value::Number(2.0),
        ])
    );
}

/// A pattern that backtracks catastrophically in a backtracking engine must not
/// hang here. The `regex` crate is linear-time, so this finishes without a
/// special timeout; if it did not, this test would be the one that stops the
/// suite rather than a Redblue program stopping its user.
#[test]
fn edge_regex_catastrophic_pattern_terminates() {
    let subject = format!("{}!", "a".repeat(10_000));
    let source = format!(r#"regex.is_match("{}", "(a+)+$")"#, subject);
    assert_eq!(eval(&source), Value::YesNo(false));
}

/// A bad argument type is refused by the function's name rather than reaching a
/// dispatcher that says the builtin does not exist.
#[test]
fn edge_regex_type_mismatch_is_refused_by_name() {
    assert!(matches!(
        eval_err("say regex.match(1, \"x\")"),
        Error::Runtime(message, _) if message.contains("regex_match")
    ));
}

/// The tree-walker and the bytecode VM answer the same values for the same
/// regex program, compared as data rather than as "both ran".
#[test]
fn edge_the_two_engines_answer_regex_identically() {
    let source = r#"set m to regex.match("héllo id=42", "id=(?P<num>[0-9]+)")
set same to regex.match("héllo id=42", "id=(?P<num>[0-9]+)")
[m, m is same, m.start, m.finish, m.groups[0], m.groups[1], regex.is_match("id=42", "^id"), regex.replace("id=42", "id=([0-9]+)", "u$1"), regex.split("a1b22c", "[0-9]+"), regex.capture("id=42", "id=([0-9]+)", 1)]"#;

    let walked = eval(source);
    let compiled = eval_compiled(source).expect("the bytecode VM should run the regex program");
    assert_eq!(
        walked, compiled,
        "the two engines answered different regex values"
    );
}

/// Every regex member either document writes exists, and every registered
/// member is written by both documents.
#[test]
fn edge_regex_is_documented_and_documented_functions_are_registered() {
    let expected = ["match", "is_match", "capture", "replace", "split"];
    let members = redblue::stdlib::MODULE_MEMBERS
        .iter()
        .find(|(module, _)| *module == "regex")
        .map(|(_, members)| *members)
        .expect("regex is a registered module");
    assert_eq!(members, &expected);

    for document in ["SPEC.md", "README.md"] {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(document);
        let source = std::fs::read_to_string(&path).expect("the document is readable");
        for member in members {
            assert!(
                source.contains(&format!("regex.{member}")),
                "{document} does not document `regex.{member}`"
            );
        }
    }
}
