//! `repeat ... until <condition>`: the post-test loop `SPEC.md` documents.
//!
//! The lexer has reserved `until` since it was written and `SPEC.md` §"Repeat
//! Until" has shown a worked example of the form, but no rule in the parser
//! consumed the keyword, so the documented loop was unreachable: a program using
//! it was refused at the end of the `repeat` line, by a parser that stopped
//! because nothing there expected an `until`.
//!
//! This file pins what the loop *does*, on both engines, rather than only that
//! it parses. `repeat` is a post-test loop: the body runs and the condition is
//! read afterwards, so a condition already true on entry still runs the body
//! once, a body that fails is not rescued by a condition that would have ended
//! the loop, and a condition that never comes out true is bounded by the same
//! per-loop cap every other loop is bounded by — reported as a limit error
//! rather than left to run.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use redblue::bytecode::compile_source;
use redblue::bytecode::vm::BytecodeVm;
use redblue::Error;

/// A scratch directory under `target/`, emptied before use.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/repeat-until-test")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir should be creatable");
    dir
}

/// Runs the `rb` binary this test was built with.
fn rb(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rb"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("rb should be runnable")
}

/// One VM's answer to a program: what it printed, and how it ended.
///
/// Both VMs are run as libraries so that a disagreement names the program
/// rather than an exit code.
#[derive(Debug, PartialEq, Eq)]
struct Outcome {
    output: Vec<String>,
    result: Result<String, String>,
}

/// A frontend failure, as an outcome: nothing ran, so nothing was printed.
fn failed(error: &Error) -> Outcome {
    Outcome {
        output: Vec::new(),
        result: Err(format!("{}: {}", error.label(), error.message())),
    }
}

fn failure_of(error: &Error) -> String {
    format!("{}: {}", error.label(), error.message())
}

fn parse(source: &str) -> Result<redblue::parser::Program, Outcome> {
    let tokens = match redblue::lexer::Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => return Err(failed(&error)),
    };
    match redblue::parser::parse(tokens) {
        Ok(ast) => Ok(ast),
        Err(error) => Err(failed(&error)),
    }
}

/// Runs `source` on the tree-walking VM.
fn tree_walk(source: &str) -> Outcome {
    let Ok(ast) = parse(source) else {
        return parse(source).unwrap_err();
    };
    if let Err(error) = redblue::analyzer::analyze(&ast) {
        return failed(&error);
    }
    let mut vm = redblue::Vm::new();
    let result = vm.run(&ast);
    Outcome {
        output: vm.take_output(),
        result: result
            .map(|value| value.to_string())
            .map_err(|error| failure_of(&error)),
    }
}

/// Runs `source` through the bytecode compiler and then the bytecode VM.
fn bytecode(source: &str) -> Outcome {
    let chunk = match compile_source(source) {
        Ok(chunk) => chunk,
        Err(error) => return failed(&error),
    };
    let mut vm = BytecodeVm::new();
    let result = vm.run(&chunk);
    Outcome {
        output: vm.take_output(),
        result: result
            .map(|value| value.to_string())
            .map_err(|error| failure_of(&error)),
    }
}

/// Runs `source` on both VMs and fails when they disagree, returning what the
/// tree-walking VM made of it.
#[track_caller]
fn assert_agrees(source: &str) -> Outcome {
    let tree = tree_walk(source);
    let byte = bytecode(source);
    assert_eq!(
        tree, byte,
        "the two VMs disagree\n--- source ---\n{source}--- tree ---\n{tree:?}\n--- bytecode ---\n{byte:?}\n"
    );
    tree
}

/// [`assert_agrees`] with the per-loop iteration cap lowered to
/// `max_iterations`.
///
/// Nothing in an ordinary program can reach the published cap — a loop that
/// needs a million turns takes a million turns to run — so this is how the
/// cap's answer to a loop is asked about rather than waited for.
#[track_caller]
fn assert_agrees_capped(source: &str, max_iterations: usize) -> Outcome {
    let ast = parse(source).expect("source should reach the analyzer");
    let mut tree = redblue::Vm::with_max_iterations(max_iterations);
    let tree_result = tree.run(&ast).map_err(|error| failure_of(&error));
    let tree = Outcome {
        output: tree.take_output(),
        result: tree_result.map(|value| value.to_string()),
    };

    let chunk = compile_source(source).expect("source should compile");
    let mut byte = BytecodeVm::with_max_iterations(max_iterations);
    let byte_result = byte.run(&chunk).map_err(|error| failure_of(&error));
    let byte = Outcome {
        output: byte.take_output(),
        result: byte_result.map(|value| value.to_string()),
    };

    assert_eq!(
        tree, byte,
        "the two VMs disagree under a cap of {max_iterations}\n--- source ---\n{source}\
         --- tree ---\n{tree:?}\n--- bytecode ---\n{byte:?}\n"
    );
    tree
}

/// The error a program produced, and the line it names.
#[track_caller]
fn runtime_error(source: &str) -> Error {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let ast = redblue::parser::parse(tokens).expect("source should parse");
    redblue::analyzer::analyze(&ast).expect("source should analyze");
    let mut vm = redblue::Vm::new();
    vm.run(&ast).expect_err("source should have failed")
}

/// The line the bytecode VM names when `source` fails there.
#[track_caller]
fn bytecode_error_line(source: &str) -> usize {
    let chunk = compile_source(source).expect("source should compile");
    let mut vm = BytecodeVm::new();
    let error = vm.run(&chunk).expect_err("source should have failed");
    error.span().expect("the failure must carry a span").line
}

/// Every `//` comment in `source`, in the order it was written.
fn comments(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| line.find("//").map(|at| line[at..].trim_end().to_string()))
        .collect()
}

/// Formats `source` and refuses a formatter that loses anything on the way.
///
/// Three things are asserted, because a post-test loop is a block whose closing
/// keyword is not an `end` and that is what makes it different for a formatter:
/// the result formats to itself again (idempotence), every comment of the source
/// is still in the result, and the result still says the same thing when run.
#[track_caller]
fn formatted(source: &str) -> String {
    let once = redblue::formatter::format(source).expect("source should format");
    let twice = redblue::formatter::format(&once).expect("formatted source should format again");
    assert_eq!(
        once, twice,
        "formatting is not stable\n--- source ---\n{source}--- once ---\n{once}--- twice ---\n{twice}\n"
    );
    assert_eq!(
        comments(&once),
        comments(source),
        "formatting moved or dropped a comment\n--- source ---\n{source}--- formatted ---\n{once}\n"
    );
    once
}

/// The worked example `SPEC.md` §"Repeat Until" is written around, on both
/// engines: the body runs before the condition is read, so `n` reaches 3 and
/// stops there rather than running to 4.
#[test]
fn a_repeat_until_stops_on_the_first_true_condition() {
    let source = "set n to 0\nrepeat\n    set n to n + 1\nuntil n is 3\nsay n\n";
    let outcome = assert_agrees(source);
    assert_eq!(
        outcome.output,
        vec!["3".to_string()],
        "the body runs until the condition is true, and not one turn past it"
    );
}

/// The counted form is the sibling of the post-test one and shares the keyword;
/// it must be untouched by a change that makes `until` a loop.
#[test]
fn the_counted_repeat_still_runs_exactly_its_count() {
    let source = "repeat 3 times\n    say \"x\"\nend\n";
    let outcome = assert_agrees(source);
    assert_eq!(
        outcome.output,
        vec!["x".to_string(), "x".to_string(), "x".to_string()],
        "`repeat 3 times` must still print three lines"
    );
}

/// The minimum a post-test loop can do: one turn, and then the condition it was
/// handed ends it.
///
/// A `while` given a false condition runs no body at all. This one cannot, or it
/// would not be the loop `SPEC.md` documents — its body runs and its condition is
/// read afterwards — so a condition that is *already* true leaves the loop after
/// exactly one turn rather than after none, and never after two.
#[test]
fn edge_a_condition_true_on_entry_leaves_after_exactly_one_turn() {
    let source = "set n to 0\nrepeat\n    set n to n + 1\n    say n\nuntil n >= 0\n";
    let outcome = assert_agrees(source);
    assert_eq!(
        outcome.output,
        vec!["1".to_string()],
        "the body runs before the condition is read, so a true condition on \
         entry still costs one turn"
    );
}

/// The loop with no body at all: its condition is read once and ends it, and
/// that once is what keeps it from spinning — a body that changes nothing would
/// read the same condition for ever if it were read again.
#[test]
fn edge_an_empty_body_reads_its_condition_once_and_stops() {
    let source = "repeat\nuntil yes is yes\nsay \"once\"\n";
    let outcome = assert_agrees(source);
    assert_eq!(
        outcome.output,
        vec!["once".to_string()],
        "an empty body must still turn once and then stop, rather than spin on \
         a condition it cannot change"
    );
}

/// A cap of N is N turns of this loop as it is of every other, on both engines,
/// and one turn less is the cap that stops it.
///
/// Both engines charge at the top of a turn, before the body runs, which is why
/// the two agree about the boundary rather than differing by the turn a charge
/// placed after the body would allow one engine and not the other.
#[test]
fn edge_a_cap_is_turns_of_a_post_test_loop_on_both_vms() {
    const TURNS: usize = 5;
    // The body's first statement counts, so what the program prints is how many
    // turns the cap allowed.
    let source = format!(
        "set turns to 0\nrepeat\n    set turns to turns + 1\nuntil turns is {TURNS}\nsay turns\n"
    );

    let tree = assert_agrees_capped(&source, TURNS);
    assert_eq!(
        tree.output,
        vec![TURNS.to_string()],
        "a cap of {TURNS} must be {TURNS} turns of this loop on both VMs, and the \
         loop must finish rather than report the cap"
    );

    let tree = assert_agrees_capped(&source, TURNS - 1);
    let message = tree
        .result
        .as_ref()
        .expect_err("a cap one turn short must stop a loop that needs one more");
    assert!(
        message.contains(&format!("Maximum of {} iterations", TURNS - 1)),
        "and it must be the per-loop cap that stops it, said: {message}"
    );
    assert_eq!(
        tree.output,
        Vec::<String>::new(),
        "the `say` is after the loop, so the turn the cap refuses takes the \
         whole program with it: no body of that turn has run on either engine"
    );
}

/// A condition that never comes out true is a loop with no end of its own, so
/// what stops it is the cap — named, and the same on both engines, rather than a
/// program left to run.
///
/// The target here is a fraction: `turns` counts 1, 2, 3 … and is never 2.5, so
/// the condition is false forever and a cap of five is the only thing that can
/// end it.
#[test]
fn edge_a_condition_that_never_comes_true_is_stopped_by_the_cap() {
    let source = "set turns to 0\nrepeat\n    set turns to turns + 1\nuntil turns is 2.5\n";

    let (tree, byte) = {
        let tree = assert_agrees_capped(source, 5);
        let byte = bytecode_capped(source, 5);
        (tree, byte)
    };

    let message = tree
        .result
        .as_ref()
        .expect_err("a condition that never comes true must still be bounded");
    assert!(
        message.contains("Maximum of 5 iterations"),
        "the per-loop cap must be what stops it, said: {message}"
    );
    assert!(
        message.contains("'repeat' loop"),
        "and it must name the kind of loop it stopped, said: {message}"
    );
    assert!(
        byte.result.is_err(),
        "the bytecode VM must be stopped by the same cap, said: {byte:?}"
    );
}

/// [`assert_agrees_capped`]'s bytecode half, for the assertions that want the
/// second engine's outcome as well as the first's.
fn bytecode_capped(source: &str, max_iterations: usize) -> Outcome {
    let chunk = compile_source(source).expect("source should compile");
    let mut vm = BytecodeVm::with_max_iterations(max_iterations);
    let result = vm.run(&chunk).map_err(|error| failure_of(&error));
    Outcome {
        output: vm.take_output(),
        result: result.map(|value| value.to_string()),
    }
}

/// The body runs before the condition is read, so a body that fails is a
/// failure — the condition that would have ended the loop never gets to say so,
/// and the failure names the line the body failed on rather than the `until`.
#[test]
fn a_failing_body_is_not_rescued_by_a_condition_that_would_have_stopped_it() {
    let source = concat!(
        "set n to 0\n",
        "repeat\n",
        "    set n to n + 1\n",
        "    say 1 / 0\n",
        "until n is 1\n",
    );

    let outcome = assert_agrees(source);
    let message = outcome
        .result
        .as_ref()
        .expect_err("a body that divides by zero must fail the program");
    assert!(
        message.contains("RuntimeError") || message.contains("Error"),
        "the failure must be reported as a failure, said: {message}"
    );

    let error = runtime_error(source);
    assert_eq!(
        error.span().expect("the failure must carry a span").line,
        4,
        "the failure must name the line the body failed on, not the `until` on \
         line 5, said: {error}"
    );
}

/// `break` and `skip` mean what they mean in every other loop: `break` leaves
/// this one, and `skip` goes on to its next turn without reading its condition —
/// which is what going to the top of a loop whose condition is at its bottom
/// means.
#[test]
fn a_break_leaves_a_post_test_loop_and_a_skip_starts_the_next_turn() {
    let breaking = concat!(
        "set n to 0\n",
        "repeat\n",
        "    set n to n + 1\n",
        "    if n is 2 then\n",
        "        break\n",
        "    end\n",
        "until n > 10\n",
        "say n\n",
    );
    let outcome = assert_agrees(breaking);
    assert_eq!(
        outcome.output,
        vec!["2".to_string()],
        "`break` must leave the loop rather than going round to its condition"
    );

    // A `skip` goes to the top of the loop, and the top of this one is its body,
    // so the turn it abandons is not the one that reads the condition. The two
    // answers are different programs: reading the condition on the first turn
    // would end the loop there, before the body had printed anything.
    let skipping = concat!(
        "set n to 0\n",
        "repeat\n",
        "    set n to n + 1\n",
        "    if n is 1 then\n",
        "        skip\n",
        "    end\n",
        "    say \"body\"\n",
        "until n >= 1\n",
        "say n\n",
    );
    let outcome = assert_agrees(skipping);
    assert_eq!(
        outcome.output,
        vec!["body".to_string(), "2".to_string()],
        "a skipped turn goes straight back to the top of the loop, so the \
         condition of the turn it abandoned is never read: had it been read, \
         `n >= 1` would have ended the loop on the first turn and this would \
         have printed only `1`"
    );
}

/// A post-test loop inside another loop, and one inside itself: the caps are
/// per loop, and the inner loop's turns are the inner loop's.
#[test]
fn edge_a_post_test_loop_nested_in_a_for_each_and_in_itself() {
    let source = concat!(
        "set total to 0\n",
        "for each width in [1, 2, 3]\n",
        "    set n to 0\n",
        "    repeat\n",
        "        set n to n + 1\n",
        "        set total to total + n\n",
        "    until n is width\n",
        "end\n",
        "say total\n",
    );
    let outcome = assert_agrees(source);
    assert_eq!(
        outcome.output,
        vec!["10".to_string()],
        "each outer turn must run the inner loop to its own condition: 1, then \
         1 + 1 + 2, then 1 + 1 + 2 + 1 + 2 + 3"
    );

    let nested = concat!(
        "set outer to 0\n",
        "repeat\n",
        "    set outer to outer + 1\n",
        "    set inner to 0\n",
        "    repeat\n",
        "        set inner to inner + 1\n",
        "    until inner is 2\n",
        "    set outer to outer + 1\n",
        "until outer >= 3\n",
        "say outer\n",
    );
    let outcome = assert_agrees(nested);
    assert_eq!(
        outcome.output,
        vec!["4".to_string()],
        "an inner post-test loop must not end the outer one, and its two turns \
         must be its own"
    );
}

/// A condition is an ordinary expression, so it can fail like one: an index
/// that names no element is a clean error on both engines, and not a panic.
#[test]
fn edge_a_condition_that_indexes_past_the_end_is_a_clean_failure() {
    let source = "set xs to [1, 2]\nset n to 0\nrepeat\n    set n to n + 1\nuntil xs[99] is 1\n";
    let outcome = assert_agrees(source);
    let message = outcome
        .result
        .as_ref()
        .expect_err("an index that names no element must fail the loop");
    assert!(
        message.contains("RuntimeError"),
        "the condition's own failure must be the failure, said: {message}"
    );
    assert_eq!(
        outcome.output,
        Vec::<String>::new(),
        "nothing is printed, because the first turn's body prints nothing"
    );
}

/// The body is statements, and they say whatever they say: this one prints text
/// the loop must not mangle, once per turn.
#[test]
fn edge_the_body_prints_the_unicode_it_was_given() {
    let source = concat!(
        "set n to 0\n",
        "repeat\n",
        "    set n to n + 1\n",
        "    say \"héllo 🌍 مرحبا\"\n",
        "until n is 2\n",
    );
    let outcome = assert_agrees(source);
    assert_eq!(
        outcome.output,
        vec!["héllo 🌍 مرحبا".to_string(), "héllo 🌍 مرحبا".to_string()],
        "one line per turn, byte for byte the text the body was given"
    );
}

/// A `repeat` with no `until` is a program the parser refuses, on both
/// frontends, with the same message — and neither of them panics finding out.
#[test]
fn edge_a_repeat_with_no_until_is_a_clean_parser_failure() {
    let source = "set n to 0\nrepeat\n    set n to n + 1\n";
    let outcome = assert_agrees(source);
    let message = outcome
        .result
        .as_ref()
        .expect_err("a post-test loop with no `until` must be refused");
    assert!(
        message.contains("ParserError"),
        "it must be refused by the parser, said: {message}"
    );
    assert!(
        !message.contains("panic"),
        "and refused rather than crashed, said: {message}"
    );

    // The counted form's `end` is an `end` the post-test form has no use for, so
    // writing one is a stray token rather than a way to close the loop.
    let stray = "set n to 0\nrepeat\n    set n to n + 1\nuntil n is 3\nend\n";
    let outcome = assert_agrees(stray);
    assert!(
        outcome.result.is_err(),
        "an `end` after the `until` line is a stray token, said: {outcome:?}"
    );
}

/// The two engines as the CLI runs them: `rb run` on the source and `rb vm` on
/// what `rb compile` made of it, which is the whole of the S1 ladder for a loop
/// that had no instruction before this phase.
#[test]
fn edge_rb_run_and_rb_vm_print_the_same_post_test_loop() {
    let dir = scratch_dir("cli");
    let source = dir.join("loop.rb");
    fs::write(
        &source,
        "set n to 0\nrepeat\n    set n to n + 1\nuntil n is 3\nsay n\n",
    )
    .expect("the program should be writable");

    let run = rb(&["run", source.to_str().expect("a path")]);
    assert!(
        run.status.success(),
        "`rb run` should run it, said: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "3");

    let compiled = rb(&["compile", source.to_str().expect("a path")]);
    assert!(
        compiled.status.success(),
        "`rb compile` should compile it, said: {}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let bytecode = source.with_extension("rbc");
    assert!(
        bytecode.exists(),
        "`rb compile` should have written a `.rbc` beside the source"
    );

    let ran = rb(&["vm", bytecode.to_str().expect("a path")]);
    assert!(
        ran.status.success(),
        "`rb vm` should run it, said: {}",
        String::from_utf8_lossy(&ran.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&ran.stdout).trim(),
        "3",
        "`rb vm` must print what `rb run` printed"
    );
}

/// A condition may read a name the body binds, because the body runs before the
/// condition is read.
///
/// The `set` writes to the program's own scope — a write inside a loop outlives
/// the loop's scope — and both VMs read the condition after the body has had its
/// turn, so `t` is bound by the time `until t is 1` looks for it. The analyzer
/// read the condition first and refused the program as an unknown variable, so
/// this is the shape that has to analyze rather than merely parse.
#[test]
fn edge_a_condition_may_read_a_name_the_body_binds() {
    let source = "repeat\n    set t to 1\nuntil t is 1\nsay t\n";
    let outcome = assert_agrees(source);
    assert_eq!(
        outcome.output,
        vec!["1".to_string()],
        "a name the body binds is bound by the time the condition reads it, so \
         the analyzer must not refuse the program"
    );

    // The same shape with the name bound on a turn before the one that ends the
    // loop, which is where a condition that reads too early would print `1`.
    let counting = "set n to 0\nrepeat\n    set n to n + 1\nuntil n is 3\nsay n\n";
    let outcome = assert_agrees(counting);
    assert_eq!(
        outcome.output,
        vec!["3".to_string()],
        "a condition reading the body's own counter must see the counter's final \
         value, not a value from before the last turn"
    );

    // Reordering the two must not have cost the analyzer its other half: a name
    // neither the body nor the program binds is still an unknown variable.
    let unknown = "repeat\n    set t to 1\nuntil q is 1\n";
    let outcome = assert_agrees(unknown);
    let message = outcome
        .result
        .as_ref()
        .expect_err("a condition reading a name nothing binds must still be refused");
    assert!(
        message.contains("AnalyzerError") && message.contains("'q'"),
        "and refused by the analyzer, naming the name it could not find, said: \
         {message}"
    );
}

/// A post-test loop is a block, and the formatter has to know where it stops.
///
/// Its closing keyword is an `until` rather than an `end`, so a formatter that
/// only knows about `end` runs the body on past it and takes the enclosing
/// block's `end` as its own bound. Two comments show where that puts them: one
/// written at the end of the `until` line is pulled up into the body, and one
/// written after the `until` line — which belongs to the block the loop is in —
/// is pulled up with it.
#[test]
fn edge_a_post_test_loop_inside_a_block_keeps_the_comments_it_was_given() {
    // A comment after the `until` line belongs to the enclosing block, and a
    // comment at the end of the `until` line trails the `until`.
    let around = concat!(
        "repeat 3 times\n",
        "    repeat\n",
        "        set n to 1\n",
        "    until n is 3 // the inner condition\n",
        "    // the outer block's tail\n",
        "end\n",
        "say \"after\"\n",
    );
    let expected = concat!(
        "repeat 3 times\n",
        "    repeat\n",
        "        set n to 1\n",
        "    until n is 3 // the inner condition\n",
        "    // the outer block's tail\n",
        "end\n",
        "say \"after\"\n",
    );
    assert_eq!(
        formatted(around),
        expected,
        "each comment must stay on the side of the `until` its author wrote it on"
    );

    // The same two comments where the post-test loop is the whole program: the
    // one after the `until` belongs to the program, not to the loop.
    let alone = concat!(
        "set n to 0\n",
        "repeat\n",
        "    set n to n + 1\n",
        "until n is 3 // stop here\n",
        "// after the loop\n",
        "say n\n",
    );
    let expected = concat!(
        "set n to 0\n",
        "repeat\n",
        "    set n to n + 1\n",
        "until n is 3 // stop here\n",
        "// after the loop\n",
        "say n\n",
    );
    assert_eq!(
        formatted(alone),
        expected,
        "a comment written after the `until` line is outside the loop, and a \
         comment trailing the `until` line stays on it"
    );

    // A tail comment of the body's own still belongs to the body.
    let inner = concat!(
        "repeat 3 times\n",
        "    repeat\n",
        "        set n to 1\n",
        "        // the inner body's tail\n",
        "    until n is 3\n",
        "end\n",
        "say \"after\"\n",
    );
    let expected = concat!(
        "repeat 3 times\n",
        "    repeat\n",
        "        set n to 1\n",
        "        // the inner body's tail\n",
        "    until n is 3\n",
        "end\n",
        "say \"after\"\n",
    );
    assert_eq!(
        formatted(inner),
        expected,
        "a comment written inside the body is the body's, and must not move \
         below the `until` that closes it"
    );
}

/// A failure in the condition is the `until` line's, on both engines.
///
/// The condition is at the bottom of the loop, so it is the one expression whose
/// line is not the line of the statement that holds it. Naming the `repeat` above
/// it instead points at a line whose text says nothing about the failure, which is
/// what the engine does for every other expression.
#[test]
fn edge_a_failure_in_the_condition_names_the_until_line_on_both_vms() {
    let source = "set xs to [1, 2]\nset n to 0\nrepeat\n    set n to n + 1\nuntil xs[99] is 1\n";

    let error = runtime_error(source);
    assert_eq!(
        error.span().expect("the failure must carry a span").line,
        5,
        "the tree-walking VM must name the `until` line the condition was \
         written on, not the `repeat` on line 3, said: {error}"
    );
    assert_eq!(
        bytecode_error_line(source),
        5,
        "the bytecode VM must name the same line for the same failure"
    );

    // The body still names its own line: the fix is the condition's, not a shift
    // of every failure in the loop onto the `until`.
    let body = "set xs to [1, 2]\nset n to 0\nrepeat\n    say xs[99]\nuntil n is 1\n";
    assert_eq!(
        runtime_error(body)
            .span()
            .expect("the failure must carry a span")
            .line,
        4,
        "a failure in the body must still name the body's own line"
    );
}

/// A loop that is skipped on every turn still turns, so the cap is what stops it.
///
/// A `skip` abandons the rest of the turn, and the condition is at the rest of
/// this one, so a body that always skips changes nothing and its condition is
/// never read. The loop is bounded all the same — a turn is charged at the top of
/// it, before the body runs, whichever engine runs it — so a cap of N is N turns
/// and the Nth is refused. This is the shape FINDINGS.md § 4 records as bounded by
/// the step budget at the *published* cap; under a lowered one the loop's own cap
/// is reached first, and it must be the same limit on both engines.
#[test]
fn edge_every_turn_skipped_is_stopped_by_the_iteration_cap_on_both_vms() {
    let source = concat!(
        "set n to 0\n",
        "repeat\n",
        "    set n to n + 1\n",
        "    skip\n",
        "until n > 1000000\n",
    );

    let tree = assert_agrees_capped(source, 5);
    let byte = bytecode_capped(source, 5);

    let message = tree
        .result
        .as_ref()
        .expect_err("a loop whose every turn is skipped must still be bounded");
    assert!(
        message.contains("Maximum of 5 iterations"),
        "the per-loop cap must be what stops it, said: {message}"
    );
    assert!(
        message.contains("'repeat' loop"),
        "and it must name the kind of loop it stopped, said: {message}"
    );
    assert!(
        byte.result.is_err(),
        "the bytecode VM must be stopped by the same cap, said: {byte:?}"
    );

    // A skipped turn is charged like any other, so a cap of four refuses the
    // fourth turn's body — and the turn the cap stops is the one that would have
    // been skipped, which is the whole point of the shape.
    let tree = assert_agrees_capped(source, 4);
    let message = tree
        .result
        .as_ref()
        .expect_err("a cap one turn short must stop this loop too");
    assert!(
        message.contains("Maximum of 4 iterations"),
        "a cap of four must be four turns and no more, said: {message}"
    );
}

/// Both spellings of the post-test loop parse, and a `repeat` that has no `until`
/// says so.
///
/// `repeat` opens the counted form and this one, and the two are told apart by
/// what follows the keyword: a newline for a body of its own lines, and on one
/// line whichever of `times` and `until` is written first. A `repeat` with nothing
/// after it is this form that never got its `until`, so that is what the parser
/// says rather than an error about a count that was never written.
#[test]
fn edge_the_one_line_form_runs_and_a_bare_repeat_says_it_has_no_until() {
    let one_line = "set n to 0\nrepeat set n to n + 1 until n is 3\nsay n\n";
    let outcome = assert_agrees(one_line);
    assert_eq!(
        outcome.output,
        vec!["3".to_string()],
        "`repeat set n to n + 1 until n is 3` is the post-test loop written on \
         one line, and must run like the multi-line spelling"
    );

    // The formatter's spelling of it is the multi-line one, and formatting must
    // be a round trip in meaning as well as in text.
    let formatted_source = formatted(one_line);
    assert_eq!(
        formatted_source, "set n to 0\nrepeat\n    set n to n + 1\nuntil n is 3\nsay n\n",
        "the formatter writes the loop across the lines the grammar gives it"
    );
    let outcome = assert_agrees(&formatted_source);
    assert_eq!(
        outcome.output,
        vec!["3".to_string()],
        "and the formatted program must still print what the one-liner printed"
    );

    // The counted form keeps its own disambiguation: `times` settles it, and an
    // `until` inside a loop further down the file cannot reach back up and
    // change what this `repeat` opens.
    let counted = concat!(
        "set total to 0\n",
        "repeat 2 times\n",
        "    set n to 0\n",
        "    repeat\n",
        "        set n to n + 1\n",
        "    until n is 2\n",
        "    set total to total + n\n",
        "end\n",
        "say total\n",
    );
    let outcome = assert_agrees(counted);
    assert_eq!(
        outcome.output,
        vec!["4".to_string()],
        "a counted loop whose body holds a post-test loop is still the counted \
         form: the `times` on its own line is what says so"
    );

    for source in [
        "set n to 0\nrepeat",
        "set n to 0\nrepeat\n    set n to n + 1\n",
    ] {
        let outcome = assert_agrees(source);
        let message = outcome
            .result
            .as_ref()
            .expect_err("a `repeat` with no `until` must be refused");
        assert!(
            message.contains("ParserError"),
            "{source:?} must be refused by the parser, said: {message}"
        );
        assert!(
            message.contains("`until"),
            "and the diagnostic must name the keyword the loop is missing rather \
             than reporting whatever token it found in its place, said: {message}"
        );
        assert!(
            !message.contains("panic"),
            "and refused rather than crashed, said: {message}"
        );
    }
}
