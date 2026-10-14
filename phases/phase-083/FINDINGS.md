# phase-083 FINDINGS

Work found while doing phase-083 that does **not** belong to this phase.
Recorded here for the auditor, per AGENTS.md § 1.7.

## 1. Phase-083's own evidence block was measured on a shallow clone — its premise is false

Phase-083 was written to "supersede phase-078's finding, which is provably false
as written". Both halves of that claim were measured against a history that had
not been completed. Measured on this checkout **after**
`tests/checkpoint_ancestry_test.rs` ran its `git fetch --unshallow`:

| Phase-083's claim | Shallow clone | Complete history | Verdict |
|---|---|---|---|
| `git log --oneline main \| grep -c checkpoint` prints `0` | `1` | **`7`** | false |
| `git merge-base --is-ancestor <c> main` is false for all eight of `7fdea15`, `074c95e`, `563a351`, `286fccd`, `e97983a`, `d664d3c`, `2179034`, `255816e` | false | **exit 0 for all eight** | false |
| thirteen `resume phase-NNN: adopt preserved work` commits on `main` | `11` | **`15`** | undercounted by 2 |

The mechanism is that a truncated `git log` stops early and
`git merge-base --is-ancestor` answers `false` for a commit the object store
has not been given — indistinguishable from "that commit is not there". This
phase's checkout started shallow (`.git/shallow` present, `main` at 120 commits,
`grep -c 'resume phase-'` = 11); the phase-078 audit unshallowed it at
2026-10-14T13:08Z and the counts above moved to 7 / all-eight / 15.

**What was done instead of the supersession.** The supersession was not
performed. `tests/unreviewed_work_ack_test.rs` **extends** phase-078's audit
with the `resume phase-NNN` shape it was blind to, and pins the truth in the
only direction that can be true:
`edge_phase_078_commits_are_ancestors_of_head_not_absent` asserts all eight
**are** ancestors of `HEAD`.

**Definition-of-done item not met, deliberately.** Phase-083 asked for "a second
test asserts phase-078's eight superseded commits are NOT ancestors of HEAD".
That assertion is false on a complete history, so writing it would have been a
test that passes only while the clone is shallow — the exact defect this phase
was raised to fix. The inverse assertion is written instead and says so in its
doc comment and in `REPORT.md`.

**Suggested phase.** A pipeline phase, not a language one: make audit-producing
steps complete the clone before they measure it, the way the phase-078 test
does. Two audits have now been raised from a truncated `git log` in this
repository — phase-083's whole premise and phase-078's undercount of
`32d52ad`/`97b4043` — and the third will be too. Anything that runs
`git log` as evidence should go through one shared "complete the history first"
helper rather than each audit re-deciding.

## 2. `rbops/verify.sh` is not inside the project checkout

Gate 4 of the four (`./rbops/verify.sh phase-083`) cannot be run from
`/home/runner/work/rbops/rbops/redblue`: there is no `rbops/` directory here.
`ls -a` at the checkout root shows `.github`, `phases`, `bootstrap`, `corpus`,
`docs`, `examples`, `modules`, `src`, `target`, `tests`, `tooling`, `wasm` — no
`rbops/`, no `verify.sh`. The phase prompt also forbids inspecting `rbops/`.

This phase ran the other three gates itself and reports their real output in
`REPORT.md`. Gate 4 was not run and is **not** claimed. `phases/phase-081/FINDINGS.md`
§ 6 records the same split from the other side: the gate reads
`/home/runner/work/rbops/rbops/phases/phase-081/REPORT.md`, one level above this
checkout, while `cargo` runs in the checkout — two different project roots. The
in-repo `phases/phase-083/REPORT.md` is written where AGENTS.md § 4 names it and
where `phases/phase-080/REPORT.md` and `phases/phase-081/REPORT.md` are
committed from; whatever copies it to the pipeline's copy is the dispatcher's
step, and this phase cannot do it without writing outside the working directory.

## 3. The gate's test-policy scanner may not see a new test file

Not observed in this run — gate 4 could not be run at all (§ 2) — but recorded
because it was measured in full for phase-081 and a stale reading of it would
mis-grade this phase. `phases/phase-081/FINDINGS.md` § 6b measured that
`tests/doc_blocks_test.rs` has 7 `#[test]` functions, 5 named `edge_*`, and one
asserting that five wrong recordings are refused, while the gate reported
`0 rust / 0 redblue / no edge_*` in all four git states tried (untracked,
staged, `git add -N`, committed). If phase-083's gate reports the same zeros,
this phase's 13 `#[test]` functions and 12 `edge_*` names are what it is not
seeing, not an absence of them.

The work is therefore left **staged with `git add -N`**, which is the state
phase-081 measured as having the most of the gate green, and uncommitted
(committing makes `diff hygiene` report "no changed files detected against
HEAD"). Nothing was moved, split or renamed to influence a counter.

## 4. Two more `resume phase-` shapes exist outside `main` and are unmeasured

`git log --all` finds **27** `resume phase-NNN: adopt preserved work` commits.
Fifteen are on `HEAD` and are acknowledged by this phase. The other twelve —
`ac1e17d`, `dd29457`, `b451e60`, `3ffb886`, `baf45b9`, `ecd1533`, `6716e6d`,
`76efc80`, `5f4bc7b`, `0b2af5d`, `319772b`, and `phase-082`'s — are on archive
and in-flight branches, not on `main`, so they are correctly outside this
audit's scope.

Recorded because the count is a moving target that this audit is now the
instrument for: the moment one of them is merged to `main`, it lands on this
test's failure list by name and has to be acknowledged. That is the check
working, not a defect.
