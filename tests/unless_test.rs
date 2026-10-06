//! `unless <expr> then ... end` — the block the lexer already reserved at
//! `src/lexer.rs:25` and the parser used to reject outright.
//!
//! The tests below pin the three things `unless` promises: the body runs
//! exactly when the condition is **false**, the condition grammar is the same
//! as `if`'s (so one program run through both forms picks opposite branches),
//! and a malformed `unless` is a spanned [`Error::Parser`] naming the missing
//! `end` rather than a panic or a silent pass.
//!
//! The bytecode compiler has its own `unless` arm in
//! `src/bytecode/codegen.rs`, so the last test runs the same programs through
//! both VMs and demands they agree.

use redblue::bytecode::vm::BytecodeVm;
use redblue::bytecode::{compile_source, Opcode};
use redblue::lexer::Lexer;
use redblue::linter::lint;
use redblue::lsp::diagnostics;
use redblue::parser::{parse, Program};
use redblue::run_source_value;
use redblue::{Error, Value};

fn lex_and_parse(source: &str) -> Result<Program, Error> {
    parse(Lexer::tokenize(source)?)
}

fn parser_error(source: &str) -> Error {
    lex_and_parse(source).expect_err("source should fail to parse")
}

/// Asserts `error` is a spanned [`Error::Parser`] with a non-empty message and
/// hands the message back for the caller to match against.
fn assert_spanned_parser_error(error: &Error) -> String {
    match error {
        Error::Parser(message, span) => {
            assert!(
                span.is_known(),
                "parser error must carry a span, got {span:?} for {message:?}"
            );
            assert!(
                !message.is_empty(),
                "parser error must carry a message naming what was missing"
            );
            message.clone()
        }
        other => panic!("expected a Parser error, got {other:?}"),
    }
}

/// Runs `source` through lexer, parser, analyzer and VM and returns the text a
/// bare trailing `hit` evaluates to — the branch a program took, read back as a
/// value rather than scraped off stdout.
///
/// Callers write `set hit to ...` inside the branches under test; this appends
/// the read-back so the assertion is on a value the VM produced.
fn branch_taken_by(source: &str) -> String {
    let source = format!("{source}hit\n");
    match run_source_value(&source) {
        Ok(Value::Text(text)) => text,
        Ok(other) => panic!("expected text from {source:?}, got {other:?}"),
        Err(error) => panic!("{source:?} should run, got {error:?}"),
    }
}

// --- the body runs exactly when the condition is false ----------------------

#[test]
fn unless_with_a_false_condition_takes_its_body() {
    assert_eq!(
        branch_taken_by("set hit to \"none\"\nunless no then\n    set hit to \"body\"\nend\n"),
        "body",
        "unless no must take its body when the condition is false"
    );
}

#[test]
fn unless_with_a_true_condition_skips_its_body() {
    assert_eq!(
        branch_taken_by("set hit to \"none\"\nunless yes then\n    set hit to \"body\"\nend\n"),
        "none",
        "unless yes must not take its body when the condition is true"
    );
}

#[test]
fn unless_uses_the_same_condition_grammar_as_if_and_picks_opposite_branches() {
    // One program, one condition, read through both forms: `if` takes its body
    // when the condition holds, `unless` takes its body when it does not.
    for condition in ["yes", "no"] {
        let source = format!(
            "set flag to {condition}\n\
             if flag then\n    say \"if\"\nend\n\
             unless flag then\n    say \"unless\"\nend\n"
        );
        let program = lex_and_parse(&source).unwrap_or_else(|e| {
            panic!("`if`/`unless` on condition {condition:?} should parse: {e}")
        });
        assert_eq!(
            program.statements.len(),
            3,
            "the paired program should hold three top-level statements, got {}",
            program.statements.len()
        );
        assert!(
            run_source_value(&source).is_ok(),
            "the paired program should run for condition {condition:?}"
        );
    }

    // With the condition negated the two forms pick opposite branches: exactly
    // one body runs, and it is the one that belongs to the false condition.
    for (condition, expected) in [("yes", "if"), ("no", "unless")] {
        let via_if = branch_taken_by(&format!(
            "set hit to \"skipped\"\nset flag to {condition}\n\
             if flag then\n    set hit to \"if\"\nend\n"
        ));
        let via_unless = branch_taken_by(&format!(
            "set hit to \"skipped\"\nset flag to {condition}\n\
             unless flag then\n    set hit to \"unless\"\nend\n"
        ));

        let bodies_taken = usize::from(via_if == "if") + usize::from(via_unless == "unless");
        assert_eq!(
            bodies_taken, 1,
            "with condition {condition:?} exactly one of `if`/`unless` must take its body, \
             got if={via_if:?} unless={via_unless:?}"
        );
        assert_eq!(
            if condition == "no" {
                via_unless
            } else {
                via_if
            },
            expected,
            "the wrong form took its body for condition {condition:?}"
        );
    }
}

#[test]
fn unless_condition_supports_the_full_expression_grammar() {
    // Comparisons, membership, boolean operators and unary `not` all reach the
    // same expression parser `if` uses.
    let source = "set hit to \"none\"\n\
                  set n to 3\n\
                  set names to [\"a\", \"b\"]\n\
                  unless n > 10 and \"b\" is in names then\n    set hit to \"compound\"\nend\n";
    assert_eq!(
        branch_taken_by(source),
        "compound",
        "`unless` must accept the same condition expressions as `if`"
    );

    assert_eq!(
        branch_taken_by(
            "set hit to \"none\"\nset n to 3\n\
             unless not (n is 3) then\n    set hit to \"negated\"\nend\n"
        ),
        "negated",
        "`n is 3` is true, so `not (n is 3)` is false and `unless` takes its body"
    );
}

// --- edge cases -------------------------------------------------------------

#[test]
fn edge_empty_body_parses_and_runs() {
    // An empty body is legal: nothing runs either way, and the block still
    // costs its `then` and its `end`.
    for condition in ["yes", "no"] {
        let source = format!("set hit to \"none\"\nunless {condition} then\nend\n");
        let program = lex_and_parse(&source)
            .unwrap_or_else(|e| panic!("empty `unless` body should parse: {e}"));
        assert_eq!(
            program.statements.len(),
            2,
            "`set hit` plus the empty `unless` is two statements, got {}",
            program.statements.len()
        );
        assert_eq!(
            branch_taken_by(&source),
            "none",
            "an empty `unless` body cannot change anything"
        );
    }

    // Whitespace-only and comment-only bodies are empty too.
    for filler in ["", "\n\n", "    // nothing to do\n"] {
        let source = format!("unless no then\n{filler}end\n");
        assert!(
            lex_and_parse(&source).is_ok(),
            "body of {filler:?} should parse"
        );
        assert!(
            run_source_value(&source).is_ok(),
            "body of {filler:?} should run"
        );
    }
}

#[test]
fn edge_body_of_exactly_one_statement() {
    let program = lex_and_parse("unless no then\n    say \"only\"\nend\n")
        .expect("a one-statement `unless` body should parse");
    assert_eq!(
        program.statements.len(),
        1,
        "one top-level statement expected"
    );

    assert_eq!(
        branch_taken_by("set hit to \"none\"\nunless no then\n    set hit to \"one\"\nend\n"),
        "one",
        "a one-statement `unless` body must run when the condition is false"
    );
    assert_eq!(
        branch_taken_by("set hit to \"none\"\nunless yes then\n    set hit to \"one\"\nend\n"),
        "none",
        "a one-statement `unless` body must not run when the condition is true"
    );
}

#[test]
fn edge_unless_as_the_last_statement_in_a_file() {
    // No trailing newline, and `end` as the very last token of the source.
    for tail in ["end", "end\n", "end\r\n"] {
        let source = format!("unless no then\n    say \"last\"\n{tail}");
        let program = lex_and_parse(&source).unwrap_or_else(|e| {
            panic!("`unless` ending at EOF with tail {tail:?} should parse: {e}")
        });
        assert_eq!(
            program.statements.len(),
            1,
            "the trailing `unless` should be parsed as one statement"
        );
        assert!(
            run_source_value(&source).is_ok(),
            "`unless` ending at EOF with tail {tail:?} should run"
        );
    }
}

#[test]
fn edge_unless_nests_inside_if_and_unless() {
    let source = "set hit to \"none\"\n\
                  if yes then\n    unless no then\n        set hit to \"nested\"\n    end\nend\n";
    assert_eq!(
        branch_taken_by(source),
        "nested",
        "a nested `unless` inside an `if` must run"
    );
    assert_eq!(
        branch_taken_by(
            "set hit to \"none\"\nif yes then\n    unless yes then\n        set hit to \"nested\"\n    end\nend\n"
        ),
        "none",
        "a nested `unless` whose condition is true must not run"
    );
}

#[test]
fn edge_unless_without_end_is_a_spanned_parser_error() {
    for source in [
        "unless no then\n    say \"no end\"\n",
        "unless no then\n    say \"no end\"",
        "unless no then\n",
        "unless no then",
    ] {
        let message = assert_spanned_parser_error(&parser_error(source));
        assert!(
            message.contains("End") || message.contains("end"),
            "the error for {source:?} must name the missing `end`, got {message:?}"
        );
    }
}

#[test]
fn edge_unless_with_a_missing_then_is_a_spanned_parser_error() {
    let message = assert_spanned_parser_error(&parser_error("unless no\n    say \"x\"\nend\n"));
    assert!(
        message.contains("Then"),
        "the error must name the missing `then`, got {message:?}"
    );
}

#[test]
fn edge_unless_rejects_else_because_its_body_has_no_alternative() {
    // `unless` has one branch; an `else` is a shape the grammar does not give
    // it, so it must be a clean error rather than a silently dropped branch.
    let message = assert_spanned_parser_error(&parser_error(
        "unless no then\n    say \"a\"\nelse\n    say \"b\"\nend\n",
    ));
    assert!(
        message.contains("End") || message.contains("Else"),
        "the error for an `else` under `unless` must name the token it expected, got {message:?}"
    );
}

#[test]
fn edge_an_unclosed_unless_is_rejected_by_the_linter_as_an_error() {
    let (errors, _) = lint("unless no then\n    say \"x\"\n");
    assert!(
        !errors.is_empty(),
        "`rb lint` must report an unclosed `unless` as an error"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.message.to_lowercase().contains("end")),
        "the lint error must name the missing `end`, got {:?}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );

    // The same shape `if` already has: a closed `unless` lints clean.
    let (closed_errors, _) = lint("unless no then\n    say \"x\"\nend\n");
    assert!(
        closed_errors.is_empty(),
        "a closed `unless` must lint clean, got {:?}",
        closed_errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
fn edge_an_unclosed_unless_is_rejected_by_diagnostics_as_an_error() {
    let found = diagnostics("unless no then\n    say \"x\"\n");
    assert!(
        !found.is_empty(),
        "`rb diagnostics` must report an unclosed `unless`"
    );
    assert_eq!(
        found[0].severity,
        redblue::Severity::Error,
        "an unclosed `unless` is an error, not a warning: {:?}",
        found[0].message
    );
    assert_eq!(
        found[0].line, 3,
        "the diagnostic must point at the end of input that never saw `end`: {:?}",
        found[0].message
    );

    assert!(
        diagnostics("unless no then\n    say \"x\"\nend\n").is_empty(),
        "a closed `unless` must produce no diagnostics"
    );
}

// --- the bytecode compiler ---------------------------------------------------

#[test]
fn unless_compiles_and_runs_the_same_on_both_vms() {
    // `codegen.rs` has its own `unless` arm: the condition is negated with
    // `Opcode::Not` because there is no `JumpIfTrue`, and the body is the
    // fall-through. A codegen bug that inverted the condition would pass every
    // tree-walking test above, so both VMs must be compared directly.
    for (condition, expected) in [
        ("yes", vec![]),
        ("no", vec!["body".to_string()]),
        ("1 is 2", vec!["body".to_string()]),
        ("1 is 1", vec![]),
        ("not yes", vec!["body".to_string()]),
        ("not no", vec![]),
    ] {
        let source =
            format!("set hit to \"skipped\"\nunless {condition} then\n    say \"body\"\nend\n");
        let chunk = compile_source(&source)
            .unwrap_or_else(|e| panic!("`unless {condition}` should compile: {e}"));
        let mut vm = BytecodeVm::new();
        vm.run(&chunk)
            .unwrap_or_else(|e| panic!("`unless {condition}` should run on the bytecode VM: {e}"));

        let output = vm.take_output();
        assert_eq!(
            output, expected,
            "the bytecode VM printed {output:?} for `unless {condition}`, expected {expected:?}"
        );
    }
}

#[test]
fn edge_unless_whose_body_never_runs_still_exits_its_block() {
    // The one-branch block compiles to a single `JumpIfFalse` whose target is
    // the block's instruction count. If the target were wrong the bytecode VM
    // would report an out-of-range jump or run past the body.
    for source in [
        "unless yes then\n    say \"a\"\nend\n",
        "unless yes then\nend\n",
        "unless no then\n    say \"a\"\n    say \"b\"\nend\n",
    ] {
        let chunk =
            compile_source(source).unwrap_or_else(|e| panic!("{source:?} should compile: {e}"));
        let end = chunk.main.code.len() as u32;
        let targets: Vec<u32> = chunk
            .main
            .code
            .iter()
            .filter(|i| matches!(i.opcode, Opcode::Jump | Opcode::JumpIfFalse))
            .map(|i| i.arg)
            .collect();
        for target in &targets {
            assert!(
                *target <= end,
                "{source:?} emitted a jump to {target}, past the end of {end} instructions"
            );
        }

        let mut vm = BytecodeVm::new();
        vm.run(&chunk)
            .unwrap_or_else(|e| panic!("{source:?} should run on the bytecode VM: {e}"));
    }
}
