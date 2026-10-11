//! `catch error <name>` — the spelled-out binder SPEC.md § Error Handling writes.
//!
//! `src/parser.rs` read the word after `catch` as the name it binds whenever it
//! lexed to an identifier, and `error` is a word to the lexer like any other — so
//! `catch error e` bound the message to a name spelled `error`, left `e` as an
//! orphan statement, and the analyzer refused the whole program with
//! `Unknown variable 'e'`. The parser now reads `error` as the handler keyword
//! when a second identifier follows it on the same line, which leaves the
//! language's own `catch error` spelling binding a name called `error`.
//!
//! Every case runs through **both** engines — the tree-walking VM and the
//! bytecode compiler plus the bytecode VM — because a form that works on one
//! and is an `AnalyzerError` on the other is not a language feature, it is an
//! accident of one frontend.

use redblue::bytecode::vm::BytecodeVm;
use redblue::{compile_source, run_isolated, Error, Value};

/// What one engine made of a program: the lines it printed, and either its value
/// or the failure it reported.
#[derive(Debug, Clone)]
struct Outcome {
    output: Vec<String>,
    result: Result<Value, Error>,
}

fn tree_walk(source: &str) -> Outcome {
    let failed = |error: Error| Outcome {
        output: Vec::new(),
        result: Err(error),
    };
    let tokens = match redblue::lexer::Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => return failed(error),
    };
    let ast = match redblue::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(error) => return failed(error),
    };
    if let Err(error) = redblue::analyzer::analyze(&ast) {
        return failed(error);
    }
    let (mut vm, result) = run_isolated(&ast);
    Outcome {
        output: vm.take_output(),
        result,
    }
}

fn bytecode(source: &str) -> Outcome {
    let chunk = match compile_source(source) {
        Ok(chunk) => chunk,
        Err(error) => {
            return Outcome {
                output: Vec::new(),
                result: Err(error),
            }
        }
    };
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let result = vm.run(&chunk);
    Outcome {
        output: vm.take_output(),
        result,
    }
}

fn agrees(a: &Outcome, b: &Outcome) -> bool {
    if a.output != b.output {
        return false;
    }
    match (&a.result, &b.result) {
        (Ok(x), Ok(y)) => x == y,
        (Err(x), Err(y)) => {
            x.label() == y.label()
                && x.message() == y.message()
                && x.span().map(|s| s.line) == y.span().map(|s| s.line)
        }
        _ => false,
    }
}

/// One program, run on both engines, with the engines' agreement asserted.
#[track_caller]
fn both(source: &str) -> Outcome {
    let tree = tree_walk(source);
    let byte = bytecode(source);
    assert!(
        agrees(&tree, &byte),
        "the two engines disagree about:\n{source}\n  tree-walking: {tree:?}\n  bytecode:     {byte:?}"
    );
    tree
}

/// The lines both engines printed, asserted line for line.
#[track_caller]
fn both_print(source: &str, expected: &[&str]) {
    let outcome = both(source);
    assert_eq!(
        outcome.result.as_ref().ok().map(|v| format!("{v:?}")),
        Some("Nothing".to_string()),
        "the program should succeed for:\n{source}"
    );
    assert_eq!(outcome.output, expected, "output for:\n{source}");
}

/// The failure both engines reported.
#[track_caller]
fn both_fail(source: &str) -> Error {
    match both(source).result {
        Ok(_) => panic!("the program should have failed, it printed instead:\n{source}"),
        Err(error) => error,
    }
}

// --- the spelling the phase is about ----------------------------------------

#[test]
fn catch_error_name_binds_the_message() {
    both_print(
        "try\n    set x to 1 / 0\ncatch error e\n    say e\nend\n",
        &["error"],
    );
}

/// The four spellings, one row each: a bare `catch` (README.md:11), `catch
/// error` (README.md:64, SPEC.md:484), `catch <name>` (README.md:51), and
/// SPEC.md:800's spelled-out `catch error <name>`. All four run, all four
/// handle the same failure, and the three that bind a name bind the same value.
#[test]
fn every_catch_spelling_handles_the_same_failure() {
    let cases: [(&str, &str, Option<&str>); 4] = [
        (
            "bare catch (README.md:11)",
            "set seen to \"\"\ntry\n    set bad to 1 / 0\ncatch\n    set seen to \"c\"\nend\nsay seen\n",
            None,
        ),
        (
            "catch error (README.md:64)",
            "set seen to \"\"\ntry\n    set bad to 1 / 0\ncatch error\n    set seen to error\nend\nsay seen\n",
            Some("error"),
        ),
        (
            "catch <name> (README.md:51)",
            "set seen to \"\"\ntry\n    set bad to 1 / 0\ncatch problem\n    set seen to problem\nend\nsay seen\n",
            Some("problem"),
        ),
        (
            "catch error <name> (SPEC.md:800)",
            "set seen to \"\"\ntry\n    set bad to 1 / 0\ncatch error problem\n    set seen to problem\nend\nsay seen\n",
            Some("problem"),
        ),
    ];

    for (label, source, binder) in cases {
        // A bare `catch` binds nothing, so it cannot read what it caught; every
        // named `catch` binds the same text, whatever it is called.
        let expected: &[&str] = match binder {
            None => &["c"],
            Some(_) => &["error"],
        };
        let outcome = both(source);
        assert_eq!(
            outcome.result.as_ref().ok().map(|v| format!("{v:?}")),
            Some("Nothing".to_string()),
            "{label} should succeed"
        );
        assert_eq!(
            outcome.output, expected,
            "{label} should handle the failure"
        );
    }
}

// --- scoping -----------------------------------------------------------------

/// The binder belongs to the `catch` body alone: a name `catch error e` binds
/// is not readable before the `try` or after the `end`.
#[test]
fn the_binder_is_scoped_to_the_catch_body_alone() {
    let before = both_fail("say e\ntry\n    set bad to 1 / 0\ncatch error e\n    say e\nend\n");
    assert!(
        before.message().contains("'e'"),
        "reading the binder before the try must name it, got {:?}",
        before.message()
    );

    let after = both_fail("try\n    set bad to 1 / 0\ncatch error e\n    say e\nend\nsay e\n");
    assert!(
        after.message().contains("'e'"),
        "reading the binder after the end must name it, got {:?}",
        after.message()
    );
}

/// A `catch error e` body is a scope of its own, so the binder shadows a
/// program variable of the same name without disturbing it.
#[test]
fn edge_a_binder_shadows_a_program_variable_of_the_same_name() {
    both_print(
        "set e to \"outer\"\ntry\n    set bad to 1 / 0\ncatch error e\n    say e\nend\nsay e\n",
        &["error", "outer"],
    );
}

// --- edge cases --------------------------------------------------------------

/// A `try` that does not fail never runs its `catch`, so the binder is never
/// assigned and reading it anywhere is an analyzer error, not a silent
/// `nothing`.
#[test]
fn edge_a_try_that_never_fails_never_binds_the_name() {
    let error = both_fail("try\n    say \"fine\"\ncatch error e\n    say e\nend\nsay e\n");
    assert_eq!(
        error.label(),
        "AnalyzerError",
        "the failure must be caught at analysis time, got {error:?}"
    );
    assert!(
        error.message().contains("'e'"),
        "the failure must name the unbound binder, got {:?}",
        error.message()
    );
}

/// An empty `catch` body is a handler: the failure is handled and the program
/// carries on, with either spelling.
#[test]
fn edge_an_empty_catch_body_still_handles_the_failure() {
    both_print(
        "try\n    set bad to 1 / 0\ncatch error\nend\nsay \"survived\"\n",
        &["survived"],
    );
    both_print(
        "try\n    set bad to 1 / 0\ncatch error e\nend\nsay \"survived\"\n",
        &["survived"],
    );
}

/// A body that starts with a bare name on the *next* line is not the spelled-out
/// `catch error <name>` — the binder and the keyword share a line, so `catch
/// error` still binds `error` and the body's first word is still the body's.
#[test]
fn edge_the_body_first_word_on_the_next_line_is_not_the_binder() {
    let error = both_fail("try\n    set bad to 1 / 0\ncatch error\n    n is 5\n    say n\nend\n");
    assert!(
        error.message().contains("'n'"),
        "the body's own first name must survive, got {:?}",
        error.message()
    );
}

/// A failure raised inside a `catch` body is not swallowed by the handler that
/// was written to handle the first one.
#[test]
fn edge_a_failure_inside_the_catch_body_is_not_swallowed() {
    let error = both_fail("try\n    set bad to 1 / 0\ncatch error e\n    say 1 / 0\nend\n");
    assert_eq!(error.label(), "RuntimeError", "got {error:?}");
    assert!(
        error.message().to_lowercase().contains("division"),
        "the second failure must reach the program, got {:?}",
        error.message()
    );
    assert_eq!(
        error.span().map(|s| s.line),
        Some(4),
        "the failure must point at the line that raised it"
    );
}

/// A `try` with no `catch` at all is not a handler: its failure is the
/// program's own and stops the program with a spanned runtime error.
#[test]
fn edge_a_try_with_no_catch_reports_its_own_failure() {
    let error = both_fail("try\n    set bad to 1 / 0\nend\nsay \"unreached\"\n");
    assert_eq!(error.label(), "RuntimeError", "got {error:?}");
    assert_eq!(
        error.span().map(|s| s.line),
        Some(2),
        "the failure must point at the line inside the try"
    );
}

/// The bound text is ordinary text: Unicode passes through a handler untouched,
/// and the empty string is a text like any other.
#[test]
fn edge_the_bound_name_is_ordinary_unicode_text() {
    both_print(
        "try\n    set bad to 1 / 0\ncatch error e\n    say \"{e} \u{2713} \u{1f389} caf\u{e9}\"\nend\n",
        &["error \u{2713} \u{1f389} caf\u{e9}"],
    );
}

/// The two spellings are one line apart, and neither may eat the other's word:
/// `catch error error` binds a name spelled `error`, and a `catch` whose body
/// starts on the same line is not a handler body at all.
#[test]
fn edge_the_keyword_and_the_binder_may_be_the_same_word() {
    both_print(
        "try\n    set bad to 1 / 0\ncatch error error\n    say error\nend\n",
        &["error"],
    );
}

/// A handler written in one line is still a handler: `catch error e` is not a
/// line break between the two words.
#[test]
fn edge_a_keyword_and_a_binder_on_one_line_bind_and_run() {
    both_print(
        "try\n    set bad to 1 / 0\ncatch error e\n    say \"caught \" + e\nend\n",
        &["caught error"],
    );
}

/// The other side of the same line test: a body that follows the binder on the
/// *same* line is still the body's, and the two words are still read as keyword
/// and binder rather than swallowing the body.
#[test]
fn edge_a_handler_written_entirely_on_one_line_binds_and_runs() {
    both_print(
        "try\n    set bad to 1 / 0\ncatch error e say \"caught \" + e\nend\n",
        &["caught error"],
    );
}

/// `catch error of <Type>` is not the language: `of` is not a word, so it reads
/// as the binder and the type name is left behind as an orphan. The failure
/// must name that orphan rather than pass for a typed handler.
#[test]
fn edge_catch_error_of_a_type_is_refused_naming_the_orphan() {
    let error = both_fail("try\n    set bad to 1 / 0\ncatch error of FileError\n    say 1\nend\n");
    assert!(
        error.message().contains("'FileError'"),
        "the orphan after the untyped binder must be named, got {:?}",
        error.message()
    );
}
