//! Every ```redblue block in the five documents that define Redblue is
//! extracted, classified and — where it is a program — executed.
//!
//! The finding this pins: the five documents carried **97** code blocks
//! (`SPEC.md` 64, `docs/GRAMMAR.md` 9, `PHILOSOPHY.md` 9, `HANDOUT.md` 8,
//! `README.md` 7) and **no test ran any of them**. Every existing reference to
//! documentation was a hand-quoted single example:
//! `tests/for_range_test.rs:2` cites `SPEC.md:380-391` and
//! `tests/formatter_test.rs:2008` cites `SPEC.md:383`. A block that nobody
//! executes rots in silence, which is exactly how `items at 0` sat in
//! SPEC.md as dead syntax until phase-080 found it.
//!
//! So the harness is the deliverable, not a number. Two data files in this
//! directory hold the classification, matched by `doc:line`:
//!
//! - `doc_blocks_runs.txt` — executed, and it exits 0.
//! - `doc_blocks_fragments.txt` — not executed, with the reason written down.
//! - `doc_blocks_known_fail.txt` — executed, exited non-zero, and the exact
//!   first line of stderr is recorded.
//!
//! **A RUNS block has to compile as well as run.** `rb run` reads the
//! tree-walking engine and nothing else, so before `compile_block` existed a
//! block could be listed here as working documentation while `rb compile`
//! refused it outright — and `SPEC.md:647`, the program the specification uses
//! to introduce function values, sat in that file for a whole phase with
//! `rb compile` stopping on it with "`to (x) ... end` as an expression is not
//! compiled yet". Every gate was green and the headline example still could
//! not be compiled. RUNS now means `rb run` exits 0 **and** `rb compile`
//! exits 0; the compiler lowers a function literal now, and
//! `tests/function_literal_test.rs` is where that is pinned.
//!
//! All three lists are checked in, and a block in none of them fails the suite, so
//! none of the lists can rot: adding a block to a document without classifying
//! it fails here, and so does deleting one — the extracted count is pinned, and
//! the entry would point at a block that is no longer there.
//!
//! Listing the *working* blocks is what makes the suite worth running. If a
//! block in `doc_blocks_runs.txt` stops exiting 0, the run test fails and it
//! has to be re-classified, rather than drifting silently into "not one of the
//! ones we run".
//!
//! **No network.** Two blocks make live HTTP requests to a host that does not
//! exist; they are classified `network` in the fragment file with that reason
//! written beside them. They are the only two, and a reader must not read the
//! fragment bucket as "runs".
//!
//! **Sandbox.** Each run gets its own empty working directory, so a block that
//! writes `output.txt` writes it there and not in the repository. The program
//! file itself is written *into the project root*, because
//! `redblue::root_of` resolves `import <name>` against the directory holding
//! the program — a block that imports `MathUtils` has to see `modules/`. The
//! scratch files are named `*.tmp`, which `.gitignore` already covers.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The five documents that define the language, and the only ones this suite
/// reads. The order is fixed so a failure message reads the same every run.
const DOCS: &[&str] = &[
    "SPEC.md",
    "README.md",
    "PHILOSOPHY.md",
    "HANDOUT.md",
    "docs/GRAMMAR.md",
];

/// The number of ```redblue blocks the audit measured across [`DOCS`].
///
/// This is a floor, not a measurement. The harness measures the real count and
/// reports it; this number exists so that *deleting* an example from a document
/// fails the suite. Without it, the cheapest way to make this test green would
/// be to delete the blocks it cannot run, which is the rot this file exists to
/// prevent.
const MINIMUM_BLOCKS: usize = 97;

/// One fenced `redblue` block, with the position a reader would jump to.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Block {
    doc: String,
    /// 1-based line of the opening fence.
    line: usize,
    body: String,
    /// False when the document ends before the closing fence.
    closed: bool,
}

impl Block {
    /// The `doc:line` key both data files are matched on.
    fn key(&self) -> String {
        format!("{}:{}", self.doc, self.line)
    }

    /// The program text handed to `rb`.
    fn source(&self) -> String {
        format!("{}\n", self.body)
    }
}

/// Extracts every ```redblue block from `text`, in document order.
///
/// A fence opens a block when the first whitespace-delimited word after the
/// backticks is `redblue`, so ```redblue and ```redblue title="…" both open one
/// and ```text, ```bash and a bare ``` do not. A fence inside the block closes
/// it whatever its info string is, because Markdown allows no other spelling and
/// an unterminated block must not swallow the rest of the document.
///
/// A block with no closing fence is still returned, with [`Block::closed`]
/// false: dropping it would let a document acquire an unterminated block and
/// have the count fall silently, which is the same failure this file exists to
/// catch, one level down.
fn extract_blocks(text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks = Vec::new();
    let mut index = 0;

    while index < lines.len() {
        let trimmed = lines[index].trim();
        if !is_redblue_fence(trimmed) {
            index += 1;
            continue;
        }
        let open = index;
        let mut cursor = index + 1;
        while cursor < lines.len() && !is_fence(lines[cursor].trim()) {
            cursor += 1;
        }
        let closed = cursor < lines.len();
        blocks.push(Block {
            doc: String::new(),
            line: open + 1,
            body: lines[open + 1..cursor].join("\n"),
            closed,
        });
        index = if closed { cursor + 1 } else { cursor };
    }

    blocks
}

/// Whether a trimmed line opens a `redblue` block.
fn is_redblue_fence(trimmed: &str) -> bool {
    fence_info(trimmed) == Some("redblue")
}

/// Whether a trimmed line is a fence at all, opening or closing.
fn is_fence(trimmed: &str) -> bool {
    trimmed.starts_with("```")
}

/// The info string of a fence: everything after the backticks, or `None` when
/// the line is not a fence at all. A closing fence carries no info string,
/// which is why the first word is what [`is_redblue_fence`] compares.
fn fence_info(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("```")?;
    let rest = rest.trim();
    if rest.is_empty() {
        return None;
    }
    Some(rest.split_whitespace().next().unwrap_or(rest))
}

/// Reads `doc` under `root`, or `Err` naming the file when it is not there.
///
/// `Result`, not a panic and not an empty vector: a document that has been
/// renamed or deleted must be a failure a reader can act on, and a suite that
/// reports "0 blocks found" for a missing document is indistinguishable from a
/// suite that ran clean.
fn read_doc(root: &Path, doc: &str) -> Result<String, String> {
    fs::read_to_string(root.join(doc)).map_err(|error| format!("{doc}: {error}"))
}

/// Every block in every document in [`DOCS`], keyed by `doc:line`.
fn all_blocks(root: &Path) -> BTreeMap<String, Block> {
    let mut found = BTreeMap::new();
    for doc in DOCS {
        let text = read_doc(root, doc).unwrap_or_else(|error| panic!("{error}"));
        for mut block in extract_blocks(&text) {
            block.doc = (*doc).to_string();
            let key = block.key();
            assert!(
                found.insert(key.clone(), block).is_none(),
                "{key} was extracted twice; two blocks share one opening line"
            );
        }
    }
    found
}

/// One row of a data file: a `doc:line` key and the text written after it.
#[derive(Clone, Debug)]
struct Row {
    key: String,
    value: String,
    file: &'static str,
    line: usize,
}

/// Parses a data file. `#` comments and blank lines are ignored; every other
/// line must carry a key, and a key may be followed by a tab and a value. The
/// RUNS file has no second column, so a value is optional here and its absence
/// is checked where it is required — a fragment with no reason, a known
/// failure with no recorded message.
fn read_rows(path: &Path, file: &'static str) -> Vec<Row> {
    let text =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let trimmed = line.trim_end();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = match trimmed.split_once('\t') {
            Some((key, value)) => (key, value),
            None => (trimmed, ""),
        };
        let key = key.trim();
        assert!(
            !key.is_empty(),
            "{file}:{number} has an empty key: `{trimmed}`"
        );
        rows.push(Row {
            key: key.to_string(),
            value: value.trim().to_string(),
            file,
            line: number,
        });
    }
    rows
}

/// The root of the checkout: the directory holding `Cargo.toml`.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// An empty working directory for `tag`, unique to the test that asked.
///
/// Each run gets its own: `cargo test` runs tests in parallel, and one test's
/// block writing `output.txt` would otherwise be another's.
fn scratch(tag: &str) -> PathBuf {
    let dir = root().join("target/tmp/docblocks").join(tag);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap_or_else(|error| panic!("{}: {error}", dir.display()));
    dir
}

/// What `rb compile` produced for one block.
#[derive(Clone, Debug)]
struct Compiled {
    compiled: bool,
    first_error_line: String,
}

/// Compiles `block` with `rb compile`, the same way [`run_block`] runs it, and
/// reports whether a `.rbc` was written at all.
///
/// `rb run` only ever reads the tree-walking engine, so a block in the RUNS
/// bucket could be listed as working documentation while `rb compile` refused
/// it outright — and that is not a hypothetical: `SPEC.md:647`, the program the
/// specification uses to introduce function values, was in this file for a
/// whole phase while `rb compile` stopped on it with `` `to (x) ... end` as an
/// expression is not compiled yet ``, a limitation no other check in the suite
/// could see. The classification said "runs", so every gate was green and the
/// specification's headline example still could not be compiled.
///
/// So a RUNS block has to be *both* runnable and compilable, and this is the
/// half that was missing. It checks that the compile succeeds and nothing more:
/// whether the compiled program then *answers* the same thing is what
/// `tests/differential_test.rs` is for, and a block can legitimately differ
/// there for reasons recorded in `phases/` rather than in a bucket here.
fn compile_block(tag: &str, key: &str, block: &Block) -> Compiled {
    let file = root().join(format!("docblocks-{tag}.tmp"));
    fs::write(&file, block.source()).unwrap_or_else(|error| panic!("{}: {error}", file.display()));
    let dir = scratch(tag);

    let output = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("compile")
        .arg(&file)
        .arg("-o")
        .arg(dir.join(format!("{tag}.rbc")))
        .current_dir(&dir)
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap_or_else(|error| panic!("rb compile {key}: {error}"));

    let _ = fs::remove_file(&file);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Compiled {
        compiled: output.status.success(),
        first_error_line: stderr
            .lines()
            .find(|line| line.trim_start().starts_with("Error"))
            .unwrap_or("")
            .trim()
            .to_string(),
    }
}

/// What running one block produced.
#[derive(Clone, Debug)]
struct Outcome {
    exit_ok: bool,
    first_error_line: String,
}

/// Writes `block` to a program file in the project root and runs it with `rb`,
/// from the scratch directory.
///
/// The program goes in the project root because `redblue::root_of` resolves
/// `import MathUtils` against the directory holding the program, and
/// `SPEC.md:780` imports it. The working directory is the scratch directory so
/// the side effects of a block land there. Stdin is closed, not inherited, so
/// `SPEC.md:1080`'s `input(...)` reads end-of-file instead of waiting on a
/// terminal nobody is sitting at.
fn run_block(tag: &str, key: &str, block: &Block) -> Outcome {
    let file = root().join(format!("docblocks-{tag}.tmp"));
    fs::write(&file, block.source()).unwrap_or_else(|error| panic!("{}: {error}", file.display()));

    let output = Command::new(env!("CARGO_BIN_EXE_rb"))
        .arg("run")
        .arg(&file)
        .current_dir(scratch(tag))
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap_or_else(|error| panic!("rb run {key}: {error}"));

    let _ = fs::remove_file(&file);
    let stderr = String::from_utf8_lossy(&output.stderr);
    Outcome {
        exit_ok: output.status.success(),
        first_error_line: stderr
            .lines()
            .find(|line| line.trim_start().starts_with("Error"))
            .unwrap_or("")
            .trim()
            .to_string(),
    }
}

/// What a checked KNOWN-FAIL row is: the block exited non-zero and printed
/// exactly the recorded first line. A unit struct rather than a bare `()` so a
/// caller can assert *that* it checked, instead of calling and hoping.
#[derive(Debug, PartialEq, Eq)]
struct Recorded;

/// Checks a KNOWN-FAIL row against the stderr the block produced today.
///
/// A free function so the mutation test can hand it a deliberately wrong
/// recording and assert the check has teeth, rather than only ever being shown
/// an entry that happens to be right.
fn check_known_fail(key: &str, recorded: &str, outcome: &Outcome) -> Recorded {
    assert!(
        !outcome.exit_ok,
        "{key} is recorded as KNOWN-FAIL but it ran to completion.\n\
         Either the documentation promise was fixed, or the entry belongs in \
         the fragment file with a reason; it does not belong in this file any \
         more."
    );
    assert_eq!(
        outcome.first_error_line, recorded,
        "{key} is recorded as failing with a different first line of stderr.\n\
         recorded: {recorded}\n\
         printed:  {}\n\
         The failure moved, changed its wording, or stopped happening. \
         Re-read the block and re-record what it does today.",
        outcome.first_error_line
    );
    Recorded
}

/// The suite as a whole: the blocks, the two tables, and the report the phase
/// is measured by. One struct so the four tests below read the same fixture
/// and cannot disagree about it.
struct Audit {
    blocks: BTreeMap<String, Block>,
    runs: Vec<Row>,
    fragments: Vec<Row>,
    known_fail: Vec<Row>,
}

impl Audit {
    fn load() -> Audit {
        let dir = root().join("tests");
        Audit {
            blocks: all_blocks(&root()),
            runs: read_rows(&dir.join("doc_blocks_runs.txt"), "doc_blocks_runs.txt"),
            fragments: read_rows(
                &dir.join("doc_blocks_fragments.txt"),
                "doc_blocks_fragments.txt",
            ),
            known_fail: read_rows(
                &dir.join("doc_blocks_known_fail.txt"),
                "doc_blocks_known_fail.txt",
            ),
        }
    }

    /// The keys in `rows`, rejecting a duplicate. Two rows for one block is a
    /// block in two buckets, which is the one classification error the test
    /// cannot otherwise see: the union still contains it.
    fn keys(&self, rows: &[Row]) -> Vec<String> {
        let mut seen = BTreeMap::new();
        for row in rows {
            assert!(
                seen.insert(row.key.clone(), row.line).is_none(),
                "{}:{} lists {} twice: it is in two buckets at once",
                row.file,
                row.line,
                row.key
            );
        }
        seen.into_keys().collect()
    }

    /// The rows of the two files that carry a value, which are the two whose
    /// value the suite checks.
    fn valued_rows(&self) -> Vec<&Row> {
        self.fragments
            .iter()
            .chain(self.known_fail.iter())
            .collect()
    }

    /// The bucket of one block, by key. `None` when the block is in none of
    /// the three files — which the classification test treats as a failure, so
    /// the run below can treat it as a bucket name rather than an error.
    fn bucket_of<'a>(&'a self, key: &str) -> Option<(&'static str, Option<&'a Row>)> {
        for (name, rows) in [
            ("RUNS", &self.runs),
            ("FRAGMENT", &self.fragments),
            ("KNOWN-FAIL", &self.known_fail),
        ] {
            if let Some(row) = rows.iter().find(|row| row.key == key) {
                return Some((name, Some(row)));
            }
        }
        None
    }
}

/// A human-readable list, one entry per line, for a failure message.
fn list(keys: &[String]) -> String {
    keys.iter()
        .map(|key| format!("  {key}"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------

/// The measured count is at least the audited floor, every block is in exactly
/// one bucket, and no real document has an unterminated fence.
#[test]
fn every_documentation_block_is_classified_exactly_once() {
    let audit = Audit::load();
    let measured = audit.blocks.len();

    assert!(
        measured >= MINIMUM_BLOCKS,
        "the five documents now hold {measured} ```redblue blocks, below the \
         {MINIMUM_BLOCKS} this suite was written against.\n\
         A block was deleted, or its fence was re-tagged, and the suite no \
         longer covers what it used to. If the deletion was deliberate, say so \
         here; do not lower the number to make this pass."
    );

    for block in audit.blocks.values() {
        assert!(
            block.closed,
            "{}:{} opens a ```redblue fence that is never closed.\n\
             The extractor stopped reading at end of input, so every block \
             after this one in that document is not covered.",
            block.doc, block.line
        );
    }

    let runs = audit.keys(&audit.runs);
    let fragments = audit.keys(&audit.fragments);
    let known_fail = audit.keys(&audit.known_fail);
    let all: Vec<String> = audit.blocks.keys().cloned().collect();

    // A block in two buckets is one classification error the union cannot see,
    // so it is looked for across all three lists rather than pairwise.
    let in_two: Vec<String> = all
        .iter()
        .filter(|key| {
            [
                (&runs, "RUNS"),
                (&fragments, "FRAGMENT"),
                (&known_fail, "KNOWN-FAIL"),
            ]
            .iter()
            .filter(|(keys, _)| keys.contains(key))
            .count()
                > 1
        })
        .cloned()
        .collect();
    assert!(
        in_two.is_empty(),
        "these blocks are in more than one bucket:\n{}\n\
         A block is either executed or it is not; the lists have to say which.",
        list(&in_two)
    );

    let dangling: Vec<String> = runs
        .iter()
        .chain(fragments.iter())
        .chain(known_fail.iter())
        .filter(|key| !audit.blocks.contains_key(*key))
        .cloned()
        .collect();
    assert!(
        dangling.is_empty(),
        "these entries point at a block that is not in any document:\n{}\n\
         A document was edited and a block moved, or one was deleted. The entry \
         has to move with the block it describes.",
        list(&dangling)
    );

    let unclassified: Vec<String> = all
        .iter()
        .filter(|key| audit.bucket_of(key).is_none())
        .cloned()
        .collect();
    assert!(
        unclassified.is_empty(),
        "these ```redblue blocks are in no bucket at all:\n{}\n\
         A new example was added to a document, or an existing one stopped \
         running. Classify it: RUNS if it exits 0, KNOWN-FAIL with the first \
         line of stderr it prints, or FRAGMENT with the reason it is not a \
         program.",
        list(&unclassified)
    );

    for row in audit.valued_rows() {
        assert!(
            !row.value.is_empty(),
            "{}:{} gives {} no value: a fragment with no reason and a known \
             failure with no recorded stderr are the two entries this suite \
             cannot check",
            row.file,
            row.line,
            row.key
        );
    }
}

/// The bucket a block is in: run it, and assert it did what the bucket says.
#[test]
fn running_every_block_agrees_with_the_bucket_it_is_in() {
    let audit = Audit::load();
    let mut ran = 0usize;
    let mut known = 0usize;
    let mut notes = String::new();

    for (key, block) in &audit.blocks {
        // The classification test rejects a block in no bucket; this one is
        // about what a bucket means, so a missing entry is reported as such.
        let (bucket, row) = audit
            .bucket_of(key)
            .unwrap_or_else(|| panic!("{key} is in no bucket; classify it"));

        match bucket {
            "RUNS" => {
                let outcome = run_block("agrees", key, block);
                assert!(
                    outcome.exit_ok,
                    "{key} is classified RUNS and did not run.\n\
                     first error line: {}\n\
                     body:\n{}",
                    outcome.first_error_line, block.body
                );
                let compiled = compile_block("agrees-compile", key, block);
                assert!(
                    compiled.compiled,
                    "{key} is classified RUNS and `rb compile` refused it.\n\
                     first error line: {}\n\
                     body:\n{}\n\
                     A RUNS block is a program `rb` runs *and* compiles. Either \
                     the compiler cannot lower this program yet — in which case \
                     that is the gap to fix and not this list to edit — or it \
                     belongs in the fragment file with the reason it is not a \
                     program.",
                    compiled.first_error_line, block.body
                );
                ran += 1;
            }
            // FRAGMENT: the suite does not execute it. The reason is checked
            // for existence in the classification test; executing a fragment
            // is exactly what the bucket says does not happen.
            "FRAGMENT" => {}
            "KNOWN-FAIL" => {
                let recorded = row.expect("a KNOWN-FAIL row").value.clone();
                let outcome = run_block("agrees", key, block);
                assert_eq!(
                    check_known_fail(key, &recorded, &outcome),
                    Recorded,
                    "{key}: a row whose recording is right must be accepted, \
                     or the mutation test below proves nothing"
                );
                known += 1;
                let _ = writeln!(notes, "  {key}  {}", outcome.first_error_line);
            }
            other => panic!("{key} is in an unknown bucket, {other}"),
        }
    }

    assert!(
        ran > 0 && known > 0,
        "the audit ran {ran} blocks and checked {known} known failures; one of \
         those numbers is zero, so half the suite asserts nothing.\n{notes}"
    );
}

/// The KNOWN-FAIL check rejects a recording that is not what the block prints.
///
/// This is the failure channel: `check_known_fail` is handed every recorded
/// entry with its message deliberately changed, and each one must be refused.
/// Without this, `check_known_fail` is only ever shown entries that happen to
/// be right, and a check that compares two equal strings cannot be shown to
/// compare two different ones.
#[test]
fn edge_a_known_fail_recording_that_is_wrong_is_rejected() {
    let audit = Audit::load();
    assert!(
        !audit.known_fail.is_empty(),
        "no KNOWN-FAIL entry exists, so there is no recorded failure to check"
    );

    for row in &audit.known_fail {
        let block = audit
            .blocks
            .get(&row.key)
            .unwrap_or_else(|| panic!("{} lists {}, which is not a block", row.file, row.key));
        let outcome = run_block("wrong-recording", &row.key, block);
        assert!(
            !outcome.exit_ok,
            "{} records {} as failing and it ran to completion",
            row.file, row.key
        );

        // The recording as written must pass, or nothing below means anything.
        assert_eq!(
            check_known_fail(&row.key, &row.value, &outcome),
            Recorded,
            "{}:{} does not record what {} does today",
            row.file,
            row.line,
            row.key
        );

        // Four wrong recordings, each a different way of being wrong.
        let mutated = [
            ("Error: A different failure entirely", "a different failure"),
            ("", "an empty recording"),
            (
                &format!("{} ", outcome.first_error_line),
                "a trailing space",
            ),
            (
                "Error: this is not what Redblue prints at all",
                "a plausible invented message",
            ),
        ];
        for (recording, what) in mutated {
            if recording == outcome.first_error_line {
                continue;
            }
            let verdict =
                std::panic::catch_unwind(|| check_known_fail(&row.key, recording, &outcome));
            assert!(
                verdict.is_err(),
                "{}: recording {} as {what} was accepted; the recorded first \
                 line of stderr is not being compared",
                row.key,
                recording
            );
        }

        // And a recording of success must be refused too, since the block did
        // not exit 0.
        let claimed_ok = Outcome {
            exit_ok: true,
            first_error_line: row.value.clone(),
        };
        let verdict =
            std::panic::catch_unwind(|| check_known_fail(&row.key, &row.value, &claimed_ok));
        assert!(
            verdict.is_err(),
            "{}: a block that failed was accepted as having exited 0",
            row.key
        );
    }
}

/// An unterminated block is extracted, with `closed` false, rather than
/// swallowing the rest of the document or disappearing.
#[test]
fn edge_a_block_with_no_closing_fence_is_still_extracted() {
    // Two blocks, both closed: the extractor moves past a closing fence and
    // finds the next one, rather than treating the first close as an opening.
    let text = concat!(
        "```redblue\n",
        "say \"first\"\n",
        "```\n",
        "\n",
        "some prose between the blocks\n",
        "\n",
        "```redblue\n",
        "say \"second\"\n",
        "```\n",
    );
    let blocks = extract_blocks(text);
    assert_eq!(blocks.len(), 2, "both fences open a block: {blocks:?}");
    assert_eq!(blocks[0].line, 1);
    assert_eq!(blocks[0].body, "say \"first\"");
    assert_eq!(blocks[1].line, 7, "the second block opens on line 7");
    assert_eq!(blocks[1].body, "say \"second\"");
    assert!(blocks.iter().all(|block| block.closed));

    // The unterminated case: the document ends before the closing fence. The
    // block is returned with `closed` false rather than dropped — dropping it
    // would let a document acquire an unterminated block and have the count
    // fall silently, which is the failure this file exists to catch.
    let cut = "before\n```redblue\nsay \"never closed\"\n";
    let blocks = extract_blocks(cut);
    assert_eq!(
        blocks.len(),
        1,
        "the unterminated block is returned, not dropped: {blocks:?}"
    );
    assert_eq!(blocks[0].line, 2);
    assert!(!blocks[0].closed, "and it is marked unterminated");
    assert_eq!(blocks[0].body, "say \"never closed\"");
    assert_eq!(blocks[0].source(), "say \"never closed\"\n");

    // An unterminated *first* block does not stop the extractor reaching a
    // later closed one, because a fence closes whatever opened it.
    let recovered = concat!(
        "```redblue\n",
        "say \"swallows the next fence as its own close\"\n",
        "```\n",
        "```redblue\n",
        "say \"second\"\n",
        "```\n",
    );
    let blocks = extract_blocks(recovered);
    assert_eq!(blocks.len(), 2);
    assert!(blocks.iter().all(|block| block.closed));
    assert_eq!(blocks[1].body, "say \"second\"");

    // No fence at all is no block, not an empty block.
    assert!(extract_blocks("just prose\nwith lines\n").is_empty());
    assert!(extract_blocks("").is_empty());
}

/// An empty block is a block: it is extracted, and it is neither a fence nor
/// skipped for having nothing in it.
#[test]
fn edge_an_empty_block_is_extracted() {
    let blocks = extract_blocks("```redblue\n```\n");
    assert_eq!(blocks.len(), 1, "an empty body is still a block");
    assert_eq!(blocks[0].body, "");
    assert!(blocks[0].closed);
    assert_eq!(blocks[0].source(), "\n");
    assert_eq!(
        blocks[0].key(),
        ":1",
        "an unfiled block has no document yet"
    );

    // A fence with only blank lines inside it is the same case, and the real
    // documents have no such block — so if one appears it is classified rather
    // than skipped.
    let blocks = extract_blocks("```redblue\n\n\n```\n");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].body, "\n");
}

/// A fence that is not tagged `redblue` is not documentation Redblue code, and
/// is left alone — as is a bare fence.
#[test]
fn edge_a_fence_that_is_not_tagged_redblue_is_ignored() {
    let text = concat!(
        "```\n",                         // 1  bare: not tagged
        "say \"bare fence\"\n",          // 2
        "```\n",                         // 3
        "```text\n",                     // 4
        "say \"prose, not code\"\n",     // 5
        "```\n",                         // 6
        "```bash\n",                     // 7
        "rb run hello.rb\n",             // 8
        "```\n",                         // 9
        "```ruby\n",                     // 10
        "puts \"hi\"\n",                 // 11
        "```\n",                         // 12
        "```redblue\n",                  // 13
        "say \"the one that counts\"\n", // 14
        "```\n",                         // 15
    );
    let blocks = extract_blocks(text);
    assert_eq!(
        blocks.len(),
        1,
        "only the ```redblue block is documentation; got {blocks:?}"
    );
    assert_eq!(blocks[0].body, "say \"the one that counts\"");
    assert_eq!(blocks[0].line, 13);

    // An info string with a suffix still tags the block redblue — Markdown
    // convention — while a different first word does not, and a tag that merely
    // contains "redblue" is not the tag.
    let blocks = extract_blocks("```redblue title=\"Set the name\"\nset n to 1\n```\n");
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].body, "set n to 1");
    for ignored in ["```notredblue", "```Redblue", "```rust", "```red"] {
        assert!(
            extract_blocks(&format!("{ignored}\nset n to 1\n```\n")).is_empty(),
            "{ignored} must not open a documentation block"
        );
    }

    // CommonMark strips whitespace around the info string, so a fence written
    // ``` redblue is the same fence. The real documents do not write it that
    // way; the rule is pinned so that a future reader of `fence_info` does not
    // have to guess which of the two it is implementing.
    let blocks = extract_blocks("``` redblue \nset n to 1\n```\n");
    assert_eq!(blocks.len(), 1, "a spaced tag is still the tag");
    assert_eq!(blocks[0].body, "set n to 1");
    // An indented fence is still a fence: the documents indent their blocks
    // inside list items, and those are blocks too.
    let blocks = extract_blocks("  ```redblue\n  set n to 1\n  ```\n");
    assert_eq!(blocks.len(), 1, "an indented fence is a fence");
    assert_eq!(blocks[0].body, "  set n to 1");
}

/// A document that is not there is an error naming the document, not a panic
/// and not an empty list.
#[test]
fn edge_a_document_that_does_not_exist_is_an_error_rather_than_a_panic() {
    let error = read_doc(&root(), "docs/NOT_A_DOCUMENT.md")
        .expect_err("a missing document must not read as an empty one");
    assert!(
        error.starts_with("docs/NOT_A_DOCUMENT.md: "),
        "the error must name the document it could not read, got: {error}"
    );

    // And the same call on a real one succeeds, so the error above is about the
    // file and not about the signature.
    let text = read_doc(&root(), "SPEC.md").expect("SPEC.md is in the checkout");
    assert!(
        text.contains("```redblue"),
        "SPEC.md carries redblue blocks"
    );
}
