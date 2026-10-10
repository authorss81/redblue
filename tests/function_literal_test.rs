//! A function literal — `to (x) ... end` — must be an expression, so an
//! anonymous function is a value.
//!
//! `Expr` had no literal-form variant, so `to` was only ever a *statement*:
//! `set double to to (x) give back x * 2` died with `Unexpected token To`, and
//! with it every higher-order call in SPEC.md (`list.map(xs, to (x) ...)`) was
//! unreachable. Closures worked, but only through a *named* nested
//! `to name() ... end`, which cannot be passed as an argument.
//!
//! The policy for what the literal captures is the one
//! `tests/closure_capture_test.rs` pins for a named closure: **capture by value,
//! at the point the literal is written** — a literal sees the bindings live
//! where it was written, not those of whoever calls it.

use std::path::PathBuf;
use std::process::Command;

use redblue::Error;
use redblue::Value;

/// A scratch directory inside `target/`, so tests never write outside the
/// project checkout.
fn scratch_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/function-literal");
    std::fs::create_dir_all(&dir).expect("scratch directory should be creatable");
    dir
}

/// Writes `source` to its own file and runs it in a child process, so the exit
/// code and what `say` printed are both observable.
fn run_in_child(name: &str, source: &str) -> (i32, String, String) {
    let path = scratch_dir().join(format!("{name}.rb"));
    std::fs::write(&path, source).expect("program should be writable");

    let out = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("child process should run");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// Lexes and parses `source`.
#[track_caller]
fn parse(source: &str) -> redblue::parser::Program {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    redblue::parser::parse(tokens).expect("source should parse")
}

/// Runs `source` and returns the value of its last statement.
#[track_caller]
fn eval(source: &str) -> Value {
    let (_vm, result) = redblue::run_isolated(&parse(source));
    result.unwrap_or_else(|error| panic!("source should have run, failed with {error:?}"))
}

/// Runs `source` and returns the error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    match redblue::parser::parse(tokens) {
        Ok(program) => {
            let (_vm, result) = redblue::run_isolated(&program);
            result.expect_err("source should have failed")
        }
        Err(error) => error,
    }
}

/// The message an `Error` carries, without the span it points at.
#[track_caller]
fn message(error: &Error) -> String {
    match error {
        Error::Parser(message, _)
        | Error::Runtime(message, _)
        | Error::Limit(message, _)
        | Error::Analyzer(message, _) => message.clone(),
        other => panic!("expected a diagnostic, got {other:?}"),
    }
}

/// Asserts `source` runs and its last statement reads as `expected`.
#[track_caller]
fn assert_yields(source: &str, expected: &str) {
    assert_eq!(eval(source).to_string(), expected, "\nprogram:\n{source}");
}

/// Asserts `source` fails, and that the failure names `expected`.
#[track_caller]
fn assert_fails(source: &str, expected: &str) {
    let error = eval_err(source);
    let message = message(&error);
    assert!(
        message.contains(expected),
        "the failure should name `{expected}`, it said: {message}\nprogram:\n{source}"
    );
}

/// Asserts `source` is stopped by one of the host's own guards: `Error::Limit`
/// and nothing else, with `is_resource_limit()` answering true for it. A
/// diagnostic read through the general [`assert_fails`] would also pass if the
/// call-depth guard started reporting itself as an ordinary program failure.
#[track_caller]
fn assert_hits_a_limit(source: &str, expected: &str) {
    let error = eval_err(source);
    match &error {
        Error::Limit(message, _) => assert!(
            message.contains(expected),
            "the limit should name `{expected}`, it said: {message}\nprogram:\n{source}"
        ),
        other => panic!(
            "a host guard should stop this as Error::Limit, got {other:?}\nprogram:\n{source}"
        ),
    }
    assert!(
        error.is_resource_limit(),
        "a limit must answer is_resource_limit(), got {error:?}\nprogram:\n{source}"
    );
}

// ---------------------------------------------------------------------------
// The reproduction: `set double to to (x) give back x * 2`
// ---------------------------------------------------------------------------

#[test]
fn literal_is_an_expression_and_runs() {
    assert_yields(
        "
        set double to to (x)
            give back x * 2
        end
        give back double(21)
        ",
        "42",
    );
}

/// The DoD shape, end to end through the binary: the literal is a value, and
/// calling that value prints what the body computed.
#[test]
fn literal_runs_through_rb_run() {
    let (code, stdout, stderr) = run_in_child(
        "literal_prints_42",
        "set double to to (x) give back x * 2\nsay double(21)\n",
    );
    assert_eq!(
        code, 0,
        "`set double to to (x) ... end` should run, stderr was:\n{stderr}"
    );
    assert_eq!(stdout, "42\n", "stdout was:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Capture: the policy tests/closure_capture_test.rs pins, for a literal
// ---------------------------------------------------------------------------

/// Capture by value, at the point the literal is written.
///
/// Both literals are written while `bias` is 1 except the third, which is
/// written after `bias` has been rebound to 100. The first two must carry the
/// value they were written next to and the third the later one, so a literal
/// that read the variable at call time would give a different answer.
#[test]
fn literal_captures_the_bindings_live_where_it_was_written() {
    // `bias` is a *parameter*, so it is a local of `make` and is what a literal
    // written beside it captures. `before_block` and `before_line` are written
    // while it is 10 and carry 10; `after` is written after the rebinding and
    // carries 100. A literal that read the variable at call time would give
    // 303 rather than 123.
    assert_yields(
        "
        to make(bias)
            set before_block to to (x)
                give back x + bias
            end
            set before_line to to (x) give back x + bias
            set bias to 100
            set after to to (x) give back x + bias
            give back before_block(1) + before_line(1) + after(1)
        end
        give back make(10)
        ",
        "123",
    );
}

/// A name at the top level of a program is a *global*, and globals are read at
/// call time rather than captured — so a literal written at the top level sees
/// the current value of the name, not the one live when it was written. This
/// is the policy `tests/closure_capture_test.rs` states, and a literal holds to
/// it as a nested declaration does.
#[test]
fn edge_a_literal_at_the_top_level_reads_globals_at_call_time() {
    assert_yields(
        "
        set bias to 1
        set apply to to (x) give back x + bias
        set bias to 100
        give back apply(1)
        ",
        "101",
    );
}

/// The other half of the policy: a literal written inside a function sees that
/// function's locals, not a same-named binding of whoever called it.
///
/// The caller has `x` live with another value, and the literal still reads the
/// `x` of `make_adder` — the escaped closure's own environment, not the frame
/// it is called from.
#[test]
fn literal_inside_a_function_reads_that_functions_locals_not_the_callers() {
    assert_yields(
        "
        to make_adder(x)
            set add_bias to to (y)
                give back x + y
            end
            give back add_bias
        end
        set add_ten to make_adder(10)
        to caller()
            set x to 1000
            give back add_ten(5)
        end
        give back caller()
        ",
        "15",
    );
}

/// A literal and a named nested declaration of the same body are the same
/// function: both capture where they were written, and both read their own
/// environment rather than the caller's. This is the equivalence that lets the
/// literal form stand in for the declaration form everywhere.
#[test]
fn a_literal_behaves_exactly_as_a_nested_declaration() {
    let declared = eval(
        "
        to make(x)
            to inner(y)
                give back x + y
            end
            give back inner
        end
        set add_ten to make(10)
        to caller()
            set x to 1000
            give back add_ten(5)
        end
        give back caller()
        ",
    );
    let literal = eval(
        "
        to make(x)
            set inner to to (y)
                give back x + y
            end
            give back inner
        end
        set add_ten to make(10)
        to caller()
            set x to 1000
            give back add_ten(5)
        end
        give back caller()
        ",
    );

    assert_eq!(literal.to_string(), "15", "the literal should add 10 and 5");
    assert_eq!(
        literal.to_string(),
        declared.to_string(),
        "a literal should behave exactly as a nested declaration does"
    );
}

// ---------------------------------------------------------------------------
// Edge cases: shape
// ---------------------------------------------------------------------------

/// No parameters at all — written both ways the grammar allows, `to ()` and a
/// bare `to`, and each called through its own name.
#[test]
fn edge_a_literal_needs_no_parameters() {
    assert_yields(
        "
        set answer to to ()
            give back 42
        end
        set bare to to give back 7
        give back answer() + bare()
        ",
        "49",
    );
}

/// An empty body is still a function: it returns `nothing` rather than failing
/// to parse, which is what `length(nothing)` then reports.
#[test]
fn edge_a_literal_with_an_empty_body_returns_nothing() {
    assert_yields(
        "
        set quiet to to ()
        end
        give back quiet()
        ",
        "nothing",
    );
}

/// A parameter shadows an enclosing binding of the same name: inside the
/// literal, `x` is the argument, and the outer `x` is only reachable by the
/// name the enclosing scope really has.
#[test]
fn edge_a_parameter_shadows_an_enclosing_binding() {
    assert_yields(
        "
        set x to 100
        set shadow to to (x)
            give back x
        end
        give back shadow(7) + x
        ",
        "107",
    );
}

/// Three literals, each capturing its own level. The innermost has to see 1, 2
/// and 3 at its own three levels rather than the caller's rebinding of them.
#[test]
fn edge_three_nested_literals_each_capture_their_own_level() {
    // Four levels, each one a parameter of the level above, so each literal
    // captures the level it was written at. `level1` is rebound to 100 after
    // `first` was written, and `first` must still carry the 1 it was written
    // beside: the sum is 10, and a literal that read anything else would give
    // 109 or 112.
    assert_yields(
        "
        to make(level1)
            set first to to (level2)
                set second to to (level3)
                    set deepest to to (level4)
                        give back level1 + level2 + level3 + level4
                    end
                    give back deepest(4)
                end
                give back second(3)
            end
            set level1 to 100
            give back first(2)
        end
        give back make(1)
        ",
        "10",
    );
}

/// A literal inside a list or a record is an ordinary element, and each one is
/// a different function rather than a second copy of the first.
#[test]
fn edge_literals_sit_inside_a_list_and_a_record() {
    assert_yields(
        r#"
        set pair to [to (n) give back n + 1, to (n) give back n * 100]
        set plus_one to pair[0]
        set times_hundred to pair[1]
        set table to {first: plus_one(1), second: times_hundred(2)}
        give back table.first + table.second
        "#,
        "202",
    );
}

/// The captured scope belongs to the literal, so a value it assigns stays in
/// the call: `total` is a parameter of `run`, so each call starts again from
/// the captured copy and `run`'s own `total` is still 0 at the end.
#[test]
fn edge_a_literal_rebinding_its_capture_leaves_the_enclosing_scope_alone() {
    assert_yields(
        "
        to run(total)
            set bump to to (n)
                set total to total + n
                give back total
            end
            set first to bump(5)
            set second to bump(5)
            give back first + second + total
        end
        give back run(0)
        ",
        "10",
    );
}

// ---------------------------------------------------------------------------
// Edge cases: a literal has to be given the right shape to run
// ---------------------------------------------------------------------------

/// Too few arguments. `map` refuses the call before anything runs, so the
/// failure names the call rather than the body's arithmetic on `nothing`.
#[test]
fn edge_calling_map_without_a_function_fails() {
    assert_fails(
        "
        give back map([1, 2, 3])
        ",
        "map requires a list and a function",
    );
}

/// The same, through the method spelling, with no argument at all.
#[test]
fn edge_calling_map_on_a_list_without_a_function_fails() {
    assert_fails(
        "
        give back [1, 2, 3].map()
        ",
        "map requires a function",
    );
}

/// Too few arguments for the literal itself.
///
/// A missing parameter binds `nothing`, as it does for a named function — the
/// corpus pins that in `corpus/functions-0015.rb` — so the failure arrives from
/// the body, on the argument that was not given. The message names the operand
/// rather than the arity, which is the honest limit of the current policy.
#[test]
fn edge_calling_a_literal_with_too_few_arguments_fails() {
    assert_fails(
        "
        set add to to (x, y)
            give back x + y
        end
        give back add(1)
        ",
        "Cannot add",
    );
}

/// The type a literal's parameter is handed is not checked for it: a text where
/// a number was expected fails in the body, naming the operand.
#[test]
fn edge_calling_a_literal_with_the_wrong_type_fails() {
    assert_fails(
        r#"
        set double to to (x)
            give back x * 2
        end
        give back double("text")
        "#,
        "Cannot multiply",
    );
}

/// A block whose `end` is missing is a diagnostic that names the `end`, and it
/// carries the position it was wanted at. It must never be a panic and never a
/// file silently swallowed to the end.
#[test]
fn edge_an_unterminated_literal_names_the_missing_end() {
    let source = "
        set f to to (x)
            give back x * 2
        ";
    let error = eval_err(source);

    match &error {
        Error::Parser(message, span) => {
            assert!(
                message.contains("Expected 'end' to close the function literal"),
                "the diagnostic should name the missing `end`, it said: {message}"
            );
            assert!(
                span.line > 0,
                "the diagnostic should point at where the `end` was wanted, it pointed at \
                 line {}",
                span.line
            );
        }
        other => panic!("expected a ParserError, got {other:?}"),
    }
}

/// The one-line form closes on its own line, so a body that *runs on* without
/// its `end` is still unterminated and is still reported.
#[test]
fn edge_a_multiline_body_without_its_end_fails_rather_than_eating_the_file() {
    assert_fails(
        "
        set f to to (x)
            give back x * 2
        say 1
        ",
        "Expected 'end' to close the function literal",
    );
}

/// A literal nested inside itself — calling itself by the name it was given.
///
/// Nothing stops the name being bound, and the call-depth counter is what ends
/// it: the refusal names the limit that was reached rather than recursing until
/// the process dies.
#[test]
fn edge_a_literal_nested_inside_itself_stops_at_the_call_depth_limit() {
    assert_hits_a_limit(
        "
        set loop to to ()
            give back loop()
        end
        give back loop()
        ",
        "Maximum call depth",
    );
}

/// Literals nest, so their bodies nest, and the parser's block budget has to
/// hold: a file of them is a diagnostic, not a stack overflow.
///
/// It runs in a child process because the failure being asserted is that the
/// parser *recurses* that deep before it reports — 65 levels of a block body
/// each cost a parser frame, and this test's own thread is smaller than the one
/// `rb` parses on. The child is the interpreter, so what is measured is what
/// `rb run` does with such a file.
#[test]
fn edge_nested_literals_past_the_block_budget_are_reported() {
    let depth = redblue::parser::MAX_BLOCK_DEPTH + 1;
    let source = format!(
        "{}give back 1{}",
        "set f to to ()".repeat(depth),
        " end".repeat(depth)
    );
    let (code, _stdout, stderr) = run_in_child("nested_literals_past_budget", &source);

    assert_ne!(
        code, 0,
        "{depth} nested literals are past the block budget and must be refused"
    );
    assert!(
        stderr.contains("nest") && stderr.contains("64"),
        "the refusal should name the block budget it spent, stderr was:\n{stderr}"
    );
    assert!(
        !stderr.contains("overflowed its stack"),
        "a file of nested literals must be a diagnostic, not an abort:\n{stderr}"
    );
}

/// At the budget, nested literals are an ordinary program: the deepest one the
/// parser accepts parses and runs.
#[test]
fn edge_nested_literals_within_the_block_budget_run() {
    let depth = 4;
    let source = format!(
        "{}give back 1{}\ngive back 7\n",
        "set f to to ()".repeat(depth),
        " end".repeat(depth)
    );
    assert_yields(&source, "7");
}

// ---------------------------------------------------------------------------
// A literal is a value, so it can be passed to something
// ---------------------------------------------------------------------------

/// `SPEC.md` § First-Class Functions: a literal handed to `map` maps the list.
#[test]
fn a_literal_can_be_passed_to_map() {
    assert_yields(
        "
        give back [1, 2, 3].map(to (x) give back x * 2)
        ",
        "[2, 4, 6]",
    );
}

/// The same call in the two other spellings SPEC.md uses, so the mapped list is
/// the same whichever one a program reaches for.
#[test]
fn a_literal_maps_in_every_spelling() {
    assert_yields(
        "
        set double to to (x) give back x * 2
        set by_name to map([1, 2, 3], double)
        set by_module to list.map([1, 2, 3], to (x) give back x * 2)
        set by_method to [1, 2, 3].map(double)
        give back length(by_name) + length(by_module) + length(by_method)
        ",
        "9",
    );
}

/// `map` calls the literal once per element, in order, and reads the value of
/// the last statement of each call — so a body that only assigns returns
/// `nothing` for that element, which is what the policy says a body returns.
#[test]
fn edge_map_over_an_empty_list_is_an_empty_list() {
    assert_yields(
        "
        give back map([], to (x) give back x * 2)
        ",
        "[]",
    );
}

/// A one-element list maps to a one-element list.
#[test]
fn edge_map_over_a_singleton_is_a_singleton() {
    assert_yields(
        "
        give back [21].map(to (x) give back x * 2)
        ",
        "[42]",
    );
}

/// The element handed to a literal is the element, not a copy of the list, and
/// the text it receives is the text itself — empty text included.
#[test]
fn edge_map_hands_each_element_to_the_literal() {
    assert_yields(
        r#"
        give back map([""], to (s) give back length(s))
        "#,
        "[0]",
    );
    assert_yields(
        r#"
        give back map(["héllo ☃ 日本語 🎉"], to (s) give back s + "🎉")
        "#,
        "[héllo ☃ 日本語 🎉🎉]",
    );
}

/// A literal is a value like any other, so it can be stored and read back — and
/// reading it back gives a function that still runs.
#[test]
fn a_literal_can_be_stored_in_a_record() {
    assert_yields(
        "
        set tools to {double: to (x) give back x * 2}
        set double to tools.double
        give back double(50)
        ",
        "100",
    );
}

// ---------------------------------------------------------------------------
// The tools have to agree with the parser
// ---------------------------------------------------------------------------

/// `rbfmt` writes a literal as a block, and writing it again changes nothing —
/// a formatter that does not settle would rewrite the file on every run.
#[test]
fn the_formatter_writes_a_literal_as_a_block_and_settles() {
    let source = "set double to to (x) give back x * 2\nsay map([1, 2], double)\n";
    let formatted = redblue::formatter::format(source).expect("the formatter should accept it");

    assert_eq!(
        formatted, "set double to to (x)\n    give back x * 2\nend\nsay map([1, 2], double)\n",
        "the formatter should write the block form"
    );

    let again = redblue::formatter::format(&formatted).expect("the output should format again");
    assert_eq!(
        again, formatted,
        "formatting the output should change nothing"
    );

    // And what it wrote is still the program that was written: it runs, and
    // the statement that printed is still the last one.
    assert_yields(&formatted, "nothing");
}

/// The linter reads a literal's body as the body it is: a parameter it uses is
/// not an unused variable, and the body is not a stray name at the top level.
#[test]
fn the_linter_does_not_report_a_literal_as_unused() {
    let (errors, _warnings) =
        redblue::linter::lint("set double to to (x) give back x * 2\nsay double(21)\n");
    assert!(
        errors.is_empty(),
        "a literal that is used should report nothing, it reported {errors:?}"
    );
}
