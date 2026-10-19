//! Audit: every commit in this repository's `HEAD` history whose subject marks
//! unreviewed work is on a written acknowledgement list, with a reason.
//!
//! # Why this file exists, and what it corrects
//!
//! `tests/checkpoint_ancestry_test.rs` (phase-078) already audits the
//! `rbops: phase-NNN checkpoint` commits on `main`. It selects that one subject
//! shape. The pipeline also writes `resume phase-NNN: adopt preserved work`,
//! and that shape reached `main` **fifteen** times carrying work the reviewer
//! never saw. Nothing measured it, so the count could only grow silently.
//! That is the gap this file measures.
//!
//! # The supersession phase-083 attempted, and why it is wrong
//!
//! Phase-083 claimed that phase-078's finding was false: that
//! `git log --oneline main | grep -c checkpoint` prints `0` and that none of
//! `7fdea15`, `074c95e`, `563a351`, `286fccd`, `e97983a`, `d664d3c`,
//! `2179034`, `255816e` is an ancestor of `main`.
//!
//! **Both claims are artefacts of a shallow clone.** Every one of them is
//! measured against a history that had not been completed:
//!
//! | measurement | shallow clone | complete history |
//! |---|---|---|
//! | `git log --oneline main \| grep -c checkpoint` | `1` | `7` |
//! | `git merge-base --is-ancestor <any of the eight> HEAD` | false | **true** |
//! | `git log --format=%s main \| grep -ci 'resume phase-'` | `11` | `15` |
//!
//! So phase-078's finding reproduces exactly, on all eight commits, once the
//! history is whole — and phase-083's premise for superseding it was false.
//! [`edge_phase_078_commits_are_ancestors_of_head_not_absent`] pins the
//! measured truth in the only direction that can be true, and
//! [`ensure_full_history`] is why: a shallow clone is completed before
//! anything here is measured, never measured short. A truncated log reads as a
//! clean one, and this is the second time that has cost this repository an
//! audit.
//!
//! # The tolerance
//!
//! Published history is deliberately **not** rewritten. No rebase, no
//! filter-branch, no reset, no amend, no force-push — AGENTS.md §1.6 forbids
//! it, and erasing the evidence would make this audit green by destroying the
//! thing it exists to record. Every entry in [`ACKNOWLEDGED`] says, in its own
//! words, that the commit carries unreviewed code and was deliberately kept.
//!
//! # What makes the list data rather than a comment
//!
//! [`validate_acknowledgements`] rejects an entry with no full lowercase object
//! id, with an empty reason, or duplicated. [`scan`] additionally reports
//! entries that name a commit this history does not contain, so a list of SHAs
//! that satisfy nothing cannot pass by looking thorough.

// Minimal replacement for an invalid placeholder that introduced a syntax error
// TODO: replace with real assertions that exercise the intended behavior.

use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;

/// Commits acknowledged as carrying unreviewed work, each with the reason it is
/// tolerated. Data, not prose: [`validate_acknowledgements`] parses it and
/// [`scan`] consumes it, so a SHA cannot be recorded without saying why.
///
/// Every entry below states two things, because they are the two halves of the
/// tolerance: the commit carries code the reviewer never saw, and it was
/// deliberately left in published history rather than rewritten (AGENTS.md §1.6
/// forbids rewriting it, and erasing the evidence would make this audit green
/// by destroying what it exists to record).
const ACKNOWLEDGED: &[(&str, &str)] = &[
    // --- `resume phase-NNN: adopt preserved work` ------------------------------
    // The pipeline's resume path. Every one of these landed on HEAD carrying work
    // that no reviewer saw. Line counts are `git show --numstat`, so they can be
    // re-derived rather than trusted.
    ("25e9b68d7529459ac0638f208d3e153681f80e32", "resume phase-020: adopt preserved work. Carries unreviewed code (+333/-0, 1 file, all outside src/). Deliberately not rewritten: published history is immutable under AGENTS.md 1.6."),
    ("11b9fe2fc7d790068459f9554affa26f15af6a45", "resume phase-021: adopt preserved work. Carries unreviewed code (+3353/-1, 5 files, +223 lines under src/). Deliberately not rewritten; kept so HEAD's ancestry stays intact."),
    ("4c45731b08b8f347beac5f46506456065aebc33e", "resume phase-021: adopt preserved work, the second of two. Carries unreviewed code (+690/-173, 6 files, +48 lines under src/). Deliberately not rewritten; a list keyed on phase number would silently tolerate this one, so it is keyed on the SHA."),
    ("dff04f67c8f2ed0b7bab3039ceb38df384fd739d", "resume phase-022: adopt preserved work. Carries unreviewed code (+1158/-313, 13 files, +245 lines under src/). Deliberately not rewritten."),
    ("861080d981d668aefa02f9d82641b2a8058fe6f6", "resume phase-025: adopt preserved work. Carries unreviewed code (+8797/-237, 18 files, +1242 lines under src/ - the largest src/ change on this list). Deliberately not rewritten."),
    ("825193a4a06afdc22a8d73644961defa33d4d630", "resume phase-031: adopt preserved work. Carries unreviewed code (+1199/-58, 14 files, +592 lines under src/). Deliberately not rewritten."),
    ("2804ab87720c5c54811429664682e2f3f5d761f3", "resume phase-033: adopt preserved work. Carries unreviewed code (+4426/-243, 11 files, +3517 lines under src/ - over half the list's src/ churn in one commit). Deliberately not rewritten."),
    ("89104562f7a2f5390906f565e4c6124c7eb09d4b", "resume phase-037: adopt preserved work. Carries unreviewed code (+1608/-29, 10 files, +247 lines under src/). Deliberately not rewritten."),
    ("b6d633b79417a55dce37869f7aef19d3171f904c", "resume phase-038: adopt preserved work. Carries unreviewed code (+942/-13, 15 files, +333 lines under src/). Deliberately not rewritten."),
    ("1ee5aba2f2474d2e929710d3c4acd0c7855f8432", "resume phase-039: adopt preserved work. Carries unreviewed code (+887/-98, 5 files, +227 lines under src/). Deliberately not rewritten."),
    ("96c3c17955d3b5233806c311110646ebe088fbd7", "resume phase-044: adopt preserved work. Carries unreviewed code (+1500/-18, 7 files, +728 lines under src/). Deliberately not rewritten."),
    ("985e4456ce8d526d292e5157a449542bcdba5259", "resume phase-046: adopt preserved work. Carries unreviewed code (+1274/-26, 11 files, +370 lines under src/). Deliberately not rewritten."),
    ("a72143420f1da24667137b3f809bece989f274d2", "resume phase-086: adopt preserved work. Carries unreviewed code (+664/-27, 7 files, +79 lines under src/). Deliberately not rewritten."),
    // The two below are invisible to a shallow `git log`, which is how phase-083
    // undercounted this shape as thirteen. Recorded so the count cannot drift.
    ("e26659d49097210a2d1ab8abecc97d61698afebe", "resume phase-048: adopt preserved work. Carries unreviewed code (+1223/-16, 8 files, +120 lines under src/). Deliberately not rewritten. Visible only on a complete history: a shallow `git log main` does not reach it."),
    ("09dd1020443e166a765991815c75c024a5ccd45b", "resume phase-049: adopt preserved work. Carries unreviewed code (+1437/-173, 35 files, +263 lines under src/) - the widest file count on this list. Deliberately not rewritten."),
    ("10f97506426673e83119b6e0ec21f6754bddf75c", "resume phase-050: adopt preserved work. Carries unreviewed code (+5410/-126, 15 files, +596 lines under src/). Deliberately not rewritten. Visible only on a complete history: a shallow `git log main` does not reach it."),
    ("7e75dc15c77a5882f086990d261a9049de9d48bb", "resume phase-054: adopt preserved work. Carries unreviewed code (+1131/-8, 6 files, +923/-8 under src/ - the REPL syntax highlighter: src/repl/highlight.rs, src/repl/style.rs, src/repl/editor.rs, src/repl/mod.rs, plus this phase's own REPORT.md and FINDINGS.md). Deliberately not rewritten."),
    ("1a7df335b8b3f656f599d5ff22856918b63858b7", "resume phase-056: adopt preserved work. Carries unreviewed code (+3776/-16, 8 files, +3520 lines under src/ - the REPL code-introspection commands `.type`/`.doc`/`.which`/`.sighelp`: src/repl/introspect.rs is new, plus src/repl/commands.rs, src/repl/mod.rs, src/stdlib.rs and src/interpreter.rs, along with this phase's tests and REPORT.md). Deliberately not rewritten; kept so HEAD's ancestry stays intact. The abandoned attempt it replaces (617e4667) is not an ancestor of HEAD, so it is not listed: `scan` reports an acknowledgement naming a commit HEAD does not contain as stale, and a stale entry is a failure."),
    ("977218301bd20726a43a15d7fa64344939ee097f", "resume phase-058: adopt preserved work. Carries unreviewed code (+612/-30, 6 files, +507 lines under src/ - the REPL compiler-view commands `.bytecode`/`.asm`: src/repl/bytecode_view.rs is new, plus the compact disassembler `disasm::render_compact` exported from src/bytecode/mod.rs, the two command rows in src/repl/commands.rs, the dispatch and printers in src/repl/mod.rs, and this phase's tests). Deliberately not rewritten; kept so HEAD's ancestry stays intact."),
    ("96beaa93fb2193239f4e8409189720002b3e94bc", "resume phase-087: adopt WIP checkpoint. Carries unreviewed code (+546/-104, 23 files, +30/-28 under src/ - `length` over text counts characters instead of UTF-8 bytes at src/runtime.rs, and the unreachable second `length` arm is deleted from src/stdlib.rs, along with the corpus and test expectations that recorded the byte count). Deliberately not rewritten; kept so HEAD's ancestry stays intact."),
    ("bf9ffc3370e97436d193f146102e8c4d69d1fade", "resume phase-089: adopt WIP checkpoint. Carries unreviewed code (+1068/-15, 11 files, +381/-15 under src/ - the typed failure value and `catch error of <Type>`: src/failure.rs is new (`FailureKind`, the closed list, `value_of`), plus the `of <Type>` parse and `catch_kind` in src/parser.rs, the decline path and the bound failure record in both src/interpreter.rs and src/bytecode/vm.rs, the kind in the catch block's parameter list in src/bytecode/codegen.rs, and `catch error of <Kind> <name>` in src/formatter.rs). Deliberately not rewritten; kept so HEAD's ancestry stays intact."),
    ("b48189aa679e0f4df60f5cd5d10a4894d77bc3f7", "resume phase-059: adopt preserved work. Carries unreviewed code (+1321/-85, 22 files, +897/-61 under src/ - REPL failures are now drawn through `Error::render` against the session buffer: `render_error`, `paint_diagnostic` and `is_caret_line` in src/repl/mod.rs, the cell-accurate drawing (`char_cells`, `display_width`, `render_single`, `render_range`) and its two bounds (`MAX_DRAWN_CELLS`, `MAX_DRAWN_LINES`) in src/error.rs, `Expr::Variable(String, Span)` in src/parser.rs and `name_position` in src/analyzer.rs so the caret lands on the name rather than the statement, plus 16 tests and this phase's REPORT.md and FINDINGS.md). Deliberately not rewritten; kept so HEAD's ancestry stays intact. The earlier phase-059 resume it replaces (8389c5a) is not an ancestor of HEAD, so it is not listed: `scan` reports an acknowledgement naming a commit HEAD does not contain as stale, and a stale entry is a failure."),
    ("5b732290fea7a65ae5b792d8afcbed9115318de0", "resume phase-060: adopt preserved work. Carries unreviewed code (+3476/-18, 10 files, +1940/-17 under src/ - the session export `.export` and TAB path completion, plus the file loop that landed on top of them: src/repl/transcript.rs is new (the export writer `markdown` and its Markdown reader `read_markdown`), the path completer `path_argument`/`complete_path_in` and `ReplCompleter::complete_path` in src/repl/completer.rs, the TAB handling in src/repl/editor.rs, the escape-stripping palette in src/repl/style.rs, and `.reload`/`.format`/`format_on_enter`/`last_file` in src/repl/commands.rs and src/repl/mod.rs, plus this phase's tests and REPORT.md/FINDINGS.md). Deliberately not rewritten; kept so HEAD's ancestry stays intact. The earlier phase-060 resume it replaces (420f7b7) is not an ancestor of HEAD, so it is not listed: `scan` reports an acknowledgement naming a commit HEAD does not contain as stale, and a stale entry is a failure."),
    ("c95062eb02a6e336f011a918daac7938f0efee6d", "ack: resume phase-088: add resume commit to ACKNOWLEDGED. Carries no production code: +1/-927 across 2 files, neither under src/ (phase-088's acknowledgement data and its bookkeeping). Acknowledged because the selector is a case-insensitive substring test on `resume phase-` and the subject merely *contains* the phrase; deliberately not rewritten."),
    // --- `rbops: phase-NNN checkpoint` ---------------------------------------
    // Already audited, with reasons, by tests/checkpoint_ancestry_test.rs against
    // tests/checkpoint_ancestry_ack.txt. Re-listed here because this selector is a
    // superset of that one; the two lists must not disagree about what is on HEAD.
    ("7fdea15f13a0cc2021c5cc69a65a126879e2d554", "rbops: phase-019 checkpoint. Carries unreviewed code, published via a resume merge the reviewer never saw. Already acknowledged with the same reason in tests/checkpoint_ancestry_ack.txt. Deliberately not rewritten."),
    ("074c95e24fbf8b1101219bc1f10c520d8b7acd24", "rbops: phase-019 checkpoint, the earlier of the two. Carries unreviewed code. Already acknowledged in tests/checkpoint_ancestry_ack.txt. Deliberately not rewritten."),
    ("563a3513c7fba2d2a39e759632415fef4bb60eb5", "rbops: phase-018 checkpoint. Carries unreviewed code, published via a resume merge the reviewer never saw. Already acknowledged in tests/checkpoint_ancestry_ack.txt. Deliberately not rewritten."),
    ("286fccd537d963cb9ad1bcbb051bce45d537a683", "rbops: phase-016 checkpoint. Carries unreviewed code, published via a resume merge the reviewer never saw. Already acknowledged in tests/checkpoint_ancestry_ack.txt. Deliberately not rewritten."),
    ("32d52ad3d50859c97db57dd10542c653f9962ea4", "rbops: phase-004 checkpoint, the oldest on HEAD. Carries unreviewed code. Already acknowledged in tests/checkpoint_ancestry_ack.txt. Deliberately not rewritten."),
    ("97b4043cd92e7b2b8a7511a6600b8e73ba9d2596", "rbops: phase-004 checkpoint, the earlier of the two. Carries unreviewed code. Already acknowledged in tests/checkpoint_ancestry_ack.txt. Deliberately not rewritten."),
    // --- A subject that merely contains the word ------------------------------
    // Selected because the selector is a substring test on purpose. This one
    // carries no unreviewed code - it only *says* `checkpoint` - and it is listed
    // anyway rather than special-cased, so the rule stays one rule.
    ("3fbf7396de1d2355267ddcfda9a40a00277a4687", "rbops: phase-078 audit the checkpoint ancestry of main instead of commenting on it. Carries no unreviewed code: +654/-0 across 2 files, both under tests/ (the phase-078 audit and its acknowledgement data). Acknowledged because the selector is a case-insensitive substring test on `checkpoint`; deliberately not rewritten."),
];

/// The commits measured on a complete history of `HEAD`, and the shape each one
/// was found under.
///
/// Every SHA here is required to be present in the log this file reads. That is
/// the half of the promise the other half does not make on its own: a list can
/// satisfy itself by naming commits that no longer exist.
const MEASURED: &[(&str, &str)] = &[
    (
        "25e9b68d7529459ac0638f208d3e153681f80e32",
        "resume phase-020",
    ),
    (
        "11b9fe2fc7d790068459f9554affa26f15af6a45",
        "resume phase-021",
    ),
    (
        "4c45731b08b8f347beac5f46506456065aebc33e",
        "resume phase-021, second",
    ),
    (
        "dff04f67c8f2ed0b7bab3039ceb38df384fd739d",
        "resume phase-022",
    ),
    (
        "861080d981d668aefa02f9d82641b2a8058fe6f6",
        "resume phase-025",
    ),
    (
        "825193a4a06afdc22a8d73644961defa33d4d630",
        "resume phase-031",
    ),
    (
        "2804ab87720c5c54811429664682e2f3f5d761f3",
        "resume phase-033",
    ),
    (
        "89104562f7a2f5390906f565e4c6124c7eb09d4b",
        "resume phase-037",
    ),
    (
        "b6d633b79417a55dce37869f7aef19d3171f904c",
        "resume phase-038",
    ),
    (
        "1ee5aba2f2474d2e929710d3c4acd0c7855f8432",
        "resume phase-039",
    ),
    (
        "96c3c17955d3b5233806c311110646ebe088fbd7",
        "resume phase-044",
    ),
    (
        "985e4456ce8d526d292e5157a449542bcdba5259",
        "resume phase-046",
    ),
    (
        "e26659d49097210a2d1ab8abecc97d61698afebe",
        "resume phase-048",
    ),
    (
        "09dd1020443e166a765991815c75c024a5ccd45b",
        "resume phase-049",
    ),
    (
        "10f97506426673e83119b6e0ec21f6754bddf75c",
        "resume phase-050",
    ),
    (
        "7e75dc15c77a5882f086990d261a9049de9d48bb",
        "resume phase-054",
    ),
    (
        "1a7df335b8b3f656f599d5ff22856918b63858b7",
        "resume phase-056",
    ),
    (
        "3fbf7396de1d2355267ddcfda9a40a00277a4687",
        "phase-078 audit, subject contains `checkpoint`",
    ),
    (
        "7fdea15f13a0cc2021c5cc69a65a126879e2d554",
        "rbops: phase-019 checkpoint",
    ),
    (
        "074c95e24fbf8b1101219bc1f10c520d8b7acd24",
        "rbops: phase-019 checkpoint, earlier",
    ),
    (
        "563a3513c7fba2d2a39e759632415fef4bb60eb5",
        "rbops: phase-018 checkpoint",
    ),
    (
        "286fccd537d963cb9ad1bcbb051bce45d537a683",
        "rbops: phase-016 checkpoint",
    ),
    (
        "32d52ad3d50859c97db57dd10542c653f9962ea4",
        "rbops: phase-004 checkpoint",
    ),
    (
        "97b4043cd92e7b2b8a7511a6600b8e73ba9d2596",
        "rbops: phase-004 checkpoint, earlier",
    ),
    (
        "c95062eb02a6e336f011a918daac7938f0efee6d",
        "ack: resume phase-088",
    ),
];

/// Everything one pass over a log learns.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Scan {
    /// Unreviewed-subject commits with no acknowledgement entry. Must be empty.
    unreviewed: Vec<String>,
    /// Acknowledged SHAs this log does not contain. Must be empty.
    stale: Vec<String>,
    /// How many commits the selector selected, acknowledged or not.
    selected: usize,
}

/// Repository root, so the audit reads this checkout and not the cwd.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Run `git` in the repository and return stdout, or a readable reason.
///
/// A failed invocation is never turned into "nothing found": that is the false
/// pass this whole file is built to avoid.
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

/// Is this clone shallow? `git rev-parse --is-shallow-repository` answers exactly
/// `true` or `false`; anything else is refused rather than guessed at.
fn is_shallow() -> Result<bool, String> {
    match git(&["rev-parse", "--is-shallow-repository"])?.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!(
            "`git rev-parse --is-shallow-repository` said {other:?}, which is neither \
             `true` nor `false`; refusing to guess whether this history is complete"
        )),
    }
}

/// Complete a shallow clone before anything is measured.
///
/// This is the repair that makes phase-083's stale finding impossible to
/// re-derive: it is the difference between the `1` and the `7` in the table in
/// the module docs. It is a no-op on a complete clone, and it is the same
/// single repair `tests/checkpoint_ancestry_test.rs` performs. If it does not
/// take, this is an error — never a short log reported as a clean one.
fn ensure_full_history() -> Result<(), String> {
    if !is_shallow()? {
        return Ok(());
    }
    // A concurrent completion by another test binary fails this on
    // `.git/shallow.lock`; that is not an error here, because the re-read below
    // is what decides.
    let _ = git(&[
        "fetch",
        "--unshallow",
        "--quiet",
        "origin",
        "+refs/heads/*:refs/remotes/origin/*",
    ]);
    if is_shallow()? {
        return Err(
            "history is shallow and completing the clone did not make it complete: the \
             audit refuses to read a truncated `git log HEAD`, because a truncated log \
             reads as a clean one. Run `git fetch --unshallow` first."
                .into(),
        );
    }
    Ok(())
}
/// The full `HEAD` log in `<sha>\t<subject>` form, or the reason it is unreadable.
fn read_head_log() -> Result<String, String> {
    if !repo_root().join(".git").exists() {
        return Err("no .git here: this is not a repository, so there is no HEAD".into());
    }
    git(&["log", "--format=%H%x09%s", "HEAD"])
}

/// Refuse a history that is short, whatever the log says.
///
/// Split out from [`read_head_log`] so the refusal can be driven from fixtures
/// instead of requiring a second, deliberately-shallow clone to exist. The depth
/// is a parameter and the log is a parameter, so both halves of the claim — a
/// truncated log audits clean, and this refuses it — are assertable offline.
fn check_history(is_shallow: bool, log: Result<String, String>) -> Result<String, String> {
    if is_shallow {
        return Err(
            "history is shallow: `git log HEAD` returns a truncated log, and a truncated \
             log reads as a clean one. Run `git fetch --unshallow` before auditing."
                .into(),
        );
    }
    log
}

/// Is this a full lowercase object id? 40 hex digits (SHA-1) or 64 (SHA-256).
///
/// Lowercase only: `git log --format=%H` emits lowercase, so an uppercase
/// entry could never match a real commit and would silently satisfy nothing.
fn is_object_id(s: &str) -> bool {
    matches!(s.len(), 40 | 64)
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// Split one `<sha>\t<subject>` log line.
///
/// Exactly two tab-separated fields are required. A third field means the line
/// is not what `--format=%H%x09%s` produces, and reading the subject as
/// everything after the *first* tab would silently absorb the extra field into
/// the subject — which is precisely how a subject could be made to stop looking
/// like unreviewed work.
fn parse_log_line(line: &str, lineno: usize) -> Result<(&str, &str), String> {
    if line.trim().is_empty() {
        return Err(format!("log line {lineno} is blank"));
    }
    let fields: Vec<&str> = line.split('\t').collect();
    if fields.len() != 2 {
        return Err(format!(
            "log line {lineno} has {} tab-separated field(s), expected exactly 2 \
             (`<sha>\\t<subject>`): {line:?}",
            fields.len()
        ));
    }
    let (sha, subject) = (fields[0], fields[1]);
    if !is_object_id(sha) {
        return Err(format!(
            "log line {lineno}: {sha:?} is not a full lowercase object id"
        ));
    }
    if subject.is_empty() {
        return Err(format!("log line {lineno} has an empty subject"));
    }
    Ok((sha, subject))
}

/// Does this subject mark unreviewed work?
///
/// Deliberately a substring test, case-insensitive, over both shapes the
/// pipeline writes: `rbops: phase-NNN checkpoint` and
/// `resume phase-NNN: adopt preserved work`. An exact-prefix test would let a
/// commit through by adding a word in front, which is the whole failure mode.
fn is_unreviewed_subject(subject: &str) -> bool {
    let lower = subject.to_lowercase();
    lower.contains("checkpoint") || lower.contains("resume phase-")
}

/// Reject an acknowledgement list that is not data.
fn validate_acknowledgements(acknowledged: &[(&str, &str)]) -> Result<(), String> {
    let mut seen: Vec<&str> = Vec::new();
    for (sha, reason) in acknowledged {
        if !is_object_id(sha) {
            return Err(format!(
                "acknowledgement {sha:?} is not a full lowercase object id"
            ));
        }
        if reason.trim().is_empty() {
            return Err(format!("acknowledgement {sha} records no reason"));
        }
        if seen.contains(sha) {
            return Err(format!("acknowledgement {sha} is listed twice"));
        }
        seen.push(sha);
    }
    Ok(())
}

/// Select every unreviewed-work commit in `log` and report the ones with no
/// acknowledgement, plus the entries that point at nothing.
fn scan(log: &str, acknowledged: &[(&str, &str)]) -> Result<Scan, String> {
    validate_acknowledgements(acknowledged)?;

    let mut selected_shas: Vec<&str> = Vec::new();
    let mut selected = 0usize;
    for (idx, raw) in log.lines().enumerate() {
        let (sha, subject) = parse_log_line(raw, idx + 1)?;
        if !is_unreviewed_subject(subject) {
            continue;
        }
        selected += 1;
        if !selected_shas.contains(&sha) {
            selected_shas.push(sha);
        }
    }

    let unreviewed = selected_shas
        .iter()
        .filter(|sha| !acknowledged.iter().any(|(entry, _)| *entry == **sha))
        .map(|sha| {
            let subject = log
                .lines()
                .find_map(|line| {
                    parse_log_line(line, 1)
                        .ok()
                        .filter(|(s, _)| s == sha)
                        .map(|(_, sub)| sub)
                })
                .unwrap_or("<subject unread>");
            format!("{sha}\t{subject}")
        })
        .collect();
    let stale = acknowledged
        .iter()
        .map(|(sha, _)| *sha)
        .filter(|sha| !selected_shas.contains(sha))
        .map(str::to_string)
        .collect();

    Ok(Scan {
        unreviewed,
        stale,
        selected,
    })
}

/// A one-line human summary, so `cargo test -- --nocapture` shows the numbers.
fn describe(scan: &Scan, acknowledged: &[(&str, &str)]) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        "unreviewed-work commits selected: {} | acknowledged: {} | unacknowledged: {} | \
         stale entries: {}",
        scan.selected,
        acknowledged.len(),
        scan.unreviewed.len(),
        scan.stale.len()
    );
    if !scan.unreviewed.is_empty() {
        let _ = write!(s, " | UNACKNOWLEDGED: {}", scan.unreviewed.join(" | "));
    }
    if !scan.stale.is_empty() {
        let _ = write!(s, " | STALE: {}", scan.stale.join(", "));
    }
    s
}

/// The audit, against this repository's `HEAD`.
fn audit_head() -> Result<(Scan, String), String> {
    ensure_full_history()?;
    let log = check_history(is_shallow()?, read_head_log())?;
    let scan = scan(&log, ACKNOWLEDGED)?;
    Ok((scan, log))
}

// ---------------------------------------------------------------------------
// The audit, against this repository's HEAD.
// ---------------------------------------------------------------------------

/// Every unreviewed-work commit in HEAD's ancestry is acknowledged, with a
/// reason, and the list names nothing this history does not contain.
#[test]
fn unreviewed_work_in_head_history_is_acknowledged() {
    let (scan, log) = audit_head().expect("HEAD's history must be readable and acknowledged");
    println!("{}", describe(&scan, ACKNOWLEDGED));

    assert!(
        scan.unreviewed.is_empty(),
        "{} commit(s) in HEAD's history carry unreviewed work and are not on the \
         acknowledgement list in this file: {}. Add each one with a reason saying what \
         it carries and that it was deliberately not rewritten.",
        scan.unreviewed.len(),
        scan.unreviewed.join(" | ")
    );

    assert!(
        scan.stale.is_empty(),
        "{} entry/entries name a commit HEAD's history does not contain: {}. An entry \
         pointing at nothing is how the list decays into a comment — a typo, an \
         abbreviated sha, or a rewrite that AGENTS.md forbids would all parse and \
         satisfy nothing.",
        scan.stale.len(),
        scan.stale.join(", ")
    );

    for (sha, subject) in MEASURED {
        let sha: &str = sha;
        assert!(
            log.lines()
                .any(|line| line.starts_with(&format!("{sha}\t"))),
            "{sha} ({subject}) was measured on this history but `git log HEAD` does not \
             report it; re-measure before trusting this file"
        );
        let entry = ACKNOWLEDGED
            .iter()
            .find(|entry| entry.0 == sha)
            .unwrap_or_else(|| panic!("{sha} is in HEAD's history but is not acknowledged"));
        assert!(
            !entry.1.trim().is_empty(),
            "{sha} is acknowledged without saying why"
        );
    }
}

/// Phase-078's eight commits are **ancestors** of HEAD, not absent.
///
/// Phase-083's evidence asserted the opposite and proposed superseding
/// phase-078 on that basis. The assertion was measured on a shallow clone, where
/// `git log` cannot see far enough back and `merge-base --is-ancestor` answers
/// `false` for a commit it has not been given. On a complete history all eight
/// are ancestors, so phase-078's finding stands and this file extends it rather
/// than replacing it. Asserting the truth here is what stops the stale claim
/// from being re-derived a third time.
#[test]
fn edge_phase_078_commits_are_ancestors_of_head_not_absent() {
    ensure_full_history().expect("this history must be complete before it is measured");

    let eight = [
        (
            "7fdea15f13a0cc2021c5cc69a65a126879e2d554",
            "rbops: phase-019 checkpoint",
        ),
        (
            "074c95e24fbf8b1101219bc1f10c520d8b7acd24",
            "rbops: phase-019 checkpoint",
        ),
        (
            "563a3513c7fba2d2a39e759632415fef4bb60eb5",
            "rbops: phase-018 checkpoint",
        ),
        (
            "286fccd537d963cb9ad1bcbb051bce45d537a683",
            "rbops: phase-016 checkpoint",
        ),
        ("e97983a", "resume-path merge"),
        ("d664d3c", "resume-path merge"),
        ("2179034", "resume-path merge"),
        ("255816e", "resume-path merge"),
    ];

    for (sha, what) in eight {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo_root())
            .args(["merge-base", "--is-ancestor", sha, "HEAD"])
            .output()
            .expect("`git merge-base` must run");
        assert!(
            out.status.success(),
            "{sha} ({what}) is NOT an ancestor of HEAD on a complete history. Phase-083 \
             claimed this for all eight of phase-078's commits and wanted to supersede \
             phase-078 on it; if this has genuinely become true the history was rewritten, \
             which AGENTS.md forbids — find out what happened before updating this test."
        );
    }
}

// ---------------------------------------------------------------------------
// The selector and the refusals, on fixtures.
// ---------------------------------------------------------------------------

const SHA_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHA_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SHA_C: &str = "cccccccccccccccccccccccccccccccccccccccc";

/// An `edge_*` case: a log with no unreviewed commits at all.
///
/// An empty log and a one-reviewed-commit log both select nothing. Every
/// acknowledgement the file carries is stale against a log that contains none of
/// them — which is the point of pairing the two halves: a list cannot be shown
/// to be doing work by a log that has none.
#[test]
fn edge_empty_log_selects_nothing() {
    let result = scan("", ACKNOWLEDGED).expect("an empty log is well formed");
    assert_eq!(result.selected, 0);
    assert_eq!(result.unreviewed, Vec::<String>::new());
    assert_eq!(result.stale.len(), ACKNOWLEDGED.len());
    assert!(
        result
            .stale
            .iter()
            .all(|sha| ACKNOWLEDGED.iter().any(|(entry, _)| entry == sha)),
        "against an empty log every entry is stale, and none of them is anything else"
    );

    // Singleton: one reviewed commit and nothing else.
    let one = format!("{SHA_A}\tci: add cargo fmt --check\n");
    let result = scan(&one, ACKNOWLEDGED).expect("a one-commit log is well formed");
    assert_eq!(result.selected, 0);
    assert!(result.unreviewed.is_empty());
    assert_eq!(result.stale.len(), ACKNOWLEDGED.len());

    // Boundary: the last line of a log with no trailing newline is still read.
    let no_newline = format!("{SHA_A}\tci: add cargo fmt --check");
    assert_eq!(
        scan(&no_newline, ACKNOWLEDGED)
            .expect("a log without a trailing newline is well formed")
            .selected,
        0
    );
    let no_newline_unreviewed = SHA_C.to_string() + "\tresume phase-083: adopt preserved work";
    assert_eq!(
        scan(&no_newline_unreviewed, ACKNOWLEDGED)
            .expect("a log without a trailing newline is well formed")
            .selected,
        1,
        "the final line must not be dropped for want of a newline"
    );
}

/// An `edge_*` case: the only unreviewed commit is acknowledged.
#[test]
fn edge_a_fully_acknowledged_log_is_clean() {
    let ack = [(
        SHA_A,
        "resume phase-020: carries unreviewed code, deliberately not rewritten",
    )];
    let log = format!(
        "{SHA_A}\tresume phase-020: adopt preserved work\n\
         {SHA_B}\tci: add cargo fmt --check\n\
         {SHA_C}\trbops: phase-016 checkpoint 1791156132\n"
    );
    let result = scan(&log, &ack).expect("a well-formed fixture log");

    assert_eq!(result.selected, 2, "both unreviewed subjects are selected");
    assert_eq!(
        result.unreviewed,
        vec![format!("{SHA_C}\trbops: phase-016 checkpoint 1791156132")],
        "the checkpoint commit has no entry, so it is the one reported"
    );
    assert_eq!(result.stale, Vec::<String>::new());
}

/// An `edge_*` case: exactly one unacknowledged commit is reported by name.
#[test]
fn edge_exactly_one_unacknowledged_commit_is_reported() {
    let ack = [(
        SHA_A,
        "acknowledged, carries unreviewed code, deliberately not rewritten",
    )];
    let log = format!(
        "{SHA_A}\tresume phase-021: adopt preserved work\n\
         {SHA_B}\tresume phase-022: adopt preserved work\n\
         {SHA_C}\tci: add cargo fmt --check\n"
    );
    let result = scan(&log, &ack).expect("a well-formed fixture log");

    assert_eq!(result.selected, 2);
    assert_eq!(
        result.unreviewed,
        vec![format!("{SHA_B}\tresume phase-022: adopt preserved work")],
        "exactly the one unacknowledged commit, with its subject, nothing more"
    );
    assert!(
        describe(&result, &ack).contains(SHA_B),
        "the summary must name the offending sha so the failure is actionable"
    );
}

/// An `edge_*` case: a subject that merely *contains* `checkpoint` inside a
/// longer phrase is still unreviewed work.
///
/// An exact-prefix selector would wave this through, and phase-078's own audit
/// commit — `rbops: phase-078 audit the checkpoint ancestry of main instead of
/// commenting on it` — is a real one in this history.
#[test]
fn edge_a_longer_phrase_containing_checkpoint_is_still_selected() {
    assert!(is_unreviewed_subject(
        "rbops: phase-078 audit the checkpoint ancestry of main instead of commenting on it"
    ));
    assert!(is_unreviewed_subject(
        "ci: allow the checkpoint job to finish before merge"
    ));
    assert!(is_unreviewed_subject(
        "RESUME PHASE-083: adopt preserved work"
    ));
    assert!(is_unreviewed_subject(
        "resume phase-083: adopt preserved work"
    ));

    for no in [
        "ci: add cargo fmt --check",
        "rbops: phase-081 (2026-10-14T09:16:08Z)",
        "resume phase: adopt preserved work",
        "resumephase-083: adopt preserved work",
        "",
    ] {
        assert!(!is_unreviewed_subject(no), "should not select: {no:?}");
    }
}

/// An `edge_*` case: a two-tab-delimited line is malformed, not a commit.
///
/// `split_once` would read it as `<sha>` plus a subject with a tab in it, and
/// `--format=%H%x09%s` never produces that. It is rejected so a subject cannot
/// be laundered out of the selector's view.
#[test]
fn edge_a_two_tab_delimited_log_line_is_malformed() {
    let log = format!("{SHA_A}\tresume phase-083: adopt preserved work\textra\n");
    let err = scan(&log, ACKNOWLEDGED).expect_err("a three-field line is not a log line");
    assert!(err.contains("log line 1"), "got: {err}");
    assert!(err.contains("3 tab-separated field"), "got: {err}");

    // The neighbouring well-formed forms still parse.
    assert!(parse_log_line(&format!("{SHA_A}\tsubject"), 1).is_ok());
    assert!(parse_log_line(&format!("{SHA_A}\tsubject with \"quotes\" and |pipes|"), 1).is_ok());
}

/// An `edge_*` case: blank, tab-only, truncated and non-hex lines are refused
/// rather than skipped.
#[test]
fn edge_malformed_log_lines_are_refused_rather_than_skipped() {
    // An empty log is a log with zero commits, not a malformed one. It is
    // distinct from a blank line *between* commits, which is refused.
    assert!(
        scan("", ACKNOWLEDGED)
            .expect("an empty log is well formed")
            .selected
            == 0
    );

    for (line, needle) in [
        ("   \n", "blank"),
        (
            &format!("{SHA_A}\tci: ok\n   \n{SHA_B}\tci: ok\n"),
            "log line 2 is blank",
        ),
        ("\tno-sha-here", "not a full lowercase object id"),
        (&format!("{SHA_A}no-tab"), "expected exactly 2"),
        (
            "7fdea15f13a0cc2021c5cc69a65a126879e2d55\tsubject",
            "not a full lowercase object id",
        ),
        (
            "7FDEA15F13A0CC2021C5CC69A65A126879E2D554\tsubject",
            "not a full lowercase object id",
        ),
        (
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz\tsubject",
            "not a full lowercase object id",
        ),
        (&format!("{SHA_A}\t"), "empty subject"),
    ] {
        let err =
            scan(line, ACKNOWLEDGED).expect_err("a malformed line must be refused, not skipped");
        assert!(
            err.contains(needle),
            "for {line:?} expected an error mentioning {needle:?}, got: {err}"
        );
    }
}

/// Unicode, escapes and control characters in a subject.
#[test]
fn edge_unicode_and_escaped_subjects_are_handled_deterministically() {
    // Non-ASCII in a subject, including RTL and combining marks, changes nothing
    // about whether it is selected.
    for subject in [
        "resume phase-083: adopt preserved work \u{1F680}",
        "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627} resume phase-083",
        "e\u{0301}l\u{0301}ve checkpoint",
        "resume phase-083: adopt \u{0631}\"quotes\" and \\\\backslash",
    ] {
        let log = format!("{SHA_A}\t{subject}\n");
        let result = scan(&log, ACKNOWLEDGED).expect("a well-formed fixture log");
        assert_eq!(
            result.selected, 1,
            "should select exactly once and not panic: {subject:?}"
        );
        assert_eq!(result.unreviewed.len(), 1, "subject: {subject:?}");
        assert!(
            result.unreviewed[0].contains(subject),
            "the subject must survive the round trip: {subject:?}"
        );
    }

    // Case folding does not have to be ASCII-only to be right here, and must not
    // be locale-dependent when it is not.
    assert!(is_unreviewed_subject("CHECKPOINT"));
    assert!(is_unreviewed_subject("ReSuMe PhAsE-083"));
    assert!(!is_unreviewed_subject("check point"));
}

/// The acknowledgement list is data: ids, reasons and uniqueness.
#[test]
fn edge_an_acknowledgement_list_must_be_data() {
    assert!(
        validate_acknowledgements(&[(SHA_A, "carries unreviewed code, not rewritten")]).is_ok(),
        "premise: a full lowercase sha with a reason is accepted"
    );

    for (entry, needle) in [
        (("7fdea15", "reason"), "not a full lowercase object id"),
        (
            (SHA_A.to_uppercase().as_str(), "reason"),
            "not a full lowercase object id",
        ),
        ((SHA_A, "   "), "records no reason"),
        ((SHA_A, ""), "records no reason"),
    ] {
        let err = validate_acknowledgements(&[entry])
            .expect_err("an acknowledgement that is not data must be refused");
        assert!(
            err.contains(needle),
            "for {entry:?} expected {needle:?}, got: {err}"
        );
    }

    let duplicated = validate_acknowledgements(&[(SHA_A, "first"), (SHA_A, "second")])
        .expect_err("the same commit twice is ambiguous");
    assert!(duplicated.contains("twice"), "got: {duplicated}");
}

/// An acknowledgement that names a commit the log does not contain is reported.
#[test]
fn edge_an_acknowledgement_that_points_at_nothing_is_stale() {
    let ack = [
        (SHA_A, "carries unreviewed code, deliberately not rewritten"),
        (SHA_C, "carries unreviewed code, deliberately not rewritten"),
    ];
    let log = format!("{SHA_A}\tresume phase-083: adopt preserved work\n");
    let result =
        scan(&log, &ack).expect("well-formed data pointing at the wrong commit is the problem");

    assert!(
        result.unreviewed.is_empty(),
        "premise: every commit here is acknowledged"
    );
    assert_eq!(
        result.stale,
        vec![SHA_C.to_string()],
        "an entry naming a commit this history does not contain is stale, not harmless"
    );
}

/// A shallow history is refused, never audited short.
#[test]
fn edge_a_shallow_history_is_refused_rather_than_audited_clean() {
    // The depth-1 log: one commit, no unreviewed work, and a selector that
    // reports a spotless history. This is the false pass the refusal exists for.
    let truncated = format!("{SHA_A}\tci: add cargo fmt --check\n");
    let laundered = scan(&truncated, ACKNOWLEDGED).expect("a short log still parses");
    assert!(
        laundered.unreviewed.is_empty() && laundered.selected == 0,
        "premise: measured as-is, a depth-1 log audits as a clean history"
    );

    let err = check_history(true, Ok(truncated))
        .expect_err("a shallow history must be refused, not audited");
    assert!(
        err.contains("shallow"),
        "the refusal must say why, got: {err}"
    );
    assert!(
        err.contains("truncated"),
        "and what refusing protects against, got: {err}"
    );

    // A complete history is passed through untouched.
    let complete = format!("{SHA_A}\tci: add cargo fmt --check\n");
    let passed =
        check_history(false, Ok(complete.clone())).expect("a complete history is auditable");
    assert_eq!(passed, complete);

    // And an unreadable history is still an error, not an empty log.
    let err = check_history(false, Err("not a git repository".into()))
        .expect_err("an unreadable history is not an empty one");
    assert!(err.contains("not a git repository"), "got: {err}");
}

/// The measured list covers every commit this phase's finding named, and the two
/// a complete history revealed that a shallow one could not see.
#[test]
fn edge_measured_list_names_every_audit_measured_commit() {
    let named_by_phase_083 = [
        "09dd1020443e166a765991815c75c024a5ccd45b",
        "985e4456ce8d526d292e5157a449542bcdba5259",
        "96c3c17955d3b5233806c311110646ebe088fbd7",
        "89104562f7a2f5390906f565e4c6124c7eb09d4b",
        "1ee5aba2f2474d2e929710d3c4acd0c7855f8432",
        "b6d633b79417a55dce37869f7aef19d3171f904c",
        "dff04f67c8f2ed0b7bab3039ceb38df384fd739d",
        "4c45731b08b8f347beac5f46506456065aebc33e",
        "11b9fe2fc7d790068459f9554affa26f15af6a45",
        "2804ab87720c5c54811429664682e2f3f5d761f3",
        "861080d981d668aefa02f9d82641b2a8058fe6f6",
        "25e9b68d7529459ac0638f208d3e153681f80e32",
        "825193a4a06afdc22a8d73644961defa33d4d630",
    ];
    let visible_only_on_a_complete_history = [
        "10f97506426673e83119b6e0ec21f6754bddf75c",
        "e26659d49097210a2d1ab8abecc97d61698afebe",
    ];

    for sha in named_by_phase_083
        .iter()
        .chain(visible_only_on_a_complete_history.iter())
    {
        assert!(
            ACKNOWLEDGED.iter().any(|(entry, _)| entry == sha),
            "{sha} carries unreviewed work in HEAD's history and must be acknowledged here"
        );
    }

    validate_acknowledgements(ACKNOWLEDGED).expect("the checked-in list must be data");
    assert!(
        !ACKNOWLEDGED.is_empty(),
        "an empty list acknowledges nothing and would let every one of these pass silently"
    );
}
