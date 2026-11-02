//! The text utilities formatting and diffing need: wrap, unified diff, dedent,
//! template substitution, padding and alignment.
//!
//! Every test runs through both engines and compares the answer as data, because
//! a builtin reached through `stdlib::builtin` is shared but a test that only
//! walked one engine could not see a dispatcher that answered on one and refused
//! on the other.

use redblue::bytecode::compile_source;
use redblue::bytecode::vm::BytecodeVm;
use redblue::{run_source_value, Value};

/// The tree-walking engine's value for `source`.
#[track_caller]
fn tree(source: &str) -> Value {
    run_source_value(source)
        .unwrap_or_else(|error| panic!("tree-walker should run:\n{source}\n{error:?}"))
}

/// The bytecode engine's value for `source`.
#[track_caller]
fn bytecode(source: &str) -> Value {
    let chunk = compile_source(source)
        .unwrap_or_else(|error| panic!("source should compile:\n{source}\n{error:?}"));
    let mut vm = BytecodeVm::new();
    vm.run(&chunk)
        .unwrap_or_else(|error| panic!("bytecode VM should run:\n{source}\n{error:?}"))
}

/// Runs `source` on both engines and asserts they answer the same value.
#[track_caller]
fn both(source: &str) -> Value {
    let walked = tree(source);
    let compiled = bytecode(source);
    assert_eq!(
        walked, compiled,
        "the two engines disagree about the value of:\n{source}"
    );
    walked
}

/// Runs `source` on both engines and asserts both refuse it with `needle`.
#[track_caller]
fn both_refuse(source: &str, needle: &str) {
    let walked = redblue::run_source_value(source)
        .expect_err("the tree-walker should have refused the program")
        .to_string();
    assert!(
        walked.contains(needle),
        "the tree-walker's refusal does not mention '{needle}':\n{source}\n{walked}"
    );

    let chunk = compile_source(source)
        .unwrap_or_else(|error| panic!("source should compile:\n{source}\n{error:?}"));
    let mut vm = BytecodeVm::new();
    let compiled = vm
        .run(&chunk)
        .expect_err("the bytecode VM should have refused the program")
        .to_string();
    assert!(
        compiled.contains(needle),
        "the bytecode VM's refusal does not mention '{needle}':\n{source}\n{compiled}"
    );
}

/// Calls `text.diff` directly, so a test can hand it a text too large to write
/// as a source literal without also asking the parser to read it.
#[track_caller]
fn diff_direct(old: &str, new: &str) -> Result<Value, redblue::Error> {
    redblue::stdlib::builtin(
        redblue::Span::new(1, 1),
        "text_diff",
        &[Value::Text(old.to_string()), Value::Text(new.to_string())],
    )
    .map(|value| value.expect("text_diff is a builtin"))
}

/// A Redblue text literal for `value`, with `\` and `"` escaped.
fn rb_text(value: &str) -> String {
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// One line of an old or new text, with whether it ended in a newline.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    text: String,
    has_newline: bool,
}

/// Splits `text` into lines the way a unified diff counts them.
fn lines_of(text: &str) -> Vec<Line> {
    text.split_inclusive('\n')
        .map(|chunk| match chunk.strip_suffix('\n') {
            Some(line) => Line {
                text: line.to_string(),
                has_newline: true,
            },
            None => Line {
                text: chunk.to_string(),
                has_newline: false,
            },
        })
        .collect()
}

/// Applies a standard unified diff to `old` and answers the new text.
///
/// This is deliberately an **independent reader** of the output: it knows only
/// the documented unified-diff shape (`---`, `+++`, `@@ -a,b +c,d @@`, ` `/`+`/`-`
/// lines and `\ No newline at end of file`), not any Redblue-specific detail.
/// If `text.diff` emitted a shape only this language understood, this reader
/// would fail on it.
#[track_caller]
fn apply_unified_diff(old: &str, diff: &str) -> String {
    if diff.is_empty() {
        return old.to_string();
    }

    let old_lines = lines_of(old);
    let mut lines = diff.lines().peekable();

    assert_eq!(
        lines.next(),
        Some("--- original"),
        "a unified diff starts with an `---` header:\n{diff}"
    );
    assert_eq!(
        lines.next(),
        Some("+++ changed"),
        "a unified diff names the new text on a `+++` line:\n{diff}"
    );

    let mut out: Vec<Line> = Vec::new();
    let mut old_index = 0usize;

    while let Some(line) = lines.next() {
        if line.starts_with("@@ ") {
            assert!(
                line.ends_with(" @@"),
                "a hunk header must end with `@@`:\n{line}"
            );
            let old_range = line
                .split_whitespace()
                .nth(1)
                .and_then(|range| range.strip_prefix('-'))
                .unwrap_or_else(|| panic!("a hunk header must carry an old range:\n{line}"));
            let (start, count) = match old_range.split_once(',') {
                Some((start, count)) => (
                    start.parse::<usize>().expect("a numeric hunk start"),
                    count.parse::<usize>().expect("a numeric hunk count"),
                ),
                None => (old_range.parse::<usize>().expect("a numeric hunk start"), 1),
            };
            let target = if count == 0 {
                start
            } else {
                start.saturating_sub(1)
            };
            assert!(
                target >= old_index,
                "a hunk starts before the lines already applied:\n{diff}"
            );
            out.extend(old_lines[old_index..target].iter().cloned());
            old_index = target;
            continue;
        }

        let (prefix, body) = line.split_at(1);
        match prefix {
            " " => {
                assert_eq!(
                    old_lines.get(old_index).map(|l| l.text.as_str()),
                    Some(body),
                    "a context line does not match the old text:\n{diff}"
                );
                let mut copied = old_lines[old_index].clone();
                old_index += 1;
                if lines.peek() == Some(&"\\ No newline at end of file") {
                    lines.next();
                    copied.has_newline = false;
                }
                out.push(copied);
            }
            "-" => {
                assert_eq!(
                    old_lines.get(old_index).map(|l| l.text.as_str()),
                    Some(body),
                    "a removed line does not match the old text:\n{diff}"
                );
                old_index += 1;
                if lines.peek() == Some(&"\\ No newline at end of file") {
                    lines.next();
                }
            }
            "+" => {
                let mut added = Line {
                    text: body.to_string(),
                    has_newline: true,
                };
                if lines.peek() == Some(&"\\ No newline at end of file") {
                    lines.next();
                    added.has_newline = false;
                }
                out.push(added);
            }
            other => panic!("unknown unified-diff line prefix {other:?} in:\n{diff}"),
        }
    }

    out.extend(old_lines[old_index..].iter().cloned());
    old_index = old_lines.len();

    assert_eq!(
        old_index,
        old_lines.len(),
        "the diff did not account for every old line:\n{diff}"
    );

    let mut result = String::new();
    for line in out {
        result.push_str(&line.text);
        if line.has_newline {
            result.push('\n');
        }
    }
    result
}

fn list_of(items: &[&str]) -> Value {
    Value::list(
        items
            .iter()
            .map(|item| Value::Text(item.to_string()))
            .collect(),
    )
}

#[test]
fn wrap_keeps_words_whole_and_only_breaks_a_word_longer_than_the_width() {
    let words = both(&format!("text.wrap({}, 7)", rb_text("hello world again")));
    assert_eq!(
        words,
        list_of(&["hello", "world", "again"]),
        "a word is moved to the next line whole rather than split"
    );

    let long = both(&format!("text.wrap({}, 4)", rb_text("abcdefghij")));
    assert_eq!(
        long,
        list_of(&["abcd", "efgh", "ij"]),
        "a word longer than the width alone is broken into width-sized pieces"
    );
}

#[test]
fn edge_wrap_handles_empty_single_no_trailing_newline_crlf_and_long_lines() {
    assert_eq!(
        both(&format!("text.wrap({}, 5)", rb_text(""))),
        list_of(&[]),
        "empty text wraps to no lines"
    );
    assert_eq!(
        both(&format!("text.wrap({}, 5)", rb_text("one"))),
        list_of(&["one"]),
        "a single line is one line"
    );
    assert_eq!(
        both(&format!("text.wrap({}, 5)", rb_text("one\ntwo"))),
        list_of(&["one", "two"]),
        "text with no trailing newline keeps both lines"
    );
    assert_eq!(
        both(&format!("text.wrap({}, 5)", rb_text("one\r\ntwo"))),
        list_of(&["one", "two"]),
        "CRLF is a line ending, not part of a word"
    );
    assert_eq!(
        both(&format!("text.wrap({}, 3)", rb_text("a b c d e f g"))),
        list_of(&["a b", "c d", "e f", "g"]),
        "every produced line stays within the width"
    );
}

#[test]
fn diff_is_empty_for_identical_text_and_pinpoints_one_changed_line() {
    assert_eq!(
        both(&format!(
            "text.diff({}, {})",
            rb_text("a\nb\nc\n"),
            rb_text("a\nb\nc\n")
        )),
        Value::Text(String::new()),
        "identical texts have an empty diff"
    );

    let old = "a\nb\nc\n";
    let new = "a\nB\nc\n";
    let diff = both(&format!("text.diff({}, {})", rb_text(old), rb_text(new)));
    let expected = "--- original\n+++ changed\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n";
    assert_eq!(
        diff,
        Value::Text(expected.to_string()),
        "the unified diff must name the changed line exactly"
    );

    let Value::Text(diff_text) = diff else {
        panic!("diff must answer text");
    };
    assert_eq!(
        apply_unified_diff(old, &diff_text),
        new,
        "an independent unified-diff reader must be able to apply the output"
    );
}

#[test]
fn edge_diff_handles_empty_and_no_trailing_newline_inputs() {
    let added = both(&format!("text.diff({}, {})", rb_text(""), rb_text("a\n")));
    assert_eq!(
        added,
        Value::Text("--- original\n+++ changed\n@@ -0,0 +1 @@\n+a\n".to_string()),
        "adding to an empty text uses the standard zero-length range"
    );

    let old = "a";
    let new = "a\nb\n";
    let Value::Text(diff_text) = both(&format!("text.diff({}, {})", rb_text(old), rb_text(new)))
    else {
        panic!("diff must answer text");
    };
    assert!(
        diff_text.contains("\\ No newline at end of file"),
        "a line without a final newline must carry the standard marker:\n{diff_text}"
    );
    assert_eq!(
        apply_unified_diff(old, &diff_text),
        new,
        "the no-newline marker must round-trip through a standard reader"
    );
}

#[test]
fn edge_diff_round_trips_separate_hunks_through_a_standard_reader() {
    let old = "0\n1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";
    let new = "0\nONE\n2\n3\n4\n5\n6\n7\n8\nNINE\n10\n";
    let Value::Text(diff_text) = both(&format!("text.diff({}, {})", rb_text(old), rb_text(new)))
    else {
        panic!("diff must answer text");
    };
    assert_eq!(
        diff_text.matches("@@ ").count(),
        2,
        "two changes separated by more than six lines are two hunks:\n{diff_text}"
    );
    assert_eq!(
        apply_unified_diff(old, &diff_text),
        new,
        "both hunks must apply through a standard reader"
    );
}

#[test]
fn dedent_removes_the_common_prefix_and_leaves_an_unaligned_line_alone() {
    assert_eq!(
        both(&format!(
            "text.dedent({})",
            rb_text("    alpha\n    beta\n")
        )),
        Value::Text("alpha\nbeta\n".to_string()),
        "the longest common leading whitespace is removed"
    );
    assert_eq!(
        both(&format!("text.dedent({})", rb_text("    alpha\nbeta\n"))),
        Value::Text("    alpha\nbeta\n".to_string()),
        "when one line has no common prefix, nothing is removed from any line"
    );
    assert_eq!(
        both(&format!("text.dedent({})", rb_text(""))),
        Value::Text(String::new()),
        "empty text dedents to empty text"
    );
}

#[test]
fn template_substitutes_a_value_escapes_a_delimiter_and_names_an_unknown_placeholder() {
    assert_eq!(
        both(&format!(
            "text.template({}, {{name: {}}})",
            rb_text("Hello << name >>!"),
            rb_text("Ada")
        )),
        Value::Text("Hello Ada!".to_string()),
        "a placeholder is replaced by the record field it names"
    );

    assert_eq!(
        both(&format!("text.template({}, {{}})", rb_text("<<<<name>>"))),
        Value::Text("<<name>>".to_string()),
        "doubling the opening delimiter writes it literally"
    );

    both_refuse(
        &format!(
            "text.template({}, {{name: {}}})",
            rb_text("<< nope >>"),
            rb_text("x")
        ),
        "nope",
    );
}

#[test]
fn pad_and_align_produce_exact_character_widths_for_non_ascii_text() {
    assert_eq!(
        both(&format!("text.pad({}, 3, {})", rb_text("é"), rb_text("-"))),
        Value::Text("é--".to_string()),
        "padding counts characters, not UTF-8 bytes"
    );
    assert_eq!(
        both(&format!(
            "text.align({}, 5, {})",
            rb_text("日本"),
            rb_text("right")
        )),
        Value::Text("   日本".to_string()),
        "right alignment counts three characters of padding, not six bytes"
    );
    assert_eq!(
        both(&format!(
            "text.align({}, 4, {})",
            rb_text("x"),
            rb_text("center")
        )),
        Value::Text(" x  ".to_string()),
        "an odd amount of centering space goes on the right"
    );
    assert_eq!(
        both(&format!("text.pad({}, 2, {})", rb_text("ab"), rb_text("."))),
        Value::Text("ab".to_string()),
        "text already at the width is left unchanged"
    );
}

#[test]
fn edge_a_bad_text_utility_argument_is_a_catchable_error_named_by_the_function() {
    both_refuse(&format!("text.wrap({}, 0)", rb_text("x")), "wrap");
    both_refuse("text.wrap(1, 5)", "wrap");
    both_refuse(
        &format!("text.template({}, {{}})", rb_text("<<name")),
        "template",
    );
    both_refuse(
        &format!("text.pad({}, 3, {})", rb_text("x"), rb_text("")),
        "pad",
    );
    both_refuse(
        &format!("text.align({}, 3, {})", rb_text("x"), rb_text("sideways")),
        "align",
    );
    both_refuse(
        &format!("text.align({}, 2, {})", rb_text("abc"), rb_text("sideways")),
        "align",
    );
}

#[test]
fn edge_text_utilities_refuse_widths_past_the_documented_limit_on_both_engines() {
    for source in [
        format!("text.wrap({}, 10000000000000000000)", rb_text("x")),
        format!(
            "text.pad({}, 10000000000000000000, {})",
            rb_text("x"),
            rb_text(".")
        ),
        format!(
            "text.align({}, 10000000000000000000, {})",
            rb_text("x"),
            rb_text("left")
        ),
    ] {
        let walked = redblue::run_source_value(&source);
        assert!(
            walked.is_err(),
            "the tree-walker must refuse a width past the limit: {source}"
        );

        let chunk = compile_source(&source).expect("the call shape should compile");
        let mut vm = BytecodeVm::new();
        let compiled = vm.run(&chunk);
        assert!(
            compiled.is_err(),
            "the bytecode VM must refuse a width past the limit: {source}"
        );
    }
}

#[test]
fn edge_text_diff_refuses_a_line_table_past_the_documented_limit_on_both_engines() {
    let old = "a\n".repeat(3000);
    let new = "b\n".repeat(3000);
    both_refuse(
        &format!("text.diff({}, {})", rb_text(&old), rb_text(&new)),
        "diff",
    );
}

#[test]
fn dedent_preserves_crlf_line_endings_while_removing_the_common_prefix() {
    assert_eq!(
        both(&format!(
            "text.dedent({})",
            rb_text("    alpha\r\n    beta\r\n")
        )),
        Value::Text("alpha\r\nbeta\r\n".to_string()),
        "dedent removes indentation without rewriting CRLF line endings"
    );
}

#[test]
fn edge_dedent_handles_blank_lines_tabs_and_no_final_newline() {
    assert_eq!(
        both(&format!(
            "text.dedent({})",
            rb_text("    alpha\n\n    beta")
        )),
        Value::Text("alpha\n\nbeta".to_string()),
        "a blank line does not vote and a missing final newline is preserved"
    );
    assert_eq!(
        both(&format!("text.dedent({})", rb_text("\t\tone\r\n\t\ttwo"))),
        Value::Text("one\r\ntwo".to_string()),
        "tabs are indentation and CRLF without a final newline is preserved"
    );
    assert_eq!(
        both(&format!("text.dedent({})", rb_text("  \r\n  \r\n"))),
        Value::Text("  \r\n  \r\n".to_string()),
        "text made only of blank lines is left unchanged"
    );
}

#[test]
fn edge_text_utilities_refuse_invalid_widths_and_sides_on_both_engines() {
    for source in [
        format!("text.wrap({}, 0)", rb_text("x")),
        format!("text.pad({}, 0, {})", rb_text("x"), rb_text(".")),
        format!("text.align({}, 2, {})", rb_text("x"), rb_text("middle")),
    ] {
        let walked = redblue::run_source_value(&source);
        assert!(
            walked.is_err(),
            "the tree-walker must refuse an invalid text utility argument: {source}"
        );

        let chunk = compile_source(&source).expect("the call shape should compile");
        let mut vm = BytecodeVm::new();
        let compiled = vm.run(&chunk);
        assert!(
            compiled.is_err(),
            "the bytecode VM must refuse an invalid text utility argument: {source}"
        );
    }
}

#[test]
fn edge_text_utilities_refuse_mismatched_argument_types_on_both_engines() {
    both_refuse(r#"text.diff("a", 1)"#, "diff");
    both_refuse("text.dedent(1)", "dedent");
    both_refuse(r#"text.template("<<name>>", 1)"#, "template");
    both_refuse(r#"text.pad("x", "five", "-")"#, "pad");
    both_refuse(r#"text.pad("x", 3, 1)"#, "pad");
    both_refuse(r#"text.align("x", "five", "left")"#, "align");
    both_refuse(r#"text.align("x", 3, 1)"#, "align");
}

#[test]
fn edge_wrap_breaks_a_long_word_after_a_partial_line_and_counts_characters() {
    assert_eq!(
        both(&format!(
            "text.wrap({}, 5)",
            rb_text("hi abcdefgh \u{4e16}\u{754c}")
        )),
        list_of(&["hi", "abcde", "fgh", "\u{4e16}\u{754c}"]),
        "a word longer than the width starts a fresh line and is split by character"
    );
    assert_eq!(
        both(&format!(
            "text.wrap({}, 2)",
            rb_text("\u{4e16}\u{754c}\u{4e16}\u{754c}\u{4e16}")
        )),
        list_of(&["\u{4e16}\u{754c}", "\u{4e16}\u{754c}", "\u{4e16}"]),
        "a long CJK word is split by characters, not by UTF-8 bytes"
    );
}

#[test]
fn edge_pad_and_align_handle_multicharacter_fill_and_exact_widths() {
    assert_eq!(
        both(&format!("text.pad({}, 6, {})", rb_text("x"), rb_text("ab"))),
        Value::Text("xababa".to_string()),
        "a multi-character fill is repeated and cut to the exact character width"
    );
    assert_eq!(
        both(&format!("text.pad({}, 5, {})", rb_text(""), rb_text("ab"))),
        Value::Text("ababa".to_string()),
        "padding empty text fills the whole width"
    );
    assert_eq!(
        both(&format!(
            "text.pad({}, 4, {})",
            rb_text("x"),
            rb_text("\u{e9}\u{65e5}")
        )),
        Value::Text("x\u{e9}\u{65e5}\u{e9}".to_string()),
        "a non-ASCII multi-character fill still counts characters"
    );
    assert_eq!(
        both(&format!(
            "text.align({}, 6, {})",
            rb_text("x"),
            rb_text("center")
        )),
        Value::Text("  x   ".to_string()),
        "center alignment puts the extra column on the right"
    );
}

#[test]
fn edge_pad_with_a_large_multicharacter_fill_reserves_only_the_output() {
    let fill = "ab".repeat(50_000);
    let width = 1_000_000;
    let expected: String = fill.chars().cycle().take(width).collect();
    assert_eq!(
        both(&format!(
            "text.pad({}, {}, {})",
            rb_text(""),
            width,
            rb_text(&fill)
        )),
        Value::Text(expected),
        "a long fill is cycled per output character, so the reservation is the width, not \
         the fill's byte length times the width"
    );
}

#[test]
fn edge_the_six_text_utilities_are_reached_only_through_the_text_module() {
    for (source, name) in [
        (r#"text_wrap("x", 3)"#, "text_wrap"),
        (r#"text_diff("a", "b")"#, "text_diff"),
        (r#"text_dedent(" x")"#, "text_dedent"),
        (r#"text_template("x", {})"#, "text_template"),
        (r#"text_pad("x", 3, "-")"#, "text_pad"),
        (r#"text_align("x", 3, "left")"#, "text_align"),
    ] {
        both_refuse(source, name);
    }

    assert_eq!(
        both("to wrap(prefix)\n    give back prefix + \"!\"\nend\nwrap(\"ok\")"),
        Value::Text("ok!".to_string()),
        "the bare `wrap` spelling remains an ordinary name a program may declare"
    );
}

#[test]
fn edge_template_repeats_a_placeholder_and_leaves_a_literal_close_alone() {
    assert_eq!(
        both(&format!(
            "text.template({}, {{a: {}}})",
            rb_text("<<a>> + <<a>>"),
            rb_text("x")
        )),
        Value::Text("x + x".to_string()),
        "the same placeholder may appear more than once"
    );
    assert_eq!(
        both(&format!("text.template({}, {{}})", rb_text(""))),
        Value::Text(String::new()),
        "an empty template and an empty record answer empty text"
    );
    assert_eq!(
        both(&format!("text.template({}, {{}})", rb_text("a >> b"))),
        Value::Text("a >> b".to_string()),
        "a closing delimiter outside a placeholder is literal text"
    );
}

#[test]
fn edge_wrap_collapses_runs_of_spaces_and_keeps_blank_lines() {
    assert_eq!(
        both(&format!("text.wrap({}, 3)", rb_text("a b  c d"))),
        list_of(&["a b", "c d"]),
        "runs of spaces between words do not count against the width"
    );
    assert_eq!(
        both(&format!("text.wrap({}, 5)", rb_text("a  b\n\nc"))),
        list_of(&["a b", "", "c"]),
        "a blank line in the middle of the text stays a blank line"
    );
}

#[test]
fn edge_text_diff_refuses_a_non_text_argument_on_both_engines() {
    let source = r#"text.diff("a", 1)"#;

    let walked = redblue::run_source_value(source);
    assert!(
        walked.is_err(),
        "the tree-walker must refuse a diff of text and a number: {source}"
    );

    let chunk = compile_source(source).expect("the call shape should compile");
    let mut vm = BytecodeVm::new();
    let compiled = vm.run(&chunk);
    assert!(
        compiled.is_err(),
        "the bytecode VM must refuse a diff of text and a number: {source}"
    );
}

#[test]
fn edge_text_diff_refuses_a_text_past_the_documented_line_limit() {
    let old = "a\n".repeat(1_000_001);
    let error = diff_direct(&old, "")
        .expect_err("a text past the line limit must be refused")
        .to_string();
    assert!(
        error.contains("diff"),
        "the refusal must name diff: {error}"
    );
    assert!(
        error.contains("1000000-line limit"),
        "the refusal must name the line limit it crossed: {error}"
    );
}

#[test]
fn edge_text_diff_reports_the_line_limit_before_the_cell_limit() {
    let text = "a\n".repeat(1_000_001);
    let error = diff_direct(&text, &text)
        .expect_err("two oversized texts must be refused")
        .to_string();
    assert!(
        error.contains("1000000-line limit"),
        "the line count is checked before the cell table: {error}"
    );
    assert!(
        !error.contains("cell limit"),
        "an oversized line count must not be reported as a cell-limit failure: {error}"
    );
}

#[test]
fn edge_text_diff_accepts_a_large_but_bounded_unbalanced_diff() {
    let old = "a\n".repeat(10_000);
    let source = format!("text.diff({}, {})", rb_text(&old), rb_text("a\n"));
    let Value::Text(diff) = both(&source) else {
        panic!("diff must answer text");
    };
    assert!(
        diff.starts_with("--- original\n+++ changed\n@@ -1,10000 +1 @@\n a\n"),
        "a large unbalanced diff still starts with a standard hunk: {diff}"
    );
    assert_eq!(
        diff.lines().filter(|line| *line == "-a").count(),
        9_999,
        "every line removed from the large text is a change line"
    );
}
