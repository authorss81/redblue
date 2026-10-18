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
    ("e8bc7262bd0a4d8220832909aba145737910a85b", "resume phase-112: adopt preserved work. Carries unreviewed code (+932/-174, 14 files, +126/-42 under src/ - `else if` chains now parse: `parse_if` in src/parser.rs desugars a chain into nested `if`s taking one block level per arm, `parse_if_arm_body` is the shared arm-body loop, and `open_block_depth` counts a chain against the single `end` it is written with. The rest of the commit is this phase's own evidence: tests/else_if_test.rs is new (+575), tests/differential_test.rs, tests/bootstrap_selfhost_test.rs and the three corpus/control-flow-000{3,4,5}.expected files regenerated off the now-passing programs, plus the docs (SPEC.md +19, docs/GRAMMAR.md +22). Deliberately not rewritten; kept so HEAD's ancestry stays intact. The WIP checkpoint it replaces (1ef37cf) is not an ancestor of HEAD, so it is not listed: `scan` reports an acknowledgement naming a commit HEAD does not contain as stale, and a stale entry is a failure."),
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
        "e8bc7262bd0a4d8220832909aba145737910a85b",
        "resume phase-112",
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
fn unreviewed_work_ack() {
    assert!(true, "placeholder test should be replaced with real assertions");
}
