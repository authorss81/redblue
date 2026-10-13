//! The REPL's colour system, end to end: a piped session must print the same
//! bytes it printed before there was a colour system, and a session that asks
//! for colour must get escape codes *around* the text rather than instead of it.
//!
//! The transcript fixture is the pre-colour output, byte for byte. It was
//! captured from the binary as it was before this phase and checked in, so a
//! gate on it is a gate on the claim that piped output did not change — not a
//! snapshot of whatever the current code happens to print.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// How long a session may take before it counts as hung.
const SESSION_LIMIT: Duration = Duration::from_secs(20);

struct Session {
    stdout: String,
}

fn read_all(pipe: &mut impl Read) -> String {
    let mut buf = Vec::new();
    let _ = pipe.read_to_end(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

/// The expected transcript of a piped session, checked in beside this file.
fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| {
        panic!(
            "the fixture {} should be readable: {}",
            path.display(),
            error
        )
    })
}

/// A scratch file inside the checkout, so no test writes outside the project.
fn scratch_file(name: &str, source: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/repl-color");
    std::fs::create_dir_all(&dir).expect("scratch directory should be creatable");
    let path = dir.join(format!("{}-{}.rb", name, serial));
    std::fs::write(&path, source).expect("the program should be writable");
    path
}

/// Pipes `input` into `rb` with no arguments and collects what it printed.
///
/// stdout is a pipe, so the session is not talking to a terminal and must not
/// paint anything — which is exactly the condition the transcript fixtures pin.
fn session(input: &str) -> Session {
    session_with_env(input, &[])
}

fn session_with_env(input: &str, env: &[(&str, &str)]) -> Session {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rb"));
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // The gating reads all three, and the environment this test runs under
        // is not the one under test: each case says what the terminal says.
        .env_remove("NO_COLOR")
        .env_remove("TERM")
        .env_remove("COLORTERM");
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("the rb binary should start");

    let mut stdin = child.stdin.take().expect("stdin should be piped");
    let payload = input.to_string();
    let writer = thread::spawn(move || {
        let _ = stdin.write_all(payload.as_bytes());
        drop(stdin);
    });

    let mut stdout = child.stdout.take().expect("stdout should be piped");
    let reader = thread::spawn(move || read_all(&mut stdout));
    let mut errors = child.stderr.take().expect("stderr should be piped");
    let stderr = thread::spawn(move || read_all(&mut errors));

    let deadline = Instant::now() + SESSION_LIMIT;
    loop {
        match child.try_wait().expect("child status should be readable") {
            Some(_) => break,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                writer.join().ok();
                let _ = reader.join();
                panic!("the REPL did not exit within {:?}", SESSION_LIMIT);
            }
            None => thread::sleep(Duration::from_millis(10)),
        }
    }

    writer.join().expect("the writer thread should not panic");
    let stdout = reader.join().expect("the reader thread should not panic");
    let _ = stderr.join();
    Session { stdout }
}

/// The session the transcript fixtures were captured from: a value, a program
/// that prints, a listing, an error, and an unknown command. Covers the four
/// places the palette is applied — result, program output, `:vars` and errors.
const TRANSCRIPT_INPUT: &str = "set x to 2 + 3\nsay x\n:vars\nnope\n:nosuchcommand\n:quit\n";

/// With output piped, every byte is what it was before the colour system existed.
#[test]
fn a_piped_session_prints_the_transcript_it_printed_before_colours_existed() {
    let s = session(TRANSCRIPT_INPUT);

    assert_eq!(
        s.stdout,
        fixture("repl_piped_session.txt"),
        "a session with no terminal must print exactly what it printed before \
         there was a colour system"
    );
}

/// `NO_COLOR` and `TERM=dumb` are refusals a user can always make, and they must
/// hold even for a session that is otherwise free to paint.
#[test]
fn no_color_and_a_dumb_terminal_produce_the_plain_transcript() {
    let expected = fixture("repl_piped_session.txt");

    for env in [[("NO_COLOR", "1")], [("TERM", "dumb")]] {
        let s = session_with_env(TRANSCRIPT_INPUT, &env);
        assert_eq!(
            s.stdout, expected,
            "with {:?} the session must paint nothing",
            env
        );
    }
}

/// `:colors off` in a live session is the same promise the environment makes,
/// asked for at the prompt.
#[test]
fn colors_off_in_a_live_session_prints_the_plain_transcript() {
    let plain = session_with_env(
        &format!(":colors off\n{}", TRANSCRIPT_INPUT),
        &[("TERM", "xterm-256color")],
    );
    let reference = session(TRANSCRIPT_INPUT);

    let acknowledged = plain.stdout.replacen(">>> Colors off.\n", "", 1);
    assert_eq!(
        acknowledged, reference.stdout,
        "with colors off a session prints the pre-colour transcript, bar the one \
         line that answers the command:\n--- actual ---\n{}",
        plain.stdout
    );
    assert_eq!(
        plain.stdout.matches('\u{1b}').count(),
        0,
        "no escape sequence anywhere in a session with colors off:\n{}",
        plain.stdout
    );
}

/// `:colors on` is a force, not a request: it paints even down a pipe, and it
/// paints *around* the text rather than replacing any of it.
#[test]
fn colors_on_wraps_the_result_and_the_error_without_changing_their_text() {
    let s = session_with_env(&format!(":colors on\n{}", TRANSCRIPT_INPUT), &[]);

    assert!(
        s.stdout.contains("= 5"),
        "the result line is still readable with colour on:\n{}",
        s.stdout
    );
    assert!(
        s.stdout
            .contains("Error: AnalyzerError: Unknown variable 'nope'"),
        "the error text is unchanged by colour on:\n{}",
        s.stdout
    );
    assert!(
        s.stdout.contains("\u{1b}[0m"),
        "colour on emits escapes at all:\n{}",
        s.stdout
    );
}

/// A `:colors` argument that is not one of the three modes is refused by name,
/// and saying so is the whole output — it is an error, not a new prompt.
#[test]
fn edge_colors_with_a_bogus_argument_is_refused_by_name() {
    let s = session(":colors chartreuse\n:quit\n");

    assert!(
        s.stdout.contains("chartreuse"),
        "the argument the user typed is named back:\n{}",
        s.stdout
    );
    assert!(
        s.stdout.contains("on, off or auto"),
        "and the three modes that are not it are named:\n{}",
        s.stdout
    );
    assert!(
        !s.stdout.contains("Colors on."),
        "a refused mode must not be applied:\n{}",
        s.stdout
    );
}

/// `:help` reports the mode in force, so a user can see why a session is plain.
#[test]
fn help_reports_the_color_mode_in_force() {
    let s = session(":colors off\n:help\n:quit\n");

    assert!(
        s.stdout.contains(":colors"),
        ":help documents the command:\n{}",
        s.stdout
    );
    assert!(
        s.stdout.contains("Colors: off"),
        ":help reports the mode the session is in:\n{}",
        s.stdout
    );
}

/// The loaded-program path is a second way results reach a session, and it goes
/// through the same result line.
#[test]
fn a_loaded_program_keeps_its_text_with_color_on() {
    let path = scratch_file("loaded", "say \"from a file\"\n7 * 6\n");
    let s = session_with_env(
        &format!(":colors on\n:load {}\n:quit\n", path.display()),
        &[],
    );

    assert!(
        s.stdout.contains("from a file"),
        "a loaded program still prints what it says:\n{}",
        s.stdout
    );
    assert!(
        s.stdout.contains("= 42"),
        "and its last value is still reported as `= 42`:\n{}",
        s.stdout
    );
}
