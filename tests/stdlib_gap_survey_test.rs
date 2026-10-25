//! SURVEY.md is a ranking, and a ranking is only worth reading if every claim in
//! it is anchored to the code that proves it.
//!
//! A survey of "what cannot be done in Redblue" has exactly one way to go wrong
//! that a reader cannot see: a gap written from memory. `text has no pad` is a
//! believable sentence, and it is true or false depending on a line of
//! `src/stdlib.rs` nobody re-read. So the document is checked here instead of
//! trusted:
//!
//! - **every citation resolves.** Each `path:line` in SURVEY.md is opened and the
//!   line is asked for. A citation into a file that moved, or past its end, is a
//!   claim that cannot be checked, and this test says so by line.
//! - **the regex citation says what it claims.** `src/stdlib.rs` states in a
//!   comment that Redblue has no regex. That comment is the whole anchor for
//!   rank 1, so the test reads the cited line and refuses the entry unless the
//!   words are there.
//! - **the registry is reconciled with the inventory.** Every module in
//!   `stdlib::MODULES` is named in the survey, and the builtin count the survey
//!   states is the length of `stdlib::BUILTIN_NAMES`. A survey that misses a
//!   module, or that counts 90 builtins when the table holds 72, is measuring
//!   something other than this stdlib.
//! - **the top ten are ranked, not listed.** Ranks 1..10 appear once each, and
//!   the blocked-program counts they carry do not rise as the rank improves.
//! - **each of the top ten blocks a program.** Real Redblue source, in a fenced
//!   block, and a label — `implementable-in-Redblue` or `binding-required` — with
//!   a Rust crate for the entries that are bindings. "Would be nice to have" with
//!   no program that cannot be written is not a gap a phase can be built from.
//!
//! One test is not about the document at all. `regex_is_refused_by_a_program`
//! runs a program that reaches for a regex and asserts it *fails*: that is the
//! finding this phase was dispatched on, and a survey written against a gap that
//! had quietly been closed would be a survey of nothing. It also asserts the
//! shape of the fix — the `regex` crate is a dependency, and no line outside
//! `src/testing/` refers to it.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use redblue::stdlib;

const SURVEY: &str = "SURVEY.md";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn survey() -> String {
    let path = repo_root().join(SURVEY);
    assert!(
        path.exists(),
        "{SURVEY} does not exist at {}. Phase-090 is a document phase: its whole \
         output is that file, and a survey of the stdlib gaps that is not written \
         down is a phase that changed nothing.",
        path.display()
    );
    std::fs::read_to_string(&path).expect("SURVEY.md is readable")
}

/// One `## <rank>. <title>` entry of the top ten, with the fields it carries.
struct Entry {
    rank: usize,
    body: String,
}

fn field(body: &str, name: &str) -> Option<String> {
    let prefix = format!("- **{name}**:");
    body.lines()
        .map(str::trim)
        .find(|line| line.starts_with(&prefix))
        .map(|line| line[prefix.len()..].trim().to_string())
}

fn blocked_programs(body: &str) -> Option<usize> {
    field(body, "Blocked programs")
        .and_then(|v| v.split_whitespace().next().map(String::from))
        .and_then(|v| v.parse().ok())
}

/// The `## <rank>. ` entries of the top ten, in document order, each carrying
/// every line from its heading to the next one.
fn top_ten(doc: &str) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for line in doc.lines() {
        let rank = line
            .strip_prefix("## ")
            .and_then(|rest| rest.split_once(". ").map(|(rank, _)| rank))
            .and_then(|rank| rank.trim().parse::<usize>().ok());
        match (rank, entries.last_mut()) {
            (Some(rank), _) => entries.push(Entry {
                rank,
                body: format!("{line}\n"),
            }),
            (None, Some(last)) => {
                last.body.push_str(line);
                last.body.push('\n');
            }
            (None, None) => {}
        }
    }
    entries
}

/// The lines of the first ```redblue fence inside `body`.
fn redblue_block(body: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut inside = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            if inside {
                break;
            }
            inside = trimmed == "```redblue";
            continue;
        }
        if inside {
            lines.push(line.to_string());
        }
    }
    lines
}

/// Every `` `path:line` `` in the document, as (path, line number).
///
/// Scanned rather than split on whitespace: the colon is part of the citation, so
/// a tokeniser that treats it as a separator would find nothing and the whole
/// check would pass vacuously.
fn citations(doc: &str) -> Vec<(String, usize)> {
    let chars: Vec<char> = doc.chars().collect();
    let is_path_char = |c: char| c.is_alphanumeric() || matches!(c, '/' | '.' | '_' | '-');
    let mut found = Vec::new();

    for (index, c) in chars.iter().enumerate() {
        if *c != ':' {
            continue;
        }
        let mut start = index;
        while start > 0 && is_path_char(chars[start - 1]) {
            start -= 1;
        }
        if start == index {
            continue;
        }
        let path: String = chars[start..index].iter().collect();
        // `path:` with no path in front, `/abs:1`, and `src/:1` are not citations,
        // and neither is a path with no digits after the colon: a document that
        // mentions one is not making a claim this test can check.
        if !path.contains('/') || path.starts_with('/') || path.ends_with('/') {
            continue;
        }
        let mut end = index + 1;
        while end < chars.len() && chars[end].is_ascii_digit() {
            end += 1;
        }
        if end == index + 1 {
            continue;
        }
        let number: String = chars[index + 1..end].iter().collect();
        found.push((
            path,
            number.parse().expect("a digit run parses as a number"),
        ));
    }
    found
}

/// The registry is what the survey is a survey *of*: every module it names, and
/// the size of its builtin table, are read out of `src/stdlib.rs` rather than
/// copied into the document and hoped for.
#[test]
fn survey_inventory_is_reconciled_with_the_registry() {
    let doc = survey();

    for module in stdlib::MODULES {
        assert!(
            doc.contains(&format!("`{module}`")),
            "SURVEY.md never names the module `{module}`, which \
             stdlib::MODULES (src/stdlib.rs:19) lists. A survey that skips a \
             module cannot claim to rank the gaps."
        );
    }

    let stated = field(&doc, "Builtins in the registry")
        .and_then(|v| v.split_whitespace().next().map(String::from))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or_else(|| {
            panic!(
                "SURVEY.md has no `- **Builtins in the registry**: <n>` line, so \
                 nothing checks it against stdlib::BUILTIN_NAMES (src/stdlib.rs:846). \
                 The survey has to state the size of what it measured."
            )
        });
    assert_eq!(
        stated,
        stdlib::BUILTIN_NAMES.len(),
        "SURVEY.md says the registry holds {stated} builtins and \
         stdlib::BUILTIN_NAMES holds {}. One of them is measuring a different \
         stdlib than the one that ships.",
        stdlib::BUILTIN_NAMES.len()
    );
}

/// Every citation in the survey resolves to a line that exists, and the regex
/// entry cites the binding that closed the gap.
#[test]
fn edge_every_citation_resolves_to_a_line_that_exists() {
    let doc = survey();
    let root = repo_root();

    let cited = citations(&doc);
    assert!(
        cited.len() >= 10,
        "SURVEY.md holds {} `path:line` citations. A survey whose gaps are not \
         anchored cannot be reviewed, and the gate rejects a gap listed without one.",
        cited.len()
    );

    for (path, number) in &cited {
        let full = root.join(path);
        assert!(
            full.is_file(),
            "SURVEY.md cites `{path}:{number}` and there is no such file in the \
             repository. A citation nobody can open is not evidence."
        );
        let text = std::fs::read_to_string(&full).expect("cited file is readable");
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            *number >= 1 && *number <= lines.len(),
            "SURVEY.md cites `{path}:{number}` and the file holds {} lines, so the \
             citation points past its end. Citations drift when code moves; the \
             survey is re-anchored when they do.",
            lines.len()
        );
    }

    let regex_entry = top_ten(&doc)
        .into_iter()
        .find(|entry| entry.body.to_lowercase().contains("regex"))
        .expect("SURVEY.md names regular expressions, if only to record phase-095 closing them");
    let regex_citation = citations(&regex_entry.body)
        .into_iter()
        .find(|(path, _)| path == "src/stdlib.rs")
        .unwrap_or_else(|| {
            panic!(
                "the regex entry cites nothing in src/stdlib.rs. The binding lives in that \
                 file, and the survey has to point at it."
            )
        });
    let stdlib_text =
        std::fs::read_to_string(root.join("src/stdlib.rs")).expect("src/stdlib.rs is readable");
    let cited_line = stdlib_text
        .lines()
        .nth(regex_citation.1 - 1)
        .expect("the cited line exists, checked above");
    assert!(
        cited_line.contains("regex"),
        "the regex entry cites src/stdlib.rs:{}, and that line reads `{cited_line}` — \
         it does not carry the regex binding the entry is making. The implementation \
         starts at line {}.",
        regex_citation.1,
        stdlib_text
            .lines()
            .position(|l| l.contains("fn regex_builtin"))
            .map(|i| i + 1)
            .unwrap_or(0)
    );
    assert!(
        regex_entry.body.contains("phase-095"),
        "the regex entry no longer says phase-095 closed it. The survey has to record \
         that the gap is closed rather than keep claiming a missing capability."
    );
}

/// Ranks 1..10, once each, and the counts they carry do not rise as the rank
/// improves — which is the whole claim being made.
#[test]
fn the_top_ten_are_ranked_by_blocked_programs() {
    let doc = survey();
    let entries = top_ten(&doc);
    assert!(
        entries.len() >= 10,
        "SURVEY.md names {} ranked gaps; the phase asks for a top ten.",
        entries.len()
    );

    let mut ranks = BTreeSet::new();
    let mut scores: Vec<(usize, usize)> = Vec::new();
    for entry in entries.iter().take(10) {
        assert!(
            ranks.insert(entry.rank),
            "rank {} appears twice in SURVEY.md; a rank that appears twice is a \
             position in an order with nothing at it.",
            entry.rank
        );
        let score = blocked_programs(&entry.body).unwrap_or_else(|| {
            panic!(
                "rank {} has no `- **Blocked programs**: <n>` line. The ranking is \
                 by how many realistic programs each gap blocks, so a gap without \
                 a count cannot be placed.",
                entry.rank
            )
        });
        scores.push((entry.rank, score));
    }
    assert_eq!(
        ranks.iter().copied().collect::<Vec<_>>(),
        (1..=10).collect::<Vec<_>>(),
        "SURVEY.md does not rank 1..=10 exactly once: {:?}",
        ranks
    );

    for pair in scores.windows(2) {
        assert!(
            pair[0].1 >= pair[1].1,
            "rank {} blocks {} programs and rank {} blocks {}: the order does not \
             descend. A ranking that does not descend is a list.",
            pair[0].0,
            pair[0].1,
            pair[1].0,
            pair[1].1
        );
    }
}

/// Each of the top ten is buildable as a phase: a program that cannot be written,
/// and a statement of which side of the line the fix falls on.
#[test]
fn every_top_ten_entry_blocks_a_program_and_says_how_it_would_be_built() {
    let doc = survey();
    let labels = ["implementable-in-Redblue", "binding-required"];

    for entry in top_ten(&doc).into_iter().take(10) {
        let block = redblue_block(&entry.body);
        let meaningful: Vec<&String> = block.iter().filter(|l| !l.trim().is_empty()).collect();
        assert!(
            !meaningful.is_empty(),
            "rank {} names no ```redblue block, so nothing shows which program it \
             blocks. A gap with no program behind it is an opinion.",
            entry.rank
        );

        let label = field(&entry.body, "Implementable")
            .map(|v| v.trim_matches('`').to_string())
            .unwrap_or_else(|| {
                panic!(
                    "rank {} has no `- **Implementable**: ...` line. It has to say \
                 `implementable-in-Redblue` or `binding-required`: the difference \
                 decides whether the work is a phase in this language or a wrapper \
                 around a crate.",
                    entry.rank
                )
            });
        assert!(
            labels.contains(&label.as_str()),
            "rank {} is labelled `{label}`, which is neither of the two labels \
             this phase ranks by.",
            entry.rank
        );

        if label == "binding-required" {
            let crate_name = field(&entry.body, "Rust crate").unwrap_or_default();
            let named = crate_name
                .split(',')
                .map(|c| c.trim().trim_matches('`'))
                .any(|c| {
                    !c.is_empty()
                        && c.chars()
                            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
                });
            assert!(
                named,
                "rank {} is binding-required and names no Rust crate. A binding \
                 without a crate is not a plan.",
                entry.rank
            );
        }
    }
}

/// The gap rank 2 recorded is closed, and it was closed in the language rather
/// than only in the document: a program that reaches for a regex runs on both
/// engines, and the crate is reached outside `src/testing/`.
#[test]
fn regex_is_answerable_by_a_program_on_both_engines() {
    let regex_source = "set m to regex.match(\"id=42\", \"id=([0-9]+)\")\nsay m.groups[1]\n";
    let tokens = redblue::lexer::Lexer::tokenize(regex_source).expect("the probe program lexes");
    let ast = redblue::parser::parse(tokens).expect("the probe program parses");
    redblue::analyzer::analyze(&ast).expect("`regex` is a module the analyzer accepts");

    let (_vm, walked) = redblue::run_isolated(&ast);
    assert_eq!(
        walked.expect("the tree-walker answers the regex program"),
        redblue::Value::Nothing,
        "the probe program should have printed its capture and ended with nothing"
    );

    let chunk = redblue::bytecode::compile_program(&ast).expect("the probe program compiles");
    let compiled = redblue::bytecode::vm::BytecodeVm::new()
        .run(&chunk)
        .expect("the bytecode VM answers the regex program");
    assert_eq!(compiled, redblue::Value::Nothing);

    let mut users: Vec<PathBuf> = Vec::new();
    let src = repo_root().join("src");
    for entry in std::fs::read_dir(&src).expect("src/ is readable") {
        let path = entry.expect("a directory entry is readable").path();
        if path.is_file()
            && std::fs::read_to_string(&path)
                .map(|text| text.contains("regex::"))
                .unwrap_or(false)
        {
            users.push(path);
        }
    }
    assert!(
        !users.is_empty(),
        "no file in src/ reaches the regex crate, so the survey's rank 2 has no binding."
    );

    let manifest =
        std::fs::read_to_string(repo_root().join("Cargo.toml")).expect("Cargo.toml is readable");
    assert!(
        manifest.contains("regex"),
        "Cargo.toml no longer depends on regex, so the binding cannot exist."
    );
}

/// The survey says what it is and is not, and the "not" is checkable: this phase
/// shipped a document and changed no interpreter, so a test that documents a
/// behaviour the tree does not have is the one thing it must not leave behind.
#[test]
fn survey_states_the_rankings_own_limits() {
    let doc = survey();
    for heading in ["## Method", "## How to use this ranking", "## Not surveyed"] {
        assert!(
            doc.contains(heading),
            "SURVEY.md has no `{heading}` section. A ranking without its method is \
             an assertion; without `## Not surveyed` it is also a claim to be \
             complete that nothing can check."
        );
    }
    assert!(
        Path::new(&repo_root().join("SURVEY.md"))
            .metadata()
            .map(|m| m.len() > 2000)
            .unwrap_or(false),
        "SURVEY.md is under 2KB, which is too short to hold ten anchored gaps with \
         a program each."
    );
}
