//! The corpus on disk: its loader, its golden format, its writer, its compare.
//!
//! ## The golden format
//!
//! A `.expected` file records everything about a run, in a fixed order:
//!
//! ```text
//! #label none            — the Error::label() it failed with, or `none`
//! #position none         — `line:column` where it failed, or `none`
//! #value number:5        — the type *and* the rendering of what it was worth
//! #message …             — present only when `#label` is not `none`
//! 5                      — one escaped line per line `say` printed
//! ```
//!
//! The three header lines are read **by position**, not by scanning for a `#`
//! prefix, so a program that prints a line beginning with `#label ` is recorded
//! and read back correctly. `#message` is present only for a failing program,
//! which is what makes a missing message a malformed file rather than an empty
//! one, and every line after the header is escaped, so a message that spans
//! three lines is three `\n`s on one line.
//!
//! `#value` names the **type** as well as the rendering. `number:5` and `text:5`
//! print the same and mean different things, and a corpus that recorded only the
//! rendering could not tell them apart.

use std::fs;
use std::path::{Path, PathBuf};

use super::vm::{Failure, Outcome, Typed};

/// The outcomes a `.expected` file may record: the five public `Error` labels,
/// or `none` for a program that ran to completion.
pub const LABELS: &[&str] = &[
    "none",
    "LexerError",
    "ParserError",
    "AnalyzerError",
    "RuntimeError",
    "IoError",
];

/// What a `.expected` file records.
#[derive(Debug, Clone, PartialEq)]
pub struct Expected {
    /// `none` for a program that ran to completion.
    pub label: String,
    /// Where it failed. Required for a failure, refused for a completion.
    pub position: Option<(usize, usize)>,
    /// What the program was worth.
    pub value: Typed,
    /// The failure's message. Empty exactly when `label` is `none`.
    pub message: String,
    /// The lines `say` produced, already unescaped.
    pub output: Vec<String>,
}

impl Expected {
    /// Records `outcome` — this is what the writer puts on disk.
    pub fn of(outcome: &Outcome) -> Expected {
        match &outcome.result {
            Ok(value) => Expected {
                label: "none".to_string(),
                position: None,
                value: value.clone(),
                message: String::new(),
                output: outcome.output.clone(),
            },
            Err(Failure::Label {
                label,
                message,
                position,
                ..
            }) => Expected {
                label: label.clone(),
                position: *position,
                value: Typed::Nothing,
                message: message.clone(),
                output: outcome.output.clone(),
            },
        }
    }

    /// Whether this program was supposed to fail.
    pub fn must_fail(&self) -> bool {
        self.label != "none"
    }
}

/// Escapes a line so it survives one line of a file.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// Reads back what [`escape`] wrote, refusing a trailing lone backslash rather
/// than inventing the character after it.
pub fn unescape(text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some(other) => return Err(format!("unknown escape \\{other}")),
            None => return Err("a line ends in a lone backslash".to_string()),
        }
    }
    Ok(out)
}

/// Renders an [`Expected`] as the bytes of a `.expected` file.
pub fn render(expected: &Expected) -> String {
    let mut out = format!("#label {}\n", expected.label);
    out.push_str("#position ");
    match expected.position {
        Some((line, column)) => out.push_str(&format!("{line}:{column}\n")),
        None => out.push_str("none\n"),
    }
    out.push_str(&format!("#value {}\n", expected.value.tagged()));
    if expected.must_fail() {
        out.push_str(&format!("#message {}\n", escape(&expected.message)));
    }
    for line in &expected.output {
        out.push_str(&escape(line));
        out.push('\n');
    }
    out
}

/// Reads a `.expected` file, refusing anything malformed.
///
/// Every refusal names what was wrong and where, because a corpus that fails to
/// load should say which file and which line rather than panic on an `unwrap`.
pub fn parse(text: &str) -> Result<Expected, String> {
    let mut lines = text.lines();

    let label = lines
        .next()
        .ok_or_else(|| "the file is empty: a program that ran records #label none".to_string())?
        .strip_prefix("#label ")
        .ok_or_else(|| "line 1 must read '#label <label>' or '#label none'".to_string())?
        .to_string();

    let position_line = lines
        .next()
        .ok_or_else(|| format!("{label}: line 2 must read '#position line:column'"))?;
    let position_text = position_line
        .strip_prefix("#position ")
        .ok_or_else(|| format!("{label}: line 2 must read '#position line:column'"))?;
    let position = if position_text == "none" {
        None
    } else {
        let (line, column) = position_text
            .split_once(':')
            .ok_or_else(|| format!("{label}: '#position {position_text}' has no ':'"))?;
        Some((
            line.parse::<usize>()
                .map_err(|_| format!("{label}: line number {line:?} is not a number"))?,
            column
                .parse::<usize>()
                .map_err(|_| format!("{label}: column {column:?} is not a number"))?,
        ))
    };

    let value_line = lines
        .next()
        .ok_or_else(|| format!("{label}: line 3 must read '#value <type>:<value>'"))?;
    let value_text = value_line
        .strip_prefix("#value ")
        .ok_or_else(|| format!("{label}: line 3 must read '#value <type>:<value>'"))?;
    let value = Typed::parse(value_text)
        .ok_or_else(|| format!("{label}: '#value {value_text}' names no known type"))?;

    // Read strictly by position: a `#message` line is the fourth line of a
    // failing program's file and output follows it. Nothing is scanned for, so
    // a printed line that begins with `#` cannot be mistaken for a header.
    let rest: Vec<&str> = lines.collect();
    let (message, rest) = if label == "none" {
        (String::new(), rest)
    } else {
        let (raw, tail) = rest.split_first().ok_or_else(|| {
            format!("{label}: a failing program must record '#message' naming the failure")
        })?;
        let message = raw
            .strip_prefix("#message ")
            .ok_or_else(|| format!("{label}: line 4 must read '#message <text>'"))?;
        (
            unescape(message).map_err(|e| format!("{label}: #message {e}"))?,
            tail.to_vec(),
        )
    };

    let mut output = Vec::with_capacity(rest.len());
    for line in &rest {
        output.push(unescape(line).map_err(|e| format!("{label}: {e}"))?);
    }

    // `#message ` is reserved: a failing program carries exactly one, on the
    // fourth line. A completing program has no fourth-line slot, so a program
    // whose *first printed line* begins with `#message ` cannot be recorded.
    // That is the format's one reservation, and it is stated here rather than
    // discovered by a program that happened to print that.
    if label == "none" {
        if let Some(first) = rest.first() {
            if first.starts_with("#message ") {
                return Err(format!(
                    "{label}: '#message ' is reserved for a failure, so a program \
                     whose first printed line begins with it cannot be recorded",
                ));
            }
        }
    }

    if !LABELS.contains(&label.as_str()) {
        return Err(format!("{label}: line 1 records an unknown outcome label"));
    }
    if label == "none" && !message.is_empty() {
        return Err(format!(
            "{label}: a program that ran to completion must not record a message",
        ));
    }
    if label != "none" && message.is_empty() {
        return Err(format!("{label}: '#message' is empty"));
    }
    if label == "none" && position.is_some() {
        return Err(format!(
            "{label}: a program that ran to completion cannot have failed somewhere",
        ));
    }
    if label != "none" && position.is_none() {
        return Err(format!(
            "{label}: a failing program must record where it failed"
        ));
    }

    Ok(Expected {
        label,
        position,
        value,
        message,
        output,
    })
}

/// Where the corpus lives: `corpus/`, at the top of the repository.
///
/// Not `tests/corpus/`. `redblue::testing::find_test_files("tests")` walks
/// `tests/` recursively and `tests/redblue_suite_test.rs` requires every `.rb`
/// file it finds to declare Redblue `test` blocks carrying an assertion. A
/// corpus program declares none, and "a corpus program is not a test" is the
/// right thing for that gate to say — so the corpus goes where the suite's
/// collector does not walk.
pub fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus")
}

/// Every corpus program, in a stable order.
pub fn corpus() -> Vec<PathBuf> {
    let dir = corpus_dir();
    let mut files: Vec<PathBuf> = match fs::read_dir(&dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("rb"))
            .collect(),
        Err(error) => {
            panic!(
                "corpus directory {} should be readable: {error}. The corpus is the \
                 generator's output: `RB_WRITE_CORPUS=1 cargo test --test \
                 differential_test` writes a refreshed copy under target/tmp/ and \
                 reports the difference rather than rewriting the goldens in place",
                dir.display()
            )
        }
    };
    files.sort();
    files
}

/// The corpus programs whose recorded outcome is a **defect both engines share**,
/// and the finding that says which defect.
///
/// A golden file records what an interpreter does, so these record what `break`
/// and `skip` do, which is nothing. They cannot be recorded as failures, because
/// they do not fail — the defect is that they *succeed* wrongly, and a `.expected`
/// file has nowhere to say so. So they are named here, and
/// `edge_a_break_and_a_skip_are_pinned_as_the_defect_they_are` asserts the no-op
/// on **both** engines: when either one implements `break`, that test fails and
/// these two goldens are expected to change, which is what a golden should do to a
/// deliberate fix rather than launder it.
pub const KNOWN_DEFECT_PROGRAMS: &[(&str, &str)] = &[
    (
        "loop-forms-0012",
        "FINDINGS.md §5: `break` does not leave the loop and does not skip the body",
    ),
    (
        "loop-forms-0013",
        "FINDINGS.md §5: `skip` does not skip the rest of the body",
    ),
];

/// The `.expected` file beside `path`, read.
///
/// A program with no expectation is a **hard failure** naming the file. An
/// orphan is skipped instead, and a skipped orphan is a corpus that silently
/// stopped checking itself.
pub fn expectation_of(path: &Path) -> Result<String, String> {
    let expected = path.with_extension("expected");
    fs::read_to_string(&expected).map_err(|e| {
        format!(
            "{} has no readable .expected file ({}): a corpus program with no \
             recorded outcome is not a check on anything",
            path.display(),
            e
        )
    })
}

/// The expectation beside `path`, parsed.
pub fn expectation(path: &Path) -> Result<Expected, String> {
    let text = expectation_of(path)?;
    parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Compares one run against what the corpus records, naming the program.
///
/// Returns a message rather than asserting, so the caller decides whether a
/// difference is one program's failure or a whole suite's.
pub fn compare(path: &Path, outcome: &Outcome, expected: &Expected) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("<unnamed>");

    if outcome.output != expected.output {
        return Err(format!(
            "{name}: printed {} line(s) but records {}: printed {:?}, records {:?}",
            outcome.output.len(),
            expected.output.len(),
            outcome.output,
            expected.output,
        ));
    }

    match (&outcome.result, expected.must_fail()) {
        (Ok(value), false) => {
            if *value != expected.value {
                return Err(format!(
                    "{name}: the program is worth {} but the corpus records {}",
                    expected.value.tagged(),
                    value.tagged(),
                ));
            }
            Ok(())
        }
        (Ok(value), true) => Err(format!(
            "{name}: recorded a {} but it ran to completion, worth {}",
            expected.label,
            value.tagged(),
        )),
        (
            Err(Failure::Label {
                label,
                message,
                position,
                ..
            }),
            true,
        ) => {
            if *label != expected.label {
                return Err(format!(
                    "{name}: recorded a {} but it failed with a {label}",
                    expected.label,
                ));
            }
            if *message != expected.message {
                return Err(format!(
                    "{name}: a {label} but its message differs: recorded {:?}, got {:?}",
                    expected.message, message,
                ));
            }
            if *position != expected.position {
                return Err(format!(
                    "{name}: a {label} but it happened somewhere else: recorded \
                     {expected_position:?}, got {position:?}",
                    expected_position = expected.position,
                ));
            }
            Ok(())
        }
        (Err(Failure::Label { label, .. }), false) => Err(format!(
            "{name}: recorded a completion but it failed with a {label}",
        )),
    }
}

/// Writes `sources` and their recorded outcomes into `dir`.
///
/// Every file is written fresh, so a run either reproduces the directory exactly
/// or leaves a difference for the caller to see; nothing is appended and
/// nothing stale is left behind.
///
/// `dir` is a **scratch** directory, never [`corpus_dir`]: the golden files are
/// what the tests check, and a check that rewrites its own expectation is not a
/// check. Stale files *are* removed, which is what makes the regenerated
/// directory comparable with the checked-in one.
pub fn write_into(dir: &Path, sources: &[(String, String)]) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    for stale in fs::read_dir(dir)? {
        let stale = stale?.path();
        if stale.extension().and_then(|e| e.to_str()) == Some("rb")
            || stale.extension().and_then(|e| e.to_str()) == Some("expected")
        {
            fs::remove_file(stale)?;
        }
    }
    for (name, source) in sources {
        fs::write(dir.join(format!("{name}.rb")), source)?;
        let outcome = super::vm::tree_walk(source);
        fs::write(
            dir.join(format!("{name}.expected")),
            render(&Expected::of(&outcome)),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::vm;

    #[test]
    fn edge_a_printed_line_that_looks_like_a_directive_round_trips() {
        // The header is read by position, so a program that prints `#value 1`
        // is recorded and read back, not mistaken for the third header line.
        let outcome = Outcome {
            output: vec!["#label none".to_string(), "#value 1".to_string()],
            result: Ok(Typed::Nothing),
        };
        let expected = Expected::of(&outcome);
        let text = render(&expected);
        assert_eq!(parse(&text), Ok(expected));
        assert_eq!(
            text.lines().count(),
            5,
            "three header lines and two printed lines: {text:?}",
        );
    }

    #[test]
    fn edge_a_multi_line_message_survives_one_line_of_a_file() {
        let outcome = Outcome {
            output: Vec::new(),
            result: Err(Failure::Label {
                label: "RuntimeError".to_string(),
                message: "first\n  --> 3:1\nsecond".to_string(),
                position: Some((3, 1)),
                column_exact: true,
            }),
        };
        let text = render(&Expected::of(&outcome));
        assert_eq!(
            text.lines().count(),
            4,
            "three headers plus one escaped message: {text:?}",
        );
        assert!(!text.contains("\nsecond"));
        assert_eq!(parse(&text), Ok(Expected::of(&outcome)));
    }

    #[test]
    fn edge_the_escape_is_a_two_way_function_over_every_case() {
        for text in [
            "",
            "plain",
            "with \\ backslash",
            "with \" quote",
            "tab\there",
            "cr\rlf\n",
            "#label none",
            "\\n is not a newline",
            "日本語 🎉 مرحبا",
        ] {
            let escaped = escape(text);
            assert!(
                !escaped.contains('\n') && !escaped.contains('\r'),
                "{text:?} escaped to something still carrying a newline: {escaped:?}",
            );
            assert_eq!(unescape(&escaped), Ok(text.to_string()), "for {text:?}");
        }
    }

    #[test]
    fn edge_an_escape_the_file_does_not_contain_is_refused() {
        assert!(
            unescape("trailing\\").is_err(),
            "a lone backslash is refused"
        );
        assert!(unescape("\\q").is_err(), "an unknown escape is refused");
    }

    #[test]
    fn edge_a_malformed_expected_file_is_rejected() {
        let good = "#label none\n#position none\n#value nothing\n";
        assert!(parse(good).is_ok(), "the control has to load");
        for bad in [
            "",
            "no header at all\n",
            "#label none\n",
            "#label none\n#position\n#value nothing\n",
            "#label none\n#position later\n#value nothing\n",
            "#label none\n#position none:1\n#value nothing\n",
            "#label none\n#position none\n#value 5\n",
            "#label none\n#position none\n#value\n",
            "#label none\n#position 1:1\n#value nothing\n",
            "#label RuntimeError\n#position none\n#value nothing\n#message x\n",
            "#label RuntimeError\n#position 2:1\n#value nothing\n",
            "#label RuntimeError\n#position 2:1\n#value nothing\n#message \n",
            "#label none\n#position none\n#value nothing\n#message surprise\n",
            "#label Nonsense\n#position 1:1\n#value nothing\n#message x\n",
            "#label none\n#position none\n#value list:[1, 2\n",
        ] {
            assert!(
                parse(bad).is_err(),
                "this must be refused, it is malformed: {bad:?}",
            );
        }
    }

    #[test]
    fn edge_a_well_formed_expected_file_is_accepted() {
        let cases = [
            "#label none\n#position none\n#value nothing\n",
            "#label none\n#position none\n#value number:-3\nprinted\n",
            "#label LexerError\n#position 1:5\n#value nothing\n#message Unterminated string\n",
        ];
        for good in cases {
            assert!(parse(good).is_ok(), "{good:?} must load");
        }
    }

    #[test]
    fn edge_a_frontend_failure_is_recorded_where_the_interpreter_reports_it() {
        let source = "say \"unterminated\n";
        let outcome = vm::tree_walk(source);
        let expected = Expected::of(&outcome);
        assert_eq!(expected.label, "LexerError");
        assert!(expected.must_fail());
        assert_eq!(
            expected.position.map(|(line, _)| line),
            Some(1),
            "a lexer failure is reported on the line it is on",
        );
        assert_eq!(
            parse(&render(&expected)),
            Ok(expected),
            "a recorded frontend failure has to read back",
        );
    }

    #[test]
    fn edge_a_changed_value_is_a_comparison_failure_naming_the_program() {
        let source = "say 1\n5\n";
        let outcome = vm::tree_walk(source);
        let path = Path::new("corpus/fixture-value.rb");
        let mut expected = Expected::of(&outcome);
        assert!(
            compare(path, &outcome, &expected).is_ok(),
            "the control must pass"
        );
        expected.value = Typed::Number(6.0);
        let wrong = compare(path, &outcome, &expected).expect_err("a changed value");
        assert!(wrong.contains("fixture-value.rb"), "{wrong}");
        assert!(wrong.contains("number:6"), "{wrong}");
    }

    #[test]
    fn edge_a_changed_message_or_position_is_a_comparison_failure() {
        let source = "set xs to [1]\nsay xs[4]\n";
        let outcome = vm::tree_walk(source);
        let path = Path::new("corpus/fixture-message.rb");
        let mut expected = Expected::of(&outcome);
        assert!(compare(path, &outcome, &expected).is_ok());
        expected.message.push_str(" (edited)");
        let wrong = compare(path, &outcome, &expected).expect_err("a changed message");
        assert!(wrong.contains("its message differs"), "{wrong}");

        let mut moved = Expected::of(&outcome);
        moved.position = Some((9, 1));
        let wrong = compare(path, &outcome, &moved).expect_err("a moved failure");
        assert!(wrong.contains("happened somewhere else"), "{wrong}");
    }

    #[test]
    fn edge_a_corpus_program_with_no_expectation_is_a_hard_failure() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/tmp/corpus-orphan");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir should be creatable");
        let orphan = dir.join("orphan.rb");
        fs::write(&orphan, "say 1\n").expect("scratch program should be writable");

        assert!(
            !orphan.with_extension("expected").exists(),
            "the fixture is only a fixture while its expectation is absent",
        );
        let reported = expectation_of(&orphan).expect_err("an orphan must be refused");
        assert!(reported.contains("orphan.rb"), "{reported}");
        assert!(
            reported.contains("no readable .expected file"),
            "the refusal must say the expectation is missing: {reported}",
        );
    }
}
