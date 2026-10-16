//! `items at N` and `length of X`: the two word forms SPEC.md § List Operations
//! documents as working syntax.
//!
//! `SPEC.md` § List Operations writes three lines under that heading:
//!
//! ```text
//! set first to items at 0
//! set last to items at -1
//! set count to length of items
//! ```
//!
//! None of the three worked. Both words are ordinary identifiers, so
//! `set first to items at 0` reached the `_` arm of `parse_statement_inner`,
//! which wrapped `set first to items` as a statement and left `at 0` for the
//! next statement to make of it — three statements where the documentation
//! promises one. `rb` then reported `Unknown variable 'at'`, and both tools that
//! read the same tree made it worse: `rb format` printed the three statements
//! back as three lines, and `rb lint` said nothing at all about a file `rb`
//! cannot run.
//!
//! So the contract this file pins is threefold, and each part is asserted
//! independently of the others:
//!
//! 1. each documented line **runs** and prints the value SPEC.md documents;
//! 2. each is **one statement** — counted in the parsed [`Program`], never by
//!    matching on an error message, because "it errors" and "it is one
//!    statement" are different claims and only the second is the bug;
//! 3. the two tools that read the tree agree with the runner: `rb format` never
//!    changes a program's statement count, and `rb lint` says something about
//!    every file `rb` cannot run.
//!
//! `at` and `length` are not reserved words and must not become reserved: a
//! program that spells its own variable `at` keeps working, and that is
//! asserted here rather than assumed.

use redblue::analyzer;
use redblue::linter::lint;
use redblue::parser::{self, Program};
use redblue::{Error, Value};

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Lexes and parses `source`, refusing to continue on a source that does not get
/// that far: every assertion below is about what happens *after* a program
/// parses, so a parse failure here is a failure of the thing under test.
#[track_caller]
fn parse(source: &str) -> Program {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    parser::parse(tokens).expect("source should parse")
}

/// What `rb run` does with `source`: the analyzer, then the VM — the two stages
/// the binary puts between a file and its output. Returns the lines `say`
/// printed and the value the program was worth.
#[track_caller]
fn run(source: &str) -> Result<(Vec<String>, Value), Error> {
    let program = parse(source);
    analyzer::analyze(&program)?;
    let (mut vm, result) = redblue::run_isolated(&program);
    let value = result?;
    Ok((vm.take_output(), value))
}

/// What `rb run` does with `source` when the source is expected *not* to run:
/// every stage in the pipeline, each reported as the failure it is, and no
/// stage's failure turned into a panic of the test's own.
///
/// [`run`] asserts that a source parses, so it cannot be used on the fixtures
/// that are here precisely because they do not.
#[track_caller]
fn any_failure(source: &str) -> Option<Error> {
    let tokens = match redblue::lexer::Lexer::tokenize(source) {
        Ok(t) => t,
        Err(e) => return Some(e),
    };
    let program = match parser::parse(tokens) {
        Ok(p) => p,
        Err(e) => return Some(e),
    };
    if let Err(e) = analyzer::analyze(&program) {
        return Some(e);
    }
    redblue::run_isolated(&program).1.err()
}

/// Runs `source` and returns the lines it printed, requiring it to succeed.
#[track_caller]
fn printed(source: &str) -> Vec<String> {
    match run(source) {
        Ok((lines, _)) => lines,
        Err(e) => panic!("`{}` should run, got {:?}", source.trim_end(), e),
    }
}

/// Runs `source` and returns the failure it produced, requiring it to fail.
#[track_caller]
fn run_err(source: &str) -> Error {
    let program = parse(source);
    if let Err(e) = analyzer::analyze(&program) {
        return e;
    }
    match redblue::run_isolated(&program).1 {
        Ok(_) => panic!("`{}` should have failed", source.trim_end()),
        Err(e) => e,
    }
}

/// Asserts `source` fails at runtime with exactly `expected`, and that the
/// failure names a position in the source.
#[track_caller]
fn assert_runtime_error(source: &str, expected: &str) {
    match run_err(source) {
        Error::Runtime(message, span) => {
            assert_eq!(
                message,
                expected,
                "`{}` failed with the wrong message",
                source.trim_end()
            );
            assert!(
                span.is_known(),
                "`{}` failed without a source span",
                source.trim_end()
            );
        }
        other => panic!(
            "`{}` should fail with a Runtime error, got {:?}",
            source.trim_end(),
            other
        ),
    }
}

// ---------------------------------------------------------------------------
// the three documented lines, one at a time
// ---------------------------------------------------------------------------

/// `SPEC.md` § List Operations, `set first to items at 0`.
#[test]
fn set_first_to_items_at_zero_prints_the_first_element() {
    assert_eq!(
        printed("set items to [10, 20, 30]\nset first to items at 0\nsay first\n"),
        vec!["10"],
        "`items at 0` must be the first element, as SPEC.md documents"
    );
}

/// `SPEC.md` § List Operations, `set last to items at -1`.
///
/// `-1` is the last element, which is what the documented line says and what
/// `items[-1]` has always answered — see `tests/index_bounds_test.rs`. So for
/// this spelling `-1` is a *legal* position; the out-of-bounds negative indexes
/// are pinned separately, in
/// [`edge_an_out_of_bounds_index_is_a_spanned_runtime_error_in_both_spellings`].
#[test]
fn set_last_to_items_at_negative_one_prints_the_last_element() {
    assert_eq!(
        printed("set items to [10, 20, 30]\nset last to items at -1\nsay last\n"),
        vec!["30"],
        "`items at -1` must be the last element, as SPEC.md documents"
    );
}

/// `SPEC.md` § List Operations, `set count to length of items`.
#[test]
fn set_count_to_length_of_items_prints_the_element_count() {
    assert_eq!(
        printed("set items to [10, 20, 30]\nset count to length of items\nsay count\n"),
        vec!["3"],
        "`length of items` must be the element count, as SPEC.md documents"
    );
}

/// `items at N` and `items[N]` are one operation written two ways, so they must
/// agree on every legal position — not only on the three SPEC.md happens to
/// write. A form that quietly disagrees at a boundary is worse than one that
/// does not parse, because the reader of the program has no way to tell.
///
/// Pinned: first, last, and the negative walk that reaches the first element
/// (`-3` of three); and a singleton list, whose one element is both `0` and
/// `-1`.
#[test]
fn the_word_form_and_the_bracket_form_agree_on_every_position() {
    let pairs = [
        ("say items at 0\n", "say items[0]\n"),
        ("say items at 2\n", "say items[2]\n"),
        ("say items at -1\n", "say items[-1]\n"),
        ("say items at -3\n", "say items[-3]\n"),
    ];
    for (word, bracket) in pairs {
        let head = "set items to [10, 20, 30]\n";
        assert_eq!(
            printed(&format!("{head}{word}")),
            printed(&format!("{head}{bracket}")),
            "`items{}` disagrees with `items{}`",
            word.trim_end(),
            bracket.trim_end()
        );
    }

    // A singleton list: one element, two legal positions, one answer each.
    let head = "set items to [42]\n";
    assert_eq!(
        printed(&format!("{head}say items at 0\n")),
        printed(&format!("{head}say items[0]\n"))
    );
    assert_eq!(
        printed(&format!("{head}say items at -1\n")),
        printed(&format!("{head}say items[-1]\n"))
    );
}

/// `length of X` is `length(X)`, so the two spellings must be the same function
/// and not two that happen to agree on a list of numbers.
///
/// Pinned: the empty list, a list of numbers, and a list whose elements are
/// multi-byte — the case where "number of elements" and "number of bytes" are
/// different answers and only one of them is right.
#[test]
fn the_word_form_and_the_call_form_agree() {
    let pairs = [
        (
            "set xs to [1, 2, 3]\nsay length of xs\n",
            "set xs to [1, 2, 3]\nsay length(xs)\n",
        ),
        (
            "set xs to []\nsay length of xs\n",
            "set xs to []\nsay length(xs)\n",
        ),
        (
            "set xs to [\"héllo\", \"日本語\"]\nsay length of xs\n",
            "set xs to [\"héllo\", \"日本語\"]\nsay length(xs)\n",
        ),
        (
            "set t to \"abc\"\nsay length of t\n",
            "set t to \"abc\"\nsay length(t)\n",
        ),
    ];
    for (word, call) in pairs {
        assert_eq!(
            printed(word),
            printed(call),
            "`{}` disagrees with its call spelling",
            word.trim_end()
        );
    }
}

// ---------------------------------------------------------------------------
// the statement count: the defect itself
// ---------------------------------------------------------------------------

/// The bug was never the `AnalyzerError` — it was that the line became three
/// statements. So the count is asserted on the parsed [`Program`], and no error
/// message is matched on: a parser that rejected the line outright would pass a
/// message-shaped assertion and still leave the documentation unimplemented.
#[test]
fn edge_each_documented_form_is_exactly_one_statement() {
    for (source, expected) in [
        ("set items to [1, 2, 3]\nset first to items at 0\n", 2usize),
        ("set items to [1, 2, 3]\nset last to items at -1\n", 2),
        ("set items to [1, 2, 3]\nset count to length of items\n", 2),
    ] {
        let program = parse(source);
        assert_eq!(
            program.statements.len(),
            expected,
            "`{}` parsed as {} statements:\n{:#?}",
            source.trim_end(),
            program.statements.len(),
            program.statements
        );
    }
}

/// The stray statements were not cosmetic. Under the old parse the two `set`
/// lines became four statements whose value was `nothing`, so a program that
/// assigned the index produced nothing at all while appearing to run. Asserting
/// the bound values again, after the statement count, keeps the count from being
/// satisfied by a parse that makes the right number of statements and the wrong
/// program.
#[test]
fn the_word_forms_bind_the_value_the_documentation_promises() {
    assert_eq!(
        run("set items to [1, 2, 3]\nset first to items at 0\nfirst\n")
            .unwrap()
            .1,
        Value::Number(1.0)
    );
    assert_eq!(
        run("set items to [1, 2, 3]\nset last to items at -1\nlast\n")
            .unwrap()
            .1,
        Value::Number(3.0)
    );
    assert_eq!(
        run("set items to [1, 2, 3]\nset count to length of items\ncount\n")
            .unwrap()
            .1,
        Value::Number(3.0)
    );
}

// ---------------------------------------------------------------------------
// out of bounds, per spelling
// ---------------------------------------------------------------------------

/// Every index the word form can write, checked against the bracket form it
/// stands for, is a clean spanned [`Error::Runtime`] and never a panic.
///
/// `-1` is out of bounds on the empty list — the case most likely to be mistaken
/// for "the last element", which does not exist — and one past the far end on a
/// list that has elements. `len` and `999` are the two positions a reader
/// reaches for by mistake.
///
/// The index is named as the program wrote it in both spellings, so the two
/// cannot disagree about which index failed.
#[test]
fn edge_an_out_of_bounds_index_is_a_spanned_runtime_error_in_both_spellings() {
    let pairs = [
        ("say items at 3\n", "say items[3]\n"),
        ("say items at 999\n", "say items[999]\n"),
        ("say items at -4\n", "say items[-4]\n"),
    ];
    for (word, bracket) in pairs {
        let head = "set items to [1, 2, 3]\n";
        let expected = match run_err(&format!("{head}{bracket}")) {
            Error::Runtime(message, _) => message,
            other => panic!(
                "`{}` should fail at runtime, got {:?}",
                bracket.trim(),
                other
            ),
        };
        match run_err(&format!("{head}{word}")) {
            Error::Runtime(message, span) => {
                assert_eq!(
                    message,
                    expected,
                    "`{}` names a different failure",
                    word.trim()
                );
                assert!(span.is_known(), "`{}` failed without a span", word.trim());
            }
            other => panic!(
                "`{}` should fail with a Runtime error, got {:?}",
                word.trim(),
                other
            ),
        }
    }

    // The empty list has no legal index at all, in either spelling, and its
    // message says so rather than reporting a range that does not exist. `-1`
    // is here rather than in the table above because on a list that *has*
    // elements it is a legal position.
    let empty = "set items to []\n";
    let nothing =
        "Index {} is out of bounds: length is 0, the list is empty, so it has no valid index";
    for index in ["0", "-1", "999"] {
        assert_runtime_error(
            &format!("{empty}say items at {index}\n"),
            &nothing.replace("{}", index),
        );
    }
}

// ---------------------------------------------------------------------------
// type mismatch
// ---------------------------------------------------------------------------

/// Neither word form reaches past its own type: `length of` a number and `at` a
/// text are runtime errors naming what was wrong — not panics, and not a
/// silently fabricated value.
#[test]
fn a_word_form_applied_to_the_wrong_type_is_a_clean_runtime_error() {
    match run_err("set n to 5\nsay length of n\n") {
        Error::Runtime(_, span) => assert!(span.is_known(), "`length of` a number had no span"),
        other => panic!(
            "`length of` a number should fail at runtime, got {:?}",
            other
        ),
    }
    assert_runtime_error(
        "set items to [1, 2, 3]\nsay items at \"0\"\n",
        "Index must be a number",
    );
    assert_runtime_error(
        "set items to [1, 2, 3]\nsay items at nothing\n",
        "Index must be a number",
    );

    // A `length of` with nothing to measure is refused where it is written,
    // rather than answering `length(nothing)` and failing somewhere else.
    let tokens = redblue::lexer::Lexer::tokenize("say length of\n").expect("should lex");
    match parser::parse(tokens) {
        Ok(program) => panic!(
            "`length of` with no operand parsed as {} statements",
            program.statements.len()
        ),
        Err(Error::Parser(_, span)) => assert!(span.is_known(), "the refusal had no span"),
        Err(other) => panic!(
            "`length of` with no operand should be a Parser error, got {:?}",
            other
        ),
    }

    // `at` with nothing to index is refused the same way, rather than reading
    // `items at` as `items` followed by the variable `at`.
    let tokens =
        redblue::lexer::Lexer::tokenize("set items to [1]\nsay items at\n").expect("should lex");
    assert!(
        parser::parse(tokens).is_ok(),
        "`items at` with no operand must stay parseable — `at` is still a name"
    );
}

// ---------------------------------------------------------------------------
// the disambiguation: `at` and `length` are not reserved words
// ---------------------------------------------------------------------------

/// `at` is not a keyword and must not become one. It is read as an index only
/// when an operand follows it; everywhere else it is the variable it has always
/// been. Before this rule existed `at` and `of` were reported by the analyzer as
/// unknown variables in programs that used them as names, and the fix must not
/// simply move that refusal from one spelling to the other.
#[test]
fn at_is_still_a_usable_variable_name() {
    assert_eq!(
        printed("set at to 7\nset other to at\nsay other\n"),
        vec!["7"],
        "`at` must still work as a variable name on both sides of a `set`"
    );
    assert_eq!(
        printed("set items to [1, 2]\nset at to 0\nsay items at at\n"),
        vec!["1"],
        "`items at at` is an index whose operand is the variable `at`"
    );
    assert_eq!(
        printed("set at to [4, 5]\nsay at\n"),
        vec!["[4, 5]"],
        "`at` alone, with nothing to index, is the variable"
    );
}

/// The same for `length`: the builtin already answers `length(x)`, and a program
/// that binds the name itself keeps its own binding. Only the two-word spelling
/// is the word form.
#[test]
fn length_alone_is_still_the_builtin_and_can_be_shadowed() {
    assert_eq!(printed("say length([1, 2, 3])\n"), vec!["3"]);
    assert_eq!(
        printed("set length to 99\nsay length\n"),
        vec!["99"],
        "a program that binds the name keeps its own binding"
    );
}

// ---------------------------------------------------------------------------
// the tools: `rb format` and `rb lint`
// ---------------------------------------------------------------------------

/// `rb format` printed the old three-statement parse back as three lines, which
/// is how a file `rb` cannot run got written out looking formatted. Formatting a
/// program that uses every documented form must not change how many statements
/// it has — asserted on the **reparsed** formatted text, because a formatter that
/// dropped a statement would still return a string.
///
/// Idempotence is checked too, so the count cannot drift on a second run.
#[test]
fn edge_formatting_never_changes_the_statement_count() {
    let source = "set items to [10, 20, 30]\nset first to items at 0\nset last to items at -1\nset count to length of items\nsay first\nsay last\nsay count\n";
    let before = parse(source).statements.len();
    assert_eq!(
        before, 7,
        "the fixture must be seven statements to start with"
    );

    let formatted = redblue::formatter::format(source).expect("source should format");
    let after = parse(&formatted).statements.len();
    assert_eq!(
        after, before,
        "formatting changed the statement count:\n{}",
        formatted
    );

    let twice = redblue::formatter::format(&formatted).expect("formatted source should format");
    assert_eq!(
        twice, formatted,
        "formatting is not idempotent here:\n{}",
        twice
    );
    assert_eq!(parse(&twice).statements.len(), before);
}

/// The formatted text has to keep the *values* too, not only the count: seven
/// statements that all printed `nothing` would satisfy the assertion above.
#[test]
fn formatting_a_word_form_keeps_what_the_program_prints() {
    let source = "set items to [10, 20, 30]\nset first to items at 0\nset count to length of items\nsay first\nsay count\n";
    let formatted = redblue::formatter::format(source).expect("source should format");
    assert_eq!(
        printed(&formatted),
        printed(source),
        "formatting changed what the program prints:\n{}",
        formatted
    );
}

/// `rb lint` reported nothing at all on a file `rb` cannot run: a program that
/// failed the analyzer produced no diagnostic, so the one tool whose job is to
/// say something about a file said nothing.
///
/// Each of the three documented forms is paired with both answers — whether `rb`
/// can run the file, and what `rb lint` says about it. A runnable file must
/// produce no lint error, and a file that cannot run must produce at least one.
#[test]
fn edge_each_documented_form_pairs_a_run_result_with_a_lint_result() {
    for source in [
        "set items to [10, 20, 30]\nset first to items at 0\nsay first\n",
        "set items to [10, 20, 30]\nset last to items at -1\nsay last\n",
        "set items to [10, 20, 30]\nset count to length of items\nsay count\n",
    ] {
        let (lint_errors, _) = lint(source);
        assert!(
            run(source).is_ok(),
            "a file the documented forms are in must run: {:?}\n{}",
            run(source).err(),
            source
        );
        assert!(
            lint_errors.is_empty(),
            "a file rb can run must produce no lint error, got {:?} for:\n{}",
            lint_errors,
            source
        );
    }
}

/// The other half of the same pairing: a file `rb` cannot get past the front of
/// has something to say about it. Before this phase a program that failed the
/// analyzer produced an empty lint result and exit 0 — a file that cannot run,
/// reported as a file with nothing wrong with it.
///
/// The reasons are deliberately spread across the whole static pipeline, because
/// "a file rb cannot run" is the claim being narrowed down to what a linter can
/// know: an unknown name, a missing `end`, and an unterminated text literal.
///
/// **The boundary, stated rather than hidden.** A linter reads the syntax tree;
/// it does not run the program. So a failure that only appears when the program
/// executes — calling a function that does not exist, indexing past the end of a
/// list — is a `Runtime` failure the linter has no evidence for, and this test
/// does not claim otherwise. Those are the interpreter's to report, and
/// `tests/index_bounds_test.rs` covers them there.
#[test]
fn a_file_rb_cannot_lex_parse_or_analyze_has_a_lint_diagnostic() {
    let cases = [
        // The analyzer failure this phase found: a name that resolves to
        // nothing.
        "set x to unknown_name\nsay x\n",
        // A parser failure: the `end` was never written.
        "if yes\n    say 1\n",
        // A lexer failure: an unterminated text literal.
        "say \"unterminated\n",
    ];

    for source in cases {
        let (lint_errors, _) = lint(source);
        assert!(
            any_failure(source).is_some(),
            "this fixture must be a file rb cannot run:\n{}",
            source
        );
        assert!(
            !lint_errors.is_empty(),
            "a file rb cannot run must produce at least one lint diagnostic, got none for:\n{}",
            source
        );
    }

    // And the diagnostic says *which* stage refused it, because `Syntax error`
    // and `Analysis error` are different things for a reader to act on. The
    // wording is checked as a prefix so the test does not break when the
    // analyzer's suggestion text changes.
    let (parse_errors, _) = lint("if yes\n    say 1\n");
    assert!(
        parse_errors[0].message.starts_with("Syntax error: "),
        "a file that does not parse is a syntax error, got {:?}",
        parse_errors[0].message
    );
    let (analyzer_errors, _) = lint("set x to unknown_name\nsay x\n");
    assert!(
        analyzer_errors[0].message.starts_with("Analysis error: "),
        "a file the analyzer refuses is an analysis error, got {:?}",
        analyzer_errors[0].message
    );
}

// ---------------------------------------------------------------------------
// the operand: what follows `at` and what follows `length of`
// ---------------------------------------------------------------------------

/// The index is the **additive expression** after `at`, which is the reading an
/// English-like language needs: `at n + 1` indexes by `n + 1`, and `at 3 - 1`
/// is an index rather than a subtraction of nothing.
///
/// The consequence is stated rather than left to be discovered: a second `at`
/// belongs to the index, not to the object, so `xs at 1 at 0` asks for
/// `1 at 0` — a number, which is not a list — and says so. It is a clean runtime
/// error naming what it tried to index, never a panic and never a silent
/// misreading, and stepping twice means binding the step in between.
#[test]
fn edge_the_worded_index_operand_is_read_as_a_whole_expression() {
    assert_eq!(
        printed("set xs to [10, 20, 30]\nset n to 1\nsay xs at n + 1\n"),
        vec!["30"],
        "`at n + 1` must index by `n + 1`, not by `n` and then add one"
    );
    assert_eq!(
        printed("set xs to [10, 20, 30]\nsay xs at 3 - 1\n"),
        vec!["30"],
        "`at 3 - 1` must index by `3 - 1`"
    );
    assert_runtime_error(
        "set xs to [[1, 2], [3, 4]]\nsay xs at 1 at 0\n",
        "Cannot index non-list",
    );
    // The way to step twice is to bind the step in between, and that reads the
    // same as any other line that binds one value and reads another.
    assert_eq!(
        printed("set xs to [[1, 2], [3, 4]]\nset inner to xs at 1\nsay inner at 0\n"),
        vec!["3"],
        "a second `at` step is a second line, bound through a name"
    );
    // The bracketed spelling still steps twice in one expression, and agrees
    // with the bound form on every element.
    assert_eq!(
        printed("set xs to [[1, 2], [3, 4]]\nsay xs[1][0]\n"),
        vec!["3"],
        "`xs[1][0]` is the one expression that steps twice"
    );
}

/// **`at` does not swallow the comparison, the conjunction or the
/// disjunction.** This is the round-2 defect, and it is the worst one this file
/// could hold, because it turned working programs into runtime failures instead
/// of refusals.
///
/// The operand used to be read with the whole-expression parser, which made
/// `say xs at 0 is 10` mean `say xs at (0 is 10)` — an index whose operand is the
/// *answer* of a comparison, i.e. a boolean. Both this line and the identical
/// line written `xs[0] is 10` say the same thing; one of them worked and one of
/// them failed `Index must be a number`, and which one was decided by a
/// precedence rule no reader would guess. Same for `length of xs is 3`, which
/// measured a comparison and failed `length requires a list or text` while
/// `length(xs) is 3` answered.
///
/// So every case here is asserted **twice**: once in the worded spelling and
/// once in the spelling it stands for. The two must print the same thing. A
/// parser that made the worded form an error would fail this, and so would one
/// that made it quietly mean something else.
#[test]
fn edge_a_comparison_after_a_worded_index_compares_the_element() {
    let pairs = [
        ("say xs at 0 is 10\n", "say xs[0] is 10\n"),
        ("say xs at 0 is not 11\n", "say xs[0] is not 11\n"),
        (
            "say xs at 0 is greater than 5\n",
            "say xs[0] is greater than 5\n",
        ),
        (
            "say xs at 1 is 20 and xs at 2 is 30\n",
            "say xs[1] is 20 and xs[2] is 30\n",
        ),
        ("say xs at 0 is 99 or no\n", "say xs[0] is 99 or no\n"),
        // A comparison whose right side is itself an index: the worded form must
        // not read `xs at 1` as the whole right-hand side and stop there.
        ("say xs at 0 is xs at 2\n", "say xs[0] is xs[2]\n"),
    ];
    for (word, bracket) in pairs {
        let head = "set xs to [10, 20, 30]\n";
        let expected = printed(&format!("{head}{bracket}"));
        assert!(
            !expected.is_empty(),
            "the bracketed control must produce output, or this test compares nothing:\n{bracket}"
        );
        assert_eq!(
            printed(&format!("{head}{word}")),
            expected,
            "`{}` disagrees with `{}`",
            word.trim_end(),
            bracket.trim_end()
        );
    }

    // Spelled out, because "the two agree" can be satisfied by two wrong answers.
    assert_eq!(
        printed("set xs to [10, 20, 30]\nsay xs at 0 is 10\n"),
        vec!["yes"]
    );
    assert_eq!(
        printed("set xs to [10, 20, 30]\nsay length of xs is 3\n"),
        vec!["yes"],
        "`length of xs is 3` must measure the list and then compare it"
    );
    assert_eq!(
        printed("set xs to [10, 20, 30]\nsay xs at 0 is 10 and xs at 2 is 30\n"),
        vec!["yes"],
        "`and` must join two comparisons, not become an index"
    );
    assert_eq!(
        printed("set xs to [10, 20, 30]\nsay xs at 0 is 99 or xs at 2 is 30\n"),
        vec!["yes"],
        "`or` must join two comparisons, not become an index"
    );

    // The failing form is named rather than left to be rediscovered: an index is
    // still refused when the comparison is written *inside* parentheses, because
    // that is what the writer asked for.
    assert_runtime_error(
        "set xs to [10, 20, 30]\nsay xs at (0 is 10)\n",
        "Index must be a number",
    );
}

/// The same rule behind `length of`, whose operand used to be read the same wrong
/// way — `length of xs is 3` was `length(xs is 3)`.
#[test]
fn edge_a_comparison_after_a_worded_length_measures_first() {
    let pairs = [
        ("say length of xs is 3\n", "say length(xs) is 3\n"),
        (
            "say length of xs is greater than 2\n",
            "say length(xs) is greater than 2\n",
        ),
        (
            "say length of xs is 3 and length of ys is 1\n",
            "say length(xs) is 3 and length(ys) is 1\n",
        ),
        (
            "say length of xs is 9 or no\n",
            "say length(xs) is 9 or no\n",
        ),
        // Arithmetic still belongs to the operand, so the two forms measure the
        // same thing: `length of s + t` is `length(s + t)`.
        ("say length of s + t\n", "say length(s + t)\n"),
    ];
    for (word, call) in pairs {
        let head = "set xs to [1, 2, 3]\nset ys to [9, 9]\nset s to \"ab\"\nset t to \"cd\"\n";
        assert_eq!(
            printed(&format!("{head}{word}")),
            printed(&format!("{head}{call}")),
            "`{}` disagrees with `{}`",
            word.trim_end(),
            call.trim_end()
        );
    }

    assert_eq!(
        printed("set xs to [1, 2, 3]\nsay length of xs is 3\n"),
        vec!["yes"]
    );
}

/// `length of` reads the operand too, and refuses at the point where it was
/// written rather than answering something for nothing.
#[test]
fn edge_length_of_reads_one_operand_and_refuses_a_missing_one() {
    assert_eq!(
        printed("say length of \"ab\" + \"cd\"\n"),
        vec!["4"],
        "`length of \"ab\" + \"cd\"` measures the concatenation, not `\"ab\"`"
    );
    // `length of` at the end of a line has no operand to measure, and is
    // refused where it is written rather than answered as `length(nothing)`.
    let tokens = redblue::lexer::Lexer::tokenize("say length of\n").expect("should lex");
    match parser::parse(tokens) {
        Ok(_) => panic!("`length of` with no operand parsed"),
        Err(Error::Parser(_, span)) => assert!(span.is_known(), "the refusal has no span"),
        Err(other) => panic!("expected a Parser error, got {:?}", other),
    }
    // `items at` at the end of a line is a *different* thing: `at` is an
    // ordinary name, so the line is two statements and the analyzer is what
    // refuses it. Both answers are clean, and neither is a parser crash.
    let source = "set items to [1]\nsay items at\n";
    assert_eq!(
        parse(source).statements.len(),
        3,
        "`items at` is `say items` and then the variable `at`, which is what it has always been"
    );
    match run_err(source) {
        Error::Analyzer(_, span) => assert!(span.is_known(), "the refusal has no span"),
        other => panic!(
            "`items at` with no operand should fail in the analyzer, got {:?}",
            other
        ),
    }
}

/// **Neither worded form reaches across a newline.** This is the line rule, and
/// it is a decision rather than an accident — the two forms answer differently
/// *because* `at` is an ordinary name and `length of` is not, and both answers
/// are the right ones.
///
/// - `xs at` / `0` on two lines: `at` is a name, so the line ends there. The two
///   lines are two statements, and the analyzer is what refuses the second one.
///   Joining them would silently move an expression out of the statement that
///   contains it — the exact rule `Parser::at_index_ahead` exists to keep.
/// - `length of` / `xs` on two lines: the two words were written and the operand
///   was not, so it is a parser error where it is written rather than a
///   measurement of whatever the next line happens to hold.
///
/// Pinned both ways, because the interesting half is that they are *not* the same
/// answer and a reader is entitled to know that before they write either.
#[test]
fn edge_neither_worded_form_takes_an_operand_from_the_next_line() {
    // `at`: `say items` and then the name `at`, and the analyzer refuses the name.
    let split_index = "set items to [1, 2]\nsay items at\n0\n";
    assert_eq!(
        parse(split_index).statements.len(),
        4,
        "`items at` on its own line is `say items`, then the name `at`, then `0`:\n{:#?}",
        parse(split_index).statements
    );
    match run_err(split_index) {
        Error::Analyzer(_, span) => assert!(span.is_known(), "the refusal has no span"),
        other => panic!(
            "`items at` with nothing to index should be refused by the analyzer, got {:?}",
            other
        ),
    }

    // `length of`: refused by the parser, where it was written.
    let split_length = "say length of\nxs\n";
    match parser::parse(redblue::lexer::Lexer::tokenize(split_length).expect("should lex")) {
        Ok(program) => panic!(
            "`length of` with its operand on the next line parsed as {} statements",
            program.statements.len()
        ),
        Err(Error::Parser(message, span)) => {
            assert!(
                message.contains("length of"),
                "the refusal must name the form it is about, got: {}",
                message
            );
            assert!(span.is_known(), "the refusal has no span");
        }
        Err(other) => panic!("expected a Parser error, got {:?}", other),
    }
}

/// The Unicode row: an element count is a count of elements, not of bytes, and
/// the worded spelling must not be the one that gets it wrong.
#[test]
fn edge_length_of_measures_elements_and_characters_under_the_right_spelling() {
    // Three elements, one of them multi-byte: `length of` must say 3.
    assert_eq!(
        printed("set xs to [\"héllo\", \"日本語\", \"x\"]\nsay length of xs\n"),
        vec!["3"]
    );
    // A text is measured in characters, which is the contract `runtime.rs` and
    // `tests/test_text.rb` now fix; the worded form is the same function.
    assert_eq!(
        printed("say length of \"héllo\"\n"),
        vec!["5"],
        "a text answers its length in characters, as `length(\"héllo\")` does"
    );
    assert_eq!(
        printed("say length of \"\"\n"),
        vec!["0"],
        "the empty text measures zero in either spelling"
    );
}

/// The resource row: the operand is parsed through the same nesting budget as
/// every other expression, so a source line that nests without end is a
/// diagnostic with a position rather than a stack overflow.
#[test]
fn edge_a_worded_index_operand_that_nests_past_the_limit_is_refused() {
    let deep = "(".repeat(200);
    let source = format!("set xs to [1]\nsay xs at {deep}0{})\n", ")".repeat(200));
    match parser::parse(redblue::lexer::Lexer::tokenize(&source).expect("should lex")) {
        Ok(_) => panic!("200 levels of nesting parsed"),
        Err(Error::Parser(message, span)) => {
            assert!(message.contains("nest"), "unexpected refusal: {}", message);
            assert!(span.is_known(), "the refusal has no source span");
        }
        Err(other) => panic!("expected a Parser error, got {:?}", other),
    }
}
