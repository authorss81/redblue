//! The failure channel: a program that must not run, and the refusal it gets.
//!
//! The other tests in this phase assert that a program **answered** — that it
//! printed a line, or that the value it ended on was the right one. This file
//! asserts the other half of the contract: a program that asks for something
//! the language does not allow must be **refused**, cleanly, on every engine,
//! with a message that names what went wrong.
//!
//! Every assertion here is of the shape "this run came back `Err`", so none of
//! them can pass on a program that answered — not even with the wrong answer.
//! That is the point: a builtin that answers `0` where it should refuse, an
//! index that answers `nothing` where it should be out of bounds, and a program
//! that runs when it should not parse are three different wrong answers, and a
//! test that only ever asks "did it run?" cannot tell them from a correct run.

use redblue::bytecode::vm::BytecodeVm;
use redblue::Error;

/// A builtin given the wrong type is refused by the library, and the refusal
/// names the builtin.
///
/// `sqrt("nine")` has no answer: there is no square root of a word. The
/// library must say so rather than answer `0`, and the answer a caller gets
/// must be a `Result::Err` — a caller can branch on it, and a test can assert
/// on it.
///
/// The programs here are the ones `tests/stdlib_module_docs_test.rs` does
/// **not** already pin. That file owns `sqrt("nine")`, `abs("three")` and
/// `text.split(1, ",")`, and it pins each of them through this same entry
/// point *and* against each engine separately; a second copy of those three
/// would be the same assertion twice, and a reader would have to work out
/// which of the two copies was the one that mattered. These are the other
/// builtins that refuse a wrong type, so the two files together cover the
/// channel rather than repeating it.
#[test]
fn edge_a_builtin_given_the_wrong_type_is_refused_by_the_library() {
    for (source, name) in [
        ("say uppercase(1)", "uppercase"),
        ("say text.uppercase(1)", "uppercase"),
        ("say trim(1)", "trim"),
        ("say round(\"x\")", "round"),
        ("say floor(\"x\")", "floor"),
        ("say ceil(\"x\")", "ceil"),
        ("say log(\"x\")", "log"),
        ("say sin(\"x\")", "sin"),
        ("say pow(\"a\", 2)", "pow"),
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
            error.message().contains(name),
            "`{source}` must name `{name}` in its refusal, got: {}",
            error.message()
        );
    }

    // The refusal is the builtin call's, not the statement's: a builtin that
    // cannot answer refuses the same way inside an expression it is only one
    // term of.
    let result = redblue::run_source("set x to 1 + floor(\"x\")");
    assert!(
        result.is_err(),
        "a builtin that cannot answer must refuse inside an expression too: {result:?}"
    );
    let error = result.expect_err("the refusal was just asserted");
    assert!(
        error.message().contains("floor"),
        "the refusal must name `floor`, got: {}",
        error.message()
    );
}

/// An index past the end of a list is refused, and the refusal says what was
/// out of bounds.
///
/// `x[5]` on a three-element list is a mistake in the program, not a value. The
/// engine must stop rather than answer `nothing` or panic, and the message must
/// carry the index and the length so a reader knows which one was wrong.
#[test]
fn edge_an_index_past_the_end_of_a_list_is_refused() {
    for source in [
        "set x to [1, 2, 3]\nsay x[5]",
        "set x to [1, 2, 3]\nsay x[3]",
        "set x to []\nsay x[0]",
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
    }
}

/// Both engines refuse the same program, and both name the same thing.
///
/// The tree-walker and the bytecode VM are two implementations of one language.
/// A program that one of them refuses and the other answers is a language that
/// changes meaning with its execution strategy — so this asserts the refusal on
/// each engine separately, and that the two refusals agree.
#[test]
fn edge_an_unknown_function_is_refused_by_both_engines() {
    let source = "say nosuchfunction(1)";

    let tokens = redblue::lexer::Lexer::tokenize(source).expect("the fixture lexes");
    let program = redblue::parser::parse(tokens).expect("the fixture parses");

    let (mut walker, walked) = redblue::run_isolated(&program);
    assert!(walked.is_err(), "the walker must refuse `{source}`");
    let walker_error = walked.expect_err("the refusal was just asserted");
    assert!(
        walker_error.message().contains("nosuchfunction"),
        "the walker's refusal must name the function, got: {}",
        walker_error.message()
    );
    assert!(
        walker.take_output().is_empty(),
        "the walker must say nothing before refusing `{source}`"
    );

    let chunk = redblue::bytecode::compile_program(&program).expect("the fixture compiles");
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let vm_result = vm.run(&chunk);
    assert!(vm_result.is_err(), "the bytecode VM must refuse `{source}`");
    let vm_error = vm_result.expect_err("the refusal was just asserted");
    assert_eq!(
        vm_error.message(),
        walker_error.message(),
        "`{source}` must be refused the same way by both engines"
    );
    assert!(
        vm.take_output().is_empty(),
        "the bytecode VM must say nothing before refusing `{source}`"
    );
}

/// A program that cannot be parsed never reaches the VM, and the refusal is a
/// parser error.
///
/// The other three tests in this file are runtime failures: the program was
/// understood and then did something impossible. This one is refused earlier —
/// the source is not a program at all — so the error is a different kind, and a
/// test that only counted "it failed" would not notice the difference.
#[test]
fn edge_a_program_that_cannot_be_parsed_is_refused_before_it_runs() {
    for source in ["say (", "to\n", "set x to"] {
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
