//! `else if <expr> then ...` — the chain `SPEC.md:431` and `docs/GRAMMAR.md:227`
//! both write and `src/parser.rs:parse_if` used to refuse.
//!
//! The parser had no else-if arm: after consuming `else` it entered a plain
//! statement loop, so the following `if` was parsed as a *nested* `if`, that
//! nested `if` consumed the `end` belonging to the outer one, and the outer
//! `parse_if` then failed on its own `expect(End)` with `Expected 'end' but got
//! Eof`. Every position was refused — as the last arm, in the middle with a
//! trailing `else`, and in a chain of two.
//!
//! The tests below pin the form the documentation promises: a chain picks the
//! first arm whose condition holds, a trailing `else` is the arm nothing matched,
//! the two-arm `if`/`else` is unchanged, a malformed chain is a spanned
//! [`Error::Parser`] naming what was missing rather than a panic, and both
//! engines — the tree-walking VM and the bytecode VM — agree on every one of
//! them.

use redblue::bytecode::compile_source;
use redblue::bytecode::vm::BytecodeVm;
use redblue::lexer::Lexer;
use redblue::parser::{open_block_depth, parse, Statement, MAX_BLOCK_DEPTH};
use redblue::{run_source_value, Error, Value};

fn lex_and_parse(source: &str) -> Result<redblue::parser::Program, Error> {
    parse(Lexer::tokenize(source)?)
}

/// Runs `source` on the tree-walking VM and reads back the branch it took from a
/// trailing `hit`, so the assertion is on a value rather than scraped stdout.
fn branch_taken_by(source: &str) -> String {
    let source = format!("{source}hit\n");
    match run_source_value(&source) {
        Ok(Value::Text(text)) => text,
        Ok(other) => panic!("expected text from {source:?}, got {other:?}"),
        Err(error) => panic!("{source:?} should run, got {error:?}"),
    }
}

/// Runs `source` through the bytecode compiler and its VM, reading back the same
/// trailing `hit` the tree-walking VM read.
fn branch_taken_by_bytecode(source: &str) -> String {
    let source = format!("{source}hit\n");
    let chunk = compile_source(&source)
        .unwrap_or_else(|error| panic!("{source:?} should compile: {error:?}"));
    let mut vm = BytecodeVm::new();
    match vm.run(&chunk) {
        Ok(Value::Text(text)) => text,
        Ok(other) => panic!("expected text from {source:?}, got {other:?}"),
        Err(error) => panic!("{source:?} should run on the bytecode VM, got {error:?}"),
    }
}

/// Asserts both engines took the same branch, and returns it.
fn branch_on_both_engines(source: &str) -> String {
    let tree = branch_taken_by(source);
    let bytecode = branch_taken_by_bytecode(source);
    assert_eq!(tree, bytecode, "the two engines disagreed on {source:?}");
    tree
}

/// The message a [`Error::Parser`] carries.
fn parser_message(error: &Error) -> String {
    match error {
        Error::Parser(message, _) => message.clone(),
        other => panic!("expected a Parser error, got {other:?}"),
    }
}

/// Asserts `source` is a spanned [`Error::Parser`] naming `expected` in its
/// message, and returns the message.
fn assert_parser_error_naming(source: &str, expected: &str) -> String {
    match lex_and_parse(source) {
        Ok(program) => panic!("{source:?} should not parse, got {program:?}"),
        Err(error) => match &error {
            Error::Parser(message, span) => {
                assert!(
                    span.is_known(),
                    "parser error must carry a span, got {span:?} for {message:?}"
                );
                assert!(
                    message.contains(expected),
                    "{source:?} should be refused naming {expected:?}, got {message:?}"
                );
                message.clone()
            }
            other => panic!("{source:?}: expected a Parser error, got {other:?}"),
        },
    }
}

// --- the chain picks the first arm whose condition holds -------------------

#[test]
fn else_if_as_the_last_arm_runs_its_branch() {
    assert_eq!(
        branch_on_both_engines(
            "set hit to \"none\"\n\
             if 9 is 9 then\n    set hit to \"nine\"\n\
             else if 9 is 1 then\n    set hit to \"one\"\nend\n"
        ),
        "nine",
        "a false `else if` arm must not run when the `if` arm's condition holds"
    );
}

#[test]
fn else_if_in_the_middle_is_reached_when_the_if_arm_is_false() {
    assert_eq!(
        branch_on_both_engines(
            "set hit to \"none\"\n\
             if 1 is 2 then\n    set hit to \"a\"\n\
             else if 2 is 2 then\n    set hit to \"b\"\n\
             else\n    set hit to \"c\"\nend\n"
        ),
        "b",
        "the `else if` arm must run when the `if` arm's condition is false"
    );
}

#[test]
fn a_trailing_else_is_the_arm_nothing_matched() {
    assert_eq!(
        branch_on_both_engines(
            "set hit to \"none\"\n\
             if 1 is 2 then\n    set hit to \"a\"\n\
             else if 2 is 3 then\n    set hit to \"b\"\n\
             else\n    set hit to \"c\"\nend\n"
        ),
        "c",
        "the trailing `else` must run when no arm's condition holds"
    );
}

#[test]
fn three_chained_else_if_arms_take_the_first_that_holds() {
    for (first, second, third, expected) in [
        ("yes", "no", "no", "a"),
        ("no", "yes", "no", "b"),
        ("no", "no", "yes", "c"),
        ("no", "no", "no", "d"),
    ] {
        let source = format!(
            "set hit to \"none\"\n\
             if {first} then\n    set hit to \"a\"\n\
             else if {second} then\n    set hit to \"b\"\n\
             else if {third} then\n    set hit to \"c\"\n\
             else\n    set hit to \"d\"\nend\n"
        );
        assert_eq!(
            branch_on_both_engines(&source),
            expected,
            "with arm conditions {first}/{second}/{third} the chain must take the \
             first that holds"
        );
    }
}

#[test]
fn a_two_arm_if_else_with_no_else_if_is_unchanged() {
    assert_eq!(
        branch_on_both_engines("set hit to \"none\"\nif yes then\n    set hit to \"a\"\nelse\n    set hit to \"b\"\nend\n"),
        "a",
        "the `if` arm must still win when its condition holds"
    );
    assert_eq!(
        branch_on_both_engines("set hit to \"none\"\nif no then\n    set hit to \"a\"\nelse\n    set hit to \"b\"\nend\n"),
        "b",
        "the `else` arm must still run when the condition is false"
    );
    assert_eq!(
        branch_on_both_engines("set hit to \"none\"\nif no then\n    set hit to \"a\"\nend\n"),
        "none",
        "an `if` with no `else` must leave the variable alone"
    );
}

#[test]
fn a_chain_desugars_to_nested_if_statements_one_end_apart() {
    // The chain is sugar for nested `if`s, so the tree it parses to has to be
    // the nested tree: each arm holds the rest of the chain as its one and only
    // `else` statement, and each desugared arm is positioned at the `if` it was
    // written as. A parser that built a chain of its own node would pass every
    // behavioural test above and fail this one.
    let source = "set hit to \"none\"\n\
                  if 1 is 2 then\n    set hit to \"a\"\n\
                  else if 2 is 3 then\n    set hit to \"b\"\n\
                  else if 3 is 3 then\n    set hit to \"c\"\n\
                  else\n    set hit to \"d\"\nend\n";
    let program = lex_and_parse(source).expect("a three-arm chain should parse");
    let chain = &program.statements[1];

    let mut spans = Vec::new();
    let mut arms = 0;
    let mut current = (&chain.span, &chain.statement);
    let innermost = loop {
        arms += 1;
        let (
            span,
            Statement::If {
                then_branch,
                else_branch,
                ..
            },
        ) = current
        else {
            panic!("arm {arms} is an `if`, got {:?}", current.1);
        };
        spans.push(span.line);
        assert_eq!(
            then_branch.len(),
            1,
            "arm {arms} holds the one statement it was written with"
        );
        assert!(
            else_branch.len() <= 1,
            "arm {arms} holds {} branches; a chain arm holds the rest of the \
             chain, or the trailing `else`, never both",
            else_branch.len()
        );
        match else_branch.as_slice() {
            [next] if matches!(next.statement, Statement::If { .. }) => {
                current = (&next.span, &next.statement)
            }
            _ => break else_branch.as_slice(),
        }
    };
    assert_eq!(
        spans,
        vec![2, 4, 6],
        "each desugared arm is positioned at the `if` it was written as"
    );
    assert_eq!(
        arms, 3,
        "an `if` plus two chained arms is three nested arms"
    );
    assert_eq!(
        innermost.len(),
        1,
        "the trailing `else` is the innermost arm's own else_branch"
    );
    assert!(
        matches!(innermost[0].statement, Statement::Set { .. }),
        "the trailing `else` holds its own statements, not another `if`: {:?}",
        innermost[0].statement
    );
}

// --- edge cases -----------------------------------------------------------

#[test]
fn edge_an_empty_then_branch_in_an_else_if_arm_is_not_the_end_of_the_chain() {
    // The empty arm falls through to the `else if` after it: reading the empty
    // body as "nothing left" is exactly the bug this pins.
    assert_eq!(
        branch_on_both_engines(
            "set hit to \"none\"\n\
             if 1 is 2 then\n    set hit to \"a\"\n\
             else if 1 is 3 then\n\
             else if 3 is 3 then\n    set hit to \"c\"\n\
             else\n    set hit to \"d\"\nend\n"
        ),
        "c",
        "an empty arm must fall through to the next one, not end the chain"
    );
}

#[test]
fn edge_an_else_if_chain_inside_a_loop_body_takes_its_branch_per_iteration() {
    let source = "set hit to \"\"\n\
                  for each n in [1, 2, 3, 4]\n    if n is 1 then\n        set hit to hit + \"1\"\n    else if n is 2 then\n        set hit to hit + \"2\"\n    else if n is 4 then\n        set hit to hit + \"4\"\n    else\n        set hit to hit + \"m\"\n    end\nend\n";
    assert_eq!(
        branch_on_both_engines(source),
        "12m4",
        "each iteration must take the arm its number matches"
    );
}

/// `while` and `repeat` open their body the same way `for` does, so the chain's
/// one `end` has to be told apart from the loop's one `end` in all three. `while`
/// is the sharper of the two: its condition is an expression, so a chain in its
/// body has to stop at the chain's `end` rather than swallow the loop's.
#[test]
fn edge_an_else_if_chain_closes_inside_a_while_body_and_a_repeat_body() {
    let while_source = "set hit to \"\"\n\
                        set i to 0\n\
                        while i < 4\n    if i is 0 then\n        set hit to hit + \"0\"\n\
                        else if i is 3 then\n        set hit to hit + \"3\"\n\
                        else\n        set hit to hit + \"m\"\n    end\n    set i to i + 1\nend\n";
    assert_eq!(
        branch_on_both_engines(while_source),
        "0mm3",
        "a chain in a `while` body must end at the chain's `end` and leave the \
         loop's alone"
    );

    let repeat_source = "set hit to \"\"\n\
                         repeat 3 times\n    if 1 is 2 then\n        set hit to hit + \"x\"\n\
                         else if 1 is 1 then\n        set hit to hit + \"y\"\n\
                         else\n        set hit to hit + \"z\"\n    end\nend\n";
    assert_eq!(
        branch_on_both_engines(repeat_source),
        "yyy",
        "a chain in a `repeat` body must end at the chain's `end` and leave the \
         loop's alone"
    );
}

#[test]
fn edge_a_nested_if_inside_an_else_if_arm_keeps_its_own_end() {
    assert_eq!(
        branch_on_both_engines(
            "set hit to \"none\"\n\
             if 1 is 2 then\n    set hit to \"a\"\n\
             else if 2 is 2 then\n    if 3 is 3 then\n        set hit to \"inner\"\n    end\n\
             else\n    set hit to \"c\"\nend\n"
        ),
        "inner",
        "a nested `if` inside an `else if` arm must own its `end`"
    );
}

#[test]
fn edge_an_else_if_arm_condition_uses_the_same_truth_rule_as_if() {
    // An `else if` arm asks its condition the same question an `if` does, so a
    // falsy value that skips an `if` body must skip an `else if` arm too: the
    // chain must not give `nothing`, `0`, `""` and `[]` a meaning of its own.
    for (value, holds) in [
        ("nothing", false),
        ("0", false),
        ("\"\"", false),
        ("[]", false),
        ("1", true),
        ("\"x\"", true),
        ("[1]", true),
        ("\"日本\"", true),
        ("\"🙂\"", true),
        ("\"日本 and 🙂\"", true),
    ] {
        // The same condition read through the `if` position and through the
        // `else if` position, the second reached by a first arm that does not
        // hold so the chain falls through to the arm under test.
        let through_if = format!("set v to {value}\nif v then\n    set hit to \"arm\"\nend\n");
        let through_else_if = format!(
            "set v to {value}\n\
             if no then\n    set hit to \"if-arm\"\n\
             else if v then\n    set hit to \"arm\"\nend\n"
        );
        let expected = if holds { "arm" } else { "none" };
        assert_eq!(
            branch_on_both_engines(&format!("set hit to \"none\"\n{through_if}")),
            expected,
            "an `if` over {value:?} must {}",
            if holds { "hold" } else { "not hold" }
        );
        assert_eq!(
            branch_on_both_engines(&format!("set hit to \"none\"\n{through_else_if}")),
            expected,
            "an `else if` over {value:?} must {} exactly as an `if` does",
            if holds { "hold" } else { "not hold" }
        );
    }
}

#[test]
fn edge_an_if_on_the_line_after_else_is_a_statement_of_the_else_block_not_an_arm() {
    // The same words, a different line: an `if` written on the line after `else`
    // is a statement of the else block and brings the `end` of its own, which is
    // the shape `else` + a nested `if` has always been written in
    // (`tests/suite.rb:39`, `examples/fizzbuzz.rb:10`). One `end` for the whole
    // construct here means the nested `if` never closes.
    let nested = "set hit to \"none\"\n\
                  if no then\n    set hit to \"a\"\n\
                  else\n    if yes then\n        set hit to \"inner\"\n    end\nend\n";
    assert_eq!(
        branch_on_both_engines(nested),
        "inner",
        "an `if` on its own line inside an `else` must still run as a nested \
         statement"
    );
    assert_parser_error_naming(
        &nested.replace(
            "        set hit to \"inner\"\n    end\n",
            "        set hit to \"inner\"\n",
        ),
        "end",
    );
    // Written as an arm instead, the same condition needs no `end` of its own.
    let arm = "set hit to \"none\"\n\
               if no then\n    set hit to \"a\"\n\
               else if yes then\n        set hit to \"inner\"\nend\n";
    assert_eq!(
        branch_on_both_engines(arm),
        "inner",
        "the same `if` condition written on the `else` line is an arm"
    );
}

/// A chain of `arms` `else if` arms after a matching `if`, with the last arm the
/// only one that holds.
fn chain_of(arms: usize) -> String {
    let mut source =
        String::from("set hit to \"none\"\nif 1 is 1 then\n    set hit to \"first\"\n");
    for arm in 0..arms {
        source.push_str(&format!(
            "else if 1 is {} then\n    set hit to \"arm-{arm}\"\n",
            arm + 2
        ));
    }
    source.push_str("end\n");
    source
}

/// `arms` `else if` arms after an `if` arm that does not hold, the last of them
/// holding — so the chain has to be walked to its end to reach it.
fn chain_to_the_last_arm(arms: usize) -> String {
    let mut source = String::from("set hit to \"none\"\nif no then\n    set hit to \"first\"\n");
    for arm in 0..arms {
        source.push_str(&format!("else if no then\n    set hit to \"arm-{arm}\"\n"));
    }
    source.push_str("else if yes then\n    set hit to \"last\"\nend\n");
    source
}

#[test]
fn edge_a_chain_within_the_block_budget_takes_the_first_arm_and_the_last() {
    // The parser reads a chain with a loop, but each arm desugars to an `if`
    // nested in the `else` of the arm before it, so a chain costs one level of
    // the block budget per arm. This pins that a chain as long as the budget
    // allows still parses and still picks the right arm.
    let arms = MAX_BLOCK_DEPTH - 2;
    assert_eq!(
        branch_taken_by(&chain_of(arms)),
        "first",
        "the `if` arm of a {arms}-arm chain must win"
    );
    assert_eq!(
        branch_on_both_engines(&chain_to_the_last_arm(arms)),
        "last",
        "the last arm of a chain as long as the budget allows must be reachable"
    );
}

#[test]
fn edge_a_chain_past_the_block_budget_is_refused_rather_than_recursing_until_it_breaks() {
    // The budget is what stops the VM recursing once per arm, and it has to
    // reach a chain written on one line: a chain that read as flat would
    // overflow the stack on a program nesting never could. The same nesting
    // written out longhand is refused at the same number of levels.
    let past_budget = MAX_BLOCK_DEPTH + 8;
    let error = lex_and_parse(&chain_of(past_budget))
        .expect_err("a chain deeper than the block budget must be refused");
    let message = match &error {
        Error::Parser(message, span) => {
            assert!(
                span.is_known(),
                "the refusal must carry a span, got {span:?}"
            );
            message.clone()
        }
        other => panic!("expected a Parser error, got {other:?}"),
    };
    assert_eq!(
        message,
        format!("Blocks nest more than {MAX_BLOCK_DEPTH} levels deep"),
        "a chain past the budget must be refused naming the budget"
    );
    let mut nested = String::new();
    for level in 0..past_budget {
        nested.push_str(&format!("if no then\n    say \"{level}\"\nelse\n"));
    }
    nested.push_str("say \"bottom\"\n");
    for _ in 0..past_budget {
        nested.push_str("end\n");
    }
    let nested_error =
        lex_and_parse(&nested).expect_err("the same nesting longhand must be refused");
    assert_eq!(
        parser_message(&nested_error),
        message,
        "a chain and the nesting it stands for must be refused alike"
    );
}

#[test]
fn edge_the_repl_block_depth_reads_a_chain_as_one_block_and_a_nested_if_as_two() {
    // The depth is what tells a REPL to read another line, so the two readings of
    // `else` + `if` have to count differently or a REPL either runs half a chain
    // or asks for an `end` the chain is never written with.
    let tokens = |source: &str| -> Vec<redblue::lexer::Token> {
        Lexer::tokenize(source).unwrap_or_else(|e| panic!("{source:?} should lex: {e}"))
    };
    let chain =
        "if x is 1 then\n    say \"a\"\nelse if x is 2 then\n    say \"b\"\nelse\n    say \"c\"\n";
    assert_eq!(
        open_block_depth(&tokens(chain)),
        1,
        "a chain waits for the one `end` that closes it"
    );
    assert_eq!(
        open_block_depth(&tokens(&format!("{chain}end\n"))),
        0,
        "that `end` closes the whole chain"
    );
    let nested = "if x is 1 then\n    say \"a\"\nelse\n    if x is 2 then\n        say \"b\"\n";
    assert_eq!(
        open_block_depth(&tokens(nested)),
        2,
        "an `if` on the line after `else` opens a block of its own, so the form \
         owes two `end`s"
    );
    assert_eq!(
        open_block_depth(&tokens(&format!("{nested}end\n"))),
        1,
        "the first `end` closes the inner `if` and leaves the outer one open"
    );
    assert_eq!(
        open_block_depth(&tokens(&format!("{nested}end\nend\n"))),
        0,
        "and the second closes the outer one"
    );
}

// --- malformed chains are refused, positioned, and never panic ------------

#[test]
fn edge_an_else_if_arm_with_no_then_names_the_missing_then() {
    assert_parser_error_naming(
        "if 1 is 2 then\n    say \"a\"\nelse if 2 is 3\n    say \"b\"\nend\n",
        "then",
    );
}

#[test]
fn edge_an_else_if_chain_with_no_end_names_the_missing_end() {
    let message = assert_parser_error_naming(
        "if 1 is 2 then\n    say \"a\"\nelse if 2 is 3 then\n    say \"b\"\nelse\n    say \"c\"\n",
        "end",
    );
    assert!(
        message.contains("Expected"),
        "the missing `end` must be named as an expectation, got {message:?}"
    );
}

#[test]
fn edge_an_else_with_nothing_after_it_is_refused_at_eof() {
    assert_parser_error_naming(
        "if 1 is 2 then\n    say \"a\"\nelse if 2 is 3 then\n    say \"b\"\nelse\n",
        "end",
    );
}

#[test]
fn edge_an_else_if_arm_that_starts_with_a_stray_token_is_refused_positioned() {
    let error = lex_and_parse("if 1 is 2 then\n    say \"a\"\nelse if then\n    say \"b\"\nend\n")
        .expect_err("`else if then` has no condition");
    match error {
        Error::Parser(message, span) => {
            assert!(
                span.is_known(),
                "a stray token after `else if` must carry a span, got {span:?}"
            );
            assert!(
                !message.is_empty(),
                "a stray token after `else if` must name what was expected"
            );
        }
        other => panic!("expected a Parser error, got {other:?}"),
    }
}

// --- the two engines agree on every position of the chain -----------------

#[test]
fn both_engines_agree_on_every_position_of_an_else_if_chain() {
    for (source, label, expected) in [
        (
            "if 1 is 1 then\n    set hit to \"a\"\nelse if 1 is 1 then\n    set hit to \"b\"\nend\n",
            "else if as the last arm",
            "a",
        ),
        (
            "if 1 is 2 then\n    set hit to \"a\"\nelse if 1 is 1 then\n    set hit to \"b\"\nelse\n    set hit to \"c\"\nend\n",
            "else if in the middle",
            "b",
        ),
        (
            "if 1 is 2 then\n    set hit to \"a\"\nelse if 1 is 3 then\n    set hit to \"b\"\nelse if 1 is 1 then\n    set hit to \"c\"\nelse\n    set hit to \"d\"\nend\n",
            "two chained else ifs",
            "c",
        ),
    ] {
        let source = format!("set hit to \"none\"\n{source}");
        assert_eq!(
            branch_taken_by(&source),
            expected,
            "{label}: the tree-walking VM took the wrong arm"
        );
        assert_eq!(
            branch_taken_by_bytecode(&source),
            expected,
            "{label}: the bytecode VM took the wrong arm"
        );
    }
}
