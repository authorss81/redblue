//! The VM must bound the call depth.
//!
//! Without a counter, a user function that calls itself recurses in Rust until
//! the process aborts with `fatal runtime error: stack overflow` and exit code
//! 134 — an uncatchable abort, not a `RuntimeError`. With the counter, the same
//! program fails with a `RuntimeError` naming the function and the limit, which
//! `try ... catch error` can observe and which exits 1.

use std::path::PathBuf;
use std::process::Command;

use redblue::Error;
use redblue::MAX_CALL_DEPTH;

/// A scratch directory inside `target/`, so cross-process tests never write
/// outside the project checkout.
fn scratch_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/call-depth");
    std::fs::create_dir_all(&dir).expect("scratch directory should be creatable");
    dir
}

/// Lexes and parses `source`.
#[track_caller]
fn parse(source: &str) -> redblue::parser::Program {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    redblue::parser::parse(tokens).expect("source should parse")
}

/// Runs `source` on a stack sized for the call depth and returns the value of
/// its last statement.
#[track_caller]
fn eval(source: &str) -> redblue::Value {
    let (_vm, result) = redblue::run_isolated(&parse(source));
    result.unwrap_or_else(|error| panic!("source should have run, failed with {:?}", error))
}

/// Runs `source` and returns the error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    let (_vm, result) = redblue::run_isolated(&parse(source));
    result.expect_err("source should have failed")
}

/// Writes `source` to its own file inside a scratch directory and runs it in a
/// child process. The file name is unique per call so tests running in parallel
/// cannot read each other's program.
fn run_in_child(name: &str, source: &str, env: Option<(&str, &str)>) -> (i32, String, String) {
    let path = scratch_dir().join(format!("{}.rb", name));
    std::fs::write(&path, source).expect("program should be writable");

    let mut command = Command::new(env!("CARGO_BIN_EXE_rb"));
    command.arg("run").arg(&path);
    if let Some((key, value)) = env {
        command.env(key, value);
    }
    let out = command.output().expect("child process should run");
    (
        out.status
            .code()
            .unwrap_or_else(|| panic!("child should exit normally, not be signalled")),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// A program that recurses `depth` times, i.e. whose deepest active frame is
/// `depth + 1`. Terminates on `n is 0`.
///
/// `reached` is declared at the top level because the analyzer only accepts a
/// variable it has already seen, and `give back` inside an `if` branch does not
/// return from the function — so the recursive call is assigned, not returned.
fn countdown(depth: usize) -> String {
    format!(
        "set reached to 0\n\nto countdown(n)\n    if n is 0 then\n        set reached to 0\n    else\n        set reached to countdown(n - 1)\n    end\n    give back reached\nend\n\ncountdown({})\n",
        depth
    )
}

/// Unbounded self-recursion must be a `RuntimeError`, not a stack overflow.
#[test]
fn infinite_recursion_is_a_runtime_error_not_a_stack_overflow() {
    let err = eval_err("to boom(n)\n    boom(n + 1)\nend\n\nboom(0)\n");

    assert!(
        // `err.label()`, not a match on the variant: a resource limit is
        // carried by its own variant so `might fail` can tell it apart, but it
        // is a runtime failure and labels as one, which is what this asserts.
        err.label() == "RuntimeError",
        "expected RuntimeError, got {:?}",
        err
    );
    let message = err.to_string();
    assert!(
        message.contains("boom"),
        "message should name the function, got {:?}",
        message
    );
    assert!(
        message.contains(&MAX_CALL_DEPTH.to_string()),
        "message should name the limit {}, got {:?}",
        MAX_CALL_DEPTH,
        message
    );
}

/// The child process must exit 1 with a rendered error. Exit 134 means the
/// process aborted on a stack overflow, which is the defect being fixed.
#[test]
fn infinite_recursion_exits_one_in_a_child_process() {
    let (code, _stdout, stderr) = run_in_child(
        "exit-one",
        "to boom(n)\n    boom(n + 1)\nend\n\nboom(0)\n",
        None,
    );

    assert_eq!(
        code, 1,
        "expected a clean exit 1, got {} (stderr: {})",
        code, stderr
    );
    assert!(
        !stderr.contains("stack overflow"),
        "the process aborted instead of reporting an error: {}",
        stderr
    );
    assert!(
        stderr.contains("boom") && stderr.contains(&MAX_CALL_DEPTH.to_string()),
        "stderr should name the function and the limit, got {:?}",
        stderr
    );
}

/// Two functions calling each other recurse just as deeply as one calling
/// itself, and must be bounded by the same counter.
#[test]
fn mutual_recursion_across_two_functions_is_bounded() {
    let source = "to ping(n)\n    pong(n + 1)\nend\nto pong(n)\n    ping(n + 1)\nend\n\nping(0)\n";
    let err = eval_err(source);

    assert!(
        // `err.label()`, not a match on the variant: a resource limit is
        // carried by its own variant so `might fail` can tell it apart, but it
        // is a runtime failure and labels as one, which is what this asserts.
        err.label() == "RuntimeError",
        "expected RuntimeError, got {:?}",
        err
    );
    let message = err.to_string();
    assert!(
        message.contains("ping") || message.contains("pong"),
        "message should name the function that hit the limit, got {:?}",
        message
    );
}

/// Recursion that stops one frame short of the limit must still succeed and
/// return its value.
#[test]
fn depth_just_below_the_limit_still_succeeds() {
    let depth = MAX_CALL_DEPTH - 2;
    assert_eq!(
        eval(&countdown(depth)),
        redblue::Value::Number(0.0),
        "recursion of depth {} should succeed under a limit of {}",
        depth,
        MAX_CALL_DEPTH
    );
}

/// Exactly `MAX_CALL_DEPTH` active frames is legal; one more is not.
#[test]
fn edge_the_limit_itself_is_the_boundary() {
    assert_eq!(
        eval(&countdown(MAX_CALL_DEPTH - 1)),
        redblue::Value::Number(0.0),
        "{} active frames should be legal under a limit of {}",
        MAX_CALL_DEPTH,
        MAX_CALL_DEPTH
    );

    let err = eval_err(&countdown(MAX_CALL_DEPTH));
    assert!(
        // `err.label()`, not a match on the variant: a resource limit is
        // carried by its own variant so `might fail` can tell it apart, but it
        // is a runtime failure and labels as one, which is what this asserts.
        err.label() == "RuntimeError",
        "{} active frames should exceed a limit of {}, got {:?}",
        MAX_CALL_DEPTH + 1,
        MAX_CALL_DEPTH,
        err
    );
}

/// A caught depth error lets execution continue: the statement after the
/// `try ... catch error` block still runs.
#[test]
fn depth_error_is_catchable_by_try_catch() {
    let source = "to boom(n)\n    boom(n + 1)\nend\nto safe(v)\n    give back v + 1\nend\nset caught to no\ntry\n    boom(0)\ncatch error\n    set caught to yes\nend\nsay safe(1)\n";

    let (code, stdout, stderr) = run_in_child("catchable", source, None);
    assert_eq!(
        code, 0,
        "the error should have been caught (stderr: {})",
        stderr
    );
    assert_eq!(stdout, "2\n", "only the say after the catch should print");
}

/// `REDBLUE_MAX_CALL_DEPTH` overrides the limit, so a program that recurses
/// legally under the default limit is rejected under a smaller one.
#[test]
fn env_var_overrides_the_call_depth_limit() {
    let (code, _stdout, stderr) = run_in_child(
        "env-too-deep",
        &countdown(10),
        Some(("REDBLUE_MAX_CALL_DEPTH", "5")),
    );
    assert_eq!(
        code, 1,
        "depth 10 must fail under a limit of 5, got {} (stderr: {})",
        code, stderr
    );
    assert!(
        stderr.contains("5"),
        "stderr should report the configured limit, got {:?}",
        stderr
    );

    let (code, stdout, stderr) = run_in_child(
        "env-legal",
        &countdown(3),
        Some(("REDBLUE_MAX_CALL_DEPTH", "5")),
    );
    assert_eq!(
        code, 0,
        "depth 3 must succeed under a limit of 5 (stderr: {})",
        stderr
    );
    assert!(
        stdout.is_empty(),
        "the program prints nothing, got {:?}",
        stdout
    );
}

/// A limit of 0 is not a usable limit; it must fall back to the default rather
/// than making every function call illegal.
#[test]
fn edge_zero_limit_falls_back_to_the_default() {
    let source = "to double(v)\n    give back v * 2\nend\n\nsay double(21)\n";
    let (code, stdout, stderr) =
        run_in_child("zero-limit", source, Some(("REDBLUE_MAX_CALL_DEPTH", "0")));

    assert_eq!(code, 0, "a zero limit must be ignored (stderr: {})", stderr);
    assert_eq!(stdout, "42\n", "the call should still succeed");
}

/// Non-numeric and negative limits are rejected in favour of the default.
#[test]
fn edge_invalid_limits_fall_back_to_the_default() {
    let source = "to ping(n)\n    pong(n + 1)\nend\nto pong(n)\n    ping(n + 1)\nend\n\nping(0)\n";

    for bad in ["abc", "-1", "", "1.5"] {
        let (code, _stdout, stderr) = run_in_child(
            &format!("invalid-{}", bad.len()),
            source,
            Some(("REDBLUE_MAX_CALL_DEPTH", bad)),
        );
        assert_eq!(
            code, 1,
            "limit {:?} must be ignored and the default {} used",
            bad, MAX_CALL_DEPTH
        );
        assert!(
            stderr.contains(&MAX_CALL_DEPTH.to_string()),
            "limit {:?} should report the default {}, got {:?}",
            bad,
            MAX_CALL_DEPTH,
            stderr
        );
    }
}

/// Three nested scopes deep — a function calling a function calling a function
/// — must not be mistaken for deep recursion.
#[test]
fn edge_three_nested_calls_are_not_recursion() {
    let source = "to third(v)\n    give back v + 1\nend\nto second(v)\n    give back third(v)\nend\nto first(v)\n    give back second(v)\nend\n\nsay first(1)\n";

    let (code, stdout, stderr) = run_in_child("nested-calls", source, None);
    assert_eq!(code, 0, "nested calls are legal (stderr: {})", stderr);
    assert_eq!(stdout, "2\n");
}

/// The limit is configurable in process, without touching the environment.
#[test]
fn with_max_call_depth_bounds_a_program_without_the_environment() {
    let mut vm = redblue::Vm::with_max_call_depth(4);
    let program = parse("set reached to 0\nto countdown(n)\n    if n is 0 then\n        set reached to 0\n    else\n        set reached to countdown(n - 1)\n    end\n    give back reached\nend\n\ncountdown(9)\n");

    let err = vm
        .run(&program)
        .expect_err("a depth of 10 must exceed a limit of 4");
    assert!(
        // `err.label()`, not a match on the variant: a resource limit is
        // carried by its own variant so `might fail` can tell it apart, but it
        // is a runtime failure and labels as one, which is what this asserts.
        err.label() == "RuntimeError",
        "expected RuntimeError, got {:?}",
        err
    );
    assert!(
        err.to_string().contains('4'),
        "message should name the configured limit, got {:?}",
        err.to_string()
    );
}
