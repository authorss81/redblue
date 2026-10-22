//! End-to-end tests for the three ways a value can be printed: `.pretty`,
//! `.json` and `.table`.
//!
//! The unit tests in `src/repl/render.rs` and `src/repl/mod.rs` hold the
//! rendering itself; these hold the *other* half, which no unit test can see:
//! that the words a user types reach the renderer and that what comes back is
//! printed the way it is meant to be read. Every test here pipes a session
//! into the real binary and asserts on what it printed.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// How long a session may take before it counts as hung.
///
/// A liveness bound, not a performance assertion: a session that never stops
/// asking for input must still finish on EOF rather than spin, so without this
/// a regression would be an unkillable test rather than a failing one.
const SESSION_LIMIT: Duration = Duration::from_secs(20);

/// The prompt a session asks at.
const PROMPT: &str = ">>> ";

/// Pipes `input` into `rb` and returns what the session printed on stdout.
fn session(input: &str) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rb"));
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

/// What the session printed after its `index`th prompt.
///
/// The prompt is printed *before* the line is read, so what a line printed is
/// exactly what sits between that prompt and the next one.
fn after_prompt(output: &str, index: usize) -> String {
    output
        .split(PROMPT)
        .nth(index + 1)
        .unwrap_or_else(|| {
            panic!(
                "the session asked for {} prompts at most, and the {}th answer is \
                 missing\n--- actual ---\n{}",
                index + 1,
                index,
                output
            )
        })
        .to_string()
}

/// The three commands print three shapes, and the shapes are the point: a
/// structure, a document and a table.
#[test]
fn edge_the_three_printing_commands_print_three_shapes() {
    let out = session(concat!(
        ".pretty { a: 1, b: [2, 3] }\n",
        ".json { a: 1, b: [2, 3] }\n",
        ".table [{ name: \"ada\", age: 36 }, { name: \"bob\" }]\n",
        ":quit\n"
    ));

    assert_eq!(
        after_prompt(&out, 0).trim_end(),
        "{\n  \"a\": 1,\n  \"b\": [\n    2,\n    3\n  ]\n}",
        ".pretty indents one level per level of nesting\n--- actual ---\n{}",
        out
    );
    assert_eq!(
        after_prompt(&out, 1).trim_end(),
        "{\"a\": 1, \"b\": [2, 3]}",
        ".json is the document the language's own json.stringify writes\n--- actual ---\n{}",
        out
    );
    assert_eq!(
        after_prompt(&out, 2).trim_end(),
        "name  age\nada   36\nbob",
        ".table names its keys once and lines the columns up\n--- actual ---\n{}",
        out
    );
}

/// The JSON a session prints is JSON: a reader that knows nothing about
/// Redblue reads it, and reads the value the session was asked about.
#[test]
fn edge_the_json_a_session_prints_is_valid_json() {
    let out = session(concat!(
        ".json { name: \"ada\", count: 3, ok: yes, missing: nothing, tags: [\"x\", \"y\"] }\n",
        ":quit\n"
    ));

    let written = after_prompt(&out, 0);
    let read: serde_json::Value =
        serde_json::from_str(written.trim_end()).unwrap_or_else(|error| {
            panic!("the document is valid JSON: {error}\n--- actual ---\n{out}")
        });

    assert_eq!(read["name"], serde_json::json!("ada"));
    assert_eq!(read["count"], serde_json::json!(3));
    assert_eq!(read["ok"], serde_json::json!(true));
    assert_eq!(read["missing"], serde_json::json!(null));
    assert_eq!(read["tags"], serde_json::json!(["x", "y"]));
}

/// A table of what is not tabular is refused, and the refusal says what it
/// got. The session goes on: a refused command is an answer, not a crash.
#[test]
fn edge_table_of_a_value_that_is_not_a_list_of_records_is_refused() {
    let out = session(concat!(
        ".table 5\n",
        ".table [1, 2]\n",
        ".pretty 1 + 1\n",
        ":quit\n"
    ));

    assert!(
        after_prompt(&out, 0).contains("Error: .table needs a list of records"),
        "a number is not a table\n--- actual ---\n{}",
        out
    );
    assert!(
        after_prompt(&out, 1).contains("item 1"),
        "and the refusal names the item that is not a record\n--- actual ---\n{}",
        out
    );
    assert_eq!(
        after_prompt(&out, 2).trim_end(),
        "2",
        "and the session goes on: the next command still answers\n--- actual ---\n{}",
        out
    );
}

/// A line that does not parse is refused as the failure it is, and the
/// session is left exactly as the refusal found it.
#[test]
fn edge_a_printing_command_of_a_line_that_does_not_parse_is_refused() {
    let out = session(concat!(
        ".pretty 1 +\n",
        ".json [1, 2\n",
        ".table { a: }\n",
        ":quit\n"
    ));

    for index in 0..3 {
        let answer = after_prompt(&out, index);
        assert!(
            answer.contains("Error:"),
            "the {}th refusal is an error\n--- actual ---\n{}",
            index + 1,
            out
        );
    }
}

/// An empty list has no rows and no keys: it is named, and it is not a crash.
#[test]
fn edge_a_table_of_an_empty_list_says_so_and_does_not_panic() {
    let out = session(concat!(".table []\n", ":quit\n"));

    let answer = after_prompt(&out, 0);
    assert!(
        answer.to_lowercase().contains("empty"),
        "the empty list is named\n--- actual ---\n{}",
        out
    );
}

/// A text written in another script, in a right-to-left script and in an
/// emoji is printed as it is: the escapes are the language's, not the
/// printer's.
#[test]
fn edge_a_printed_text_keeps_its_own_script() {
    let out = session(concat!(
        ".json [\"café\", \"日本語\", \"مرحبا\", \"🎉\"]\n",
        ".table [{ name: \"日本語\" }, { name: \"ada\" }]\n",
        ":quit\n"
    ));

    let written = after_prompt(&out, 0);
    let read: serde_json::Value =
        serde_json::from_str(written.trim_end()).unwrap_or_else(|error| {
            panic!("the document is valid JSON: {error}\n--- actual ---\n{out}")
        });
    assert_eq!(
        read,
        serde_json::json!(["café", "日本語", "مرحبا", "🎉"]),
        "every script survives the round trip\n--- actual ---\n{}",
        out
    );

    let tabled = after_prompt(&out, 1);
    assert!(
        tabled.contains("日本語") && tabled.contains("ada"),
        "and the table cell holds its text\n--- actual ---\n{}",
        out
    );
}
