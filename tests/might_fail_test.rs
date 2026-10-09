//! `might fail <call>` — the expression form that discards a failure.
//!
//! `might fail` was reserved by the lexer and read by nothing: `TokenKind::MightFail`
//! appears in no `parse_*` method, so `set data to might fail files.read("x")` was a
//! `ParserError: Unexpected token MightFail`. SPEC.md uses the form as an expression
//! prefix, which is the only place a compiler can put an `emit` or a `write` that is
//! allowed to fail.
//!
//! Every case here runs through **both** engines — the tree-walking VM and the
//! bytecode compiler plus the bytecode VM — because a form that works on one and is a
//! `ParserError` on the other is not a language feature, it is an accident of one
//! frontend.

use redblue::bytecode::vm::BytecodeVm;
use redblue::bytecode::Instruction;
use redblue::bytecode::MIGHT_FAIL_END_MARKER;
use redblue::{compile_source, run_isolated, Error, Value};

/// What one engine made of a program: the lines it printed, and either its value
/// or the failure it reported.
#[derive(Debug, Clone)]
struct Outcome {
    output: Vec<String>,
    result: Result<Value, Error>,
}

fn tree_walk(source: &str) -> Outcome {
    let failed = |error: Error| Outcome {
        output: Vec::new(),
        result: Err(error),
    };
    let tokens = match redblue::lexer::Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => return failed(error),
    };
    let ast = match redblue::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(error) => return failed(error),
    };
    if let Err(error) = redblue::analyzer::analyze(&ast) {
        return failed(error);
    }
    let (mut vm, result) = run_isolated(&ast);
    Outcome {
        output: vm.take_output(),
        result,
    }
}

fn bytecode(source: &str) -> Outcome {
    let chunk = match compile_source(source) {
        Ok(chunk) => chunk,
        Err(error) => {
            return Outcome {
                output: Vec::new(),
                result: Err(error),
            }
        }
    };
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let result = vm.run(&chunk);
    Outcome {
        output: vm.take_output(),
        result,
    }
}

/// [`tree_walk`] with both resource limits brought down.
///
/// The published limits are far above what a test may spend, so a program that
/// has to *reach* one is not a test. These build the VM with the limits set
/// directly rather than reading the environment, which is shared with every other
/// test running in parallel — the same reason `tests/bytecode_vm_test.rs` does it.
fn tree_walk_bounded(source: &str, max_iterations: usize, max_steps: usize) -> Outcome {
    let failed = |error: Error| Outcome {
        output: Vec::new(),
        result: Err(error),
    };
    let tokens = match redblue::lexer::Lexer::tokenize(source) {
        Ok(tokens) => tokens,
        Err(error) => return failed(error),
    };
    let ast = match redblue::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(error) => return failed(error),
    };
    if let Err(error) = redblue::analyzer::analyze(&ast) {
        return failed(error);
    }
    let mut vm = redblue::Vm::with_limits(max_iterations, max_steps);
    let result = vm.run(&ast);
    Outcome {
        output: vm.take_output(),
        result,
    }
}

/// [`bytecode`] with both resource limits brought down.
fn bytecode_bounded(source: &str, max_iterations: usize, max_steps: usize) -> Outcome {
    let chunk = match compile_source(source) {
        Ok(chunk) => chunk,
        Err(error) => {
            return Outcome {
                output: Vec::new(),
                result: Err(error),
            }
        }
    };
    let mut vm = BytecodeVm::with_limits(max_iterations, max_steps);
    vm.set_echo(false);
    let result = vm.run(&chunk);
    Outcome {
        output: vm.take_output(),
        result,
    }
}

/// One program, run on both engines, with the engines' agreement asserted.
///
/// A helper that ran one engine would let a divergence pass as a pass, so this is
/// the only way a case in this file reaches an engine at all. The comparison is
/// over the printed lines and the label, message and line of any failure: the
/// column is compared only where both engines can carry one, because a compiled
/// instruction has a line and no column.
#[track_caller]
fn both(source: &str) -> (Outcome, Outcome) {
    let tree = tree_walk(source);
    let byte = bytecode(source);
    assert!(
        agrees(&tree, &byte),
        "the two engines disagree about:\n{}\n  tree-walking: {:?}\n  bytecode:     {:?}",
        source,
        tree,
        byte
    );
    (tree, byte)
}

fn agrees(a: &Outcome, b: &Outcome) -> bool {
    if a.output != b.output {
        return false;
    }
    match (&a.result, &b.result) {
        (Ok(x), Ok(y)) => x == y,
        (Err(x), Err(y)) => {
            x.label() == y.label()
                && x.message() == y.message()
                && x.span().map(|s| s.line) == y.span().map(|s| s.line)
        }
        _ => false,
    }
}

/// What both engines printed, asserted line for line.
#[track_caller]
fn both_print(source: &str, expected: &[&str]) {
    let (tree, byte) = both(source);
    assert_eq!(
        tree.output, expected,
        "tree-walking output for:\n{}",
        source
    );
    assert_eq!(byte.output, expected, "bytecode output for:\n{}", source);
}

/// The value both engines ended with, asserted.
#[track_caller]
fn both_value(source: &str, expected: Value) {
    let (tree, byte) = both(source);
    assert_eq!(
        tree.result.as_ref().ok(),
        Some(&expected),
        "tree-walking value for:\n{}",
        source
    );
    assert_eq!(
        byte.result.as_ref().ok(),
        Some(&expected),
        "bytecode value for:\n{}",
        source
    );
}

/// The failure both engines reported, asserted as `(label, message-substring)` and
/// as carrying a position.
#[track_caller]
fn both_fail(source: &str, label: &str, message: &str) {
    let (tree, byte) = both(source);
    let failure =
        tree.result.as_ref().err().unwrap_or_else(|| {
            panic!("`{}` was expected to fail, but it ran:\n{:?}", source, tree)
        });
    assert_eq!(failure.label(), label, "wrong label for:\n{}", source);
    assert!(
        failure.message().contains(message),
        "message {:?} does not name {:?} for:\n{}",
        failure.message(),
        message,
        source
    );
    // `Error::Io` carries no position at all — it is the one variant that
    // cannot, because the failure is about the host rather than about a place in
    // the source — so the assertion applies to the other four labels.
    if label != "IoError" {
        assert!(
            failure.span().is_some(),
            "`{}` failed without a position",
            source
        );
    }
    assert_eq!(
        byte.result.as_ref().err().map(|e| e.label()),
        Some(label),
        "bytecode label for:\n{}",
        source
    );
}

// ---------------------------------------------------------------------------
// The failing path
// ---------------------------------------------------------------------------

#[test]
fn failing_call_is_discarded_and_the_program_survives() {
    both_print(
        "set data to might fail files.read(\"/nope/does-not-exist\")\nsay data\nsay \"alive\"",
        &["nothing", "alive"],
    );
}

#[test]
fn a_succeeding_call_yields_its_value_not_nothing() {
    // Written into `target/tmp/` by the test itself, so it is a real file and the
    // assertion is about the success path rather than about the failure path twice.
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/might-fail");
    std::fs::create_dir_all(&dir).expect("scratch directory");
    let path = dir.join("present.txt");
    std::fs::write(&path, "written bytes").expect("scratch file");

    let source = format!(
        "set data to might fail files.read(\"{}\")\nsay data",
        path.display()
    );
    both_print(&source, &["written bytes"]);
}

#[test]
fn a_failing_call_yields_nothing_rather_than_aborting() {
    both_value(
        "might fail files.read(\"/nope/does-not-exist\")",
        Value::Nothing,
    );
}

// ---------------------------------------------------------------------------
// Position: not only in a `set`
// ---------------------------------------------------------------------------

#[test]
fn inside_a_for_each_body_the_loop_continues() {
    both_print(
        "for each path in [\"/nope/one\", \"/nope/two\", \"/nope/three\"]\n\
         \x20   set got to might fail files.read(path)\n\
         \x20   say got\n\
         end\n\
         say \"loop finished\"",
        &["nothing", "nothing", "nothing", "loop finished"],
    );
}

#[test]
fn edge_empty_argument_list_is_still_a_call() {
    // The boundary case of the "not a call" rule: `might fail now()` has no
    // arguments at all, and is a call nonetheless.
    both_print("set clock to might fail now()\nsay clock", &["nothing"]);
}

#[test]
fn edge_a_might_fail_whose_argument_itself_raises_is_discarded() {
    // The failure happens while the *arguments* are being evaluated, before the
    // call is made. It is still a failure of the guarded expression, and the
    // program carries on.
    both_print(
        "to boom(n)\n\
         \x20   give back upper(n)\n\
         end\n\
         \x20\n\
         set label to might fail boom(might fail files.read(\"/nope/none\"))\n\
         say label\n\
         say \"alive\"",
        &["nothing", "alive"],
    );
}

// ---------------------------------------------------------------------------
// A guarded call inside a larger expression
// ---------------------------------------------------------------------------

#[test]
fn a_guarded_call_is_an_expression_wherever_a_value_is_taken() {
    // `might fail <call>` is an expression, not a statement, so it can be an
    // argument, a list element, a record value, or a parenthesized operand of a
    // larger expression. Each of these leaves exactly one value behind it, which is
    // what `a_guard_leaves_one_value_on_the_operand_stack` pins from the bytecode
    // side.
    both_print(
        "set l to [might fail files.read(\"/nope/x\"), 7]\n\
         say l",
        &["[nothing, 7]"],
    );
}

#[test]
fn edge_a_guarded_call_as_a_record_value_does_not_disturb_its_neighbours() {
    // The boundary of the one-value rule on the side where it is easiest to get
    // wrong: a record's keys and values are read positionally off the operand
    // stack, so a guard that left a second value behind would shift every value
    // after it and build the record out of the wrong pairs — a stray `nothing`
    // where a key belongs, and a `RuntimeError` about a key that was not text.
    both_print(
        "set r to {a: might fail files.read(\"/nope/x\"), b: 2}\n\
         say r",
        &["{a: nothing, b: 2}"],
    );
}

#[test]
fn a_guard_leaves_one_value_on_the_operand_stack() {
    // The bytecode VM keeps an operand stack, and a guard that pushed its
    // `nothing` twice would not be caught by a `set`: `STORE` pops one value and
    // the extra stays below it, invisible to everything afterwards in the same
    // statement. It is caught where a value is read back *positionally* — a
    // record's keys and values, a list's elements — because a stray shifts every
    // value after it.
    //
    // Repeated, because one stray is a wrong value and 500 of them are a stack
    // that has grown by 500: a per-call leak is invisible until something reads
    // the stack by position, and the read has to be past all of them.
    let mut source = String::from("{");
    for index in 0..500 {
        if index > 0 {
            source.push_str(", ");
        }
        source.push_str(&format!(
            "k{index}: might fail files.read(\"/nope/{index}\")"
        ));
    }
    source.push('}');

    let chunk = compile_source(&source).expect("program should compile");
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let result = vm
        .run(&chunk)
        .expect("500 discarded failures must still run and still build their record");

    // Every value was read back at its own position, so each key must have found
    // its own `nothing`. A stray before a key would put a `nothing` where the key
    // belongs, and the record builder reports that rather than building nonsense.
    let fields = match &result {
        Value::Record(fields) => fields,
        other => panic!("expected a record, got {other:?}"),
    };
    assert_eq!(fields.len(), 500, "every key must have kept its own value");
    for index in 0..500 {
        assert_eq!(
            fields.get(&format!("k{index}")),
            Some(&Value::Nothing),
            "key k{index} did not get its own guarded value"
        );
    }
}

// ---------------------------------------------------------------------------
// The one thing that must NOT be accepted
// ---------------------------------------------------------------------------

#[test]
fn edge_non_call_right_hand_side_is_a_spanned_parser_error() {
    let source = "set x to might fail 1 + 1";
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    let error = redblue::parser::parse(tokens).expect_err("`might fail 1 + 1` must not parse");
    match error {
        redblue::Error::Parser(message, span) => {
            assert!(
                message.contains("might fail"),
                "the message must name the form, got: {message}"
            );
            assert_eq!(span.line, 1, "the error must name the line it is on");
            assert!(
                span.column > 1,
                "the error must name a column, got {}",
                span.column
            );
        }
        other => panic!("expected a Parser error, got {other:?}"),
    }
    // Both engines refuse it the same way, and neither runs the program.
    both_fail(source, "ParserError", "might fail");
}

#[test]
fn edge_a_bare_number_after_might_fail_is_refused_too() {
    both_fail("set x to might fail 7", "ParserError", "might fail");
}

#[test]
fn might_fail_without_an_argument_is_refused() {
    both_fail("set x to might fail", "ParserError", "might fail");
}

#[test]
fn edge_a_lone_might_prefix_is_refused() {
    // The lexer gives `might` and `fail` the same token, so the two-word spelling
    // arrives as two of them and the parser has to insist on both. A prefix that
    // accepted either half alone would turn `might x` — or `fail x` — into a guard
    // nobody wrote, so both are refused with the pair named.
    both_fail("set x to might upper(\"hi\")", "ParserError", "might fail");
}

#[test]
fn edge_a_lone_fail_prefix_is_refused() {
    both_fail("set x to fail upper(\"hi\")", "ParserError", "might fail");
}

// ---------------------------------------------------------------------------
// Interaction with the rest of the language
// ---------------------------------------------------------------------------

#[test]
fn a_user_function_that_fails_is_discarded_by_the_call_site() {
    both_print(
        "to risky()\n\
         \x20   give back files.read(\"/nope/never\")\n\
         end\n\
         \x20\n\
         set got to might fail risky()\n\
         say got",
        &["nothing"],
    );
}

#[test]
fn a_user_function_that_succeeds_is_not_discarded() {
    both_print(
        "to fine(n)\n\
         \x20   give back n * 2\n\
         end\n\
         \x20\n\
         say might fail fine(21)",
        &["42"],
    );
}

#[test]
fn a_guard_does_not_stop_a_later_failure_from_being_reported() {
    // `might fail` discards *its own* failure and nothing else: the program below
    // discards one failure and then still aborts on the next one.
    both_fail(
        "set ok to might fail files.read(\"/nope/none\")\n\
         set bad to files.read(\"/nope/also-none\")",
        "IoError",
        "Failed to read",
    );
}

#[test]
fn edge_a_try_with_no_catch_inside_a_guarded_call_still_lets_the_guard_take_it() {
    // The ordering case that a `try` with no `catch` creates. A `try` with no
    // `catch` is not a handler: it runs the `finally` it is owed and passes the
    // failure on to whatever is written around it. The guard here is written
    // around the call, so the `try` is *inside* the guard — it must be asked
    // first, decline to handle the failure, and hand it on, and the guard must
    // then be asked again rather than being skipped by a search that had already
    // passed it. Getting that wrong ends the program where the tree-walking VM
    // prints `nothing` and carries on.
    both_print(
        "to inner()\n\
         \x20   try\n\
         \x20       set v to files.read(\"/nope/x\")\n\
         \x20   finally\n\
         \x20       say \"cleaned\"\n\
         \x20   end\n\
         \x20   give back v\n\
         end\n\
         \x20\n\
         to outer()\n\
         \x20   give back inner()\n\
         end\n\
         \x20\n\
         say might fail outer()\n\
         say \"alive\"",
        &["cleaned", "nothing", "alive"],
    );
}

#[test]
fn edge_a_guard_inside_a_try_does_not_eat_the_try() {
    // A `catch` written around a `might fail` still sees the failures that are not
    // guarded, which is what makes the two forms composable.
    both_print(
        "try\n\
         \x20   set ok to might fail files.read(\"/nope/none\")\n\
         \x20   say ok\n\
         \x20   set bad to files.read(\"/nope/also-none\")\n\
         catch error\n\
         \x20   say \"caught\"\n\
         end",
        &["nothing", "caught"],
    );
}

#[test]
fn nested_guards_are_each_discardable() {
    both_print(
        "set outer to might fail files.read(\"/nope/outer\")\n\
         set inner to might fail files.read(\"/nope/inner\")\n\
         say outer\n\
         say inner",
        &["nothing", "nothing"],
    );
}

// ---------------------------------------------------------------------------
// The guard is general: it discards any failure of the call it wraps
// ---------------------------------------------------------------------------

#[test]
fn edge_a_type_mismatch_inside_the_guarded_call_is_discarded() {
    // `uppercase(1)` is a number where text is expected — a `Runtime` failure, not
    // the `Io` failure the earlier cases use, so this pins that the guard is not
    // a "file errors are survivable" special case.
    both_print(
        "say might fail uppercase(1)\nsay \"alive\"",
        &["nothing", "alive"],
    );
}

#[test]
fn edge_an_out_of_bounds_index_inside_the_guarded_call_is_discarded() {
    both_print(
        "to pick(items)\n\
         \x20   give back items[5]\n\
         end\n\
         \x20\n\
         say might fail pick([1])",
        &["nothing"],
    );
}

#[test]
fn edge_a_missing_record_field_inside_the_guarded_call_is_discarded() {
    both_print(
        "to field(record)\n\
         \x20   give back record.missing\n\
         end\n\
         \x20\n\
         say might fail field({a: 1})",
        &["nothing"],
    );
}

#[test]
fn edge_division_by_zero_inside_the_guarded_call_is_discarded() {
    both_print("say might fail divide(1, 0)", &["nothing"]);
}

#[test]
fn edge_a_guarded_call_that_would_recurse_without_limit_stops_at_the_limit() {
    // The guard is a boundary, not an escape hatch, in both directions. The call
    // still hits the call-depth limit rather than running until the host stack
    // gives out, and the limit is *reported* rather than discarded: a program
    // stopped by the host has not carried on, and turning that into `nothing`
    // would report a runaway recursion as a successful value.
    both_fail(
        "to endless(n)\n\
         \x20   give back endless(n + 1)\n\
         end\n\
         \x20\n\
         set stopped to might fail endless(1)\n\
         say stopped\n\
         say \"alive\"",
        "RuntimeError",
        "Maximum call depth",
    );
}

#[test]
fn edge_a_step_budget_spent_inside_the_guarded_call_is_reported() {
    // The step budget is the second resource limit, and it is the one a guard is
    // most likely to swallow by accident: the budget is charged at a statement
    // marker, and a guarded call's body is full of statements. Lowering the
    // budget to a handful of steps puts the exhaustion inside the guarded call,
    // where the guard would otherwise yield `nothing` and the program would carry
    // on having run out of budget.
    let source = "to spend(n)\n\
         \x20   say n\n\
         \x20   say n\n\
         \x20   say n\n\
         \x20   say n\n\
         \x20   say n\n\
         end\n\
         \x20\n\
         set got to might fail spend(1)\n\
         say \"alive\"";

    let tree = tree_walk_bounded(source, 1_000, 4);
    let byte = bytecode_bounded(source, 1_000, 4);
    for (engine, outcome) in [("tree-walking", &tree), ("bytecode", &byte)] {
        let failure = outcome.result.as_ref().err().unwrap_or_else(|| {
            panic!("the {engine} engine must report the limit, got {outcome:?}")
        });
        assert_eq!(
            failure.label(),
            "RuntimeError",
            "wrong label from the {engine} engine: {failure:?}"
        );
        assert!(
            failure.message().contains("Step budget"),
            "the {engine} engine must name the budget, got: {}",
            failure.message()
        );
    }
    assert_eq!(
        tree.output, byte.output,
        "the two engines must stop at the same point"
    );
    assert!(
        !tree.output.contains(&"alive".to_string()),
        "the program must not have carried on past its budget: {:?}",
        tree.output
    );
}

#[test]
fn edge_an_iteration_cap_reached_after_the_guarded_call_is_reported() {
    // The third limit. It is reached *outside* any guard here, so this pins that
    // adding a guard to a loop's body did not change what stops the loop.
    let source = "set n to 0\n\
         repeat 100 times\n\
         \x20   set n to n + 1\n\
         \x20   set g to might fail files.read(\"/nope/x\")\n\
         end\n\
         say n";

    let tree = tree_walk_bounded(source, 3, 1_000_000);
    let byte = bytecode_bounded(source, 3, 1_000_000);
    for (engine, outcome) in [("tree-walking", &tree), ("bytecode", &byte)] {
        let failure =
            outcome.result.as_ref().err().unwrap_or_else(|| {
                panic!("the {engine} engine must report the cap, got {outcome:?}")
            });
        assert!(
            failure.message().contains("iterations"),
            "the {engine} engine must name the cap, got: {}",
            failure.message()
        );
    }
}

#[test]
fn edge_unicode_survives_a_guarded_call_that_succeeds() {
    both_print(
        "to shout(words)\n\
         \x20   give back uppercase(words) + \"!\"\n\
         end\n\
         \x20\n\
         say might fail shout(\"héllo 🌍 日本語\")",
        &["HÉLLO 🌍 日本語!"],
    );
}

#[test]
fn edge_a_failure_crossing_three_frames_reaches_the_guard() {
    // The guard is written in the outermost frame and the failure is raised in
    // the innermost one, so this is the case that decides whether unwinding out
    // of a call reaches a `might fail` at all — the same shape `?` unwinds to
    // reach a `try`.
    both_print(
        "to inner()\n\
         \x20   give back files.read(\"/nope/leaf\")\n\
         end\n\
         \x20
\
         to middle()\n\
         \x20   give back inner()\n\
         end\n\
         \x20
\
         to outer()\n\
         \x20   give back middle()\n\
         end\n\
         \x20
\
         say might fail outer()\n\
         say \"alive\"",
        &["nothing", "alive"],
    );
}

#[test]
fn the_statement_form_writes_a_file_it_is_allowed_not_to() {
    // `might fail <call>` is an expression, so it is also a statement — which is
    // how `SPEC.md` § Error Handling writes `files.write` and `files.append`.
    both_print(
        "might fail files.write(\"/nope/nowhere/out.txt\", \"body\")\nsay \"alive\"",
        &["alive"],
    );
}

#[test]
fn edge_a_file_naming_recovery_outside_its_block_is_refused_not_trusted() {
    // The operand of `MIGHT_FAIL` is written into the file, so a file can say
    // anything: a target past the end of the block must be a clean failure and
    // not an index that reads past the instruction list.
    let refuse = |arg: u32| {
        let mut chunk = compile_source("say might fail now()\n").expect("program should compile");
        for instruction in &mut chunk.main.code {
            if instruction.opcode == redblue::bytecode::Opcode::MightFail {
                instruction.arg = arg;
            }
        }
        let mut vm = BytecodeVm::new();
        vm.set_echo(false);
        vm.run(&chunk)
            .expect_err("an out-of-block recovery must be refused")
    };

    let error = refuse(9999);
    assert_eq!(error.label(), "RuntimeError", "got {error:?}");
    assert!(
        error.message().contains("outside its block"),
        "the refusal must say why, got: {}",
        error.message()
    );

    // The boundary case, which a `>` rather than a `>=` would have let through:
    // the last instruction of a block is at `len - 1`, so an offset of exactly
    // `len` names nothing at all. Accepting it only moved the problem —
    // `set_ip` clamps an out-of-range target to `len` and the guarded region then
    // ran to the end of the block as though nothing had failed, which is the
    // opposite of what the operand said.
    let chunk = compile_source("say might fail now()\n").expect("program should compile");
    let beyond_the_end = chunk.main.code.len() as u32;
    let error = refuse(beyond_the_end);
    assert_eq!(error.label(), "RuntimeError", "got {error:?}");
    assert!(
        error.message().contains("outside its block"),
        "an offset of exactly the block's length must be refused too, got: {}",
        error.message()
    );
}

#[test]
fn edge_a_region_marker_with_no_guard_does_nothing() {
    // A `NOP` carrying the region marker is data the compiler wrote; a file that
    // carries one where no guard was installed has no guard to pop, and must run
    // on rather than take a marker for a guard that is not there.
    let mut chunk = compile_source("say \"before\"\n").expect("program should compile");
    chunk.main.code.push(Instruction {
        opcode: redblue::bytecode::Opcode::Nop,
        arg: MIGHT_FAIL_END_MARKER,
        aux: 0,
        line: 1,
    });
    chunk.main.code.push(Instruction {
        opcode: redblue::bytecode::Opcode::PushConst,
        arg: 0,
        aux: 0,
        line: 1,
    });
    chunk.main.code.push(Instruction {
        opcode: redblue::bytecode::Opcode::Say,
        arg: 0,
        aux: 0,
        line: 1,
    });
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let result = vm.run(&chunk);
    assert!(
        result.is_ok(),
        "a stray marker must not fail the run: {result:?}"
    );
    assert_eq!(vm.take_output(), vec!["before", "before"]);
}

#[test]
fn edge_a_guard_does_not_swallow_a_failed_expect() {
    // An `expect` that fails is a test result, not an ordinary failure: the
    // harness reads it off the VM rather than off the error a run returns, so a
    // guard that discarded it would report a red test green. The guarded call
    // fails here, and the guard passes the failure on rather than eating it.
    let source = "to check(n)\n\
         \x20   if n is 1 then\n\
         \x20       expect n to be 2\n\
         \x20   end\n\
         \x20   give back n\n\
         end\n\
         \x20
\
         say might fail check(1)";
    both_fail(source, "RuntimeError", "Values not equal");
}

#[test]
fn edge_the_boundaries_of_a_singleton_reach_the_guard_intact() {
    // Both boundaries of a one-element list, on the path where nothing fails: a
    // guard that truncated or duplicated the value on the way through would
    // show up here as a wrong answer rather than as a swallowed error.
    both_print(
        "to pick(items)
\
         \x20   give back items[0]
\
         end\n\
         \x20
\
         say might fail pick([7])",
        &["7"],
    );
}

#[test]
fn edge_a_guard_inside_a_loop_does_not_survive_its_own_iteration() {
    // Every iteration installs and drops its own guard: the failure below is
    // raised *after* the loop and must still be reported, which it cannot be if
    // a guard from the loop's last turn were left behind.
    both_fail(
        "for each n in [1, 2, 3]\n\
         \x20   set got to might fail files.read(\"/nope/x\")\n\
         end\n\
         set bad to files.read(\"/nope/y\")",
        "IoError",
        "Failed to read",
    );
}

#[test]
fn a_guarded_call_that_raises_inside_a_try_still_lets_the_catch_see_later_failures() {
    both_print(
        "to risky()\n\
         \x20   give back files.read(\"/nope/never\")\n\
         \x20   end\n\
         \x20\n\
         \x20   try\n\
         \x20       say might fail risky()\n\
         \x20       set bad to files.read(\"/nope/also-never\")\n\
         \x20   catch error\n\
         \x20       say \"caught\"\n\
         \x20   end",
        &["nothing", "caught"],
    );
}

/// A resource limit raised inside a `might fail` is passed on rather than taken,
/// and the search for something that *can* take it goes on past the guard.
///
/// A guard declining is not a guard absent. The failure belongs to whatever is
/// written around the guard — an enclosing guard, a `catch`, or the program — and
/// answering "nothing handled this" the moment the innermost guard says no ends
/// the program where the tree-walking VM's `?` carries on into the enclosing
/// `try`. Here the limit is reached inside a guarded call that a `try` surrounds,
/// and the `catch` is what the program wrote to be told about it.
#[test]
fn a_resource_limit_a_guard_declines_reaches_the_catch_around_it() {
    let source = "to spin()\n\
         \x20   repeat 100 times\n\
         \x20       set n to 1\n\
         \x20   end\n\
         \x20   give back n\n\
         \x20   end\n\
         \x20\n\
         \x20   try\n\
         \x20       set reached to might fail spin()\n\
         \x20   catch error\n\
         \x20       say \"caught\"\n\
         \x20   end\n\
         \x20   say \"alive\"";

    let tree = tree_walk_bounded(source, 3, 1_000_000);
    let byte = bytecode_bounded(source, 3, 1_000_000);
    assert_eq!(
        tree.output,
        vec!["caught".to_string(), "alive".to_string()],
        "the tree-walking engine must hand the limit to the catch"
    );
    assert_eq!(
        byte.output, tree.output,
        "the bytecode engine must reach the same catch as the tree-walking one"
    );
    assert!(
        tree.result.is_ok() && byte.result.is_ok(),
        "a caught limit ends the program on neither engine: {tree:?} / {byte:?}"
    );
}

/// A failed `expect` inside a guard inside a `try` is the other thing a guard
/// declines, and it declines it for the same reason a limit does: neither is a
/// failure of the guarded expression that `nothing` can stand in for.
///
/// So it belongs to the `catch` written around the call. A search that stopped at
/// the guard's "no" would end the program here, where the tree-walking VM hands
/// the failure to the `catch` and carries on — and would leave the harness
/// holding the red result with a program that claims it never failed.
#[test]
fn a_failed_expect_a_guard_declines_reaches_the_catch_around_it() {
    let source = "to check()\n\
         \x20   expect 3 to be 4\n\
         \x20   give back 0\n\
         \x20   end\n\
         \x20\n\
         \x20   try\n\
         \x20       set reached to might fail check()\n\
         \x20   catch error\n\
         \x20       say \"caught\"\n\
         \x20   end\n\
         \x20   say \"alive\"";

    both_print(source, &["caught", "alive"]);
}

/// A limit is told apart from an ordinary failure by *what raised it*, not by how
/// its message reads.
///
/// A program cannot `raise` — there is no such form in the language — so the case
/// is reached through the one builtin whose message carries program text:
/// `to_number` refuses what it cannot read, and the text it refuses is the
/// program's own string. Spelling that string like a limit's message must not
/// turn the failure into one: the guard is written to discard a failure of its own
/// expression, and this failure is exactly that, so it is discarded and the
/// program carries on. Getting this wrong from the other direction is just as bad
/// — a limit that reads as an ordinary failure would be swallowed rather than
/// reported — which is why the question is asked of the variant rather than of the
/// wording.
#[test]
fn a_failure_whose_message_reads_like_a_limit_is_still_an_ordinary_failure() {
    let source = "to refuses(text)\n\
         \x20   give back to_number(text)\n\
         \x20   end\n\
         \x20\n\
         \x20   say might fail refuses(\"Step budget of 5 reached before the program finished\")\n\
         \x20   say \"alive\"";

    both_print(source, &["nothing", "alive"]);
}

/// A limit is the host refusing to keep going, so a guard passes one on rather
/// than discarding it — and it is told apart from a failure of its own expression
/// by the fact that the VM reached a limit, not by how the message reads.
///
/// The two cases here are the same words in opposite directions. A failure whose
/// message is spelled like a limit's is the program's own, and the guard written
/// for its expression takes it. A limit is the host's, and no guard takes it, so
/// this pins the two the bounded runs below can reach against a guard, on the
/// iteration cap and on the step budget. The third, the call-depth limit, is
/// pinned by [`edge_a_guarded_call_that_would_recurse_without_limit_stops_at_the_limit`].
#[test]
fn each_resource_limit_is_passed_on_rather_than_discarded() {
    for (label, source, iterations, steps) in [
        (
            "iteration cap",
            "to spin()\n\
             \x20   repeat 100 times\n\
             \x20       set n to 1\n\
             \x20   end\n\
             \x20   give back n\n\
             \x20   end\n\
             \x20   say might fail spin()",
            3usize,
            1_000_000usize,
        ),
        (
            "step budget",
            "to spend(n)\n\
             \x20   say n\n\
             \x20   say n\n\
             \x20   say n\n\
             \x20   say n\n\
             \x20   say n\n\
             \x20   end\n\
             \x20   say might fail spend(1)",
            1_000,
            4,
        ),
    ] {
        // Each is run with the limit it names brought down, so the program reaches
        // it rather than running to its natural end.
        for (engine, outcome) in [
            (
                "tree-walking",
                &tree_walk_bounded(source, iterations, steps),
            ),
            ("bytecode", &bytecode_bounded(source, iterations, steps)),
        ] {
            let failure = outcome.result.as_ref().err().unwrap_or_else(|| {
                panic!("the {engine} engine must report the {label}, got {outcome:?}")
            });
            assert_eq!(
                failure.label(),
                "RuntimeError",
                "the {label} is a runtime failure on the {engine} engine: {failure:?}"
            );
            assert!(
                !outcome.output.contains(&"nothing".to_string()),
                "the {engine} engine must not have turned the {label} into a value"
            );
        }
    }
}

/// A `might fail` never reports a failed `expect` green, however many have failed
/// before it.
///
/// The question a guard asks is whether the failure in hand is a test result that
/// *this* expression recorded, and a yes/no "has one been recorded" cannot answer
/// it once one has: a second failing `expect` inside a guard installed after the
/// first reads as "already asserted", so the guard discards it and the test is
/// reported green with a red assertion in it. Both engines must refuse, which is
/// what `both_fail` asserts — the failure leaves the program and the harness still
/// holds the result.
#[test]
fn a_second_failed_expect_inside_a_guard_is_still_reported() {
    let source = "to check()\n\
         \x20   expect 3 to be 4\n\
         \x20   give back 0\n\
         \x20   end\n\
         \x20\n\
         \x20   expect 1 to be 2\n\
         \x20   set reached to might fail check()\n\
         \x20   say \"alive\"";

    both_fail(source, "RuntimeError", "Values not equal");
}

/// The same, with the guard *between* the two assertions rather than after the
/// first: the guard is installed while no result is recorded, so nothing about it
/// can be inherited from outside, and the failing `expect` inside it must still
/// leave the program rather than be discarded.
#[test]
fn a_failed_expect_inside_a_guard_installed_before_any_assertion_is_reported() {
    both_fail(
        "to check()\n\
         \x20   expect 3 to be 4\n\
         \x20   give back 0\n\
         \x20   end\n\
         \x20\n\
         \x20   set reached to might fail check()\n\
         \x20   say \"alive\"",
        "RuntimeError",
        "Values not equal",
    );
}

/// A guard whose frame has finished does not guard anything that runs after it.
///
/// A guard is installed and dropped inside one expression, so the frame that
/// installed one is its innermost frame — but a frame does not only finish by
/// running its last instruction. A region left by a jump (a `break` out of a loop
/// the guarded call is in) never reaches the marker that drops the guard, so a
/// frame can finish with its own guard still installed.
///
/// The frame that just finished is at index `finished`, which is the *length*
/// after the pop, so the bound that drops its guard is `>=` and not `>`. With
/// `>` the stale entry survives above every live guard, where the marker — which
/// checks only the top — can no longer pop it, and the next failure raised in a
/// frame entered at the same depth meets a guard whose recovery runs in a block
/// that no longer exists.
///
/// The compiler never writes such a file — a marker is emitted for every guard it
/// installs — so the case is built by taking the marker out of the compiled block,
/// which is the state a frame is left in by the jump this describes. Without the
/// fix the leftover guard swallows the second failure and leaves the operand stack
/// short, and the run reports `bytecode asked for 1 values its frame never pushed`
/// instead of the failure the callee raised.
#[test]
fn a_guard_whose_frame_finished_does_not_swallow_the_next_call_s_failure() {
    fn strip_markers(block: &mut redblue::bytecode::Block, removed: &mut usize) {
        block.code.retain(|instruction| {
            let marker = instruction.opcode == redblue::bytecode::Opcode::Nop
                && instruction.arg == MIGHT_FAIL_END_MARKER;
            if marker {
                *removed += 1;
            }
            !marker
        });
        for child in block.blocks.iter_mut() {
            strip_markers(child, removed);
        }
    }

    let source = "to ok()\n\
         \x20   give back 5\n\
         \x20   end\n\
         \x20   to guarded()\n\
         \x20       set kept to might fail ok()\n\
         \x20   end\n\
         \x20   to fails_after()\n\
         \x20       give back files.read(\"/nope/later\")\n\
         \x20   end\n\
         \x20   say guarded()\n\
         \x20   say fails_after()";

    let mut chunk = compile_source(source).expect("program should compile");
    let mut removed = 0;
    strip_markers(&mut chunk.main, &mut removed);
    assert_eq!(removed, 1, "the file must carry the one marker");

    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let failure = vm
        .run(&chunk)
        .expect_err("the later failure must still be reported")
        .to_string();
    assert!(
        failure.contains("Failed to read"),
        "the failure the callee raised must reach the program, got: {failure}"
    );
    assert_eq!(
        vm.take_output(),
        vec!["5"],
        "the guarded call's own value is unaffected"
    );
}
