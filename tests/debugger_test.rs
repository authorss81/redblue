//! End-to-end tests for the debugger: `rb` driven over a pipe, stopping,
//! stepping and inspecting a running program.
//!
//! The debugger is only real if a user can reach it from the prompt and it
//! really stops the program, so every test here pipes a session into the real
//! binary and asserts on what it printed — never on an internal API.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// How long a session may take before it counts as hung.
///
/// A liveness bound, not a performance assertion: a debugger that never stops
/// asking for input must still finish on EOF rather than spin, so without this
/// a regression would be an unkillable test rather than a failing one.
const SESSION_LIMIT: Duration = Duration::from_secs(20);

/// Pipes `input` into `rb` and returns what the session printed on stdout.
fn session(input: &str) -> String {
    session_with(input, &[])
}

/// [`session`], with environment variables the child is run under.
fn session_with(input: &str, env: &[(&str, &str)]) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rb"));
    for (name, value) in env {
        command.env(name, value);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the rb binary should start");

    let mut stdin = child.stdin.take().expect("stdin should be piped");
    let payload = input.to_string();
    let writer = thread::spawn(move || {
        let _ = stdin.write_all(payload.as_bytes());
        drop(stdin);
    });

    let mut stdout = child.stdout.take().expect("stdout should be piped");
    let reader = thread::spawn(move || read_all(&mut stdout));
    let mut stderr = child.stderr.take().expect("stderr should be piped");
    let errors = thread::spawn(move || read_all(&mut stderr));

    let deadline = Instant::now() + SESSION_LIMIT;
    let status = loop {
        match child.try_wait().expect("child status should be readable") {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                writer.join().ok();
                let _ = reader.join();
                let _ = errors.join();
                panic!(
                    "the REPL did not exit within {:?} of its input closing: it hangs \
                     instead of terminating on EOF.\ninput:\n{}",
                    SESSION_LIMIT, input
                );
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    };

    writer.join().expect("the writer thread should not panic");
    let stdout = reader.join().expect("the reader thread should not panic");
    let stderr = errors.join().expect("the error thread should not panic");

    assert_eq!(
        status.code(),
        Some(0),
        "the session should exit 0\ninput:\n{}\nstdout:\n{}\nstderr:\n{}",
        input,
        stdout,
        stderr
    );
    stdout
}

/// Drains a pipe on its own thread: a child writing more than the pipe buffer
/// holds would otherwise block forever waiting for a reader.
fn read_all(pipe: &mut impl Read) -> String {
    let mut buf = Vec::new();
    let _ = pipe.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

/// How many times `needle` appears, for the assertions that are about *how
/// often* a session stops rather than whether it stops at all.
fn count(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[test]
fn breakpoint_in_a_loop_fires_on_every_iteration() {
    let out = session(":break 2\nfor each x in [1, 2, 3]\n    say x\nend\nc\nc\nc\n:quit\n");

    assert_eq!(
        count(&out, "Stopped at line 2"),
        3,
        "a breakpoint inside a loop should stop on all three iterations, and \
         `c` should resume to the next\n--- actual ---\n{}",
        out
    );
    for value in ["x = 1", "x = 2", "x = 3"] {
        assert!(
            out.contains(&format!("    {}", value)),
            "the stopped frame should show {:?}\n--- actual ---\n{}",
            value,
            out
        );
    }
    assert!(
        out.contains("1\n2\n3\n"),
        "the program should still print every value once\n--- actual ---\n{}",
        out
    );
    assert_eq!(
        count(&out, "(rb-debug) "),
        3,
        "each stop should ask once\n--- actual ---\n{}",
        out
    );
}

#[test]
fn step_over_runs_the_call_and_step_into_descends_into_it() {
    let out = session(concat!(
        ":step\n",
        "repeat 1 times\n",
        "    to add(a, b)\n",
        "        set total to a + b\n",
        "        say total\n",
        "    end\n",
        "    say add(2, 3)\n",
        "end\n",
        "s\nn\ns\nc\n:quit\n"
    ));

    // `n` at the `to` declaration steps *over* everything until the call site,
    // skipping the declaration's own body, which has not been entered.
    assert!(
        out.contains("Stopped at line 6: say add(2, 3)"),
        "`n` should stop at the next statement of the same call, skipping the \
         function it has not entered\n--- actual ---\n{}",
        out
    );
    assert!(
        !out.contains("Stopped at line 3: set total to a + b\nline 3 (depth 0)"),
        "`n` must not descend into the call\n--- actual ---\n{}",
        out
    );

    // `s` from the call site descends into the body, one call deeper, with the
    // arguments bound as the frame shows.
    assert!(
        out.contains("Stopped at line 3: set total to a + b\nline 3 (depth 1)"),
        "`s` should descend into the call and stop one frame deeper\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("    a = 2") && out.contains("    b = 3"),
        "the frame inside the call should show its arguments\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("5\nnothing\n"),
        "the program should still finish and print\n--- actual ---\n{}",
        out
    );
}

#[test]
fn a_recursive_call_reaches_the_same_breakpoint_at_every_depth() {
    let out = session(concat!(
        ":break 4\n",
        "repeat 1 times\n",
        "    to countdown(n)\n",
        "        if n is greater than 0 then\n",
        "            countdown(n - 1)\n",
        "        end\n",
        "    end\n",
        "    countdown(2)\n",
        "end\n",
        "c\nc\n:quit\n"
    ));

    assert_eq!(
        count(&out, "Stopped at line 4"),
        2,
        "the same line should stop once per call that reaches it\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("line 4 (depth 1)") && out.contains("line 4 (depth 2)"),
        "the two stops should be one call apart\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("    n = 2") && out.contains("    n = 1"),
        "each stop should show the argument that call was given\n--- actual ---\n{}",
        out
    );
}

/// The boundary of the set: a line past the end of the program, and a line the
/// program has but holds no statement on. Neither is a crash and neither stops.
#[test]
fn edge_break_on_a_line_the_program_never_reaches_is_simply_never_taken() {
    let out =
        session(":break 999\nrepeat 1 times\n    say \"a program ran\"\nend\n:breaks\n:quit\n");

    assert_eq!(
        count(&out, "Stopped at line"),
        0,
        "a breakpoint on a line no statement is written on must never stop \
         anything\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("a program ran\n"),
        "the program should run to its end regardless\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("  line 999"),
        "the breakpoint should still be listed, so a user can see it was set \
         on the wrong line\n--- actual ---\n{}",
        out
    );
}

/// Nothing bound yet: a stop on a program's first line, before anything has
/// been declared. The listing must be empty rather than listing the VM's own
/// builtins as if the program had written them.
#[test]
fn edge_frame_of_a_stop_with_nothing_bound_lists_no_names() {
    let out = session(":break 1\nrepeat 1 times\n    say \"first\"\nend\nc\n:quit\n");

    assert!(
        out.contains("Stopped at line 1: repeat 1 times\nline 1 (depth 0)\n(rb-debug)"),
        "a stop before anything is bound should list no names at all\n--- actual ---\n{}",
        out
    );
    assert!(
        !out.contains("<builtin"),
        "the frame must not list the VM's own builtins\n--- actual ---\n{}",
        out
    );
}

#[test]
fn edge_end_of_input_at_a_stop_resumes_rather_than_hanging() {
    // The breakpoint is on a loop that runs forever, and the only answer given
    // is end of input: a debugger that kept asking a closed pipe for answers
    // would never let this session finish.
    let out = session(":break 2\nrepeat 3 times\n    say \"tick\"\nend\n");

    assert_eq!(
        count(&out, "Stopped at line 2"),
        3,
        "end of input resumes each iteration, so the breakpoint is reached on \
         all three without a single answer being typed\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("tick\ntick\ntick\n"),
        "the program should still run to its end\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("Goodbye!"),
        "the session should reach EOF and exit rather than spin\n--- actual ---\n{}",
        out
    );
}

/// A line number that is not one is refused, in every way it can be not one.
#[test]
fn edge_a_line_number_that_is_not_a_line_is_refused() {
    let out = session(":break\n:break abc\n:break 0\n:break -1\n:break 1 2\n:break 99999999999999999999999\n:breaks\n:quit\n");

    assert!(
        out.contains("Usage: :break <line>"),
        "`:break` with no argument should say what it wants\n--- actual ---\n{}",
        out
    );
    for refusal in [
        "'abc' is not a line number",
        "lines are counted from 1, so there is no line 0",
        "No breakpoints set.",
    ] {
        assert!(
            out.contains(refusal),
            "the session should say {:?}\n--- actual ---\n{}",
            refusal,
            out
        );
    }
    assert!(
        !out.contains("Breakpoint set on line 1"),
        "no refused argument may leave a breakpoint behind, and `:break 1 2` \
         must not be read as line 1\n--- actual ---\n{}",
        out
    );
}

#[test]
fn edge_break_twice_clears_the_breakpoint_and_delete_says_what_it_did() {
    let out =
        session(":break 2\n:breaks\n:break 2\n:breaks\n:delete 2\n:breaks\n:delete 2\n:quit\n");

    assert_eq!(
        count(&out, "Breakpoint set on line 2."),
        1,
        "setting the same line twice should report one set and one removal\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("Breakpoint removed from line 2."),
        "the second `:break` on the same line should take it away\n--- actual ---\n{}",
        out
    );
    assert_eq!(
        count(&out, "No breakpoints set."),
        2,
        "the list should be empty after each removal\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("No breakpoint on line 2."),
        "deleting a line that has none should say so rather than claim it did\n--- actual ---\n{}",
        out
    );
}

/// A program that fails still fails, with the debugger attached and stopped on
/// it. The debugger observes; it does not swallow what the program raised.
#[test]
fn a_program_that_fails_still_reports_its_own_error_under_the_debugger() {
    let out = session(":break 1\nset x to 1 + \"a\"\n:quit\n");

    assert!(
        out.contains("Stopped at line 1: set x to 1 + \"a\""),
        "the debugger should stop before the statement that fails\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("RuntimeError: Cannot add non-numbers"),
        "the program's own type failure should still reach the user\n--- actual ---\n{}",
        out
    );
}

#[test]
fn a_program_that_will_not_parse_is_reported_without_ever_stopping() {
    let out = session(":break 1\nsay (\n:quit\n");

    assert!(
        out.contains("ParserError"),
        "a program the parser refuses should be reported as such\n--- actual ---\n{}",
        out
    );
    assert_eq!(
        count(&out, "Stopped at line"),
        0,
        "a program that never ran cannot have stopped\n--- actual ---\n{}",
        out
    );
}

#[test]
fn the_debugger_is_not_reachable_from_redblue_source() {
    // Every debugger word, typed without the sigil that makes it a command.
    // `break` is the loop keyword and means something else; the rest are not
    // Redblue at all. This is the test that would fail if the debugger ever
    // became grammar.
    let out = session("break 3\nnext\nstep\ncontinue\nframe\n:quit\n");

    for expected in [
        "'break' is only valid inside a loop",
        "Unknown variable 'next'",
        "Unknown variable 'step'",
        "Unknown variable 'continue'",
        "Unknown variable 'frame'",
    ] {
        assert!(
            out.contains(expected),
            "the bare word should reach the language, not the debugger: expected \
             {:?}\n--- actual ---\n{}",
            expected,
            out
        );
    }
    assert_eq!(
        count(&out, "Stopped at line"),
        0,
        "source with no sigil must never reach the debugger\n--- actual ---\n{}",
        out
    );
}

#[test]
fn frame_at_the_prompt_reports_the_last_stop_and_nothing_before_one() {
    let out = session(concat!(
        ":frame\n",
        ":break 3\n",
        "repeat 1 times\n",
        "    set total to 1 + 2\n",
        "    say total\n",
        "end\n",
        "c\n",
        ":frame\n",
        ":quit\n"
    ));

    assert!(
        out.contains("Nothing has stopped yet."),
        "before anything stops there is no frame to report\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("\nline 3 (depth 0)\n    total = 3\n"),
        "`:frame` after the run should report the frame the last stop left\n--- actual ---\n{}",
        out
    );
}

/// The loop cap is the interpreter's, and the debugger does not lift it: a
/// program over the cap still stops once, and still fails on the cap rather
/// than running to the end.
#[test]
fn edge_a_loop_over_the_iteration_cap_still_stops_and_still_ends() {
    let out = session_with(
        ":break 3\nrepeat 50 times\n    set k to 0\n    set k to k + 1\nend\nc\n:quit\n",
        &[("REDBLUE_MAX_ITERATIONS", "3")],
    );

    assert_eq!(
        count(&out, "Stopped at line 3"),
        3,
        "the breakpoint should fire on every iteration the cap allows\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("Maximum of 3 iterations reached in a 'repeat' loop"),
        "the interpreter's own cap should still stop the loop; the debugger \
         must not lift it\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("Goodbye!"),
        "the session should still finish\n--- actual ---\n{}",
        out
    );
}

/// A breakpoint on the only line of a one-line program: the singleton, at both
/// ends of the range, stops exactly once.
#[test]
fn edge_a_breakpoint_on_a_one_line_program_fires_once() {
    let out = session(":break 1\nsay \"only\"\nc\n:quit\n");

    assert_eq!(
        count(&out, "Stopped at line 1"),
        1,
        "one statement is one stop, however many times the line was asked for\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("(rb-debug) only\n"),
        "the program should still run to its end\n--- actual ---\n{}",
        out
    );
}

#[test]
fn unicode_in_a_stopped_frame_is_shown_intact() {
    let out = session(":break 3\nrepeat 1 times\n    set greeting to \"héllo 🌍 世界\"\n    say greeting\nend\nc\n:quit\n");

    assert!(
        out.contains("Stopped at line 3: say greeting"),
        "the stop should be on the line it was set for\n--- actual ---\n{}",
        out
    );
    assert!(
        out.contains("héllo 🌍 世界"),
        "the frame should show the text exactly as it is held, with no \
         escaping or truncation\n--- actual ---\n{}",
        out
    );
}

/// Non-ASCII and emoji aside, a name is a name: a frame listing must not be
/// reordered or dropped by the ordering the map it was built from had.
#[test]
fn edge_a_frame_lists_the_same_names_in_the_same_order_twice() {
    let program = concat!(
        ":break 4\n",
        "repeat 1 times\n",
        "    set zebra to 1\n",
        "    set alpha to 2\n",
        "    say alpha\n",
        "end\n",
        "c\n:quit\n"
    );
    let first = session(program);
    let second = session(program);

    assert_eq!(
        first, second,
        "the same session printed twice should print the same thing twice"
    );
    let alpha = first.find("    alpha = 2").expect("alpha should be listed");
    let zebra = first.find("    zebra = 1").expect("zebra should be listed");
    assert!(
        alpha < zebra,
        "the listing is ordered by name, not by the map's iteration order\n--- actual ---\n{}",
        first
    );
}
