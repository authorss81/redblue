//! `{expr}` substitution in text literals (phase-047).
//!
//! `Expr::InterpolatedText` was declared by the AST and handled by the
//! analyzer, both VMs, the formatter and the linter, but nothing constructed
//! it: the text arm of `parse_primary` returned `Expr::Text` with the braces
//! intact, so `say "Hello, {name}!"` printed the *name*. SPEC.md documents the
//! form as working and AGENTS.md § 2 lists it as a language invariant.
//!
//! Every assertion about what a program *prints* runs on **both** engines,
//! because "it works" means the tree-walking VM and the bytecode VM substitute
//! identically or not at all — so those go through `both_engines_print`. The
//! rest assert something one engine cannot show: the shape of the node the
//! parser built, the linter's use analysis, and the budget and brace-scan the
//! parser holds itself to, which are `src/parser.rs`'s own unit tests rather
//! than this file's.

use redblue::parser::{Expr, Statement};
use redblue::{analyzer, compile_source, lexer::Lexer, parser};

/// The expression a one-statement program holds, or a panic that says what it
/// holds instead.
fn expression_of(source: &str) -> Expr {
    let ast = parser::parse(Lexer::tokenize(source).expect("the source lexes"))
        .expect("the source parses");
    match &ast.statements[0].statement {
        Statement::Expr(expr) => expr.clone(),
        other => panic!("{source:?} is not an expression statement: {other:?}"),
    }
}

/// Runs `source` through the tree-walking pipeline, the way `rb run` does.
fn tree_walk(source: &str) -> Result<(Vec<String>, redblue::Value), redblue::Error> {
    let tokens = Lexer::tokenize(source)?;
    let ast = parser::parse(tokens)?;
    analyzer::analyze(&ast)?;
    let (mut vm, result) = redblue::run_isolated(&ast);
    let output = vm.take_output();
    result.map(|value| (output, value))
}

/// Runs `source` through the bytecode compiler and then the bytecode VM, the
/// way `rb compile` then `rb vm` does.
fn bytecode(source: &str) -> Result<(Vec<String>, redblue::Value), redblue::Error> {
    let chunk = compile_source(source)?;
    let mut vm = redblue::bytecode::vm::BytecodeVm::new();
    vm.set_echo(false);
    let result = vm.run(&chunk);
    let output = vm.take_output();
    result.map(|value| (output, value))
}

/// The lines a program printed, with a message naming the program if it failed.
fn printed(source: &str) -> Vec<String> {
    tree_walk(source)
        .map(|(lines, _)| lines)
        .unwrap_or_else(|error| panic!("{source:?} must run: {error:?}"))
}

fn printed_bytecode(source: &str) -> Vec<String> {
    bytecode(source)
        .map(|(lines, _)| lines)
        .unwrap_or_else(|error| panic!("{source:?} must run: {error:?}"))
}

/// Asserts that both engines printed exactly `expected`.
fn both_engines_print(source: &str, expected: &[&str]) {
    let want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
    assert_eq!(printed(source), want, "tree-walking VM: {source}");
    assert_eq!(printed_bytecode(source), want, "bytecode VM: {source}");
}

/// The error a program produced on the tree-walking pipeline, or a panic that
/// names what it produced instead.
fn tree_failure(source: &str) -> redblue::Error {
    tree_walk(source).expect_err(&format!("{source:?} must fail"))
}

/// The same error on the bytecode VM.
fn bytecode_failure(source: &str) -> redblue::Error {
    bytecode(source)
        .err()
        .unwrap_or_else(|| panic!("{source:?} must fail"))
}

#[test]
fn interpolation_substitutes_a_bound_name_on_both_engines() {
    let source = "set name to \"World\"\nsay \"Hello, {name}!\"\n";
    both_engines_print(source, &["Hello, World!"]);
}

#[test]
fn interpolation_evaluates_an_arbitrary_expression() {
    // SPEC.md:297 — `set message to "Value: {x + y}"`. The braces hold an
    // *expression*, not a bare name, so arithmetic inside them is evaluated.
    let source = "set x to 2\nset y to 3\nsay \"Value: {x + y}\"\n";
    both_engines_print(source, &["Value: 5"]);
}

// --- a literal with no substitution is unchanged --------------------------------

#[test]
fn edge_a_literal_without_braces_is_the_text_it_was_written_as() {
    let source = "say \"plain\"\nsay \"a, b; c: 1/2\"\n";
    both_engines_print(source, &["plain", "a, b; c: 1/2"]);
}

#[test]
fn edge_an_empty_literal_is_still_empty() {
    both_engines_print("say \"\"\n", &[""]);
}

#[test]
fn edge_a_lone_open_brace_round_trips() {
    // No `}` closes it, so there is nothing for the braces to be markup around.
    let source = "set x to 1\nsay \"{\" + to_text(x) + \"\"\nsay \"a{x\"\n";
    both_engines_print(source, &["{1", "a{x"]);
}

#[test]
fn edge_a_lone_close_brace_round_trips() {
    let source = "say \"}\"\nsay \"a}b\"\nsay \"}}\"\n";
    both_engines_print(source, &["}", "a}b", "}}"]);
}

#[test]
fn edge_an_empty_brace_pair_is_not_a_substitution() {
    // `{}` holds no expression, so it is not well formed markup.
    let source = "say \"{}\"\nsay \"a{}b\"\n";
    both_engines_print(source, &["{}", "a{}b"]);
}

#[test]
fn edge_a_pair_holding_something_that_is_not_an_expression_survives() {
    // The contents have to read as one *whole* expression. A JSON document is
    // the case that matters: `json.parse` is handed it on line 2 and it is
    // still a string on line 3.
    let source = "set doc to \"{\\\"a\\\": 1}\"\nsay doc\nsay json.parse(doc).a\n";
    both_engines_print(source, &["{\"a\": 1}", "1"]);
}

#[test]
fn edge_a_pair_holding_a_prefix_of_a_larger_expression_survives() {
    // `{1 + }` is not an expression: only `1 +` parses, and the `}` never
    // becomes part of it.
    let source = "say \"a{1 + }b\"\nsay \"c{x y}d\"\n";
    both_engines_print(source, &["a{1 + }b", "c{x y}d"]);
}

// --- the parts -----------------------------------------------------------------

#[test]
fn edge_two_substitutions_keep_their_order_and_the_text_between_them() {
    let source = "set a to \"one\"\nset b to 2\nsay \"{a} then {b} then {a}\"\n";
    both_engines_print(source, &["one then 2 then one"]);
}

#[test]
fn edge_a_substitution_at_the_start_and_at_the_end_of_a_literal() {
    let source = "set x to 9\nsay \"{x} middle {x}\"\n";
    both_engines_print(source, &["9 middle 9"]);
}

#[test]
fn edge_nested_braces_are_one_substitution_holding_a_record() {
    // The record's own braces are counted, so the outer pair closes where the
    // record ends rather than at the record's first `}`.
    let source = "set n to 3\nsay \"r: {{n: n}}\"\n";
    both_engines_print(source, &["r: {n: 3}"]);
}

#[test]
fn edge_a_substitution_inside_a_record_value_and_a_list_item() {
    let source = "set n to 7\n\
                  set r to {label: \"n is {n}\", plain: \"{}\"}\n\
                  set xs to [\"n is {n}\", \"{}\", 1]\n\
                  say r.label\nsay r.plain\nsay xs[0]\nsay xs[1]\nsay xs[2]\n";
    both_engines_print(source, &["n is 7", "{}", "n is 7", "{}", "1"]);
}

#[test]
fn edge_a_list_a_record_and_an_object_all_substitute() {
    let source = "set xs to [1, 2]\n\
                  set r to {a: 1}\n\
                  object Counter\n    has total default 0\nend\n\
                  set Counter.total to 5\n\
                  say \"list: {xs}\"\n\
                  say \"record: {r}\"\n\
                  say \"object: {Counter.total}\"\n";
    both_engines_print(source, &["list: [1, 2]", "record: {a: 1}", "object: 5"]);
}

// --- values --------------------------------------------------------------------

#[test]
fn edge_the_substituted_value_is_rendered_the_way_the_interpreter_prints_it() {
    let source = "set flag to yes\n\
                  set missing to nothing\n\
                  set half to 0.5\n\
                  set xs to []\n\
                  say \"yesno: {flag}\"\n\
                  say \"nothing: {missing}\"\n\
                  say \"number: {half}\"\n\
                  say \"empty list: {xs}\"\n";
    both_engines_print(
        source,
        &[
            "yesno: yes",
            "nothing: nothing",
            "number: 0.5",
            "empty list: []",
        ],
    );
}

#[test]
fn edge_unicode_around_and_inside_a_substitution_survives() {
    let source = "set name to \"世界\"\nsay \"héllo {name} 🎉 — done\"\n";
    both_engines_print(source, &["héllo 世界 🎉 — done"]);
}

#[test]
fn edge_an_escaped_quote_next_to_a_substitution_is_not_a_delimiter() {
    // The lexer resolves `\"` before the parser sees the literal, so the quote
    // is a character and the brace pair after it is still markup.
    let source = "set x to 1\nsay \"a\\\"b{x}\"\n";
    both_engines_print(source, &["a\"b1"]);
}

#[test]
fn edge_a_substitution_inside_a_function_body_reaches_the_parameters() {
    let source = "to greet(who, n)\n    say \"{who} x{n}\"\nend\n\
                  greet(\"Ada\", 3)\n";
    both_engines_print(source, &["Ada x3"]);
}

// --- failures ------------------------------------------------------------------

#[test]
fn edge_a_name_nothing_binds_inside_braces_is_an_analyzer_error() {
    // The braces hold an expression, so the name inside them is resolved the
    // way any other name is. It must not reach the VM as the text `{nope}`.
    // The name is written inside the literal, at column 16 of the `say`, so the
    // column it is reported at is a position under that name and not a default.
    let source = "to f()\n    say \"n is {nope}\"\nend\nf()\n";
    for (engine, error) in [
        ("tree", tree_failure(source)),
        ("bytecode", bytecode_failure(source)),
    ] {
        assert_eq!(
            error.label(),
            "AnalyzerError",
            "{engine}: a name inside braces is resolved by the analyzer"
        );
        assert!(
            error.message().starts_with("Unknown variable 'nope'"),
            "{engine}: the failure must name the variable, got {:?}",
            error.message()
        );
        assert_eq!(
            error.span().map(|s| s.line),
            Some(2),
            "{engine}: the failure names the line the string is written on"
        );
        assert_eq!(
            error.span().map(|s| s.column),
            Some(16),
            "{engine}: and the column of the name inside the braces"
        );
    }
}

#[test]
fn edge_an_expression_that_fails_inside_braces_is_a_runtime_error() {
    // `1 / 0` is a runtime failure, and it is the string's failure: the line
    // and the column are the `say`, which is where the string is written.
    let source = "to f()\n    say \"half: {1 / 0}\"\nend\nf()\n";
    for (engine, error) in [
        ("tree", tree_failure(source)),
        ("bytecode", bytecode_failure(source)),
    ] {
        assert_eq!(error.label(), "RuntimeError", "{engine}");
        assert!(
            error.message().contains("Division by zero"),
            "{engine}: the failure must come from the expression, got {:?}",
            error.message()
        );
        assert_eq!(
            error.span().map(|s| s.line),
            Some(2),
            "{engine}: the failure names the line the string is written on"
        );
    }
    // The tree-walking VM walks the source, so it can name the column; a `.rbc`
    // instruction carries a line and no column, so that engine reports the
    // start of its line. Both are the same *place*, which is the claim.
    assert_eq!(tree_failure(source).span().map(|s| s.column), Some(5));
    assert_eq!(bytecode_failure(source).span().map(|s| s.column), Some(1));
}

#[test]
fn edge_a_failure_inside_braces_is_caught_by_try_on_both_engines() {
    // The same failure is caught by `try` on either engine, which is what makes
    // a substitution usable where an expression is: this ends the same way
    // `try say 1 / 0` does.
    let source = "set caught to no\n\
                  try\n    say \"half: {1 / 0}\"\ncatch error\n    set caught to yes\nend\n\
                  say \"caught: {caught}\"\n";
    both_engines_print(source, &["caught: yes"]);
}

#[test]
fn edge_a_substitution_nests_one_level_through_a_list() {
    // One level of nesting: the inner source is lexed on its own, so a text
    // literal written inside the braces brings its own delimiters with it.
    let source = "set x to \"a\"\nsay \"{[uppercase(\\\"{x}\\\")]}\"\n";
    both_engines_print(source, &["[A]"]);
}

#[test]
fn edge_substitution_nests_through_a_text_literal_inside_the_braces() {
    // Three levels, which is as deep as a source of any length can go — the
    // next level costs an escaped copy of this one, so each level doubles the
    // characters and [`MAX_NESTING_DEPTH`] is out of reach of a file (see
    // `parser::tests::edge_the_interpolation_budget_is_shared_and_bounds_the_chain`).
    //
    // Worth pinning because it is not what the rule reads like. The outer lexer
    // resolves `\"` to `"` before the parser sees the literal, so the inner
    // source is re-lexed from the *resolved* text and its quotes are its own
    // delimiters — which is exactly what makes another level of substitution
    // possible rather than impossible.
    let source = "set a to \"five\"\nsay \"{\\\"{\\\"{a}\\\"}\\\"}\\\"\"\n";
    both_engines_print(source, &["{\"five\"}\""]);
}

#[test]
fn edge_a_very_long_literal_of_braces_is_text_and_is_scanned_once() {
    // Ten thousand pairs that are not markup. The scan is linear and holds no
    // per-brace state, so the cost is the length of the literal and nothing
    // else — a file cannot make the parser slower than quadratic this way.
    let pairs = 10_000;
    let mut literal = String::with_capacity(pairs * 2);
    for _ in 0..pairs {
        literal.push_str("{}");
    }
    let source = format!("set out to \"{literal}\"\nsay length(out)\n");
    both_engines_print(&source, &[(pairs * 2).to_string().as_str()]);
}

#[test]
fn edge_a_very_long_literal_of_unmatched_opens_is_text_and_is_scanned_once() {
    // The shape the scan is most expensive on, and the one the matched-pair test
    // above does not reach: fifty thousand `{`s and no `}` anywhere, so *every*
    // one of them closes nothing. An open is answered by asking whether a `}` ever
    // reaches it, and asking that by walking forward to look for one costs the
    // rest of the literal — fifty thousand times over. `unmatched_opens` settles
    // them all in one backward pass.
    //
    // Fifty thousand is chosen so that the two costs cannot be confused: this
    // reads 50 000 characters of literal, while the scan it guards reads
    // 2 500 000 000 of them — seconds, against a thousandth of a second here.
    let opens = 50_000;
    let literal = "{".repeat(opens);
    let source = format!("set out to \"{literal}\"\nsay length(out)\n");
    both_engines_print(&source, &[opens.to_string().as_str()]);
}

#[test]
fn edge_an_unmatched_open_does_not_hide_the_pair_behind_it() {
    // The case the two tests above cannot show on their own: an open that closes
    // nothing, *followed* by one that does. Skipping the first must not skip the
    // second — the text keeps the first `{` and still substitutes the second.
    let source = "set a to 7\nsay \"{ {a}\"\n";
    both_engines_print(source, &["{ 7"]);
}

// --- the AST the parser now builds ----------------------------------------------

#[test]
fn the_parser_builds_the_node_the_ast_declares() {
    let parts = match expression_of("\"a{b}c\"\n") {
        Expr::InterpolatedText(parts) => parts,
        other => panic!("a brace pair must be the node the AST declares, got {other:?}"),
    };
    assert_eq!(parts.len(), 3, "a text, the expression, and a text");
    assert!(matches!(parts[0], Expr::Text(ref t) if t == "a"));
    assert!(matches!(parts[1], Expr::Variable(ref n, _) if n == "b"));
    assert!(matches!(parts[2], Expr::Text(ref t) if t == "c"));
}

#[test]
fn a_literal_without_a_substitution_is_still_one_text_node() {
    // The whole point of the "well formed" rule: a program that writes no
    // brace pair gets the node it got yesterday, so nothing downstream of the
    // parser — analyzer, VMs, formatter, linter — changes for it.
    assert!(matches!(
        expression_of("\"plain\"\n"),
        Expr::Text(ref t) if t == "plain"
    ));
}

// --- the consumers that had no path to the node ---------------------------------

#[test]
fn edge_the_linter_sees_a_name_used_only_inside_braces() {
    // Before the parser built the node, `name` was inside a text literal and
    // this program warned "Unused variable: 'name'". Reading the node is what
    // makes the warning fire here and stay quiet there.
    let used = "set name to \"x\"\nsay \"hi {name}\"\n";
    // `name2` is bound. The fixture used to read `set name to "x"` and
    // `say "hi {name2}"` with no `name2` anywhere, which is a program `rb run`
    // refuses with `Unknown variable 'name2'`; the `assert!(errors.is_empty())`
    // below passed only because `rb lint` asked the analyzer whether the file
    // ran *only* when the tree walk had found nothing, and this file had a
    // warning. The pairing now holds, so the fixture has to be a program that
    // runs: binding `name2` keeps the assertion and the intent — a name nothing
    // reads is reported, a name read inside braces is not — exactly as they were.
    let unused = "set name to \"x\"\nset name2 to \"y\"\nsay \"hi {name2}\"\n";
    let warnings = |source: &str| -> Vec<String> {
        let (errors, warnings) = redblue::linter::lint(source);
        assert!(errors.is_empty(), "{source:?} must be valid: {errors:?}");
        warnings.into_iter().map(|w| w.message).collect()
    };
    assert_eq!(
        warnings(used),
        Vec::<String>::new(),
        "a name read inside braces is used"
    );
    assert_eq!(
        warnings(unused),
        vec!["Unused variable: 'name'".to_string()],
        "a name nothing reads is still reported"
    );
}

#[test]
fn edge_the_linter_reads_names_through_nested_braces() {
    // The name is read inside a list literal that is itself inside a
    // substitution, so the linter has to walk InterpolatedText → List →
    // Variable to see it. It did not have that path before this phase.
    let (errors, warnings) = redblue::linter::lint("set a to 1\nsay \"{[a]}\"\n");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        warnings
            .iter()
            .map(|w| w.message.clone())
            .collect::<Vec<_>>(),
        Vec::<String>::new(),
        "a name read two nodes deep inside braces is used"
    );
}

#[test]
fn edge_the_formatter_writes_a_substitution_back_as_braces() {
    let source = "set x to 1\nsay \"n is { x + 2 }\"\n";
    let formatted = redblue::formatter::format(source).expect("the program formats");
    assert!(
        formatted.contains("\"n is {x + 2}\""),
        "the braces and the expression survive, got:\n{formatted}"
    );
    // And the formatted text still means the same thing when it is run.
    both_engines_print(&formatted, &["n is 3"]);
}

#[test]
fn edge_the_formatter_keeps_a_literal_brace_that_is_not_markup() {
    let source = "say \"a{}b\"\nsay \"c{d\"\n";
    let formatted = redblue::formatter::format(source).expect("the program formats");
    both_engines_print(&formatted, &["a{}b", "c{d"]);
}

// --- the Redblue tests this phase corrected ------------------------------------

/// The Redblue tests this phase rewrote, named here so that deleting or
/// reverting one fails this phase rather than passing quietly.
///
/// `edge_text_an_unknown_name_inside_braces_is_never_printed` is here because it
/// used to carry a name it never mentioned: its body bound `known`, never wrote
/// a brace pair, and passed whether interpolation resolved anything, refused
/// anything or printed its braces as text.
const CORRECTED: [&str; 2] = [
    "text: a well formed brace pair substitutes",
    "edge_text_an_unknown_name_inside_braces_is_never_printed",
];

#[test]
fn edge_the_corrected_redblue_test_still_exists_and_still_passes() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/test_text.rb");
    let source = std::fs::read_to_string(path).expect("the Redblue text tests are readable");
    for name in CORRECTED {
        assert!(
            source.contains(&format!("test \"{name}\"")),
            "tests/test_text.rb must still carry the test named `{name}`"
        );
    }
    // And the unknown-name one has to still be about braces: a body that names
    // no brace at all is the shape it had when it proved nothing.
    let unknown_name = source
        .split("test \"edge_text_an_unknown_name_inside_braces_is_never_printed\"")
        .nth(1)
        .and_then(|rest| rest.split("\nend").next())
        .expect("the test body");
    assert!(
        unknown_name.contains('{') && unknown_name.contains('}'),
        "the unknown-name test has to put the name inside braces, got:\n{unknown_name}"
    );

    let results = redblue::testing::run_test_file(path).expect("the file runs");
    assert_eq!(
        results.failed, 0,
        "every test in tests/test_text.rb must pass, including {:?}: {:?}",
        CORRECTED, results.errors
    );
    assert_eq!(results.passed, results.total, "no test may be skipped");
}
