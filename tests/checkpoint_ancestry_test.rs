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
//! entry fails the audit by name, and an entry naming a commit `main` does not
//! contain fails it too — the other half of the same promise, because a list of
//! SHAs that satisfy nothing is a comment with a `.txt` extension.
//!
//! # The audit never measures a truncated log
//!
//! A depth-1 clone's `git log main` is one commit long, and one commit long
//! reads as a spotless history. So a shallow clone is never measured on what it
//! has: the audit completes it first (`git fetch --unshallow`), then reads the
//! depth *again* and refuses if the history is still short. Completing a clone
//! is not tolerating a short one. What the audit will never do is report "no
//! checkpoints found" out of a log it knows is incomplete.
//!
//! # The repair waits for the clone, not for a count of attempts
//!
//! The repair can lose `.git/shallow.lock` to another fetch of the same clone
//! — the pipeline's own, or the other audit file's. A fetch that loses the
//! lock leaves the clone exactly as short as it was, so the repair waits for
//! the clone's **state**: the depth is re-read after every attempt, and the
//! depth is watched until a fetch that is already in flight lands. A repair
//! that counted its attempts instead refused clones that were complete a
//! moment later, which is the false refusal this audit exists to prevent.

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One history repair at a time.
///
/// `cargo test` runs the tests in a binary on parallel threads and three of
/// them audit `main`, so on a shallow clone all three would run
/// `git fetch --unshallow` at the same instant and collide on
/// `.git/shallow.lock`. From the outside that is indistinguishable from the
/// "clone cannot be completed" refusal this audit is built around, so the
/// repair is serialised: it happens once, and the threads behind it find the
/// clone already complete.
static HISTORY_REPAIR: Mutex<()> = Mutex::new(());

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
    /// Acknowledged SHAs that `main` no longer contains. Must be empty: an entry
    /// pointing at nothing is how the list stops being data.
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

/// Is this clone shallow?
///
/// `git rev-parse --is-shallow-repository` answers exactly `true` or `false`.
/// Anything else is not a depth reading, it is a git we do not understand, and
/// it is an error rather than a `false` — a `false` here would launder a
/// truncated log into a clean history.
fn is_shallow() -> Result<bool, String> {
    let out = git(&["rev-parse", "--is-shallow-repository"])?;
    match out.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!(
            "`git rev-parse --is-shallow-repository` said {other:?}, which is neither \
             `true` nor `false`; refusing to guess whether the history is complete"
        )),
    }
}

/// How the repair behaves.
///
/// The repair is a `git fetch --unshallow`, and it can lose
/// `.git/shallow.lock` to another fetch of the same clone — the pipeline's
/// own, or the other audit file's. A fetch that loses the lock leaves the
/// clone exactly as short as it was, so **counting attempts is the wrong thing
/// to count**: three attempts a second apart is a two-second window, and a
/// contending fetch that holds the lock for three seconds loses every one of
/// them. What the repair has to wait for is the clone's *state*, and the
/// ceiling on that wait is generous, because the fetch that will complete the
/// clone may still be running.
#[derive(Clone, Copy)]
struct RepairPolicy {
    /// How many times the repair may be attempted.
    ///
    /// Bounded, because a clone that cannot be completed is an error, not a
    /// hang.
    attempts: usize,
    /// How long to wait between two attempts.
    retry_delay: Duration,
    /// How long, in total, the repair is willing to spend watching the clone's
    /// depth for a completion that lands late.
    wait: Duration,
    /// How often the depth is read while waiting.
    poll: Duration,
}

/// The policy the audit runs under: three attempts, and half a minute of
/// watching the clone's depth for the completion that lands late.
///
/// The ceiling is generous on purpose. It has to outlast a fetch that is
/// already in flight, because that is the fetch whose lock every attempt
/// loses. A ceiling that is too small brings back the failure this file
/// exists to prevent: a clone that is complete a moment later is refused.
fn repair_policy() -> RepairPolicy {
    RepairPolicy {
        attempts: 3,
        retry_delay: Duration::from_secs(1),
        wait: Duration::from_secs(30),
        poll: Duration::from_millis(200),
    }
}

/// The same policy with the clock turned down, for the tests that drive the
/// decision from fixtures.
///
/// The shape is the shape production has — the same number of attempts, the
/// same order of events — with the waits scaled down so that a test does not
/// spend half a minute proving a point about milliseconds.
fn fixture_policy() -> RepairPolicy {
    RepairPolicy {
        attempts: 3,
        retry_delay: Duration::from_millis(20),
        wait: Duration::from_millis(500),
        poll: Duration::from_millis(5),
    }
}

/// Make the clone able to answer for `main`'s whole history, or refuse.
///
/// `is_shallow` reports the depth and `complete` is the one repair the audit is
/// allowed to attempt. Both are parameters so this decision can be driven from
/// fixtures instead of from a network, and so the refusal after a failed repair
/// is testable without one.
///
/// This is **not** a tolerance for shallow history. A depth-1 clone's `git log
/// main` is one commit long, and one commit long reads as a spotless history —
/// the exact false pass this audit exists to prevent. So a shallow clone is
/// never measured on what it has: it is completed first, and if it is still
/// short afterwards the audit refuses instead of reporting a truncated log.
///
/// The repair waits for the clone's **state**, not for a count of attempts: a
/// fetch that loses `.git/shallow.lock` to a fetch that is still running
/// leaves the clone short, and the clone becomes complete the moment that
/// fetch lands. Counting attempts refused a clone that was complete a moment
/// later; watching the depth does not.
fn ensure_history(
    is_shallow: &dyn Fn() -> Result<bool, String>,
    complete: &dyn Fn() -> Result<(), String>,
) -> Result<(), String> {
    ensure_history_with(is_shallow, complete, repair_policy())
}

/// The decision, with its policy as a third parameter.
///
/// The policy is a parameter because the production ceiling is half a minute
/// and a test that drives the decision from fixtures must not spend half a
/// minute proving a point about milliseconds. The shape is the same either
/// way: the same attempts, the same order of events, the same refusal.
fn ensure_history_with(
    is_shallow: &dyn Fn() -> Result<bool, String>,
    complete: &dyn Fn() -> Result<(), String>,
    policy: RepairPolicy,
) -> Result<(), String> {
    if !is_shallow()? {
        return Ok(());
    }
    // A fetch that loses the lock leaves the clone exactly as short as it was,
    // so one attempt refuses a clone that is complete a moment later. The
    // repair therefore waits for the clone's *state*: the depth is re-read
    // after every attempt, and between attempts the depth is watched until
    // the next attempt is due. The attempts are bounded and the watching is
    // bounded too, so a clone that really cannot be completed is still
    // refused.
    let mut failure: Option<String> = None;
    let mut waited = Duration::ZERO;
    for attempt in 1..=policy.attempts {
        if let Err(e) = complete() {
            failure = Some(e);
        }
        if !is_shallow()? {
            return Ok(());
        }
        if attempt == policy.attempts {
            break;
        }
        // The fetch that holds the lock may land at any moment, and when it
        // does the clone is complete. Watch for it rather than counting.
        let window = policy.retry_delay.min(policy.wait.saturating_sub(waited));
        if wait_for_completion(is_shallow, &mut waited, window, policy.poll)? {
            return Ok(());
        }
    }
    // The attempts are spent and the clone is still short — but the fetch that
    // holds the lock may still be running, and it will leave the clone
    // complete. The rest of the budget is spent watching for that, and only
    // then is the clone refused.
    let window = policy.wait.saturating_sub(waited);
    if wait_for_completion(is_shallow, &mut waited, window, policy.poll)? {
        return Ok(());
    }
    Err(refusal(failure))
}

/// Watch the clone's depth until it is complete or the window ends.
///
/// `waited` is charged with the time spent here, so the ceiling covers every
/// wait the repair makes and not only this one. The answer is whether the
/// clone became complete: a repair that lands late is accepted, and a clone
/// that never becomes complete is refused by the caller.
fn wait_for_completion(
    is_shallow: &dyn Fn() -> Result<bool, String>,
    waited: &mut Duration,
    window: Duration,
    poll: Duration,
) -> Result<bool, String> {
    let start = Instant::now();
    loop {
        if !is_shallow()? {
            *waited += start.elapsed();
            return Ok(true);
        }
        let elapsed = start.elapsed();
        if elapsed >= window {
            *waited += elapsed;
            return Ok(false);
        }
        std::thread::sleep(poll.max(Duration::from_millis(1)).min(window - elapsed));
    }
}

/// The refusal, with the repair's own failure named when there was one.
///
/// The reason is in the refusal because "the clone could not be completed" and
/// "the fetch could not take the lock" are different problems, and a reader
/// holding the second one cannot fix it by running the first one's remedy.
fn refusal(failure: Option<String>) -> String {
    match failure {
        Some(e) => format!(
            "history is shallow and completing the clone did not make it complete: {e}. \
             The audit refuses to read a truncated `git log main`, because a truncated log \
             reads as a clean one. Run `git fetch --unshallow` before auditing."
        ),
        None => "history is shallow and completing the clone did not make it complete: \
             `git log main` would return a truncated log, and a truncated log reads as a \
             clean one. Run `git fetch --unshallow` before auditing."
            .into(),
    }
}

/// The real accessor: this clone, and the single repair the audit may attempt.
///
/// A CI checkout is a depth-1 clone (`actions/checkout@v4` defaults to
/// `fetch-depth: 1`), so without this the audit would fail on every run. The
/// repair is a fetch with an explicit refspec rather than a relaxed check: it
/// gets the missing history instead of pretending the history it has is
/// enough, and it leaves a branch to audit behind even when the checkout is a
/// pull request's detached merge ref, which has no local `main`.
fn ensure_full_history() -> Result<(), String> {
    let _guard = match HISTORY_REPAIR.lock() {
        Ok(guard) => guard,
        // The lock guards a subprocess call, not state. A thread that panicked
        // leaves nothing half-written in the repository, so the next thread
        // may carry on rather than turn one failure into three.
        Err(poisoned) => poisoned.into_inner(),
    };
    ensure_history(&is_shallow, &|| {
        git(&[
            "fetch",
            "--unshallow",
            "--quiet",
            "origin",
            "+refs/heads/*:refs/remotes/origin/*",
        ])
        .map(|_| ())
    })
}

/// The branch to audit: `main`, wherever this clone keeps it.
///
/// CI checks a pull request out as a detached merge ref, so there is no local
/// `refs/heads/main` there — only the remote-tracking `origin/main`. Both name
/// the same commits, so reading whichever exists is the same audit, not a
/// substitute for it. If neither exists there is no main here, and the audit
/// says so instead of measuring whatever branch it happens to be sitting on.
fn resolve_main() -> Result<String, String> {
    for candidate in ["refs/heads/main", "refs/remotes/origin/main"] {
        if git(&["rev-parse", "--verify", "--quiet", candidate]).is_ok() {
            return Ok(candidate.to_string());
        }
    }
    Err("no main here: this clone has neither refs/heads/main nor \
         refs/remotes/origin/main, so there is no main history to audit"
        .into())
}

/// The full `main` log, or the reason it could not be read.
///
/// Refuses on a shallow clone. A depth-1 `git log main` is one commit long, so a
/// shallow checkout would look exactly like a clean one — and "the audit found
/// nothing" is the answer we most need to distrust. Truncated history is
/// reported, never laundered into a pass.
///
/// Callers that mean to audit this repository go through [`audit_main`], which
/// completes a shallow clone first ([`ensure_full_history`]) rather than
/// skipping the audit.
fn read_main_log() -> Result<String, String> {
    if !repo_root().join(".git").exists() {
        return Err("no .git here: this is not a repository, so there is no main".into());
    }
    if is_shallow()? {
        return Err(
            "history is shallow (git rev-parse --is-shallow-repository = true); \
             `git log main` on it is a truncated log, which reads as a clean one"
                .into(),
        );
    }
    let main = resolve_main()?;
    git(&["log", "--format=%H%x09%s", main.as_str()])
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
///
/// A line is a directive only when its `=` comes before any `|`. A `=` inside a
/// reason belongs to the reason: `deadbeef|tolerated: old_gate=true` is an
/// entry, and reading it as a directive would reject a perfectly good
/// acknowledgement as an unknown one — which is the failure mode where a real
/// entry silently cannot be recorded at all.
fn parse_ack_file(text: &str) -> Result<AckFile, String> {
    let mut entries: Vec<Acknowledgement> = Vec::new();
    let mut merges: Option<usize> = None;

    for (idx, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lineno = idx + 1;

        // `find` returns char boundaries, so slicing at them cannot panic on a
        // multi-byte character.
        let directive_at = line
            .find('=')
            .filter(|eq| line.find('|').is_none_or(|pipe| pipe > *eq));

        if let Some(eq) = directive_at {
            let (key, value) = (&line[..eq], &line[eq + 1..]);
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
    // A shallow clone is completed before anything is measured, never measured
    // short. `ensure_full_history` is the only repair, and it refuses if the
    // repair does not take, so the depth is checked twice: once to decide, and
    // once by `check_history` below to refuse.
    ensure_full_history()?;
    let log = check_history(is_shallow()?, read_main_log())?;
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

    assert!(
        result.stale_acknowledgements.is_empty(),
        "{} entry/entries in {ACK_FILE} name a commit that is not a \
         `rbops: phase-NNN checkpoint` in main's ancestry: {}. An entry pointing at \
         nothing is how the list decays into a comment — an abbreviated SHA, a \
         mistyped one, or a commit that was rewritten (which AGENTS.md forbids) all \
         parse fine and would satisfy nothing. Correct the SHA or delete the entry.",
        result.stale_acknowledgements.len(),
        result.stale_acknowledgements.join(", ")
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

// ---------------------------------------------------------------------------
// Boundary cases the first pass through this file did not reach.
// ---------------------------------------------------------------------------

/// One phase, checkpointed twice, both checkpoints must be listed.
///
/// This is not hypothetical: `7fdea15` and `074c95e` are both
/// `rbops: phase-019 checkpoint`, and `32d52ad` and `97b4043` are both
/// `rbops: phase-004 checkpoint`. A list keyed on the *phase number* would
/// acknowledge one and silently tolerate the other, so the key is the SHA.
#[test]
fn edge_a_phase_checkpointed_twice_needs_both_shas() {
    let ack = parse_ack_file(concat!(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa|phase-019, first checkpoint\n",
        "wip_merges_on_main=2\n",
    ))
    .expect("one of the pair is listed");
    let log = concat!(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\trbops: phase-019 checkpoint 1791227938\n",
        "cccccccccccccccccccccccccccccccccccccccc\trbops: phase-019 checkpoint 1791198338\n",
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\tMerge remote-tracking branch \
         'origin/rbops-wip/phase-019'\n",
        "dddddddddddddddddddddddddddddddddddddddd\tMerge remote-tracking branch \
         'origin/rbops-wip/phase-019'\n",
    );
    let result = audit(log, &ack).expect("a well-formed fixture log");

    assert_eq!(
        result.checkpoints.len(),
        2,
        "both checkpoints are in the log"
    );
    assert_eq!(
        result.unacknowledged,
        vec!["cccccccccccccccccccccccccccccccccccccccc".to_string()],
        "listing one checkpoint of a phase does not acknowledge its sibling"
    );
}

/// An abbreviated SHA must not stand in for the commit it names.
///
/// The data file holds full SHAs. An entry of `7fdea15` would match nothing in
/// `git log --format=%H`, and the audit would report the real commit as
/// unacknowledged while looking like it had been listed — the exact silent
/// failure the phase exists to prevent.
#[test]
fn edge_an_abbreviated_sha_does_not_acknowledge_its_commit() {
    let ack = parse_ack_file(concat!(
        "7fdea15|phase-019, tolerated: abbreviated by mistake\n",
        "wip_merges_on_main=0\n",
    ))
    .expect("an abbreviated sha is well formed data");
    let log = "7fdea15f13a0cc2021c5cc69a65a126879e2d554\trbops: phase-019 checkpoint 1791227938\n";
    let result = audit(log, &ack).expect("a well-formed fixture log");

    assert!(
        result.unacknowledged.len() == 1,
        "the abbreviated entry must not satisfy the full sha, got {:?}",
        result.unacknowledged
    );
    assert_eq!(
        result.stale_acknowledgements,
        vec!["7fdea15".to_string()],
        "the abbreviated entry points at nothing on main"
    );
}

/// A reason made only of whitespace is no reason at all.
#[test]
fn edge_a_whitespace_only_reason_is_rejected() {
    for body in ["deadbeef|   \n", "deadbeef|\t\n", "deadbeef|\u{a0}\n"] {
        assert!(
            parse_ack_file(&format!("{body}wip_merges_on_main=0\n")).is_err(),
            "a blank reason must be rejected, got accepted: {body:?}"
        );
    }
    // A reason that merely *contains* a separator is still a reason:
    // `split_once` splits at the first `|`, so the rest is the reason verbatim.
    let kept = parse_ack_file("deadbeef|see the other note | here\nwip_merges_on_main=0\n")
        .expect("a reason may contain a pipe");
    assert_eq!(kept.entries[0].reason, "see the other note | here");
}

/// A data file of nothing but comments acknowledges nothing, and says so.
#[test]
fn edge_a_comment_only_acknowledgement_file_is_rejected() {
    let err = parse_ack_file("# every one of them is fine\n# really\n")
        .expect_err("comments are not acknowledgements");
    assert!(err.contains("wip_merges_on_main"), "got: {err}");

    // And with the directive supplied, it parses but acknowledges nothing — which
    // the real audit must notice rather than report as a clean main.
    let ack = parse_ack_file("# nothing here\nwip_merges_on_main=0\n")
        .expect("a comment-only file with the pinned count parses");
    assert!(ack.entries.is_empty());

    let log = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\trbops: phase-016 checkpoint 1791156132\n";
    let result = audit(log, &ack).expect("a well-formed fixture log");
    assert_eq!(
        result.unacknowledged.len(),
        1,
        "an empty list acknowledges nothing"
    );
    assert_eq!(result.stale_acknowledgements, Vec::<String>::new());
}

/// A pinned merge count must be a count, not a number-shaped string.
#[test]
fn edge_the_pinned_merge_count_must_be_a_number() {
    for bad in [
        "wip_merges_on_main=-1\n",
        "wip_merges_on_main=6x\n",
        "wip_merges_on_main=\n",
    ] {
        let err = parse_ack_file(bad).expect_err("a bad count is not a count");
        assert!(
            err.contains("not a number"),
            "expected a not-a-number error for {bad:?}, got: {err}"
        );
    }
    // 0 is a legitimate count: main with no wip merges is exactly the state
    // dispatch.sh's `git merge --squash` resume leaves behind.
    assert_eq!(
        parse_ack_file("wip_merges_on_main=0\n")
            .expect("zero is a count")
            .wip_merges_on_main,
        0
    );
}

/// A CRLF checkout must not make every entry look like it has a reason of `\r`.
///
/// The parser is what is under test here, so the fixture is written by hand
/// rather than read off disk. Whether the *checked-in* file arrives with CRLF
/// is a property of the checkouter's EOL normalisation (`core.autocrlf`), not
/// of this parser, and asserting on it would couple the audit to how somebody
/// cloned the repository — a test that fails on a Windows clone for a reason no
/// code can fix. `lines()` plus `trim` make the two forms identical; that is
/// the whole claim.
#[test]
fn edge_crlf_line_endings_in_the_acknowledgement_file_parse() {
    let lf = parse_ack_file("deadbeef|tolerated\nwip_merges_on_main=0\n").expect("lf parses");
    let crlf = parse_ack_file("deadbeef|tolerated\r\nwip_merges_on_main=0\r\n")
        .expect("crlf parses the same as lf");
    assert_eq!(
        lf, crlf,
        "a \\r must not become part of the sha or the reason"
    );

    // Every entry, not just the sha: a `\r` that survived `trim` would sit at
    // the end of the reason and make two different files compare unequal.
    assert_eq!(crlf.entries[0].sha, "deadbeef");
    assert_eq!(crlf.entries[0].reason, "tolerated");
    assert_eq!(crlf.wip_merges_on_main, 0);
}

/// Subjects the audit must never mistake for a checkpoint, whatever they contain.
///
/// The sha/subject split is a tab, so a subject containing spaces, quotes or a
/// pipe — all of which the acknowledgement file itself uses — stays intact.
#[test]
fn edge_subject_content_after_the_marker_is_ignored() {
    for subject in [
        "rbops: phase-019 checkpoint 1791227938",
        "rbops: phase-019 checkpoint\tsubject with a tab",
        "rbops: phase-019 checkpoint |a pipe| and \"quotes\"",
        "rbops: phase-019 checkpoint",
    ] {
        let log = format!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\t{subject}\n");
        let ack = parse_ack_file("wip_merges_on_main=0\n").expect("a bare directive file");
        let result = audit(&log, &ack).expect("a well-formed fixture log");
        assert_eq!(
            result.checkpoints.len(),
            1,
            "should still select: {subject:?}"
        );
    }
}

/// An `=` inside a reason is part of the reason, not a directive.
///
/// The separator decides what a line is, not the `=`. Reading `=` first meant
/// `deadbeef|a=b` was tried as `key=value`, failed as an "unknown directive",
/// and could not be recorded at all — so the exact entries that need the most
/// careful prose are the ones the data file refuses.
#[test]
fn edge_an_equals_sign_in_a_reason_is_not_read_as_a_directive() {
    let ack = parse_ack_file(concat!(
        "deadbeef|phase-019, tolerated: the old gate=true is what shipped\n",
        "wip_merges_on_main=0\n",
    ))
    .expect("an `=` in a reason must not turn the line into a directive");
    assert_eq!(ack.entries.len(), 1, "the entry survived");
    assert_eq!(ack.entries[0].sha, "deadbeef");
    assert_eq!(
        ack.entries[0].reason, "phase-019, tolerated: the old gate=true is what shipped",
        "the reason is kept verbatim, `=` and all"
    );
    assert_eq!(
        ack.wip_merges_on_main, 0,
        "the directive still parsed as a directive"
    );

    // A reason that quotes the directive by name, is still a reason — and the
    // real directive on the next line is not mistaken for a duplicate.
    let ack = parse_ack_file(concat!(
        "deadbeef|wip_merges_on_main=99 was the old pinned value\n",
        "wip_merges_on_main=1\n",
    ))
    .expect("a reason quoting a directive is still a reason");
    assert_eq!(ack.entries.len(), 1);
    assert_eq!(
        ack.entries[0].reason,
        "wip_merges_on_main=99 was the old pinned value"
    );
    assert_eq!(ack.wip_merges_on_main, 1, "the real directive sets the pin");

    // A leading `=` still makes a line a directive candidate, and an unknown one
    // is still rejected: the rule is about ORDER, not about refusing `=` lines.
    assert!(parse_ack_file("wip_merge_on_main=0\n")
        .expect_err("a misspelled directive must not be ignored")
        .contains("unknown directive"));
    assert!(parse_ack_file("=0\n")
        .expect_err("an empty directive name is not a directive")
        .contains("unknown directive"));

    // And the sha side is untouched: a `|` before an `=` means entry, whatever
    // the reason is.
    let ack = parse_ack_file("deadbeef|a=b=c\nwip_merges_on_main=0\n")
        .expect("only the first `=` would ever have been a separator");
    assert_eq!(ack.entries[0].reason, "a=b=c");
}

/// A shallow clone is completed, and a repair that does not take is a refusal.
///
/// The depth-1 log is the premise: measured as-is it audits clean, which is why
/// the audit may never be handed one. Everything below drives [`ensure_history`]
/// from fixtures, so the "refuses" half is asserted without a network.
#[test]
fn edge_a_shallow_clone_is_completed_rather_than_audited_short() {
    let ack = parse_ack_file("wip_merges_on_main=0\n").expect("a bare directive file");
    let truncated: Result<String, String> =
        Ok("dddddddddddddddddddddddddddddddddddddddd\tci: add cargo fmt --check\n".to_string());
    assert!(
        audit(&truncated.clone().expect("a short log still parses"), &ack)
            .expect("a short log still parses")
            .unacknowledged
            .is_empty(),
        "premise: the depth-1 log the audit must refuse audits as a clean main"
    );

    // Complete already: no repair is attempted, so nothing is invented.
    let repairs = Cell::new(0usize);
    ensure_history_with(
        &|| Ok(false),
        &|| {
            repairs.set(repairs.get() + 1);
            Ok(())
        },
        fixture_policy(),
    )
    .expect("a complete clone is audit-ready");
    assert_eq!(repairs.get(), 0, "a complete clone must not be 'repaired'");

    // Shallow and the repair fails: an error, and git's own reason survives it.
    let err = ensure_history_with(
        &|| Ok(true),
        &|| Err("fatal: 'origin' does not appear to be a git repository".into()),
        fixture_policy(),
    )
    .expect_err("a shallow clone that cannot be completed is not auditable");
    assert!(
        err.contains("shallow"),
        "the refusal must say the history was short, got: {err}"
    );
    assert!(
        err.contains("truncated"),
        "and what refusing to do protects against, got: {err}"
    );
    assert!(
        err.contains("does not appear to be a git repository"),
        "the underlying git reason must not be swallowed, got: {err}"
    );

    // Shallow, the repair runs, and the clone is STILL short: refused, not
    // audited. This is the case that would otherwise be the false pass.
    let attempts = Cell::new(0usize);
    let err = ensure_history_with(
        &|| Ok(true),
        &|| {
            attempts.set(attempts.get() + 1);
            Ok(())
        },
        fixture_policy(),
    )
    .expect_err("a clone that is still shallow reads as a clean one, so it is refused");
    assert!(
        attempts.get() >= 1,
        "the repair must have been attempted before the history is refused"
    );
    assert!(
        attempts.get() <= fixture_policy().attempts,
        "the repair must be bounded, or a clone that cannot be completed is a \
         hang instead of an error: {} attempts",
        attempts.get()
    );
    assert!(err.contains("truncated"), "got: {err}");

    // Shallow, the repair runs, and the clone is now complete: audited.
    let reads = Cell::new(0usize);
    ensure_history_with(
        &|| {
            reads.set(reads.get() + 1);
            Ok(reads.get() == 1)
        },
        &|| Ok(()),
        fixture_policy(),
    )
    .expect("a clone that was completed is audit-ready");
    assert_eq!(
        reads.get(),
        2,
        "the depth is re-read after the repair, not assumed"
    );

    // An unreadable depth is an error, not a `false` that would launder a
    // truncated log into a clean history.
    assert!(ensure_history_with(
        &|| Err("not a repository".into()),
        &|| Ok(()),
        fixture_policy()
    )
    .expect_err("an unknown depth is not a depth of zero")
    .contains("not a repository"));

    // And the real accessor: this checkout is complete, so nothing is fetched.
    ensure_full_history().expect("this checkout's history is complete, so no fetch is needed");
}

/// A repair that lands after every attempt has been spent is still accepted.
///
/// This is the contention the repair can meet against a fetch from outside
/// this binary: the pipeline's own fetch holds `.git/shallow.lock` for as long
/// as it takes, every attempt made while it holds the lock loses, and the
/// clone becomes complete the moment it lands. A repair that counted its
/// attempts refused a clone that was complete a moment later — the exact
/// failure the audit exists to prevent, arrived at from the other direction.
#[test]
fn edge_a_repair_that_lands_after_the_attempts_are_spent_is_accepted() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    // The contending fetch: it holds the lock for longer than the whole
    // attempt window, and only then leaves the clone complete.
    let contention = Duration::from_millis(150);
    assert!(
        contention > fixture_policy().retry_delay * fixture_policy().attempts as u32,
        "premise: the contention must outlast every attempt, or this is the \
         test that already passed"
    );
    let landed = Arc::new(AtomicBool::new(false));
    let started = Instant::now();

    let attempts = Cell::new(0usize);
    let reading = Arc::clone(&landed);
    ensure_history_with(
        &|| {
            if started.elapsed() >= contention {
                reading.store(true, Ordering::SeqCst);
            }
            Ok(!reading.load(Ordering::SeqCst))
        },
        &|| {
            attempts.set(attempts.get() + 1);
            // Every attempt loses the lock: the fetch that holds it is still
            // running, and it is that fetch which will complete the clone.
            Err("fatal: Unable to create '.git/shallow.lock': File exists.".into())
        },
        fixture_policy(),
    )
    .expect("a repair that lands after the attempts are spent must be accepted");

    assert_eq!(
        attempts.get(),
        fixture_policy().attempts,
        "every attempt must have lost the lock, or this test did not measure \
         the contention it is about"
    );
    assert!(
        started.elapsed() >= contention,
        "the repair must have waited for the clone to become complete, not \
         counted its attempts and refused: {:?}",
        started.elapsed()
    );
}

/// The branch the audit reads is `main`, and only `main`.
///
/// A pull-request CI checkout is a detached merge ref with no local `main`, so
/// the audit reads the remote-tracking `origin/main` instead. That is the same
/// branch, not a substitute — and anything else would be the audit measuring
/// whatever ref it happened to be sitting on.
#[test]
fn edge_the_audited_ref_resolves_to_main_and_nothing_else() {
    // Same precondition the audit itself has: a full history, or a refusal.
    ensure_full_history().expect("this clone's history is complete, or was completed");

    let main = resolve_main().expect("this clone has a main to audit");
    assert!(
        main == "refs/heads/main" || main == "refs/remotes/origin/main",
        "the audit must read main, got {main:?}"
    );

    let head = git(&["rev-parse", main.as_str()]).expect("the resolved ref resolves");
    assert_eq!(
        head.trim().len(),
        40,
        "a full sha, not a partial ref: {:?}",
        head.trim()
    );

    // And it really is main's history, not some other branch: the six measured
    // checkpoint commits are reachable from it.
    let log = git(&["log", "--format=%H%x09%s", main.as_str()]).expect("main logs");
    for measured in [
        "7fdea15f13a0cc2021c5cc69a65a126879e2d554",
        "97b4043cd92e7b2b8a7511a6600b8e73ba9d2596",
    ] {
        assert!(
            log.lines().any(|l| l.starts_with(measured)),
            "{measured} must be reachable from the audited ref {main:?}"
        );
    }
}

/// An acknowledgement that points at nothing is a list decaying into a comment.
///
/// This is the assertion `edge_an_abbreviated_sha_does_not_acknowledge_its_commit`
/// proves is computed, made fatal in the audit itself: junk entries parse, so
/// nothing but an assertion stops them accumulating in the checked-in file.
#[test]
fn edge_an_acknowledgement_that_points_at_nothing_is_reported_as_stale() {
    let ack = parse_ack_file(concat!(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa|tolerated: real checkpoint\n",
        "deadbeef|tolerated: typo, or a commit that was rewritten\n",
        "7fdea15|tolerated: abbreviated by mistake\n",
        "wip_merges_on_main=0\n",
    ))
    .expect("junk entries are well-formed data, which is the problem");
    assert_eq!(
        ack.entries.len(),
        3,
        "premise: an abbreviated sha, a typo sha and a real sha all parse"
    );

    let log = concat!(
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\trbops: phase-016 checkpoint 1791156132\n",
        "7fdea15f13a0cc2021c5cc69a65a126879e2d554\trbops: phase-019 checkpoint 1791227938\n",
    );
    let result = audit(log, &ack).expect("a well-formed fixture log");

    assert_eq!(
        result.unacknowledged,
        vec!["7fdea15f13a0cc2021c5cc69a65a126879e2d554".to_string()],
        "the abbreviated entry does not acknowledge its commit, so the commit is \
         reported as unacknowledged AND the entry is reported as stale — neither \
         reading is a pass"
    );
    assert_eq!(
        result.stale_acknowledgements,
        vec!["deadbeef".to_string(), "7fdea15".to_string()],
        "both junk entries are reported, and neither is silently tolerated"
    );
    let summary = describe(&result, &ack);
    assert!(
        summary.contains("STALE") && summary.contains("deadbeef"),
        "the summary must name the stale entries, got: {summary}"
    );
}
