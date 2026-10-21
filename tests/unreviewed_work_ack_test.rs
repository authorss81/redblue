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
//! # The repair is one at a time, and it waits for the clone
//!
//! Completing a shallow clone is a `git fetch --unshallow`, and two tests in
//! this binary both complete it before they audit anything. `cargo test` runs
//! the tests in a binary on parallel threads, so on a shallow clone both would
//! fetch at the same instant and collide on `.git/shallow.lock` — and the
//! loser of that collision leaves the clone exactly as short as it was, which
//! is the one thing the audit exists to refuse. The first run of this binary
//! in a fresh depth-1 checkout failed two tests exactly that way, over a
//! clone that was whole by the time the failure printed.
//!
//! So the repair is serialised ([`HISTORY_REPAIR`]), the decision about it is
//! split out of the subprocess call so it can be driven from fixtures
//! ([`ensure_history`]), and the repair waits for the clone's **state** rather
//! than for a count of attempts. That last part is the one that matters
//! against a fetch from *outside* this binary: the pipeline's own fetch holds
//! `.git/shallow.lock` for as long as it takes, every attempt made while it
//! holds the lock loses, and the clone becomes complete the moment it lands.
//! Counting attempts refused a clone that was complete a moment later;
//! watching the depth does not.
//!
//! # The delivery commit, acknowledged by its subject
//!
//! One commit in this history is created by the pipeline **after** the run that
//! wrote the work, and the pipeline re-creates it on every round: each
//! delivery is squashed onto the same parent, so the SHA one round recorded
//! leaves `HEAD`'s ancestry the moment the next one lands.
//!
//! That makes a SHA-pinned entry for it a guarantee of failure, and the
//! audit caught it paying that bill for twelve rounds: the entry named the
//! previous round's SHA, so the audit reported two things at once — a commit
//! in the log that nothing acknowledged, and an acknowledged commit the log
//! does not contain. Both are failures, and both are correct: the list was
//! describing a commit that was no longer there.
//!
//! What survives the rebuild is the **subject**, so that is what is pinned
//! here — [`DELIVERY_SUBJECT_PREFIX`] — and the SHA is resolved out of the
//! log at audit time by [`delivery_acknowledgements`]. A rebuilt delivery
//! commit is therefore neither unreviewed nor stale **by construction**: it
//! is acknowledged by the rule, and it cannot be stale because no SHA is
//! pinned that could go stale.
//!
//! The rule is narrow on purpose. It matches this phase's own delivery
//! subject and nothing else: another phase's `resume phase-083: …` is still
//! unreviewed work with no acknowledgement, and
//! [`edge_a_delivery_subject_of_another_phase_is_not_acknowledged_by_the_rule`]
//! says so.
//!
//! # What makes the list data rather than a comment
//!
//! [`validate_acknowledgements`] rejects an entry with no full lowercase object
//! id, with an empty reason, or duplicated. [`scan`] additionally reports
//! entries that name a commit this history does not contain, so a list of SHAs
//! that satisfy nothing cannot pass by looking thorough.

// What this file now is: the audit described above, written as assertions
// rather than as prose. The acknowledgement list below is data, the selector
// that picks the commits out of the history is a function, and the tests at the
// bottom are what make a commit that no reviewer saw impossible to leave
// unrecorded.

use std::cell::Cell;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

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
    //
    // A commit is listed here only while it is IN this history. The list is
    // checked against `git log HEAD` in both directions: a commit that is in the
    // log and not listed is unreviewed work, and a listed commit that the log
    // does not contain is stale. Both are failures. The pipeline creates the
    // commit that carries a phase's work *after* the agent's run, so the SHA of
    // that commit cannot be written here by the agent that produced the work --
    // it can only be written by the run that finds it in HEAD's history. The
    // parked deliveries of phase-084 named their own commit's SHA here
    // (`48d5bf0`, `c1ebb59`, `10f40c6a`), and each went stale the moment main
    // moved past it; the entries are gone for that reason, and the commits
    // themselves are still on their own branches, not on HEAD.
    //
    // The delivery commit of THIS phase is not listed here either, and the
    // reason is different: it is not that the SHA is unknown and will be
    // written later, but that the pipeline rebuilds that commit on every round
    // and so NO SHA can be written here at all. It is acknowledged by the
    // rule below instead.
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
    ("373513e4bba22305b6967f524ea02f1daa497532", "resume phase-061: adopt preserved work. Carries unreviewed code (+2247/-22, 7 files, +1907/-22 under src/ - the REPL discoverability work: member completion after a module dot (`complete_dotted`/`split_dotted` and the bare-dot case in src/repl/completer.rs, +127), the `$EDITOR` round trip for `.edit` (`ScratchDir`, `private_scratch_dir`, `write_scratch_file`, `read_edited_block`, `edit_in`, `edit_with`, `execute_code_from` in src/repl/mod.rs), the guided tour `:tutorial` with `:hint` and the `advance_tutorial_step` walk in src/repl/mod.rs, the inline call-signature hint behind the `Hinter` hook (`signature_hint` in src/repl/editor.rs, +184/-5), `stdlib::module_members` in src/stdlib.rs (+14), and the three command rows in src/repl/commands.rs (+41)), plus this phase's REPORT.md and 29 tests). Deliberately not rewritten; kept so HEAD's ancestry stays intact. The earlier phase-061 resume it replaces (0e84349) is not an ancestor of HEAD, so it is not listed: `scan` reports an acknowledgement naming a commit HEAD does not contain as stale, and a stale entry is a failure."),
    ("0647108eaf64d08d8df2f4a4338f9447ab1b4f35", "resume phase-062: adopt preserved work. Carries unreviewed code (+1231/-22, 5 files, +1231/-22 under src/ - the debug sub-REPL: `.debug` with `.watch`/`.stack`/`.locals`/`.back` (src/repl/debug_repl.rs is new, +514; the five command rows in src/repl/commands.rs, +116/-7; the session-loop wiring in src/repl/mod.rs, +446/-15), the frame-tracking fields `call_stack`, `scope_base` and `frame_writes` on `DebugEvent` in src/debugger.rs (+77) and the matching `Vm` fields and `call_user_function` bookkeeping in src/interpreter.rs (+78)). Deliberately not rewritten; kept so HEAD's ancestry stays intact."),
    ("c95062eb02a6e336f011a918daac7938f0efee6d", "ack: resume phase-088: add resume commit to ACKNOWLEDGED. Carries no production code: +1/-927 across 2 files, neither under src/ (phase-088's acknowledgement data and its bookkeeping). Acknowledged because the selector is a case-insensitive substring test on `resume phase-` and the subject merely *contains* the phrase; deliberately not rewritten."),
    // The commit that carried THIS phase's work into the tree is NOT listed
    // here. It used to be, and the entry went stale on every round: the
    // pipeline creates that commit after the run that wrote the work and
    // re-creates it on every round, rebuilt on the same parent (b21821d), so
    // the SHA one round wrote here left `HEAD`'s ancestry the moment the next
    // one landed — and the audit reported both halves of that: the new
    // commit as unreviewed work, and the old entry as stale.
    //
    // The delivery commit is acknowledged by the RULE below
    // ([`DELIVERY_SUBJECT_PREFIX`]), which pins the subject the pipeline
    // writes rather than a SHA the pipeline has not chosen yet. See
    // `delivery_acknowledgements`.
    //
    // It is acknowledged and DELIBERATELY not on `MEASURED`. The audit's
    // requirement is that every unreviewed commit in the log carries an
    // acknowledgement, and this commit is in the log, so it needs one.
    // `MEASURED` asks a different question — "measured on a complete
    // history" — and the delivery commit is not measured: it does not exist
    // while the work that goes into it is being written, and the pipeline
    // rebuilds it on the same parent on every round, so a SHA pinned there
    // leaves `HEAD`'s ancestry the moment the next round lands and costs
    // that round a second line for a commit nobody measured.
    //
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

/// The subject prefix that identifies the delivery commit of the phase that
/// owns this file.
///
/// The pipeline writes `resume <phase>: adopt preserved work` when it adopts a
/// preserved attempt, and `resume <phase>: adopt WIP checkpoint` when it adopts
/// a timed-out one. Both are this phase's own delivery commit, and both are
/// pinned by the prefix rather than by the whole subject, so the rule survives
/// the two shapes the pipeline uses.
///
/// The subject is what is pinned here and not a SHA, and the reason is
/// mechanical rather than stylistic: the pipeline creates this commit AFTER the
/// run that wrote the work, and re-creates it on every round, squashed onto
/// the same parent. The SHA one round recorded therefore leaves `HEAD`'s
/// ancestry the moment the next round lands, which made a SHA-pinned entry
/// stale by construction — the audit reported it as stale, and reported the
/// new commit as unreviewed, on every single round.
///
/// A prefix, not the whole subject, and this phase's, not a general one: the
/// rule must acknowledge exactly the commit the pipeline creates for THIS
/// phase, and nothing else. Another phase's `resume phase-083: …` is still
/// unreviewed work with nothing acknowledging it.
const DELIVERY_SUBJECT_PREFIX: &str = "resume phase-084:";

/// The reason recorded for the delivery commit this rule acknowledges.
///
/// It says the two things every entry in [`ACKNOWLEDGED`] says — the commit
/// carries unreviewed code, and it was deliberately not rewritten — and it
/// says why this one is keyed on its subject rather than on its SHA.
const DELIVERY_REASON: &str = "resume phase-084: adopt preserved work. The delivery commit of \
    this phase: it carries the unreviewed code this phase shipped — the repository-root \
    opencode.json that gives the reviewer its denied edit and bash permissions, the phase's \
    own suites (tests/agent_permissions_test.rs and tests/report_markdown_test.rs), the \
    acknowledgement data in tests/unreviewed_work_ack_test.rs, and this phase's REPORT.md and \
    FINDINGS.md) — none of it under src/. Deliberately not rewritten: published history is \
    immutable under AGENTS.md 1.6. Acknowledged by SUBJECT rather than by SHA because the \
    pipeline creates this commit after the run that wrote the work and rebuilds it on the same \
    parent on every round, so a SHA written here would name a commit this history does not \
    contain on the next round.";

/// The commits measured on a complete history of `HEAD`, and the shape each one
/// was found under.
///
/// Every SHA here is required to be present in the log this file reads. That is
/// the half of the promise the other half does not make on its own: a list can
/// satisfy itself by naming commits that no longer exist.
///
/// The delivery commit of the phase that owns this file is DELIBERATELY not
/// on this list, though it IS acknowledged — by the rule in
/// [`DELIVERY_SUBJECT_PREFIX`], not by an entry. The two lists answer
/// different questions. `ACKNOWLEDGED` says "this commit is in the log and
/// carries unreviewed work", and the audit needs an acknowledgement for every
/// such commit — including the one this phase's own work arrived in, which the
/// pipeline creates after the run that wrote the work. `MEASURED` says "this
/// commit was MEASURED on a complete history", and the delivery commit is not
/// one: it does not exist while the work is being written, and the pipeline
/// re-creates it on the same parent on every round, so a SHA written here
/// leaves `HEAD`'s ancestry the moment the next round lands. Pinning it here
/// would make the next round pay for a commit nobody measured — and neither
/// of the repairs the phase's own findings propose for the acknowledgement
/// list (a commit-time dispatcher line, or a subject shape the selector
/// misses) can repair a list that asserts the SHA's PRESENCE in the log.
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
    (
        "0647108eaf64d08d8df2f4a4338f9447ab1b4f35",
        "resume phase-062",
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
    /// The delivery commits this log carries, acknowledged by the rule rather
    /// than by an entry. Reported so a run can see which commit the rule
    /// covered, and so a rule that silently stopped matching is visible.
    delivery: Vec<String>,
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

/// One history repair at a time, within this test binary.
///
/// `cargo test` runs the tests in a binary on parallel threads, and two of
/// them complete the clone before they audit it. On a shallow clone both would
/// run `git fetch --unshallow` at the same instant and collide on
/// `.git/shallow.lock`; from the outside that is indistinguishable from the
/// "clone cannot be completed" refusal this audit is built around, and that is
/// exactly what it looked like: the first run of this binary in a fresh
/// depth-1 checkout failed two tests with that refusal, and the history they
/// went on to measure was complete a moment later.
///
/// `tests/checkpoint_ancestry_test.rs` serialises the same repair for the same
/// reason. The repair happens once, and the thread behind it finds the clone
/// already complete.
static HISTORY_REPAIR: Mutex<()> = Mutex::new(());

/// How many repairs are inside the locked region right now, and the highest
/// number ever seen there.
///
/// The mutex says the repair is one at a time. These two say that it is
/// *measured* to be, by the repair's own users: the counter is incremented
/// inside the locked region, so what it counts is the repair path the audit
/// actually runs. A test that takes the lock itself proves only that a mutex
/// excludes — not that the repair runs under it, which is the property that
/// was broken.
static REPAIRS_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);
static REPAIRS_PEAK: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// How many times this thread has entered the locked repair region.
    ///
    /// Thread-local so that a thread can be asked "did *you* run the repair?"
    /// without another thread's repair answering for it.
    static REPAIRS_ENTERED: Cell<usize> = const { Cell::new(0) };
}

/// How the repair behaves.
///
/// The repair is a `git fetch --unshallow`, and it can lose
/// `.git/shallow.lock` to another fetch of the same clone — the pipeline's
/// own, or another test binary's. A fetch that loses the lock leaves the
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
///
/// A function rather than a constant, so that the test which asserts on these
/// numbers asserts on the values the audit runs under rather than on
/// something the compiler folded away.
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

/// Complete a shallow clone before anything is measured.
///
/// This is the repair that makes phase-083's stale finding impossible to
/// re-derive: it is the difference between the `1` and the `7` in the table in
/// the module docs. It is a no-op on a complete clone, and it is the same
/// single repair `tests/checkpoint_ancestry_test.rs` performs. If it does not
/// take, this is an error — never a short log reported as a clean one.
///
/// The repair is serialised by [`HISTORY_REPAIR`], and the depth is re-read
/// after it, so a clone that was completed a moment earlier is recognised as
/// complete instead of being reported as un-completable.
fn ensure_full_history() -> Result<(), String> {
    with_history_repair(|| ensure_history(&is_shallow, &repair_shallow_clone))
}

/// The real accessor, with its two facts and its policy as parameters.
///
/// Split out so that a test can drive the **same path** — the lock, the
/// counter and the decision — with a repair it controls, instead of driving
/// the lock by hand and proving only that a mutex excludes.
fn ensure_full_history_with(
    is_shallow: &dyn Fn() -> Result<bool, String>,
    complete: &dyn Fn() -> Result<(), String>,
    policy: RepairPolicy,
) -> Result<(), String> {
    with_history_repair(|| ensure_history_with(is_shallow, complete, policy))
}

/// Run the repair, one at a time, and count what ran inside the lock.
///
/// The counter is incremented *inside* the locked region, so the number it
/// reports is about the repair the audit runs and not about the lock.
fn with_history_repair<T>(work: impl FnOnce() -> T) -> T {
    let _guard = match HISTORY_REPAIR.lock() {
        Ok(guard) => guard,
        // The lock guards a subprocess call, not state. A thread that panicked
        // leaves nothing half-written in the repository, so the next thread
        // may carry on rather than turn one failure into two.
        Err(poisoned) => poisoned.into_inner(),
    };
    REPAIRS_ENTERED.with(|entered| entered.set(entered.get() + 1));
    let in_flight = REPAIRS_IN_FLIGHT.fetch_add(1, Ordering::SeqCst) + 1;
    REPAIRS_PEAK.fetch_max(in_flight, Ordering::SeqCst);
    let _gauge = InFlight;
    work()
}

/// Decrements the in-flight count when the locked region is left, however it
/// is left.
struct InFlight;

impl Drop for InFlight {
    fn drop(&mut self) {
        REPAIRS_IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}

/// How many times this thread has run the repair inside the locked region.
fn repairs_entered_here() -> usize {
    REPAIRS_ENTERED.with(|entered| entered.get())
}

/// The one repair this audit may attempt: fetch the history the clone lacks.
///
/// The exit status is not the decision — the depth read after the repair is.
/// A fetch that lost a lock to a concurrent fetch of the same clone reports
/// failure and leaves the clone exactly as short as it was, which is a
/// different thing from a clone that cannot be completed at all.
fn repair_shallow_clone() -> Result<(), String> {
    git(&[
        "fetch",
        "--unshallow",
        "--quiet",
        "origin",
        "+refs/heads/*:refs/remotes/origin/*",
    ])
    .map(|_| ())
}

/// The decision, with the two facts it is made of as parameters.
///
/// The depth and the repair are parameters so the decision can be driven from
/// fixtures instead of from a network, and so the refusal after a failed
/// repair is testable without one.
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
    // A fetch can lose `.git/shallow.lock` to another fetch of the same clone
    // and leave the clone exactly as short as it was, so a single attempt
    // refuses a clone that is complete a moment later. The repair therefore
    // waits for the clone's *state*: the depth is re-read after every
    // attempt, and between attempts the depth is watched until the next
    // attempt is due. The attempts are bounded, and the watching is bounded
    // too, so a clone that really cannot be completed is still refused.
    // The refusal names the LAST attempt's outcome, not an earlier one's. An
    // error the last attempt no longer reproduces is not the reason the clone
    // is still short, and a reader who is handed it is being told about a
    // problem that has already gone away.
    let mut failure: Option<String> = None;
    let mut waited = Duration::ZERO;
    for attempt in 1..=policy.attempts {
        failure = complete().err();
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
             The audit refuses to read a truncated `git log HEAD`, because a truncated log \
             reads as a clean one. Run `git fetch --unshallow` first."
        ),
        None => "history is shallow and completing the clone did not make it complete: the \
             audit refuses to read a truncated `git log HEAD`, because a truncated log \
             reads as a clean one. Run `git fetch --unshallow` first."
            .into(),
    }
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

/// Is this the delivery commit of the phase that owns this file?
///
/// A prefix test on [`DELIVERY_SUBJECT_PREFIX`], so it matches the two shapes
/// the pipeline writes for this phase and nothing else.
fn is_delivery_subject(subject: &str) -> bool {
    subject.starts_with(DELIVERY_SUBJECT_PREFIX)
}

/// The delivery commits in `log`, resolved by subject, each with the reason it
/// is tolerated.
///
/// This is the repair for a rule that could not be satisfied: the pipeline
/// creates the delivery commit after the run that wrote the work, and rebuilds
/// it on the same parent on every round, so the SHA is not knowable when the
/// work is written and is stale by the time the next round's tree is
/// committed. The subject is the part that survives the rebuild, so the
/// subject is what is pinned, and the SHA is read out of the log here.
///
/// A commit resolved this way is acknowledged by construction: it is not
/// unreviewed, and it cannot be stale, because no SHA is pinned that could
/// point at nothing.
///
/// The reason is a constant, not a per-commit string, because the commit's
/// contents differ from round to round and a reason that quoted last round's
/// line counts would be a reason about a commit that is not here. It says what
/// every entry in [`ACKNOWLEDGED`] says — the commit carries unreviewed code,
/// and it was deliberately not rewritten.
///
/// The SHAs come out of the log itself, so a subject that matches nothing
/// resolves to nothing and the commit it named — if there is one — is reported
/// as unreviewed like any other. The rule cannot hide a commit; it can only
/// cover the one it names.
fn delivery_acknowledgements(log: &str) -> Vec<(&str, &str)> {
    let mut found: Vec<(&str, &str)> = Vec::new();
    for (idx, raw) in log.lines().enumerate() {
        let (sha, subject) = match parse_log_line(raw, idx + 1) {
            Ok(parsed) => parsed,
            // A malformed line is `scan`'s error to raise, not this function's
            // to swallow: it is reported by the caller with its line number.
            Err(_) => continue,
        };
        if is_delivery_subject(subject) && !found.iter().any(|(entry, _)| *entry == sha) {
            found.push((sha, DELIVERY_REASON));
        }
    }
    found
}

/// The delivery commits in `log`, by SHA.
fn delivery_commits(log: &str) -> Vec<&str> {
    delivery_acknowledgements(log)
        .into_iter()
        .map(|(sha, _)| sha)
        .collect()
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

    // The delivery commit is acknowledged by the rule, not by an entry, so it
    // joins the acknowledged set here rather than in `ACKNOWLEDGED`. It is
    // deliberately NOT added to the stale check: it was resolved out of this
    // log, so it is in this log by construction and cannot point at nothing.
    let delivery = delivery_acknowledgements(log);
    // The rule's reason is held to the same standard as every entry's: a
    // reason that records nothing would acknowledge nothing.
    validate_acknowledgements(&delivery)?;
    let mut acknowledged_shas: Vec<&str> = acknowledged.iter().map(|(sha, _)| *sha).collect();
    acknowledged_shas.extend(delivery.iter().map(|(sha, _)| *sha));

    let unreviewed = selected_shas
        .iter()
        .filter(|sha| !acknowledged_shas.contains(*sha))
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
        delivery: delivery
            .into_iter()
            .map(|(sha, _)| sha.to_string())
            .collect(),
    })
}

/// A one-line human summary, so `cargo test -- --nocapture` shows the numbers.
fn describe(scan: &Scan, acknowledged: &[(&str, &str)]) -> String {
    let mut s = String::new();
    let _ = write!(
        s,
        "unreviewed-work commits selected: {} | acknowledged: {} ({} by the delivery rule) | \
         unacknowledged: {} | stale entries: {}",
        scan.selected,
        acknowledged.len() + scan.delivery.len(),
        scan.delivery.len(),
        scan.unreviewed.len(),
        scan.stale.len()
    );
    if !scan.delivery.is_empty() {
        let _ = write!(s, " | DELIVERY: {}", scan.delivery.join(", "));
    }
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

/// The repair decision, driven from fixtures instead of from a network.
///
/// A complete history is never repaired; a history that cannot be completed is
/// refused, with a reason; and a repair that fails on a clone that stayed
/// short is refused with the failure named, so the refusal says which of the
/// two things went wrong.
#[test]
fn edge_the_history_repair_is_driven_by_the_depth_and_not_by_the_fetch() {
    // A complete history: the repair is never attempted. The repair is a
    // network call, and running it on a clone that needs no repair is how a
    // fetch ends up racing another one over the same lock.
    let repairs = Cell::new(0);
    ensure_history_with(
        &|| Ok(false),
        &|| {
            repairs.set(repairs.get() + 1);
            Ok(())
        },
        fixture_policy(),
    )
    .expect("a complete history needs no repair");
    assert_eq!(repairs.get(), 0, "a complete history must not be repaired");

    // A history that stays short: refused, and the repair was attempted.
    let attempts = Cell::new(0);
    let err = ensure_history_with(
        &|| Ok(true),
        &|| {
            attempts.set(attempts.get() + 1);
            Ok(())
        },
        fixture_policy(),
    )
    .expect_err("a history that stays shallow must be refused, not audited");
    assert!(
        err.contains("shallow"),
        "the refusal must say why, got: {err}"
    );
    assert!(
        err.contains("truncated"),
        "and what refusing protects against, got: {err}"
    );
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

    // A repair that failed and left the clone as short as it was: refused, and
    // the failure is named, so the reader does not have to guess which of the
    // two things went wrong.
    let err = ensure_history_with(
        &|| Ok(true),
        &|| Err("cannot lock ref".into()),
        fixture_policy(),
    )
    .expect_err("a failed repair on a still-shallow history must be refused");
    assert!(
        err.contains("cannot lock ref"),
        "the refusal must name the failure, got: {err}"
    );
    assert!(
        err.contains("shallow"),
        "the refusal must say why, got: {err}"
    );
}

/// The refusal names the failure of the LAST attempt, not a stale one.
///
/// The repair attempts the fetch more than once, and one attempt can fail
/// while the next one succeeds without the clone becoming complete — a fetch
/// that succeeds is not the same thing as a fetch that completes the history.
/// The refusal has to say what the LAST attempt did, because that is the
/// attempt a reader has to reproduce. An earlier attempt's error is a problem
/// that has already gone away, and a reader holding it is sent looking for a
/// lock that is free.
#[test]
fn edge_the_refusal_names_the_last_attempts_failure_and_not_a_stale_one() {
    // The behaviour change: the first attempt fails and the ones after it do
    // not. The last attempt's outcome is "no failure", so the refusal must
    // name none — the earlier attempt's error is a problem that has already
    // gone away.
    let attempts = Cell::new(0);
    let err = ensure_history_with(
        &|| Ok(true),
        &|| {
            attempts.set(attempts.get() + 1);
            if attempts.get() == 1 {
                Err("the FIRST attempt's error".into())
            } else {
                Ok(())
            }
        },
        fixture_policy(),
    )
    .expect_err("a clone that stays short must be refused even when the last attempt did not fail");
    assert!(
        !err.contains("the FIRST attempt's error"),
        "the refusal must not name a failure the last attempt did not have, got: {err}"
    );
    assert!(
        err.contains("shallow"),
        "the refusal must still say why the history was not read, got: {err}"
    );

    // The other direction: every attempt fails, and the refusal names the LAST
    // one. This half is NOT the behaviour change — the old code overwrote the
    // failure on every failing attempt, so it named the last failing attempt
    // too — and it is here to keep the rule whole, so that a repair which
    // fails for two different reasons names the one a reader can reproduce.
    let attempts = Cell::new(0);
    let err = ensure_history_with(
        &|| Ok(true),
        &|| {
            attempts.set(attempts.get() + 1);
            if attempts.get() == 1 {
                Err("the FIRST attempt's error".into())
            } else {
                Err("the LAST attempt's error".into())
            }
        },
        fixture_policy(),
    )
    .expect_err("a clone that stays short must be refused, not audited");
    assert!(
        err.contains("the LAST attempt's error"),
        "the refusal must name the failure of the last attempt, got: {err}"
    );
    assert!(
        !err.contains("the FIRST attempt's error"),
        "the refusal must not name an earlier attempt's failure: it is not the reason the clone \
         is still short, got: {err}"
    );
}

/// A repair that lands only after the first attempt is accepted.
///
/// This is the contention the repair can meet: the fetch that loses
/// `.git/shallow.lock` to a concurrent fetch of the same clone leaves the
/// clone exactly as short as it was, so a single attempt refuses a clone that
/// is complete a moment later. The audit must wait for the repair that does
/// land rather than report the clone as un-completable — which is what a
/// first run in a fresh depth-1 checkout did, failing two tests over a clone
/// whose history was whole by the time it was measured.
#[test]
fn edge_a_repair_that_lands_after_a_contended_attempt_is_accepted() {
    let attempts = Cell::new(0);
    let shallow = Cell::new(true);
    ensure_history_with(
        &|| Ok(shallow.get()),
        &|| {
            attempts.set(attempts.get() + 1);
            if attempts.get() >= 2 {
                // The contended fetch has finished; the clone is complete.
                shallow.set(false);
            }
            Ok(())
        },
        fixture_policy(),
    )
    .expect("a repair that lands on a later attempt must be accepted, not refused");

    assert_eq!(
        attempts.get(),
        2,
        "the first attempt lost the lock and the second landed"
    );
}

/// A repair that lands after every attempt has been spent is still accepted.
///
/// This is the contention the review named, and it is the one a count of
/// attempts cannot survive. The repair used to make three attempts a second
/// apart and then refuse, so a contending fetch that held
/// `.git/shallow.lock` for longer than that two-second window lost every
/// attempt — and the audit refused a clone that was complete a moment later.
/// The repair now waits for the clone's *state*, so what bounds it is the
/// ceiling and not the number of attempts.
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

    let attempts = Cell::new(0);
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

/// The ceiling the repair waits under is generous, and the attempts are
/// bounded.
///
/// The ceiling is what outlasts a contending fetch, so a ceiling that is too
/// small brings back the failure this file exists to prevent: a clone that is
/// complete a moment later is refused. It is asserted here because a bound
/// nobody reads is a bound that can be shrunk by accident, and the refusal it
/// brings back is indistinguishable from the refusal it is supposed to be.
#[test]
fn edge_the_repair_waits_under_a_generous_ceiling_and_a_bounded_number_of_attempts() {
    assert!(
        repair_policy().wait >= Duration::from_secs(30),
        "the ceiling must outlast a fetch that is already in flight, got {:?}",
        repair_policy().wait
    );
    assert!(
        repair_policy().attempts >= 2,
        "a repair that is attempted once cannot lose a lock and try again, got {}",
        repair_policy().attempts
    );
    assert!(
        repair_policy().poll <= Duration::from_secs(1),
        "the depth is read too rarely to notice a completion, got {:?}",
        repair_policy().poll
    );
    assert!(
        repair_policy().poll < repair_policy().wait,
        "the ceiling must be longer than the interval it is read at"
    );
}

/// The repair is one at a time, so two audits cannot collide on the clone.
///
/// The defect this closes is a collision, and a collision is only visible while
/// it is happening: two threads that fetch the same clone at the same instant,
/// one of which loses `.git/shallow.lock` and leaves the clone exactly as
/// short as it was. So this drives the **repair path** — the same function
/// the audit calls, through the same lock, with the counter that is
/// incremented inside the locked region — from four threads that ask at the
/// same instant, and measures the peak number of repairs in flight. Delete
/// the lock from that path and the peak is four, and this is the test that
/// notices.
#[test]
fn edge_the_history_repair_is_serialised_between_threads() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};

    const THREADS: usize = 4;

    let shallow = Arc::new(AtomicBool::new(true));
    let start = Arc::new(Barrier::new(THREADS));

    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let shallow = Arc::clone(&shallow);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                // Every thread asks at the same instant, so the lock is the
                // only thing that can keep them apart.
                start.wait();
                let depth = Arc::clone(&shallow);
                ensure_full_history_with(
                    &|| Ok(depth.load(Ordering::SeqCst)),
                    &|| {
                        // The repair takes long enough for a second thread to
                        // overlap it if the lock did not exclude it.
                        std::thread::sleep(Duration::from_millis(50));
                        depth.store(false, Ordering::SeqCst);
                        Ok(())
                    },
                    fixture_policy(),
                )
                .expect("the simulated repair completes the clone");
                repairs_entered_here()
            })
        })
        .collect();

    let mut entered = 0;
    for handle in handles {
        entered += handle
            .join()
            .expect("a thread that asks for the repair must not panic");
    }

    assert_eq!(
        entered, THREADS,
        "every thread must have run the repair inside the locked region: the \
         counter is what the lock is for"
    );
    assert_eq!(
        REPAIRS_PEAK.load(Ordering::SeqCst),
        1,
        "the repair must be one at a time: two audits fetching the same clone at \
         once is what left the clone short and failed the audit"
    );
}

/// The real repair runs inside the locked region.
///
/// The serialisation test above drives the path with a repair it controls.
/// This one pins that the accessor the audit actually calls — git's own fetch
/// and all — is that same path, and not a copy of it that happens to look
/// similar.
#[test]
fn edge_the_real_history_repair_runs_inside_the_locked_region() {
    let before = repairs_entered_here();
    ensure_full_history().expect("this checkout's history must be completable");
    assert_eq!(
        repairs_entered_here(),
        before + 1,
        "the real repair must run inside the locked region, or the lock guards \
         nothing the audit uses"
    );
}

/// After the repair, this checkout's history is complete.
///
/// The refusal is worth nothing if the repair does not land, so the end of it
/// is measured rather than assumed: after [`ensure_full_history`] the depth is
/// read again, and a clone that is still short is a failure — it is the
/// truncated log that reads as a clean one.
#[test]
fn edge_this_checkouts_history_is_complete_after_the_repair() {
    ensure_full_history().expect("this checkout's history must be completable");
    assert!(
        !is_shallow().expect("the depth must be readable"),
        "the repair ran and the clone is still shallow: the audit would read a \
         truncated log and call it clean"
    );
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

/// An `edge_*` case: the delivery commit, rebuilt under a new SHA.
///
/// The pipeline creates the delivery commit after the run that wrote the work,
/// and re-creates it on every round, squashed onto the same parent. So the
/// SHA one round recorded leaves `HEAD`'s ancestry the moment the next round
/// lands, and the audit reported that twice over: the NEW commit as
/// unreviewed work, and the OLD entry as stale. Both are failures, and both
/// were correct — the list was describing a commit that was no longer there.
///
/// Neither can happen now. The commit is acknowledged by the rule, and no SHA
/// is pinned that could point at nothing.
#[test]
fn edge_a_rebuilt_delivery_commit_is_neither_unreviewed_nor_stale() {
    let previous_round = SHA_A;
    let this_round = SHA_B;
    let log = format!(
        "{this_round}\tresume phase-084: adopt preserved work\n\
         {SHA_C}\tci: add cargo fmt --check\n"
    );
    let nothing_pinned: &[(&str, &str)] = &[];

    let result = scan(&log, nothing_pinned).expect("a well-formed fixture log");

    assert_eq!(result.selected, 1, "the delivery commit is selected");
    assert_eq!(
        result.delivery,
        vec![this_round.to_string()],
        "the rule resolves the delivery commit out of the log by its subject"
    );
    assert!(
        result.unreviewed.is_empty(),
        "the rebuilt delivery commit is acknowledged by the rule, not reported as unreviewed: {:?}",
        result.unreviewed
    );
    assert!(
        result.stale.is_empty(),
        "no SHA is pinned for the delivery commit, so there is nothing that can go stale: {:?}",
        result.stale
    );

    // The contrast that says what the rule is for: pin the PREVIOUS round's
    // SHA — which is what this file used to do — and the very same log reports
    // it as stale, because the commit it names is not in this history.
    let pinned = [(previous_round, "the previous round's delivery commit")];
    let pinned_result = scan(&log, &pinned).expect("a well-formed fixture log");
    assert_eq!(
        pinned_result.stale,
        vec![previous_round.to_string()],
        "a SHA-pinned delivery entry goes stale the moment the pipeline rebuilds the commit, \
         which is why the rule pins the subject instead"
    );
}

/// An `edge_*` case: the rule covers this phase's delivery commit and nothing else.
///
/// A rule that acknowledged any unreviewed commit would be a rule that
/// acknowledged nothing. Another phase's delivery commit is still unreviewed
/// work until something acknowledges it, and the audit must say so.
#[test]
fn edge_a_delivery_subject_of_another_phase_is_not_acknowledged_by_the_rule() {
    let nothing_pinned: &[(&str, &str)] = &[];

    let other_phase = format!("{SHA_C}\tresume phase-083: adopt preserved work\n");
    let result = scan(&other_phase, nothing_pinned).expect("a well-formed fixture log");
    assert!(
        result.delivery.is_empty(),
        "the rule covers this phase's delivery commit and no other"
    );
    assert_eq!(
        result.unreviewed,
        vec![format!("{SHA_C}\tresume phase-083: adopt preserved work")],
        "another phase's delivery commit is unreviewed work until something acknowledges it"
    );

    // And the rule anchors at the START of the subject: a commit that merely
    // mentions this phase's delivery subject is not the delivery commit.
    let mentioned = format!("{SHA_C}\tci: not really a resume phase-084: adopt preserved work\n");
    let result = scan(&mentioned, nothing_pinned).expect("a well-formed fixture log");
    assert!(
        result.delivery.is_empty(),
        "the rule matches a subject that BEGINS with the delivery prefix, not one that mentions it"
    );
    assert_eq!(
        result.unreviewed.len(),
        1,
        "a commit that merely mentions the delivery subject is still unreviewed work"
    );
}

/// The delivery commit in this history is acknowledged by the rule, and by no
/// entry.
///
/// The list this file keeps cannot hold the delivery commit's SHA — the
/// pipeline rebuilds that commit on every round, so a SHA written here is
/// stale by the time the next round's tree is committed. The rule is what
/// acknowledges it, and this asserts that the rule really covers the commit
/// this history carries, and that nothing pins it by SHA behind the rule's
/// back.
#[test]
fn edge_the_delivery_commit_is_acknowledged_by_the_rule_and_by_no_entry() {
    let (scan, log) = audit_head().expect("HEAD's history must be readable");
    let delivery = delivery_commits(&log);

    assert!(
        !delivery.is_empty(),
        "this history must carry a delivery commit whose subject begins with \
         {DELIVERY_SUBJECT_PREFIX:?}. If the pipeline stopped writing that subject, the rule \
         has to be re-pointed at the shape it writes now — silently, this history's delivery \
         commit would be reported as unreviewed work."
    );
    for sha in &delivery {
        assert!(
            !ACKNOWLEDGED.iter().any(|(entry, _)| entry == sha),
            "{sha} is pinned in ACKNOWLEDGED: a SHA-pinned delivery entry goes stale on the \
             next round, which is the failure this rule exists to remove"
        );
    }
    assert!(
        scan.unreviewed.is_empty(),
        "the delivery commit must be acknowledged by the rule: {}",
        scan.unreviewed.join(" | ")
    );
    assert!(
        scan.stale.is_empty(),
        "and no entry may point at a commit this history does not contain: {}",
        scan.stale.join(", ")
    );
}
