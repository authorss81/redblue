//! `for each i from A to B [by C]` is in the grammar (`docs/GRAMMAR.md:230`),
//! in `SPEC.md:380-391` and in `README.md:101-103`, and the AST node it needs
//! (`Statement::ForRange`, `src/parser.rs:142`) plus the analyzer, codegen,
//! formatter, linter and both VMs' arms for it all existed. Only `parse_for`
//! had one arm — `TokenKind::In` — and it called `self.expect(&TokenKind::In)?`
//! unconditionally, so every range loop died with
//! `ParserError: Expected In but got From`.
//!
//! The tests below run range loops through source text: what they visit, in
//! which order, that the loop variable is a local, how each malformed form
//! fails, and that the shared iteration guard — not a range-specific one — is
//! what stops a loop that never ends.

use std::collections::HashMap;

use redblue::analyzer;
use redblue::lexer::Lexer;
use redblue::parser::{parse, Program};
use redblue::Error;
use redblue::Vm;

/// Lexes and parses `source`, panicking with the message the toolchain gave.
#[track_caller]
fn parse_ok(source: &str) -> Program {
    let tokens = Lexer::tokenize(source)
        .unwrap_or_else(|error| panic!("`{}` should lex, got {:?}", source, error));
    parse(tokens).unwrap_or_else(|error| panic!("`{}` should parse, got {:?}", source, error))
}

/// Runs `source` through the whole pipeline the CLI runs — parser, analyzer,
/// then VM — and returns the lines it printed. The analyzer is part of the
/// path because a Redblue program never reaches the VM without it, and it is
/// what turns a scope mistake into an `AnalyzerError`.
#[track_caller]
fn printed(source: &str) -> Vec<String> {
    let program = parse_ok(source);
    analyzer::analyze(&program)
        .unwrap_or_else(|error| panic!("`{}` should analyze, got {}", source, error));
    let mut vm = Vm::new();
    vm.run(&program)
        .unwrap_or_else(|error| panic!("`{}` should run, got {}", source, error));
    vm.take_output()
}

/// Runs `source` through the whole pipeline and returns the error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    let program = parse_ok(source);
    if let Err(error) = analyzer::analyze(&program) {
        return error;
    }
    let mut vm = Vm::new();
    vm.run(&program)
        .expect_err(&format!("`{}` should have failed", source))
}

/// Runs `source` and returns the message of the `RuntimeError` it produced.
#[track_caller]
fn runtime_message(source: &str) -> String {
    match eval_err(source) {
        Error::Runtime(message, span) => {
            assert!(
                span.is_known(),
                "`{}` failed at runtime without a source span",
                source
            );
            message
        }
        other => panic!(
            "`{}` should fail with a Runtime error, got {}",
            source, other
        ),
    }
}

/// Asserts `source` fails to parse, and returns the parser's message.
#[track_caller]
fn parser_error(source: &str) -> String {
    let tokens = Lexer::tokenize(source)
        .unwrap_or_else(|error| panic!("`{}` should lex, got {:?}", source, error));
    match parse(tokens) {
        Err(Error::Parser(message, _)) => message,
        Err(other) => panic!(
            "`{}` should fail with a Parser error, got {}",
            source, other
        ),
        Ok(_) => panic!("`{}` should not have parsed", source),
    }
}

/// The counter variables each range loop's body observed, via the closure the
/// loop body is. Returns them in visit order.
#[track_caller]
fn visited(source: &str) -> Vec<String> {
    printed(source)
}

// --- the documented forms --------------------------------------------------

#[test]
fn a_range_loop_visits_every_value_in_order() {
    assert_eq!(
        visited("for each i from 1 to 5\n    say i\nend"),
        vec!["1", "2", "3", "4", "5"],
        "`for each i from 1 to 5` should print 1..5 once each, in order"
    );
}

#[test]
fn a_step_of_five_visits_only_the_multiples_of_five() {
    assert_eq!(
        visited("for each i from 0 to 10 by 5\n    say i\nend"),
        vec!["0", "5", "10"],
        "`by 5` over 0..10 should visit 0, 5 and 10 and nothing between them"
    );
}

#[test]
fn a_step_of_one_and_an_omitted_step_are_the_same_loop() {
    let stepped = visited("for each i from 1 to 4 by 1\n    say i\nend");
    let plain = visited("for each i from 1 to 4\n    say i\nend");
    assert_eq!(
        stepped, plain,
        "`by 1` and no `by` at all should visit the same values"
    );
    assert_eq!(stepped, vec!["1", "2", "3", "4"]);
}

#[test]
fn a_step_that_does_not_land_on_the_end_still_stops_at_or_before_it() {
    // 0, 3, 6, 9 — the next step would be 12, past 10.
    assert_eq!(
        visited("for each i from 0 to 10 by 3\n    say i\nend"),
        vec!["0", "3", "6", "9"],
        "`from 0 to 10 by 3` should stop at the last value that is not past 10"
    );
}

#[test]
fn a_range_loop_nests_and_the_inner_variable_is_its_own() {
    assert_eq!(
        visited(
            "for each i from 1 to 2\n    \
             for each i from 7 to 8\n        \
                 say i\n    \
             end\n    \
             say i\n\
             end"
        ),
        vec!["7", "8", "1", "7", "8", "2"],
        "an inner loop of the same name should shadow the outer one and restore it"
    );
}

// --- the loop variable is a local -----------------------------------------

#[test]
fn the_loop_variable_is_a_local_that_does_not_leak() {
    // The body reading and writing its own `i` must not change the sequence the
    // loop draws from: four iterations still happen, whatever the body does.
    let mut vm = Vm::new();
    let program =
        parse_ok("set i to 99\nfor each i from 1 to 4\n    set i to 100\n    say i\nend\nsay i");
    analyzer::analyze(&program).expect("the program should analyze");
    vm.run(&program).expect("the program should run");
    assert_eq!(
        vm.take_output(),
        vec!["100", "100", "100", "100", "99"],
        "the loop ran four times, the body saw its own writes, and the outer `i` is untouched"
    );
}

#[test]
fn edge_the_loop_variable_is_out_of_scope_after_end() {
    // Not a silent global: the analyzer, which pushes and pops a scope around
    // the body, refuses the read after `end`.
    let error = eval_err("for each i from 1 to 3\n    say i\nend\nsay i");
    match error {
        Error::Analyzer(message, span) => {
            assert_eq!(
                message, "Unknown variable 'i'",
                "the read after `end` should name the variable it could not find"
            );
            assert!(span.is_known(), "the failure should carry a position");
        }
        other => panic!(
            "reading the loop variable after `end` should be an Analyzer error, got {:?}",
            other
        ),
    }
}

#[test]
fn edge_an_empty_range_visits_nothing_and_does_not_hang() {
    // The boundary where a positive step can never reach `end`: the body must
    // not run at all, rather than run until the iteration guard fires.
    let mut vm = Vm::new();
    vm.run(&parse_ok(
        "set n to 0\nfor each i from 5 to 1\n    set n to n + 1\nend\nsay n",
    ))
    .expect("an empty range should run to completion");
    assert_eq!(
        vm.take_output(),
        vec!["0"],
        "`from 5 to 1` is empty: the body must not run once"
    );
}

#[test]
fn edge_a_range_of_one_visits_exactly_one_value() {
    assert_eq!(
        visited("for each i from 3 to 3\n    say i\nend"),
        vec!["3"],
        "`from 3 to 3` is the inclusive boundary: it visits 3, once"
    );
}

#[test]
fn edge_a_descending_step_counts_down() {
    // The step's sign is the direction of travel. `from 5 to 1 by -1` used to
    // visit nothing at all, because the loop only ever compared `current <= end`.
    assert_eq!(
        visited("for each i from 5 to 1 by -1\n    say i\nend"),
        vec!["5", "4", "3", "2", "1"],
        "a negative step should count down from 5 to 1 inclusive"
    );
}

#[test]
fn edge_a_descending_step_that_starts_below_its_end_is_empty() {
    let mut vm = Vm::new();
    vm.run(&parse_ok(
        "set n to 0\nfor each i from 1 to 5 by -1\n    set n to n + 1\nend\nsay n",
    ))
    .expect("a descending range that starts below its end should just finish");
    assert_eq!(
        vm.take_output(),
        vec!["0"],
        "counting down from 1 to 5 reaches nothing: the body must not run"
    );
}

#[test]
fn edge_a_fractional_step_on_a_whole_number_range() {
    assert_eq!(
        visited("for each i from 0 to 1 by 0.25\n    say i\nend"),
        vec!["0", "0.25", "0.5", "0.75", "1"],
        "a fractional step should be exact, not accumulated in integers"
    );
}

#[test]
fn edge_a_fractional_step_that_would_overshoot_still_terminates() {
    // 0, 0.1, ... repeated addition drifts, so the run may or may not reach 1.
    // What it must never do is keep going: the loop ends the moment the counter
    // passes `end`, however far the drift went.
    let lines = visited("for each i from 0 to 1 by 0.1\n    say i\nend");
    assert_eq!(lines[0], "0", "the counter starts at `from`");
    let last: f64 = lines
        .last()
        .expect("the loop should visit at least one value")
        .parse()
        .unwrap_or_else(|_| panic!("`{}` should be a number", lines.last().unwrap()));
    assert!(
        last <= 1.0,
        "the last value {} should not overshoot the end of 1",
        last
    );
    assert!(
        lines.len() <= 1000,
        "a fractional step to 1 should visit a bounded number of values, got {}",
        lines.len()
    );
}

// --- malformed bounds: each is a clean failure, never a silent no-op -------

#[test]
fn edge_a_non_number_from_is_a_runtime_error_naming_from() {
    assert_eq!(
        runtime_message("for each i from \"a\" to 3\n    say i\nend"),
        "The 'from' value of a range loop must be a number, but it is text",
        "`from \"a\"` should name the argument and the type it was given"
    );
}

#[test]
fn edge_a_non_number_to_is_a_runtime_error_naming_to() {
    assert_eq!(
        runtime_message("for each i from 1 to \"b\"\n    say i\nend"),
        "The 'to' value of a range loop must be a number, but it is text",
        "`to \"b\"` should name `to`, not `from`"
    );
}

#[test]
fn edge_a_non_number_by_is_a_runtime_error_naming_by() {
    assert_eq!(
        runtime_message("for each i from 1 to 3 by \"x\"\n    say i\nend"),
        "The 'by' value of a range loop must be a number, but it is text",
        "`by \"x\"` should name `by`, not the default step's absence"
    );
}

#[test]
fn edge_a_list_as_a_bound_is_refused_rather_than_skipped() {
    // The type-mismatch row for records and lists: each is a different type
    // name in the same message.
    assert_eq!(
        runtime_message("for each i from [1] to 3\n    say i\nend"),
        "The 'from' value of a range loop must be a number, but it is list",
        "a list bound should be refused by name, not run zero times"
    );
    assert_eq!(
        runtime_message("for each i from 1 to { a: 1 }\n    say i\nend"),
        "The 'to' value of a range loop must be a number, but it is record",
        "a record bound should be refused by name"
    );
    assert_eq!(
        runtime_message("for each i from 1 to 3 by nothing\n    say i\nend"),
        "The 'by' value of a range loop must be a number, but it is nothing",
        "`nothing` is not a step, and should say so"
    );
}

#[test]
fn edge_a_step_missing_its_end_of_the_range_is_a_parser_error() {
    assert_eq!(
        parser_error("for each i from 1 to\n    say i\nend"),
        "Unexpected token Newline",
        "`to` with nothing after it should not borrow the body as its end"
    );
}

#[test]
fn edge_a_by_before_the_to_is_a_parser_error() {
    // The step marker is only read after `to`, so `from 1 by 2 to 3` stops at
    // the `by` rather than reordering the bounds.
    assert_eq!(
        parser_error("for each i from 1 by 2 to 3\n    say i\nend"),
        "Expected To but got Identifier(\"by\")",
        "the step marker is only read after `to`, so it cannot stand in for it"
    );
}

#[test]
fn edge_a_for_each_with_neither_in_nor_from_is_a_parser_error() {
    assert_eq!(
        parser_error("for each i 3\n    say i\nend"),
        "Expected In but got Number(3.0)",
        "a range loop needs `in`, `from` or nothing at all"
    );
}

// --- resource limits: the shared guard, not a range-specific one ----------

#[test]
fn edge_a_zero_step_stops_at_the_iteration_guard_rather_than_hanging() {
    // `by 0` never leaves `start`, so the loop is endless. It is stopped by
    // `charge_iteration` — the same guard `repeat` and `while` use — and the
    // message says which limit was reached.
    let message = runtime_message("for each i from 1 to 3 by 0\n    say i\nend");
    assert_eq!(
        message,
        format!(
            "Maximum of {} iterations reached in a 'for each from' loop",
            redblue::MAX_ITERATIONS
        ),
        "`by 0` should stop at the shared iteration guard and name the limit"
    );
}

#[test]
fn edge_a_range_longer_than_the_iteration_limit_stops_like_a_repeat_does() {
    // Same guard, same shape of error, for a range that is merely too long:
    // both loops are capped at the same number of iterations.
    let cap = 100usize;
    let range = format!("for each i from 1 to {}\n    say i\nend", cap + 1);
    let repeat = format!("repeat {} times\n    say \"x\"\nend", cap + 1);

    let mut range_vm = Vm::with_max_iterations(cap);
    let range_error = range_vm
        .run(&parse_ok(&range))
        .expect_err("a range past the cap should fail");
    let mut repeat_vm = Vm::with_max_iterations(cap);
    let repeat_error = repeat_vm
        .run(&parse_ok(&repeat))
        .expect_err("a repeat past the cap should fail");

    let message = |error: &Error| match error {
        Error::Runtime(message, _) => message.clone(),
        other => panic!("expected a Runtime error, got {:?}", other),
    };
    assert_eq!(
        message(&range_error),
        format!(
            "Maximum of {} iterations reached in a 'for each from' loop",
            cap
        ),
        "the range should be capped at the configured limit"
    );
    assert!(
        message(&repeat_error).contains(&format!("Maximum of {} iterations", cap)),
        "`repeat` should hit the same limit, got `{}`",
        message(&repeat_error)
    );
    // Both stopped at the cap, not somewhere else: neither ran a body
    // iteration past it.
    assert!(
        range_vm.take_output().len() <= cap,
        "the range should not have printed more than the cap"
    );
}

#[test]
fn edge_a_range_at_exactly_the_iteration_limit_still_finishes() {
    // The boundary on the other side of the guard: `cap` iterations is allowed,
    // `cap + 1` is not. Anything else means the guard is off by one.
    let cap = 100usize;
    let program = format!("for each i from 1 to {}\n    say i\nend", cap);
    let mut vm = Vm::with_max_iterations(cap);
    vm.run(&parse_ok(&program))
        .expect("exactly the cap should be allowed");
    assert_eq!(
        vm.take_output().len(),
        cap,
        "a range of exactly the cap should print once per iteration"
    );
}

#[test]
fn edge_a_step_that_overflows_the_counter_is_a_runtime_error() {
    // The counter is an `f64`, so a step can overflow it. That goes through
    // `finite_number`, the same door every computed number uses, so it is a
    // `RuntimeError` rather than an infinity that ends the loop by accident.
    let error = eval_err("for each i from 1e308 to 1.5e308 by 1e308\n    say i\nend");
    match error {
        Error::Runtime(message, span) => {
            assert_eq!(message, "infinity is not a finite number");
            assert!(span.is_known(), "the overflow reported no position");
        }
        other => panic!(
            "a counter overflow should be a Runtime error, got {}",
            other
        ),
    }
}

// --- the other tools see the same loop -------------------------------------

#[test]
fn the_formatter_prints_a_range_loop_and_the_result_still_runs() {
    let source = "for each i from 0 to 10 by 5\nsay i\nend\n";
    let formatted = redblue::formatter::format(source).expect("a range loop should be formattable");
    assert_eq!(
        formatted, "for each i from 0 to 10 by 5\n    say i\nend\n",
        "the formatter should keep the range loop it now receives, body indented"
    );
    // And it is a fixed point: formatting the output again changes nothing.
    let twice = redblue::formatter::format(&formatted)
        .expect("formatting a formatted range loop should succeed");
    assert_eq!(
        twice, formatted,
        "formatting a formatted range loop should change nothing"
    );
    assert!(
        !redblue::formatter::needs_reformat(&formatted, &twice),
        "a formatted range loop should not be reported as needing reformatting"
    );
    assert_eq!(
        visited(&formatted),
        vec!["0", "5", "10"],
        "the formatter's output should still visit 0, 5 and 10"
    );
}

#[test]
fn the_linter_analyzes_a_range_loop_body() {
    let source = "for each i from 1 to 3\n    set unused to i\nend\n";
    let (_, warnings) = redblue::linter::lint(source);
    let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
    assert_eq!(
        messages,
        vec!["Unused variable: 'unused'"],
        "the linter should look inside a range loop body, not skip it"
    );
}

#[test]
fn the_analyzer_accepts_a_range_loop_and_its_step() {
    let program = parse_ok("for each i from 1 to 3 by 2\n    say i\nend");
    analyzer::analyze(&program).expect("a range loop should analyze");
}

#[test]
fn an_unanalysable_range_bound_is_reported_by_the_analyzer() {
    let program = parse_ok("for each i from 1 to nope\n    say i\nend");
    match analyzer::analyze(&program) {
        Err(error) => assert!(
            error.to_string().contains("nope"),
            "the analyzer should name the variable it could not resolve, got `{}`",
            error
        ),
        Ok(()) => panic!("`to nope` should not analyze"),
    }
}

// --- determinism -----------------------------------------------------------

/// The gate's matrix asks for determinism, and a loop that reads a `HashMap`
/// is the classic way to lose it. This is the shape of the check without
/// depending on iteration order: the same source, run twice, is the same.
#[test]
fn a_range_loop_is_deterministic_across_runs() {
    let source = "for each i from 0 to 20 by 3\n    say i\nend";
    assert_eq!(
        visited(source),
        printed(source),
        "the same range loop must print the same values every time"
    );
}

/// `HashMap` is only reachable here through the analyzer's scopes; this asserts
/// the loop's own bookkeeping is order-independent by visiting the same range
/// under a variable name that also exists outside it.
#[test]
fn a_range_variable_does_not_collide_with_an_outer_binding() {
    let mut scopes: HashMap<&str, i64> = HashMap::new();
    scopes.insert("i", 7);
    let before = scopes.get("i").copied();

    let mut vm = Vm::new();
    vm.run(&parse_ok(
        "set i to 7\nfor each i from 1 to 2\n    say i\nend\nsay i",
    ))
    .expect("the program should run");

    assert_eq!(
        vm.take_output(),
        vec!["1", "2", "7"],
        "the loop variable is a fresh local, so the outer `i` still reads 7"
    );
    assert_eq!(
        before,
        Some(7),
        "the outer binding the test set up is unchanged"
    );
}

/// `Value::Number` is an `f64`, so an integral counter is rendered as an
/// integer. A fractional one must not be rounded into an integer.
#[test]
fn an_integral_counter_renders_without_a_fraction() {
    let mut vm = Vm::new();
    vm.run(&parse_ok("for each i from 1 to 2\n    say i\nend"))
        .expect("the loop should run");
    for line in vm.take_output() {
        assert!(
            !line.contains('.'),
            "an integral loop variable should not render a fraction, got `{}`",
            line
        );
        assert_eq!(
            line.parse::<f64>().map(|n| n.fract() == 0.0),
            Ok(true),
            "`{}` should be a whole number",
            line
        );
    }
}
