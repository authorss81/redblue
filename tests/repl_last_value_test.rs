//! The last value, driven through the real binary.
//!
//! The unit tests in `src/repl/mod.rs` hold a `Repl` and call it. This one types
//! at the process a user would type at, because two things are only observable
//! from the outside: that the REPL's own read loop passes `$_` through to the
//! program it runs, and that a line the lexer refuses is refused as a lexer
//! error rather than being quietly swallowed.

use std::io::Write;
use std::process::{Command, Stdio};

/// Types `lines` at a piped `rb` and returns everything it printed.
///
/// No `:session` here and no `:sessions`: those write under `$HOME`, and a test
/// has no business writing there.
fn at_the_prompt(lines: &[&str]) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_rb"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("rb should be runnable");

    let mut input = String::new();
    for line in lines {
        input.push_str(line);
        input.push('\n');
    }
    child
        .stdin
        .take()
        .expect("the child was given a pipe")
        .write_all(input.as_bytes())
        .expect("the pipe should accept the typed lines");

    let output = child.wait_with_output().expect("rb should finish");
    assert!(
        output.status.success(),
        "a piped session exits cleanly\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Whether a piped session printed `value` as a whole line.
///
/// A pipe has no terminal, so the prompt and the output share a line: what the
/// user sees as "typed, then printed" arrives here as `>>> 45`. Matching the
/// whole line rather than a substring is what keeps an echoed source line —
/// `1 | say $_ + 3`, which ends in a 3 — from counting as output.
fn printed_value(session: &str, value: &str) -> bool {
    session.lines().any(|line| {
        let line = line.trim_end();
        line == value || line == format!(">>> {value}")
    })
}

/// `$_` is the value of the line before, read by the line after. Asserted
/// against printed output rather than against a binding, because what the user
/// is promised is a number the program can use.
#[test]
fn the_last_value_is_readable_from_the_next_line_at_the_prompt() {
    let printed = at_the_prompt(&["40 + 2", "say $_ + 3"]);

    assert!(
        printed.contains("= 42"),
        "the first line echoes its value: {printed}"
    );
    assert!(
        printed_value(&printed, "45"),
        "and the next line adds three to it: {printed}"
    );
}

/// `$_` skips a line that failed: the last value is the last one that was
/// produced, not the last thing typed.
#[test]
fn a_failed_line_leaves_the_last_successful_value_readable() {
    let printed = at_the_prompt(&["40 + 2", "1 / 0", "say $_ + 3"]);

    assert!(
        printed.contains("Error:"),
        "the division is reported: {printed}"
    );
    assert!(
        printed_value(&printed, "45"),
        "and $_ is still 42, so the sum is still 45: {printed}"
    );
}

/// The first line has no last value. It is refused by name, as an unknown
/// variable, rather than printing nothing and letting the session carry on as
/// though the name were `nothing`.
#[test]
fn the_last_value_on_the_first_line_is_refused_by_name() {
    let printed = at_the_prompt(&["say $_ + 3"]);

    assert!(
        printed.contains("Unknown variable '$_'"),
        "the name is what is missing, and the message says so: {printed}"
    );
    assert!(
        !printed_value(&printed, "3"),
        "and nothing was computed from it: {printed}"
    );
}

/// `:last` prints the same value `$_` reads, and on a session where nothing has
/// succeeded it says why instead of printing nothing.
#[test]
fn the_last_command_prints_what_the_name_reads_and_refuses_when_there_is_none() {
    let printed = at_the_prompt(&["40 + 2", "say $_ + 3", ":last"]);

    assert!(
        printed_value(&printed, "= 42"),
        "`:last` prints the last value: {printed}"
    );

    let empty = at_the_prompt(&[":last"]);
    assert!(
        empty.contains("no line in this session has returned a value"),
        "and on a session with no value it names the condition: {empty}"
    );
}
