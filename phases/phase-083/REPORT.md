# Phase 083 — Make unreviewed work on redblue main detectable

## Reproduction (before any change)

Phase-083's evidence block claims phase-078's finding is false and that the
real defect is `resume phase-NNN: adopt preserved work`. **The first half does
not reproduce; the second half does, and is worse than stated.**

This checkout arrived **shallow** (`.git/shallow` present). Measured on it:

```
$ git log --oneline main | grep -c checkpoint
1
$ git log --format=%s main | grep -ci 'resume phase-'
11
$ git merge-base --is-ancestor 7fdea15 HEAD ; echo $?
1
```

which is what phase-083 measured. Then `cargo test` ran
`tests/checkpoint_ancestry_test.rs`, whose `ensure_full_history()` completed the
clone (`git fetch --unshallow`, `.git/FETCH_HEAD` at 2026-10-14T13:08Z). Measured
again on the **complete** history:

```
$ git log --oneline main | grep -c checkpoint
7
$ git log --format=%s main | grep -ci 'resume phase-'
15
$ for c in 7fdea15 074c95e 563a351 286fccd e97983a d664d3c 2179034 255816e; do
      git merge-base --is-ancestor $c HEAD; echo "$c exit=$?"; done
7fdea15 exit=0   074c95e exit=0   563a351 exit=0   286fccd exit=0
e97983a exit=0   d664d3c exit=0   2179034 exit=0   255816e exit=0
```

So phase-078's finding reproduces exactly, on all eight commits, and
`tests/checkpoint_ancestry_test.rs` passes against the same history
(24/24). **Phase-083's supersession premise is false and the supersession was
not performed.** See "Phase-078's finding: measured false, and why" below.

The real, unreproduced gap: `is_checkpoint_subject` in
`tests/checkpoint_ancestry_test.rs:341` matches only
`rbops: phase-<digits> checkpoint` at the start of a subject. The pipeline also
writes `resume phase-NNN: adopt preserved work`, and that shape reached `main`
**fifteen** times carrying work no reviewer saw. Nothing measured it.

## What changed

| File | Lines | What |
|---|---|---|
| `tests/unreviewed_work_ack_test.rs` | +912 −0 | new audit: every `checkpoint`/`resume phase-` subject in `HEAD`'s ancestry is on a parsed acknowledgement list, with a reason; plus 12 fixture-driven `edge_*` tests |
| `phases/phase-083/REPORT.md` | +213 | this file |
| `phases/phase-083/FINDINGS.md` | +95 | §1 the stale premise, §2 `rbops/verify.sh` not in the checkout, §3 the gate's test scanner, §4 twelve out-of-scope resume commits |

`git diff HEAD --stat`: **3 files changed, 1220 insertions(+), 0 deletions(-)**.

No file under `src/` is touched. This phase's whole surface is a measurement
over git history, which is what `must_touch: ["tests/"]` is for.

### The selector

`is_unreviewed_subject` (`tests/unreviewed_work_ack_test.rs:356`) is a
case-insensitive **substring** test for `checkpoint` or `resume phase-`. It is
not prefix-anchored on purpose: a prefix test lets a commit through by putting a
word in front, which is the entire failure mode. Phase-078's own audit commit —
`rbops: phase-078 audit the checkpoint ancestry of main instead of commenting on
it` — is a real commit on `main` that only a substring test selects, and it is
acknowledged for exactly that reason.

### The acknowledgement list

`ACKNOWLEDGED` (`tests/unreviewed_work_ack_test.rs:66`) is **22 entries**, all
full 40-character lowercase object ids, each with a reason. It is data, not
prose: `validate_acknowledgements` rejects an entry with no full lowercase id,
no non-whitespace reason, or a duplicate, and `scan` reports every entry that
names a commit the log does not contain.

| Group | Count | Already covered by |
|---|---|---|
| `resume phase-NNN: adopt preserved work` | **15** | nothing — this phase |
| `rbops: phase-NNN checkpoint` | 6 | `tests/checkpoint_ancestry_ack.txt` (re-listed; the two selectors are not disjoint) |
| phase-078 audit commit (subject merely contains `checkpoint`) | 1 | nothing — this phase |

Each of the fifteen `resume` entries records the shape it was found under, the
measured `git show --numstat` line counts, and that it carries unreviewed code
and was **deliberately not rewritten** (AGENTS.md § 1.6). The six checkpoints are
cross-referenced to the phase-078 data file rather than re-argued, so the two
lists cannot disagree about what is on `main`.

`e26659d` (phase-048) and `10f9750` (phase-050) are called out in their
entries as visible only on a complete history — they are the two the shallow
count of 11 could not reach.

## Tests added

All in `tests/unreviewed_work_ack_test.rs`. 13 `#[test]`, 12 named `edge_*`,
0 ignored, 0 `#[allow]`.

| Test | Edge class covered |
|---|---|
| `unreviewed_work_in_head_history_is_acknowledged` | the audit itself: 22 selected, 22 acknowledged, 0 unacknowledged, 0 stale; every `MEASURED` sha proven present in the log |
| `edge_phase_078_commits_are_ancestors_of_head_not_absent` | **asserts a failure is produced** by phase-083's stale claim; resource/state (complete vs shallow history) |
| `edge_empty_log_selects_nothing` | empty (log of 0 commits); singleton (1 reviewed commit); boundary (last line with no trailing `\n`) |
| `edge_a_fully_acknowledged_log_is_clean` | boundary: only unreviewed commit is acknowledged, 0 stale |
| `edge_exactly_one_unacknowledged_commit_is_reported` | **asserts a failure** — exactly one unacknowledged commit, named with its subject, and named again in the summary |
| `edge_a_longer_phrase_containing_checkpoint_is_still_selected` | boundary: `checkpoint` inside a longer phrase still selected; case folding; near-misses (`resume phase:`, `resumephase-083`) not selected |
| `edge_a_two_tab_delimited_log_line_is_malformed` | **asserts a failure** — malformed input: a 3-field line is rejected, not read as a subject with a tab in it |
| `edge_malformed_log_lines_are_refused_rather_than_skipped` | **asserts a failure** — malformed input: blank line between commits, empty sha, missing tab, 39-hex, uppercase hex, non-hex, empty subject |
| `edge_a_shallow_history_is_refused_rather_than_audited_clean` | **asserts a failure** — resource/state: a depth-1 log audits clean on its own, and `check_history` refuses it |
| `edge_an_acknowledgement_list_must_be_data` | **asserts a failure** — abbreviated sha, uppercase sha, blank reason, empty reason, duplicate |
| `edge_an_acknowledgement_that_points_at_nothing_is_stale` | missing keys: an entry naming an absent commit is reported stale |
| `edge_unicode_and_escaped_subjects_are_handled_deterministically` | unicode/escapes: emoji, Arabic RTL, combining acute accent, quotes, backslash |
| `edge_measured_list_names_every_audit_measured_commit` | all thirteen commits phase-083 named are present, plus the two only a complete history shows |

The audit caught a real defect in its own data while being written: the
phase-022 sha was transcribed `dff04f6…f4ed0b…` in three places and the true
value is `dff04f6…f2ed0b…`. The red run reported it exactly as designed —
`UNACKNOWLEDGED: dff04f67c8f2ed…` and `STALE: dff04f67c8f4ed…` in the same pass,
because the two halves of the check disagree in exactly that way. No test was
weakened to make it pass; the transcription was corrected.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass (clean after one `cargo fmt`) |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings, no `allow(` added |
| `cargo test --all-targets` | **1557 passed, 0 failed, 0 ignored** across 59 test binaries |
| `./rbops/verify.sh phase-083` | **NOT RUN — the script is not in this checkout** |

Gate 4 honestly: `./rbops/verify.sh` does not exist under
`/home/runner/work/rbops/rbops/redblue`; there is no `rbops/` directory in the
project root, and the phase prompt forbids inspecting it. It is not claimed as
passing. `FINDINGS.md` § 2 records this, and `FINDINGS.md` § 3 records the
test-policy-scanner result phase-081 measured, since that gate may not be able
to see a new test file at all.

`cargo test --test unreviewed_work_ack_test -- --nocapture` prints the
measurement the audit is for:

```
unreviewed-work commits selected: 22 | acknowledged: 22 | unacknowledged: 0 | stale entries: 0
```

## Phase-078's finding: measured false as a supersession, and why

Phase-083's Definition of Done required "a second test asserts phase-078's eight
superseded commits … are **NOT** ancestors of HEAD". **That test was not
written, because the assertion is false.** Writing it would have produced a test
that passes only while the clone is shallow — a green light wired to the very
defect this phase was raised to fix, and a test that stops passing the moment
CI unshallow-completes its checkout.

What is asserted instead, in
`edge_phase_078_commits_are_ancestors_of_head_not_absent`: all eight
(`7fdea15`, `074c95e`, `563a351`, `286fccd`, `e97983a`, `d664d3c`, `2179034`,
`255816e`) **are** ancestors of `HEAD`, after `ensure_full_history()` has
completed the clone, and the test says what to do if that ever becomes false
(find out what happened — AGENTS.md § 1.6 forbids rewriting them).

The measured comparison is in the module docs of the test file and in
`FINDINGS.md` § 1: `grep -c checkpoint` is `1` shallow and `7` complete;
`merge-base --is-ancestor` is false shallow and exit `0` for all eight complete;
`resume phase-` on `main` is `11` shallow and `15` complete. Every number in
phase-083's evidence block is a truncated-history reading. **Phase-078's finding
is not superseded; it is extended**, with the `resume phase-` shape it could not
see added to the measurement, and phase-083's two extra commits added.

## Invariants touched

- None. No file under `src/`, no grammar, no `Value` variant, no error type, no
  `.rb` extension change. `examples/` and `modules/` untouched. All 1557 tests
  that existed before still pass.

## Test-requirement matrix (AGENTS.md § 3)

| Row | Status |
|---|---|
| empty | covered — `edge_empty_log_selects_nothing`: zero-commit log, zero-commit `scan`, and `stale.len() == ACKNOWLEDGED.len()` against it |
| singleton | covered — same test, one reviewed commit; and one *acknowledged* unreviewed commit in `edge_a_fully_acknowledged_log_is_clean` |
| boundary | covered — longer phrase containing `checkpoint`; case-folded `CHECKPOINT`/`ReSuMe PhAsE-083`; near-misses; final log line with no trailing newline; last-unreviewed line with no trailing newline |
| out_of_bounds | N/A + why — there is no indexable sequence here. The out-of-range analogue is a log line past the end and past the middle: an extra tab field and an absent sha are both refused rather than read (`edge_a_two_tab_delimited_log_line_is_malformed`, `edge_malformed_log_lines_are_refused_rather_than_skipped`) |
| type_mismatch | N/A + why — the input is `git` stdout, not Redblue values. The analogue is a well-formed line of the wrong kind: a subject with no unreviewed marker selects 0, and `SHA_C`'s checkpoint subject is reported while `SHA_A`'s resume subject is not (`edge_a_fully_acknowledged_log_is_clean`) |
| numeric_boundary | N/A + why — no arithmetic. `selected`/`stale` are counts and are asserted for 0, 1 and 22 |
| unicode | covered — `edge_unicode_and_escaped_subjects_are_handled_deterministically`: emoji, Arabic RTL, combining acute accent, quotes, backslash round-tripped through the selector unchanged |
| nesting_recursion | N/A + why — one flat pass over a log, no nesting. The recursive structure of the phase (`unreviewed_work_ack_test.rs` and `checkpoint_ancestry_test.rs` selecting overlapping sets) is handled by cross-referencing the six shared entries rather than by nesting |
| duplicate_missing_keys | covered — `edge_an_acknowledgement_that_points_at_nothing_is_stale` (entry for an absent commit) and `edge_an_acknowledgement_list_must_be_data` (duplicate sha refused). This is the row the phase-022 transcription bug actually exercised |
| malformed_input | covered — blank line, whitespace-only line, empty sha, no tab, 39-hex, uppercase hex, non-hex, empty subject, three tab-delimited fields, plus an absent repository and an unreadable history |
| resource_limit | covered — `edge_a_shallow_history_is_refused_rather_than_audited_clean`: a depth-1 log audits as spotless on its own, and `check_history` refuses it. That is this audit's resource limit: the size of the history read, not a stack |

## Known gaps / follow-ups

- **`git fetch --unshallow` is reachable.** `ensure_full_history` completes the
  clone when `git rev-parse --is-shallow-repository` says `true`, which is the
  single network call in this file. It is a no-op on a complete clone (verified
  here: `is-shallow-repository` = `false`, so the fetch never runs in this
  checkout), it is the same repair `tests/checkpoint_ancestry_test.rs:175`
  already performs, and without it a depth-1 CI checkout audits as spotless. The
  DoD's "offline" requirement is met by every path the tests assert over — all
  twelve fixture tests and the selector are pure functions with no process and
  no socket — but the completion path is not offline, and that is deliberate.
- **`3fbf739` is acknowledged for carrying no unreviewed code.** It is listed
  because the selector is a substring test. If a future phase wants to exclude
  such commits, that is a second rule, not an exemption in the data.
- **Twelve further `resume phase-` commits exist off `main`** (archive and
  in-flight branches) and are correctly out of scope. If one is merged, it lands
  on this test's failure list by name and has to be acknowledged. `FINDINGS.md`
  § 4.
- **The audit covers `HEAD`, which is `main` today.** A phase run on a branch
  audits that branch. Phase-078's file resolves `main` explicitly; this file
  audits what it is run against, which is the DoD's wording and the honest thing
  for a branch run to measure. Deliberate, and recorded here so it is not
  mistaken for an oversight.
- The work is **staged with `git add -N`, not committed**, so `git diff HEAD`
  shows it. See `FINDINGS.md` § 3 for why.
