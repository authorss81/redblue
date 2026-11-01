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
}
