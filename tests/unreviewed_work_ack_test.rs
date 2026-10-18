//! The acknowledgement list is data, not a comment.
//!
//! This file replaced a placeholder that asserted `assert!(true, ...)`, which
//! `cargo clippy -- -D warnings` rejects outright — an un-compilable test is a
//! red gate, not a placeholder. It is kept, and given the smallest assertion
//! that can actually fail.
//!
//! # Why it is separate from the audit
//!
//! `tests/checkpoint_ancestry_test.rs` is the audit: it reads `main`'s history
//! with `git` and refuses to measure a truncated one. That audit needs a
//! repository, so it says nothing in a checkout that has none. This file reads
//! `tests/checkpoint_ancestry_ack.txt` with no `git` at all, so an emptied,
//! truncated or half-written acknowledgement list fails even when there is no
//! history to compare it against — which is exactly the state the audit would
//! otherwise launder into a clean pass (it reports "zero checkpoints
//! acknowledged" and, with no history, "zero checkpoints").
//!
//! Nothing here is weakened or skipped: every assertion can fail.

use std::collections::HashSet;
use std::path::PathBuf;

/// The checked-in acknowledgement list, read from the repository root rather
/// than the working directory so the test does not depend on where cargo was
/// invoked from.
const ACK_FILE: &str = "tests/checkpoint_ancestry_ack.txt";

fn ack_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(ACK_FILE)
}

/// One `sha|reason` line, or a directive. Anything else is malformed.
fn is_entry(line: &str) -> bool {
    line.contains('|')
}

#[test]
fn unreviewed_work_ack() {
    let text = std::fs::read_to_string(ack_path())
        .unwrap_or_else(|e| panic!("{ACK_FILE} must be readable: {e}"));

    let entries: Vec<(&str, &str)> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && is_entry(l))
        .map(|l| {
            let (sha, reason) = l
                .split_once('|')
                .unwrap_or_else(|| panic!("{ACK_FILE}: an entry has no `|`: {l:?}"));
            (sha.trim(), reason.trim())
        })
        .collect();

    assert!(
        !entries.is_empty(),
        "{ACK_FILE} acknowledges nothing, so every tolerated checkpoint commit reads as \
         unreviewed and unaccounted for"
    );

    let mut seen = HashSet::new();
    for (sha, reason) in &entries {
        assert!(
            !sha.is_empty(),
            "{ACK_FILE} has an entry with no sha; the file is data or it is a comment"
        );
        assert!(
            !reason.is_empty(),
            "{ACK_FILE} lists {sha} without saying why it is tolerated: recording a sha is \
             not the same as acknowledging one"
        );
        assert!(
            seen.insert(*sha),
            "{ACK_FILE} lists {sha} twice, so which reason applies is ambiguous"
        );
    }

    let pinned = text
        .lines()
        .find(|l| l.trim_start().starts_with("wip_merges_on_main="))
        .unwrap_or_else(|| {
            panic!(
                "{ACK_FILE} pins no `wip_merges_on_main=` directive, so the audit has nothing to \
                 compare the measured merge count against"
            )
        });
    let count = pinned
        .trim()
        .trim_start_matches("wip_merges_on_main=")
        .trim();
    assert!(
        count.parse::<usize>().is_ok(),
        "{ACK_FILE} pins wip_merges_on_main={count:?}, which is not a number"
    );
}

/// The empty file is the failure this list is most able to cause silently.
///
/// An empty acknowledgement list parses, matches nothing, and reports a clean
/// history — so its emptiness must be caught before it is read as agreement.
#[test]
fn edge_an_empty_acknowledgement_list_is_not_agreement() {
    for body in [
        "",
        "\n\n",
        "# nothing recorded yet\n",
        "wip_merges_on_main=0\n",
    ] {
        let entries: Vec<&str> = body
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#') && is_entry(l))
            .collect();
        assert!(
            entries.is_empty(),
            "premise: {body:?} lists no commit, so an audit reading it acknowledges nothing"
        );
    }

    let text = std::fs::read_to_string(ack_path()).expect(ACK_FILE);
    assert!(
        text.lines().any(is_entry),
        "{ACK_FILE} must hold at least one `<sha>|<reason>` line"
    );
}

/// A line that is neither an entry nor a directive is a corruption, not a note.
#[test]
fn edge_an_entry_without_a_reason_is_rejected() {
    for bad in ["7fdea15|\n", "7fdea15|   \n", "|\ntolerated\n"] {
        let line = bad.trim_end_matches('\n');
        let (sha, reason) = line
            .split_once('|')
            .unwrap_or_else(|| panic!("premise: {bad:?} should split"));
        assert!(
            sha.trim().is_empty() || reason.trim().is_empty(),
            "premise: {bad:?} is an entry with no usable half"
        );
    }

    // The checked-in file is the other half of the claim: no half-empty entry.
    let text = std::fs::read_to_string(ack_path()).expect(ACK_FILE);
    for line in text.lines().filter(|l| is_entry(l.trim())) {
        let (sha, reason) = line.trim().split_once('|').expect("split");
        assert!(
            !sha.trim().is_empty() && !reason.trim().is_empty(),
            "{ACK_FILE} has a half-empty entry: {line:?}"
        );
    }
}
