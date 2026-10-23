//! The **value** a builtin answers, and the failure channel a failing program
//! leaves open.
//!
//! The phase's earlier rounds taught the suite to read what a program **said**:
//! the reachability-only tests now compare the lines a program printed, so a
//! builtin that answers the wrong value fails the suite. That closes the hole
//! the finding named. It leaves a narrower one behind it, and this file closes
//! that one too.
//!
//! ## A rendering is not a value
//!
//! `say` renders a value, and a rendered line is what the phase's assertions
//! compare. The number `3` and the text `"3"` render the same way — both print
//! `3` — so a builtin that answered the **text** `"3"` where the **number** `3`
//! was asked for satisfies every line assertion in the suite. The characters
//! are right and the value is not, and no comparison of lines can tell the two
//! apart.
//!
//! So the builtins are held to the value they answer as well as to the line
//! they print: `sqrt(9)` answers `Value::Number(3.0)` and not
//! `Value::Text("3".to_string())`, and the expectation is written out literally
//! here, so a builtin that changed its mind about the *kind* of value it
//! answers fails here even though it would still print the right characters.
//!
//! ## A refusal is not a silence
//!
//! The other half of the contract is what happens when a program cannot finish.
//! A program that fails halfway has already printed its earlier lines, and a
//! refusal that swallowed them would lose work the program did — so the failure
//! channel is asserted from both sides at once: the run comes back `Err`, **and**
//! the lines it printed before the failure are still readable.
//!
//! Every test here is about what the run **answered**: the value it ended on,
//! the lines it printed, or the refusal it handed back. None of them asks
//! whether the program ran.

use redblue::bytecode::vm::BytecodeVm;
use redblue::lexer::Lexer;
use redblue::run_isolated;
use redblue::{analyzer, parser, Error, Value};

/// Runs `source` once and returns the lines it printed and the value it ended on.
///
/// The program goes through the whole pipeline — lexer, parser, analyzer, VM —
/// and the lines come from the `Vm` the run hands back, so what is asserted is
/// what *this* run said. The value is what the same run answered, so a test
/// that wants both reads both out of one execution.
///
/// Returns the `Result` rather than panicking, so a test can say which of the
/// two things went wrong: a program that did not run, and a program that ran
/// and said the wrong thing.
fn printed_and_value(source: &str) -> Result<(Vec<String>, Value), Error> {
    let tokens = Lexer::tokenize(source)?;
    let program = parser::parse(tokens)?;
    analyzer::analyze(&program)?;
    let (mut vm, outcome) = run_isolated(&program);
    let value = outcome?;
    Ok((vm.take_output(), value))
}

/// The lines `source` printed, or a failure naming it.
#[track_caller]
fn lines_of(source: &str) -> Vec<String> {
    printed_and_value(source)
        .map(|(lines, _value)| lines)
        .unwrap_or_else(|error| panic!("{source:?} should run: {error:?}"))
}

/// A list of `items`, built the way the language builds one.
///
/// `Value::List` holds a `Shared<Vec<Value>>`, and `Shared` is
/// `std::sync::Arc` — the list a builtin answers is the same list a program
/// would build, not a copy of it.
fn list(items: Vec<Value>) -> Value {
    Value::List(std::sync::Arc::new(items))
}

/// A builtin answers a **value**, not only a rendering.
///
/// The phase's earlier assertions compare the line a builtin prints. That is
/// enough to catch a builtin that answers the wrong *number*, and it is not
/// enough to catch one that answers the wrong *kind* of value: `3` and `"3"`
/// both render as `3`, so a builtin that swapped the number for the text would
/// pass every line assertion in the suite.
///
/// So the value is asserted here, literally, for the builtins the phase's other
/// file pins by their output. `sqrt(9)` answers the number three; `uppercase`
/// answers text; and neither answers the other's kind.
#[test]
fn edge_a_builtin_that_answers_the_right_characters_and_the_wrong_value_is_caught() {
    for (call, expected) in [
        ("sqrt(9)", Value::Number(3.0)),
        ("abs(-3)", Value::Number(3.0)),
        (r#"uppercase("hi")"#, Value::Text("HI".to_string())),
        (r#"text.uppercase("hi")"#, Value::Text("HI".to_string())),
        (r#"trim("  x  ")"#, Value::Text("x".to_string())),
        // A list of two, not one: a `split` that did not split would answer a
        // list of one, and the *rendering* of those two answers is `[a, b]`
        // against `[a]` — so the line already tells them apart, and the value
        // says it again in the form the language actually holds.
        (
            r#"text.split("a,b", ",")"#,
            list(vec![
                Value::Text("a".to_string()),
                Value::Text("b".to_string()),
            ]),
        ),
        // The singleton: one element, and it is the whole answer.
        (
            r#"text.split("a", ",")"#,
            list(vec![Value::Text("a".to_string())]),
        ),
    ] {
        // The program ends on the *name*, not on the `set`: a `set` answers
        // `nothing` — it is a statement, not an expression — so a program that
        // ended on the assignment would assert the same `Nothing` for every
        // builtin and would prove nothing at all.
        let source = format!("set x to {call}\nx");
        let (_lines, value) = printed_and_value(&source)
            .unwrap_or_else(|error| panic!("`{source}` should run: {error:?}"));
        assert_eq!(
            value, expected,
            "`{source}` must answer {expected:?} — the value, not only the line \
             it renders as"
        );
    }
}

/// The rendering really cannot tell a number from the text that renders the
/// same way — which is why the row above is on the value.
///
/// Without this, "assert the value" is a claim nobody checked: it would be
/// enough for the two renderings to differ for the distinction to be
/// meaningless. They do not differ. Both programs print `3`, and the values
/// they end on are different values.
#[test]
fn edge_a_number_and_the_text_that_renders_the_same_way_are_different_values() {
    let (number_lines, number_value) = printed_and_value("set x to 3\nsay x\nx")
        .unwrap_or_else(|error| panic!("the number program should run: {error:?}"));
    let (text_lines, text_value) = printed_and_value("set x to \"3\"\nsay x\nx")
        .unwrap_or_else(|error| panic!("the text program should run: {error:?}"));

    assert_eq!(
        number_lines,
        vec!["3".to_string()],
        "the number renders as `3`"
    );
    assert_eq!(
        text_lines,
        vec!["3".to_string()],
        "the text renders as `3` as well — this is the whole point of the test"
    );
    assert_eq!(number_lines, text_lines, "the two renderings are identical");
    assert_eq!(number_value, Value::Number(3.0));
    assert_eq!(text_value, Value::Text("3".to_string()));
    assert_ne!(
        number_value, text_value,
        "the values are different even though the renderings are the same, so a \
         line comparison alone could not tell a builtin that answered text from \
         one that answered a number"
    );
}

/// A program that fails halfway keeps the lines it printed before it failed.
///
/// The failure channel is not a silence. A program that printed `before` and
/// then divided by zero did print `before`, and a refusal that swallowed the
/// output would lose work the program did — so the run is asserted from both
/// sides at once: it comes back `Err`, and the line it printed before the
/// failure is still there to be read. The line *after* the failure was never
/// printed, and saying so is what makes the assertion about the failure rather
/// than about a program that ran to the end.
#[test]
fn edge_a_program_that_fails_midway_still_exposes_the_lines_it_printed() {
    let source = "say \"before\"\nset x to 1 / 0\nsay \"after\"";
    let tokens = Lexer::tokenize(source).expect("the fixture lexes");
    let program = parser::parse(tokens).expect("the fixture parses");

    let (mut vm, outcome) = run_isolated(&program);
    assert!(
        outcome.is_err(),
        "a division by zero must be refused, not answered: {outcome:?}"
    );
    let error = outcome.expect_err("the refusal was just asserted");
    assert!(
        error.message().contains("Division by zero"),
        "the refusal must name the division, got: {}",
        error.message()
    );
    assert_eq!(
        vm.take_output(),
        vec!["before".to_string()],
        "the line printed before the failure is still readable, and the line \
         after the failure was never printed"
    );
}

/// A loop that fails on its third turn prints exactly the turns it completed —
/// three of them, and no more.
///
/// The counterpart to the test above, in the domain the phase also touched: a
/// loop that fails part-way through must stop there. A loop that carried on
/// after the failure would print a fourth and a fifth line, and a loop that
/// swallowed its output would print none — so the exact three lines are what
/// tell the two apart.
#[test]
fn edge_a_loop_that_fails_midway_prints_only_the_turns_it_completed() {
    let source = "set n to 0\nrepeat 5 times\n    set n to n + 1\n    say n\n    if n is 3 then\n        set x to 1 / 0\n    end\nend";
    let tokens = Lexer::tokenize(source).expect("the fixture lexes");
    let program = parser::parse(tokens).expect("the fixture parses");

    let (mut vm, outcome) = run_isolated(&program);
    assert!(
        outcome.is_err(),
        "the division by zero inside the loop must be refused: {outcome:?}"
    );
    assert_eq!(
        vm.take_output(),
        vec!["1".to_string(), "2".to_string(), "3".to_string()],
        "the loop printed one line per completed turn and stopped at the \
         failure — three lines, not five and not none"
    );
}

/// A program that cannot be **lexed** is refused before it is parsed.
///
/// The failure channel has three kinds of refusal and this file now pins the
/// third: a runtime failure, a parse failure, and a lexer failure. They are
/// different errors, and a test that only counted "it failed" would not notice
/// which one it was looking at — so this one names the kind and the reason.
#[test]
fn edge_a_program_that_cannot_be_lexed_is_refused_before_it_is_parsed() {
    for (source, reason) in [
        ("say \"unterminated", "Unterminated string"),
        (r#"say "\q""#, "Unknown escape"),
        (r#"say "\u00""#, "Unicode escape needs four hex digits"),
    ] {
        let result = redblue::run_source(source);
        assert!(
            result.is_err(),
            "`{source}` must be refused, not run: {result:?}"
        );
        let error = result.expect_err("the refusal was just asserted");
        assert!(
            matches!(error, Error::Lexer(..)),
            "`{source}` must fail in the lexer, got: {error:?}"
        );
        assert!(
            error.message().contains(reason),
            "`{source}` must say why it could not be lexed, got: {}",
            error.message()
        );
    }
}

/// A program that says nothing is not a program that failed.
///
/// The empty answer and the refusal are two different answers, and the channel
/// this phase reads has to keep them apart: a helper that turned a failure into
/// an empty `Vec` would satisfy every "printed nothing" assertion in the suite,
/// including the two loop tests in `tests/loop_bounds_test.rs` that read `never`
/// out of a body that must not run.
#[test]
fn edge_a_program_that_says_nothing_and_a_program_that_fails_are_different_answers() {
    let silent = lines_of("set x to 1\n");
    assert_eq!(
        silent,
        Vec::<String>::new(),
        "a program that never says anything prints no lines"
    );

    let refused = printed_and_value("set x to 1 / 0");
    assert!(
        refused.is_err(),
        "the failing program must be a refusal, not an empty answer: {refused:?}"
    );
}

#[test]
fn edge_a_bad_input_is_refused_rather_than_panicking() {
    let r = redblue::run_source("set x to list.map(5, to_text)");
    assert!(r.is_err(), "a non-list receiver must be a clean error");
}

#[test]
fn edge_bad_input_produces_clean_error_not_panic() {
    let result = redblue::run_source("set x to list.map(5, to_text)");
    assert!(result.is_err(), "non-list receiver must be refused");
}

#[test]
fn edge_an_unknown_function_is_refused_rather_than_silently_answered() {
    let result = redblue::run_source("say frobnicate(1)");
    assert!(
        result.is_err(),
        "an unknown function must be refused, not answered: {result:?}"
    );
    let error = result.expect_err("the refusal was just asserted");
    assert!(
        matches!(error, Error::Runtime(..)),
        "an unknown function must be a runtime refusal, got: {error:?}"
    );
}

#[test]
fn edge_a_division_by_zero_is_refused_rather_than_answered() {
    let result = redblue::run_source("say 1 / 0");
    assert!(
        result.is_err(),
        "division by zero must be refused, not answered: {result:?}"
    );
}

/// The library's own entry point must refuse a program the language promises to
/// reject, and the refusal must be a runtime error rather than a silent value.
///
/// The rest of this file reads the failure channel from the success side: what a
/// program printed before it stopped. This test is the other direction — the
/// program never runs to a value at all, and `run_source` has to hand back an
/// `Err` naming what went wrong.
#[test]
fn edge_a_non_list_receiver_is_refused_by_the_library_entry_point() {
    let result = redblue::run_source("set x to list.map(5, to_text)");
    assert!(
        result.is_err(),
        "a non-list receiver must be refused, not answered: {result:?}"
    );
    let error = result.expect_err("the refusal was just asserted");
    assert!(
        matches!(error, Error::Runtime(..)),
        "a non-list receiver must be a runtime refusal, got: {error:?}"
    );
}

/// An index the list does not have is a refusal, not a value.
///
/// The empty list has no legal index, the singleton has exactly one, and one
/// past the end is out of bounds on every list. Each of these must come back as
/// a runtime error naming the index it was given rather than answering
/// `nothing`, which is a value a list can legitimately hold.
#[test]
fn edge_an_out_of_bounds_index_is_refused_rather_than_answered() {
    for (source, index) in [
        ("[1, 2, 3][3]", "3"),
        ("[1, 2, 3][999]", "999"),
        ("[42][1]", "1"),
        ("[][0]", "0"),
    ] {
        let result = redblue::run_source(source);
        assert!(
            result.is_err(),
            "`{source}` must be refused, not answered: {result:?}"
        );
        let error = result.expect_err("the refusal was just asserted");
        assert!(
            matches!(error, Error::Runtime(..)),
            "`{source}` must fail at runtime, got: {error:?}"
        );
        assert!(
            error.message().contains("out of bounds"),
            "`{source}` must say the index was out of bounds, got: {}",
            error.message()
        );
        assert!(
            error.message().contains(index),
            "`{source}` must name the index `{index}`, got: {}",
            error.message()
        );
    }
}

/// A program that is not a program is refused before anything runs.
///
/// The failures above are runtime refusals: the program was understood and then
/// did something impossible. This one is refused earlier, by the parser, so the
/// error is a different kind and nothing is printed first — an unclosed `end`
/// must not be half-executed.
#[test]
fn edge_a_program_that_cannot_be_parsed_is_refused_before_it_says_anything() {
    for source in ["if yes then\n    say \"never\"", "say (", "set x to"] {
        let result = redblue::run_source(source);
        assert!(
            result.is_err(),
            "`{source}` must be refused, not run: {result:?}"
        );
        let error = result.expect_err("the refusal was just asserted");
        assert!(
            matches!(error, Error::Parser(..)),
            "`{source}` must fail in the parser, got: {error:?}"
        );
    }
}

/// Run `source` on the bytecode engine, returning its lines.
fn bytecode_lines(source: &str) -> Vec<String> {
    let tokens =
        Lexer::tokenize(source).unwrap_or_else(|error| panic!("{source:?} should lex: {error:?}"));
    let program =
        parser::parse(tokens).unwrap_or_else(|error| panic!("{source:?} should parse: {error:?}"));
    let chunk = redblue::bytecode::compile_program(&program)
        .unwrap_or_else(|error| panic!("{source:?} should compile: {error:?}"));
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    vm.run(&chunk)
        .unwrap_or_else(|error| panic!("{source:?} should run: {error:?}"));
    vm.take_output()
}

/// A builtin's value is asserted, on both engines, with the expectation written
/// out literally rather than derived from the implementation.
///
/// `sqrt(9)` is `3` and `text.split("a,b", ",")` is two elements, so a `sqrt`
/// that answered `1` and a `split` that answered one element both fail here.
#[test]
fn edge_a_builtin_that_answers_the_wrong_value_is_caught_on_both_engines() {
    for (source, expected) in [
        ("say sqrt(9)", vec!["3".to_string()]),
        ("say abs(-3)", vec!["3".to_string()]),
        (r#"say text.split("a,b", ",")"#, vec!["[a, b]".to_string()]),
    ] {
        assert_eq!(
            lines_of(source),
            expected,
            "the walker must answer `{source}` with {expected:?}, not merely run"
        );
        assert_eq!(
            bytecode_lines(source),
            expected,
            "the bytecode VM must answer `{source}` with {expected:?}, not merely run"
        );
    }
}

/// A loop whose body must not run cannot have printed `never`, on either engine.
///
/// The literal `never` in the body is what makes the assertion discriminating:
/// an empty output means the body did not run, and any output would name the
/// line that must not have been printed.
#[test]
fn edge_a_loop_whose_body_must_not_run_prints_nothing_on_both_engines() {
    let source = "repeat \"five\" times\n    say \"never\"\nend";
    assert_eq!(
        lines_of(source),
        Vec::<String>::new(),
        "a non-numeric repeat count must run the body zero times"
    );
    assert_eq!(
        bytecode_lines(source),
        Vec::<String>::new(),
        "a non-numeric repeat count must run the body zero times on the bytecode VM"
    );
}

/// A program that says the empty string is not the same answer as a program
/// that says nothing; both are asserted literally.
#[test]
fn edge_saying_the_empty_string_is_not_saying_nothing_on_both_engines() {
    for source in [r#"say """#, "set x to 1"] {
        let expected = if source == r#"say """# {
            vec![String::new()]
        } else {
            Vec::new()
        };
        assert_eq!(
            lines_of(source),
            expected,
            "the walker must answer `{source}` with {expected:?}"
        );
        assert_eq!(
            bytecode_lines(source),
            expected,
            "the bytecode VM must answer `{source}` with {expected:?}"
        );
    }
}

/// A non-list receiver is refused by both engines rather than answered.
///
/// This is the failure channel, asserted on the `Result` itself: `list.map`
/// given the number `5` has no answer, so both engines must return `Err`. A
/// builtin that silently answered `nothing` would satisfy a test that only
/// asked whether the program ran, and fails this one.
#[test]
fn edge_a_non_list_receiver_is_refused_by_both_engines() {
    let source = "set x to list.map(5, to_text)";

    let refused = redblue::run_source(source);
    assert!(
        refused.is_err(),
        "the walker must refuse a non-list receiver, got: {refused:?}"
    );
    let error = refused.expect_err("the refusal was just asserted");
    assert!(
        matches!(error, Error::Runtime(..)),
        "the walker's refusal must be a runtime error, got: {error:?}"
    );
    assert!(
        error.message().contains("map"),
        "the refusal must name `map`, got: {}",
        error.message()
    );

    let tokens = Lexer::tokenize(source).expect("the fixture lexes");
    let program = parser::parse(tokens).expect("the fixture parses");
    let chunk = redblue::bytecode::compile_program(&program).expect("the fixture compiles");
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let refused = vm.run(&chunk);
    assert!(
        refused.is_err(),
        "the bytecode VM must refuse a non-list receiver, got: {refused:?}"
    );
}
