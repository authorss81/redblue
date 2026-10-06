//! The word forms of the two operators `SPEC.md:206` and `SPEC.md:325` document.
//!
//! `mod` was missing from the `KEYWORDS` table in `src/lexer.rs`, so the
//! documented `set remainder to 10 mod 3` lexed `mod` as an identifier and died
//! with `AnalyzerError: Unknown variable 'mod'` — a misleading message for a
//! program that is well-formed Redblue. Only the symbol `%` reached
//! `BinaryOp::Mod`.
//!
//! `BinaryOp::In` existed in `src/runtime.rs`, `src/analyzer.rs`,
//! `src/formatter.rs` and the bytecode backend, but nothing in the parser ever
//! built one, so the documented `if "hello" is in words` died with
//! `ParserError: Unexpected token In`.
//!
//! These tests pin both word forms against their symbol forms, and pin the
//! behaviour of every operand shape and failure the phase requires.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use redblue::bytecode::compile_source;
use redblue::bytecode::vm::BytecodeVm;
use redblue::formatter;
use redblue::lexer::{Lexer, TokenKind};
use redblue::parser::{parse, Statement};
use redblue::{run_isolated, Error, Value};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/rb-mod-in")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

fn tokens(source: &str) -> Vec<redblue::lexer::Token> {
    Lexer::tokenize(source).unwrap_or_else(|e| panic!("source should lex: {}", e))
}

/// Runs `source` on the tree-walking VM and returns the value of its last
/// statement. Only a bare expression statement carries a value out, so a
/// source ending in `set …` or `say …` is rejected rather than left comparing
/// `nothing` against `nothing`.
#[track_caller]
fn eval(source: &str) -> Value {
    let ast = parse(tokens(source)).unwrap_or_else(|e| panic!("`{}` should parse: {}", source, e));
    assert!(
        matches!(
            ast.statements.last().map(|s| &s.statement),
            Some(Statement::Expr(_))
        ),
        "`{}` does not end in a bare expression, so it has no value to assert on",
        source
    );
    let mut vm = redblue::Vm::new();
    vm.run(&ast)
        .unwrap_or_else(|e| panic!("`{}` should run: {}", source, e))
}

/// Runs `source` and returns the runtime error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    let ast = parse(tokens(source)).unwrap_or_else(|e| panic!("`{}` should parse: {}", source, e));
    let mut vm = redblue::Vm::new();
    vm.run(&ast)
        .expect_err(&format!("`{}` should have failed to run", source))
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

fn number(n: f64) -> Value {
    Value::Number(n)
}

fn yes_no(flag: bool) -> Value {
    Value::YesNo(flag)
}

/// Runs `program` through `rb run` and returns `(exit success, stdout)`.
#[track_caller]
fn rb_run(name: &str, program: &str) -> (bool, String, String) {
    let script = scratch_dir(name).join("program.rb");
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

/// Runs `say expr` on both engines and asserts both printed `expected`.
///
/// The bytecode VM does not carry a bare expression's value out of `run`, so
/// the engines are compared over observable behaviour — what they print —
/// rather than over a return value only one of them produces.
#[track_caller]
fn assert_both_say(expr: &str, expected: &str) {
    let program = format!("say {}", expr);

    let ast = parse(tokens(&program)).unwrap_or_else(|e| panic!("`{}` should parse: {}", expr, e));
    let (mut tree_vm, tree_result) = run_isolated(&ast);
    let tree_output = tree_vm.take_output();
    tree_result.unwrap_or_else(|e| panic!("`{}` should run: {}", expr, e));

    let chunk = compile_source(&program)
        .unwrap_or_else(|e| panic!("`{}` should compile to bytecode: {}", expr, e));
    let mut byte_vm = BytecodeVm::new();
    let byte_result = byte_vm.run(&chunk);
    let byte_output = byte_vm.take_output();
    byte_result.unwrap_or_else(|e| panic!("`{}` should run as bytecode: {}", expr, e));

    assert_eq!(
        tree_output, byte_output,
        "the two engines printed differently for `{}`",
        expr
    );
    assert_eq!(tree_output.join("\n"), expected, "`{}`", expr);
}

/// Runs `source` on both engines and asserts both fail with `expected`.
#[track_caller]
fn assert_both_fail(source: &str, expected: &str) {
    let ast = parse(tokens(source)).unwrap_or_else(|e| panic!("`{}` should parse: {}", source, e));
    let (_, tree_result) = run_isolated(&ast);
    let chunk = compile_source(source)
        .unwrap_or_else(|e| panic!("`{}` should compile to bytecode: {}", source, e));
    let byte_result = BytecodeVm::new().run(&chunk);

    for (engine, result) in [("tree-walking", tree_result), ("bytecode", byte_result)] {
        let error = result.expect_err(&format!("`{}` should fail on {}", source, engine));
        assert_eq!(error.label(), "RuntimeError", "`{}` on {}", source, engine);
        assert_eq!(error.message(), expected, "`{}` on {}", source, engine);
    }
}

// ---------------------------------------------------------------------------
// `mod`
// ---------------------------------------------------------------------------

/// The finding, at the lexer: `mod` is a keyword, not an identifier. Without
/// this the parser never sees a `TokenKind::Mod` and the analyzer reports
/// `Unknown variable 'mod'`.
#[test]
fn mod_is_a_keyword_and_not_an_identifier() {
    let lexed = tokens("mod");
    assert_eq!(
        lexed.iter().map(|t| t.kind.clone()).collect::<Vec<_>>(),
        vec![TokenKind::Mod, TokenKind::Eof],
        "`mod` must lex as the modulo operator, not as an identifier"
    );
    assert_eq!((lexed[0].line, lexed[0].column), (1, 1), "`mod` span");

    // The keyword is published to editor tooling, which is asserted by
    // `tooling_grammar_test::grammar_covers_the_lexer_keyword_set`.
    assert!(
        Lexer::keywords().contains(&"mod"),
        "`mod` must appear in the published keyword set"
    );

    // A word that merely starts with `mod` is unaffected: `module` keeps its own
    // keyword and the rest stay identifiers, so reserving `mod` swallowed
    // nothing.
    assert_eq!(tokens("module")[0].kind, TokenKind::Module);
    for word in ["modify", "modern", "modulo", "mode", "moved"] {
        assert_eq!(
            tokens(word)[0].kind,
            TokenKind::Identifier(word.to_string()),
            "`{}` must stay an identifier",
            word
        );
    }
}

/// The documented program from `SPEC.md:206` runs and prints what `%` prints.
#[test]
fn the_word_mod_computes_the_documented_remainder() {
    assert_eq!(eval("10 mod 3"), number(1.0));
    assert_eq!(eval("10 % 3"), number(1.0));
    assert_both_say("10 mod 3", "1");

    // It also parses inside a statement, which is how SPEC.md spells it.
    let program = "\
set remainder to 10 mod 3
say remainder
say 10 % 3
";
    let (ok, stdout, stderr) = rb_run("mod-in-set", program);
    assert!(ok, "`set remainder to 10 mod 3` failed: {}", stderr);
    assert_eq!(
        stdout.trim(),
        "1\n1",
        "`mod` and `%` must print the same remainder"
    );
}

/// `x mod y` and `x % y` are one operator with two spellings, so they must agree
/// on every operand — including the sign of the dividend and the unit and zero
/// divisors.
#[test]
fn the_word_mod_and_the_symbol_percent_agree_on_every_operand() {
    let cases: &[(f64, f64)] = &[
        (10.0, 3.0),
        (-5.0, 3.0), // SPEC.md:224 — remainder takes the dividend's sign
        (5.0, -3.0),
        (-5.0, -3.0),
        (0.0, 3.0),
        (1.0, 1.0), // unit divisor
        (-1.0, 1.0),
        (1.0, -1.0),
        (7.5, 2.0), // fractional operands
        (-7.5, 2.0),
        (2.0, 1e308),
        (1e308, 7.0),
    ];

    for (a, b) in cases {
        let symbol = eval(&format!("{} % {}", a, b));
        let word = eval(&format!("{} mod {}", a, b));
        assert_eq!(
            symbol, word,
            "`{} % {}` and `{} mod {}` disagree",
            a, b, a, b
        );
        let expected = number(a % b);
        assert_eq!(symbol, expected, "`{} % {}`", a, b);
        assert_eq!(word, expected, "`{} mod {}`", a, b);
    }
}

/// `mod` keeps multiplication-level precedence and associativity, so it mixes
/// with `*`, `/`, `+` and `-` exactly as `%` does.
#[test]
fn the_word_mod_keeps_multiplicative_precedence() {
    for (word, symbol) in [
        ("10 mod 2 + 1", "10 % 2 + 1"),
        ("1 + 10 mod 4 * 2", "1 + 10 % 4 * 2"),
        ("2 * 7 mod 5", "2 * 7 % 5"),
        ("(10 mod 3) * 2", "(10 % 3) * 2"),
    ] {
        assert_eq!(
            eval(word),
            eval(symbol),
            "`{}` and `{}` disagree: the word form must bind like the symbol",
            word,
            symbol
        );
    }
}

/// A zero divisor is a clean runtime error from either spelling, named
/// `Modulo by zero` — not a panic and not `NaN`.
#[test]
fn edge_modulo_by_zero_is_a_caught_runtime_error() {
    for source in ["5 mod 0", "5 mod 0.0", "5 mod -0.0", "5 % 0", "0 mod 0"] {
        assert_runtime_error(source, "Modulo by zero");
    }

    assert_both_fail("10 mod 0", "Modulo by zero");

    // And the language can observe it, so it is a catchable error rather than a
    // fatal one: the program exits 0 and the catch clause runs. The message
    // itself is pinned by `assert_both_fail` above.
    let program = "\
set caught to no
try
    set bad to 10 mod 0
catch failure
    set caught to failure
end
say caught
";
    let (ok, stdout, stderr) = rb_run("mod-zero", program);
    assert!(
        ok,
        "`10 mod 0` inside try/catch should be caught: {}",
        stderr
    );
    assert_eq!(stdout.trim(), "error", "the catch clause should have run");
}

/// The type mismatch boundary: `mod` refuses non-numbers on either side, the
/// same way `%` does.
#[test]
fn edge_modulo_rejects_non_numbers() {
    assert_runtime_error("1 mod \"2\"", "Cannot modulo non-numbers");
    assert_runtime_error("\"1\" mod 2", "Cannot modulo non-numbers");
    assert_runtime_error("nothing mod 2", "Cannot modulo non-numbers");
    assert_runtime_error("[1] mod 2", "Cannot modulo non-numbers");
    assert_runtime_error("1 mod [2]", "Cannot modulo non-numbers");
}

// ---------------------------------------------------------------------------
// `is in`
// ---------------------------------------------------------------------------

/// The finding, at the parser: `is in` builds a `BinaryOp::In` instead of
/// dying on the `in` token.
#[test]
fn is_in_takes_the_branch_it_names() {
    let program = "\
set words to [\"a\", \"b\"]
set hit to \"no\"
set miss to \"no\"
if \"a\" is in words then
    set hit to \"yes\"
end
if \"z\" is in words then
    set miss to \"yes\"
end
say hit
say miss
";
    let (ok, stdout, stderr) = rb_run("is-in-branches", program);
    assert!(ok, "`is in` program failed: {}", stderr);
    assert_eq!(stdout.trim(), "yes\nno", "`is in` chose the wrong branches");

    assert_eq!(eval("1 is in [1, 2, 3]"), yes_no(true));
    assert_eq!(eval("4 is in [1, 2, 3]"), yes_no(false));
}

/// `is in` is a comparison, so it sits at comparison precedence: it binds
/// tighter than `and`/`or` and looser than arithmetic.
#[test]
fn is_in_keeps_comparison_precedence() {
    // Arithmetic binds tighter, so the left operand is the computed value.
    assert_eq!(eval("1 + 1 is in [1, 2]"), yes_no(true));
    assert_eq!(eval("2 * 3 is in [6, 7]"), yes_no(true));
    assert_eq!(eval("2 * 3 is in [5, 7]"), yes_no(false));

    // A parenthesised operand is the same value.
    assert_eq!(eval("(1 + 1) is in [2]"), yes_no(true));

    // `and` binds looser, so the membership test is the whole left operand.
    assert_eq!(eval("1 is in [1, 2] and 3 is in [3]"), yes_no(true));
    assert_eq!(eval("1 is in [9] and 3 is in [3]"), yes_no(false));

    // Subtraction binds tighter too, and a negative result is still just a
    // number to compare.
    assert_eq!(eval("3 - 5 is in [-2, 0]"), yes_no(true));
    assert_eq!(eval("3 - 5 is in [2]"), yes_no(false));
}

/// A bare `is` is still equality, and a word that merely starts with `in` is
/// still an identifier. Adding the `in` branch must not swallow either.
#[test]
fn is_before_a_partial_in_word_still_tests_equality() {
    assert_eq!(
        eval("set inside to \"inside\"\n\"inside\" is inside"),
        yes_no(true)
    );
    assert_eq!(
        eval("set inside to \"inside\"\n\"other\" is inside"),
        yes_no(false)
    );

    // `is` with nothing comparable after it is still plain equality.
    assert_eq!(eval("5 is 5"), yes_no(true));
    assert_eq!(eval("5 is not 5"), yes_no(false));
    assert_eq!(eval("5 is equal to 5"), yes_no(true));

    // The other documented comparison words are untouched.
    assert_eq!(eval("5 is greater than 4"), yes_no(true));
    assert_eq!(eval("5 is less than 4"), yes_no(false));
}

/// The empty haystack: nothing is in it, and asking is not an error.
#[test]
fn edge_empty_list_haystack_contains_nothing() {
    assert_eq!(eval("1 is in []"), yes_no(false));
    assert_eq!(eval("\"\" is in []"), yes_no(false));
    assert_eq!(eval("nothing is in []"), yes_no(false));
    assert_eq!(eval("[] is in []"), yes_no(false));

    let program = "\
set found to yes
if \"a\" is in [] then
    set found to yes
else
    set found to no
end
say found
";
    let (ok, stdout, stderr) = rb_run("empty-haystack", program);
    assert!(ok, "empty-haystack program failed: {}", stderr);
    assert_eq!(stdout.trim(), "no", "nothing is in an empty list");
}

/// A haystack that is the empty text is the `type_mismatch` boundary: `in` is
/// defined for lists, so text is a clean error rather than a substring search
/// and rather than `no`.
#[test]
fn edge_empty_text_haystack_is_a_runtime_error() {
    assert_runtime_error("\"a\" is in \"\"", "Right side of 'in' must be a list");
    assert_runtime_error("\"a\" is in \"abc\"", "Right side of 'in' must be a list");
    assert_runtime_error("\"\" is in \"\"", "Right side of 'in' must be a list");
}

/// A `nothing` needle is a value like any other: it is `in` a list only when
/// the list holds `nothing`.
#[test]
fn edge_nothing_needle_matches_only_nothing() {
    assert_eq!(eval("nothing is in [1, 2, 3]"), yes_no(false));
    assert_eq!(eval("nothing is in [nothing]"), yes_no(true));
    assert_eq!(eval("nothing is in []"), yes_no(false));
    assert_eq!(eval("1 is in [nothing, 2]"), yes_no(false));
    assert_eq!(eval("1 is in [nothing, 1]"), yes_no(true));
    assert_eq!(eval("nothing is in [nothing, 1]"), yes_no(true));
}

/// The needle's type never matters — membership is equality — so a text needle
/// against numeric elements and the reverse both answer `no` without faulting.
#[test]
fn edge_needle_of_the_wrong_type_answers_no_rather_than_failing() {
    assert_eq!(eval("1 is in [\"1\"]"), yes_no(false));
    assert_eq!(eval("\"1\" is in [1]"), yes_no(false));
    assert_eq!(eval("1.0 is in [1]"), yes_no(true));
    assert_eq!(eval("yes is in [no]"), yes_no(false));
}

/// The singleton boundary: a one-element list is found only by that element,
/// and `nothing` must not be read as a wildcard.
#[test]
fn edge_singleton_haystack_matches_only_its_one_element() {
    assert_eq!(eval("\"a\" is in [\"a\"]"), yes_no(true));
    assert_eq!(eval("\"b\" is in [\"a\"]"), yes_no(false));
    assert_eq!(eval("0 is in [0]"), yes_no(true));
    assert_eq!(eval("nothing is in [0]"), yes_no(false));
}

/// A record or a number is not a list, so `in` over it must be a catchable
/// runtime error — not `no`, and not a panic.
#[test]
fn edge_membership_in_a_record_or_a_number_is_a_caught_runtime_error() {
    assert_runtime_error("\"a\" is in {a: 1}", "Right side of 'in' must be a list");
    assert_runtime_error("\"a\" is in 5", "Right side of 'in' must be a list");
    assert_runtime_error("1 is in 1.5", "Right side of 'in' must be a list");
    assert_runtime_error("nothing is in 5", "Right side of 'in' must be a list");
    assert_runtime_error("1 is in nothing", "Right side of 'in' must be a list");

    let program = "\
set caught to 0
try
    set a to \"a\" is in {a: 1}
catch error
    set caught to caught + 1
end
try
    set b to \"a\" is in 5
catch error
    set caught to caught + 2
end
say caught
";
    let (ok, stdout, stderr) = rb_run("in-wrong-haystack", program);
    assert!(
        ok,
        "`is in` over a record and a number should be catchable: {}",
        stderr
    );
    assert_eq!(stdout.trim(), "3", "both wrong haystacks should be caught");
}

/// The formatter has to write `in` back the way the parser reads it, or
/// `rb format` turns a valid program into one that no longer parses. `mod` has
/// a symbol, so it may legitimately normalise to `%`.
#[test]
fn edge_the_formatter_round_trips_the_word_forms() {
    // `in` has no symbol of its own, so the formatter has to write the `is`
    // back: emitting a bare `in` would turn a valid program into one that no
    // longer parses. `mod` has `%`, so normalising to it is fine.
    let formatted = formatter::format("if \"a\" is in [\"a\"] then\nsay 1\nend\n")
        .expect("source should format");
    assert!(
        formatted.contains("is in"),
        "the formatter must write membership back as `is in`, got {:?}",
        formatted
    );

    let ast = parse(tokens(&formatted))
        .unwrap_or_else(|e| panic!("formatted source should parse: {}", e));
    let mut vm = redblue::Vm::new();
    vm.run(&ast).expect("formatted source should run");
    assert_eq!(
        vm.take_output(),
        vec!["1".to_string()],
        "the formatted program must still run the same branch"
    );

    let formatted = formatter::format("say 10 mod 3\n").expect("source should format");
    assert!(
        !formatted.contains("mod"),
        "`mod` has a symbol, so the formatter may normalise it, got {:?}",
        formatted
    );
    let ast = parse(tokens(&formatted)).expect("formatted `mod` should parse");
    let mut vm = redblue::Vm::new();
    vm.run(&ast).expect("formatted `mod` should run");
    assert_eq!(vm.take_output(), vec!["1".to_string()]);
}

/// Nested and unicode haystacks: membership is structural equality all the way
/// down, and text comparison is by code point.
#[test]
fn edge_nested_and_unicode_haystacks_are_compared_structurally() {
    assert_eq!(eval("[1, 2] is in [[1, 2], [3]]"), yes_no(true));
    assert_eq!(eval("[1, 3] is in [[1, 2], [3]]"), yes_no(false));
    assert_eq!(eval("{a: 1} is in [{a: 1}, {b: 2}]"), yes_no(true));

    assert_eq!(eval("\"héllo\" is in [\"héllo\", \"日本\"]"), yes_no(true));
    assert_eq!(eval("\"日本\" is in [\"héllo\", \"日本\"]"), yes_no(true));
    assert_eq!(eval("\"👍\" is in [\"👍\"]"), yes_no(true));
    assert_eq!(eval("\"́e\" is in [\"è\"]"), yes_no(false));
    assert_eq!(eval("\"مرحبا\" is in [\"مرحبا\"]"), yes_no(true));
}

/// The bytecode engine resolves `in` and `mod` through the same runtime, so a
/// program written with the word forms compiles and runs identically.
#[test]
fn the_word_forms_run_on_the_bytecode_vm_too() {
    assert_both_say("10 mod 3", "1");
    assert_both_say("1 is in [1, 2]", "yes");
    assert_both_say("3 is in [1, 2]", "no");
    assert_both_say("1 is in []", "no");
    assert_both_say("\"a\" is in [\"a\", \"b\"]", "yes");

    assert_both_fail("10 mod 0", "Modulo by zero");
    assert_both_fail("\"a\" is in 5", "Right side of 'in' must be a list");

    // The modulo failure is catchable on the bytecode engine too.
    let chunk = compile_source("try\n set bad to 10 mod 0\ncatch failure\n say \"caught\"\nend")
        .expect("`mod` by zero should compile to bytecode");
    let mut vm = BytecodeVm::new();
    vm.run(&chunk).expect("bytecode should run");
    assert_eq!(vm.take_output(), vec!["caught".to_string()]);
}
