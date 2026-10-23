//! A Redblue program that cannot run has to say which rule it broke.
//!
//! This target exists to hold the assertion that a *refusal* is produced, and it
//! asserts it properly: each case is a real program run through the real pipeline,
//! and each one must come back as the named `Error` variant with a message that
//! names the problem. An earlier version of this file was a `#[should_panic]`
//! test whose entire body was `panic!("intentional")` — it asserted that a panic
//! is a panic, which every Rust program asserts for free, and it would have kept
//! passing if the interpreter had begun accepting every one of the programs below.
//!
//! Two boundaries are here as well as the happy refusals:
//!
//! - **empty** — an empty program is *not* an error. It runs and yields
//!   `Nothing`. If it ever becomes one, the empty case has moved and this says so.
//! - **singleton / out of bounds** — indexing the empty list is index `0` of a
//!   length `0`, which is the boundary of every list index, and it must be a
//!   clean `Runtime` error naming both numbers, not a panic.

use redblue::Error;

/// Run `source` through the whole pipeline and return the `Error` it produced.
///
/// `Err` rather than a `String`: the variant is part of the claim. "It failed
/// somehow" and "the lexer refused it" are different contracts, and a test that
/// only rendered the error would accept both.
fn refusal(source: &str) -> Error {
    let tokens = match redblue::lexer::Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => return error,
    };
    let ast = match redblue::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(error) => return error,
    };
    if let Err(error) = redblue::analyzer::analyze(&ast) {
        return error;
    }
    let (_vm, result) = redblue::run_isolated(&ast);
    result.expect_err("this program is expected to be refused, not to run")
}

fn message(error: &Error) -> String {
    format!("{error}")
}

/// The pipeline refuses at the stage the program breaks at, and says so in
/// words. `fragment` is a lowercase word the message must contain, so a rewording
/// that loses the reason fails rather than passing on "there was an error".
fn assert_refusal(name: &str, source: &str, expect: fn(&Error) -> bool, fragment: &str) {
    let error = refusal(source);
    assert!(
        expect(&error),
        "{name}: expected the stage it breaks at, got `{error}`",
    );
    let message = message(&error).to_lowercase();
    assert!(
        message.contains(fragment),
        "{name}: the refusal `{message}` does not say `{fragment}`. A refusal a \
         reader cannot act on is not a diagnostic."
    );
}

/// (case name, program, which `Error` variant owns it, word the message must carry)
type Case = (&'static str, &'static str, fn(&Error) -> bool, &'static str);

/// Each malformed program is refused by the stage that owns it, with a message
/// that names the rule.
#[test]
fn a_malformed_program_is_refused_by_the_stage_that_owns_it() {
    let cases: &[Case] = &[
        (
            "unterminated string",
            "set x to \"abc\nsay x\n",
            |e| matches!(e, Error::Lexer(..)),
            "unterminated",
        ),
        (
            "unclosed if",
            "if 1 > 0 then\n  say \"hi\"\n",
            |e| matches!(e, Error::Parser(..)),
            "end",
        ),
        (
            "stray close paren",
            "say (1 + 2))\n",
            |e| matches!(e, Error::Parser(..)),
            "unexpected",
        ),
        (
            "unclosed list",
            "set xs to [1, 2, 3\nsay xs\n",
            |e| matches!(e, Error::Parser(..)),
            "unexpected",
        ),
        (
            "adding text to a number",
            "set x to 1 + \"two\"\nsay x\n",
            |e| matches!(e, Error::Runtime(..)),
            "non-numbers",
        ),
        (
            "division by zero",
            "set x to 1 / 0\nsay x\n",
            |e| matches!(e, Error::Runtime(..)),
            "zero",
        ),
    ];

    for (name, source, expect, fragment) in cases {
        assert_refusal(name, source, *expect, fragment);
    }
}

/// The boundary: index `0` of a length-`0` list. It is refused, it names both
/// numbers, and it does not panic — which is what makes it safe to put an
/// out-of-bounds index in a program at all.
#[test]
fn edge_indexing_the_empty_list_is_a_clean_refusal_not_a_panic() {
    let error = refusal("set xs to []\nsay xs[0]\n");
    assert!(
        matches!(error, Error::Runtime(..)),
        "an out-of-bounds index produced `{error}`; it must be a Runtime error, \
         and never a panic."
    );
    let message = message(&error);
    assert!(
        message.contains("out of bounds"),
        "the refusal `{message}` does not say the index was out of bounds. The \
         caller has to be able to tell a bad index from a bad value."
    );
    assert!(
        message.contains("length is 0"),
        "the refusal `{message}` does not give the length it compared against, so \
         a reader cannot check the index without rerunning the program."
    );
}

/// The other boundary: the empty program. It is legal, it runs, and it answers
/// `Nothing` — so a pipeline that started refusing it would be caught here
/// rather than by a user with an empty file.
#[test]
fn edge_an_empty_program_runs_and_answers_nothing() {
    let tokens = redblue::lexer::Lexer::tokenize("").expect("an empty program lexes");
    let ast = redblue::parser::parse(tokens).expect("an empty program parses");
    redblue::analyzer::analyze(&ast).expect("an empty program analyses");
    let (_vm, result) = redblue::run_isolated(&ast);
    let value = result.unwrap_or_else(|error| panic!("an empty program was refused: {error}"));
    assert!(
        matches!(value, redblue::Value::Nothing),
        "an empty program answered `{value:?}`; it answers `Nothing`."
    );
}
