//! `break` and `skip` must leave a loop, not sit in it doing nothing.
//!
//! Both words were lexed, parsed and then evaluated to `Nothing`:
//! `src/vm.rs` carried a `// TODO: Implement proper control flow` on each of
//! them and no loop ever consulted them. So `for each i in [1, 2, 3]` with a
//! `break` when `i` is 2 printed 1, 2, 3 and exited 0 — a program that ran to
//! completion, reported success, and gave the wrong answer. That is what these
//! tests pin down.
//!
//! The Redblue-language versions of the same matrix live in
//! `tests/test_loop_control.rb`, and `tests/test_lists.rb` carries the two
//! `for each` cases that were corrected along with the implementation.

use redblue::{Error, Value, Vm};

/// The two `tests/test_lists.rb` cases this phase corrected. They were pinned
/// against the no-op: one asserted every iteration ran (`seen` is 4), the other
/// that a `break` did not truncate the loop (`total` is 10).
const CORRECTED_LIST_TESTS: [&str; 2] = [
    "lists: a skip inside a loop leaves its own iteration out",
    "lists: break inside a loop ends it at the value that broke",
];

/// Lexes and parses `source`.
#[track_caller]
fn parse(source: &str) -> redblue::parser::Program {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    redblue::parser::parse(tokens).expect("source should parse")
}

/// Runs `source` and returns the lines `say` printed.
#[track_caller]
fn say_lines(source: &str) -> Result<Vec<String>, Error> {
    let mut vm = Vm::new();
    let result = vm.run(&parse(source));
    let printed = vm.take_output();
    result.map(|_| printed)
}

/// Runs `source` and returns the value of its last statement.
#[track_caller]
fn last_value(source: &str) -> Result<Value, Error> {
    let mut vm = Vm::new();
    vm.run(&parse(source))
}

/// Runs `source` in a VM whose per-loop iteration cap is `max_iterations`.
#[track_caller]
fn run_capped(source: &str, max_iterations: usize) -> Result<Value, Error> {
    let mut vm = Vm::with_max_iterations(max_iterations);
    vm.run(&parse(source))
}

/// The message of a `RuntimeError`, ignoring the span it carries.
#[track_caller]
fn runtime_message(error: &Error) -> String {
    match error {
        Error::Runtime(message, _) => message.clone(),
        other => panic!("expected a RuntimeError, got {:?}", other),
    }
}

/// The reproduction: a `break` on the second element prints the first and stops.
/// Before the fix this printed 1, 2 and 3 and exited 0.
#[test]
fn break_leaves_a_for_each_loop() {
    let source = concat!(
        "for each i in [1, 2, 3]\n",
        "    if i is 2 then\n",
        "        break\n",
        "    end\n",
        "    say i\n",
        "end\n",
    );

    let printed = say_lines(source).expect("a break must not fail the program");

    assert_eq!(
        printed,
        vec!["1".to_string()],
        "a break at the second element must stop the loop after the first"
    );
}

/// The other half of the finding: `skip` prints the values either side of the one
/// it skipped. Before the fix this printed 1, 2 and 3.
#[test]
fn skip_goes_on_to_the_next_value() {
    let source = concat!(
        "for each i in [1, 2, 3]\n",
        "    if i is 2 then\n",
        "        skip\n",
        "    end\n",
        "    say i\n",
        "end\n",
    );

    let printed = say_lines(source).expect("a skip must not fail the program");

    assert_eq!(
        printed,
        vec!["1".to_string(), "3".to_string()],
        "a skip must drop its own turn and carry on with the next value"
    );
}

/// The same two statements in the other two loop forms. `while`'s condition is
/// re-read after a `skip`, exactly as it is after a turn that ran to its end.
#[test]
fn break_and_skip_work_in_repeat_and_while() {
    let cases: [(&str, &str, Value); 4] = [
        (
            "repeat/break",
            "set n to 0\nrepeat 5 times\n    set n to n + 1\n    if n is 3 then\n        break\n    end\nend",
            Value::Number(3.0),
        ),
        (
            "repeat/skip",
            "set n to 0\nrepeat 3 times\n    set n to n + 1\n    if n is 2 then\n        skip\n    end\nend",
            Value::Number(3.0),
        ),
        (
            "while/break",
            "set n to 0\nwhile n is not 9\n    set n to n + 1\n    if n is 4 then\n        break\n    end\nend",
            Value::Number(4.0),
        ),
        (
            "while/skip",
            "set n to 0\nwhile n is not 3\n    set n to n + 1\n    skip\nend",
            Value::Number(3.0),
        ),
    ];

    for (name, source, expected) in cases {
        // The trailing `n` is how the value is read: a `set` yields `nothing`,
        // so a program's value is the expression it ends on.
        let value =
            last_value(&format!("{source}\nn")).unwrap_or_else(|e| panic!("{name} failed: {e:?}"));
        assert_eq!(value, expected, "{name} gave the wrong answer");
    }
}

/// A `break` in the inner loop leaves the inner loop. The outer one goes on:
/// before the fix, the two loops ran to completion and the inner one only
/// appeared to because nothing could end it early.
#[test]
fn a_break_in_a_nested_loop_leaves_only_the_inner_one() {
    let source = concat!(
        "set outer to 0\n",
        "set inner to 0\n",
        "for each a in [1, 2, 3]\n",
        "    for each b in [1, 2, 3]\n",
        "        if b is 2 then\n",
        "            break\n",
        "        end\n",
        "        set inner to inner + 1\n",
        "    end\n",
        "    set outer to outer + 1\n",
        "end\n",
    );

    // Three outer turns, one inner turn each: the outer loop is not shortened
    // by a break that happened two loops down.
    for (name, expected) in [("outer", 3.0), ("inner", 3.0)] {
        let value = last_value(&format!("{source}{name}"))
            .unwrap_or_else(|e| panic!("{name} failed: {e:?}"));
        assert_eq!(
            value,
            Value::Number(expected),
            "the outer loop must keep going after the inner one is broken out of"
        );
    }
}

/// The same nesting for `skip`, where the inner loop must turn over but the
/// outer one must not lose a turn.
#[test]
fn a_skip_in_a_nested_loop_advances_only_the_inner_one() {
    let source = concat!(
        "set outer to 0\n",
        "set inner to 0\n",
        "for each a in [1, 2]\n",
        "    set outer to outer + 1\n",
        "    for each b in [1, 2]\n",
        "        set inner to inner + 1\n",
        "        skip\n",
        "    end\n",
        "end\n",
    );

    // Two outer turns of two inner turns is four: a skipped turn is still a
    // turn, so the inner loop is not shortened by it either.
    for (name, expected) in [("outer", 2.0), ("inner", 4.0)] {
        let value = last_value(&format!("{source}{name}"))
            .unwrap_or_else(|e| panic!("{name} failed: {e:?}"));
        assert_eq!(
            value,
            Value::Number(expected),
            "two outer turns of two inner turns is four, and a skip is still a turn"
        );
    }
}

/// A `break` written where there is no loop is a mistake in the program, and it
/// is reported as one. It used to be a no-op: the program ran on and exited 0.
#[test]
fn break_outside_a_loop_is_a_clean_runtime_error() {
    let error = last_value("break").expect_err("a break in no loop must not be a no-op");

    let message = runtime_message(&error);
    assert!(
        message.contains("break"),
        "the refusal must name the statement, got {:?}",
        message
    );
}

/// The same for `skip`, which is the word a reader is least likely to notice
/// doing nothing.
#[test]
fn skip_outside_a_loop_is_a_clean_runtime_error() {
    let error = last_value("say 1\nskip").expect_err("a skip in no loop must not be a no-op");

    let message = runtime_message(&error);
    assert!(
        message.contains("skip"),
        "the refusal must name the statement, got {:?}",
        message
    );
}

/// The refusal is catchable from inside the language, and the program carries on
/// afterwards: a `try` that handled it does not end the program.
#[test]
fn edge_a_refused_break_is_catchable_with_try_catch_error() {
    let source = concat!(
        "set caught to no\n",
        "set survived to no\n",
        "try\n",
        "    break\n",
        "catch error\n",
        "    set caught to yes\n",
        "end\n",
        "set survived to yes\n",
    );

    for (name, expected) in [
        ("caught", Value::YesNo(true)),
        ("survived", Value::YesNo(true)),
    ] {
        let value = last_value(&format!("{source}{name}"))
            .unwrap_or_else(|e| panic!("a caught refusal must not fail the program: {e:?}"));
        assert_eq!(value, expected, "{name} must show the failure was caught");
    }
}

/// A function body is not lexically inside the loop that calls it, so a `break`
/// there has no loop of its own. It is refused rather than reaching out and
/// ending the caller's iteration — and the caller's loop survives the refusal.
#[test]
fn edge_a_break_in_a_function_body_is_refused_and_the_caller_survives() {
    let source = concat!(
        "to escape()\n",
        "    break\n",
        "end\n",
        "set n to 0\n",
        "set caught to no\n",
        "repeat 2 times\n",
        "    set n to n + 1\n",
        "    try\n",
        "        escape()\n",
        "    catch error\n",
        "        set caught to yes\n",
        "    end\n",
        "end\n",
    );

    for (name, expected) in [("n", Value::Number(2.0)), ("caught", Value::YesNo(true))] {
        let value = last_value(&format!("{source}{name}")).unwrap_or_else(|e| {
            panic!("a refused break in a call must not end the caller's loop: {e:?}")
        });
        assert_eq!(
            value, expected,
            "a refused break must be caught, not obeyed"
        );
    }
}

/// A `break` whose whole body is itself: nothing else in the body can run, and
/// the loop must still end rather than spin.
#[test]
fn edge_break_as_the_only_statement_of_a_loop_body() {
    let source = "set n to 0\nfor each i in [1, 2, 3]\n    break\nend\nset n to 1\nn";

    let value = last_value(source).expect("a one-statement body should run");

    assert_eq!(
        value,
        Value::Number(1.0),
        "the loop must end and the statement after it must run"
    );
}

/// `break` first and `break` last: the loop must end in both, and must end
/// before the statements that follow it.
#[test]
fn edge_break_as_the_first_and_last_statement_of_a_body() {
    let first =
        last_value("set n to 0\nrepeat 3 times\n    break\n    set n to n + 1\nend\nset n to 7\nn")
            .expect("a break first should run");
    let last =
        last_value("set n to 0\nrepeat 3 times\n    set n to n + 1\n    break\nend\nset n to 7\nn")
            .expect("a break last should run");

    assert_eq!(
        first,
        Value::Number(7.0),
        "a break first must leave before the rest of the body"
    );
    assert_eq!(
        last,
        Value::Number(7.0),
        "a break last must leave before the next turn"
    );
}

/// Two blocks of `if` between the loop and the `break`: the signal has to pass
/// through statements that know nothing about loops.
#[test]
fn edge_break_two_blocks_deep_inside_a_conditional() {
    let source = concat!(
        "set n to 0\n",
        "for each i in [1, 2, 3]\n",
        "    if i is 3 then\n",
        "        if n is 2 then\n",
        "            break\n",
        "        end\n",
        "    end\n",
        "    set n to n + 1\n",
        "end\n",
    );

    let value = last_value(&format!("{source}n")).expect("a nested conditional should run");

    assert_eq!(
        value,
        Value::Number(2.0),
        "the third value must never be reached"
    );
}

/// A `skip` on the last value of a list is the boundary of the statement: it
/// ends a turn that is the last one, and the loop still ends normally.
#[test]
fn edge_skip_on_the_final_iteration_of_a_for_each() {
    let source = concat!(
        "set seen to 0\n",
        "for each i in [1, 2, 3]\n",
        "    if i is 3 then\n",
        "        skip\n",
        "    end\n",
        "    set seen to seen + i\n",
        "end\n",
    );

    let value = last_value(&format!("{source}seen")).expect("a skip on the last value should run");

    assert_eq!(
        value,
        Value::Number(3.0),
        "1 + 2 only: the skipped turn is the last one and it adds nothing"
    );
}

/// The iteration budget: a `break` must stop the loop rather than run it to the
/// cap, so a loop with a hundred turns and a cap of two succeeds. This is the
/// accounting rule — a broken-out-of turn is charged, and no more.
#[test]
fn edge_a_break_stops_the_loop_before_the_iteration_cap() {
    let source = concat!(
        "set n to 0\n",
        "for each i in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]\n",
        "    set n to n + 1\n",
        "    break\n",
        "end\n",
    );

    run_capped(source, 2).unwrap_or_else(|error| {
        panic!(
            "a break must end the loop instead of spending the whole cap: {:?}",
            error
        )
    });
}

/// The same rule from the other side: a `skip` is a turn like any other, so
/// exactly the cap is allowed and one past it is a clean error. A `skip` that
/// were free would let a loop run forever without ever reaching its own limit.
#[test]
fn edge_a_skip_is_charged_as_one_iteration() {
    let source = "set n to 0\nrepeat 5 times\n    set n to n + 1\n    skip\nend";

    run_capped(source, 5).unwrap_or_else(|error| {
        panic!("five skipped turns are five iterations and must be allowed: {error:?}")
    });

    let error = run_capped(source, 4).expect_err("six turns are one past the cap");
    assert!(
        matches!(error, Error::Runtime(..)),
        "the cap must be a clean RuntimeError, got {:?}",
        error
    );
}

/// The same rule for a `while`, whose turns are counted by a statement the
/// bytecode VM does not have: a `while` re-reads its condition rather than
/// drawing the next value from a sequence, so there is no `STORE` at its `top` to
/// charge the turn with. `SKIP` jumps to that `top` over the backward jump that
/// does the charging there, which left a `while` that skipped every turn
/// spending nothing at all — the cap the language publishes was not what stopped
/// such a program, the ten-million-statement step budget was.
///
/// The test says it in the language's own terms: the loop counts its own turns
/// and the `catch` around it reports them, so what the cap allows is a value
/// rather than a message.
#[test]
fn edge_a_skip_in_a_while_is_charged_as_one_iteration() {
    let source = concat!(
        "try\n",
        "    set turns to 0\n",
        "    while turns is not 100\n",
        "        set turns to turns + 1\n",
        "        skip\n",
        "    end\n",
        "catch error\n",
        "    set stopped to turns\n",
        "end\n",
    );

    for cap in [1, 3, 4] {
        let value = run_capped(&format!("{source}stopped"), cap)
            .unwrap_or_else(|error| panic!("a caught cap must not fail the program: {error:?}"));
        assert_eq!(
            value,
            Value::Number(cap as f64),
            "a cap of {cap} must be {cap} skipped turns of a while loop, and no more",
        );
    }
}

/// The other side of the same rule, and the shape the bytecode VM had to be
/// brought back to: a `break` leaves the loop, so a loop that breaks on its first
/// turn is one turn however large the cap is. This is on the tree-walking VM,
/// where the rule was already right; `edge_a_break_stops_the_loop_at_the_same_turn_on_both_vms`
/// in `tests/bytecode_vm_test.rs` is the same case across both engines.
#[test]
fn edge_a_break_on_the_first_turn_of_a_while_is_one_turn() {
    let source = "set n to 0\nwhile n is not 100\n    set n to n + 1\n    break\nend\nn";

    let value = run_capped(source, 1).unwrap_or_else(|error| {
        panic!("a break on the first turn must be inside a cap of one: {error:?}")
    });
    assert_eq!(
        value,
        Value::Number(1.0),
        "the turn that broke is the only one"
    );
}

/// An empty list has no turns at all, so nothing is skipped or broken out of.
/// The statements after the loop still run.
#[test]
fn edge_break_and_skip_in_a_loop_over_an_empty_list() {
    let value = last_value(
        "set n to 0\nfor each i in []\n    set n to n + 1\n    skip\nend\nset n to 5\nn",
    )
    .expect("a loop over no values should run");

    assert_eq!(
        value,
        Value::Number(5.0),
        "an empty loop leaves nothing to skip and must not run its body"
    );
}

/// The two `tests/test_lists.rb` cases this phase corrected are still there and
/// still pass. They used to assert the no-op's answer — `seen` is 4 and `total`
/// is 10 — so a silent return to it would fail the suite as loudly as the
/// original defect did.
#[test]
fn the_corrected_list_tests_still_exist_and_pass() {
    let path = format!("{}/tests/test_lists.rb", env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(&path).expect("tests/test_lists.rb should be readable");

    for name in CORRECTED_LIST_TESTS {
        assert!(
            source.contains(&format!("test \"{name}\"")),
            "the corrected test {:?} must still be in tests/test_lists.rb",
            name
        );
    }
    for expectation in ["expect seen to be 3", "expect total to be 3"] {
        assert!(
            source.contains(expectation),
            "{:?} must be what the corrected tests assert",
            expectation
        );
    }

    let results = redblue::testing::run_test_file(&path).expect("tests/test_lists.rb should run");

    assert_eq!(
        results.failed, 0,
        "every test in tests/test_lists.rb must pass, failures: {:?}",
        results.errors
    );
    assert!(
        results.passed >= CORRECTED_LIST_TESTS.len(),
        "tests/test_lists.rb reported {} passes, which cannot cover both corrected tests",
        results.passed
    );
}

/// A list of one value: the smallest loop there is. A `break` and a `skip` both
/// end its only turn, and neither is an error.
#[test]
fn edge_singleton_list_runs_one_turn_and_ends() {
    let broke = last_value("set n to 0\nfor each i in [7]\n    set n to i\n    break\nend\nn")
        .expect("a break in a one-element loop should run");
    let skipped = last_value("set n to 0\nfor each i in [7]\n    skip\n    set n to i\nend\nn")
        .expect("a skip in a one-element loop should run");

    assert_eq!(
        broke,
        Value::Number(7.0),
        "the only turn ran to the break, so its value was assigned"
    );
    assert_eq!(
        skipped,
        Value::Number(0.0),
        "the only turn was skipped, so nothing was assigned"
    );
}

/// A loop whose iterable is not a list is not a loop, so its body never runs and
/// the `break` in it is never reached — no error either way. This is the
/// pre-existing behaviour a working `break` must not turn into a failure.
#[test]
fn edge_break_in_a_loop_over_a_non_list_never_runs() {
    for (name, source) in [
        (
            "a non-list iterable",
            "set n to 0\nfor each i in 5\n    break\nend\nset n to 1\nn",
        ),
        (
            "a non-numeric count",
            "set n to 0\nrepeat \"five\" times\n    break\nend\nset n to 1\nn",
        ),
    ] {
        let value = run_capped(source, 5).unwrap_or_else(|error| panic!("{name}: {:?}", error));
        assert_eq!(value, Value::Number(1.0), "{name} must not run the body");
    }
}

/// The numeric boundary: a count beyond any representable number feeds a loop
/// that can only be stopped by its iteration cap. A `break` on the first turn
/// means the cap is never reached, so the program finishes — the boundary is
/// still bounded by the same arithmetic, and a `break` is a legitimate way out.
#[test]
fn edge_a_break_stops_a_loop_whose_count_is_beyond_i64() {
    let source = "set n to 0\nrepeat 99999999999999999999 times\n    set n to 1\n    break\nend\nn";

    let value = run_capped(source, 5).unwrap_or_else(|error| {
        panic!(
            "a break on the first turn must not wait for the count to run out: {:?}",
            error
        )
    });

    assert_eq!(value, Value::Number(1.0), "exactly one turn ran");
}

/// Text values, however they are spelled: a `break` and a `skip` do not look at
/// what the loop variable holds, so emoji, CJK and combining marks pass through
/// them unchanged.
#[test]
fn edge_break_and_skip_over_unicode_values() {
    let source = concat!(
        "set seen to \"\"\n",
        "for each c in [\"héllo\", \"日本語\", \"🙂\"]\n",
        "    if c is \"héllo\" then\n",
        "        skip\n",
        "    end\n",
        "    set seen to seen + c\n",
        "    break\n",
        "end\n",
    );

    let value = last_value(&format!("{source}seen")).expect("unicode values should run");

    assert_eq!(
        value,
        Value::Text("日本語".to_string()),
        "the first value was skipped and the second ended the loop"
    );
}

/// A block that looks like one but is not a loop: an `object` body. The
/// statements in it are run once, when the type is declared, so a `break` there
/// is a `break` in no loop and is refused rather than ending the declaration.
#[test]
fn edge_break_in_an_object_body_is_refused() {
    let source = concat!(
        "object Broken\n",
        "    has n\n",
        "    break\n",
        "end\n",
        "set n to 1\n",
        "n\n",
    );

    let error = last_value(source).expect_err("a break in an object body has no loop to leave");

    let message = runtime_message(&error);
    assert!(
        message.contains("break"),
        "the refusal must name the statement, got {:?}",
        message
    );
}

/// A `break` has to end the `if` branch it is written in, not only the loop.
///
/// The branch ran its statements one at a time and never looked at the signal,
/// so `if i is 2 then break say "leaked" end` printed the line after the break:
/// the program left the loop having done everything the `break` was supposed to
/// stop it doing. Both branches are pinned, because either one leaking would be
/// the same defect with a different spelling.
#[test]
fn a_break_in_an_if_branch_does_not_run_the_statements_after_it() {
    // Each turn prints its value first, so a line that belongs to a turn the
    // break was supposed to stop is visible in the output rather than inferred.
    let then_branch = concat!(
        "for each i in [1, 2, 3]\n",
        "    say i\n",
        "    if i is 2 then\n",
        "        break\n",
        "        say \"leaked from the then branch\"\n",
        "    end\n",
        "    say \"the rest of the turn\"\n",
        "end\n",
        "say \"after the loop\"\n",
    );
    let else_branch = concat!(
        "for each i in [1, 2, 3]\n",
        "    say i\n",
        "    if i is 2 then\n",
        "        say \"the then branch\"\n",
        "        say \"the rest of the turn\"\n",
        "    else\n",
        "        break\n",
        "        say \"leaked from the else branch\"\n",
        "        say \"leaked from the else branch again\"\n",
        "    end\n",
        "    say \"the rest of the turn\"\n",
        "end\n",
        "say \"after the loop\"\n",
    );

    for (name, source, expected) in [
        (
            "then",
            then_branch,
            vec![
                "1".to_string(),
                "the rest of the turn".to_string(),
                "2".to_string(),
                "after the loop".to_string(),
            ],
        ),
        (
            "else",
            else_branch,
            vec!["1".to_string(), "after the loop".to_string()],
        ),
    ] {
        let printed =
            say_lines(source).unwrap_or_else(|e| panic!("the {name} branch failed: {e:?}"));
        assert_eq!(
            printed, expected,
            "a break in the {name} branch must end the branch, not only the loop"
        );
    }
}

/// The same for `skip`, where the statement after it is the one that would have
/// printed the value the skip skipped — so this is where a leaked statement is
/// visible as a duplicated or missing line.
#[test]
fn a_skip_in_an_if_branch_does_not_run_the_statements_after_it() {
    let source = concat!(
        "for each i in [1, 2, 3]\n",
        "    if i is 2 then\n",
        "        skip\n",
        "        say \"leaked from the branch\"\n",
        "    end\n",
        "    say i\n",
        "end\n",
    );

    let printed = say_lines(source).expect("a skip in a branch should run");

    assert_eq!(
        printed,
        vec!["1".to_string(), "3".to_string()],
        "the statements after a skip in a branch are part of the turn it dropped"
    );
}

/// The same defect through a `try`: the protected code ran statement by statement
/// without looking at the signal, so a `break` inside it neither ended the block
/// nor reached the loop until the whole block had run.
///
/// The `finally` still runs — an abrupt exit from a protected region is not a
/// failure — and the `catch` does not, which is the shape
/// `loop control: a break inside a try still leaves the loop and runs the finally`
/// in `tests/test_loop_control.rb` already claims for the simple case.
#[test]
fn a_break_in_a_try_body_does_not_run_the_statements_after_it() {
    let source = concat!(
        "set n to 0\n",
        "repeat 3 times\n",
        "    set n to n + 1\n",
        "    try\n",
        "        break\n",
        "        say \"leaked inside the try\"\n",
        "    catch error\n",
        "        say \"leaked into the catch\"\n",
        "    finally\n",
        "        say \"the finally ran\"\n",
        "    end\n",
        "    say \"leaked after the try\"\n",
        "end\n",
        "say \"after the loop\"\n",
    );

    let printed = say_lines(source).expect("a break in a try body should run");

    assert_eq!(
        printed,
        vec!["the finally ran".to_string(), "after the loop".to_string()],
        "a break ends the try body and the loop, and only the finally runs on the way out"
    );
}

/// A conditional inside the `try` body, which is the shape the phase report was
/// written against: the `break` is in the `if`, and the `say` after the `if` is
/// inside the protected code. The turn that broke must print nothing else.
#[test]
fn a_break_in_a_conditional_inside_a_try_body_ends_that_turn() {
    let source = concat!(
        "set n to 0\n",
        "repeat 3 times\n",
        "    set n to n + 1\n",
        "    try\n",
        "        if n is 2 then\n",
        "            break\n",
        "        end\n",
        "        say \"leaked inside the try\"\n",
        "    finally\n",
        "        say \"the finally ran\"\n",
        "    end\n",
        "end\n",
    );

    let printed = say_lines(source).expect("a break in a try body should run");

    assert_eq!(
        printed,
        vec![
            "leaked inside the try".to_string(),
            "the finally ran".to_string(),
            "the finally ran".to_string(),
        ],
        "only the first turn runs the rest of its body; the second breaks and the third never comes"
    );
    assert_eq!(
        last_value(&format!("{source}n\n")).unwrap_or_else(|e| panic!("n: {e:?}")),
        Value::Number(2.0),
        "the loop must end on the turn that broke"
    );
}

/// The `catch` body is a block too: a `break` in it ends the loop, and the
/// `finally` of the `try` it belongs to runs on the way out. Nothing after the
/// `try` runs on that turn either.
#[test]
fn a_break_in_a_catch_body_leaves_the_loop_and_runs_the_finally() {
    let source = concat!(
        "set n to 0\n",
        "repeat 3 times\n",
        "    set n to n + 1\n",
        "    try\n",
        "        set x to 1 + \"one\"\n",
        "    catch error\n",
        "        if n is 2 then\n",
        "            break\n",
        "        end\n",
        "        say \"caught\"\n",
        "    finally\n",
        "        say \"the finally ran\"\n",
        "    end\n",
        "    say \"leaked after the try\"\n",
        "end\n",
    );

    let printed = say_lines(source).expect("a break in a catch body should run");

    assert_eq!(
        printed,
        vec![
            "caught".to_string(),
            "the finally ran".to_string(),
            "leaked after the try".to_string(),
            "the finally ran".to_string(),
        ],
        "the first turn is caught and carries on; the second breaks, and its finally is still owed"
    );
    assert_eq!(
        last_value(&format!("{source}n\n")).unwrap_or_else(|e| panic!("n: {e:?}")),
        Value::Number(2.0),
        "the loop must end on the turn whose catch broke"
    );
}

/// A turn that *fails* is unwinding out of the loop on its own, so a signal it
/// raised before failing has no loop left to act on.
///
/// The failing turn is `try break finally <a failure> end`: the `break` raises
/// the signal and the `finally` then fails, so the failure — not the `break` —
/// is what leaves the loop. Before the fix the signal stayed pending in the VM,
/// and the *next* loop in the program found it waiting: its first turn was
/// consumed by a `break` it never contained, so it ran one turn instead of
/// three.
#[test]
fn a_failed_turn_leaves_no_signal_for_the_next_loop() {
    let program = concat!(
        "set caught to no\n",
        "try\n",
        "    repeat 3 times\n",
        "        try\n",
        "            break\n",
        "        finally\n",
        "            expect 1 to be 2\n",
        "        end\n",
        "    end\n",
        "catch error\n",
        "    set caught to yes\n",
        "end\n",
    );
    let second_loop = concat!(
        "set n to 0\n",
        "repeat 3 times\n",
        "    set n to n + 1\n",
        "end\n",
    );

    assert_eq!(
        last_value(&format!("{program}caught\n")).expect("the failing finally should be caught"),
        Value::YesNo(true),
        "the failing finally must be caught and the program must go on to the next loop"
    );
    assert_eq!(
        last_value(&format!("{program}{second_loop}n\n")).expect("the second loop should run"),
        Value::Number(3.0),
        "a signal left pending by a failed turn must not shorten the next loop"
    );
}

/// The same failing turn, seen from the scope stack rather than the signal: the
/// per-turn scope is released on the failing path too.
///
/// A scope left behind keeps the loop's variable bound for the rest of the
/// program, so a name the program bound outside the loop reads the loop's last
/// value instead of its own.
#[test]
fn a_failed_turn_does_not_leak_the_scope_of_its_loop_variable() {
    let source = concat!(
        "set i to \"outer\"\n",
        "try\n",
        "    repeat 3 times\n",
        "        for each i in [1, 2]\n",
        "            try\n",
        "                break\n",
        "            finally\n",
        "                expect 1 to be 2\n",
        "            end\n",
        "        end\n",
        "    end\n",
        "catch error\n",
        "end\n",
        "i\n",
    );

    assert_eq!(
        last_value(source).expect("the failing loop should be caught"),
        Value::Text("outer".to_string()),
        "the loop variable's scope must not survive the turn that failed"
    );
}

/// A `catch` body's scope is released whether the body ran to its end, failed, or
/// was left by a `break` — so the name the catch binds does not outlive it. This
/// is the invariant the failing path above depends on: a leaked catch scope would
/// shadow the program's own binding of the same name.
#[test]
fn a_catch_body_does_not_leave_its_scope_behind() {
    let caught_then_left = concat!(
        "set error to \"outer\"\n",
        "set n to 0\n",
        "repeat 2 times\n",
        "    set n to n + 1\n",
        "    try\n",
        "        set x to 1 + \"one\"\n",
        "    catch error\n",
        "        set error to \"caught\"\n",
        "        break\n",
        "    end\n",
        "end\n",
        "error\n",
    );
    let caught_and_carried_on =
        "set error to \"outer\"\ntry\n    set x to 1 + \"one\"\ncatch error\n    set error to \"caught\"\nend\nerror\n";

    assert_eq!(
        last_value(caught_then_left).expect("a caught break should run"),
        Value::Text("outer".to_string()),
        "a break must not leave the catch's binding alive in the enclosing block"
    );
    assert_eq!(
        last_value(caught_and_carried_on).expect("a caught failure should run"),
        Value::Text("outer".to_string()),
        "a catch that ran to its end must not leave its binding alive either"
    );
}

/// A `finally` is owed on the way out of a `break`, so every statement of it
/// runs — the cleanup is not half a cleanup.
///
/// This is the rule that decides where the pending signal lives. Held in place
/// while the `finally` runs, it stops the block after its first statement,
/// because that is what "stop at the signal" means; so the signal is taken out
/// for the duration and put back afterwards. The bytecode VM, which jumps rather
/// than unwinding, runs all of them — and `rb vm` and `rb run` must print the
/// same thing.
#[test]
fn a_finally_runs_every_statement_on_the_way_out_of_a_break() {
    let source = concat!(
        "set cleaned to 0\n",
        "repeat 3 times\n",
        "    try\n",
        "        break\n",
        "    finally\n",
        "        set cleaned to cleaned + 1\n",
        "        set cleaned to cleaned + 10\n",
        "    end\n",
        "end\n",
        "cleaned\n",
    );

    assert_eq!(
        last_value(source).expect("a break out of a try body should run"),
        Value::Number(11.0),
        "both statements of the finally are owed, not just the first"
    );
}

/// And the `finally` is a block like any other: a `break` in it ends the
/// statements after it in the same block, and still names the loop that is being
/// left.
#[test]
fn a_break_in_a_finally_ends_that_block_and_still_ends_the_loop() {
    let program = concat!(
        "set log to \"\"\n",
        "set n to 0\n",
        "repeat 3 times\n",
        "    set n to n + 1\n",
        "    try\n",
        "        break\n",
        "    finally\n",
        "        set log to log + \"a\"\n",
        "        break\n",
        "        set log to log + \"b\"\n",
        "    end\n",
        "    set log to log + \"after the try\"\n",
        "end\n",
    );

    assert_eq!(
        last_value(&format!("{program}log\n")).expect("a break in a finally should run"),
        Value::Text("a".to_string()),
        "the finally stops at its own break, and nothing after the try runs either"
    );
    assert_eq!(
        last_value(&format!("{program}n\n")).unwrap_or_else(|e| panic!("n: {e:?}")),
        Value::Number(1.0),
        "the loop still ends on the turn that broke"
    );
}

/// A `catch` body is a block, and a block written inside a loop is inside that
/// loop: a `break` in one names it and leaves it. The `finally` the `catch`
/// belongs to is still owed on the way out, and the statement after the `try` is
/// part of the turn the `break` stopped.
///
/// This is the tree-walking VM's answer, and the bytecode VM has to give the same
/// one — a `catch` body is a frame of its own there, and finding a loop only
/// within the frame the instruction runs in made it refuse. The same program in
/// `tests/test_loop_control.rb` and in the differential corpus runs on both.
#[test]
fn a_break_in_a_catch_body_leaves_the_loop_the_catch_is_written_in() {
    let program = concat!(
        "set log to \"\"\n",
        "set n to 0\n",
        "repeat 3 times\n",
        "    set n to n + 1\n",
        "    try\n",
        "        set bad to 1 + \"one\"\n",
        "    catch error\n",
        "        if n is 2 then\n",
        "            break\n",
        "        end\n",
        "        set log to log + \"c\"\n",
        "    finally\n",
        "        set log to log + \"f\"\n",
        "    end\n",
        "    set log to log + \".\"\n",
        "end\n",
    );

    assert_eq!(
        last_value(&format!("{program}log\n")).expect("a break in a catch should run"),
        Value::Text("cf.f".to_string()),
        "the turn that broke still owed its finally, and nothing after the try ran"
    );
    assert_eq!(
        last_value(&format!("{program}n\n")).unwrap_or_else(|e| panic!("n: {e:?}")),
        Value::Number(2.0),
        "the loop ended on the turn whose catch broke"
    );
}

/// The same for `skip`: it advances the loop the `catch` body is written in, and
/// the rest of the turn — the statement after the `try` included — is abandoned.
#[test]
fn a_skip_in_a_catch_body_advances_the_loop_the_catch_is_written_in() {
    let source = concat!(
        "set seen to \"\"\n",
        "for each i in [\"one\", \"two\", \"three\"]\n",
        "    try\n",
        "        set bad to 1 + \"one\"\n",
        "    catch error\n",
        "        if i is \"two\" then\n",
        "            skip\n",
        "        end\n",
        "        set seen to seen + i\n",
        "    end\n",
        "    set seen to seen + \"!\"\n",
        "end\n",
        "seen\n",
    );

    assert_eq!(
        last_value(source).expect("a skip in a catch should run"),
        Value::Text("one!three!".to_string()),
        "the skipped turn drew the next value and ran none of the rest of its body"
    );
}

/// A `test` body is a block too, so a `break` written in one inside a loop leaves
/// that loop rather than being refused for want of one.
#[test]
fn a_break_in_a_test_body_leaves_the_loop_the_test_is_written_in() {
    let program = concat!(
        "set log to \"\"\n",
        "set n to 0\n",
        "repeat 3 times\n",
        "    set n to n + 1\n",
        "    test \"a test written inside a loop\"\n",
        "        if n is 2 then\n",
        "            break\n",
        "        end\n",
        "        set log to log + \"t\"\n",
        "    end\n",
        "    set log to log + \".\"\n",
        "end\n",
    );

    assert_eq!(
        last_value(&format!("{program}log\n")).expect("a break in a test should run"),
        Value::Text("t.".to_string()),
        "the first turn ran to its end; the second broke and nothing after the test ran"
    );
    assert_eq!(
        last_value(&format!("{program}n\n")).unwrap_or_else(|e| panic!("n: {e:?}")),
        Value::Number(2.0),
        "the loop ended on the turn whose test broke"
    );
}

/// And an `object` body: its statements after the field and method declarations
/// are run where they were written, so a `break` in one of them names the loop
/// around the declaration.
#[test]
fn a_break_in_an_object_body_leaves_the_loop_the_declaration_is_written_in() {
    let source = concat!(
        "set log to \"\"\n",
        "for each x in [\"a\", \"b\"]\n",
        "    set log to log + x\n",
        "    object Once\n",
        "        has a\n",
        "        break\n",
        "        set log to log + \"unreachable\"\n",
        "    end\n",
        "    set log to log + \".\"\n",
        "end\n",
        "log\n",
    );

    assert_eq!(
        last_value(source).expect("a break in an object body should run"),
        Value::Text("a".to_string()),
        "the declaration's body stopped at its break and the loop ended with it"
    );
}

/// `skip` in a `test` body: the block above has a frame of its own, so this is
/// the second statement through the same ownership lookup `break` takes there.
///
/// Turn two is the one that skips, so the body statement never runs on it and
/// neither does the `.` after the test — `t.t.`, not `t.tt.` and not `t.tt.`
/// with a turn missing.
#[test]
fn a_skip_in_a_test_body_advances_the_loop_the_test_is_written_in() {
    let program = concat!(
        "set log to \"\"\n",
        "set n to 0\n",
        "repeat 3 times\n",
        "    set n to n + 1\n",
        "    test \"a test written inside a loop\"\n",
        "        if n is 2 then\n",
        "            skip\n",
        "        end\n",
        "        set log to log + \"t\"\n",
        "    end\n",
        "    set log to log + \".\"\n",
        "end\n",
    );

    assert_eq!(
        last_value(&format!("{program}log\n")).expect("a skip in a test should run"),
        Value::Text("t.t.".to_string()),
        "the skipped turn ran neither the rest of the test body nor the rest of the turn"
    );
    assert_eq!(
        last_value(&format!("{program}n\n")).unwrap_or_else(|e| panic!("n: {e:?}")),
        Value::Number(3.0),
        "the loop ran all three of its turns: a skip is still a turn"
    );
}

/// `skip` in an `object` body, which is the last of the three blocks with a frame
/// of its own.
///
/// The declaration is made under a guard because a name cannot be declared twice
/// and a `skip` carries the loop on to the next turn rather than ending it — so
/// the body is entered on the first turn only, skips there, and the turns after it
/// find nothing left to declare. `bc`: the `a` turn added nothing at all, and the
/// `o` after the `skip` in the body never ran. `oabc` is what a `skip` that only
/// set the signal would leave, and `O is already declared` is what a `skip` that
/// did not reach the loop would leave instead.
#[test]
fn a_skip_in_an_object_body_advances_the_loop_the_declaration_is_written_in() {
    let source = concat!(
        "set log to \"\"\n",
        "set declared to no\n",
        "for each x in [\"a\", \"b\", \"c\"]\n",
        "    if declared is no then\n",
        "        set declared to yes\n",
        "        object Once\n",
        "            has a\n",
        "            if x is \"a\" then\n",
        "                skip\n",
        "            end\n",
        "            set log to log + \"o\"\n",
        "        end\n",
        "    end\n",
        "    set log to log + x\n",
        "end\n",
        "log\n",
    );

    assert_eq!(
        last_value(source).expect("a skip in an object body should run"),
        Value::Text("bc".to_string()),
        "the declaration's body stopped at its skip and the loop went on to its next turn"
    );
}

/// The finding itself: the two blocks with a frame of their own that had a
/// `break` test and no `skip` one, through **both** VMs.
///
/// A `skip` is a different instruction from a `break` and reaches the same
/// ownership lookup by a different path, so a `break` test says nothing about
/// it. Each program prints its own answer, so a `skip` that arrived as a signal
/// the block ignored would show up as a `leaked` line.
#[test]
fn edge_both_vms_advance_the_loop_for_a_skip_in_a_test_or_an_object_body() {
    let programs = [
        (
            "skip in a test body",
            concat!(
                "set log to \"\"\n",
                "set n to 0\n",
                "repeat 3 times\n",
                "    set n to n + 1\n",
                "    set log to log + \".\"\n",
                "    test \"a test written inside a loop\"\n",
                "        if n is 2 then\n",
                "            skip\n",
                "        end\n",
                "        say \"leaked\"\n",
                "    end\n",
                "end\n",
                "say log\n",
                "say n\n",
            ),
            vec!["leaked", "leaked", "...", "3"],
        ),
        (
            "skip in an object body",
            concat!(
                "set log to \"\"\n",
                "set declared to no\n",
                "for each x in [\"a\", \"b\", \"c\"]\n",
                "    if declared is no then\n",
                "        set declared to yes\n",
                "        object Once\n",
                "            has a\n",
                "            if x is \"a\" then\n",
                "                skip\n",
                "            end\n",
                "            say \"leaked\"\n",
                "        end\n",
                "    end\n",
                "    set log to log + x\n",
                "end\n",
                "say log\n",
            ),
            vec!["bc"],
        ),
    ];
    for (name, source, expected) in programs {
        let program = parse(source);
        let tree = say_lines_of(&program).unwrap_or_else(|e| panic!("{name} on the tree: {e:?}"));
        let byte = bytecode_say_lines(&program)
            .unwrap_or_else(|e| panic!("{name} on the bytecode: {e:?}"));
        assert_eq!(tree, byte, "the two VMs disagree about {name}\n{source}");
        assert_eq!(
            tree,
            expected
                .iter()
                .map(|line| line.to_string())
                .collect::<Vec<String>>(),
            "{name} printed the wrong lines\n{source}"
        );
    }
}

/// The exemption the four above need. A function body is not lexically inside the
/// loop that calls it, so a `break` in a block of the function's own is in no
/// loop: it is refused, and the caller's loop survives the refusal.
#[test]
fn edge_a_break_in_a_block_in_a_function_body_is_still_refused() {
    let source = concat!(
        "to escape()\n",
        "    test \"a test inside a function inside a loop\"\n",
        "        break\n",
        "    end\n",
        "end\n",
        "set caught to no\n",
        "set n to 0\n",
        "repeat 2 times\n",
        "    set n to n + 1\n",
        "    try\n",
        "        escape()\n",
        "    catch error\n",
        "        set caught to yes\n",
        "    end\n",
        "end\n",
        "n\n",
    );

    assert_eq!(
        last_value(source).expect("the refusal should be catchable"),
        Value::Number(2.0),
        "the loop that called the function runs to its end rather than being cut short"
    );
}

/// The loop's variable is still bound while a `finally` the signal passed through
/// runs, because the turn the `break` stopped has not ended until it has. The
/// turn's scope is what puts the outer binding back, and it is popped after the
/// `finally` rather than before it.
#[test]
fn a_finally_a_catch_break_passed_through_still_reads_the_loop_variable() {
    let source = concat!(
        "set seen to \"none\"\n",
        "set i to \"outer\"\n",
        "for each i in [1, 2]\n",
        "    try\n",
        "        set bad to 1 + \"one\"\n",
        "    catch error\n",
        "        break\n",
        "    finally\n",
        "        set seen to i\n",
        "    end\n",
        "end\n",
    );

    assert_eq!(
        last_value(&format!("{source}seen\n")).expect("the finally should run"),
        Value::Number(1.0),
        "the cleanup sees the turn's own value of the loop's variable"
    );
    assert_eq!(
        last_value(&format!("{source}i\n")).unwrap_or_else(|e| panic!("i: {e:?}")),
        Value::Text("outer".to_string()),
        "and the program's own binding is back once the turn has ended"
    );
}

/// The expression a source snippet parses to, for building an AST the parser
/// cannot reach.
///
/// `parse_for` accepts `for each x in <expression>` and nothing else, so
/// `Statement::ForRange` — which `SPEC.md` and `docs/GRAMMAR.md` both write a
/// `for each x from a to b [by s]` rule for — is carried through the analyzer,
/// the compiler, the formatter and the linter but cannot be built from source.
/// This phase routes it through `run_iteration` like every other loop form, and
/// this is the only door into it, so the tests below go through here.
#[track_caller]
fn expression(source: &str) -> redblue::parser::Expr {
    let program = parse(source);
    match program.statements.into_iter().next() {
        Some(redblue::parser::Stmt {
            statement: redblue::parser::Statement::Expr(expr),
            ..
        }) => expr,
        other => panic!(
            "`{}` should parse to one expression, got {:?}",
            source, other
        ),
    }
}

/// `for each <variable> from <start> to <end> [by <step>]` with `body`, built as
/// the AST the parser does not produce, with `trailer` after it.
#[track_caller]
fn range_loop(
    variable: &str,
    start: &str,
    end: &str,
    step: Option<&str>,
    body: &str,
    trailer: &str,
) -> redblue::parser::Program {
    use redblue::parser::{Statement, Stmt};
    let mut program = parse(trailer);
    program.statements.insert(
        0,
        Stmt {
            span: redblue::Span::new(1, 1),
            statement: Statement::ForRange {
                variable: variable.to_string(),
                start: expression(start),
                end: expression(end),
                step: step.map(expression),
                body: parse(body).statements,
            },
        },
    );
    program
}

/// Runs a program the tree-walking VM was handed, and returns the lines `say`
/// printed.
#[track_caller]
fn say_lines_of(program: &redblue::parser::Program) -> Result<Vec<String>, Error> {
    let mut vm = Vm::new();
    let result = vm.run(program);
    let printed = vm.take_output();
    result.map(|_| printed)
}

/// Runs a program on the bytecode VM, through the compiler the `rb compile`
/// path uses, and returns the lines `say` printed.
#[track_caller]
fn bytecode_say_lines(program: &redblue::parser::Program) -> Result<Vec<String>, Error> {
    let chunk = redblue::bytecode::compile_program(program)?;
    let mut vm = redblue::bytecode::vm::BytecodeVm::new();
    let result = vm.run(&chunk);
    let printed = vm.take_output();
    result.map(|_| printed)
}

/// A `break` in a `for each i from a to b` loop ends it, and both VMs say so.
///
/// The form is built as an AST rather than written as source because the parser
/// has no `from` production; see [`range_loop`].
#[test]
fn a_break_in_a_range_loop_ends_it() {
    let source = range_loop(
        "i",
        "1",
        "10",
        None,
        concat!("if i is 5 then\n", "    break\n", "end\n", "say i\n",),
        "",
    );

    let printed = say_lines_of(&source).expect("a break must not fail the program");

    assert_eq!(
        printed,
        vec!["1", "2", "3", "4"],
        "the loop stops at the value that broke, and the value itself is not printed"
    );
}

/// The `skip` companion: the turn it drops is out of the printout and the loop
/// carries on to its end.
#[test]
fn a_skip_in_a_range_loop_advances_to_the_next_value() {
    let source = range_loop(
        "i",
        "1",
        "6",
        None,
        concat!("if i is 3 then\n", "    skip\n", "end\n", "say i\n",),
        "",
    );

    let printed = say_lines_of(&source).expect("a skip must not fail the program");

    assert_eq!(
        printed,
        vec!["1", "2", "4", "5", "6"],
        "the skipped value is left out and the rest of the range still runs"
    );
}

/// The stepped form, both statements: a `by` step is the same loop with a
/// different stride, and `break` and `skip` do not consult it except to say
/// which values the turns have.
#[test]
fn edge_break_and_skip_in_a_stepped_range_loop() {
    let broken = range_loop(
        "i",
        "0",
        "10",
        Some("2"),
        concat!("if i is 6 then\n", "    break\n", "end\n", "say i\n",),
        "",
    );
    assert_eq!(
        say_lines_of(&broken).expect("a break must not fail the program"),
        vec!["0", "2", "4"],
        "the loop stops at the stepped value that broke"
    );

    let skipped = range_loop(
        "i",
        "0",
        "10",
        Some("2"),
        concat!("if i is 4 then\n", "    skip\n", "end\n", "say i\n",),
        "",
    );
    assert_eq!(
        say_lines_of(&skipped).expect("a skip must not fail the program"),
        vec!["0", "2", "6", "8", "10"],
        "the skipped stepped value is left out and the range carries on"
    );
}

/// An empty range — one whose start is past its end — is no turns at all, and a
/// `break` or a `skip` written in the body of one never runs.
#[test]
fn edge_break_and_skip_in_a_range_loop_that_runs_no_turns() {
    for statement in ["break", "skip"] {
        let source = range_loop("i", "5", "1", None, &format!("{statement}\n"), "");
        assert_eq!(
            say_lines_of(&source).expect("an empty range must not fail the program"),
            Vec::<String>::new(),
            "a range whose start is past its end runs no turns, so `{statement}` never runs"
        );
    }
}

/// The bytecode VM's range loop is the same loop: the same programs, the same
/// printouts, the same failures.
///
/// The corpus's range programs run through this path too, but none of them
/// contains a `break` or a `skip`, so nothing before this phase compared the
/// two VMs' answer for either statement in this loop form.
#[test]
fn edge_both_vms_answer_the_same_for_break_and_skip_in_a_range_loop() {
    let bodies = [
        ("break", "if i is 5 then\n    break\nend\nsay i\n"),
        ("skip", "if i is 3 then\n    skip\nend\nsay i\n"),
        (
            "break-then-skip",
            "if i is 3 then\n    skip\nend\nif i is 5 then\n    break\nend\nsay i\n",
        ),
    ];
    for (name, body) in bodies {
        for step in [None, Some("2")] {
            let (start, end) = match step {
                Some(_) => ("0", "10"),
                None => ("1", "10"),
            };
            let source = range_loop("i", start, end, step, body, "say \"after\"\n");
            let tree = say_lines_of(&source)
                .unwrap_or_else(|e| panic!("{name} on the tree-walking VM: {e:?}"));
            let byte = bytecode_say_lines(&source)
                .unwrap_or_else(|e| panic!("{name} on the bytecode VM: {e:?}"));
            assert_eq!(
                tree, byte,
                "the two VMs disagree about {name} in a range loop (step {step:?})\n{body}"
            );
        }
    }
}

/// Neither `break` nor `skip` takes an operand.
///
/// `docs/GRAMMAR.md` wrote `skip_statement = 'skip' [ expression ]`, which
/// promised a form the parser does not implement: `TokenKind::Skip` advances
/// once and returns, so `skip 1` is two statements — the jump, and `1` as an
/// expression statement of its own. The grammar is the thing that was wrong,
/// since there is nothing for an operand to say; the loop a `skip` acts on is
/// the one around it and the turn it goes to is the next one.
#[test]
fn edge_neither_break_nor_skip_takes_an_operand() {
    // `skip 1` skips the turn, and the `1` after it is a statement of its own:
    // the value that was skipped never reaches it.
    let source = concat!(
        "for each i in [1, 2, 3]\n",
        "    if i is 2 then\n",
        "        skip 1\n",
        "    end\n",
        "    say i\n",
        "end\n",
    );
    assert_eq!(
        say_lines(source).expect("a skip must not fail the program"),
        vec!["1".to_string(), "3".to_string()],
        "the operand of `skip` is not an operand: it is a separate statement"
    );

    // The same for `break`: `break 1` leaves the loop at the same place, and the
    // `1` is never evaluated.
    let source = concat!(
        "for each i in [1, 2, 3]\n",
        "    if i is 2 then\n",
        "        break 1\n",
        "    end\n",
        "    say i\n",
        "end\n",
    );
    assert_eq!(
        say_lines(source).expect("a break must not fail the program"),
        vec!["1".to_string()],
        "the operand of `break` is not an operand: it is a separate statement"
    );
}

/// A `skip` or a `break` followed by an expression the program cannot evaluate is
/// still a clean skip or break, because the expression is a statement of its own
/// that the turn never reaches.
#[test]
fn edge_a_skip_leaves_the_statements_after_it_unreached() {
    let source = concat!(
        "for each i in [1, 2, 3]\n",
        "    skip\n",
        "    say 1 + \"one\"\n",
        "end\n",
        "say \"after\"\n",
    );
    assert_eq!(
        say_lines(source).expect("a skip must not fail the program"),
        vec!["after".to_string()],
        "the statement after a `skip` is part of the turn the skip dropped"
    );
}

// -- round 4: the two blocks that still ran past a signal --------------------

/// An `unless` body is a block, so a `break` in it ends the statements after it
/// as well as the loop.
///
/// The body used to run as a plain statement list, which meant the program left
/// the loop having printed the line *after* the `break` — and the bytecode VM,
/// whose `unless` body compiles into the enclosing block and whose `BREAK` jumps
/// over the rest, did not. The two VMs answered the same program differently.
#[test]
fn a_break_in_an_unless_body_does_not_run_the_statements_after_it() {
    let source = concat!(
        "for each i in [1, 2, 3]\n",
        "    unless i is 2 then\n",
        "        say i\n",
        "        break\n",
        "        say \"leaked\"\n",
        "    end\n",
        "    say \"after\"\n",
        "end\n",
        "say \"done\"\n",
    );

    assert_eq!(
        say_lines(source).expect("a break must not fail the program"),
        vec!["1".to_string(), "done".to_string()],
        "the statements after a `break` in an `unless` body are part of the turn it stopped"
    );
}

/// The `skip` companion: the turn is dropped whole, so the rest of the body and
/// the rest of the turn after the `unless` are both out of the printout.
#[test]
fn a_skip_in_an_unless_body_leaves_the_statements_after_it_unreached() {
    let source = concat!(
        "set log to \"\"\n",
        "for each i in [1, 2, 3]\n",
        "    unless i is 99 then\n",
        "        say i\n",
        "        skip\n",
        "        say \"leaked\"\n",
        "    end\n",
        "    set log to log + i\n",
        "end\n",
        "say log\n",
        "say \"done\"\n",
    );

    assert_eq!(
        say_lines(source).expect("a skip must not fail the program"),
        vec![
            "1".to_string(),
            "2".to_string(),
            "3".to_string(),
            String::new(),
            "done".to_string(),
        ],
        "a `skip` in an `unless` body drops its own turn whole, body and all"
    );
}

/// The `unless` fix through **both** VMs, in a `while` as well as a `for each`:
/// the block rule is the same whatever loop the body sits in.
#[test]
fn edge_both_vms_answer_the_same_for_break_and_skip_in_an_unless_body() {
    let programs = [
        (
            "break in a for each",
            concat!(
                "for each i in [1, 2, 3]\n",
                "    unless i is 2 then\n",
                "        say i\n",
                "        break\n",
                "        say \"leaked\"\n",
                "    end\n",
                "    say \"after\"\n",
                "end\n",
                "say \"done\"\n",
            ),
            vec!["1", "done"],
        ),
        (
            "skip in a for each",
            concat!(
                "for each i in [1, 2, 3]\n",
                "    unless i is 99 then\n",
                "        say i\n",
                "        skip\n",
                "        say \"leaked\"\n",
                "    end\n",
                "end\n",
                "say \"done\"\n",
            ),
            vec!["1", "2", "3", "done"],
        ),
        (
            "break in a while",
            concat!(
                "set n to 0\n",
                "while n is not 4\n",
                "    set n to n + 1\n",
                "    if n is 3 then\n",
                "        unless n is 99 then\n",
                "            say n\n",
                "            break\n",
                "            say \"leaked\"\n",
                "        end\n",
                "    end\n",
                "end\n",
            ),
            vec!["3"],
        ),
        (
            "an unless body whose condition holds runs nothing at all",
            concat!(
                "for each i in [1, 2]\n",
                "    unless 1 is 1 then\n",
                "        say \"never\"\n",
                "        break\n",
                "    end\n",
                "    say i\n",
                "end\n",
            ),
            vec!["1", "2"],
        ),
    ];
    for (name, source, expected) in programs {
        let program = parse(source);
        let tree = say_lines_of(&program).unwrap_or_else(|e| panic!("{name} on the tree: {e:?}"));
        let byte = bytecode_say_lines(&program)
            .unwrap_or_else(|e| panic!("{name} on the bytecode: {e:?}"));
        assert_eq!(
            tree, byte,
            "the two VMs disagree about an `{name}`\n{source}"
        );
        assert_eq!(
            tree,
            expected
                .iter()
                .map(|line| line.to_string())
                .collect::<Vec<String>>(),
            "{name} printed the wrong lines\n{source}"
        );
    }
}

/// A module body is written where the declaration is written, so a `break` in it
/// leaves the loop around the declaration — exactly as it does in an `object`
/// body, and unlike a function body, which runs when it is called.
///
/// The body used to run as a statement list that consulted only the failure, so
/// the statements after the `break` ran and then the loop ended anyway; the
/// bytecode VM refused the `break` outright, since the module's frame recorded
/// no loop to leave.
#[test]
fn a_break_in_a_module_body_inside_a_loop_leaves_that_loop() {
    let source = concat!(
        "set count to 0\n",
        "for each i in [1, 2, 3]\n",
        "    set count to count + 1\n",
        "    module Inner\n",
        "        set held to i\n",
        "        break\n",
        "        say \"leaked\"\n",
        "    end\n",
        "    say \"leaked\"\n",
        "end\n",
        "say count\n",
    );

    assert_eq!(
        say_lines(source).expect("a break in a module body must not fail the program"),
        vec!["1".to_string()],
        "the loop stops at the turn that broke, and the body stops at the `break`"
    );
}

/// The `skip` companion, in a loop with more than one turn to advance through.
#[test]
fn edge_a_skip_in_a_module_body_advances_the_loop_it_is_written_in() {
    let source = concat!(
        "set log to \"\"\n",
        "for each i in [\"a\", \"b\", \"c\"]\n",
        "    set log to log + i\n",
        "    module Inner\n",
        "        skip\n",
        "        say \"leaked\"\n",
        "    end\n",
        "    set log to log + \".\"\n",
        "end\n",
        "say log\n",
    );

    assert_eq!(
        say_lines(source).expect("a skip in a module body must not fail the program"),
        vec!["abc".to_string()],
        "each turn is skipped whole, so neither the rest of the body nor the rest of the turn runs"
    );
}

/// A module body left by a jump publishes nothing and declares nothing: an
/// `import` of that name afterwards is the miss an `import` of an unknown module
/// is, rather than a call into a module that published nothing.
#[test]
fn edge_a_module_body_left_by_a_break_publishes_nothing_and_declares_nothing() {
    let source = concat!(
        "set count to 0\n",
        "repeat 2 times\n",
        "    set count to count + 1\n",
        "    module Gone\n",
        "        export value\n",
        "        set value to 1\n",
        "        break\n",
        "    end\n",
        "end\n",
        "say count\n",
        "try\n",
        "    import Gone\n",
        "    say \"imported\"\n",
        "catch error\n",
        "    say \"no module\"\n",
        "end\n",
    );

    assert_eq!(
        say_lines(source).expect("a caught import must not fail the program"),
        vec!["1".to_string(), "no module".to_string()],
        "the module is not declared, so the import that follows is a miss"
    );
}

/// The two refusals, which are the module-body counterpart of the function-body
/// exemption: a module body written where there is no loop has no loop to leave,
/// and one written inside a function body inherits the function's exemption even
/// when the call was made from a loop. In both the failure is catchable and the
/// loop that called it survives.
#[test]
fn edge_a_break_in_a_module_body_outside_a_loop_is_refused() {
    let source = concat!(
        "set caught to \"no\"\n",
        "try\n",
        "    module Lone\n",
        "        break\n",
        "    end\n",
        "catch error\n",
        "    set caught to \"yes\"\n",
        "end\n",
        "say caught\n",
    );
    assert_eq!(
        say_lines(source).expect("the refusal is catchable, not fatal"),
        vec!["yes".to_string()],
        "a `break` in a module body written outside a loop is refused and caught"
    );

    let in_a_function = concat!(
        "to declare()\n",
        "    module Inner\n",
        "        break\n",
        "    end\n",
        "end\n",
        "set n to 0\n",
        "set caught to \"no\"\n",
        "repeat 2 times\n",
        "    set n to n + 1\n",
        "    try\n",
        "        declare()\n",
        "    catch error\n",
        "        set caught to \"yes\"\n",
        "    end\n",
        "end\n",
        "say n\n",
        "say caught\n",
    );
    assert_eq!(
        say_lines(in_a_function).expect("the refusal is catchable, not fatal"),
        vec!["2".to_string(), "yes".to_string()],
        "a module body inside a function is in no loop, and the caller's loop survives"
    );

    let uncaught = "module Lone\n    break\nend\n";
    assert_eq!(
        runtime_message(&say_lines(uncaught).expect_err("a `break` in no loop is refused")),
        "'break' is only valid inside a loop",
        "the refusal names the statement, as it does everywhere else"
    );
}

/// A `finally` a jump passes through runs *after* the module body has given its
/// scope back, so a `set` in it is a name of the program rather than one of the
/// module the exit just left.
///
/// This is the ordering rule the tree-walking VM gets for free — the signal
/// passes out through one block at a time — and the one the bytecode VM has to
/// finish a crossed frame before running the `finally` of the `try` written
/// around it.
#[test]
fn edge_a_finally_around_a_module_declaration_runs_after_its_scope_is_gone() {
    let source = concat!(
        "set log to \"\"\n",
        "repeat 2 times\n",
        "    set log to log + \"t\"\n",
        "    try\n",
        "        module Inner\n",
        "            set held to 1\n",
        "            break\n",
        "        end\n",
        "        set log to log + \"leaked\"\n",
        "    catch error\n",
        "        set log to log + \"c\"\n",
        "    finally\n",
        "        set log to log + \"f\"\n",
        "    end\n",
        "    set log to log + \".\"\n",
        "end\n",
        "say log\n",
    );

    assert_eq!(
        say_lines(source).expect("an abrupt exit is not a failure"),
        vec!["tf".to_string()],
        "the turn's `t` and the `finally`'s `f`, with nothing the exit skipped"
    );
}

/// The module-body rules through **both** VMs: the jumps that leave a loop, the
/// two refusals, the abandoned module and the `finally` ordering.
#[test]
fn edge_both_vms_answer_the_same_for_a_jump_in_a_module_body() {
    let programs = [
        (
            "break in a for each",
            concat!(
                "set count to 0\n",
                "for each i in [1, 2, 3]\n",
                "    set count to count + 1\n",
                "    module Inner\n",
                "        set held to i\n",
                "        break\n",
                "        say \"leaked\"\n",
                "    end\n",
                "    say \"leaked\"\n",
                "end\n",
                "say count\n",
            ),
        ),
        (
            "skip in a for each",
            concat!(
                "set log to \"\"\n",
                "for each i in [\"a\", \"b\", \"c\"]\n",
                "    set log to log + i\n",
                "    module Inner\n",
                "        skip\n",
                "        say \"leaked\"\n",
                "    end\n",
                "    set log to log + \".\"\n",
                "end\n",
                "say log\n",
            ),
        ),
        (
            "break in a while",
            concat!(
                "set n to 0\n",
                "while n is not 9\n",
                "    set n to n + 1\n",
                "    module Inner\n",
                "        break\n",
                "    end\n",
                "    say \"leaked\"\n",
                "end\n",
                "say n\n",
            ),
        ),
        (
            "an abandoned module publishes nothing",
            concat!(
                "repeat 2 times\n",
                "    module Gone\n",
                "        export value\n",
                "        set value to 1\n",
                "        break\n",
                "    end\n",
                "end\n",
                "try\n",
                "    import Gone\n",
                "    say \"imported\"\n",
                "catch error\n",
                "    say \"no module\"\n",
                "end\n",
            ),
        ),
        (
            "a refusal outside any loop",
            concat!(
                "set caught to \"no\"\n",
                "try\n",
                "    module Lone\n",
                "        break\n",
                "    end\n",
                "catch error\n",
                "    set caught to \"yes\"\n",
                "end\n",
                "say caught\n",
            ),
        ),
        (
            "the exemption a function body carries",
            concat!(
                "to declare()\n",
                "    module Inner\n",
                "        break\n",
                "    end\n",
                "end\n",
                "set n to 0\n",
                "set caught to \"no\"\n",
                "repeat 2 times\n",
                "    set n to n + 1\n",
                "    try\n",
                "        declare()\n",
                "    catch error\n",
                "        set caught to \"yes\"\n",
                "    end\n",
                "end\n",
                "say n\n",
                "say caught\n",
            ),
        ),
        (
            "a finally around the declaration",
            concat!(
                "set log to \"\"\n",
                "repeat 2 times\n",
                "    set log to log + \"t\"\n",
                "    try\n",
                "        module Inner\n",
                "            set held to 1\n",
                "            break\n",
                "        end\n",
                "    finally\n",
                "        set log to log + \"f\"\n",
                "    end\n",
                "end\n",
                "say log\n",
            ),
        ),
    ];
    for (name, source) in programs {
        let program = parse(source);
        let tree = say_lines_of(&program).unwrap_or_else(|e| panic!("{name} on the tree: {e:?}"));
        let byte = bytecode_say_lines(&program)
            .unwrap_or_else(|e| panic!("{name} on the bytecode: {e:?}"));
        assert_eq!(
            tree, byte,
            "the two VMs disagree about a module body: {name}\n{source}"
        );
    }
}
