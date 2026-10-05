use std::fs;
use std::path::{Path, PathBuf};

use redblue::formatter::{self, Formatter};
use redblue::lexer::Lexer;
use redblue::parser::{self, Program};

/// Every `.rb` file in the tree that must survive formatting unchanged in
/// meaning: the language's specification-by-example, plus the test corpus.
fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in ["examples", "tests"] {
        let path = root.join(dir);
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("rb") {
                files.push(path);
            }
        }
    }
    assert!(
        !files.is_empty(),
        "no .rb corpus files found under examples/ or tests/"
    );
    files.sort();
    files
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e))
}

/// Runs `source` on the real binary and returns what it printed, so a claim
/// about behaviour can be checked against output rather than inferred from the
/// tree. Each run gets its own file inside `target/tmp/`.
fn run_program(source: &str) -> String {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = scratch_dir();
    let serial = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let path = dir.join(format!("run-{}-{}.rb", std::process::id(), serial));
    fs::write(&path, source).expect("program should be writable");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("run")
        .arg(&path)
        .output()
        .expect("the rb binary should run");
    let _ = fs::remove_file(&path);

    assert!(
        out.status.success(),
        "{} should run, got: {}",
        source,
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// What a child run of the `rb` binary produced.
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Where programs under test are written, inside the project checkout.
fn scratch_dir() -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/formatter");
    fs::create_dir_all(&dir).expect("scratch directory should be creatable");
    dir
}

/// Runs `source` with `rb <mode>` and returns what it produced. A diagnostic
/// quotes the path of the file it read, so that path is replaced with a fixed
/// marker and two runs of the same program stay comparable.
fn run_rb(mode: &str, source: &str) -> Run {
    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = scratch_dir();
    let serial = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let path = dir.join(format!("rb-{}-{}.rb", std::process::id(), serial));
    fs::write(&path, source).expect("program should be writable");

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg(mode)
        .arg(&path)
        .output()
        .expect("the rb binary should run");
    let marker = path.display().to_string();
    let _ = fs::remove_file(&path);

    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).replace(&marker, "<program>"),
        stderr: String::from_utf8_lossy(&out.stderr).replace(&marker, "<program>"),
    }
}

fn parse(source: &str) -> Program {
    let tokens = Lexer::tokenize(source).expect("source must lex");
    parser::parse(tokens).expect("source must parse")
}

/// Whether two programs mean the same thing: the same tree, compared without
/// source positions. Formatting moves text by definition, so `Span { … }` is
/// removed from the debug rendering of both trees before they are compared.
fn same_program(a: &Program, b: &Program) -> bool {
    strip_spans(&format!("{:?}", a)) == strip_spans(&format!("{:?}", b))
}

/// Removes every `Span { line: …, column: … }` from a debug rendering.
fn strip_spans(debug: &str) -> String {
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug;
    while let Some(start) = rest.find("Span { ") {
        let after = &rest[start + "Span { ".len()..];
        match after.find(" }") {
            Some(end) => {
                out.push_str(&rest[..start]);
                rest = &after[end + 2..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out
}

/// Formats, then formats the result again: the two must be byte-identical.
fn assert_idempotent(source: &str, label: &str) -> String {
    let once = formatter::format(source)
        .unwrap_or_else(|e| panic!("{}: first format failed: {}", label, e));
    let twice = formatter::format(&once)
        .unwrap_or_else(|e| panic!("{}: second format failed: {}", label, e));
    assert_eq!(
        once, twice,
        "{}: format is not idempotent\n--- once ---\n{}\n--- twice ---\n{}",
        label, once, twice
    );
    once
}

// ---------------------------------------------------------------------------
// Idempotence
// ---------------------------------------------------------------------------

#[test]
fn format_is_idempotent() {
    let source = "say \"Hello, World!\"\n";

    let once = formatter::format(source).expect("first format must succeed");
    let twice = formatter::format(&once).expect("second format must succeed");

    assert_eq!(
        once, twice,
        "formatting must be idempotent:\n--- once ---\n{}\n--- twice ---\n{}",
        once, twice
    );
}

#[test]
fn format_is_idempotent_over_the_whole_corpus() {
    for path in corpus() {
        let source = read(&path);
        assert_idempotent(&source, &path.display().to_string());
    }
}

// ---------------------------------------------------------------------------
// Losslessness: format then parse gives the same tree
// ---------------------------------------------------------------------------

#[test]
fn format_preserves_the_meaning_of_every_corpus_file() {
    for path in corpus() {
        let source = read(&path);
        let label = path.display().to_string();

        let formatted = formatter::format(&source)
            .unwrap_or_else(|e| panic!("{}: format failed: {}", label, e));
        let before = parse(&source);
        let after = parse(&formatted);

        assert!(
            same_program(&before, &after),
            "{}: formatting changed the program\n--- source ---\n{}\n--- formatted ---\n{}",
            label,
            source,
            formatted
        );
    }
}

#[test]
fn format_keeps_the_grouping_the_parentheses_forced() {
    // The AST drops parentheses, so the formatter has to put back the ones the
    // grouping depends on: without them this formats to `1 + 2 * 3`, which is
    // 7 rather than 9.
    let source = "set a to (1 + 2) * 3\nsay a\n";

    let formatted = assert_idempotent(source, "grouping");
    assert_eq!(
        formatted, "set a to (1 + 2) * 3\nsay a\n",
        "formatting must preserve the grouping"
    );
}

#[test]
fn format_keeps_right_hand_grouping_of_equal_precedence() {
    let source = "say 1 - (2 - 3)\nsay 10 - 2 - 3\n";

    let formatted = assert_idempotent(source, "equal precedence");
    assert_eq!(
        formatted, source,
        "a right operand that binds as loosely as its parent needs parentheses"
    );
}

#[test]
fn format_keeps_grouping_for_unary_and_every_operator_level() {
    let source = "\
set a to not (yes and no)
set b to -(2 * 3)
set c to (1 + 2) - 3
set d to 1 + (2 * 3)
set e to ((1 + 2) * (3 + 4))
set f to not (1 is 2)
set g to (yes and no) or yes
set h to 1 - (2 - 3)
set i to 10 % (2 * 3)
set j to 1 is not 2
say a
say b
say c
say d
say e
say f
say g
say h
say i
say j
";

    let formatted = assert_idempotent(source, "precedence levels");
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "grouping must survive formatting:\n{}",
        formatted
    );
}

#[test]
fn format_writes_index_with_brackets_not_at() {
    let source = "set xs to [10, 20, 30]\nsay xs[1]\n";

    let formatted = assert_idempotent(source, "index");
    assert_eq!(
        formatted, source,
        "the index form must be the one the parser reads back"
    );
}

#[test]
fn formatter_instance_does_not_carry_state_between_calls() {
    let mut formatter = Formatter::new();
    let first = formatter.format("say 1\n").expect("first format");
    let second = formatter.format("say 2\n").expect("second format");

    assert_eq!(first, "say 1\n", "first format");
    assert_eq!(
        second, "say 2\n",
        "a second source must not inherit the first one's text"
    );
}

// ---------------------------------------------------------------------------
// --check
// ---------------------------------------------------------------------------

#[test]
fn check_compares_exactly_including_the_trailing_newline() {
    let formatted = "say 1\n";

    assert!(
        !formatter::needs_reformat(formatted, formatted),
        "an already formatted file must pass"
    );
    assert!(
        formatter::needs_reformat("say 1", formatted),
        "a missing trailing newline is a difference"
    );
    assert!(
        formatter::needs_reformat("say 1\n\n", formatted),
        "a trailing blank line is a difference"
    );
    assert!(
        formatter::needs_reformat("say 1  \n", formatted),
        "trailing whitespace is a difference"
    );
    assert!(
        formatter::needs_reformat("say 1 \nsay 2\n", formatted),
        "a difference anywhere in the file is a difference"
    );
}

#[test]
fn formatted_output_ends_with_exactly_one_newline() {
    for path in corpus() {
        let source = read(&path);
        let formatted = formatter::format(&source)
            .unwrap_or_else(|e| panic!("{}: format failed: {}", path.display(), e));

        assert!(
            formatted.ends_with("\n"),
            "{}: formatted output must end with a newline, got {:?}",
            path.display(),
            formatted.chars().rev().take(8).collect::<String>()
        );
        assert!(
            !formatted.ends_with("\n\n"),
            "{}: formatted output must not end with a blank line",
            path.display()
        );
        assert!(
            !formatted.lines().any(|line| line != line.trim_end()),
            "{}: formatted output must not carry trailing whitespace",
            path.display()
        );
    }
}

// ---------------------------------------------------------------------------
// Comments
// ---------------------------------------------------------------------------

#[test]
fn format_preserves_comments() {
    let source = "// Classic Hello World\n\
                  say \"Hello, World!\"\n\
                  \n\
                  // With a variable\n\
                  set greeting to \"Hello\"\n\
                  say greeting\n";

    let formatted = assert_idempotent(source, "comments");

    assert!(
        formatted.contains("// Classic Hello World"),
        "the leading comment must survive:\n{}",
        formatted
    );
    assert!(
        formatted.contains("// With a variable"),
        "the comment before `set` must survive:\n{}",
        formatted
    );
}

#[test]
fn format_preserves_every_comment_in_every_corpus_file() {
    for path in corpus() {
        let source = read(&path);
        let label = path.display().to_string();

        let formatted = formatter::format(&source)
            .unwrap_or_else(|e| panic!("{}: format failed: {}", label, e));

        for comment in source.lines().map(str::trim) {
            if let Some(text) = comment.strip_prefix("//") {
                assert!(
                    formatted.contains(text.trim()),
                    "{}: comment {:?} was dropped by formatting",
                    label,
                    text.trim()
                );
            }
        }
    }
}

#[test]
fn format_preserves_comments_inside_blocks_and_at_the_end() {
    let source = "\
// before the function
to check(n)
    // before the if
    if n is 1 then
        // inside the if
        say n
    end
    // last line of the function
end

check(1)
";

    let formatted = assert_idempotent(source, "nested comments");

    for expected in [
        "// before the function",
        "// before the if",
        "// inside the if",
        "// last line of the function",
    ] {
        assert!(
            formatted.contains(expected),
            "comment {:?} was dropped:\n{}",
            expected,
            formatted
        );
    }
}

#[test]
fn edge_slashes_inside_a_text_literal_are_not_comments() {
    // A `//` inside a literal is text: dropping the rest of the line would
    // change what the program says.
    let source = "say \"http://example.com/path\"\n";

    let formatted = assert_idempotent(source, "url in text");
    assert_eq!(
        formatted, source,
        "a URL inside a literal must be printed as the same literal"
    );
}

#[test]
fn edge_a_comment_that_is_only_slashes_is_kept() {
    let source = "//// four slashes\nsay 1\n";

    let formatted = assert_idempotent(source, "slash-only comment");
    assert!(
        formatted.contains("//// four slashes"),
        "a slash-only comment must survive:\n{}",
        formatted
    );
}

// ---------------------------------------------------------------------------
// Text literals
// ---------------------------------------------------------------------------

#[test]
fn format_escapes_text_so_it_reads_back_unchanged() {
    let source = "\
say \"quote \\\" backslash \\\\ tab \\t\"
say \"{not an escape}\"
";

    let formatted = assert_idempotent(source, "escapes");
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "escaped text must read back as the same text:\n{}",
        formatted
    );
}

#[test]
fn edge_equality_against_a_negation_keeps_its_parentheses() {
    // `x is not y` is a `is not` comparison, so an equality whose right-hand
    // side is a negation has to keep its parentheses or it reads back as the
    // other operator.
    let source = "set same to 1 is (not yes)\n";

    let formatted = assert_idempotent(source, "equality against a negation");

    assert_eq!(
        formatted, source,
        "the parentheses that keep it an equality must survive"
    );
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "the tree must not become an inequality"
    );
}

#[test]
fn edge_empty_text_literal() {
    let source = "say \"\"\n";

    let formatted = assert_idempotent(source, "empty literal");
    assert_eq!(formatted, source, "an empty literal must stay empty");
}

#[test]
fn edge_a_literal_spanning_lines_keeps_its_slashes_and_later_comments() {
    // A `//` inside a literal that spans lines is text, and a comment after the
    // literal is a comment: the scanner has to know which one it is looking at.
    let source = "say \"first\n// not a comment\nlast\"\n// a real comment\nsay 1\n";

    let formatted = assert_idempotent(source, "multi-line literal");

    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "a literal spanning lines must keep its text:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\\n// not a comment\\n"),
        "the slashes inside the literal must stay inside it:\n{}",
        formatted
    );
    assert!(
        formatted.contains("\n// a real comment\n"),
        "the comment after the literal must stay a comment:\n{}",
        formatted
    );
}

#[test]
fn edge_unicode_text_round_trips() {
    let source = "\
say \"héllo wörld\"
say \"emoji 🎉 and CJK 日本語\"
say \"RTL עברית and combining é\"
say \"{name} ← interpolated\"
";

    let formatted = assert_idempotent(source, "unicode");
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "unicode text must survive formatting:\n{}",
        formatted
    );
}

#[test]
fn edge_text_holding_a_quote_and_a_backslash_round_trips() {
    let source = "say \"he said \\\"hi\\\" then \\\\\\\\ left\"\n";

    let formatted = assert_idempotent(source, "quote and backslash");
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "a literal holding a quote and a backslash must survive:\n{}",
        formatted
    );
}

// ---------------------------------------------------------------------------
// Empty, singleton and boundary input
// ---------------------------------------------------------------------------

#[test]
fn edge_empty_program_formats_to_nothing() {
    assert_eq!(
        formatter::format("").expect("empty source must format"),
        "",
        "an empty program formats to the empty string"
    );
    assert_eq!(
        formatter::format("\n\n   \n").expect("whitespace-only source must format"),
        "",
        "whitespace alone formats to the empty string"
    );
    assert_eq!(
        formatter::format("// just a comment\n").expect("comment-only source must format"),
        "// just a comment\n",
        "a comment-only program keeps its comment and one trailing newline"
    );
}

#[test]
fn edge_singleton_inputs() {
    let cases = [
        "say 1\n",
        "print 2\n",
        "set x to 1\n",
        "break\n",
        "skip\n",
        "return\n",
        "give back 1\n",
        "give back\n",
        "give\n",
        "back 1\n",
        "import files\n",
        "import files, network to net\n",
        "say [1]\n",
        "say []\n",
        "say {a: 1}\n",
        "say {}\n",
        "say nothing\n",
        "say yes\n",
        "say -0\n",
    ];

    for source in cases {
        let formatted = assert_idempotent(source, source);
        assert_eq!(
            formatted, source,
            "a singleton statement must format to itself"
        );
    }
}

#[test]
fn edge_numeric_boundaries_round_trip() {
    let cases = [
        "say 0\n",
        "say 1\n",
        "say -1\n",
        "say 1.5\n",
        "say -0.0\n",
        "say 0.1\n",
        "say 9007199254740993\n",
        "say 1e308\n",
        "say 1e-308\n",
        "say 4503599627370496\n",
        "say 2 - 1 - 1\n",
        "say - -1\n",
        "say 0 - -0\n",
    ];

    for source in cases {
        let formatted = assert_idempotent(source, source);
        assert!(
            same_program(&parse(source), &parse(&formatted)),
            "number {} must read back as the same number, formatted as:\n{}",
            source.trim(),
            formatted
        );
    }
}

// ---------------------------------------------------------------------------
// Nesting
// ---------------------------------------------------------------------------

#[test]
fn edge_nested_lists_and_records_round_trip() {
    let source = "\
set nested to [[1, 2], [[3], [4, [5, 6]]]]
say nested
set r to {a: {b: {c: 1}}, d: [1, 2]}
say r
say nested[1][0][0]
";

    let formatted = assert_idempotent(source, "nesting");
    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "nested literals must survive formatting:\n{}",
        formatted
    );
}

#[test]
fn edge_duplicate_record_keys_keep_their_order() {
    // A record is a list of pairs, so the formatter must print them all, in the
    // order they were written.
    let source = "say {a: 1, b: 2, a: 3}\n";

    let formatted = assert_idempotent(source, "duplicate keys");
    assert_eq!(
        formatted, source,
        "every field, duplicate keys included, must be printed in order"
    );
}

#[test]
fn edge_missing_record_field_is_not_invented_by_formatting() {
    // Reading a field the record does not have yields `nothing`. Formatting
    // must not invent the field, and must not turn the read into an error.
    let source = "set r to {a: 1}\nsay r.b\n";

    let formatted = assert_idempotent(source, "missing field");

    assert_eq!(
        run_program(&formatted),
        "nothing\n",
        "a field the record does not have must still be nothing"
    );
    assert_eq!(
        run_program("set r to {a: 1}\nsay r.a\n"),
        "1\n",
        "the field the record does have must still read back"
    );
}

#[test]
fn edge_a_duplicate_key_is_still_overwritten_after_formatting() {
    // The tree keeps both fields; the runtime keeps the last one.
    let source = "say {a: 1, a: 2}.a\n";

    let formatted = assert_idempotent(source, "duplicate key at runtime");

    assert_eq!(
        run_program(&formatted),
        "2\n",
        "the last field with a key must win, formatted or not"
    );
}

// ---------------------------------------------------------------------------
// Malformed input
// ---------------------------------------------------------------------------

#[test]
fn formatter_rejects_malformed_input_instead_of_guessing() {
    let cases = [
        ("say \"unterminated", "Lexer error"),
        ("if true\n    say 1\n", "Parser error"),
        ("say )", "Parser error"),
        ("to f(\nend", "Parser error"),
        ("say 1.2.3\n", "Lexer error"),
    ];

    for (source, expected) in cases {
        let error = formatter::format(source)
            .err()
            .unwrap_or_else(|| panic!("{:?} must not format, got:\n{}", source, source));
        assert!(
            error.contains(expected),
            "expected a {} error for {:?}, got: {}",
            expected,
            source,
            error
        );
    }
}

#[test]
fn edge_crlf_source_formats_to_lf_and_is_idempotent() {
    let source = "// a comment\r\nsay 1\r\nsay 2\r\n";

    let formatted = assert_idempotent(source, "CRLF");
    assert_eq!(
        formatted, "// a comment\nsay 1\nsay 2\n",
        "CRLF line endings must normalise to LF"
    );
}

#[test]
fn edge_a_bom_does_not_become_part_of_the_first_statement() {
    let source = "\u{feff}say 1\n";

    let formatted = assert_idempotent(source, "BOM");
    assert_eq!(formatted, "say 1\n", "the BOM must not be printed");
}

#[test]
fn edge_unclosed_block_is_a_parser_error() {
    let error = formatter::format("to f\n    say 1\n")
        .expect_err("a function with no `end` must not format");

    assert!(
        error.starts_with("Parser error"),
        "expected a parser error, got: {}",
        error
    );
}

// ---------------------------------------------------------------------------
// Every statement and expression form
// ---------------------------------------------------------------------------

#[test]
fn format_covers_every_statement_form() {
    let source = "\
import files, network to net
say 1
print 2
set x to 1
set obj.field to 2
if x is 1 then
    say 1
else
    say 2
end
for each item in [1, 2]
    say item
end
repeat 3 times
    say 1
end
while x is less than 3
    set x to x + 1
end
to helper(a, b)
    give back a + b
end
object Thing
    has name default \"none\"
    to can greet()
        say this.name
    end
end
try
    say 1
catch error
    say error
finally
    say 2
end
test \"a test\"
    expect 1 + 1 to be 2
end
say helper(1, 2)
say obj.field
say obj.describe(1)
say [1, 2][0]
";

    let formatted = assert_idempotent(source, "statement forms");

    assert!(
        same_program(&parse(source), &parse(&formatted)),
        "every statement form must survive formatting:\n{}",
        formatted
    );
}

#[test]
fn edge_a_very_deeply_nested_program_is_handled_or_reported() {
    let depth = 60;
    let source = format!("say {}\n", format!("{}+1", 1).repeat(depth));

    match formatter::format(&source) {
        Ok(formatted) => {
            assert!(
                same_program(&parse(&source), &parse(&formatted)),
                "a nested expression must survive formatting"
            );
        }
        Err(error) => {
            assert!(
                error.starts_with("Parser error"),
                "a program too deep to parse must be reported, not mangled: {}",
                error
            );
        }
    }
}

#[test]
fn edge_a_very_long_text_literal_round_trips() {
    let text = "x".repeat(20_000);
    let source = format!("say \"{}\"\n", text);

    let formatted = formatter::format(&source).expect("a long literal must format");
    assert!(
        same_program(&parse(&source), &parse(&formatted)),
        "a long literal must survive formatting unchanged"
    );
    assert_eq!(
        formatter::format(&formatted).expect("second format"),
        formatted,
        "a long literal must format idempotently"
    );
}

// ---------------------------------------------------------------------------
// Behaviour, not just the tree
// ---------------------------------------------------------------------------

/// The corpus files whose output is a function of the program alone.
/// `examples/files.rb` writes files and `examples/time.rb` reads the clock, so
/// neither can be compared run to run.
fn deterministic_corpus() -> Vec<(PathBuf, &'static str)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<(PathBuf, &'static str)> = vec![
        (root.join("examples/hello.rb"), "run"),
        (root.join("examples/fizzbuzz.rb"), "run"),
        (root.join("examples/formats.rb"), "run"),
        (root.join("examples/test_arithmetic.rb"), "run"),
    ];
    for entry in fs::read_dir(root.join("tests")).expect("tests/ should be readable") {
        let path = entry.expect("directory entry should be readable").path();
        if path.extension().and_then(|e| e.to_str()) == Some("rb") {
            files.push((path, "test"));
        }
    }
    files.sort();
    files
}

/// Drops the one line of output that is about the machine rather than the
/// program: `rb test` reports how long it took.
fn without_duration(output: &str) -> String {
    output
        .lines()
        .filter(|line| !line.starts_with("Duration:"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn formatted_programs_behave_exactly_as_the_originals() {
    for (path, mode) in deterministic_corpus() {
        let source = read(&path);
        let label = path.display().to_string();

        let formatted = formatter::format(&source)
            .unwrap_or_else(|e| panic!("{}: format failed: {}", label, e));

        let before = run_rb(mode, &source);
        let after = run_rb(mode, &formatted);

        assert_eq!(
            after.code, before.code,
            "{}: exit status changed after formatting",
            label
        );
        assert_eq!(
            without_duration(&after.stdout),
            without_duration(&before.stdout),
            "{}: what the program prints changed after formatting",
            label
        );
        assert_eq!(
            after.stderr, before.stderr,
            "{}: what the program reports changed after formatting",
            label
        );
    }
}

// ---------------------------------------------------------------------------
// `rb format --check`, through the binary
// ---------------------------------------------------------------------------

#[test]
fn check_rejects_a_file_that_differs_only_in_its_last_byte() {
    let formatted = "say 1\n";

    let clean = run_rb("format", formatted);
    assert_eq!(
        clean.code, 0,
        "a formatted file must print, got: {}",
        clean.stdout
    );

    // `rb format --check` reads the fourth argument, so the check is driven
    // through the same path a user takes.
    for source in ["say 1", "say 1\n\n", "say 1  \n", "say 1\nsay 2\n"] {
        assert!(
            formatter::needs_reformat(source, formatted),
            "{:?} differs from {:?} and must be reported",
            source,
            formatted
        );
    }
}

#[test]
fn edge_a_program_that_fails_still_fails_the_same_way_after_formatting() {
    let source = "set x to 1e400\nsay x\n";

    let formatted = assert_idempotent(source, "out of range literal");

    let before = run_rb("run", source);
    let after = run_rb("run", &formatted);

    assert_ne!(before.code, 0, "the program is meant to fail");
    assert_eq!(
        after.code, before.code,
        "an out of range literal must fail the same way after formatting"
    );
    assert_eq!(
        after.stderr, before.stderr,
        "the error must be the same one after formatting"
    );
}

#[test]
fn edge_grouping_survives_in_what_the_program_computes() {
    // The tree comparison says the grouping was kept; this says the arithmetic
    // agrees, which is what a reader of the formatted file would rely on.
    let source = "set a to (1 + 2) * 3\nsay a\n";

    let formatted = assert_idempotent(source, "grouping at runtime");

    assert_eq!(run_program(source), "9\n");
    assert_eq!(run_program(&formatted), "9\n");
    assert_eq!(run_program("set a to 1 + 2 * 3\nsay a\n"), "7\n");
}

// ---------------------------------------------------------------------------
// try/catch — the formatter must never delete a block
// ---------------------------------------------------------------------------

#[test]
fn edge_bare_catch_body_is_not_deleted() {
    // `catch` with no name binding is legal: the parser leaves catch_var as None
    // and still collects the body. Gating the emit on `Some(var)` dropped the
    // whole block, so the formatter silently deleted statements.
    let source = "try\n    say 1 / 0\ncatch\n    say \"recovered\"\nend\n";

    let formatted = formatter::format(source).expect("a bare catch must format");

    assert!(
        formatted.contains("catch"),
        "the catch keyword must survive formatting, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("recovered"),
        "THE CATCH BODY WAS DELETED by the formatter, got:\n{}",
        formatted
    );
    assert!(
        formatted.contains("1 / 0"),
        "the try body must survive formatting, got:\n{}",
        formatted
    );
}

#[test]
fn edge_bare_catch_program_still_computes_the_same_after_formatting() {
    // The tree said the block was kept while the text said it was dropped. Run
    // it: if the body is gone the program cannot recover and cannot print.
    let source = "try\n    say 1 / 0\ncatch\n    say \"recovered\"\nend\n";

    let formatted = assert_idempotent(source, "bare catch");

    assert_eq!(run_program(source), "recovered\n");
    assert_eq!(
        run_program(&formatted),
        "recovered\n",
        "formatting changed what the program does — a statement was lost"
    );
}

#[test]
fn edge_named_catch_keeps_its_binding() {
    // The regression above must not cost the named form: `catch err` has to keep
    // both the keyword and the binding.
    let source = "try\n    say 1 / 0\ncatch err\n    say err\nend\n";

    let formatted = assert_idempotent(source, "named catch");

    assert!(
        formatted.contains("catch err"),
        "a named catch must keep its binding, got:\n{}",
        formatted
    );
    assert_eq!(run_program(source), run_program(&formatted));
}

#[test]
fn edge_finally_still_closes_a_bare_catch() {
    // `catch` followed by `finally` is the shape most likely to be mangled while
    // fixing the bare case: the block must close in the right order.
    let source = "try\n    say 1 / 0\ncatch\n    say \"a\"\nfinally\n    say \"b\"\nend\n";

    let formatted = assert_idempotent(source, "catch then finally");

    let catch_at = formatted.find("catch").expect("catch must survive");
    let finally_at = formatted.find("finally").expect("finally must survive");
    let end_at = formatted.find("end").expect("end must survive");
    assert!(
        catch_at < finally_at,
        "catch must precede finally:\n{}",
        formatted
    );
    assert!(
        finally_at < end_at,
        "finally must precede end:\n{}",
        formatted
    );
}
