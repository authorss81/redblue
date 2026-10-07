//! The `files`, `json`, `csv` and `network` modules must be testable and total.
//!
//! Every filesystem test runs against a temp directory of its own under
//! `target/tmp`, so the suite is deterministic and never writes outside the
//! checkout. Nothing here performs a successful network request: the one test
//! that opens a socket asserts the *absence of a hang*, and the rest check
//! argument handling, which never reaches the network.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use redblue::{Error, Value, NETWORK_CONNECT_TIMEOUT_SECS, NETWORK_TIMEOUT_SECS};

/// One test's private directory under `target/tmp/phase014`.
///
/// Removed before it is created, so a directory left behind by an interrupted
/// run cannot make the next run of the same test see stale files.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    #[track_caller]
    fn new(name: &str) -> TempDir {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/tmp/phase014")
            .join(name);
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)
            .unwrap_or_else(|e| panic!("{} should be creatable: {}", path.display(), e));
        TempDir { path }
    }

    /// A path inside this directory. The name is used verbatim, so a test can
    /// ask for a name with a space in it.
    fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    /// The path as a Redblue text literal.
    fn literal(&self, name: &str) -> String {
        redblue_text(&self.file(name).to_string_lossy())
    }

    /// Writes `content` to `name` and returns the path.
    #[track_caller]
    fn write_file(&self, name: &str, content: &str) -> PathBuf {
        let path = self.file(name);
        std::fs::write(&path, content)
            .unwrap_or_else(|e| panic!("{} should be writable: {}", path.display(), e));
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Lexes and parses `source`.
#[track_caller]
fn parse(source: &str) -> redblue::parser::Program {
    let tokens = redblue::lexer::Lexer::tokenize(source).expect("source should lex");
    redblue::parser::parse(tokens).expect("source should parse")
}

/// Runs `source` and returns the value of its last statement, which is why
/// every source below ends in a bare expression and never in `say`.
#[track_caller]
fn eval(source: &str) -> Value {
    let (_vm, result) = redblue::run_isolated(&parse(source));
    result.unwrap_or_else(|error| panic!("source should have run, failed with {:?}", error))
}

/// Runs `source` and returns the error it produced.
#[track_caller]
fn eval_err(source: &str) -> Error {
    let (_vm, result) = redblue::run_isolated(&parse(source));
    result.expect_err("source should have failed")
}

/// Asserts `source` fails with a `Runtime` error whose message contains `part`.
#[track_caller]
fn assert_runtime_error(source: &str, part: &str) {
    match eval_err(source) {
        Error::Runtime(message, span) => {
            assert!(
                message.contains(part),
                "expected a message containing `{}`, got `{}`",
                part,
                message
            );
            assert!(span.is_known(), "failed without a source span");
        }
        other => panic!("expected a Runtime error, got {:?}", other),
    }
}

/// A Redblue text literal holding `value`, with `\` and `"` escaped for the
/// lexer, so a path with a space or a quote in it is still one literal.
fn redblue_text(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// A list of text cells, for comparing what `csv.parse` produced.
fn cells(fields: &[&str]) -> Value {
    Value::List(
        fields
            .iter()
            .map(|f| Value::Text((*f).to_string()))
            .collect(),
    )
}

/// Rows of cells, for comparing what `csv.parse` produced.
fn rows(expected: &[&[&str]]) -> Value {
    Value::List(expected.iter().map(|r| cells(r)).collect())
}

// ---------------------------------------------------------------- files

/// `files.write` then `files.read` against a temp directory: the round trip
/// returns what was written, and the file is on disk.
#[test]
fn files_write_then_read_round_trips_in_a_temp_dir() {
    let dir = TempDir::new("files-round-trip");
    assert_eq!(
        eval(&format!(
            "files.write({}, {})",
            dir.literal("note.txt"),
            redblue_text("hello\nworld")
        )),
        Value::Nothing,
        "files.write answers nothing"
    );
    assert_eq!(
        std::fs::read_to_string(dir.file("note.txt")).expect("file should exist"),
        "hello\nworld",
        "files.write must write exactly its argument"
    );
    assert_eq!(
        eval(&format!("files.read({})", dir.literal("note.txt"))),
        Value::Text("hello\nworld".to_string()),
        "files.read must read back what files.write wrote"
    );
}

/// An empty file is an empty text, not a missing file and not an error.
#[test]
fn edge_empty_file_reads_as_empty_text() {
    let dir = TempDir::new("files-empty");
    dir.write_file("empty.txt", "");
    assert_eq!(
        eval(&format!("files.read({})", dir.literal("empty.txt"))),
        Value::Text(String::new()),
        "an empty file is empty text"
    );
    assert_eq!(
        eval(&format!("files.lines({})", dir.literal("empty.txt"))),
        Value::List(Vec::new()),
        "an empty file has no lines"
    );
    assert_eq!(
        eval(&format!(
            "files.write({}, {})",
            dir.literal("fresh.txt"),
            redblue_text("")
        )),
        Value::Nothing
    );
    assert!(
        dir.file("fresh.txt").exists(),
        "files.write of empty text must create the file"
    );
}

/// A path with a space in it is an ordinary path, not two arguments.
#[test]
fn files_handles_a_path_with_spaces() {
    let dir = TempDir::new("files-spaces");
    let literal = dir.literal("a file with spaces.txt");
    eval(&format!(
        "files.write({}, {})",
        literal,
        redblue_text("spaced")
    ));
    assert_eq!(
        eval(&format!("files.read({})", literal)),
        Value::Text("spaced".to_string()),
        "a path with spaces must name one file"
    );
    assert_eq!(
        eval(&format!("files.exists({})", literal)),
        Value::YesNo(true)
    );
}

/// `files.append` creates the file when it is missing and adds to the end when
/// it is not.
#[test]
fn files_append_creates_then_appends() {
    let dir = TempDir::new("files-append");
    let literal = dir.literal("log.txt");
    assert_eq!(
        eval(&format!(
            "files.append({}, {})",
            literal,
            redblue_text("one\n")
        )),
        Value::Nothing
    );
    eval(&format!(
        "files.append({}, {})",
        literal,
        redblue_text("two\n")
    ));
    assert_eq!(
        eval(&format!("files.read({})", literal)),
        Value::Text("one\ntwo\n".to_string()),
        "append must add to the end of what is there"
    );
}

/// `files.lines` splits on newlines, keeps a blank line as a blank line, and
/// does not invent a line for the newline that ends the file.
#[test]
fn files_lines_splits_lines_and_keeps_blank_ones() {
    let dir = TempDir::new("files-lines");
    dir.write_file("lines.txt", "a\n\nb\n");
    assert_eq!(
        eval(&format!("files.lines({})", dir.literal("lines.txt"))),
        Value::List(vec![
            Value::Text("a".to_string()),
            Value::Text(String::new()),
            Value::Text("b".to_string()),
        ]),
        "three lines: a, a blank one, and b — the trailing newline adds none"
    );

    dir.write_file("crlf.txt", "a\r\nb\r\n");
    assert_eq!(
        eval(&format!("files.lines({})", dir.literal("crlf.txt"))),
        Value::List(vec![
            Value::Text("a".to_string()),
            Value::Text("b".to_string())
        ]),
        "a CRLF file must not leave a carriage return on each line"
    );
}

/// `files.copy` and `files.rename` move content and report nothing.
#[test]
fn files_copy_and_rename_move_content() {
    let dir = TempDir::new("files-move");
    dir.write_file("from.txt", "payload");
    assert_eq!(
        eval(&format!(
            "files.copy({}, {})",
            dir.literal("from.txt"),
            dir.literal("copy.txt")
        )),
        Value::Nothing
    );
    assert_eq!(
        std::fs::read_to_string(dir.file("copy.txt")).expect("copy should exist"),
        "payload",
        "files.copy must copy the content"
    );
    assert!(
        dir.file("from.txt").exists(),
        "files.copy must leave the original alone"
    );

    assert_eq!(
        eval(&format!(
            "files.rename({}, {})",
            dir.literal("from.txt"),
            dir.literal("moved.txt")
        )),
        Value::Nothing
    );
    assert!(
        !dir.file("from.txt").exists(),
        "files.rename must remove the old name"
    );
    assert_eq!(
        eval(&format!("files.read({})", dir.literal("moved.txt"))),
        Value::Text("payload".to_string()),
        "files.rename must keep the content"
    );
}

/// `files.exists` and `files.delete` bracket a file's life.
#[test]
fn files_exists_and_delete_bracket_a_files_life() {
    let dir = TempDir::new("files-delete");
    let literal = dir.literal("doomed.txt");
    assert_eq!(
        eval(&format!("files.exists({})", literal)),
        Value::YesNo(false),
        "a file that was never written does not exist"
    );
    dir.write_file("doomed.txt", "x");
    assert_eq!(
        eval(&format!("files.exists({})", literal)),
        Value::YesNo(true)
    );
    assert_eq!(eval(&format!("files.delete({})", literal)), Value::Nothing);
    assert_eq!(
        eval(&format!("files.exists({})", literal)),
        Value::YesNo(false),
        "a deleted file does not exist"
    );
}

/// A missing file is a catchable `Io` error naming the path — not a panic, not
/// an empty text, and not something that ends the process.
#[test]
fn edge_files_read_of_a_missing_file_is_a_catchable_error() {
    let dir = TempDir::new("files-missing");
    let literal = dir.literal("not-here.txt");
    match eval_err(&format!("files.read({})", literal)) {
        Error::Io(message) => assert!(
            message.contains("not-here.txt"),
            "the error must name the path it could not read, got `{}`",
            message
        ),
        other => panic!("a missing file must be an Io error, got {:?}", other),
    }

    // Catchable from Redblue, which is what makes it testable in a program.
    let caught = eval(&format!(
        "set note to \"not caught\"\n\
         try\n    files.read({literal})\n\
         catch error\n    set note to \"caught\"\n\
         end\n\
         note",
        literal = literal
    ));
    assert_eq!(
        caught,
        Value::Text("caught".to_string()),
        "try ... catch error must be able to see a missing file"
    );

    for source in [
        format!("files.delete({})", literal),
        format!("files.lines({})", literal),
        format!("files.copy({}, {})", literal, dir.literal("out.txt")),
        format!("files.rename({}, {})", literal, dir.literal("out.txt")),
    ] {
        assert!(
            matches!(eval_err(&source), Error::Io(_)),
            "`{}` must fail with an Io error when the file is missing",
            source
        );
    }
}

/// A path whose parent is a file, a path holding a NUL byte, and a directory
/// read as a file: three ways of naming something unreadable, none of which may
/// panic.
#[test]
fn edge_files_paths_that_name_nothing_readable_are_errors() {
    let dir = TempDir::new("files-bad-paths");
    dir.write_file("plain.txt", "x");
    let through_a_file = dir.file("plain.txt").join("under.txt");
    assert!(
        matches!(
            eval_err(&format!(
                "files.read({})",
                redblue_text(&through_a_file.to_string_lossy())
            )),
            Error::Io(_)
        ),
        "a path under a regular file is not readable"
    );

    // A NUL byte cannot be in a path at all; it must be an error, not a panic.
    assert!(
        matches!(
            eval_err("files.read(\"bad\\u0000path\")"),
            Error::Io(_) | Error::Runtime(..)
        ),
        "a NUL byte in a path must be a clean error, got a panic or Ok"
    );

    assert!(
        matches!(
            eval_err(&format!(
                "files.read({})",
                redblue_text(&dir.path.to_string_lossy())
            )),
            Error::Io(_)
        ),
        "reading a directory is an error, not its contents"
    );
}

/// A file this process may not read is an error. The check is conditional on
/// privilege because `root` bypasses the permission bits entirely: DAC has
/// nothing to say to uid 0, so the assertion would be meaningless there rather
/// than flaky.
#[test]
fn edge_files_unreadable_file_is_an_error_when_unprivileged() {
    let dir = TempDir::new("files-permission");
    let secret = dir.write_file("secret.txt", "classified");
    set_private(&secret);

    match std::fs::read_to_string(&secret) {
        // Unprivileged: the VM must answer the same way the host does.
        Err(_) => match eval_err(&format!(
            "files.read({})",
            redblue_text(&secret.to_string_lossy())
        )) {
            Error::Io(message) => assert!(
                message.contains("secret.txt"),
                "the error must name the unreadable file, got `{}`",
                message
            ),
            other => panic!("an unreadable file must be an Io error, got {:?}", other),
        },
        // Privileged: the file is readable, so the honest assertion is that the
        // module reads what the host reads and does not invent an error.
        Ok(_) => assert_eq!(
            eval(&format!(
                "files.read({})",
                redblue_text(&secret.to_string_lossy())
            )),
            Value::Text("classified".to_string()),
            "a privileged process reads what the host reads"
        ),
    }
}

/// Makes `path` readable by its owner and nobody else.
fn set_private(path: &std::path::Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("permissions should be settable");
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
}

/// `files` resolves a relative path against the process's working directory and
/// does not confine it: `..` is not a sandbox escape error, it is an ordinary
/// path segment. This pins that, so a future sandbox is a deliberate change
/// rather than an accident.
#[test]
fn files_paths_are_relative_to_the_working_directory_and_not_confined() {
    assert_eq!(
        eval("files.exists(\"target/../Cargo.toml\")"),
        Value::YesNo(true),
        "a path with `..` in it resolves like any other path"
    );
    assert_eq!(
        eval("files.exists(\"does/not/exist.txt\")"),
        Value::YesNo(false),
        "a missing path is missing, not an error"
    );
    assert!(
        matches!(eval_err("files.read(\"does/not/exist.txt\")"), Error::Io(_)),
        "reading a missing path must be an error, not a fabricated value"
    );
}

/// The `files` functions refuse a non-text argument instead of coercing it.
///
/// A call with an argument too few is refused by the *count* rather than by the
/// type of the argument that is missing, so the two say different things. Both
/// are refusals, and both name the function, which is what the list below
/// asserts.
#[test]
fn edge_files_argument_of_the_wrong_type_is_an_error() {
    for (source, part) in [
        ("files.read(5)", "requires a text path"),
        ("files.exists(nothing)", "requires a text path"),
        ("files.lines([1])", "requires a text path"),
        ("files.delete(1)", "requires a text path"),
        ("files.write(\"p\")", "takes 2 argument(s), given 1"),
        ("files.write(1, 2)", "requires two text arguments"),
        ("files.append(\"p\", 3)", "requires two text arguments"),
        ("files.copy(\"a\")", "takes 2 argument(s), given 1"),
        ("files.rename(\"a\", 2)", "requires two text arguments"),
    ] {
        assert_runtime_error(source, part);
    }
}

// ------------------------------------------------------------------ json

/// `csv.parse` must keep a quoted field whole: a comma inside quotes is data,
/// not a separator.
#[test]
fn csv_quoted_field_with_embedded_comma_is_one_cell() {
    assert_eq!(
        eval("csv.parse(\"a,\\\"b,c\\\",d\")"),
        rows(&[&["a", "b,c", "d"]]),
        "a quoted field holding a comma must be one cell, not two"
    );
}

/// A quoted field keeps the spaces inside the quotes; a bare field is trimmed.
#[test]
fn csv_quoted_field_keeps_its_inner_spaces() {
    assert_eq!(
        eval("csv.parse(\"\\\"  b c  \\\" , d\")"),
        rows(&[&["  b c  ", "d"]]),
        "spaces inside quotes are data, spaces around a bare field are not"
    );
}

/// A doubled quote inside a quoted field is one literal quote.
#[test]
fn csv_doubled_quote_is_one_literal_quote() {
    assert_eq!(
        eval("csv.parse(\"\\\"say \\\"\\\"hi\\\"\\\"\\\",b\")"),
        rows(&[&["say \"hi\"", "b"]]),
        "a doubled quote inside a quoted field is one quote"
    );
}

/// A newline inside a quoted field is part of the cell, so one record can span
/// two lines.
#[test]
fn csv_quoted_field_may_contain_a_newline() {
    assert_eq!(
        eval("csv.parse(\"a,\\\"b\\nc\\\",d\")"),
        rows(&[&["a", "b\nc", "d"]]),
        "a newline inside quotes must not end the row"
    );
}

/// A CRLF file and an LF file parse to the same value, and neither leaves a
/// carriage return on a cell or an empty row at the end.
#[test]
fn csv_parses_crlf_and_lf_the_same() {
    assert_eq!(
        eval("csv.parse(\"a,b\\r\\n1,2\\r\\n\")"),
        rows(&[&["a", "b"], &["1", "2"]]),
        "CRLF line endings must not leak into the cells"
    );
    assert_eq!(
        eval("csv.parse(\"a,b\\n1,2\\n\")"),
        eval("csv.parse(\"a,b\\r\\n1,2\\r\\n\")"),
        "CRLF and LF are the same document"
    );
    assert_eq!(
        eval("csv.parse(\"a,b\\r1,2\")"),
        rows(&[&["a", "b"], &["1", "2"]]),
        "a lone CR ends a row too"
    );
}

/// Rows may be ragged: each row carries the cells it actually has, so a short
/// row is short rather than padded and never an error.
#[test]
fn edge_csv_ragged_rows_keep_their_own_cells() {
    assert_eq!(
        eval("csv.parse(\"a,b,c\\nd,e\\nf\")"),
        rows(&[&["a", "b", "c"], &["d", "e"], &["f"]]),
        "ragged rows keep their own cells"
    );
    assert_eq!(
        eval("length(csv.parse(\"a,b,c\\nd\")[1])"),
        Value::Number(1.0),
        "a ragged row is as long as it was written"
    );
}

/// Empty text has no rows; one blank line is one row with one empty cell.
#[test]
fn edge_csv_empty_text_and_a_blank_line() {
    assert_eq!(
        eval("csv.parse(\"\")"),
        Value::List(Vec::new()),
        "empty text is no rows"
    );
    assert_eq!(
        eval("csv.parse(\"\\n\")"),
        rows(&[&[""]]),
        "one blank line is one empty row"
    );
    assert_eq!(
        eval("csv.parse(\"a\")"),
        rows(&[&["a"]]),
        "a single field with no separator is one cell"
    );
    assert_eq!(
        eval("csv.parse(\",,\")"),
        rows(&[&["", "", ""]]),
        "empty cells are cells, and the row keeps all three"
    );
}

/// A quote that is never closed is malformed input, reported as such and
/// catchable, rather than swallowing the rest of the document.
#[test]
fn edge_csv_unterminated_quote_is_an_error() {
    assert_runtime_error("csv.parse(\"a,\\\"b,c\")", "unterminated quoted field");
    assert_eq!(
        eval(
            "set note to \"not caught\"\n\
             try\n    csv.parse(\"\\\"open\")\n\
             catch error\n    set note to \"caught\"\n\
             end\n\
             note"
        ),
        Value::Text("caught".to_string()),
        "an unterminated quoted field must be catchable"
    );
}

/// A cell keeps its emoji, CJK, RTL and combining marks through the parse.
#[test]
fn csv_unicode_cells_survive_the_parse() {
    assert_eq!(
        eval("csv.parse(\"\u{1f389},\u{4e2d}\u{6587}\")"),
        rows(&[&["\u{1f389}", "\u{4e2d}\u{6587}"]]),
        "a quoted emoji cell must survive"
    );
    assert_eq!(
        eval("csv.parse(\"\u{202b}e\u{301}\")"),
        rows(&[&["\u{202b}e\u{301}"]]),
        "an RTL mark and a combining mark are data"
    );
}

/// `csv.parse` refuses a non-text argument.
#[test]
fn edge_csv_argument_of_the_wrong_type_is_an_error() {
    assert_runtime_error("csv.parse(5)", "csv.parse requires text");
    assert_runtime_error("csv.parse([1, 2])", "csv.parse requires text");
}

/// A row has no cell at an index past its end, which is a clean runtime error
/// naming the index and the length — never a panic and never a `nothing` that
/// looks like data.
#[test]
fn edge_csv_row_index_out_of_bounds_is_a_clean_error() {
    let ragged = "set rows to csv.parse(\"a,b\\nc\")\nrows[1][5]";
    assert_runtime_error(ragged, "out of bounds");
    let too_many_rows = "set rows to csv.parse(\"a\\n\")\nrows[9]";
    assert_runtime_error(too_many_rows, "out of bounds");
}

/// JSON's `\uXXXX` escape decodes to the character it names. It used to be
/// dropped, so the escape's letters were returned as text: `\u0041` came back
/// as `u0041`.
#[test]
fn json_unicode_escape_decodes_to_its_character() {
    assert_eq!(
        eval("json.parse(\"\\\"\\\\u0041\\\"\")"),
        Value::Text("A".to_string()),
        "\\u0041 is the character U+0041"
    );
    assert_eq!(
        eval("json.parse(\"\\\"\\\\u00e9\\\\u4e2d\\\\ud83c\\\\udf89\\\"\")"),
        Value::Text("\u{e9}\u{4e2d}\u{1f389}".to_string()),
        "a BMP escape, a CJK escape and a surrogate pair all decode"
    );
    assert_eq!(
        eval("json.parse(\"\\\"\\\\u0000\\\\b\\\\f\\\"\")"),
        Value::Text("\u{0}\u{8}\u{c}".to_string()),
        "\\u0000, \\b and \\f decode too"
    );
}

/// `json.stringify` must escape a record's keys as well as its values. It did
/// not, so a key holding `"` or a newline produced output no JSON parser could
/// read back.
#[test]
fn json_record_keys_are_escaped_when_written() {
    let parsed = eval("json.parse(\"{\\\"a\\\\\\\"b\\\": 1, \\\"line\\\\nbreak\\\": 2}\")");
    assert_eq!(
        parsed,
        Value::Record(
            [
                ("a\"b".to_string(), Value::Number(1.0)),
                ("line\nbreak".to_string(), Value::Number(2.0)),
            ]
            .into_iter()
            .collect()
        ),
        "the record parses to the two keys it spells"
    );
    let source =
        "set parsed to json.parse(\"{\\\"a\\\\\\\"b\\\": 1, \\\"line\\\\nbreak\\\": 2}\")\n\
                  json.stringify(parsed)";
    assert_eq!(
        eval(source),
        Value::Text("{\"a\\\"b\": 1, \"line\\nbreak\": 2}".to_string()),
        "a key holding a quote or a newline must come back escaped"
    );
}

/// What `json.stringify` writes, `json.parse` reads back: a round trip over
/// nesting, unicode, escapes and empty containers.
#[test]
fn json_round_trips_nesting_unicode_and_empty_containers() {
    // The document as the lexer sees it: `\"` is one quote inside the Redblue
    // literal, `\t` and `\n` are the characters they name.
    let document = concat!(
        r#"{\"list\": [1, [2, 3], {\"deep\": {}}],"#,
        r#" \"empty_list\": [], \"empty_record\": {},"#,
        r#" \"text\": \"a\tb\nc\", \"yes\": true, \"nothing\": null}"#,
    );
    let expected = concat!(
        r#"{"list": [1, [2, 3], {"deep": {}}], "empty_list": [],"#,
        r#" "empty_record": {}, "text": "a\tb\nc","#,
        r#" "yes": true, "nothing": null}"#,
    );
    let source = format!(
        "set original to json.parse(\"{document}\")\n\
         set written to json.stringify(original)\n\
         expect json.parse(written) to be original\n\
         written"
    );
    assert_eq!(
        eval(&source),
        Value::Text(expected.to_string()),
        "the round trip must not change the document, empty containers included"
    );
}

/// A record's fields keep their order through the round trip, so the output is
/// not dependent on a hash.
#[test]
fn json_stringify_keeps_field_order() {
    assert_eq!(
        eval("json.stringify(json.parse(\"{\\\"b\\\": 1, \\\"a\\\": 2, \\\"c\\\": 3}\"))"),
        Value::Text("{\"b\": 1, \"a\": 2, \"c\": 3}".to_string()),
        "fields must be written in the order they were parsed"
    );
}

/// A document of exactly one element, and every JSON scalar, round trip.
#[test]
fn edge_json_singleton_and_scalar_values() {
    assert_eq!(
        eval("json.parse(\"[7]\")"),
        Value::List(vec![Value::Number(7.0)]),
        "a one-element array is a one-element list"
    );
    assert_eq!(eval("json.parse(\"null\")"), Value::Nothing);
    assert_eq!(eval("json.parse(\"true\")"), Value::YesNo(true));
    assert_eq!(eval("json.parse(\"false\")"), Value::YesNo(false));
    assert_eq!(eval("json.parse(\"\\\"\\\"\")"), Value::Text(String::new()));
    assert_eq!(eval("json.stringify([])"), Value::Text("[]".to_string()));
    assert_eq!(eval("json.stringify({})"), Value::Text("{}".to_string()));
}

/// Numbers at the edges of what a Redblue number can hold: the precision limit,
/// a negative zero, and a magnitude too large to be a number at all.
#[test]
fn edge_json_numeric_boundaries() {
    assert_eq!(
        eval("json.parse(\"9007199254740993\")"),
        eval("json.parse(\"9007199254740992\")"),
        "2^53 and 2^53+1 are the same f64, which is the documented limit"
    );
    assert_eq!(
        eval("json.parse(\"-0\")"),
        Value::Number(-0.0),
        "negative zero is a number"
    );
    assert_eq!(
        eval("json.parse(\"1.5\")"),
        Value::Number(1.5),
        "a fraction is not truncated to an integer"
    );
    assert_runtime_error("json.parse(\"1e400\")", "not a finite number");
    assert_runtime_error("json.parse(\"-1e400\")", "not a finite number");
}

/// A field the document does not have reads as `nothing`, and a field that is
/// there reads as its value: the difference is what `is nothing` is for.
#[test]
fn edge_json_missing_field_is_nothing_and_present_field_is_not() {
    assert_eq!(
        eval("set parsed to json.parse(\"{\\\"a\\\": 1}\")\nparsed.missing is nothing"),
        Value::YesNo(true),
        "a field the document does not have is nothing"
    );
    assert_eq!(
        eval("set parsed to json.parse(\"{\\\"a\\\": 1}\")\nparsed.a is nothing"),
        Value::YesNo(false),
        "a field the document has is not nothing"
    );
}

/// The same key twice is one key holding the last value, and the key order of
/// first appearance is kept.
#[test]
fn edge_json_duplicate_key_keeps_the_last_value() {
    assert_eq!(
        eval("set parsed to json.parse(\"{\\\"a\\\": 1, \\\"a\\\": 2}\")\nparsed.a"),
        Value::Number(2.0),
        "the later value wins, as JSON parsers do"
    );
    assert_eq!(
        eval("json.stringify(json.parse(\"{\\\"a\\\": 1, \\\"a\\\": 2}\"))"),
        Value::Text("{\"a\": 2}".to_string()),
        "a repeated key is written once"
    );
}

/// Malformed JSON is an error, never a partly-built value.
#[test]
fn edge_json_malformed_input_is_an_error() {
    for source in [
        "json.parse(\"{\")",
        "json.parse(\"{\\\"a\\\": 1\")",
        "json.parse(\"{\\\"a\\\":}\")",
        "json.parse(\"[1, 2\")",
        "json.parse(\"\\\"unterminated\")",
        "json.parse(\"nul\")",
        "json.parse(\"{\\\"a\\\" 1}\")",
        "json.parse(\"not json at all\")",
    ] {
        assert_runtime_error(source, "JSON");
    }
}

/// An escape JSON does not define is an error, not a silent rewrite of the
/// text: `\q` used to parse as `q`.
#[test]
fn edge_unknown_json_escape_is_an_error_not_silent_corruption() {
    assert_runtime_error("json.parse(\"\\\"a\\\\qb\\\"\")", "Invalid JSON escape");
    assert_runtime_error("json.parse(\"\\\"a\\\\\\\"\")", "Invalid JSON escape");
    assert_runtime_error("json.parse(\"\\\"\\\\uZZZZ\\\"\")", "needs four hex digits");
    assert_runtime_error("json.parse(\"\\\"\\\\u00\\\"\")", "needs four hex digits");
    assert_runtime_error("json.parse(\"\\\"\\\\ud83c\\\"\")", "unpaired surrogate");
    assert_runtime_error(
        "json.parse(\"\\\"\\\\udc00\\\\ud83c\\\\udf89\\\"\")",
        "unpaired surrogate",
    );
}

/// A control character in a value is written as its `\u00XX` escape, because a
/// literal one is not legal JSON.
#[test]
fn edge_json_control_characters_are_escaped_on_write() {
    assert_eq!(
        eval("json.stringify(\"a\u{1}b\")"),
        Value::Text("\"a\\u0001b\"".to_string()),
        "a control character must be written as \\u00XX"
    );
    assert_eq!(
        eval("json.parse(json.stringify(\"a\u{1}b\u{7f}\"))"),
        Value::Text("a\u{1}b\u{7f}".to_string()),
        "and must read back unchanged"
    );
}

/// Non-ASCII text is written as itself: JSON is UTF-8 and an emoji needs no
/// escape to survive.
#[test]
fn json_unicode_text_survives_stringify_unchanged() {
    let label = "\u{1f389} \u{4e2d}\u{6587} \u{202b}e\u{301}";
    assert_eq!(
        eval(&format!("json.stringify({})", redblue_text(label))),
        Value::Text(format!("\"{}\"", label)),
        "unicode text is written as itself"
    );
    assert_eq!(
        eval(&format!(
            "set text to {}\njson.parse(json.stringify(text))",
            redblue_text(label)
        )),
        Value::Text(label.to_string()),
        "and reads back identical"
    );
}

/// `json.parse` and `json.stringify` refuse a value of the wrong type.
#[test]
fn edge_json_argument_of_the_wrong_type_is_an_error() {
    assert_runtime_error("json.parse(5)", "json.parse requires text");
    assert_runtime_error("json.parse([1])", "json.parse requires text");
}

/// The network client must not wait forever: `reqwest::blocking::Client::new()`
/// has no timeout of its own, so a black-holed address left the program running
/// until the host killed it, with nothing catchable and nothing assertable.
#[test]
fn edge_network_get_to_a_black_holed_address_gives_up() {
    // 10.255.255.1 is RFC 5737 TEST-NET-3: either it is unreachable and this
    // returns at once, or packets are dropped and only the client's own timeout
    // can end the wait. Either way the program must end, and end in an error.
    let started = Instant::now();
    let error = eval_err("network.get(\"http://10.255.255.1:81/\")");
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(NETWORK_TIMEOUT_SECS + 5),
        "network.get took {:?}, longer than the documented timeout of {}s",
        elapsed,
        NETWORK_TIMEOUT_SECS
    );
    assert!(
        matches!(error, Error::Runtime(..)),
        "an unreachable host must be a Runtime error, got {:?}",
        error
    );
}

/// The timeouts the network module uses are published, positive, and shorter
/// than the operating system's own connect timeout, which is what made the
/// previous behaviour a two-and-a-half-minute wait.
#[test]
fn network_timeouts_are_published_and_bounded() {
    // The constants are the policy, so these are checked when the test binary
    // is built: a change that made one non-positive, or longer than the minute
    // a request may take, would fail the build rather than wait to be noticed.
    const {
        assert!(NETWORK_TIMEOUT_SECS > 0, "a request needs a timeout");
        assert!(
            NETWORK_TIMEOUT_SECS <= 60,
            "a request may not take longer than a minute"
        );
        assert!(
            NETWORK_CONNECT_TIMEOUT_SECS > 0,
            "a connect needs a timeout"
        );
        assert!(
            NETWORK_CONNECT_TIMEOUT_SECS <= NETWORK_TIMEOUT_SECS,
            "giving up on a connect is part of giving up on the request"
        );
    }

    // And the published constant is the one in force: a request to a
    // black-holed address ends inside it, with an error naming the request.
    let started = Instant::now();
    match eval_err("network.get(\"http://10.255.255.1:81/\")") {
        Error::Runtime(message, _) => assert!(
            message.contains("HTTP request failed"),
            "the failure must name the request, got `{}`",
            message
        ),
        other => panic!(
            "an unreachable host must be a Runtime error, got {:?}",
            other
        ),
    }
    assert!(
        started.elapsed() < Duration::from_secs(NETWORK_TIMEOUT_SECS + 5),
        "the request took {:?}, longer than the published {}s",
        started.elapsed(),
        NETWORK_TIMEOUT_SECS
    );
}

/// The network functions refuse an argument that is not a URL, and a URL the
/// client cannot parse, without opening a socket.
///
/// `network.post` with one argument is refused by its count rather than by the
/// URL it was given, so that entry pins the count and the one after it pins the
/// type.
#[test]
fn edge_network_rejects_an_argument_that_is_not_a_url() {
    for (source, part) in [
        ("network.get(5)", "requires a URL"),
        ("network.get(nothing)", "requires a URL"),
        (
            "network.post(\"http://example.invalid\")",
            "takes 2 argument(s), given 1",
        ),
        ("network.post(5, 5)", "requires URL and data"),
    ] {
        assert_runtime_error(source, part);
    }

    // A URL with no scheme is refused by the client before any connection.
    let started = Instant::now();
    assert!(matches!(
        eval_err("network.get(\"not a url\")"),
        Error::Runtime(..)
    ));
    assert!(
        started.elapsed() < Duration::from_secs(NETWORK_TIMEOUT_SECS),
        "an unparseable URL must fail without waiting for the network"
    );
}
