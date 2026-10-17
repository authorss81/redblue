//! The five programs a newcomer meets first, and the exact lines each one prints.
//!
//! These tests used to be five `assert!(run_source(src).is_ok())`. `run_source`
//! answers `Result<(), Error>` — it maps the program's value away at
//! `src/lib.rs`, `result.map(|_| ())` — so "the program ran" was all any of them
//! could say. A `say` that printed the wrong number, or a function that greeted
//! nobody, passed all five.
//!
//! So each one asserts the lines. The run itself goes through
//! [`redblue::run_isolated`], which is the same lexer, parser, analyzer and VM
//! `run_source` runs and which hands back the `Vm` — and `Vm::take_output` is how
//! a caller reads what the program said. `run_source` is still covered: the
//! helper below calls it first, so a program that no longer runs at all fails
//! here for the reason it used to.

use redblue::lexer::Lexer;
use redblue::run_isolated;
use redblue::{analyzer, parser, run_source, Error, Value};

/// Runs `source` the way `run_source` does and returns the lines it printed.
///
/// Returns the `Result` rather than panicking, so a test can say which of the two
/// things went wrong: a program that did not run, and a program that ran and said
/// the wrong thing.
fn printed(source: &str) -> Result<Vec<String>, Error> {
    run_source(source)?;

    let tokens = Lexer::tokenize(source)?;
    let program = parser::parse(tokens)?;
    analyzer::analyze(&program)?;
    let (mut vm, outcome) = run_isolated(&program);
    outcome?;
    Ok(vm.take_output())
}

/// The lines `source` printed, or a failure naming it.
#[track_caller]
fn lines_of(source: &str) -> Vec<String> {
    printed(source).unwrap_or_else(|error| panic!("{source:?} should run: {error:?}"))
}

#[test]
fn test_hello_world() {
    assert_eq!(
        lines_of(r#"say "Hello, World!""#),
        vec!["Hello, World!".to_string()],
    );
}

#[test]
fn test_numbers() {
    assert_eq!(
        lines_of(
            r#"
        set x to 10
        set y to 20
        say x
    "#,
        ),
        vec!["10".to_string()],
    );
}

#[test]
fn test_math() {
    assert_eq!(
        lines_of(
            r#"
        set x to 10
        set y to 20
        set z to x + y
        say z
    "#,
        ),
        vec!["30".to_string()],
    );
}

#[test]
fn test_list() {
    assert_eq!(
        lines_of(
            r#"
        set items to [1, 2, 3]
        say items
    "#,
        ),
        vec!["[1, 2, 3]".to_string()],
    );
}

#[test]
fn test_function() {
    assert_eq!(
        lines_of(
            r#"
        to greet(name)
            say "Hello, {name}!"
        end
        greet("World")
    "#,
        ),
        vec!["Hello, World!".to_string()],
    );
}

/// The value of the last statement, not just the line it printed.
///
/// `say` renders a value; the run hands one back. A `say` that rendered `10` and a
/// `10` that was a **text** print identically, and this is the row that tells them
/// apart — the same reason `tests/common/vm.rs` tags its golden values.
#[test]
fn edge_the_value_a_program_ends_on_is_a_number_and_not_its_rendering() {
    let source = r#"
        set x to 10
        say x
        x
    "#;
    assert_eq!(lines_of(source), vec!["10".to_string()]);

    let tokens = Lexer::tokenize(source).expect("the fixture lexes");
    let program = parser::parse(tokens).expect("the fixture parses");
    analyzer::analyze(&program).expect("the fixture analyzes");
    let (_, outcome) = run_isolated(&program);
    assert_eq!(
        outcome.expect("the fixture runs"),
        Value::Number(10.0),
        "the program's value is the number, not the text that renders as `10`"
    );

    // And the text that prints the same way is a different value.
    let source = r#"
        set x to "10"
        say x
        x
    "#;
    assert_eq!(lines_of(source), vec!["10".to_string()]);
    let tokens = Lexer::tokenize(source).expect("the fixture lexes");
    let program = parser::parse(tokens).expect("the fixture parses");
    analyzer::analyze(&program).expect("the fixture analyzes");
    let (_, outcome) = run_isolated(&program);
    assert_eq!(
        outcome.expect("the fixture runs"),
        Value::Text("10".to_string()),
        "`10` as text is not `10` as a number, even though both print alike"
    );
}

/// A program whose correct output is the **empty string** is a real program, and
/// the empty answer has to be pinned.
///
/// Two shapes, because they are two different claims and `take_output` cannot tell
/// them apart: a program that says one empty line (`say ""`) and a program that
/// says nothing at all. Both answer an empty `Vec` of *lines*, and each is
/// asserted on its own terms.
#[test]
fn edge_an_empty_answer_is_pinned_for_both_shapes_of_empty() {
    // One empty line: a line that is present and has nothing in it.
    assert_eq!(
        lines_of(r#"say """#),
        vec![String::new()],
        "`say \"\"` prints one line and that line is empty"
    );

    // Nothing at all: a program with no `say` says nothing, which is a different
    // answer from an empty line and is what the loop tests above rely on when
    // they read `never`.
    assert_eq!(
        lines_of("set x to 1\nset y to 2\n"),
        Vec::<String>::new(),
        "a program that never says anything prints no lines"
    );
}

/// The five tests above are the whole file, and they must not regress into
/// "it ran". This is the assertion that they would notice.
///
/// It runs the *same* helper over one program and over a program that says
/// something else, and requires that the two differ. If the helper stopped reading
/// output and returned an empty `Vec` for everything — which is exactly what
/// `assert!(run_source(..).is_ok())` reduced to — every program would answer
/// `[]`, this would fail, and the five tests above would be visibly vacuous rather
/// than quietly so.
#[test]
fn edge_a_program_that_says_something_differs_from_one_that_does_not() {
    let silent = lines_of("set x to 1\n");
    let loud = lines_of("say \"a line\"\nset x to 1\n");

    assert_eq!(silent, Vec::<String>::new());
    assert_eq!(loud, vec!["a line".to_string()]);
    assert_ne!(
        silent, loud,
        "if these are equal the helper is not reading output, and every \
         assertion built on it passes by default"
    );
}

/// A program that fails is a **failure**, not an empty answer.
///
/// `run_source` is `Result<(), Error>` and the helper keeps that half, so the
/// channel this phase is about has to be asserted from both sides: a program that
/// runs and says something, and a program that does not run at all. A helper that
/// turned a failure into an empty `Vec` would satisfy every "printed nothing"
/// assertion in the suite.
#[test]
fn edge_a_program_that_cannot_run_reports_a_failure_rather_than_no_output() {
    let failure = printed("say nosuchfunction(1)");
    let error = match failure {
        Ok(lines) => {
            panic!("`say nosuchfunction(1)` must be refused, and it printed {lines:?} instead")
        }
        Err(error) => error,
    };
    assert!(
        error.message().contains("nosuchfunction"),
        "the refusal must name what it could not find, got: {}",
        error.message()
    );

    // The same helper on a program that does run, so the failure above is a
    // failure and not the helper's idea of an empty answer.
    assert_eq!(
        lines_of("say \"fine\""),
        vec!["fine".to_string()],
        "the helper must still read output from a program that runs"
    );
}
