//! How many `BREAK` and `SKIP` instructions the bytecode VM actually executed.
//!
//! The rest of the loop-control matrix — `tests/loop_control_test.rs` — pins
//! what `break` and `skip` *do* by what a program prints. That is the right
//! check for behaviour, but it cannot distinguish a handler that ran from a
//! handler that never ran at all: a `break` that was decoded, dropped, and
//! happened to be written where the loop would have ended anyway prints exactly
//! the same lines as a `break` that ended the loop. The two programs differ, and
//! only the instruction count tells them apart.
//!
//! So these tests count the instructions the VM dispatched, which is a different
//! channel from the printed output, and read the count back through the public
//! [`redblue::bytecode::vm::BytecodeVm`] API. Both words are covered on both
//! engines: the tree-walking VM counts the `Statement::Break` and
//! `Statement::Skip` bodies it evaluated, the bytecode VM the opcodes it
//! dispatched, and a disagreement between the two is a disagreement about the
//! language rather than about the plumbing.

use redblue::bytecode::vm::BytecodeVm;
use redblue::bytecode::{compile_program, Opcode};
use redblue::parser::Program;
use redblue::{Error, Vm};

/// Lexes and parses `source`.
#[track_caller]
fn parse(source: &str) -> Program {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    redblue::parser::parse(tokens).expect("source should parse")
}

/// How many `break` and `skip` statements the tree-walking VM evaluated.
#[track_caller]
fn tree_counts(source: &str) -> (usize, usize) {
    let mut vm = Vm::new();
    vm.run(&parse(source))
        .unwrap_or_else(|e| panic!("the tree-walking VM must run the program: {e:?}"));
    vm.loop_control_counts()
}

/// How many `BREAK` and `SKIP` opcodes the bytecode VM dispatched.
#[track_caller]
fn bytecode_counts(source: &str) -> (usize, usize) {
    let chunk = compile_program(&parse(source)).expect("the program should compile");
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    vm.run(&chunk)
        .unwrap_or_else(|e| panic!("the bytecode VM must run the program: {e:?}"));
    vm.loop_control_counts()
}

/// Both engines' counts for one source, which is what every test below asserts
/// on: the language has one answer for how many times a `break` runs, whichever
/// VM is asked.
#[track_caller]
fn counts_on_both_engines(source: &str) -> (usize, usize) {
    let tree = tree_counts(source);
    let byte = bytecode_counts(source);
    assert_eq!(
        tree, byte,
        "the two VMs disagree about how many break/skip statements ran\n{source}"
    );
    tree
}

/// `for each i in [1, 2, 3]` with a `break` when `i` is 2 runs exactly one
/// `BREAK`.
///
/// One, not zero and not three: zero would mean the instruction was decoded and
/// dropped (the defect this file exists to catch), and three would mean the
/// `break` did not leave the loop.
#[test]
fn a_break_in_a_for_each_loop_dispatches_exactly_one_break() {
    let source = concat!(
        "for each i in [1, 2, 3]\n",
        "    if i is 2 then\n",
        "        break\n",
        "    end\n",
        "    say i\n",
        "end\n",
    );

    assert_eq!(
        counts_on_both_engines(source),
        (1, 0),
        "a break on the second of three values runs once and ends the loop"
    );
}

/// The `skip` companion: `skip` on the middle value runs exactly one `SKIP`, and
/// `BREAK` never runs at all — which is what "the loop continued" means at the
/// instruction level, and is not something the printed lines alone can say.
#[test]
fn a_skip_in_a_for_each_loop_dispatches_exactly_one_skip() {
    let source = concat!(
        "for each i in [1, 2, 3]\n",
        "    if i is 2 then\n",
        "        skip\n",
        "    end\n",
        "    say i\n",
        "end\n",
    );

    assert_eq!(
        counts_on_both_engines(source),
        (0, 1),
        "a skip on the second of three values runs once and the loop goes on"
    );
}

/// A `break` that ends the loop on its *first* value still dispatches once — the
/// count is a statement count, not a per-element one.
///
/// The mirror of the first test, and the one that separates a working `break`
/// from a dropped one in the shape that hides best: here the loop would have
/// printed `1` and stopped either way if the instruction were dropped, because
/// `break` is the last statement reached before the third turn prints. Only the
/// instruction count shows that a handler ran at all.
#[test]
fn a_break_on_the_first_value_still_dispatches_exactly_once() {
    let source = concat!(
        "for each i in [1, 2, 3]\n",
        "    if i is 1 then\n",
        "        break\n",
        "    end\n",
        "    say i\n",
        "end\n",
    );

    assert_eq!(
        counts_on_both_engines(source),
        (1, 0),
        "a break on the first of three values runs once, not once per value"
    );
}

/// A `skip` on the *last* value dispatches the `SKIP` and then the loop draws no
/// further turn, so the count is one rather than being scaled by the length of
/// the list. The boundary where "skip the turn" and "leave the loop" look the
/// same from the outside.
#[test]
fn edge_a_skip_on_the_final_value_dispatches_one_skip_and_no_break() {
    let source = concat!(
        "for each i in [1, 2, 3]\n",
        "    if i is 3 then\n",
        "        skip\n",
        "    end\n",
        "    say i\n",
        "end\n",
    );

    assert_eq!(
        counts_on_both_engines(source),
        (0, 1),
        "a skip on the last value runs once; there is no later turn to skip"
    );
}

/// A `break` in the inner loop dispatches one `BREAK`, and the outer loop's two
/// turns mean the inner body is entered twice — so the inner `break` is the only
/// thing that stops the second inner turn.
///
/// Two would mean the inner loop was not left; zero would mean the inner break
/// was dropped and the inner loop ran its full length both times.
#[test]
fn a_break_in_a_nested_loop_dispatches_one_break_and_the_outer_turns_continue() {
    let source = concat!(
        "set outer to 0\n",
        "for each a in [1, 2]\n",
        "    set outer to outer + 1\n",
        "    for each b in [1, 2, 3]\n",
        "        if b is 2 then\n",
        "            break\n",
        "        end\n",
        "    end\n",
        "end\n",
        "say outer\n",
    );

    assert_eq!(
        counts_on_both_engines(source),
        (2, 0),
        "each outer turn enters the inner loop and breaks it once: two breaks"
    );
}

/// Both words in one program, in one loop each, so the two counters are pinned
/// against a program that runs both — a pair of zeros here would be a `break` and
/// a `skip` that were both dropped, which is the original defect with the loop
/// bodies arranged not to notice.
#[test]
fn edge_both_words_in_one_program_each_dispatch_once() {
    let source = concat!(
        "for each i in [1, 2, 3, 4]\n",
        "    if i is 2 then\n",
        "        skip\n",
        "    end\n",
        "    if i is 4 then\n",
        "        break\n",
        "    end\n",
        "    say i\n",
        "end\n",
    );

    assert_eq!(
        counts_on_both_engines(source),
        (1, 1),
        "one skip on the second value and one break on the fourth"
    );
}

/// A `break` outside every loop dispatches the instruction once and is then
/// refused — the count rises even though the program fails, which is what makes
/// the refusal an *answer* rather than a silent no-op.
///
/// The refusal itself is asserted too, and it is a `RuntimeError`: a `break`
/// outside a loop must not be a panic, and must not be a clean exit.
#[test]
fn a_refused_break_still_dispatches_the_instruction_and_then_fails() {
    let chunk = compile_program(&parse("break")).expect("a bare break should compile");
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let result = vm.run(&chunk);

    let error = result.expect_err("a break outside every loop must be refused, not ignored");
    match &error {
        Error::Runtime(message, _) => assert!(
            message.contains("break"),
            "the refusal must name the statement, got {message:?}"
        ),
        other => panic!("a refused break must be a RuntimeError, got {other:?}"),
    }
    assert_eq!(
        vm.loop_control_counts(),
        (1, 0),
        "the refusal is made by the break handler, so the handler ran exactly once"
    );
}

/// The same for `skip`, reached through its own handler: it dispatches once and
/// is refused with a `RuntimeError` that names `skip`.
#[test]
fn edge_a_refused_skip_still_dispatches_the_instruction_and_then_fails() {
    let chunk = compile_program(&parse("skip")).expect("a bare skip should compile");
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let result = vm.run(&chunk);

    let error = result.expect_err("a skip outside every loop must be refused, not ignored");
    match &error {
        Error::Runtime(message, _) => assert!(
            message.contains("skip"),
            "the refusal must name the statement, got {message:?}"
        ),
        other => panic!("a refused skip must be a RuntimeError, got {other:?}"),
    }
    assert_eq!(
        vm.loop_control_counts(),
        (0, 1),
        "the refusal is made by the skip handler, so the handler ran exactly once"
    );
}

/// A refused `break` the program catches still counts: the instruction ran, the
/// handler refused it, and the `catch` turned the refusal into a value. A count
/// of zero here would mean the refusal came from somewhere the program cannot
/// reach — a compile-time check — which would be a different contract.
#[test]
fn edge_a_caught_break_still_dispatches_the_instruction() {
    let source = concat!(
        "try\n",
        "    break\n",
        "catch error\n",
        "    say \"caught\"\n",
        "end\n",
    );

    assert_eq!(
        counts_on_both_engines(source),
        (1, 0),
        "a caught refusal is still the handler having run once"
    );
}

/// An empty loop body carries no `break` and no `skip`, so nothing dispatches.
///
/// The zero case: a counter that reads zero because the program never reached
/// one, against a counter that reads zero because the words were dropped. Both
/// are zero here, and the printed output is what distinguishes them, which is
/// why this test is pinned to both.
#[test]
fn edge_a_loop_with_no_break_or_skip_dispatches_none() {
    let source = concat!(
        "for each i in []\n",
        "    say i\n",
        "end\n",
        "for each j in [1, 2, 3]\n",
        "    say j\n",
        "end\n",
    );

    assert_eq!(
        counts_on_both_engines(source),
        (0, 0),
        "a loop with neither word dispatches neither instruction"
    );
}

/// A `break` inside a function body is refused, and it is the *function's* call
/// that is refused rather than the loop around the call — so the loop's own
/// turns are not truncated and the loop runs to its end.
///
/// The count is one: the instruction was dispatched once, by the call, and the
/// handler refused it. Two would mean the loop ran its own `break` as well.
#[test]
fn edge_a_break_in_a_function_body_is_refused_once_and_the_caller_keeps_counting() {
    let source = concat!(
        "set turns to 0\n",
        "to leaky\n",
        "    break\n",
        "end\n",
        "for each i in [1, 2, 3]\n",
        "    try\n",
        "        leaky()\n",
        "    catch error\n",
        "        say \"caught\"\n",
        "    end\n",
        "    set turns to turns + 1\n",
        "end\n",
        "say turns\n",
    );

    assert_eq!(
        counts_on_both_engines(source),
        (3, 0),
        "each of the three turns calls the function and is refused once"
    );
}

/// The opcode table names both words, so the two the counter reports on are
/// instructions the compiler can actually emit — the count is not reading a
/// counter for an opcode that no program contains.
#[test]
fn edge_the_counted_opcodes_are_the_two_the_compiler_emits() {
    let chunk = compile_program(&parse(concat!(
        "for each i in [1, 2, 3]\n",
        "    if i is 2 then\n",
        "        break\n",
        "    end\n",
        "    if i is 3 then\n",
        "        skip\n",
        "    end\n",
        "end\n",
    )))
    .expect("the program should compile");

    let emitted: Vec<Opcode> = chunk
        .main
        .code
        .iter()
        .map(|instruction| instruction.opcode)
        .collect();

    assert!(
        emitted.contains(&Opcode::Break),
        "the compiler must emit BREAK for a break: {emitted:?}"
    );
    assert!(
        emitted.contains(&Opcode::Skip),
        "the compiler must emit SKIP for a skip: {emitted:?}"
    );
}
