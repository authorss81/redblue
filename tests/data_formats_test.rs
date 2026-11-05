//! Rounding out the data formats: a CSV writer, JSON Lines, TOML and archives.
//!
//! The CSV half is pinned in both directions. A writer that quotes correctly
//! and a reader that disagrees about the quoting are two failures that pass a
//! one-sided test, so every value written here is read back through the
//! `csv.parse` that already shipped.

use std::path::{Path, PathBuf};

use redblue::{run_source, run_source_value, Value};

fn eval(source: &str) -> Value {
    run_source_value(source).unwrap_or_else(|error| {
        panic!("the program should answer, but failed: {error}\n--- source\n{source}")
    })
}

fn text(value: &str) -> Value {
    Value::Text(value.to_string())
}

fn rows(rows: &[&[&str]]) -> Value {
    Value::list(
        rows.iter()
            .map(|row| {
                Value::list(
                    row.iter()
                        .map(|cell| Value::Text((*cell).to_string()))
                        .collect(),
                )
            })
            .collect(),
    )
}

fn refusal(source: &str) -> String {
    let error = run_source(source).expect_err("the program must be refused");
    error.message().to_string()
}

/// One test's own scratch directory, so the suite cannot race itself.
fn scratch(tag: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/tmp/phase105")
        .join(tag);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// A field is quoted for the four reasons CSV has for quoting it, and each
/// expected document is written out literally rather than derived from the
/// writer.
#[test]
fn csv_writer_quotes_a_comma_a_quote_a_newline_and_space() {
    let written = eval(concat!(
        "csv.stringify([[\"plain\", \"a,b\", \"say \\\"hi\\\"\", ",
        "\"line\\nbreak\", \"  pad  \", \"   \"]])"
    ));
    assert_eq!(
        written,
        text("plain,\"a,b\",\"say \"\"hi\"\"\",\"line\nbreak\",\"  pad  \",\"   \""),
        "the writer must quote commas, quotes, newlines and surrounding space"
    );
}

/// What the writer writes, the reader already in the tree must read back as
/// the same rows. This is the half a one-sided test cannot see.
#[test]
fn edge_csv_writer_round_trips_through_csv_parse() {
    let original = rows(&[&[
        "a,b",
        "say \"hi\"",
        "line\nbreak",
        "  pad  ",
        "",
        "   ",
        "emoji 🎉",
        "日本語",
    ]]);
    let back = eval(concat!(
        "csv.parse(csv.stringify([[\"a,b\", \"say \\\"hi\\\"\", ",
        "\"line\\nbreak\", \"  pad  \", \"\", \"   \", ",
        "\"emoji 🎉\", \"日本語\"]]))"
    ));
    assert_eq!(
        back, original,
        "csv.parse must read what csv.stringify wrote"
    );
}

/// Empty text is no rows and a header-only document is one row; a field that is
/// only whitespace and a very long field are both data. The BOM is preserved as
/// the first character of a cell rather than becoming structure.
#[test]
fn edge_csv_empty_header_only_whitespace_long_and_bom() {
    assert_eq!(
        eval("csv.stringify([])"),
        text(""),
        "no rows is the empty document, which csv.parse reads as no rows"
    );
    assert_eq!(
        eval("csv.stringify([[\"name\", \"age\"]])"),
        text("name,age"),
        "a header-only document is one row and no trailing newline"
    );

    let long = "x".repeat(100_000);
    let back = eval(&format!("csv.parse(csv.stringify([[\"{long}\"]]))"));
    assert_eq!(
        back,
        rows(&[&[long.as_str()]]),
        "a very long field survives the writer and the reader"
    );

    let bom = "\u{feff}name,value";
    let back = eval(&format!("csv.parse(csv.stringify([[\"{bom}\", \"one\"]]))"));
    assert_eq!(
        back,
        rows(&[&[bom, "one"]]),
        "a BOM is data in a cell, not something either half silently drops"
    );
}

/// JSON Lines writes one JSON value per line and reads it back as a list. A
/// newline inside a record is escaped by the JSON writer, so it cannot break
/// the one-record-per-line structure.
#[test]
fn json_lines_round_trip_and_escape_an_inner_newline() {
    let written = eval(concat!(
        "set rows to [{name: \"a\", note: \"line\\nbreak\"}, ",
        "{name: \"b\", note: \"\"}]\n",
        "json.stringify_lines(rows)"
    ));
    assert_eq!(
        written,
        text("{\"name\": \"a\", \"note\": \"line\\nbreak\"}\n{\"name\": \"b\", \"note\": \"\"}"),
        "a record per line, and a newline inside one is escaped"
    );

    let back = eval(concat!(
        "set rows to [{name: \"a\", note: \"line\\nbreak\"}, ",
        "{name: \"b\", note: \"\"}]\n",
        "json.parse_lines(json.stringify_lines(rows))"
    ));
    assert_eq!(
        back,
        eval("[{name: \"a\", note: \"line\\nbreak\"}, {name: \"b\", note: \"\"}]"),
        "JSON Lines must round-trip the list of records"
    );
}

/// Empty JSON Lines is no records; a malformed line names the line it was on
/// rather than leaving the caller to guess.
#[test]
fn edge_json_lines_empty_and_malformed_line() {
    assert_eq!(
        eval("json.parse_lines(\"\")"),
        Value::list(Vec::new()),
        "an empty JSON Lines file has no records"
    );
    assert_eq!(
        eval("json.parse_lines(\"{\\\"a\\\": 1}\\n\\n{\\\"b\\\": 2}\\n\")"),
        eval("[{a: 1}, {b: 2}]"),
        "blank lines and a trailing newline are not records"
    );

    let message = refusal("json.parse_lines(\"{\\\"a\\\": 1}\\nnot json\\n\")");
    assert!(
        message.contains("line 2"),
        "the refusal must name the bad line, got: {message}"
    );
}

/// The documented TOML subset: comments, bare and quoted keys, dotted keys,
/// tables, arrays of tables, basic and literal strings, integers, floats,
/// booleans and arrays.
#[test]
fn toml_reader_parses_the_documented_subset() {
    let document = concat!(
        "# a comment\\n",
        "title = \\\"Redblue\\\"\\n",
        "enabled = true\\n",
        "count = 42\\n",
        "ratio = 1.5\\n",
        "tags = [\\\"a\\\", \\\"b\\\"]\\n",
        "nested.value = 7\\n",
        "[server]\\n",
        "host = 'localhost'\\n",
        "ports = [80, 443]\\n",
        "[[users]]\\n",
        "name = \\\"Ada\\\"\\n",
        "[[users]]\\n",
        "name = \\\"Grace\\\"\\n"
    );
    let parsed = eval(&format!("toml.parse(\"{document}\")"));
    let expected = eval(concat!(
        "{title: \"Redblue\", enabled: yes, count: 42, ratio: 1.5, ",
        "tags: [\"a\", \"b\"], nested: {value: 7}, ",
        "server: {host: \"localhost\", ports: [80, 443]}, ",
        "users: [{name: \"Ada\"}, {name: \"Grace\"}]}"
    ));
    assert_eq!(parsed, expected, "the TOML reader must answer the document");
}

/// Hexadecimal integers are integers even when their digits spell an
/// exponent, so `0xDEAD`, `0xCAFE` and `0x1e5` must not be sent to a float
/// parser.
#[test]
fn edge_toml_hexadecimal_integers_with_e_are_not_floats() {
    let parsed = eval("toml.parse(\"dead = 0xDEAD\\ncafe = 0xCAFE\\nsmall = 0x1e5\")");
    assert_eq!(
        parsed,
        eval("{dead: 57005, cafe: 51966, small: 485}"),
        "hexadecimal integers with E or A-F digits must parse as integers"
    );
}

/// Nesting is bounded by an explicit limit with a line-numbered refusal rather
/// than by the process stack, and an overlong dotted key is refused the same
/// way.
#[test]
fn edge_toml_deep_nesting_and_long_key_paths_are_refused() {
    let nested = format!("a = {}1{}\n", "[".repeat(200), "]".repeat(200));
    let message = refusal(&format!("toml.parse(\"{nested}\")"));
    assert!(
        message.contains("line 1") && message.contains("nest"),
        "deep nesting must be refused with a line, got: {message}"
    );

    let key = (0..150)
        .map(|index| format!("k{index}"))
        .collect::<Vec<_>>()
        .join(".");
    let document = format!("{key} = 1\n");
    let message = refusal(&format!("toml.parse(\"{document}\")"));
    assert!(
        message.contains("line 1") && message.contains("key path"),
        "an overlong key path must be refused with a line, got: {message}"
    );
}

/// A malformed TOML file is a catchable error that names its line. A leading
/// BOM is accepted rather than becoming a malformed key.
#[test]
fn edge_toml_malformed_and_bom() {
    let message = refusal("toml.parse(\"title = \\\"ok\\\"\\ncount = \\n\")");
    assert!(
        message.contains("line 2"),
        "a malformed value must name its line, got: {message}"
    );

    let duplicate = refusal("toml.parse(\"a = 1\\na = 2\\n\")");
    assert!(
        duplicate.contains("line 2") && duplicate.contains("duplicate"),
        "a duplicate key must be refused and name its line, got: {duplicate}"
    );

    let parsed = eval(concat!(
        "toml.parse(\"",
        "\u{feff}",
        "name = \\\"Ada\\\"\\n\")"
    ));
    assert_eq!(
        parsed,
        eval("{name: \"Ada\"}"),
        "a BOM at the start of a TOML file is not part of the first key"
    );
}

/// A zip and a tar each write and read back the same names and contents.
#[test]
fn zip_and_tar_round_trip_the_same_files() {
    let dir = scratch("archives");
    let zip = slash(&dir.join("bundle.zip"));
    let tar = slash(&dir.join("bundle.tar"));

    for (module, path) in [("zip", &zip), ("tar", &tar)] {
        let source = format!(
            "set files to json.parse(\"{{\\\"a.txt\\\": \\\"alpha\\\", \\\"dir/b.txt\\\": \\\"beta\\\\nline\\\"}}\")\n\
             {module}.write(\"{path}\", files)\n\
             {module}.read(\"{path}\")"
        );
        assert_eq!(
            eval(&source),
            eval("json.parse(\"{\\\"a.txt\\\": \\\"alpha\\\", \\\"dir/b.txt\\\": \\\"beta\\\\nline\\\"}\")"),
            "{module} must write and read the same file list and contents"
        );
    }

    assert!(dir.join("bundle.zip").metadata().expect("zip exists").len() > 0);
    assert!(dir.join("bundle.tar").metadata().expect("tar exists").len() > 0);
}

/// An empty archive is still an archive, and empty entries are contents rather
/// than missing entries.
#[test]
fn edge_archive_empty_and_empty_entry() {
    let dir = scratch("archive_empty");
    let zip = slash(&dir.join("empty.zip"));
    let tar = slash(&dir.join("empty.tar"));

    for (module, path) in [("zip", &zip), ("tar", &tar)] {
        let empty =
            format!("{module}.write(\"{path}\", json.parse(\"{{}}\"))\n{module}.read(\"{path}\")");
        assert_eq!(
            eval(&empty),
            eval("json.parse(\"{}\")"),
            "{module} must read an empty archive as an empty record"
        );

        let entry = format!(
            "{module}.write(\"{path}\", json.parse(\"{{\\\"empty.txt\\\": \\\"\\\"}}\"))\n\
             {module}.read(\"{path}\")"
        );
        assert_eq!(
            eval(&entry),
            eval("json.parse(\"{\\\"empty.txt\\\": \\\"\\\"}\")"),
            "{module} must keep an empty entry as an entry"
        );
    }
}

/// Archives refuse a missing file and a non-archive input by name.
#[test]
fn edge_archive_rejects_missing_and_malformed_files() {
    let dir = scratch("archive_bad");
    for module in ["zip", "tar"] {
        let missing = slash(&dir.join(format!("missing.{module}")));
        let missing_message = refusal(&format!("{module}.read(\"{missing}\")"));
        assert!(
            missing_message.contains(&format!("{module}.read")),
            "a missing {module} archive must name the call, got: {missing_message}"
        );

        let bad = dir.join(format!("bad.{module}"));
        std::fs::write(&bad, format!("not a {module}")).expect("bad fixture");
        let message = refusal(&format!("{module}.read(\"{}\")", slash(&bad)));
        assert!(
            message.contains(&format!("not a {module} archive")),
            "a non-{module} must be refused as one, got: {message}"
        );
    }
}

/// The smallest i64 is a decimal integer, not an overflow: parsing the digits
/// without the sign used to reject `-9223372036854775808` while accepting every
/// other value in the range. Both out-of-range neighbours stay refused.
#[test]
fn toml_reader_parses_i64_minimum_and_refuses_decimal_overflow() {
    assert_eq!(
        eval("toml.parse(\"smallest = -9223372036854775808\")"),
        eval("{smallest: -9223372036854775808}"),
        "the smallest i64 is representable and must parse"
    );

    let too_big = run_source("toml.parse(\"too_big = 9223372036854775808\")");
    assert!(
        too_big.is_err(),
        "an integer past the largest i64 must be refused"
    );

    let message = refusal("toml.parse(\"too_small = -9223372036854775809\")");
    assert!(
        message.contains("line 1") && message.contains("integer"),
        "an integer past the smallest i64 must name its line, got: {message}"
    );
}

/// A carriage return and a CRLF are data inside a quoted cell, and non-ASCII
/// text is written and read back unchanged.
#[test]
fn edge_csv_writer_round_trips_carriage_return_and_unicode() {
    let cells = ["a\rb", "\r", "café", "漢字", "🙂", "line\r\nbreak"];
    let literals = cells
        .iter()
        .map(|cell| format!("{cell:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!("csv.parse(csv.stringify([[{literals}]]))");
    let expected = Value::list(vec![Value::list(
        cells.iter().map(|cell| text(cell)).collect(),
    )]);
    assert_eq!(
        eval(&source),
        expected,
        "CR, CRLF, accented, CJK and emoji cells must round-trip through csv.parse"
    );
}

/// JSON Lines needs a list, and a leading BOM is not part of the first record.
#[test]
fn edge_json_lines_rejects_a_non_list_and_accepts_a_bom() {
    let wrong = run_source("json.stringify_lines({a: 1})");
    assert!(wrong.is_err(), "a record is not a list of records");
    let message = refusal("json.stringify_lines({a: 1})");
    assert!(
        message.contains("list"),
        "the refusal must say a list is required, got: {message}"
    );

    let source = "json.parse_lines(\"\u{feff}1\\n\")";
    assert_eq!(
        eval(source),
        eval("[1]"),
        "a BOM at the start of a JSON Lines file is not part of the first value"
    );
}

/// Archive contents that are not UTF-8 come back as the same bytes that were
/// written, not as lossy text.
#[test]
fn edge_archive_binary_contents_round_trip_through_zip_and_tar() {
    let dir = scratch("archive_binary");
    for module in ["zip", "tar"] {
        let path = slash(&dir.join(format!("binary.{module}")));
        let source = format!(
            "{module}.write(\"{path}\", json.parse(\"{{\\\"data.bin\\\": \
             [0, 128, 255, 1, 254]}}\"))\n{module}.read(\"{path}\")"
        );
        assert_eq!(
            eval(&source),
            eval("json.parse(\"{\\\"data.bin\\\": [0, 128, 255, 1, 254]}\")"),
            "{module} must return the same bytes that were written"
        );
    }
}
