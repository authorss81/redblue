//! The phase's own delivery documents must be readable by *every* markdown
//! reader, because a heading that one reader cannot see is a section that
//! reader cannot find.
//!
//! # Why this file exists
//!
//! Round 4 of phase-084 failed its preflight with one line:
//!
//! ```text
//!   -   FAIL report missing section: Tests added
//! ```
//!
//! `phases/phase-084/REPORT.md` **did** carry a `## Tests added` heading, with
//! the table AGENTS.md §4 asks for. It was invisible to the reader that
//! reported the failure, and the reason was the file's **code fences**: a
//! ` ```diff ` fence sat *inside* an open ` ``` ` fence.
//!
//! CommonMark reads that as literal text — a fence opened with three
//! backticks is closed by a run of backticks with **no info string**, so
//! ` ```diff ` inside it is content, not a boundary. A reader that treats every
//! fence-looking line as a boundary reads it as a fence instead, the pairing
//! shifts by one, and the rest of the file is *inside a code block* — which is
//! where `## Tests added`, `## Gates`, `## Invariants touched` and
//! `## Known gaps / follow-ups` went.
//!
//! The gate's own scanner found the heading in that same file (`PASS section:
//! Tests added`, in the paste under "Gates" of that report), so the gate and
//! the preflight did not read the file the same way. A document that only one
//! of them can read fails the other.
//!
//! # What these tests pin
//!
//! Three properties, all of which the shipped documents now hold:
//!
//! 1. **No ambiguous fence.** A fence long enough to close the block around
//!    it, carrying an info string, inside an open fence — the shape the two
//!    readings disagree about.
//! 2. **No unterminated block.** A document that ends inside a code block
//!    hides whatever follows the last fence.
//! 3. **The sections are headings to every reader.** A heading inside a code
//!    block is text, not a heading, and the preflight's `report missing
//!    section: Tests added` was exactly that.
//!
//! And, because a section can also go missing by being renamed: the report
//! still carries the four section headings AGENTS.md §4 names, and the two
//! readings of its fences agree — so `## Tests added` is a heading to both.
//!
//! A closing fence shorter than the one it closes is text to every reader, so
//! the two readings differ only in the info string. That is what lets a
//! review request or a diff be pasted into this report verbatim, inside a
//! four-backtick fence, without eating the sections that follow it.

use std::fs;
use std::path::PathBuf;

/// A fenced region: the line it opens on, and the line it closes on.
type Region = (usize, usize);

/// The documents this phase delivers, by name.
const DELIVERABLES: [&str; 2] = ["REPORT.md", "FINDINGS.md"];

/// The sections AGENTS.md §4 requires of a phase report.
///
/// `Known gaps` is matched as a prefix because the contract's own heading is
/// `Known gaps / follow-ups`.
const REQUIRED_SECTIONS: [&str; 4] = [
    "## What changed",
    "## Tests added",
    "## Gates",
    "## Known gaps",
];

/// The repository root, so the documents are read from the checkout and not
/// from the working directory the test happens to run in.
fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// The phase's own directory.
fn phase_dir() -> PathBuf {
    project_root().join("phases").join("phase-084")
}

/// Read one of the phase's delivery documents.
fn read_document(name: &str) -> String {
    let path = phase_dir().join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{} must be readable: {e}", path.display()))
}

/// The backtick run and the info string of a fence-looking line, if it is one.
///
/// A fence-looking line is a line whose first non-space characters are three
/// or more backticks. The info string is whatever follows the run.
fn fence_line(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_start();
    let run = trimmed.chars().take_while(|c| *c == '`').count();
    if run < 3 {
        return None;
    }
    Some((run, trimmed[run..].trim()))
}

/// The fenced regions of a document, and the line an unterminated fence
/// opens on.
struct Fences {
    /// The closed regions: the line each opens on, and the line it closes on.
    regions: Vec<Region>,
    /// The line of a fence that is never closed, if there is one.
    unterminated: Option<usize>,
}

/// Read the fences of `lines`.
///
/// With `commonmark` the closing fence must carry no info string and must be
/// at least as long as the opening one; without it, every fence-looking line
/// that is at least as long as the opening one is a boundary. The info string
/// is the whole difference between the two readings.
///
/// The length rule is shared on purpose. A closing fence shorter than the
/// opening one is text to every markdown reader, so a document that quotes a
/// three-backtick fence inside a four-backtick fence reads the same way to
/// both — which is what lets a review request or a diff be pasted verbatim.
fn scan(lines: &[&str], commonmark: bool) -> Fences {
    let mut regions: Vec<Region> = Vec::new();
    let mut open: Option<(usize, usize)> = None;
    for (index, line) in lines.iter().enumerate() {
        let Some((run, info)) = fence_line(line) else {
            continue;
        };
        match open {
            None => open = Some((index + 1, run)),
            Some((start, ticks)) => {
                let closes = if commonmark {
                    info.is_empty() && run >= ticks
                } else {
                    run >= ticks
                };
                if closes {
                    regions.push((start, index + 1));
                    open = None;
                }
            }
        }
    }
    Fences {
        regions,
        unterminated: open.map(|(start, _)| start),
    }
}

/// The first fence-looking line inside an open fence that the two readings
/// disagree about, as `(line number, info string)`.
///
/// This is the ambiguous shape: a fence long enough to close the block around
/// it, carrying an info string. To CommonMark it is content; to the simpler
/// reading it is a boundary.
fn ambiguous_fence(lines: &[&str]) -> Option<(usize, String)> {
    let mut open: Option<(usize, usize)> = None;
    for (index, line) in lines.iter().enumerate() {
        let Some((run, info)) = fence_line(line) else {
            continue;
        };
        match open {
            None => open = Some((index + 1, run)),
            Some((_, ticks)) => {
                if run < ticks {
                    continue;
                }
                if !info.is_empty() {
                    return Some((index + 1, info.to_string()));
                }
                open = None;
            }
        }
    }
    None
}

/// The fenced regions of a document, under either reading of its fences.
///
/// A region is the line a fence opens on and the line it closes on, both
/// 1-based and inclusive. The two readings are unioned: a heading inside a
/// region is inside a code block to somebody, and a section only one reader
/// can find is a section the other reader cannot find.
fn fenced_regions(text: &str) -> Vec<Region> {
    let lines: Vec<&str> = text.lines().collect();
    let mut regions = scan(&lines, true).regions;
    for region in scan(&lines, false).regions {
        if !regions.contains(&region) {
            regions.push(region);
        }
    }
    regions.sort_unstable();
    regions
}

/// Whether the 1-based `line` falls inside one of `regions`.
fn inside_a_fence(regions: &[Region], line: usize) -> bool {
    regions
        .iter()
        .any(|(start, end)| line >= *start && line <= *end)
}

/// The required sections a document does NOT show a reader.
///
/// A heading inside a code block is text, not a heading. The scanner that
/// reported `report missing section: Tests added` could not see the heading
/// that was there, and a test that asks only "does some line start with the
/// heading" passes that document — so the heading has to be outside every
/// code block, not merely present.
fn missing_sections(text: &str) -> Vec<String> {
    let regions = fenced_regions(text);
    let mut missing = Vec::new();
    for section in REQUIRED_SECTIONS {
        let found = text
            .lines()
            .enumerate()
            .any(|(index, line)| line.starts_with(section) && !inside_a_fence(&regions, index + 1));
        if !found {
            missing.push(section.to_string());
        }
    }
    missing
}

/// Check that `text` reads the same way under both markdown readers.
///
/// Returns the reason it does not, so the failure says which document shape
/// broke and where.
fn check_document(text: &str) -> Result<(), String> {
    let lines: Vec<&str> = text.lines().collect();

    if let Some((line, info)) = ambiguous_fence(&lines) {
        return Err(format!(
            "line {line} is a fence with the info string {info:?} inside an open code \
             block: CommonMark reads it as text, a simpler reader reads it as a fence, \
             and the two disagree about where the code blocks end"
        ));
    }

    let strict = scan(&lines, true);
    let naive = scan(&lines, false);
    if strict.regions != naive.regions {
        return Err(format!(
            "the two readings of the code fences disagree: CommonMark finds {:?}, \
             the simpler reading finds {:?}",
            strict.regions, naive.regions
        ));
    }

    if let Some(line) = strict.unterminated {
        return Err(format!(
            "the document ends inside a code block: the fence that opens on line {line} \
             is never closed"
        ));
    }

    Ok(())
}

/// The phase's two delivery documents read the same way under both readers.
#[test]
fn phase_084_delivery_documents_are_unambiguous_markdown() {
    for name in DELIVERABLES {
        let text = read_document(name);
        check_document(&text).unwrap_or_else(|e| {
            panic!("{name} must read the same way under every markdown reader: {e}")
        });
    }
}

/// The report still carries the sections the gate and the preflight look for.
///
/// This is the finding itself, pinned: a report can carry a heading and still
/// fail a scanner that cannot see it, but it can also lose a section by being
/// renamed, and that is a plain mistake this test catches.
///
/// The heading has to be a heading to every reader, so it has to be OUTSIDE
/// every code block. The preflight failed with `report missing section: Tests
/// added` on a report that carried the heading — inside a fence, where the
/// scanner could not see it. A test that only asked "does some line start
/// with the heading" would have passed that document.
#[test]
fn phase_084_report_carries_the_sections_the_gate_looks_for() {
    let text = read_document("REPORT.md");
    let missing = missing_sections(&text);
    assert!(
        missing.is_empty(),
        "REPORT.md must carry these sections as headings a scanner can see: {missing:?}"
    );
}

/// A heading inside a code block is not a section.
///
/// The exact shape that started this: the heading is present, and the reader
/// that scans for it cannot see it. This drives the rule the section test
/// uses, so the section test cannot pass on a document that hides a heading.
#[test]
fn edge_a_section_heading_inside_a_code_block_is_not_a_section() {
    let document = "\
## What changed

```
## Tests added
```

## Gates
";
    let missing = missing_sections(document);
    assert!(
        missing.contains(&"## Tests added".to_string()),
        "a heading inside a code block must not count as a section, missing: {missing:?}"
    );
    assert!(
        !missing.contains(&"## Gates".to_string()),
        "a heading outside every code block must count, missing: {missing:?}"
    );
}

/// A fence with an info string inside an open fence is the ambiguous shape.
///
/// It is refused with the line it is on, so the fix has an address.
#[test]
fn edge_a_fence_with_an_info_string_inside_an_open_fence_is_rejected() {
    let document = "\
```
# a heading inside a block
```diff
```
";
    let error = check_document(document)
        .expect_err("a fence with an info string inside an open block is ambiguous");
    assert!(
        error.contains("line 3"),
        "the error must name the line the ambiguity is on, got: {error}"
    );
    assert!(
        error.contains("disagree"),
        "the error must say what the ambiguity does, got: {error}"
    );
}

/// A document that ends inside a code block is refused.
///
/// The headings after the last fence would be inside that block.
#[test]
fn edge_a_document_that_ends_inside_a_code_block_is_rejected() {
    let document = "\
# Phase 084 — a title

```
a block that is never closed
";
    let error =
        check_document(document).expect_err("a document that ends inside a block is refused");
    assert!(
        error.contains("ends inside a code block"),
        "the error must say what is wrong, got: {error}"
    );
}

/// A balanced document is accepted.
///
/// The happy path, so the two rejections above are rejections and not the
/// checker refusing everything.
#[test]
fn edge_a_balanced_document_is_accepted() {
    let document = "\
# Phase 084 — a title

```
a block
```

## A heading outside the block

```
diff --git a/x b/x
```
";
    check_document(document).expect("a balanced document is accepted");
}

/// A longer outer fence may hold a shorter inner fence.
///
/// This is what lets a review request or a diff — which carries three-backtick
/// fences of its own — be quoted verbatim inside the report. Under the older
/// rule, in which every fence-looking line closed the block around it, the
/// inner fence ended the outer one and the rest of the document fell inside a
/// code block, which is how a quoted artifact used to cost the report a
/// section.
#[test]
fn edge_a_longer_outer_fence_may_hold_a_shorter_inner_fence() {
    let document = "\
````text
```
## Tests added
```
````

## Gates
";
    check_document(document).expect("a four-backtick fence may hold a three-backtick one");
}

/// A rejected document produces a clean error, not a panic.
///
/// The failure the check above found was a *silent* one — a heading that
/// vanished from a reader without anything erroring. The checker's own
/// contract is the opposite: a bad document is an `Err` with a line number,
/// and this pins that its rejections stay errors.
#[test]
fn edge_a_rejected_document_produces_an_error_not_a_panic() {
    let document = "\
```
# a heading inside a block
```diff
";
    let result = check_document(document);
    assert!(
        result.is_err(),
        "an ambiguous document must come back as an error, not a panic or an Ok"
    );
}

/// An empty document has no fences and no headings to hide, so it is accepted.
///
/// The boundary of the input space: the checker refuses *ambiguous* documents,
/// not *small* ones.
#[test]
fn edge_an_empty_document_is_accepted() {
    check_document("").expect("an empty document is vacuously unambiguous");
}

/// A document with no code blocks at all is accepted.
///
/// The heading-only shape: nothing fences, so nothing can be misread, and the
/// sections are headings to every reader.
#[test]
fn edge_a_document_without_code_blocks_is_accepted() {
    let document = "\
# Phase 084 — a title

## What changed

## Tests added

## Gates

## Known gaps
";
    check_document(document).expect("a document without code blocks is unambiguous");
}
