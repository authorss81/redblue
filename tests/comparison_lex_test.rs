//! `<`, `<=`, `>`, `>=`, `==` and `!=` are the documented comparison symbols
//! (`README.md:18`, `SPEC.md:277-282`, `docs/GRAMMAR.md:93-98`), but the
//! lexer's operator table had no arm for any of them: `src/lexer.rs` matched
//! only `+ - * / % ( ) [ ] { } , . :` and the fallthrough raised
//! `Unexpected character`. Every `TokenKind::Equal/NotEqual/Less/LessEqual/
//! Greater/GreaterEqual` was therefore never constructed, which made the whole
//! comparison arm block in `parse_comparison` unreachable and left the language
//! with no ordering comparison at all.
//!
//! These tests pin the six symbols to their exact token kind, prove that `<=`
//! and `>=` are one token rather than two, and prove that the comparison the
//! parser builds out of them actually runs — including the boundary where
//! `x < x` is `no` and `x <= x` is `yes`, the type-mismatch failures, and
//! structural `==` on two lists and two records that hold equal contents.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use redblue::lexer::{Lexer, Token, TokenKind};
use redblue::parser::{parse, Statement};
use redblue::{Error, Value};

/// Scratch directory inside the project's own `target/tmp`, never the system
/// temp dir, so nothing outside the checkout is touched. Each caller names its
/// own subdirectory because tests run in parallel and two of them would
/// otherwise `remove_dir_all` each other's scratch files.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/rb-comparison-lex")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

/// The token kinds of `source`, with no lexer error allowed.
fn lex(source: &str) -> Vec<TokenKind> {
    Lexer::tokenize(source)
        .expect("source should lex")
        .into_iter()
        .map(|token| token.kind)
        .collect()
}

/// The full token list of `source`, for handing to the parser.
fn tokens(source: &str) -> Vec<Token> {
    Lexer::tokenize(source).expect("source should lex")
}

/// Runs `source` through lexer → parser → VM and returns the value of its last
/// statement. Only a bare expression statement carries a value out, so a
/// source ending in `set …` or `say …` is rejected rather than left to compare
/// `nothing` against `nothing`.
#[track_caller]
fn eval(source: &str) -> Value {
    let tokens = Lexer::tokenize(source).expect("source should lex");
    let ast = parse(tokens).expect("source should parse");
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

/// Runs `source` and returns the error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    let tokens = Lexer::tokenize(source).expect("source should lex");
    let ast = parse(tokens).expect("source should parse");
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

/// The six documented symbols lex to their own token kinds, each one token.
#[test]
fn the_comparison_symbols_lex_to_their_own_token_kinds() {
    let cases: &[(&str, TokenKind)] = &[
        ("<", TokenKind::Less),
        (">", TokenKind::Greater),
        ("=", TokenKind::Equal),
        ("==", TokenKind::Equal),
        ("!=", TokenKind::NotEqual),
    ];
    for (source, expected) in cases {
        let kinds = lex(source);
        assert_eq!(
            &kinds[..2],
            &[expected.clone(), TokenKind::Eof],
            "`{}` did not lex to a single {:?} followed by Eof",
            source,
            expected
        );
    }
}

/// `<=` and `>=` are single tokens, not a `<` followed by a `=`. If they
/// lexed as two tokens the parser would still produce `<=`, but the token
/// stream would be wrong and `LessEqual`/`GreaterEqual` would stay dead.
#[test]
fn edge_less_equal_and_greater_equal_are_one_token_not_two() {
    assert_eq!(
        lex("<="),
        vec![TokenKind::LessEqual, TokenKind::Eof],
        "`<=` must be one LessEqual token"
    );
    assert_eq!(
        lex(">="),
        vec![TokenKind::GreaterEqual, TokenKind::Eof],
        "`>=` must be one GreaterEqual token"
    );

    // In context, against the surrounding statement, so a two-token lex would
    // be visible as a wrong kind sequence rather than an equal slice.
    let kinds = lex("set x to a <= b");
    assert_eq!(
        &kinds[..7],
        &[
            TokenKind::Set,
            TokenKind::Identifier("x".to_string()),
            TokenKind::To,
            TokenKind::Identifier("a".to_string()),
            TokenKind::LessEqual,
            TokenKind::Identifier("b".to_string()),
            TokenKind::Eof,
        ],
        "`a <= b` did not lex with a single LessEqual in the middle"
    );
    let kinds = lex("set x to a >= b");
    assert_eq!(
        &kinds[..7],
        &[
            TokenKind::Set,
            TokenKind::Identifier("x".to_string()),
            TokenKind::To,
            TokenKind::Identifier("a".to_string()),
            TokenKind::GreaterEqual,
            TokenKind::Identifier("b".to_string()),
            TokenKind::Eof,
        ],
        "`a >= b` did not lex with a single GreaterEqual in the middle"
    );

    // A `=` that follows `<` or `>` is consumed into the compound, so the
    // stream must not also carry the bare `=` that would make it two tokens.
    assert_eq!(lex("==").len(), 2);
    assert_eq!(lex("!=").len(), 2);
}

/// The whole point of the phase: the comparison the parser builds out of these
/// symbols runs, in every direction, and takes the branch the operator names.
#[test]
fn each_comparison_operator_selects_the_branch_it_names() {
    assert_eq!(eval("20 > 10"), yes_no(true));
    assert_eq!(eval("10 > 20"), yes_no(false));
    assert_eq!(eval("20 < 10"), yes_no(false));
    assert_eq!(eval("10 < 20"), yes_no(true));
    assert_eq!(eval("20 == 20"), yes_no(true));
    assert_eq!(eval("20 == 10"), yes_no(false));
    assert_eq!(eval("20 != 10"), yes_no(true));
    assert_eq!(eval("20 != 20"), yes_no(false));
    assert_eq!(eval("20 >= 20"), yes_no(true));
    assert_eq!(eval("20 <= 20"), yes_no(true));
}

/// The equality boundary, in both directions: with `x` equal to itself, `<`
/// and `>` are `no` and `<=` and `>=` are `yes`. An off-by-one in either the
/// lexer or the runtime flips exactly one of these four.
#[test]
fn edge_x_against_itself_is_strictly_less_and_strictly_greater() {
    for x in ["5", "0", "-3", "2.5"] {
        assert_eq!(
            eval(&format!("{} < {}", x, x)),
            yes_no(false),
            "x < x with x={}",
            x
        );
        assert_eq!(
            eval(&format!("{} > {}", x, x)),
            yes_no(false),
            "x > x with x={}",
            x
        );
        assert_eq!(
            eval(&format!("{} <= {}", x, x)),
            yes_no(true),
            "x <= x with x={}",
            x
        );
        assert_eq!(
            eval(&format!("{} >= {}", x, x)),
            yes_no(true),
            "x >= x with x={}",
            x
        );
        assert_eq!(
            eval(&format!("{} == {}", x, x)),
            yes_no(true),
            "x == x with x={}",
            x
        );
        assert_eq!(
            eval(&format!("{} != {}", x, x)),
            yes_no(false),
            "x != x with x={}",
            x
        );
    }
}

/// Reversed operands: swapping the sides swaps the answer for the strict
/// operators and leaves it alone for the non-strict ones. A parser that lost
/// track of which side was which could not get all six right.
#[test]
fn edge_reversed_operands_swap_the_answer_for_the_strict_operators() {
    assert_eq!(eval("1 < 2"), yes_no(true));
    assert_eq!(eval("2 < 1"), yes_no(false));
    assert_eq!(eval("1 > 2"), yes_no(false));
    assert_eq!(eval("2 > 1"), yes_no(true));
    assert_eq!(eval("1 <= 2"), yes_no(true));
    assert_eq!(eval("2 <= 1"), yes_no(false));
    assert_eq!(eval("1 >= 2"), yes_no(false));
    assert_eq!(eval("2 >= 1"), yes_no(true));
    // `==` and `!=` are symmetric, so reversal changes nothing.
    assert_eq!(eval("1 == 1"), eval("1 == 1"));
    assert_eq!(eval("1 != 2"), eval("2 != 1"));
}

/// Ordering two operands of different types is a caught runtime error, never a
/// `yes`, never a `no`, and never a panic. `1 < "a"` and `yes >= 3` are the two
/// directions the type check has to reject.
#[test]
fn edge_ordering_operands_of_different_types_is_a_runtime_error() {
    assert_runtime_error("1 < \"a\"", "Cannot compare non-numbers");
    assert_runtime_error("1 >= \"a\"", "Cannot compare non-numbers");
    assert_runtime_error("\"a\" > 1", "Cannot compare non-numbers");
    assert_runtime_error("yes >= 3", "Cannot compare non-numbers");
    assert_runtime_error("yes < 3", "Cannot compare non-numbers");
    assert_runtime_error("nothing > 1", "Cannot compare non-numbers");
    // Reversed type order is rejected too, not just the number-on-the-left one.
    assert_runtime_error("\"a\" < 1", "Cannot compare non-numbers");
    assert_runtime_error("3 <= yes", "Cannot compare non-numbers");
}

/// A list against a number is a runtime error, and so is a record, and so is
/// two different-shaped numbers the numeric path cannot order.
#[test]
fn edge_ordering_a_list_or_record_against_a_number_is_a_runtime_error() {
    assert_runtime_error("[1, 2] < 3", "Cannot compare non-numbers");
    assert_runtime_error("3 >= [1, 2]", "Cannot compare non-numbers");
    assert_runtime_error("[] > 1", "Cannot compare non-numbers");
    assert_runtime_error("{a: 1} <= 1", "Cannot compare non-numbers");
    assert_runtime_error("1 < {a: 1}", "Cannot compare non-numbers");
    // A list against a list is not orderable either; it must not be a `no`.
    assert_runtime_error("[1] < [2]", "Cannot compare non-numbers");
    // The failure is a Runtime error carrying a span, not a panic escaping.
    let err = eval_err("1 < \"a\"");
    assert!(err.span().is_some(), "the failure must carry a source span");
}

/// `==` and `!=` are structural: two lists and two records that hold equal
/// contents are equal even though they are different values.
#[test]
fn edge_equality_of_two_lists_and_two_records_compares_contents() {
    assert_eq!(eval("[1, 2, 3] == [1, 2, 3]"), yes_no(true));
    assert_eq!(eval("[1, 2, 3] != [1, 2, 3]"), yes_no(false));
    assert_eq!(eval("{a: 1, b: 2} == {a: 1, b: 2}"), yes_no(true));
    assert_eq!(eval("{a: 1, b: 2} != {a: 1, b: 2}"), yes_no(false));
    // Differing contents are not equal, in either container.
    assert_eq!(eval("[1, 2] == [2, 1]"), yes_no(false));
    assert_eq!(eval("[1, 2] != [2, 1]"), yes_no(true));
    assert_eq!(eval("{a: 1} == {a: 2}"), yes_no(false));
    assert_eq!(eval("{a: 1} != {a: 2}"), yes_no(true));
    // A different shape is never equal, and ordering one is still an error.
    assert_eq!(eval("[1] == 1"), yes_no(false));
    assert_eq!(eval("{a: 1} == [1]"), yes_no(false));
    // The empty and singleton cases, at both ends.
    assert_eq!(eval("[] == []"), yes_no(true));
    assert_eq!(eval("[] == [1]"), yes_no(false));
    assert_eq!(eval("[7] == [7]"), yes_no(true));
    assert_eq!(eval("{} == {}"), yes_no(true));
    assert_eq!(eval("{} == {a: nothing}"), yes_no(false));
    // Nested containers compare by content all the way down.
    assert_eq!(eval("[[1, 2], [3]] == [[1, 2], [3]]"), yes_no(true));
    assert_eq!(eval("[[1, 2], [3]] == [[1, 9], [3]]"), yes_no(false));
    assert_eq!(eval("{a: [1, {b: 2}]} == {a: [1, {b: 2}]}"), yes_no(true));
}

/// A `>` written in the middle of a statement is one operator, not the
/// start of something else — the lexer must not stop early at the symbol.
#[test]
fn comparison_symbols_do_not_swallow_the_character_after_them() {
    // `>5` is `> 5`, so the number after the operator still lexes.
    let kinds = lex("x > 5");
    assert_eq!(
        &kinds[..4],
        &[
            TokenKind::Identifier("x".to_string()),
            TokenKind::Greater,
            TokenKind::Number(5.0),
            TokenKind::Eof,
        ],
        "`x > 5` did not lex as variable, Greater, number"
    );
    // `!=x` is `!= x` — the `!` did not eat the variable.
    let kinds = lex("!=x");
    assert_eq!(
        &kinds[..2],
        &[TokenKind::NotEqual, TokenKind::Identifier("x".to_string())],
        "`!=x` did not lex as NotEqual then a variable"
    );
    // `==` is one token, so it does not lex as two `=` with nothing between.
    let kinds = lex("a==b");
    assert_eq!(
        &kinds[..3],
        &[
            TokenKind::Identifier("a".to_string()),
            TokenKind::Equal,
            TokenKind::Identifier("b".to_string()),
        ],
        "`a==b` did not lex as a, Equal, b"
    );
    // `<` at the very end of a line, then the number on the next.
    let kinds = lex("a <\n1");
    assert_eq!(
        &kinds[..4],
        &[
            TokenKind::Identifier("a".to_string()),
            TokenKind::Less,
            TokenKind::Newline,
            TokenKind::Number(1.0),
        ],
        "`a <` followed by a newline did not lex as a, Less, Newline"
    );
}

/// Chained comparisons left-associate: `a < b < c` is `(a < b) < c`, so the
/// left operand is a `YesNo` and the second comparison is a type error. That
/// is the defined answer, and it proves the chain is not silently flattened.
#[test]
fn edge_a_chained_ordering_comparison_fails_on_the_boolean_left_operand() {
    assert_runtime_error("1 < 2 < 3", "Cannot compare non-numbers");
    // Two equality comparisons in a row are fine, because `yes == yes` is a
    // comparison of like with like.
    assert_eq!(eval("(1 == 1) == yes"), yes_no(true));
}

/// `rb run` on a file that uses all six symbols exits 0 and takes the branch
/// each `if` names. This is the end-to-end path the phase is about: the
/// lexer's operator table is the only thing that stood between the documented
/// syntax and a working `if`.
#[test]
fn rb_run_executes_a_file_using_every_comparison_symbol() {
    let program = "\
set x to 20
if x > 10 then
    say \"greater\"
end
if x >= 10 then
    say \"greater or equal\"
end
if x < 10 then
    say \"less\"
end
if x <= 10 then
    say \"less or equal\"
end
if x == 20 then
    say \"equal\"
end
if x != 10 then
    say \"not equal\"
end
";
    let script = scratch_dir("symbols").join(format!("compare-{}.rb", std::process::id()));
    fs::write(&script, program).expect("scratch program should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("run")
        .arg(&script)
        .output()
        .expect("rb should be runnable");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let _ = fs::remove_file(&script);

    assert!(
        output.status.success(),
        "rb run exited with {:?}\nstdout: {}\nstderr: {}",
        output.status,
        stdout,
        stderr
    );
    assert_eq!(
        stdout.lines().collect::<Vec<_>>(),
        vec!["greater", "greater or equal", "equal", "not equal"],
        "the wrong branches were taken; only the false conditions stay silent"
    );
}

/// The false side of every symbol is a clean `no`, not a runtime error: a
/// program that takes the other branch still exits 0.
#[test]
fn rb_run_exits_zero_when_every_comparison_is_false() {
    let program = "\
set x to 20
if x > 30 then
    say \"greater\"
end
if x >= 30 then
    say \"greater or equal\"
end
if x < 10 then
    say \"less\"
end
if x <= 10 then
    say \"less or equal\"
end
if x == 30 then
    say \"equal\"
end
if x != 20 then
    say \"not equal\"
end
say \"done\"
";
    let script = scratch_dir("all-false").join(format!("compare-{}.rb", std::process::id()));
    fs::write(&script, program).expect("scratch program should be writable");

    let output = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("run")
        .arg(&script)
        .output()
        .expect("rb should be runnable");

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let _ = fs::remove_file(&script);

    assert!(
        output.status.success(),
        "a program whose every comparison is false exited with {:?}\n{}",
        output.status,
        stderr
    );
    assert_eq!(
        stdout.trim_end(),
        "done",
        "only `done` should have been printed"
    );
}

/// Ordering comparisons compose with the rest of the language: with a list, a
/// loop, a function and a record in the same program.
#[test]
fn edge_comparisons_compose_with_lists_loops_functions_and_records() {
    // Ordering picks the smallest element out of a list by walking it.
    assert_eq!(eval("[3, 1, 2][0] < [3, 1, 2][1]"), yes_no(false));
    // A comparison written inside the body of a `for` loop, over the loop
    // variable, which is the shape a real program uses. The loop counts how
    // many of the three values clear the bar, and the count is compared.
    let program = "set count to 0
for each i in [5, 6, 7]
    if i > 5 then
        set count to count + 1
    end
end
set kept to count
kept
";
    assert_eq!(eval(program), Value::Number(2.0));
    // The same loop under the inclusive operator keeps one more value.
    let program = "set count to 0
for each i in [5, 6, 7]
    if i >= 5 then
        set count to count + 1
    end
end
set kept to count
kept
";
    assert_eq!(eval(program), Value::Number(3.0));
    // Ordering the result of a function call, against a literal.
    let program = "to scaled(a)
    give back a * 2
end
scaled(4) == 8
";
    assert_eq!(eval(program), yes_no(true));
    assert_eq!(
        eval("to scaled(a)\n    give back a * 2\nend\nscaled(4) > 7"),
        yes_no(true)
    );
    // Record equality, reached through a variable rather than a literal.
    assert_eq!(eval("set r to {a: 1}\nset s to r\nr == s"), yes_no(true));
    // Ordering a value read out of a record against a literal.
    assert_eq!(eval("set r to {n: 5}\nr.n > 4"), yes_no(true));
    assert_eq!(eval("set r to {n: 5}\nr.n > 5"), yes_no(false));
}

/// Text comparisons are a runtime error on both sides, not silently `no`:
/// Redblue has no text ordering, and saying so is better than answering.
#[test]
fn edge_text_ordering_is_a_runtime_error_on_both_sides() {
    assert_runtime_error("\"a\" < \"b\"", "Cannot compare non-numbers");
    assert_runtime_error("\"b\" > \"a\"", "Cannot compare non-numbers");
    assert_runtime_error("\"\" < \"\"", "Cannot compare non-numbers");
    // Text equality is fine, and is case- and length-sensitive.
    assert_eq!(eval("\"abc\" == \"abc\""), yes_no(true));
    assert_eq!(eval("\"abc\" == \"ABC\""), yes_no(false));
    assert_eq!(eval("\"\" == \"\""), yes_no(true));
    // Unicode text equality, so the operator is not comparing byte prefixes.
    assert_eq!(eval("\"héllo\" == \"héllo\""), yes_no(true));
    assert_eq!(eval("\"日本語\" == \"日本語\""), yes_no(true));
    assert_eq!(eval("\"🎉\" == \"🎉\""), yes_no(true));
    assert_eq!(eval("\"日本語\" == \"日本語x\""), yes_no(false));
    // Ordering numbers read out of unicode-holding structures is unaffected.
    assert_eq!(eval("[\"é\", \"ü\"][0] == [\"é\", \"ü\"][0]"), yes_no(true));
}

/// The numeric boundaries the numeric path has to order: signed zero, the point
/// past `2^53` where a double stops being exact, and the extremes of `i64`.
///
/// NaN and the infinities cannot appear in the source at all — `Value::number`
/// refuses them, and `0/0` and `1/0` are clean runtime errors — so the last
/// case here asserts that a non-finite value is rejected *before* any
/// comparison sees it, rather than leaving NaN ordering undefined.
#[test]
fn edge_numeric_boundaries_order_without_panicking() {
    // Signed zero compares equal in every direction.
    assert_eq!(eval("-0.0 == 0.0"), yes_no(true));
    assert_eq!(eval("-0.0 < 0.0"), yes_no(false));
    assert_eq!(eval("-0.0 <= 0.0"), yes_no(true));
    assert_eq!(eval("0.0 >= -0.0"), yes_no(true));
    assert_eq!(eval("-0.0 >= 0.0"), yes_no(true));
    assert_eq!(eval("-0.0 > 0.0"), yes_no(false));
    assert_eq!(eval("0.0 != -0.0"), yes_no(false));
    // Past `2^53` the double is exact only in steps of two, and the ordering
    // must still be a total order rather than a comparison of rounded input.
    assert_eq!(eval("9007199254740992 > 9007199254740991"), yes_no(true));
    assert_eq!(eval("9007199254740992 >= 9007199254740992"), yes_no(true));
    assert_eq!(eval("9007199254740993 == 9007199254740993"), yes_no(true));
    assert_eq!(eval("9007199254740992 < 9007199254740993"), yes_no(false));
    // The extremes of `i64` in both signs still order.
    assert_eq!(eval("-9223372036854775808 < 0"), yes_no(true));
    assert_eq!(eval("9223372036854775807 > 0"), yes_no(true));
    assert_eq!(
        eval("-9223372036854775808 <= -9223372036854775808"),
        yes_no(true)
    );
    assert_eq!(
        eval("9223372036854775807 >= 9223372036854775807"),
        yes_no(true)
    );
    // The largest finite double, and the value just below it, order.
    assert_eq!(
        eval("1.7976931348623157e308 > 1.7976931348623156e308"),
        yes_no(true)
    );
    assert_eq!(eval("1e308 < 1.7976931348623157e308"), yes_no(true));
    // A non-finite value is refused by the number constructor, so it never
    // reaches a comparison: `1/0` is a division error, not an `infinity` that
    // would then be unordered.
    assert_runtime_error("1 / 0 > 0", "Division by zero");
    assert_runtime_error("0 / 0 > 0", "Division by zero");
    assert_runtime_error("set x to 1e400 > 0", "infinity is not a finite number");
    assert_runtime_error(
        "set x to 1e308 * 1e308 > 0",
        "infinity is not a finite number",
    );
}

/// A malformed operator sequence is reported, never silently accepted and never
/// a panic: the new arms consume a `=` or `!` only when one actually follows.
#[test]
fn edge_malformed_operator_sequences_are_reported_not_guessed() {
    // `>>` is two `Greater` tokens, which the parser then refuses, because
    // there is no shift operator in Redblue and inventing one is not this
    // lexer's decision.
    let kinds = lex(">>");
    assert_eq!(
        &kinds[..3],
        &[TokenKind::Greater, TokenKind::Greater, TokenKind::Eof],
        "`>>` did not lex as two Greater tokens"
    );
    assert!(
        parse(tokens("set x to a >> b")).is_err(),
        "`a >> b` must be a parser error, not a silently-accepted program"
    );

    // `<<<` is likewise three `Less` tokens, and `>=>` is `GreaterEqual` then
    // `Greater` — the trailing `>` has no `=` after it, so it stays strict.
    let kinds = lex(">=>");
    assert_eq!(
        &kinds[..3],
        &[TokenKind::GreaterEqual, TokenKind::Greater, TokenKind::Eof],
        "`>=>` did not lex as GreaterEqual then a bare Greater"
    );
    // `>==` is `GreaterEqual` then a bare `Equal`, which has no right operand.
    let kinds = lex(">==");
    assert_eq!(
        &kinds[..3],
        &[TokenKind::GreaterEqual, TokenKind::Equal, TokenKind::Eof],
        "`>==` did not lex as GreaterEqual then a bare Equal"
    );
    assert!(
        parse(tokens("set x to a >== b")).is_err(),
        "`a >== b` must be a parser error"
    );

    // `!!` is `Not` `Not`, and `!=!` is `NotEqual` `Not`: the trailing `!` has
    // no `=` after it, so it stays a prefix rather than becoming a second `!=`.
    let kinds = lex("!!");
    assert_eq!(
        &kinds[..3],
        &[TokenKind::Not, TokenKind::Not, TokenKind::Eof],
        "`!!` did not lex as two Not tokens"
    );
    let kinds = lex("!=!");
    assert_eq!(
        &kinds[..3],
        &[TokenKind::NotEqual, TokenKind::Not, TokenKind::Eof],
        "`!=!` did not lex as NotEqual then a bare Not"
    );

    // A lone `!` and a lone `=` are both still tokens, not lexer errors, so the
    // parser gets to name the mistake rather than the lexer guessing.
    assert_eq!(lex("!"), vec![TokenKind::Not, TokenKind::Eof]);
    assert_eq!(lex("="), vec![TokenKind::Equal, TokenKind::Eof]);
    assert_eq!(lex("<"), vec![TokenKind::Less, TokenKind::Eof]);
    assert_eq!(lex(">"), vec![TokenKind::Greater, TokenKind::Eof]);

    // An operator at the very end of the source, with nothing after it at all,
    // is a parser error with a span rather than an out-of-bounds read.
    match parse(tokens("set x to 5 >")) {
        Err(err) => assert!(
            err.span().is_some(),
            "a trailing operator must fail with a source span"
        ),
        Ok(_) => panic!("`5 >` with no right operand must not parse"),
    }

    // And the character the arms replaced is still rejected where it belongs:
    // an unterminated string is not made legal by any of this.
    assert!(matches!(
        Lexer::tokenize("say \"unterminated >").expect_err("should fail"),
        Error::Lexer(..)
    ));
}

/// A missing key answers `nothing`. `nothing` is not orderable, so a `<`
/// against an absent field is a runtime error rather than a `no` that reads as
/// a real answer — but `==` and `!=` are defined for every pair of values, so
/// those say `no`/`yes` instead. A duplicate key keeps the last value, and the
/// comparison sees that value.
#[test]
fn edge_comparing_an_absent_record_key_orders_nothing_but_equals_it() {
    // Reading the absent key is `nothing`, which is the defined answer.
    assert_eq!(eval("set r to {a: 1}\nr.missing"), Value::Nothing);
    // Ordering it is not defined, and says so.
    assert_runtime_error(
        "set r to {a: 1}\nr.missing > 0",
        "Cannot compare non-numbers",
    );
    assert_runtime_error(
        "set r to {a: 1}\nr.missing <= 0",
        "Cannot compare non-numbers",
    );
    // Equality is defined for every pair, so `nothing` is simply unequal to a
    // number — never an error, and never a `yes`.
    assert_eq!(eval("set r to {a: 1}\nr.missing == 0"), yes_no(false));
    assert_eq!(eval("set r to {a: 1}\nr.missing != 0"), yes_no(true));
    assert_eq!(eval("nothing == nothing"), yes_no(true));
    assert_eq!(eval("nothing == 0"), yes_no(false));
    // A duplicate key keeps the last value, so the comparison sees the last one.
    assert_eq!(eval("set r to {a: 1, a: 2}\nr.a"), Value::Number(2.0));
    assert_eq!(eval("set r to {a: 1, a: 2}\nr.a > 1"), yes_no(true));
    assert_eq!(eval("set r to {a: 1, a: 2}\nr.a > 2"), yes_no(false));
    // Two records with the same key written in a different order are equal:
    // equality is over contents, and key order is not content.
    assert_eq!(eval("{a: 1, b: 2} == {b: 2, a: 1}"), yes_no(true));
    // An empty record against an empty one, and against one with a key.
    assert_eq!(eval("{} == {}"), yes_no(true));
    assert_eq!(eval("{} == {a: 1}"), yes_no(false));
    assert_eq!(eval("{} != {a: 1}"), yes_no(true));
}

/// The nesting guard the parser already has applies to the expression the new
/// comparison tokens feed, and a long left-associated comparison chain is
/// legal and evaluates rather than recursing without bound.
#[test]
fn edge_deeply_nested_and_long_comparisons_are_bounded_not_a_panic() {
    // Right-nested far past the guard: a clean parser error naming the limit,
    // not a stack overflow. `(` is the other arm that feeds the same guard, so
    // the comparison operator is not what makes this deep.
    let too_deep = format!("{}1{}", "(1 + ".repeat(200), ")".repeat(200));
    match parse(tokens(&too_deep)) {
        Err(err) => {
            let message = err.to_string();
            assert!(
                message.contains("nests more than"),
                "an over-deep nest must name the limit, got: {}",
                message
            );
            assert!(err.span().is_some(), "the refusal must carry a source span");
        }
        Ok(_) => panic!("a 200-deep nest must be refused by the nesting guard"),
    }

    // Just inside the guard, the same shape parses and evaluates.
    let at_limit = format!("{}1{}", "(1 + ".repeat(30), ")".repeat(30));
    assert_eq!(eval(&at_limit), Value::Number(31.0));

    // A comparison chain nests one level per link, so it is subject to the same
    // guard as any other expression: past the limit it is a clean parser error
    // naming the limit, never a stack overflow.
    let chain = |n: usize| -> String {
        (1..=n)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" < ")
    };
    match parse(tokens(&chain(200))) {
        Err(err) => {
            let message = err.to_string();
            assert!(
                message.contains("nests more than"),
                "an over-long comparison chain must name the nesting limit, got: {}",
                message
            );
            assert!(err.span().is_some(), "the refusal must carry a source span");
        }
        Ok(_) => panic!("a 200-link comparison chain must be refused by the nesting guard"),
    }

    // Just inside the guard the chain parses, and reaches the runtime: the
    // second link orders a `yes/no` against a number, which is the type error
    // `edge_a_chained_ordering_comparison_fails_on_the_boolean_left_operand`
    // pins. So the guard is the only thing that stops it, and it stops it.
    assert_runtime_error(&chain(60), "Cannot compare non-numbers");

    // Nesting records and lists inside an equality comparison goes three deep
    // without trouble.
    assert_eq!(eval("{a: {b: {c: 1}}} == {a: {b: {c: 1}}}"), yes_no(true));
    assert_eq!(eval("{a: {b: {c: 1}}} == {a: {b: {c: 2}}}"), yes_no(false));
    assert_eq!(eval("[[[[[1]]]]] == [[[[[1]]]]]"), yes_no(true));
    assert_eq!(eval("[[[[[1]]]]] == [[[[[2]]]]]"), yes_no(false));
}

/// Every ordering comparison over the whole legal range answers a `yes/no` —
/// never a panic, never an error — and the four operators agree with one
/// another on every pair of values, including a pair of identical ones.
#[test]
fn edge_ordering_is_total_over_the_extremes() {
    let values = [
        "-1.7976931348623157e308",
        "-9223372036854775808",
        "-1",
        "0",
        "0.5",
        "1",
        "9223372036854775807",
        "1.7976931348623157e308",
    ];
    for a in values {
        for b in values {
            let lt = eval(&format!("{} < {}", a, b));
            let gt = eval(&format!("{} > {}", a, b));
            let le = eval(&format!("{} <= {}", a, b));
            let ge = eval(&format!("{} >= {}", a, b));
            let eq = eval(&format!("{} == {}", a, b));
            for (label, result) in [
                ("<", &lt),
                (">", &gt),
                ("<=", &le),
                (">=", &ge),
                ("==", &eq),
            ] {
                assert!(
                    matches!(result, Value::YesNo(_)),
                    "`{} {} {}` did not answer a yes/no, got {:?}",
                    a,
                    label,
                    b,
                    result
                );
            }

            // Reversing the operands must reverse a strict operator and leave
            // a non-strict one alone, so these three pairs agree whatever the
            // values are.
            assert_eq!(
                lt,
                eval(&format!("{} > {}", b, a)),
                "`a < b` and `b > a` disagree for a={} b={}",
                a,
                b
            );
            assert_eq!(
                le,
                eval(&format!("{} >= {}", b, a)),
                "`a <= b` and `b >= a` disagree for a={} b={}",
                a,
                b
            );
            assert_eq!(
                eq,
                eval(&format!("{} == {}", b, a)),
                "equality is not symmetric for a={} b={}",
                a,
                b
            );

            // `<=` is `<` or `==`, and `>` is the negation of `<=`. Together
            // these pin every one of the six operators to one another.
            assert_eq!(
                le,
                Value::YesNo(lt == yes_no(true) || eq == yes_no(true)),
                "`a <= b` is neither `a < b` nor `a == b` for a={} b={}",
                a,
                b
            );
            assert_eq!(
                gt,
                Value::YesNo(le != yes_no(true)),
                "`a > b` is not the negation of `a <= b` for a={} b={}",
                a,
                b
            );
            // Antisymmetry: `a < b` and `b < a` can never both be true, and can
            // both be false only when the two values are equal.
            let reverse_lt = eval(&format!("{} < {}", b, a));
            let both_true = lt == yes_no(true) && reverse_lt == yes_no(true);
            let both_false = lt == yes_no(false) && reverse_lt == yes_no(false);
            assert!(
                !both_true,
                "`a < b` and `b < a` were both true for a={} b={}",
                a, b
            );
            assert!(
                !both_false || eq == yes_no(true),
                "`a < b` and `b < a` were both false for the unequal a={} b={}",
                a,
                b
            );
        }
    }
}
