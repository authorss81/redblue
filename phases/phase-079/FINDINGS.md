# phase-079 — FINDINGS

## Verdict: the finding does not reproduce on `main`. No code was changed.

This phase was written against a snapshot of `main` in which phase-041's work
was uncommitted. That snapshot is gone. Every load-bearing claim in the
evidence block is now false, and the first instruction in the phase body says
what to do about it:

> **Re-verify this before you change anything.** If the finding no longer
> reproduces on `main`, stop and write that to `FINDINGS.md` — a stale phase
> must never be "fixed" by inventing a change.

I did that. There is no defect left to fix, so there is no `src/` change to
make. Details and measurements below.

## The two commands the finding is built on

```
$ git status --porcelain
                                     <- prints NOTHING, not even untracked
$ git log --oneline main --grep='phase-041 '
034c5f0 rbops: phase-041 (2026-10-10T16:20:14Z)
```

The finding claims the first "reports ` M src/parser.rs`, `A
tests/text_split_join_test.rs`, `A phases/phase-041/REPORT.md`, `A
phases/phase-041/FINDINGS.md`" and the second "prints nothing". Both are the
exact opposite of what `main` does today.

## The work is already behind a commit

```
$ git show --stat 034c5f0
commit 034c5f06b754d3f95e9aebb5348e7c5be4f740bf
Author: rbops-bot <rbops-bot@users.noreply.github.com>
Date:   Sat Oct 10 16:20:14 2026 +0000

    rbops: phase-041 (2026-10-10T16:20:14Z)

 phases/phase-041/FINDINGS.md  |  120 ++++
 phases/phase-041/REPORT.md    |  323 +++++++++++
 src/parser.rs                 |  139 ++++-
 tests/text_split_join_test.rs | 1235 +++++++++++++++++++++++++++++++++++++
 4 files changed, 1800 insertions(+), 17 deletions(-)
```

The finding's own numbers — "1800 insertions and 17 deletions, of which
tests/text_split_join_test.rs is 1235 lines" — match this commit exactly. The
commit already exists; the finding described its contents while it was still
unlanded.

## `phases/phase-041/.done` never existed

The phase offers two outcomes: land the work, "or take back its `.done`". There
is no `.done` to take back.

```
$ git log --oneline --all -- phases/phase-041/.done
                                     <- never committed, no history at all
$ ls phases/phase-041/
FINDINGS.md  REPORT.md
```

So "REMOVE `.done`" is not merely already-done, it is not applicable.

## The Definition of Done, criterion by criterion

Measured on `main` at `c8156ab`, working tree clean.

| DoD criterion | Measured | How |
|---|---|---|
| `git status --porcelain` empty for `src/` and `tests/text_split_join_test.rs` | **met** | porcelain is empty outright, `--untracked-files=all` included |
| `git log --oneline main --grep='phase-041 '` prints ≥1 line | **met** | `034c5f0` |
| commit file list matches REPORT "What changed" by name | **met** | table claims `src/parser.rs`, `tests/text_split_join_test.rs`; both are in `034c5f0`, and the two `phases/phase-041/*.md` files are phase artifacts, which the table does not claim as implementation |
| assert that comparison in the phase's own test | **not met** | no such test; see "Residue" below |
| assert `git status --porcelain` empty before `cargo test --test text_split_join_test` | **not met** | no such test; see "Residue" below |
| commit single-parent on previous main commit | **met** | `git rev-list --parents -n 1 034c5f0` → one parent, `31622c9` = phase-040 |
| `git log --oneline main \| grep -c checkpoint` still 4 | **stale, cannot be met as written** | the count is **1**; see below |
| edge matrix: `split` empty text, `split` separator absent, `join` empty list | **met** | see below |
| `cargo test` 0 failures, clippy clean, no `allow(`, no `#[ignore]` | **met for the suite** | `cargo test --test text_split_join_test` → `20 passed; 0 failed; 0 ignored` |

### The checkpoint-count criterion is stale too

The DoD says the count "is still 4 afterwards". It is 1:

```
$ git log --oneline main | grep -c checkpoint
1
$ git log --oneline main | grep checkpoint
3fbf739 rbops: phase-078 audit the checkpoint ancestry of main instead of commenting on it
```

`3fbf739` is phase-078, the commit immediately below `main`'s HEAD `c8156ab`,
and it took this exact question as its subject. Its message records that six
`rbops: phase-NNN checkpoint` commits are ancestors of `main`
(`7fdea15, 074c95e, 563a351, 286fccd, 32d52ad, 97b4043`), each put there by a
real merge, and that it moved the concern into parsed data:
`tests/checkpoint_ancestry_ack.txt`, enforced by
`tests/checkpoint_ancestry_test.rs`.

So the "4" in phase-079 predates phase-078. It is a snapshot number, not an
invariant. Phase-079 should not assert it, and phase-078's audit is the thing
that now owns it.

### The three edge cases the phase names are already asserted

All three are in the committed file, each to a named value, not a smoke test:

- `split` on empty text — `SPLIT_CASES[0]` is `{subject: "", separator: ",",
  answer: [""]}`, plus `edge_empty_subject_splits_to_one_empty_part`
  (`tests/text_split_join_test.rs:783`)
- `split` with a separator absent from the text — `SPLIT_CASES[2]` is
  `{subject: "abc", separator: ",", answer: ["abc"]}`
  (`tests/text_split_join_test.rs:100`)
- `join` on an empty list — `edge_empty_list_joins_to_empty_text`
  (`tests/text_split_join_test.rs:798`)

Eight `edge_*` tests are present in total, covering unicode, nesting, wrong
types on both engines, an empty separator, and a record where a list is wanted.

## Residue — two real gaps, deliberately not filled by this run

Both are honest, non-fabricated, and are in the phase's own DoD, so the
auditor may want them as a real phase:

1. **No test asserts the commit's file list against phase-041's REPORT "What
   changed" table.** I verified it by hand above; it is not asserted anywhere,
   so it is exactly what the DoD says is "left to review".
2. **No test asserts `git status --porcelain` is empty before running
   `cargo test --test text_split_join_test`.** Without it, a green run from a
   dirty tree still reads as green. This is the guard that would have caught
   the original defect.

I did not add them here. Both are test-only, neither touches `src/`, and
adding them under a phase whose stated defect does not exist is scope
expansion against AGENTS.md rule 7 ("One phase, one concern").

## Manifest bug: `must_touch` cannot be satisfied honestly

phase-079 declares `must_touch: ["src/"]`. Since the finding is stale there is
no `src/` defect to change, so the gate will fail this phase on that check no
matter what I do. The only way to pass it is to edit `src/parser.rs` or another
`src/` file for no reason, which is fabrication and a review BLOCKER under
AGENTS.md section 5 ("fake completion"). Per AGENTS.md 3.5 this is the
documented case — "if your finding genuinely cannot be fixed in the declared
area, that is a manifest bug: say so in `FINDINGS.md` with the file:line".

Recommendation for the auditor: retire phase-079 rather than re-run it. If the
guard tests in "Residue" are wanted, they are a small phase of their own with
`must_touch: ["tests/"]`, in the shape of phase-020.