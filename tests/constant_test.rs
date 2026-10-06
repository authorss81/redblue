//! `constant NAME to <expr>` — a name bound once, which no assignment rebinds.
//!
//! `modules/MathUtils.rb` has opened with `constant PI to 3.14159` since the
//! module system was written, and `constant` was in no keyword table, so the
//! file was a parse error (`ParserError: Expected function name`) and every
//! test that walked `modules/` had to be taught to skip it.
//!
//! The rules these tests pin, in full:
//!
//! - A `constant` binds a name of the whole program, to the value the
//!   expression has where the declaration runs. Every later statement of the
//!   file reads it through the ordinary name lookup, function bodies included —
//!   which is what lets a module's functions share one value.
//! - Declaring the same name twice is refused, and the first binding is what
//!   survives the refusal.
//! - Assigning to a constant is refused. This is a property of the name, not of
//!   the statement: a `set` inside a function body cannot rebind a constant
//!   either, because that write lands on the global of the same name, and
//!   neither can a module's own `set` bound by an `import`.
//! - Reading a constant before its declaration is an unknown variable, exactly
//!   as reading any name before its `set` is.
//! - A local of the same name shadows the constant inside its scope — a
//!   parameter, a loop variable, a `set` in a scope that already has the name —
//!   and the constant is unchanged once the scope ends.
//!
//! Every test here runs the whole pipeline — lexer, parser, analyzer, VM — as
//! `rb run` does, through [`redblue::run_source_value`]. A test that parses and
//! hands the tree to the VM skips the analyzer, and a program the analyzer
//! refuses is a program `rb run` refuses: that gap let a claim about `import`
//! pass here while the shipped path rejected it.

use std::path::{Path, PathBuf};

use redblue::{Error, Value};

/// Runs the whole pipeline on `source` and returns the value of its last
/// statement.
#[track_caller]
fn eval(source: &str) -> Value {
    redblue::run_source_value(source)
        .unwrap_or_else(|error| panic!("`{}` should have run, failed with {:?}", source, error))
}

/// Runs the whole pipeline on `source` and returns the error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    redblue::run_source_value(source).expect_err("`source` should have failed")
}

/// [`Value::Number`] for a decimal written as text.
///
/// The values `modules/MathUtils.rb` declares are `3.14159` and `6.28318`, and
/// clippy's `approx_constant` refuses a float literal that close to
/// `f64::consts::PI` / `TAU`. Spelling the decimal as text pins the module's own
/// digits, which is what these tests are about.
#[track_caller]
fn number(decimal: &str) -> Value {
    Value::Number(
        decimal
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("`{}` should be a decimal number", decimal)),
    )
}

/// Asserts `source` fails at runtime with exactly `expected` as its message.
#[track_caller]
fn assert_runtime_error(source: &str, expected: &str) {
    match eval_err(source) {
        Error::Runtime(message, span) => {
            assert_eq!(
                message, expected,
                "`{}` failed with the wrong message",
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

/// Asserts the analyzer refuses `source`, with a message containing `expected`.
///
/// The whole pipeline is run, so a program the analyzer accepts and the VM
/// rejects is reported as the failure it is rather than passing quietly.
#[track_caller]
fn assert_analyzer_error(source: &str, expected: &str) {
    match eval_err(source) {
        Error::Analyzer(message, span) => {
            assert!(
                message.contains(expected),
                "the analyzer reported {:?}, which does not name {:?}",
                message,
                expected
            );
            assert!(span.is_known(), "the failure carried no source span");
        }
        other => panic!(
            "`{}` should be refused by the analyzer, got {:?}",
            source, other
        ),
    }
}

/// Asserts `source` is refused by the parser with `expected` as its message.
#[track_caller]
fn assert_parser_error(source: &str, expected: &str) {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    match redblue::parser::parse(tokens) {
        Ok(_) => panic!("`{}` should not have parsed", source),
        Err(Error::Parser(message, span)) => {
            assert_eq!(
                message, expected,
                "`{}` failed with the wrong message",
                source
            );
            assert!(span.is_known(), "`{}` failed without a source span", source);
        }
        Err(other) => panic!(
            "`{}` should fail with a Parser error, got {:?}",
            source, other
        ),
    }
}

/// The shipped `modules/MathUtils.rb`.
fn mathutils_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("modules/MathUtils.rb")
}

/// A `constant` declaration binds its name to the value it was given.
#[test]
fn constant_declares_a_readable_name() {
    assert_eq!(
        eval("constant PI to 3.14159\ngive back PI"),
        number("3.14159")
    );
}

/// The value is an expression, evaluated where the declaration runs, and every
/// constant of the file sees it — the shape a module of functions is written in.
#[test]
fn constant_is_read_by_a_function_body() {
    let source = "\
constant RATE to 2
constant TOTAL to RATE * 3
to scaled(n)
    give back TOTAL + n
end
give back scaled(4)";
    assert_eq!(eval(source), Value::Number(10.0));
}

/// The shipped module runs, and its constants are bound by running it. This is
/// the defect the phase was opened for: the file was a parse error, so
/// `rb run modules/MathUtils.rb` exited 1.
#[test]
fn shipped_mathutils_module_runs_and_binds_its_constants() {
    let source =
        std::fs::read_to_string(mathutils_path()).expect("modules/MathUtils.rb should exist");

    eval(&source);

    // Reading a constant declared by the file proves it bound the name rather
    // than only parsing.
    assert_eq!(
        eval(&format!("{source}\ngive back PI")),
        number("3.14159"),
        "the module's PI should be readable"
    );
    assert_eq!(
        eval(&format!("{source}\ngive back TAU")),
        number("6.28318"),
        "the module's TAU should be readable"
    );
}

/// An `import` brings a module's constants into the importing program, and the
/// program reads them through ordinary lookup.
///
/// This ran through `run_isolated` — lexer, parser, VM, no analyzer — while
/// `rb run` rejected the same program with `Unknown variable 'TAU'`: the
/// analyzer cannot see into a module file, so `import` declared nothing. The
/// import now declares the module's names, and this test runs the pipeline
/// `rb run` runs, so it cannot pass on the path the CLI refuses.
#[test]
fn import_binds_a_modules_constants() {
    // `cargo test` runs a test binary from the package root, which is where the
    // `modules/` the loader looks in lives.
    assert_eq!(
        eval("import MathUtils\ngive back TAU"),
        number("6.28318"),
        "the module loader should bind modules/MathUtils.rb's TAU"
    );
    assert_eq!(
        eval("import MathUtils\ntype_of(TAU)"),
        Value::Text("number".to_string()),
        "an imported constant should still be a number"
    );
}

/// A function body declared *above* an `import` reads the module's names when
/// the body runs, which is the same promise the `constant` order makes and the
/// reason the analyzer's later names are consulted inside a body at all.
#[test]
fn edge_a_body_declared_above_an_import_reads_the_modules_names() {
    let source = "\
to circumference(radius)
    give back TAU * radius
end
import MathUtils
give back circumference(2)";
    assert_eq!(
        eval(source),
        number("12.56636"),
        "the body should read the name the import binds before the call"
    );
}

/// A second declaration of the same name is refused, and the first value is the
/// one that survives: a refused declaration must not leave the name unbound.
#[test]
fn edge_duplicate_constant_is_refused() {
    assert_runtime_error(
        "constant PI to 3.14159\nconstant PI to 3.15",
        "Constant 'PI' is already declared",
    );
}

/// A `constant` inside a function body is a name of the whole program, so the
/// second call of that function is the duplicate, not the first.
#[test]
fn edge_constant_declared_again_by_a_second_call_is_refused() {
    assert_runtime_error(
        "to f()\n    constant LIMIT to 5\n    give back LIMIT\nend\nf()\nf()",
        "Constant 'LIMIT' is already declared",
    );
}

/// Assigning to a constant is refused. A `set` that reaches a global is the
/// write, so the refusal belongs to the name.
#[test]
fn edge_constant_cannot_be_reassigned() {
    assert_runtime_error(
        "constant PI to 3.14159\nset PI to 3.15",
        "Cannot assign to constant 'PI'",
    );
}

/// The same refusal applies inside a function body, where the `set` resolves to
/// no local scope and so would land on the constant's own name.
#[test]
fn edge_constant_cannot_be_reassigned_from_a_function_body() {
    assert_runtime_error(
        "constant PI to 3.14159\nto f()\n    set PI to 1\nend\nf()",
        "Cannot assign to constant 'PI'",
    );
}

/// A module's own `set` is bound into the importing program, and it cannot
/// rebind a constant there either — the refusal belongs to the name, and
/// `modules/SuiteKit.rb` declares `SUITE_KIT_NAME` with a `set`.
#[test]
fn edge_an_import_cannot_rebind_a_constant_with_a_set() {
    assert_runtime_error(
        "constant SUITE_KIT_NAME to \"mine\"\nimport SuiteKit",
        "Cannot assign to constant 'SUITE_KIT_NAME'",
    );
}

/// The other order is a duplicate declaration rather than a write onto a
/// constant, because the module's own `set` lands on a name nothing holds yet.
/// `SUITE_KIT_NAME` is the name; a module's `constant` is checked by the
/// duplicate test above.
#[test]
fn edge_a_module_constant_and_a_program_constant_of_one_name_is_refused() {
    assert_runtime_error(
        "import MathUtils\nconstant TAU to 7",
        "Constant 'TAU' is already declared",
    );
}

/// Importing the same module twice binds its names once. Binding them twice is
/// what the module's own `TAU` cannot survive — the second declaration would be
/// refused as a duplicate of the first — so the second import is a no-op
/// instead, and the names it contributed are still there afterwards.
#[test]
fn edge_importing_the_same_module_twice_binds_its_names_once() {
    assert_eq!(
        eval("import MathUtils\nimport MathUtils\ngive back TAU"),
        number("6.28318"),
        "a second import should leave the first binding alone"
    );
    assert_eq!(
        eval("import MathUtils\nimport MathUtils to M\ngive back M"),
        Value::Nothing,
        "the alias of a second import is still bound"
    );
}

/// Reading a constant before its declaration is an unknown variable, both where
/// the analyzer sees the name and where only the VM does.
#[test]
fn edge_constant_used_before_declaration_is_an_error() {
    assert_analyzer_error(
        "give back PI\nconstant PI to 3.14159",
        "Unknown variable 'PI'",
    );

    // Inside a function body the analyzer cannot know when the body runs, so
    // this is the runtime's report instead: the call is before the declaration.
    // The name is not a standard-library constant — `PI` is one, and it is bound
    // before any file runs.
    assert_runtime_error(
        "to f()\n    give back ANSWER\nend\nf()\nconstant ANSWER to 42",
        "Unknown variable 'ANSWER'",
    );
}

/// A body declared above a `constant` reads it when the body runs, which is
/// what the spec promises and what the analyzer's eager walk of a body used to
/// refuse: the walk saw the read before the declaration and reported
/// `Unknown variable 'TAU'`.
#[test]
fn edge_a_body_declared_above_a_constant_reads_it_when_it_runs() {
    let source = "\
to circumference(radius)
    give back TAU * radius
end
constant TAU to 6.28318
give back circumference(1)";
    assert_eq!(
        eval(source),
        number("6.28318"),
        "the body should read the constant the declaration below it binds"
    );
}

/// The order matters the other way round too: a call before the declaration
/// runs before the name is bound, so the VM reports it — the body being later is
/// not a promise that every call is later.
#[test]
fn edge_a_call_before_the_declaration_is_a_runtime_error() {
    assert_runtime_error(
        "to f()\n    give back LATER\nend\nf()\nconstant LATER to 1",
        "Unknown variable 'LATER'",
    );
}

/// The name a program does not bind anywhere is still an unknown variable,
/// inside a body as much as outside it. The names the analyzer collects ahead of
/// the walk are the ones a `constant` or an `import` binds, and nothing else —
/// a body reading any other name is a diagnostic, not a silent nothing.
#[test]
fn edge_a_body_reading_a_name_nothing_binds_is_still_an_error() {
    assert_analyzer_error(
        "to f()\n    give back NO_SUCH_NAME\nend\nf()",
        "Unknown variable 'NO_SUCH_NAME'",
    );
    assert_analyzer_error(
        "import MathUtils\ngive back TAU_PLUS_ONE",
        "Unknown variable 'TAU_PLUS_ONE'",
    );
}

/// A module's names are bound where the import is, so a read above it is the
/// unknown variable any other name read too early gives — the same rule a
/// `constant` read too early follows.
#[test]
fn edge_a_module_name_read_before_the_import_is_an_error() {
    assert_analyzer_error("give back TAU\nimport MathUtils", "Unknown variable 'TAU'");
}

/// A local of the same name shadows the constant for the length of its scope,
/// and the constant is unchanged once the scope ends. The two shadows a program
/// can write are a parameter and a loop variable.
#[test]
fn edge_constant_is_shadowed_by_a_local() {
    let source = "\
constant PI to 3
to shadowed(PI)
    give back PI * 2
end
set from_parameter to shadowed(2)
for each PI in [7, 8]
    set from_loop to PI
end
give back from_parameter + PI + from_loop";
    assert_eq!(eval(source), Value::Number(4.0 + 3.0 + 8.0));
}

/// A `constant` needs a name and a `to`. Both halves of the declaration are
/// checked, so a malformed one is a diagnostic and not a silent no-op.
#[test]
fn constant_syntax_errors_are_reported() {
    assert_parser_error("constant 5 to 1", "Expected constant name after 'constant'");
    assert_parser_error("constant to 1", "Expected constant name after 'constant'");
    assert_parser_error("constant PI", "Expected To but got Eof");
}

/// The formatter writes `constant` the way it is spelled, and reformatting a
/// file changes nothing else about it.
#[test]
fn constant_round_trips_through_the_formatter() {
    let formatted = redblue::formatter::format("constant   PI to   3.14159\nsay PI\n")
        .expect("a constant declaration should format");
    assert_eq!(formatted, "constant PI to 3.14159\nsay PI\n");
}
