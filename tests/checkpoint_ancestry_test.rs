//! Audit: every `rbops: phase-NNN checkpoint` commit in the ancestry of redblue
//! `main` appears on a hand-maintained acknowledgement list, with a reason.
//!
//! # Why this exists
//!
//! `rbops/dispatch.sh` resumes an interrupted phase with a real `git merge`, so a
//! phase that was checkpointed mid-flight landed on `main` **without ever passing
//! the reviewer**. That was recorded as prose in a comment. Nothing measured it, so
//! the count could only grow silently. This module is the measurement.
//!
//! # State of the repository, stated plainly
//!
//! * **Six** `rbops: phase-NNN checkpoint` commits are ancestors of `main`:
//!   `7fdea15`, `074c95e`, `563a351`, `286fccd`, `32d52ad`, `97b4043`.
//! * **Six** `Merge remote-tracking branch 'origin/rbops-wip/...'` commits put
//!   them there. That is the recurrence path; it is why the merge count is
//!   measured and pinned too, not just the checkpoint count.
//! * **All six carry code that no human reviewed.**
//! * **Published history is deliberately NOT rewritten.** No rebase, no
//!   filter-branch, no reset, no amend of an already-published commit.
//! * **No force-push is attempted** by this phase, by any test in this file, or by
//!   any gate. AGENTS.md §1.6 forbids it; this file records the tolerance
//!   explicitly rather than erasing it.
//!
//! # The acknowledgement list is data, not a comment
//!
//! It lives in [`ACK_FILE`] and is parsed. Each entry is `<sha>|<reason>`; the
//! reason is required and must be non-empty, so "somebody wrote a SHA down" is
//! not enough — they have to say why it is tolerated. A checkpoint SHA with no
//! entry fails the audit by name.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The checked-in acknowledgement list. Parsed, not skimmed.
const ACK_FILE: &str = "tests/checkpoint_ancestry_ack.txt";

/// A commit on `main` that a resume-path merge published without review.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Acknowledgement {
    sha: String,
    reason: String,
}

/// Everything the audit learns from one pass over `main`'s history.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Audit {
    /// Every `rbops: phase-NNN checkpoint` subject found, in log order.
    checkpoints: Vec<String>,
    /// Of those, the ones with no acknowledgement entry. Must be empty.
    unacknowledged: Vec<String>,
    /// Acknowledged SHAs that `main` no longer contains. Reported, not fatal.
    stale_acknowledgements: Vec<String>,
    /// `Merge remote-tracking branch 'origin/rbops-wip/...'` commits on `main`.
    wip_merges: usize,
}

/// Repository root, so the audit runs against this checkout and not the cwd.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Run `git` in the repository, returning stdout.
///
/// `Err` carries a human-readable reason. The audit must never turn a failed
/// `git` invocation into "no checkpoints found", which would be a false pass.
fn git(args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_root())
        .args(args)
        .output()
        .map_err(|e| format!("could not run `git {}`: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "`git {}` failed ({}): {}",
            args.join(" "),
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The full `main` log, or the reason it could not be read.
///
/// Refuses on a shallow clone. A depth-1 `git log main` is one commit long, so a
/// shallow checkout would look exactly like a clean one — and "the audit found
/// nothing" is the answer we most need to distrust. Truncated history is
/// reported, never laundered into a pass.
fn read_main_log() -> Result<String, String> {
    if !repo_root().join(".git").exists() {
        return Err("no .git here: this is not a repository, so there is no main".into());
    }
    let shallow = git(&["rev-parse", "--is-shallow-repository"])?;
    if shallow.trim() != "false" {
        return Err(format!(
            "history is shallow (git rev-parse --is-shallow-repository = {})",
            shallow.trim()
        ));
    }
    git(&["log", "--format=%H%x09%s", "main"])
}

/// Refuse a history we cannot vouch for, whatever the log says.
///
/// Split out from [`read_main_log`] so the shallow and absent cases can be fed
/// as fixtures instead of requiring a second repository to exist.
fn check_history(is_shallow: bool, log: Result<String, String>) -> Result<String, String> {
    if is_shallow {
        return Err(
            "history is shallow: `git log main` returns a truncated log, and a truncated \
             log would read as a clean one. Run `git fetch --unshallow` before auditing."
                .into(),
        );
    }
    log
}

/// The parsed acknowledgement file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AckFile {
    entries: Vec<Acknowledgement>,
    /// Pinned `Merge remote-tracking branch 'origin/rbops-wip/...'` count.
    wip_merges_on_main: usize,
}

/// Parse `sha|reason` lines and `key=value` directives.
///
/// `#` lines and blank lines are skipped. Anything else that is not a directive
/// must be an entry with a non-empty sha **and** a non-empty reason — an entry
/// that records no reason is a comment wearing a data file's clothes, and is
/// rejected.
fn parse_ack_file(text: &str) -> Result<AckFile, String> {
    let mut entries: Vec<Acknowledgement> = Vec::new();
    let mut merges: Option<usize> = None;

    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lineno = idx + 1;

        if let Some((key, value)) = line.split_once('=') {
            match key.trim() {
                "wip_merges_on_main" => {
                    let n: usize = value.trim().parse().map_err(|_| {
                        format!("line {lineno}: wip_merges_on_main is not a number: {value:?}")
                    })?;
                    if merges.is_some() {
                        return Err(format!("line {lineno}: wip_merges_on_main set twice"));
                    }
                    merges = Some(n);
                }
                other => return Err(format!("line {lineno}: unknown directive {other:?}")),
            }
            continue;
        }

        let (sha, reason) = line
            .split_once('|')
            .ok_or_else(|| format!("line {lineno}: entry has no `|` separator: {line:?}"))?;
        let sha = sha.trim();
        let reason = reason.trim();
        if sha.is_empty() {
            return Err(format!("line {lineno}: entry has an empty sha"));
        }
        if reason.is_empty() {
            return Err(format!("line {lineno}: sha {sha} has no reason recorded"));
        }
        if entries.iter().any(|e| e.sha == sha) {
            return Err(format!("line {lineno}: sha {sha} is listed twice"));
        }
        entries.push(Acknowledgement {
            sha: sha.to_string(),
            reason: reason.to_string(),
        });
    }

    let wip_merges_on_main =
        merges.ok_or_else(|| "the file pins no `wip_merges_on_main=` directive".to_string())?;
    Ok(AckFile {
        entries,
        wip_merges_on_main,
    })
}

/// Does this subject name a phase checkpoint?
///
/// Matches `rbops: phase-<digits> checkpoint` at the start of the subject and
/// nothing else: `rbops: phase- checkpoint` (no digits) and
/// `rbops: phase-01 checkpoints` (plural) are not checkpoints, and a subject
/// that merely contains the phrase is not one either.
fn is_checkpoint_subject(subject: &str) -> bool {
    let Some(rest) = subject.strip_prefix("rbops: phase-") else {
        return false;
    };
    let digits = rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return false;
    }
    let Some(after) = rest[digits..].strip_prefix(" checkpoint") else {
        return false;
    };
    after.is_empty() || after.starts_with(char::is_whitespace)
}

/// Is this the merge that a resume used to publish a checkpoint?
///
/// Only the `rbops-wip` remote is interesting: those are the interrupted-phase
/// branches, and a merge from them is the path that put unreviewed code on main.
fn is_wip_merge_subject(subject: &str) -> bool {
    subject.starts_with("Merge remote-tracking branch 'origin/rbops-wip/")
}

/// Audit a log against the acknowledgement list.
fn audit(log: &str, ack: &AckFile) -> Result<Audit, String> {
    let by_sha: HashMap<&str, &Acknowledgement> =
        ack.entries.iter().map(|e| (e.sha.as_str(), e)).collect();

    let mut checkpoints: Vec<String> = Vec::new();
    let mut wip_merges = 0usize;

    for (idx, raw) in log.lines().enumerate() {
        if raw.trim().is_empty() {
            continue;
        }
        let (sha, subject) = raw
            .split_once('\t')
            .ok_or_else(|| format!("log line {} is not `<sha>\\t<subject>`: {raw:?}", idx + 1))?;
        if sha.is_empty() {
            return Err(format!("log line {} has an empty sha", idx + 1));
        }
        if is_checkpoint_subject(subject) {
            checkpoints.push(sha.to_string());
        }
        if is_wip_merge_subject(subject) {
            wip_merges += 1;
        }
    }

    let unacknowledged = checkpoints
        .iter()
        .filter(|sha| !by_sha.contains_key(sha.as_str()))
        .cloned()
        .collect();
    let stale_acknowledgements = ack
        .entries
        .iter()
        .filter(|e| !checkpoints.contains(&e.sha))
        .map(|e| e.sha.clone())
        .collect();

    Ok(Audit {
        checkpoints,
        unacknowledged,
        stale_acknowledgements,
        wip_merges,
    })
}

/// The checked-in acknowledgement file, read from the repository.
fn load_ack_file() -> Result<AckFile, String> {
    let path = repo_root().join(ACK_FILE);
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("could not read {}: {e}", Path::new(ACK_FILE).display()))?;
    parse_ack_file(&text)
}

/// Run the whole audit against `main`, printing what it measured.
fn audit_main() -> Result<(Audit, AckFile), String> {
    let shallow = git(&["rev-parse", "--is-shallow-repository"])
        .map(|s| s.trim() == "true")
        .unwrap_or(true);
    let log = check_history(shallow, read_main_log());
    let log = log?;
    let ack = load_ack_file()?;
    let result = audit(&log, &ack)?;
    Ok((result, ack))
}

/// A one-line human summary, so `cargo test -- --nocapture` shows the numbers.
fn describe(audit: &Audit, ack: &AckFile) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        "checkpoint commits on main: {} | acknowledged: {} | unacknowledged: {} | \
         wip merge commits on main: {} (pinned {})",
        audit.checkpoints.len(),
        ack.entries.len(),
        audit.unacknowledged.len(),
        audit.wip_merges,
        ack.wip_merges_on_main,
    );
    if !audit.unacknowledged.is_empty() {
        let _ = write!(s, " | UNACKNOWLEDGED: {}", audit.unacknowledged.join(", "));
    }
    if !audit.stale_acknowledgements.is_empty() {
        let _ = write!(
            s,
            " | STALE acknowledgements (no longer on main): {:?}",
            audit.stale_acknowledgements
        );
    }
    s
}

// ---------------------------------------------------------------------------
// The audit, against this repository's main.
// ---------------------------------------------------------------------------

/// Every checkpoint commit in main's ancestry is acknowledged, with a reason.
///
/// The four SHAs the finding named — `7fdea15`, `074c95e`, `563a351`, `286fccd`
/// — plus the two below the boundary the finding's own `git log` could reach
/// (`32d52ad`, `97b4043`) are all six that carry unreviewed code today.
#[test]
fn checkpoint_ancestry_on_main_is_acknowledged() {
    let (result, ack) = audit_main().expect("main's history must be readable and acknowledged");
    println!("{}", describe(&result, &ack));

    assert!(
        result.unacknowledged.is_empty(),
        "{} checkpoint commit(s) on main carry unreviewed code and are not on the \
         acknowledgement list in {ACK_FILE}: {}. Add each one as \
         `<sha>|<reason>` once a human has looked at what it introduced.",
        result.unacknowledged.len(),
        result.unacknowledged.join(", ")
    );

    for measured in [
        "7fdea15f13a0cc2021c5cc69a65a126879e2d554",
        "074c95e24fbf8b1101219bc1f10c520d8b7acd24",
        "563a3513c7fba2d2a39e759632415fef4bb60eb5",
        "286fccd537d963cb9ad1bcbb051bce45d537a683",
        "32d52ad3d50859c97db57dd10542c653f9962ea4",
        "97b4043cd92e7b2b8a7511a6600b8e73ba9d2596",
    ] {
        assert!(
            result.checkpoints.iter().any(|sha| sha == measured),
            "{measured} is expected in main's ancestry but `git log main` did not report it"
        );
        let entry = ack
            .entries
            .iter()
            .find(|e| e.sha == measured)
            .unwrap_or_else(|| panic!("{measured} is on main but has no acknowledgement entry"));
        assert!(
            !entry.reason.is_empty(),
            "{measured} is acknowledged without saying why"
        );
    }
}

/// The recurrence path is measured, not just the checkpoint commits.
///
/// `dispatch.sh` now resumes with `git merge --squash`, so a *new* merge on
/// `main` means something put unreviewed code there by the old route again. The
/// count is pinned in the data file: a new merge fails until somebody decides
/// whether it is another tolerated one.
#[test]
fn wip_merge_commits_on_main_are_counted_and_pinned() {
    let (result, ack) = audit_main().expect("main's history must be readable");
    println!("{}", describe(&result, &ack));

    assert_eq!(
        result.wip_merges, ack.wip_merges_on_main,
        "main now has {} `Merge remote-tracking branch 'origin/rbops-wip/...'` commit(s), \
         but {ACK_FILE} pins {}. A merge onto main is how unreviewed code got there the \
         first time; either acknowledge it or find out how it landed.",
        result.wip_merges, ack.wip_merges_on_main
    );
}

// ---------------------------------------------------------------------------
// The selector and the refusals, on fixtures.
// ---------------------------------------------------------------------------

/// A fixture history holding exactly one SHA that is not on the list.
const FIXTURE_WITH_ONE_UNLISTED: &str = concat!(
    "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\trbops: phase-016 checkpoint 1791156132\n",
    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\tMerge remote-tracking branch \
     'origin/rbops-wip/phase-016'\n",
    "cccccccccccccccccccccccccccccccccccccccc\trbops: phase-078 checkpoint 1799999999\n",
    "dddddddddddddddddddddddddddddddddddddddd\tci: add cargo fmt --check\n",
);

/// An `edge_*` case: one unlisted checkpoint SHA must fail the check by name.
#[test]
fn edge_unlisted_checkpoint_sha_fails_the_check() {
    let ack = parse_ack_file(concat!(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa|phase-016, tolerated\n",
        "wip_merges_on_main=1\n",
    ))
    .expect("a one-entry acknowledgement file");
    let result = audit(FIXTURE_WITH_ONE_UNLISTED, &ack).expect("a well-formed fixture log");

    assert_eq!(
        result.checkpoints,
        vec![
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            "cccccccccccccccccccccccccccccccccccccccc".to_string(),
        ],
        "the selector must pick both checkpoint subjects out of the fixture"
    );
    assert_eq!(
        result.unacknowledged,
        vec!["cccccccccccccccccccccccccccccccccccccccc".to_string()],
        "exactly the unlisted sha must be reported"
    );
    assert!(
        describe(&result, &ack).contains("cccccccccccccccccccccccccccccccccccccccc"),
        "the failure message must name the offending sha"
    );
}

/// A shallow history must be reported as unreadable, never as clean.
#[test]
fn edge_shallow_history_is_not_treated_as_clean() {
    // A depth-1 checkout: one commit, zero checkpoints, and a selector that
    // reports a spotless history. This is the false pass the audit must not make.
    let truncated =
        Ok("dddddddddddddddddddddddddddddddddddddddd\tci: add cargo fmt --check\n".to_string());
    let ack = parse_ack_file("wip_merges_on_main=0\n").expect("a bare directive file");
    let laundered = audit(
        &check_history(false, truncated.clone()).expect("a short log still parses"),
        &ack,
    )
    .expect("a short log still parses");
    assert!(
        laundered.unacknowledged.is_empty(),
        "premise: without the shallow check this truncated log would look clean"
    );

    let err = check_history(true, truncated).expect_err("a shallow history must be refused");
    assert!(
        err.contains("shallow"),
        "the refusal must say why the history could not be read, got: {err}"
    );
}

/// An absent repository must be reported as unreadable, never as clean.
#[test]
fn edge_absent_history_is_not_treated_as_clean() {
    let err = check_history(false, Err("no .git here: this is not a repository".into()))
        .expect_err("a missing repository must be refused");
    assert!(err.contains("no .git"), "got: {err}");

    // And the real accessor must produce a reason rather than an empty log.
    match read_main_log() {
        Err(_) => {}
        Ok(log) => assert!(
            !log.trim().is_empty(),
            "a readable history with no commits should have been an error"
        ),
    }
}

/// Malformed input is rejected rather than silently skipped.
#[test]
fn edge_malformed_log_line_is_rejected() {
    let ack = parse_ack_file("wip_merges_on_main=0\n").expect("a bare directive file");
    let err = audit("no-tab-between-sha-and-subject\n", &ack)
        .expect_err("a line with no tab is not a log line");
    assert!(err.contains("log line 1"), "got: {err}");
}

/// An empty history contains no checkpoints — and the empty case is checked.
#[test]
fn edge_empty_history_has_no_checkpoints() {
    let ack = parse_ack_file("wip_merges_on_main=0\n").expect("a bare directive file");
    let result = audit("", &ack).expect("an empty log is well formed");
    assert_eq!(result.checkpoints, Vec::<String>::new());
    assert_eq!(result.unacknowledged, Vec::<String>::new());
    assert_eq!(result.wip_merges, 0);
    assert_eq!(result.stale_acknowledgements, Vec::<String>::new());

    let blank = audit("\n   \n\n", &ack).expect("blank lines are skipped");
    assert_eq!(blank, result);
}

/// The singleton case: one acknowledged checkpoint, nothing unacknowledged.
#[test]
fn singleton_acknowledged_checkpoint_is_clean() {
    let ack = parse_ack_file(concat!(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa|phase-016, tolerated\n",
        "wip_merges_on_main=1\n",
    ))
    .expect("a one-entry file");
    let log = concat!(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\trbops: phase-016 checkpoint 1791156132\n",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\tMerge remote-tracking branch \
         'origin/rbops-wip/phase-016'\n",
    );
    let result = audit(log, &ack).expect("a well-formed fixture log");
    assert_eq!(result.checkpoints.len(), 1);
    assert!(result.unacknowledged.is_empty());
    assert_eq!(result.wip_merges, 1);
}

/// Subject matching is exact: near misses are not checkpoints.
#[test]
fn edge_checkpoint_subject_matching_is_exact() {
    for yes in [
        "rbops: phase-019 checkpoint 1791227938",
        "rbops: phase-004 checkpoint",
        "rbops: phase-0 checkpoint",
    ] {
        assert!(is_checkpoint_subject(yes), "should match: {yes:?}");
    }
    for no in [
        "rbops: phase- checkpoint",
        "rbops: phase-abc checkpoint",
        "rbops: phase-01 checkpoints",
        "rbops: phase-01 checkpointx",
        "rbops: phase-01 notacheckpoint",
        "ci: rbops: phase-01 checkpoint",
        "Merge remote-tracking branch 'origin/rbops-wip/phase-019'",
        "rbops: phase-01",
        "",
    ] {
        assert!(!is_checkpoint_subject(no), "should not match: {no:?}");
    }
}

/// Only `rbops-wip` merges count as the resume path.
#[test]
fn edge_wip_merge_matching_is_exact() {
    assert!(is_wip_merge_subject(
        "Merge remote-tracking branch 'origin/rbops-wip/phase-019'"
    ));
    for no in [
        "Merge remote-tracking branch 'origin/main'",
        "Merge remote-tracking branch 'origin/rbops-recovery/phase-019'",
        "Merge branch 'rbops-wip/phase-019'",
        "ci: merge remote-tracking branch 'origin/rbops-wip/phase-019'",
        "",
    ] {
        assert!(!is_wip_merge_subject(no), "should not match: {no:?}");
    }
}

/// An acknowledgement without a reason is a comment, and is rejected.
#[test]
fn edge_acknowledgement_entries_require_a_sha_and_a_reason() {
    // Missing reason — the failure this whole phase is about, in miniature.
    let err = parse_ack_file("deadbeef|\nwip_merges_on_main=0\n")
        .expect_err("an entry with no reason is not data");
    assert!(err.contains("no reason"), "got: {err}");

    // Missing sha.
    assert!(parse_ack_file(" |tolerated\nwip_merges_on_main=0\n")
        .expect_err("an entry with no sha is not data")
        .contains("empty sha"));

    // Duplicate sha: the list would then have two reasons and no way to say which
    // was the operative one.
    let err = parse_ack_file(concat!(
        "deadbeef|first reason\n",
        "deadbeef|second reason\n",
        "wip_merges_on_main=0\n",
    ))
    .expect_err("a duplicated sha is ambiguous");
    assert!(err.contains("twice"), "got: {err}");

    // Missing and unknown directives.
    assert!(parse_ack_file("deadbeef|tolerated\n")
        .expect_err("the pinned merge count is required")
        .contains("wip_merges_on_main"));
    assert!(parse_ack_file("wip_merge_on_main=0\n")
        .expect_err("a misspelled directive must not be ignored")
        .contains("unknown directive"));

    // The well-formed minimum still parses.
    let ok = parse_ack_file("deadbeef|tolerated\nwip_merges_on_main=0\n").expect("valid");
    assert_eq!(ok.entries.len(), 1);
    assert_eq!(ok.wip_merges_on_main, 0);
}

/// The checked-in acknowledgement file itself is well formed and non-empty.
#[test]
fn checked_in_acknowledgement_file_parses() {
    let ack = load_ack_file().unwrap_or_else(|e| panic!("{ACK_FILE} must parse: {e}"));
    assert!(
        !ack.entries.is_empty(),
        "{ACK_FILE} is empty, so the audit would acknowledge nothing and say so silently"
    );
    assert!(
        ack.entries.iter().all(|e| !e.reason.is_empty()),
        "{ACK_FILE} has an entry with no reason"
    );
    let raw = std::fs::read_to_string(repo_root().join(ACK_FILE)).expect("the ack file");
    assert!(
        raw.contains('|'),
        "{ACK_FILE} must hold `<sha>|<reason>` entries, not prose"
    );
}

/// The file this audit depends on is where the module doc says it is.
#[test]
fn acknowledgement_file_lives_under_tests() {
    assert!(ACK_FILE.starts_with("tests/"), "{ACK_FILE}");
    assert!(repo_root().join(ACK_FILE).exists(), "{ACK_FILE} is missing");
}
