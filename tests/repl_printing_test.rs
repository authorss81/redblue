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
    session_with_env(input, &[])
}

/// Pipes `input` into `rb` with `env` set, and returns what the session
/// printed on stdout.
///
/// The environment is the only way to reach the coloured path from outside the
/// process: a pipe is not a terminal, so the session would paint nothing at
/// all and a colour assertion would have nothing to assert about.
fn session_with_env(input: &str, env: &[(&str, &str)]) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rb"));
    command.envs(env.iter().copied());
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

/// A list of records that hold no keys at all is named too.
///
/// The list is not empty, so the empty list's answer is not the answer here —
/// and a table with no columns is a blank line and says nothing at all, so a
/// reader cannot tell it from a failure.
#[test]
fn edge_a_table_of_records_with_no_keys_says_so() {
    let out = session(concat!(".table [{}]\n", ":quit\n"));

    let answer = after_prompt(&out, 0);
    assert!(
        answer.to_lowercase().contains("no keys"),
        "the records holding no keys are named\n--- actual ---\n{:?}",
        answer
    );
    assert!(
        !answer.trim().is_empty(),
        "and the answer is not silence\n--- actual ---\n{:?}",
        answer
    );
}

/// The document `.json` prints is the document `json.stringify` writes.
///
/// This is the command's whole claim, and it is checked against the language's
/// own writer inside a session: one value, printed twice, byte for byte the
/// same.
#[test]
fn edge_the_json_a_session_prints_is_the_document_json_stringify_writes() {
    let out = session(concat!(
        "set v to { name: \"ada\", count: 3, ok: yes, missing: nothing, \
         tags: [\"x\", \"y\"], nested: { a: [1, 2] } }\n",
        ".json v\n",
        "say json.stringify(v)\n",
        ":quit\n"
    ));

    let printed = after_prompt(&out, 1);
    let written = after_prompt(&out, 2);

    assert_eq!(
        printed.trim_end(),
        written.trim_end(),
        "the document the REPL printed is the document the language writes\n--- actual ---\n{:?}",
        printed.escape_debug()
    );
}

/// `.json` and `.pretty` both spell out the characters a terminal acts on, in
/// JSON's own escape, so neither document can reorder the line it is printed
/// on — and `.json` is still the document `json.stringify` writes.
#[test]
fn edge_the_json_document_escapes_the_marks_a_terminal_would_act_on() {
    let out = session(concat!(
        ".json \"\\u202e\"\n",
        ".pretty \"\\u202e\"\n",
        "say json.stringify(\"\\u202e\")\n",
        ":quit\n"
    ));

    let document = after_prompt(&out, 0);
    let rendering = after_prompt(&out, 1);
    let written = after_prompt(&out, 2);

    for (name, shown) in [
        ("the document", document.as_str()),
        ("the rendering", rendering.as_str()),
    ] {
        assert!(
            !shown.contains('\u{202e}'),
            "{} must not put the mark on the terminal raw\n--- actual ---\n{:?}",
            name,
            shown.escape_debug()
        );
        assert!(
            shown.contains("\\u202e"),
            "{} spells the mark out in JSON's own escape\n--- actual ---\n{:?}",
            name,
            shown.escape_debug()
        );
    }
    assert_eq!(
        document.trim_end(),
        written.trim_end(),
        "and `.json` is still byte for byte the document `json.stringify` writes"
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
        "every script survives the round trip\n--- actual ---\n{out}"
    );

    let tabled = after_prompt(&out, 1);
    assert!(
        tabled.contains("日本語") && tabled.contains("ada"),
        "and the table cell holds its text\n--- actual ---\n{out}"
    );
}

/// A value nested past the renderers' budget is refused, and the session
/// survives it.
///
/// This is the case that used to kill the process rather than answer: a
/// hundred thousand levels of nesting is a value a session can build in a
/// loop, and a renderer that follows it all the way down is a hundred
/// thousand stack frames — which in Rust is a fatal abort, not a catchable
/// panic. The session died instead of answering, and it died the same way for
/// `.json` and for `.table` while `.pretty` refused the value cleanly.
#[test]
fn edge_a_value_nested_past_the_budget_is_refused_and_the_session_survives() {
    let out = session(concat!(
        "set xs to []\n",
        "repeat 2000 times\n",
        "    set xs to [xs]\n",
        "end\n",
        ".json xs\n",
        ".table [{ deep: xs }]\n",
        ".pretty 1 + 1\n",
        ":quit\n"
    ));

    for (index, command) in [(2, ".json"), (3, ".table")] {
        let answer = after_prompt(&out, index);
        assert!(
            answer.contains("Error:"),
            "{} refuses a value nested past the budget\n--- actual ---\n{}",
            command,
            out
        );
        assert!(
            answer.contains("64 levels deep"),
            "{} names the budget it refused to go past\n--- actual ---\n{}",
            command,
            out
        );
    }

    assert_eq!(
        after_prompt(&out, 4).trim_end(),
        "2",
        "and the session goes on: the next command still answers\n--- actual ---\n{}",
        out
    );
}

/// A cell holding something a terminal would act on is escaped rather than
/// written out: an escape sequence that colours the rest of the row, a
/// right-to-left override that reorders it.
#[test]
fn edge_a_table_cell_a_terminal_would_mangle_is_escaped_rather_than_written_out() {
    let out = session(concat!(
        ".table [{ name: \"\\u001b[31mred\" }, { name: \"ada\" }]\n",
        ":quit\n"
    ));

    let tabled = after_prompt(&out, 0);
    assert!(
        !tabled.contains('\u{1b}'),
        "no raw escape sequence reaches the terminal\n--- actual ---\n{:?}",
        tabled
    );
    assert!(
        tabled.contains("\\u{1b}"),
        "the escape is spelled out instead\n--- actual ---\n{:?}",
        tabled
    );
    assert!(
        tabled.contains("ada"),
        "and the other row is still there\n--- actual ---\n{:?}",
        tabled
    );
}

/// JSON has no literal for a function, so `.json` refuses one by name rather
/// than answering `null` — which is what `nothing` is written as.
#[test]
fn edge_the_json_of_a_function_is_refused_and_not_null() {
    let out = session(concat!(
        "to f()\n",
        "    return 1\n",
        "end\n",
        ".json f\n",
        ".pretty f\n",
        ":quit\n"
    ));

    let refused = after_prompt(&out, 1);
    assert!(
        refused.contains("Error:") && refused.contains("function"),
        "the refusal names what JSON cannot hold\n--- actual ---\n{}",
        out
    );
    assert!(
        !refused.trim_end().ends_with("null"),
        "and it does not answer `null`\n--- actual ---\n{}",
        out
    );

    assert_eq!(
        after_prompt(&out, 2).trim_end(),
        "<function f>",
        "and .pretty shows the function's own form\n--- actual ---\n{}",
        out
    );
}

/// A printing command answers a misspelling exactly as `:type` does.
///
/// The four share one refusal path: the same prefix, the same renderer and
/// the same colour contract, so a line that does not parse is drawn the same
/// way whichever command asked about it — the failure is painted, and the
/// reader's own source line is left as they typed it.
///
/// A command that painted the whole refusal in one escape would answer a
/// misspelling with a different picture of it from the one the reader gets
/// everywhere else, and the difference would be in the escape bytes rather
/// than in the words.
#[test]
fn edge_a_printing_command_answers_a_misspelling_exactly_as_type_does() {
    let out = session_with_env(
        concat!(
            ":type 1 +\n",
            ".pretty 1 +\n",
            ".json 1 +\n",
            ".table 1 +\n",
            ":quit\n"
        ),
        &[("REDBLUE_COLOR", "always")],
    );

    let typed = after_prompt(&out, 0);
    assert!(
        typed.contains("Error:"),
        "the line does not parse, and the refusal says so\n--- actual ---\n{}",
        out
    );
    for index in 1..4 {
        let answer = after_prompt(&out, index);
        assert_eq!(
            answer,
            typed,
            "the {}th printing command answers exactly as `:type` does\n--- actual ---\n{:?}",
            index + 1,
            out
        );
    }
}

/// A cell shows the characters a terminal draws as they are.
///
/// Only the characters a terminal acts on are spelled out. A quote, an
/// apostrophe and a combining mark are drawn by every terminal there is, so
/// a cell that spelled them out would show the reader a character the value
/// does not hold — and the cell would no longer be what `say` would have
/// printed.
#[test]
fn edge_a_table_cell_shows_the_characters_a_terminal_draws_as_they_are() {
    let out = session(concat!(
        ".table [{ name: \"He said \\\"hi\\\"\" }]\n",
        ":quit\n"
    ));

    let tabled = after_prompt(&out, 0);
    assert!(
        tabled.contains("He said \"hi\""),
        "the cell holds the text as the value holds it\n--- actual ---\n{:?}",
        tabled
    );
    assert!(
        !tabled.contains("\\\""),
        "and no backslash was put in front of the quote\n--- actual ---\n{:?}",
        tabled
    );
}

/// A column holding an emoji is padded by the cells the emoji is drawn in, so
/// the columns line up on the reader's screen.
///
/// An emoji is one character and two cells, so a column padded by characters
/// drifts a cell for every emoji in it: the second column starts one cell
/// further along in that row than in the others, and the table does not line
/// up on the screen even though every row is the same length in characters.
#[test]
fn edge_a_table_column_holding_an_emoji_lines_up_on_the_screen() {
    let out = session(concat!(
        ".table [{ name: \"⭐\", n: 1 }, { name: \"ada\", n: 2 }]\n",
        ":quit\n"
    ));

    assert_eq!(
        after_prompt(&out, 0).trim_end(),
        "name  n\n⭐    1\nada   2",
        "the emoji cell is padded by the two cells it is drawn in\n--- actual ---\n{:?}",
        after_prompt(&out, 0)
    );
}
