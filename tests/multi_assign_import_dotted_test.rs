//! `set x, y to 1, 2` and `import a.b as c` — the two productions
//! `docs/GRAMMAR.md` § 4.1 and § 3.1 write that no program could contain.
//!
//! Both were refused with a `ParserError`: `set x, y to 1, 2` was
//! `Expected 'to' but got Comma` and `import math.sqrt as s` was
//! `Unexpected token Dot`, while the grammar described them in the same
//! sections that carry a Not Implemented note for what is genuinely absent.
//! These tests pin what they do now, and what they refuse.
//!
//! - `set a, b to <expr>, <expr>` binds every name, left to right, and a name
//!   count that is not the value count is a clean `ParserError` naming both.
//! - `import module.member as alias` binds `alias` to *that member*, so a
//!   call reaches it without the namespace: `import math.sqrt as s` then
//!   `s(9)`. The `as` is required — the grammar writes it — and the plain
//!   `import Mod` / `import Mod as m` forms are untouched.
//!
//! Every test runs the whole pipeline — lexer, parser, analyzer, VM — as
//! `rb run` does, through [`redblue::run_source_value`].

use redblue::Error;

/// Runs the whole pipeline on `source` and returns its last value.
#[track_caller]
fn eval(source: &str) -> redblue::Value {
    redblue::run_source_value(source)
        .unwrap_or_else(|error| panic!("`{}` should have run, failed with {:?}", source, error))
}

/// Runs the whole pipeline on `source` and returns the error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    redblue::run_source_value(source).expect_err("`source` should have failed")
}

/// [`redblue::Value`] is a number written as decimal text.
#[track_caller]
fn number(decimal: &str) -> redblue::Value {
    redblue::Value::Number(
        decimal
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("`{}` should be a decimal number", decimal)),
    )
}

/// Asserts `source` fails at parse time, and that the message says `needle`.
#[track_caller]
fn assert_parser_error(source: &str, needle: &str) {
    match eval_err(source) {
        Error::Parser(message, span) => {
            assert!(
                message.contains(needle),
                "`{}` failed with {:?}, which does not name {:?}",
                source,
                message,
                needle
            );
            assert!(span.is_known(), "`{}` failed without a source span", source);
        }
        other => panic!(
            "`{}` should fail with a Parser error, got {:?}",
            source, other
        ),
    }
}

/// Asserts `source` fails at analysis time, and that the message says
/// `needle`.
#[track_caller]
fn assert_analyzer_error(source: &str, needle: &str) {
    match eval_err(source) {
        Error::Analyzer(message, span) => {
            assert!(
                message.contains(needle),
                "`{}` failed with {:?}, which does not name {:?}",
                source,
                message,
                needle
            );
            assert!(span.is_known(), "`{}` failed without a source span", source);
        }
        other => panic!(
            "`{}` should fail with an Analyzer error, got {:?}",
            source, other
        ),
    }
}

/// Asserts `source` fails at run time, and that the message says `needle`.
#[track_caller]
fn assert_runtime_error(source: &str, needle: &str) {
    match eval_err(source) {
        Error::Runtime(message, span) => {
            assert!(
                message.contains(needle),
                "`{}` failed with {:?}, which does not name {:?}",
                source,
                message,
                needle
            );
            assert!(span.is_known(), "`{}` failed without a source span", source);
        }
        other => panic!(
            "`{}` should fail with a Runtime error, got {:?}",
            source, other
        ),
    }
}

/// The headline claim of § 4.1: `set x, y to 1, 2` binds both names.
#[test]
fn multiple_assignment_binds_every_name() {
    assert_eq!(
        eval("set x, y to 1, 2\nx + y"),
        number("3"),
        "`set x, y to 1, 2` should bind both names"
    );
}

/// The singleton of the multiple form: `set a, b to 1, 1` and a list, which
/// is the shape a program is likeliest to reach for.
#[test]
fn multiple_assignment_takes_any_expressions() {
    assert_eq!(
        eval("set a, b to 1 + 1, 2 * 3\na * b"),
        number("12"),
        "each value should be its own expression"
    );
    assert_eq!(
        eval("set first, rest to [9, 8][0], [1, 2]\nfirst"),
        number("9"),
        "an indexed value should bind like any other"
    );
}

/// Two and three names. The bindings are made left to right, each value
/// evaluated and bound before the next is evaluated — the order consecutive
/// `set` statements would have made — so `set y, x to 3, 4` after
/// `set x, y to 1, 2` leaves `y` at 3 and `x` at 4, not the tuple reading
/// where both would be written before either was read.
#[test]
fn edge_multiple_assignment_of_two_and_three_names() {
    assert_eq!(
        eval("set x, y to 1, 2\nset y, x to 3, 4\nx"),
        number("4"),
        "a later multiple assignment should rebind both names in order"
    );
    assert_eq!(
        eval("set x, y to 1, 2\nset y, x to 3, 4\ny"),
        number("3"),
        "the first name of the second statement should be bound first"
    );
    assert_eq!(
        eval("set a, b, c to 1, 2, 3\na + b + c"),
        number("6"),
        "three names and three values should bind"
    );
    // A value cannot read a name its own statement binds: the analyzer walks
    // the values before the names are declared, exactly as it does for a
    // single `set`, so `set x to x + 1` and `set x, y to 1, x + 1` are refused
    // the same way.
    assert_analyzer_error("set a, b to 1, a + 10", "Unknown variable 'a'");
}

/// A trailing comma on the left is not the grammar's `{ ',' identifier }`,
/// and it is refused naming the token the parser wanted instead.
#[test]
fn edge_multiple_assignment_with_a_trailing_comma_is_refused() {
    assert_parser_error("set x, y, to 1, 2", "to");
}

/// A value count that is not the name count, in both directions: the missing
/// right-hand side of the phase's own edge list, and one value too many.
#[test]
fn edge_multiple_assignment_arity_mismatch_is_refused() {
    assert_parser_error("set x, y to 1", "2");
    assert_parser_error("set x, y to 1, 2, 3", "2");
}

/// The empty form: `set to 1` is not a declaration at all, and a multiple
/// assignment whose right-hand side is empty is refused at the newline rather
/// than binding two names to nothing.
#[test]
fn edge_multiple_assignment_with_no_names_is_refused() {
    assert_parser_error("set to 1", "variable name");
    assert_parser_error("set x, y to", "Unexpected token");
}

/// `set a.b, c to 1, 2` — the property target of § 4.3, which a multiple
/// assignment does not reach. It names the spelling rather than reporting
/// `Expected 'to' but got Comma` at a position that is not the problem.
#[test]
fn edge_multiple_assignment_refuses_a_property_target() {
    assert_parser_error("set a.b, c to 1, 2", "property");
}

/// § 3.1's dotted import, binding one member of a builtin namespace under a
/// name of its own: `import math.sqrt as s` then `s(9)`.
#[test]
fn dotted_import_binds_the_named_member() {
    assert_eq!(
        eval("import math.sqrt as s\ns(9)"),
        number("3"),
        "`import math.sqrt as s` should bind `s` to `sqrt`"
    );
}

/// The same dotted spelling against a module file, whose members are
/// published by the loader rather than by the builtin registry.
#[test]
fn dotted_import_reaches_a_module_files_member() {
    assert_eq!(
        eval("import MathUtils.degrees_to_radians as deg\ndeg(180)"),
        number("3.14159"),
        "a dotted import should reach a member a module file declares"
    );
}

/// The forms that already worked, pinned so this phase cannot have taken them
/// with it: `import Mod` and `import a as b`.
#[test]
fn plain_imports_still_work() {
    assert_eq!(
        eval("import json\njson.stringify(1)"),
        redblue::Value::Text("1".to_string()),
        "`import Mod` should still bind the module's own name"
    );
    assert_eq!(
        eval("import json as J\nJ.stringify(1)"),
        redblue::Value::Text("1".to_string()),
        "`import Mod as m` should still bind the alias"
    );
    assert_eq!(
        eval("import MathUtils as M\nM.degrees_to_radians(180)"),
        number("3.14159"),
        "an alias should still name a module file's function"
    );
}

/// The module of a dotted import is still the module: a name the program
/// already holds is the program's, and a dotted import does not take it over.
#[test]
fn edge_dotted_import_does_not_take_over_a_bound_name() {
    assert_eq!(
        eval("import math.floor as f\nset f to 5\nf"),
        number("5"),
        "a program's own binding should win over the imported member"
    );
}

/// A member that does not exist is refused by name, on both engines' terms,
/// and the alias is left unbound rather than bound to `nothing` silently.
#[test]
fn edge_dotted_import_of_a_missing_member_is_refused() {
    assert_runtime_error("import math.no_such_function as q", "no_such_function");
    assert_runtime_error("import MathUtils.no_such_member as q", "no_such_member");
}

/// The module itself not existing is the ordinary `Cannot find module`, and
/// the dotted spelling does not change the message.
#[test]
fn edge_dotted_import_of_a_missing_module_is_refused() {
    assert_runtime_error("import no_such_module.member as m", "no_such_module");
}

/// The grammar writes `'import' identifier '.' identifier 'as' identifier`, so
/// the `as` is required. A dotted import without one is refused naming it,
/// rather than binding the member under its own name behind the reader's back.
#[test]
fn edge_dotted_import_without_as_is_refused() {
    assert_parser_error("import math.sqrt", "as");
}

/// A dotted import is a single item; a comma continues the list, and the list
/// after it is still an import.
#[test]
fn edge_dotted_import_joins_a_comma_list() {
    assert_eq!(
        eval("import math.sqrt as s, json as J\ns(4) + 0"),
        number("2"),
        "a dotted import should be one item of an import list"
    );
}

/// Both engines run the new spellings. `rb run` is the tree-walking VM; the
/// bytecode VM is what `rb compile`/`rb vm` run, and the two answering
/// differently about the same program is a divergence the corpus runner hunts.
#[test]
fn edge_both_engines_agree_on_the_new_spellings() {
    let program = "set x, y to 1, 2\nimport math.sqrt as s\nx + y + s(16)";
    let walked = eval(program);
    let chunk = redblue::bytecode::compile_source(program).unwrap_or_else(|error| {
        panic!(
            "the bytecode compiler should accept `{}`: {:?}",
            program, error
        )
    });
    let compiled = redblue::bytecode::vm::run(&chunk)
        .unwrap_or_else(|error| panic!("the bytecode VM should run `{}`: {:?}", program, error));
    assert_eq!(
        walked, compiled,
        "the two engines disagreed about `{}`",
        program
    );
    assert_eq!(
        walked,
        number("7"),
        "`{}` should be 7 on either engine",
        program
    );
}
/// The formatter has to keep both spellings, and a format that lost the
/// member would be a silent change of program: `import math.sqrt as s` written
/// back as `import math to s` parses, and binds a namespace instead of the
/// member. Formatting is therefore run, then the result is run, and the two
/// answers have to agree.
#[test]
fn edge_the_formatter_keeps_both_new_spellings() {
    let source = "set x,y to 1,2\nimport math.sqrt as s\nx + y + s(4)\n";
    let formatted = redblue::formatter::format(source)
        .unwrap_or_else(|error| panic!("the formatter should accept `{}`: {}", source, error));
    assert!(
        formatted.contains("set x, y to 1, 2"),
        "a multiple assignment should be written back in full: `{}`",
        formatted
    );
    assert!(
        formatted.contains("import math.sqrt to s"),
        "a dotted import should keep the member it names: `{}`",
        formatted
    );
    // Twice through: a format that is not a fixed point is not a format.
    let again = redblue::formatter::format(&formatted)
        .unwrap_or_else(|error| panic!("the formatter should accept `{}`: {}", formatted, error));
    assert_eq!(formatted, again, "formatting should be a fixed point");

    // Both engines run what the formatter wrote, and say the same thing.
    let walked = redblue::run_source_value(&formatted)
        .expect("the formatted program should run on the tree-walking VM");
    let chunk = redblue::bytecode::compile_source(&formatted)
        .expect("the formatted program should compile");
    let compiled = redblue::bytecode::vm::run(&chunk)
        .expect("the formatted program should run on the bytecode VM");
    assert_eq!(
        walked, compiled,
        "the two engines disagreed after formatting"
    );
    assert_eq!(walked, number("5"), "1 + 2 + sqrt(4) is 5");
}

/// The linter sees a multiple assignment as as many bindings as it names: a
/// name bound and never read is an unused variable, and every one of them is
/// reported by name rather than the statement being skipped as a whole.
#[test]
fn edge_the_linter_sees_every_name_of_a_multiple_assignment() {
    let (errors, warnings) = redblue::linter::lint("set used, never_read to 1, 2\nsay used");
    assert!(
        errors.is_empty(),
        "a used variable is not a lint error: {:?}",
        errors
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.message.contains("never_read")),
        "the unread name of a multiple assignment should be reported: {:?}",
        warnings
    );
    assert_eq!(
        warnings.len(),
        1,
        "exactly the unread name should be reported, not the read one: {:?}",
        warnings
    );
}

/// Many names, and the boundary between a long list and a wrong one: 64 names
/// and 64 values bind, and 64 names with 63 values is refused naming the
/// counts rather than binding all but the last.
#[test]
fn edge_multiple_assignment_of_many_names() {
    let count = 64;
    let names: Vec<String> = (0..count).map(|index| format!("n{index}")).collect();
    let values: Vec<String> = (1..=count).map(|value| value.to_string()).collect();
    let program = format!(
        "set {} to {}\n{}",
        names.join(", "),
        values.join(", "),
        names[count - 1]
    );
    assert_eq!(
        eval(&program),
        number(&count.to_string()),
        "a long multiple assignment should bind every name"
    );

    let short = format!(
        "set {} to {}",
        names.join(", "),
        values[..count - 1].join(", ")
    );
    assert_parser_error(&short, "64 names");
    assert_parser_error(&short, "63 values");
}

/// Names are not ASCII-only: a multiple assignment of two non-ASCII names is
/// two names, and the count between the two lists is still what decides
/// whether it parses.
#[test]
fn edge_multiple_assignment_of_unicode_names() {
    assert_eq!(
        eval("set \u{3b1}, \u{3b2} to 1, 2\n\u{3b1} + \u{3b2}"),
        number("3"),
        "non-ASCII names should bind like any other"
    );
    assert_parser_error("set \u{3b1}, \u{3b2} to 1", "2 names");
}

/// A value of any type binds: the language has one `Value` enum and a multiple
/// assignment adds no conversion of its own, so a number and a text sit side by
/// side without either being asked to be the other.
#[test]
fn edge_multiple_assignment_binds_mixed_types() {
    assert_eq!(
        eval("set n, t, l to 7, \"seven\", [1, 2]\n[t, n, l[0]]"),
        redblue::Value::List(std::sync::Arc::new(vec![
            redblue::Value::Text("seven".to_string()),
            number("7"),
            number("1"),
        ])),
        "a multiple assignment should bind a number, a text and a list side by side"
    );
    assert_eq!(
        eval("set a, b to nothing, nothing\na"),
        redblue::Value::Nothing,
        "`nothing` is a value like any other"
    );
}

/// Malformed input at the edges of the new forms: a CRLF line ending, and an
/// empty file — neither is a parse error introduced by this phase, and both
/// are the kind of thing that a `Comma` arm could quietly break.
#[test]
fn edge_crlf_and_empty_input_are_unchanged() {
    assert_eq!(
        eval("set x, y to 1, 2\r\nx + y"),
        number("3"),
        "a CRLF line ending should not become part of the value"
    );
    assert_eq!(
        eval(""),
        redblue::Value::Nothing,
        "an empty program is still an empty program"
    );
    assert_parser_error("import math.sqrt as", "alias name");
    assert_parser_error("import math. as s", "member name");
    assert_parser_error("import as s", "module name");
}
