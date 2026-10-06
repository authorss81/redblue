//! Every loop must be bounded, and the whole interpreter must be interruptible.
//!
//! Without a counter, a `while` whose condition never becomes false, or a
//! `repeat` with a huge count, runs until the operating system kills the
//! process. There is no limit to raise, no hook to observe and no way for a
//! test harness to bound a program: the only way to stop one is `kill -9`,
//! which `try ... catch error` cannot see and which no test can assert on.
//!
//! Two limits close that gap:
//!
//! - [`redblue::MAX_ITERATIONS`] caps the iterations of any single loop, so a
//!   runaway loop is a `RuntimeError` naming the limit that was set.
//! - a *step budget* caps the statements the VM executes in total, so a test
//!   harness can bound a whole program even when no one loop is the culprit.

use std::path::PathBuf;
use std::process::Command;

use redblue::Error;
use redblue::{Vm, MAX_ITERATIONS, MAX_ITERATIONS_ENV, MAX_STEPS};

/// A scratch directory inside `target/`, so cross-process tests never write
/// outside the project checkout.
fn scratch_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/loop-bounds");
    std::fs::create_dir_all(&dir).expect("scratch directory should be creatable");
    dir
}

/// Lexes and parses `source`.
#[track_caller]
fn parse(source: &str) -> redblue::parser::Program {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    redblue::parser::parse(tokens).expect("source should parse")
}

/// Runs `source` in a VM capped at `max_iterations` iterations per loop.
#[track_caller]
fn run_capped(source: &str, max_iterations: usize) -> Result<redblue::Value, Error> {
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
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

// ---------------------------------------------------------------------------
// The reproduction: loops that never finish
// ---------------------------------------------------------------------------

/// A `while` whose condition never changes must stop with a `RuntimeError`
/// rather than run until something kills the process. This is the finding: with
/// no cap, this program never returned and no test could assert anything about
/// it.
#[test]
fn infinite_while_loop_terminates_with_a_clean_error() {
    let source =
        "set count to 0\nset stopped to 0\nwhile stopped is 0\n    set count to count + 1\nend";

    let error = run_capped(source, 50).expect_err("a loop past its cap must fail, not hang");

    assert!(
        matches!(error, Error::Runtime(..)),
        "a runaway loop must be a RuntimeError, got {:?}",
        error
    );
    let message = runtime_message(&error);
    assert!(
        message.contains("50"),
        "the error must name the limit that was set, got {:?}",
        message
    );
}

/// A huge `repeat` count is the same defect: before the cap it ran for as long
/// as the host allowed.
#[test]
fn huge_repeat_count_fails_instead_of_running_forever() {
    let source = "set count to 0\nrepeat 500000000 times\n    set count to count + 1\nend";

    let error = run_capped(source, 100).expect_err("a repeat past the cap must fail");

    let message = runtime_message(&error);
    assert!(
        message.contains("100"),
        "the error must name the cap, got {:?}",
        message
    );
}

/// The same runaway loop as a real process must exit 1 with the limit in its
/// message, rather than hang until an operator kills it.
#[test]
fn runaway_loop_process_exits_instead_of_hanging() {
    let (code, stdout, stderr) = run_in_child(
        "runaway_while",
        "set count to 0\nset stopped to 0\nwhile stopped is 0\n    set count to count + 1\nend\nsay \"finished\"\n",
        Some((MAX_ITERATIONS_ENV, "500")),
    );

    assert_eq!(code, 1, "a runaway loop must exit 1, stderr was {}", stderr);
    assert!(
        stderr.contains("500"),
        "the process error must name the limit, got {:?}",
        stderr
    );
    assert!(
        !stdout.contains("finished"),
        "execution must stop at the cap, not reach the statement after the loop"
    );
}

/// A runaway loop is a `RuntimeError`, so `try ... catch error` can observe it
/// from inside the language rather than only from a harness.
#[test]
fn runaway_loop_is_catchable_from_inside_redblue() {
    let source = concat!(
        "set caught to no\n",
        "set count to 0\n",
        "try\n",
        "    set stopped to 0\n",
        "    while stopped is 0\n",
        "        set count to count + 1\n",
        "    end\n",
        "catch error\n",
        "    set caught to yes\n",
        "end\n",
        "expect caught to be yes\n",
    );

    let mut vm = Vm::with_max_iterations(100);
    vm.run(&parse(source))
        .expect("the loop's RuntimeError must be catchable");
}

// ---------------------------------------------------------------------------
// edge: 0 iterations, exactly max, and max + 1
// ---------------------------------------------------------------------------

/// Edge: a loop that must not iterate at all is not an error, however small the
/// cap. A cap of 1 still permits zero iterations.
#[test]
fn edge_zero_iterations_is_allowed() {
    for (name, source) in [
        ("repeat", "set n to 0\nrepeat 0 times\n    set n to 1\nend"),
        (
            "for each",
            "set n to 0\nfor each x in []\n    set n to 1\nend",
        ),
        ("while", "set i to 0\nwhile i is 5\n    set i to i + 1\nend"),
    ] {
        run_capped(source, 1).unwrap_or_else(|error| {
            panic!(
                "a zero-iteration {} loop must not spend budget: {:?}",
                name, error
            )
        });
    }
}

/// Edge: a loop that runs *exactly* the cap succeeds. An off-by-one that rejects
/// `max` is the common way an iteration cap is written wrong.
#[test]
fn edge_exactly_max_iterations_succeeds() {
    for (name, source) in [
        (
            "repeat",
            "set n to 0\nrepeat 10 times\n    set n to n + 1\nend",
        ),
        (
            "for each",
            "set n to 0\nfor each x in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]\n    set n to x\nend",
        ),
        (
            "while",
            "set i to 0\nwhile i is not 10\n    set i to i + 1\nend",
        ),
    ] {
        run_capped(source, 10).unwrap_or_else(|error| {
            panic!("exactly the cap must be allowed for {}: {:?}", name, error)
        });
    }
}

/// Edge: one iteration past the cap is a clean error, for every loop form the
/// parser can build.
#[test]
fn edge_max_plus_one_iterations_fails() {
    for (name, source) in [
        (
            "repeat",
            "set n to 0\nrepeat 11 times\n    set n to n + 1\nend",
        ),
        (
            "for each",
            "set n to 0\nfor each x in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]\n    set n to x\nend",
        ),
        (
            "while",
            "set i to 0\nwhile i is not 11\n    set i to i + 1\nend",
        ),
    ] {
        let error = run_capped(source, 10).expect_err("one iteration past the cap must fail");
        let message = runtime_message(&error);
        assert!(
            message.contains("10"),
            "the {} loop's error must name the limit, got {:?}",
            name,
            message
        );
    }
}

/// Edge: the cap is the same for every loop form. A long `for each` list is the
/// cheapest unbounded loop to write, so it must not be exempt from the bound.
#[test]
fn edge_iteration_cap_applies_to_each_loop_form() {
    let source = "set n to 0\nfor each x in [1, 2, 3, 4, 5]\n    set n to x\nend";

    let error = run_capped(source, 3).expect_err("a fourth iteration is over the cap");

    assert!(matches!(error, Error::Runtime(..)), "got {:?}", error);
}

/// Edge: the per-loop cap is per *loop*, not per program. Nested loops each get
/// the full allowance, or a nested program would be bounded by the inner loop's
/// arithmetic rather than by the operator's limit.
#[test]
fn edge_nested_loops_each_get_the_full_cap() {
    let mut vm = Vm::with_max_iterations(10);
    let program = parse("set n to 0\nfor each outer in [1, 2, 3]\n    for each inner in [1, 2, 3, 4]\n        set n to n + 1\n    end\nend");

    vm.run(&program)
        .expect("each loop stays inside the cap, so nesting must not double-charge");
}

// ---------------------------------------------------------------------------
// The step budget: a harness can bound a whole program
// ---------------------------------------------------------------------------

/// The per-loop cap is not enough on its own — an unbounded program can be
/// written as a million two-iteration loops — so a harness needs a budget over
/// the statements the VM runs in total.
#[test]
fn step_budget_bounds_a_program_of_many_short_loops() {
    let mut vm = Vm::with_max_steps(40);
    let program = parse(
        "set n to 0\nrepeat 100 times\n    repeat 2 times\n        set n to n + 1\n    end\nend",
    );

    let error = vm
        .run(&program)
        .expect_err("the step budget must stop a program of many short loops");

    let message = runtime_message(&error);
    assert!(
        message.contains("40"),
        "the error must name the step budget, got {:?}",
        message
    );
}

/// The budget is a budget, not a failure: a program that stays inside it runs to
/// completion untouched.
#[test]
fn step_budget_does_not_fire_under_the_limit() {
    let mut vm = Vm::with_max_steps(10_000);

    vm.run(&parse(
        "set n to 0\nrepeat 10 times\n    set n to n + 1\nend",
    ))
    .expect("a program inside the budget must run");
}

/// The budget counts steps actually taken, so a loop that is never entered
/// costs its own statement and nothing more. An enormous count therefore cannot
/// spend budget it never uses.
#[test]
fn edge_step_budget_is_not_spent_by_unentered_loops() {
    for (name, source) in [
        ("while", "set i to 5\nwhile i is 0\n    say \"never\"\nend"),
        (
            "for each",
            "set n to 0\nfor each x in []\n    set n to 1\nend",
        ),
    ] {
        let mut vm = Vm::with_max_steps(4);
        vm.run(&parse(source)).unwrap_or_else(|error| {
            panic!(
                "an unentered {} loop must spend no steps: {:?}",
                name, error
            )
        });
    }
}

/// The step budget is what makes a runaway loop *testable*, not merely
/// survivable: a harness can bound a program to a few hundred steps and assert
/// the exact error, without depending on how fast the host happens to be.
#[test]
fn step_budget_makes_a_runaway_loop_assertable() {
    let mut vm = Vm::with_max_steps(200);
    let program = parse("set stopped to 0\nwhile stopped is 0\n    set count to count + 1\nend");

    let error = vm
        .run(&program)
        .expect_err("200 steps of an endless loop must be a clean failure");

    assert!(matches!(error, Error::Runtime(..)), "got {:?}", error);
}

// ---------------------------------------------------------------------------
// The published limits
// ---------------------------------------------------------------------------

/// Both defaults are finite, positive numbers an operator can read, rather than
/// a magic constant buried in a loop body, and the program-wide budget is larger
/// than one loop's cap so the budget never stops a single legal loop.
#[test]
fn published_limits_are_finite_and_positive() {
    let iterations = redblue::resolve_max_iterations_from(None);
    let steps = redblue::resolve_max_steps_from(None);

    assert!(
        iterations > 0,
        "the default per-loop cap must be a real bound, got {}",
        iterations
    );
    assert!(
        steps > iterations,
        "the step budget ({}) must be larger than one loop's cap ({}), or the \
         budget would stop a loop that is inside its own limit",
        steps,
        iterations
    );
}

/// A limit of zero would make every loop illegal, which is never what an
/// operator means, so a zero or non-numeric setting falls back to the default.
#[test]
fn a_zero_limit_falls_back_to_the_default() {
    assert_eq!(
        redblue::resolve_max_iterations_from(Some(0)),
        MAX_ITERATIONS,
        "a zero cap must not be honoured"
    );
    assert_eq!(
        redblue::resolve_max_iterations_from(Some(42)),
        42,
        "a positive cap must be honoured"
    );
    assert_eq!(
        redblue::resolve_max_steps_from(Some(0)),
        MAX_STEPS,
        "a zero budget must not be honoured"
    );
}

/// Every example the repository ships must still fit inside the published
/// limits, or the bound would reject programs the language itself documents.
///
/// `modules/` is walked too: a module file is a program the loader runs, and
/// `modules/MathUtils.rb` used to be left out because its `constant`
/// declarations did not parse.
#[test]
fn shipped_examples_fit_inside_the_published_limits() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut checked = 0;

    // Sorted so that a failure names the same file every run.
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for dir in ["examples", "modules"] {
        let entries = std::fs::read_dir(root.join(dir))
            .unwrap_or_else(|e| panic!("{dir}/ should be readable: {e}"));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rb") {
                files.push(path);
            }
        }
    }
    files.sort();

    for path in files {
        let source = std::fs::read_to_string(&path).expect("example should be readable");
        let program = parse(&source);
        let mut vm = Vm::new();
        vm.run(&program).unwrap_or_else(|error| {
            panic!(
                "{} must run inside the published limits, failed with {:?}",
                path.display(),
                error
            )
        });
        checked += 1;
    }

    assert!(checked > 0, "the repository ships no examples to check");
}

// ---------------------------------------------------------------------------
// edge: counts at the numeric edges
// ---------------------------------------------------------------------------

/// Edge: a count beyond `i64`, and one beyond any representable number, must be
/// a clean iteration-cap error. Before the cap these were `as i64` saturations
/// fed into an unbounded loop, so the number's size decided only how long the
/// program hung.
#[test]
fn edge_count_beyond_i64_is_a_clean_error_not_a_panic() {
    for count in ["99999999999999999999", "1e300"] {
        let source = format!(
            "set n to 0\nrepeat {} times\n    set n to n + 1\nend",
            count
        );

        let error = run_capped(&source, 100)
            .expect_err("a count past i64 must be bounded, not hang or panic");
        assert!(
            matches!(error, Error::Runtime(..)),
            "count {} produced {:?}, which is not a RuntimeError",
            count,
            error
        );
    }
}

/// Edge: a fractional count truncates to zero iterations and is not an error.
/// The negative direction of the same boundary: `0..-5` is empty, not a run of
/// five backwards.
#[test]
fn edge_fractional_and_negative_counts_run_zero_times() {
    for count in ["0.5", "0", "-5"] {
        let source = format!(
            "set n to 0\nrepeat {} times\n    set n to n + 1\nend",
            count
        );

        run_capped(&source, 5)
            .unwrap_or_else(|error| panic!("count {} must run zero times: {:?}", count, error));
    }
}

/// Edge: a count that is not a number is not a loop at all. This is the
/// pre-existing behaviour and the cap must not change it into an error or a
/// hang.
#[test]
fn edge_type_mismatch_count_is_not_a_loop() {
    let mut vm = Vm::with_max_iterations(5);

    vm.run(&parse("repeat \"five\" times\n    say \"never\"\nend"))
        .expect("a text count is not a number, so the loop does not run");
}

/// Edge: `for each` over something that is not a list is likewise not a loop,
/// and cannot be made into a hang by the cap.
#[test]
fn edge_type_mismatch_iterable_is_not_a_loop() {
    let mut vm = Vm::with_max_iterations(5);

    vm.run(&parse("for each x in 5\n    say \"never\"\nend"))
        .expect("a number is not a list, so the loop does not run");
}

/// Edge: a loop nested inside a function that a loop calls is still bounded.
/// The function's own loop gets a fresh counter per call, so what stops the
/// whole program is the step budget — the case the per-loop cap alone misses.
#[test]
fn edge_nested_recursion_is_bounded_by_the_step_budget() {
    let source = concat!(
        "to spin()\n",
        "    set i to 0\n",
        "    while i is not 100000\n",
        "        set i to i + 1\n",
        "    end\n",
        "end\n",
        "for each x in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]\n",
        "    spin()\n",
        "end\n",
    );

    let mut vm = Vm::with_max_steps(2_000);
    let error = vm
        .run(&parse(source))
        .expect_err("ten calls to an endless loop must be bounded");

    let message = runtime_message(&error);
    assert!(
        message.contains("2000"),
        "the step budget must be what stops the program, got {:?}",
        message
    );
}

/// Edge: deeply nested loops terminate. Nesting multiplies iterations, so the
/// body is counted so the total stays small enough to finish: sixteen levels of
/// a two-iteration loop is 2^16 inner executions, all inside every cap.
#[test]
fn edge_deeply_nested_loops_terminate() {
    let depth = 16;
    let mut source = String::from("set n to 0\n");
    for _ in 0..depth {
        source.push_str("repeat 2 times\n");
    }
    source.push_str("set n to n + 1\n");
    for _ in 0..depth {
        source.push_str("end\n");
    }

    let mut vm = Vm::with_max_iterations(5);
    vm.run(&parse(&source)).unwrap_or_else(|error| {
        panic!(
            "{} levels of a 2-iteration loop is inside every cap, failed with {:?}",
            depth, error
        )
    });
}
