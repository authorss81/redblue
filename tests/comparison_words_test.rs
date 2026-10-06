//! The word comparison forms — `is greater than`, `is greater than or equal
//! to`, `is less than`, `is less than or equal to`, `is equal to` and `is
//! not` — are the flagship example in `README.md:18`, and `SPEC.md:272-282`
//! and `docs/GRAMMAR.md:93-98` spell out the same six. The parser handled only
//! the symbolic tails of two of them: `parse_comparison` looked past an `is`
//! for a `TokenKind::Equal` or a `TokenKind::Not` and otherwise fell through
//! to plain equality, so `greater`, `less`, `than` and `equal` were parsed as
//! the right operand of `is` and the rest of the line died with
//! `ParserError: Expected Then but got Identifier("than")`.
//!
//! Phase 025 made the symbolic half of this pair (`>`, `>=`, `<`, `<=`, `==`,
//! `!=`) work, in `tests/comparison_lex_test.rs`. These tests pin the word half
//! and pin that the two halves agree.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use redblue::lexer::Lexer;
use redblue::parser::{parse, Statement};
use redblue::{Error, Value};

/// Scratch directory inside the project's own `target/tmp`, never the system
/// temp dir, so nothing outside the checkout is touched. Each caller names its
/// own subdirectory because tests run in parallel and would otherwise
/// `remove_dir_all` each other's scratch files.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/rb-comparison-words")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

/// The full token list of `source`, for handing to the parser.
fn tokens(source: &str) -> Vec<redblue::lexer::Token> {
    Lexer::tokenize(source).expect("source should lex")
}

/// Runs `source` through lexer → parser → VM and returns the value of its last
/// statement. Only a bare expression statement carries a value out, so a
/// source ending in `set …` or `say …` is rejected rather than left to compare
/// `nothing` against `nothing`.
#[track_caller]
fn eval(source: &str) -> Value {
    let ast = parse(tokens(source)).expect("source should parse");
    assert!(
        matches!(
            ast.statements.last().map(|s| &s.statement),
            Some(Statement::Expr(_))
        ),
        "`{}` does not end in a bare expression, so it has no value to assert on",
        source
    );
    let mut vm = redblue::Vm::new();
    vm.run(&ast).expect("source should run")
}

/// Runs `source` through the parser and returns the error it produced.
#[track_caller]
fn parse_err(source: &str) -> Error {
    parse(tokens(source)).expect_err("source should have failed to parse")
}

/// Runs `source` and returns the runtime error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    let ast = parse(tokens(source)).expect("source should parse");
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

fn yes_no(flag: bool) -> Value {
    Value::YesNo(flag)
}

/// Runs `program` through `rb run` and returns `(exit success, stdout)`.
#[track_caller]
fn rb_run(name: &str, program: &str) -> (bool, String, String) {
    let script = scratch_dir(name).join(format!("words-{}.rb", std::process::id()));
    fs::write(&script, program).expect("scratch program should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("run")
        .arg(&script)
        .output()
        .expect("rb should be runnable");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let _ = fs::remove_file(&script);
    (output.status.success(), stdout, stderr)
}

/// The whole point of the phase: all six documented word forms parse, and each
/// takes the branch it names.
#[test]
fn the_six_word_comparison_forms_parse_and_select_the_branch_they_name() {
    let program = "\
set x to 20
set taken to \"none\"
if x is greater than 10 then
    set taken to \"greater\"
end
if x is greater than or equal to 20 then
    set taken to \"greater or equal\"
end
if x is less than 10 then
    set taken to \"less\"
end
if x is less than or equal to 20 then
    set taken to \"less or equal\"
end
if x is equal to 20 then
    set taken to \"equal\"
end
if x is not 10 then
    set taken to \"not\"
end
taken
";
    assert_eq!(
        eval(program),
        Value::Text("not".to_string()),
        "the word comparison forms did not parse, or parsed to the wrong operator"
    );
}

/// The same six, reached through `rb run` on a real file: it exits 0, and the
/// printed lines are the branches the conditions name — a form that silently
/// parsed as something else would print the wrong line or none at all.
#[test]
fn rb_run_executes_a_file_using_every_word_comparison_form() {
    let program = "\
set x to 20
if x is greater than 10 then
    say \"greater\"
end
if x is greater than or equal to 20 then
    say \"greater or equal\"
end
if x is less than 10 then
    say \"less\"
end
if x is less than or equal to 20 then
    say \"less or equal\"
end
if x is equal to 20 then
    say \"equal\"
end
if x is not 10 then
    say \"not\"
end
if x is 20 then
    say \"bare is\"
end
say \"done\"
";
    let (ok, stdout, stderr) = rb_run("all-forms", program);
    assert!(
        ok,
        "rb run exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        vec![
            "greater",
            "greater or equal",
            "less or equal",
            "equal",
            "not",
            "bare is",
            "done"
        ],
        "the wrong branches were taken; only the false conditions stay silent"
    );
}

/// The word forms and the symbolic forms phase 025 added must answer
/// identically on the same operands, including on operands that cannot be
/// ordered at all. This is the cross-check between the two halves of the
/// documented comparison set.
#[test]
fn word_and_symbolic_forms_agree_on_the_same_operands() {
    let pairs = [
        ("1", "1"),
        ("1", "2"),
        ("2", "1"),
        ("0", "-0.0"),
        ("-1", "1"),
        ("2.5", "2.5"),
        ("0", "0.0001"),
        ("-9223372036854775808", "9223372036854775807"),
    ];
    let words = [
        ("is equal to", "=="),
        ("is not", "!="),
        ("is greater than", ">"),
        ("is less than", "<"),
        ("is greater than or equal to", ">="),
        ("is less than or equal to", "<="),
    ];
    for (a, b) in pairs {
        for (words_form, symbol) in words {
            let word = eval(&format!("{} {} {}", a, words_form, b));
            let sym = eval(&format!("{} {} {}", a, symbol, b));
            assert_eq!(
                word, sym,
                "`{} {} {}` and `{} {} {}` disagree",
                a, words_form, b, a, symbol, b
            );
        }
    }

    // Non-comparable operands: both spellings must fail the same way, not one
    // of them answering `no`.
    let incomparable = [
        ("1", "\"a\""),
        ("\"a\"", "1"),
        ("yes", "3"),
        ("nothing", "1"),
        ("{a: 1}", "1"),
        ("[1, 2]", "3"),
    ];
    for (a, b) in incomparable {
        assert_runtime_error(
            &format!("{} is greater than {}", a, b),
            "Cannot compare non-numbers",
        );
        assert_runtime_error(
            &format!("{} is less than {}", a, b),
            "Cannot compare non-numbers",
        );
        assert_runtime_error(
            &format!("{} is greater than or equal to {}", a, b),
            "Cannot compare non-numbers",
        );
        assert_runtime_error(
            &format!("{} is less than or equal to {}", a, b),
            "Cannot compare non-numbers",
        );
        // And the symbolic form says exactly the same thing.
        assert_runtime_error(&format!("{} > {}", a, b), "Cannot compare non-numbers");
        assert_runtime_error(&format!("{} <= {}", a, b), "Cannot compare non-numbers");
        // Equality is defined for every pair of values, so the word and the
        // symbol forms both answer `yes`/`no` here rather than failing.
        assert_eq!(
            eval(&format!("{} is equal to {}", a, b)),
            eval(&format!("{} == {}", a, b)),
            "`is equal to` and `==` disagree on {} / {}",
            a,
            b
        );
        assert_eq!(
            eval(&format!("{} is not {}", a, b)),
            eval(&format!("{} != {}", a, b)),
            "`is not` and `!=` disagree on {} / {}",
            a,
            b
        );
    }
}

/// `x is greater than or equal to y` is ONE comparison, not
/// `(x is greater than y) or equal to y`. If `or` were left for `parse_or` the
/// rest of the line would not parse at all, and if it were bound loosely the
/// strict operator would win and the inclusive boundary would be wrong.
#[test]
fn edge_or_equal_to_binds_as_one_comparison_not_as_a_logical_or() {
    // The inclusive boundary, where strict and inclusive disagree.
    for (x, y, expected) in [
        ("5", "5", true),
        ("5", "6", false),
        ("6", "5", true),
        ("6", "6", true),
        ("4", "5", false),
    ] {
        assert_eq!(
            eval(&format!("{} is greater than or equal to {}", x, y)),
            yes_no(expected),
            "`{} is greater than or equal to {}`",
            x,
            y
        );
        // `a is less than or equal to b` is the mirror of `b is greater than
        // or equal to a` — the exact identity, with no assumptions of mine.
        assert_eq!(
            eval(&format!("{} is less than or equal to {}", x, y)),
            eval(&format!("{} is greater than or equal to {}", y, x)),
            "the inclusive forms are not mirror images for x={} y={}",
            x,
            y
        );
        // A strict comparison of the same operands disagrees at the boundary,
        // which is what makes this test able to fail.
        assert_eq!(
            eval(&format!("{} is greater than {}", x, y)),
            yes_no(x > y),
            "`{} is greater than {}`",
            x,
            y
        );
    }

    // A real `or` still binds looser than a word comparison, so this is the
    // disjunction and both sides parse as comparisons in their own right.
    assert_eq!(
        eval("5 is greater than 6 or 5 is less than 6"),
        yes_no(true)
    );
    assert_eq!(
        eval("5 is greater than 6 or 5 is greater than 7"),
        yes_no(false)
    );
    assert_eq!(
        eval("5 is greater than or equal to 6 or 5 is greater than 6"),
        yes_no(false)
    );
    assert_eq!(
        eval("5 is greater than or equal to 5 or 5 is less than 5"),
        yes_no(true)
    );
    // `and` binds tighter than either.
    assert_eq!(
        eval("5 is greater than 1 and 5 is less than 9"),
        yes_no(true)
    );
    assert_eq!(
        eval("5 is greater than 9 and 5 is less than 1"),
        yes_no(false)
    );
}

/// The equality boundary in both directions: with `x` equal to itself the
/// strict forms are `no` and the inclusive forms are `yes`, whichever side the
/// operands are written on.
#[test]
fn edge_word_comparison_at_the_equality_boundary_in_both_directions() {
    for x in ["5", "0", "-3", "2.5", "0.0"] {
        assert_eq!(
            eval(&format!("{} is greater than {}", x, x)),
            yes_no(false),
            "x is greater than x with x={}",
            x
        );
        assert_eq!(
            eval(&format!("{} is less than {}", x, x)),
            yes_no(false),
            "x is less than x with x={}",
            x
        );
        assert_eq!(
            eval(&format!("{} is greater than or equal to {}", x, x)),
            yes_no(true),
            "x is greater than or equal to x with x={}",
            x
        );
        assert_eq!(
            eval(&format!("{} is less than or equal to {}", x, x)),
            yes_no(true),
            "x is less than or equal to x with x={}",
            x
        );
        assert_eq!(
            eval(&format!("{} is equal to {}", x, x)),
            yes_no(true),
            "x is equal to x with x={}",
            x
        );
        assert_eq!(
            eval(&format!("{} is not {}", x, x)),
            yes_no(false),
            "x is not x with x={}",
            x
        );
    }
    // Off the boundary in both directions, for both signs.
    assert_eq!(eval("6 is greater than 5"), yes_no(true));
    assert_eq!(eval("4 is greater than 5"), yes_no(false));
    assert_eq!(eval("6 is less than 5"), yes_no(false));
    assert_eq!(eval("4 is less than 5"), yes_no(true));
    assert_eq!(eval("5 is greater than or equal to 6"), yes_no(false));
    assert_eq!(eval("5 is less than or equal to 4"), yes_no(false));
}

/// Reversed operands: swapping the sides swaps the answer for the strict
/// forms and leaves it alone for the inclusive ones. A parser that lost track
/// of which side was which could not get all of these right.
#[test]
fn edge_word_comparison_with_the_operands_reversed() {
    let pairs = [("1", "2"), ("2", "1"), ("0", "10"), ("-1", "1")];
    for (a, b) in pairs {
        assert_eq!(
            eval(&format!("{} is greater than {}", a, b)),
            eval(&format!("{} is less than {}", b, a)),
            "`a is greater than b` and `b is less than a` disagree for a={} b={}",
            a,
            b
        );
        assert_eq!(
            eval(&format!("{} is greater than or equal to {}", a, b)),
            eval(&format!("{} is less than or equal to {}", b, a)),
            "the inclusive forms disagree when reversed for a={} b={}",
            a,
            b
        );
        assert_eq!(
            eval(&format!("{} is greater than {}", a, b)),
            yes_no(a > b),
            "`{} is greater than {}`",
            a,
            b
        );
        assert_eq!(
            eval(&format!("{} is less than {}", a, b)),
            yes_no(a < b),
            "`{} is less than {}`",
            a,
            b
        );
    }
    // Equality is symmetric, so reversing changes nothing.
    for (a, b) in pairs {
        assert_eq!(
            eval(&format!("{} is equal to {}", a, b)),
            eval(&format!("{} is equal to {}", b, a)),
            "equality is not symmetric for a={} b={}",
            a,
            b
        );
    }
}

/// A number ordered against text is a caught `Runtime` error with a span —
/// never a `yes`, never a `no`, never a panic, and never a parse failure,
/// since the word forms themselves must be recognised.
#[test]
fn edge_word_ordering_a_number_against_text_is_a_runtime_error() {
    assert_runtime_error("1 is greater than \"a\"", "Cannot compare non-numbers");
    assert_runtime_error("\"a\" is greater than 1", "Cannot compare non-numbers");
    assert_runtime_error("1 is less than \"a\"", "Cannot compare non-numbers");
    assert_runtime_error("\"a\" is less than 1", "Cannot compare non-numbers");
    assert_runtime_error(
        "1 is greater than or equal to \"a\"",
        "Cannot compare non-numbers",
    );
    assert_runtime_error(
        "\"a\" is less than or equal to 1",
        "Cannot compare non-numbers",
    );
    // The empty string is text too, and Redblue has no text ordering at all.
    assert_runtime_error("\"\" is less than \"a\"", "Cannot compare non-numbers");
    // Text *equality* through the word forms is fine, and is length- and
    // case-sensitive. This is the pair of paths that share an operator name.
    assert_eq!(eval("\"a\" is equal to \"a\""), yes_no(true));
    assert_eq!(eval("\"a\" is equal to \"A\""), yes_no(false));
    assert_eq!(eval("\"\" is equal to \"\""), yes_no(true));
    assert_eq!(eval("\"a\" is not \"b\""), yes_no(true));
    // Unicode text compares equal to itself and differs from a longer string,
    // so the word forms are not comparing byte prefixes.
    assert_eq!(eval("\"héllo\" is equal to \"héllo\""), yes_no(true));
    assert_eq!(eval("\"日本語\" is equal to \"日本語\""), yes_no(true));
    assert_eq!(eval("\"🎉\" is equal to \"🎉\""), yes_no(true));
    assert_eq!(eval("\"日本語\" is equal to \"日本語x\""), yes_no(false));
    assert_eq!(eval("\"日本\" is not \"日本語\""), yes_no(true));
    // And text that came out of a list or record still compares equal to
    // itself through the word form.
    assert_eq!(eval("[\"é\"][0] is equal to \"é\""), yes_no(true));
    assert_eq!(eval("{t: \"ü\"}.t is equal to \"ü\""), yes_no(true));
}

/// A record ordered against a number is the same caught runtime error, and so
/// is a list. The word forms must not make an unorderable value orderable.
#[test]
fn edge_word_ordering_a_record_or_list_against_a_number_is_a_runtime_error() {
    assert_runtime_error("{a: 1} is greater than 1", "Cannot compare non-numbers");
    assert_runtime_error("1 is greater than {a: 1}", "Cannot compare non-numbers");
    assert_runtime_error("{a: 1} is less than 1", "Cannot compare non-numbers");
    assert_runtime_error("1 is less than {a: 1}", "Cannot compare non-numbers");
    assert_runtime_error(
        "{a: 1} is greater than or equal to 1",
        "Cannot compare non-numbers",
    );
    assert_runtime_error(
        "1 is less than or equal to {a: 1}",
        "Cannot compare non-numbers",
    );
    assert_runtime_error("{} is greater than 0", "Cannot compare non-numbers");
    assert_runtime_error("[1, 2] is greater than 3", "Cannot compare non-numbers");
    assert_runtime_error("[] is less than 1", "Cannot compare non-numbers");
    assert_runtime_error("3 is less than [1, 2]", "Cannot compare non-numbers");
    // A list against a list is not orderable either, and must not be a `no`.
    assert_runtime_error("[1] is less than [2]", "Cannot compare non-numbers");
    assert_runtime_error("{a: 1} is less than {a: 2}", "Cannot compare non-numbers");
    // Two empty containers are equal, though neither can be ordered.
    assert_eq!(eval("[] is equal to []"), yes_no(true));
    assert_eq!(eval("{} is equal to {}"), yes_no(true));
    assert_eq!(eval("[] is equal to [1]"), yes_no(false));
    assert_eq!(eval("[7] is equal to [7]"), yes_no(true));
    assert_eq!(eval("{} is not {a: nothing}"), yes_no(true));
}

/// A word only becomes an operator when the whole phrase is present. A program
/// that uses `greater`, `less`, `equal` or `than` as ordinary variable names
/// keeps working exactly as it did before this phase, which is what the
/// partial-match rule buys.
#[test]
fn edge_a_partial_word_phrase_still_compares_for_equality() {
    let program = "\
set greater to 5
set less to 6
set equal to 7
set than to 8
set x to 5
set taken to \"none\"
if x is greater then
    set taken to \"greater\"
end
if x is equal then
    set taken to \"equal\"
end
if x is than then
    set taken to \"than\"
end
if x is less then
    set taken to \"less\"
end
taken
";
    assert_eq!(
        eval(program),
        Value::Text("greater".to_string()),
        "only `x is greater` holds here, since equal is 7, less is 6 and than is 8"
    );

    // `less` and `equal` still compare for equality when they are the operand.
    assert_eq!(eval("set less to 3\n5 is less"), yes_no(false));
    assert_eq!(eval("set less to 5\n5 is less"), yes_no(true));
    // A record field of the same name is untouched too.
    assert_eq!(eval("{greater: 1}.greater is equal to 1"), yes_no(true));
    // And `x is greater` with nothing named `greater` is the undefined-variable
    // error, not a silently-equal comparison.
    let err = eval_err("5 is greater");
    assert!(
        matches!(err, Error::Runtime(..)),
        "`5 is greater` with no variable named `greater` must be a runtime error, got {}",
        err
    );
}

/// A word comparison that is missing its operand, or that names half a phrase,
/// is reported rather than guessed at — and every refusal carries a span.
#[test]
fn edge_malformed_word_comparisons_are_reported_not_guessed() {
    let cases = [
        "set x to 1\nif x is greater than then\n    say \"a\"\nend",
        "set x to 1\nif x is less than then\n    say \"a\"\nend",
        "set x to 1\nif x is greater than or equal to then\n    say \"a\"\nend",
        "set x to 1\nif x is less than or equal to then\n    say \"a\"\nend",
        "set x to 1\nif x is equal to then\n    say \"a\"\nend",
        "set x to 1\nif x is not then\n    say \"a\"\nend",
        "set x to 1\nif x is greater than or then\n    say \"a\"\nend",
        "set x to 1\nif x is less than or equal then\n    say \"a\"\nend",
        "set x to 1\nif x is greater\nend",
    ];
    for case in cases {
        let err = parse_err(case);
        assert!(
            err.span().is_some(),
            "`{}` must fail with a source span, got {}",
            case,
            err
        );
    }

    // The line just ends: no operand at all, at end of file.
    let err = parse_err("set x to 1\nif x is greater than");
    assert!(
        err.span().is_some(),
        "a word comparison with no right operand must carry a span"
    );
    // A phrase split across lines is not a phrase.
    let err = parse_err("set x to 1\nif x is greater\nthan 5 then\n    say \"a\"\nend");
    assert!(
        err.span().is_some(),
        "a phrase split across a newline must not be joined, got {}",
        err
    );
    // `is` alone is still equality, and `is =` still is too — neither arm was
    // narrowed by this phase.
    assert_eq!(eval("set x to 5\nx is 5"), yes_no(true));
    assert_eq!(eval("set x to 5\nx is = 5"), yes_no(true));
    assert_eq!(eval("set x to 5\nx is not 6"), yes_no(true));
    assert_eq!(
        eval("set name to \"Alice\"\nname is \"Alice\""),
        yes_no(true)
    );
}

/// The numeric extremes the word forms have to order. NaN and the infinities
/// cannot be built in Redblue at all, so the last two cases assert they are
/// refused before any comparison sees them, in the word form too.
#[test]
fn edge_word_comparison_orders_the_numeric_boundaries() {
    // Signed zero is equal in every direction.
    assert_eq!(eval("-0.0 is equal to 0.0"), yes_no(true));
    assert_eq!(eval("-0.0 is greater than 0.0"), yes_no(false));
    assert_eq!(eval("-0.0 is less than 0.0"), yes_no(false));
    assert_eq!(eval("-0.0 is greater than or equal to 0.0"), yes_no(true));
    assert_eq!(eval("-0.0 is less than or equal to 0.0"), yes_no(true));
    assert_eq!(eval("0.0 is not -0.0"), yes_no(false));
    // Past `2^53` a double is only exact in steps of two: `2^53+1` rounds back
    // onto `2^53`, and the comparison must see that rather than the source
    // digits. So `2^53` is the greater one, and `2^53+1` equals it.
    assert_eq!(
        eval("9007199254740992 is greater than 9007199254740991"),
        yes_no(true)
    );
    assert_eq!(
        eval("9007199254740992 is less than 9007199254740993"),
        yes_no(false)
    );
    assert_eq!(
        eval("9007199254740993 is equal to 9007199254740992"),
        yes_no(true)
    );
    assert_eq!(
        eval("9007199254740993 is greater than or equal to 9007199254740992"),
        yes_no(true)
    );
    assert_eq!(
        eval("9007199254740992 is less than or equal to 9007199254740993"),
        yes_no(true)
    );
    assert_eq!(
        eval("9007199254740993 is equal to 9007199254740993"),
        yes_no(true)
    );
    // The extremes of `i64` in both signs still order.
    assert_eq!(eval("-9223372036854775808 is less than 0"), yes_no(true));
    assert_eq!(eval("9223372036854775807 is greater than 0"), yes_no(true));
    assert_eq!(
        eval("-9223372036854775808 is greater than or equal to -9223372036854775808"),
        yes_no(true)
    );
    assert_eq!(
        eval("9223372036854775807 is less than or equal to 9223372036854775807"),
        yes_no(true)
    );
    // The largest finite double, and the one just below it, order.
    assert_eq!(
        eval("1.7976931348623157e308 is greater than 1.7976931348623156e308"),
        yes_no(true)
    );
    assert_eq!(
        eval("1e308 is less than 1.7976931348623157e308"),
        yes_no(true)
    );
    // A non-finite value is refused by the number constructor, so the word
    // forms never order an infinity.
    assert_runtime_error("1 / 0 is greater than 0", "Division by zero");
    assert_runtime_error("0 / 0 is less than 1", "Division by zero");
    assert_runtime_error(
        "set x to 1e400 is greater than 0",
        "infinity is not a finite number",
    );
    assert_runtime_error(
        "set x to 1e308 * 1e308 is less than 1e308",
        "infinity is not a finite number",
    );
}

/// The word forms compose with records, functions, loops and nesting: they are
/// an operator like any other, and a phrase that sits inside a loop body or a
/// function call must parse there too. A missing record key is `nothing`,
/// which is not orderable but is equal to `nothing`.
#[test]
fn edge_word_comparisons_of_records_containers_and_nested_values() {
    // A duplicate key keeps the last value, and the comparison sees that one.
    assert_eq!(
        eval("set r to {a: 1, a: 2}\nr.a is greater than 1"),
        yes_no(true)
    );
    assert_eq!(
        eval("set r to {a: 1, a: 2}\nr.a is greater than 2"),
        yes_no(false)
    );
    assert_eq!(
        eval("set r to {a: 1, a: 2}\nr.a is equal to 2"),
        yes_no(true)
    );
    // Key order is not content, so two records written in a different order are
    // equal through the word form.
    assert_eq!(eval("{a: 1, b: 2} is equal to {b: 2, a: 1}"), yes_no(true));
    assert_eq!(eval("{a: 1, b: 2} is not {b: 2, a: 1}"), yes_no(false));
    // A missing key is `nothing`: equal to `nothing`, and not orderable.
    assert_eq!(
        eval("set r to {a: 1}\nr.missing is equal to nothing"),
        yes_no(true)
    );
    assert_eq!(eval("set r to {a: 1}\nr.missing is not 0"), yes_no(true));
    assert_runtime_error(
        "set r to {a: 1}\nr.missing is greater than 0",
        "Cannot compare non-numbers",
    );
    // Nested containers compare by content all the way down.
    assert_eq!(
        eval("{a: {b: [1, {c: 2}]}} is equal to {a: {b: [1, {c: 2}]}}"),
        yes_no(true)
    );
    assert_eq!(
        eval("{a: {b: [1, {c: 2}]}} is equal to {a: {b: [1, {c: 3}]}}"),
        yes_no(false)
    );
    // Nesting a word comparison inside an `and` chain, which is how
    // `SPEC.md` writes a range check.
    let program = "\
set x to 50
set in_range to no
if x is greater than 0 and x is less than 100 then
    set in_range to yes
end
in_range
";
    assert_eq!(eval(program), yes_no(true));
    // The same range check written with the inclusive form at both ends.
    let program = "\
set x to 100
set in_range to no
if x is greater than or equal to 0 and x is less than or equal to 100 then
    set in_range to yes
end
in_range
";
    assert_eq!(eval(program), yes_no(true));
    // Inside a loop body, over the loop variable.
    let program = "\
set count to 0
for each i in [5, 6, 7, 8]
    if i is greater than 6 then
        set count to count + 1
    end
end
count
";
    assert_eq!(eval(program), Value::Number(2.0));
    // Against the result of a function call, and against a record field read
    // through a variable rather than a literal.
    let program = "\
to doubled(a)
    give back a * 2
end
set r to {n: 5}
set kept to no
if doubled(3) is greater than r.n then
    set kept to yes
end
kept
";
    assert_eq!(eval(program), yes_no(true));
}

/// A long left-associated chain of word comparisons is subject to the same
/// nesting guard as every other operator: past the limit it is a clean parser
/// error naming the limit, never a stack overflow, and just inside the limit
/// it parses and reaches the runtime.
#[test]
fn edge_a_long_word_comparison_chain_is_bounded_by_the_nesting_guard() {
    let chain = |n: usize| -> String {
        (1..=n)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" is less than ")
    };
    match parse(tokens(&chain(200))) {
        Err(err) => {
            let message = err.to_string();
            assert!(
                message.contains("nests more than"),
                "an over-long word comparison chain must name the nesting limit, got: {}",
                message
            );
            assert!(err.span().is_some(), "the refusal must carry a source span");
        }
        Ok(_) => panic!("a 200-link word comparison chain must be refused by the nesting guard"),
    }
    // Just inside the guard, the chain parses, and the second link orders a
    // `yes/no` against a number — the type error, not a hang.
    assert_runtime_error(&chain(60), "Cannot compare non-numbers");

    // And `rb run` on a file that uses the word forms inside a long-but-legal
    // chain still exits 0.
    let program = format!("set x to 1\nset hits to 0\n{}\nsay \"ok\"\n", {
        let mut out = String::new();
        for i in 1..=20 {
            if i > 1 {
                out.push_str(" or ");
            }
            out.push_str(&format!("x is equal to {}", i));
        }
        out
    });
    let (ok, stdout, stderr) = rb_run("chain", &program);
    assert!(ok, "a long legal chain exited non-zero\nstderr: {stderr}");
    assert_eq!(stdout.trim_end(), "ok", "the chain must reach its end");
}
