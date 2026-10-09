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
    // The guard is a boundary, not an escape hatch: the call still hits the
    // call-depth limit rather than running until the host stack gives out, and
    // the limit is a clean failure the guard then discards. Without the limit
    // this program would not return at all.
    both_print(
        "to endless(n)\n\
         \x20   give back endless(n + 1)\n\
         end\n\
         \x20\n\
         set stopped to might fail endless(1)\n\
         say stopped\n\
         say \"alive\"",
        &["nothing", "alive"],
    );
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
    let mut chunk = compile_source("say might fail now()\n").expect("program should compile");
    for instruction in &mut chunk.main.code {
        if instruction.opcode == redblue::bytecode::Opcode::MightFail {
            instruction.arg = 9999;
        }
    }
    let mut vm = BytecodeVm::new();
    vm.set_echo(false);
    let error = vm
        .run(&chunk)
        .expect_err("an out-of-block recovery must be refused");
    assert_eq!(error.label(), "RuntimeError", "got {error:?}");
    assert!(
        error.message().contains("outside its block"),
        "the refusal must say why, got: {}",
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
         end\n\
         \x20\n\
         try\n\
         \x20   say might fail risky()\n\
         \x20   set bad to files.read(\"/nope/also-never\")\n\
         catch error\n\
         \x20   say \"caught\"\n\
         end",
        &["nothing", "caught"],
    );
}
