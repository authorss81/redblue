//! `else if <expr> then ... end` — the chain `SPEC.md` and `docs/GRAMMAR.md`
//! both document, and which the parser used to refuse.
//!
//! The chain shares a single closing `end`, so it is not a nested `if` in an
//! `else` body: `else if` written on one line is an arm of the same block. The
//! tests below pin the branches each arm takes, that a trailing `else` still
//! belongs to the whole chain, that both VMs agree, and that each malformed
//! shape is a spanned [`redblue::Error::Parser`] rather than a panic.

use redblue::bytecode::compile_source;
use redblue::bytecode::vm::BytecodeVm;
use redblue::lexer::Lexer;
use redblue::parser::{parse, Program};
use redblue::{run_source_value, Error, Value};

fn lex_and_parse(source: &str) -> Result<Program, Error> {
    parse(Lexer::tokenize(source)?)
}

fn parser_error(source: &str) -> Error {
    lex_and_parse(source).expect_err("source should fail to parse")
}

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

fn branch_taken_by(source: &str) -> String {
    let source = format!("{source}hit\n");
    match run_source_value(&source) {
        Ok(Value::Text(text)) => text,
        Ok(other) => panic!("expected text from {source:?}, got {other:?}"),
        Err(error) => panic!("{source:?} should run, got {error:?}"),
    }
}

fn assert_both_engines_take(source: &str, expected: &str) {
    let tree = run_source_value(source)
        .unwrap_or_else(|e| panic!("tree-walking VM should run {source:?}: {e}"));
    let chunk = compile_source(source)
        .unwrap_or_else(|e| panic!("bytecode compiler should compile {source:?}: {e}"));
    let mut vm = BytecodeVm::new();
    let byte = vm
        .run(&chunk)
        .unwrap_or_else(|e| panic!("bytecode VM should run {source:?}: {e}"));
    assert_eq!(
        tree,
        Value::Text(expected.to_string()),
        "tree-walking VM took the wrong branch in {source:?}"
    );
    assert_eq!(
        byte,
        Value::Text(expected.to_string()),
        "bytecode VM took the wrong branch in {source:?}"
    );
}

#[test]
fn else_if_chain_picks_the_matching_arm() {
    let source = "set hit to \"none\"\n\
                  set n to 2\n\
                  if n is 1 then\n    set hit to \"one\"\n\
                  else if n is 2 then\n    set hit to \"two\"\n\
                  else\n    set hit to \"other\"\n\
                  end\n";
    assert_eq!(
        branch_taken_by(source),
        "two",
        "the middle `else if` arm must be the one a condition of 2 selects"
    );
}

#[test]
fn edge_else_if_as_the_last_arm_runs_without_a_trailing_else() {
    let source = "set hit to \"none\"\n\
                  set n to 4\n\
                  if n is 1 then\n    set hit to \"one\"\n\
                  else if n is 4 then\n    set hit to \"last\"\n\
                  end\n";
    assert_eq!(
        branch_taken_by(source),
        "last",
        "an `else if` can be the last arm, with the chain's one `end` closing it"
    );
}

#[test]
fn edge_three_arms_plus_else_pick_every_branch() {
    for (n, expected) in [(1, "first"), (2, "second"), (3, "third"), (4, "else")] {
        let source = format!(
            "set hit to \"none\"\n\
             set n to {n}\n\
             if n is 1 then\n    set hit to \"first\"\n\
             else if n is 2 then\n    set hit to \"second\"\n\
             else if n is 3 then\n    set hit to \"third\"\n\
             else\n    set hit to \"else\"\n\
             end\n"
        );
        assert_eq!(
            branch_taken_by(&source),
            expected,
            "the chain must pick {expected:?} for n = {n}"
        );
    }
}

#[test]
fn edge_an_empty_then_branch_in_an_else_if_arm_takes_no_trailing_else() {
    let source = "set hit to \"none\"\n\
                  if no then\n    set hit to \"first\"\n\
                  else if yes then\n\
                  else\n    set hit to \"else\"\n\
                  end\n";
    assert_eq!(
        branch_taken_by(source),
        "none",
        "a matched `else if` with an empty then-branch must not fall through to `else`"
    );

    let source = "set hit to \"none\"\n\
                  if no then\n    set hit to \"first\"\n\
                  else if no then\n\
                  else\n    set hit to \"else\"\n\
                  end\n";
    assert_eq!(
        branch_taken_by(source),
        "else",
        "an unmatched `else if` with an empty then-branch must still take the `else`"
    );
}

#[test]
fn edge_else_if_inside_for_while_and_repeat_bodies() {
    let cases = [
        (
            "set hit to \"none\"\n\
             for each n in [1, 2, 3]\n\
                 if n is 1 then\n        set hit to \"one\"\n\
                 else if n is 3 then\n        set hit to \"three\"\n\
                 end\n\
             end\n",
            "three",
        ),
        (
            "set hit to \"none\"\n\
             set n to 0\n\
             while n < 3\n\
                 if n is 1 then\n        set hit to \"one\"\n\
                 else if n is 2 then\n        set hit to \"two\"\n\
                 end\n\
                 set n to n + 1\n\
             end\n",
            "two",
        ),
        (
            "set hit to \"none\"\n\
             set n to 0\n\
             repeat 3 times\n\
                 if n is 1 then\n        set hit to \"one\"\n\
                 else if n is 2 then\n        set hit to \"two\"\n\
                 end\n\
                 set n to n + 1\n\
             end\n",
            "two",
        ),
    ];
    for (source, expected) in cases {
        assert_eq!(
            branch_taken_by(source),
            expected,
            "an `else if` chain inside a loop body must still pick its arm"
        );
    }
}

#[test]
fn edge_a_nested_if_inside_an_else_if_arm_keeps_its_own_end() {
    let source = "set hit to \"none\"\n\
                  if no then\n    set hit to \"first\"\n\
                  else if yes then\n\
                      if yes then\n          set hit to \"nested\"\n      end\n\
                  else\n    set hit to \"else\"\n\
                  end\n";
    assert_eq!(
        branch_taken_by(source),
        "nested",
        "a nested `if` in an `else if` arm must close on its own `end` and leave the chain's"
    );
}

#[test]
fn edge_plain_if_else_without_else_if_is_unchanged() {
    assert_eq!(
        branch_taken_by("set hit to \"none\"\nif yes then\n    set hit to \"then\"\nelse\n    set hit to \"else\"\nend\n"),
        "then",
        "a plain `if`/`else` must still take its then branch"
    );
    assert_eq!(
        branch_taken_by("set hit to \"none\"\nif no then\n    set hit to \"then\"\nelse\n    set hit to \"else\"\nend\n"),
        "else",
        "a plain `if`/`else` must still take its else branch"
    );
}

#[test]
fn edge_else_then_a_newline_then_if_is_still_a_nested_if_in_the_else_body() {
    // A newline between `else` and `if` is not the `else if` arm: it is an
    // ordinary nested `if` statement, which needs its own `end` and then the
    // outer `end`. Keeping that shape is what makes the change backwards
    // compatible.
    let source = "set hit to \"none\"\n\
                  if no then\n    set hit to \"first\"\n\
                  else\n\
                      if yes then\n          set hit to \"nested\"\n      end\n\
                  end\n";
    assert_eq!(
        branch_taken_by(source),
        "nested",
        "`else` on its own line followed by `if` is a nested statement in the else body"
    );
}

#[test]
fn edge_a_same_line_chain_shares_its_end_and_rejects_the_old_two_end_shape() {
    // Before the chain form existed, `else if` on one line was read as a nested
    // `if`, so a second `end` closed the outer `if`. The chain form shares one
    // `end`; that second `end` now closes nothing. Pin the deliberate refusal so
    // the same-line rule cannot drift back silently.
    assert_spanned_parser_error(&parser_error(
        "if no then\n    say \"a\"\nelse if yes then\n    say \"b\"\nend\nend\n",
    ));
}

#[test]
fn edge_else_if_chain_agrees_between_the_two_engines() {
    for n in [1, 2, 3, 4] {
        let source = format!(
            "set hit to \"none\"\n\
             set n to {n}\n\
             if n is 1 then\n    set hit to \"one\"\n\
             else if n is 2 then\n    set hit to \"two\"\n\
             else if n is 3 then\n    set hit to \"three\"\n\
             else\n    set hit to \"other\"\n\
             end\n\
             hit\n"
        );
        let expected = match n {
            1 => "one",
            2 => "two",
            3 => "three",
            _ => "other",
        };
        assert_both_engines_take(&source, expected);
    }
}

#[test]
fn edge_else_if_missing_then_is_a_spanned_parser_error() {
    let message = assert_spanned_parser_error(&parser_error(
        "if no then\n    say \"a\"\nelse if yes\n    say \"b\"\nend\n",
    ));
    assert!(
        message.contains("'then'"),
        "the error for an `else if` with no `then` must name it, got {message:?}"
    );
}

#[test]
fn edge_else_if_missing_end_is_a_spanned_parser_error() {
    for source in [
        "if no then\n    say \"a\"\nelse if yes then\n    say \"b\"\n",
        "if no then\n    say \"a\"\nelse if yes then",
    ] {
        let message = assert_spanned_parser_error(&parser_error(source));
        assert!(
            message.contains("'end'"),
            "the error for an `else if` with no `end` must name it, got {message:?}"
        );
    }
}

#[test]
fn edge_else_with_nothing_after_it_is_a_spanned_parser_error() {
    for source in [
        "if yes then\n    say \"a\"\nelse\n",
        "if yes then\n    say \"a\"\nelse",
    ] {
        let message = assert_spanned_parser_error(&parser_error(source));
        assert!(
            message.contains("'end'") || message.contains("end of input"),
            "an `else` with no body and no `end` must name what was expected, got {message:?}"
        );
    }
}

#[test]
fn edge_a_chain_deeper_than_the_block_budget_is_a_spanned_parser_error() {
    let mut source = String::from("if n is 0 then\n    say \"0\"\n");
    for arm in 1..=70 {
        source.push_str(&format!("else if n is {arm} then\n    say \"{arm}\"\n"));
    }
    source.push_str("end\n");

    let message = assert_spanned_parser_error(&parser_error(&source));
    assert!(
        message.contains("nest more than"),
        "a chain past the block budget must be a clean depth error, got {message:?}"
    );
}

#[test]
fn edge_else_if_chain_inside_a_function_picks_its_arm() {
    for (n, expected) in [(0, "zero"), (-2, "negative"), (1, "one"), (9, "many")] {
        let source = format!(
            "to classify(n)\n\
                 set label to \"none\"\n\
                 if n is 0 then\n        set label to \"zero\"\n\
                 else if n < 0 then\n        set label to \"negative\"\n\
                 else if n is 1 then\n        set label to \"one\"\n\
                 else\n        set label to \"many\"\n\
                 end\n\
                 give back label\n\
             end\n\
             set hit to classify({n})\n"
        );
        assert_eq!(
            branch_taken_by(&source),
            expected,
            "an `else if` chain in a function body must pick {expected:?} for n = {n}"
        );
    }
}

#[test]
fn edge_else_if_with_no_condition_is_a_spanned_parser_error() {
    let result = lex_and_parse("if no then\n    say \"a\"\nelse if then\n    say \"b\"\nend\n");
    assert!(
        result.is_err(),
        "an `else if` with no condition must be refused, not parsed as an empty condition"
    );
    let message = assert_spanned_parser_error(&result.expect_err("missing condition must fail"));
    assert!(
        message.contains("then") || message.contains("Unexpected token"),
        "the error must name the token where the condition should be, got {message:?}"
    );
}

#[test]
fn edge_else_if_chain_survives_crlf_line_endings() {
    let source = "set hit to \"none\"\r\n\
                  if no then\r\n    set hit to \"a\"\r\n\
                  else if yes then\r\n    set hit to \"b\"\r\n\
                  else\r\n    set hit to \"c\"\r\n\
                  end\r\n";
    assert_eq!(
        branch_taken_by(source),
        "b",
        "a same-line `else if` chain must still parse when every line ends in CRLF"
    );
}

#[test]
fn edge_else_if_compound_conditions_choose_the_first_matching_arm() {
    for (n, expected) in [(5, "small"), (20, "medium"), (-1, "negative")] {
        let source = format!(
            "set hit to \"none\"\n\
             set n to {n}\n\
             if n < 0 then\n    set hit to \"negative\"\n\
             else if n > 0 and n < 10 then\n    set hit to \"small\"\n\
             else if n > 10 or n is 10 then\n    set hit to \"medium\"\n\
             else\n    set hit to \"other\"\n\
             end\n"
        );
        assert_eq!(
            branch_taken_by(&source),
            expected,
            "compound conditions must pick {expected:?} for n = {n}"
        );
    }
}
