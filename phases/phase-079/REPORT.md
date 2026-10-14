# Phase 079 — Land phase-041's work behind a commit, or take back its `.done`

## What changed

| File | Lines | What |
|---|---|---|
| `phases/phase-079/FINDINGS.md` | +140 | evidence that the finding is stale, criterion by criterion |
| `phases/phase-079/REPORT.md` | this file | phase report |

**No file under `src/` was changed, and no file under `tests/` was changed.**

That is deliberate, and it means this phase does **not** satisfy its own
`must_touch: ["src/"]` declaration. The finding this phase was created from
does not reproduce on `main`, the phase body says explicitly that "a stale
phase must never be 'fixed' by inventing a change", and the only way to touch
`src/` here would be to edit production code for no reason. Under AGENTS.md
section 5 that is a review BLOCKER, so I stopped and documented instead.

The reproduction, verbatim:

```
$ git status --porcelain
                                     <- empty, including --untracked-files=all
$ git log --oneline main --grep='phase-041 '
034c5f0 rbops: phase-041 (2026-10-10T16:20:14Z)
```

The finding claimed the first printed four modified/added paths and the second
printed nothing. Both were the reverse of reality. `034c5f0` already carries the
work — `src/parser.rs`, `tests/text_split_join_test.rs`, and the two phase
reports, "4 files changed, 1800 insertions(+), 17 deletions(-)", which is the
finding's own arithmetic. `phases/phase-041/.done` has never existed in any
commit, so there was no marker to take back either.

Full measurement, including the two genuinely-unmet DoD sub-criteria and the
stale checkpoint-count assertion, is in `FINDINGS.md`.

## Tests added

| Test | Edge class covered |
|---|---|
| *(none added — the phase adds no behaviour)* | n/a |

The phase's "Definition of Done" asks me to *audit* the committed tests against
AGENTS.md 3.2 and add any uncovered row. I audited; nothing was uncovered among
the rows the phase names, so there was nothing to add:

| Row the phase names | Already asserted at |
|---|---|
| `split` on empty text | `SPLIT_CASES[0]` + `edge_empty_subject_splits_to_one_empty_part` (`tests/text_split_join_test.rs:783`) |
| `split` with a separator absent from the text | `SPLIT_CASES[2]` `{subject:"abc", separator:",", answer:["abc"]}` (`tests/text_split_join_test.rs:100`) |
| `join` on an empty list | `edge_empty_list_joins_to_empty_text` (`tests/text_split_join_test.rs:798`) |

Eight `edge_*` tests exist in that file in total (lines 783, 798, 814, 859, 887,
943, 974, 1005), covering unicode boundaries, nesting, wrong-argument types on
both engines, empty separator, and a record where a list is wanted.

I verified these by running the suite, not by reading it:

```
$ cargo test --test text_split_join_test
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Gates

Reporting only what I actually ran. This table is not a claim about
`verify.sh`, because I did not run it — see the note beneath.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass (exit 0, no diff) |
| `cargo clippy --all-targets -- -D warnings` | **not run** — no code changed; AGENTS.md rule 4 forbids long local builds |
| `cargo test --test text_split_join_test` | 20 passed, 0 failed, 0 ignored |
| `cargo test` (full suite) | **not run** — same reason; I changed no code, so suite state is whatever `main` already had |
| `./rbops/verify.sh phase-079` | **not run** — `rbops/` is out of bounds to me under AGENTS.md rule 1 |

## Invariants touched

- None. No lexer, parser, analyzer, VM, `Value`, `Error`, or `.rb` surface was
  touched. The tree at the end of this run is byte-identical to `c8156ab` for
  every path except the two files in this phase directory.

## Known gaps / follow-ups

- **`must_touch: ["src/"]` is unmet by this run.** The gate will fail phase-079
  on that check. Satisfying it would require a fabricated `src/` edit, so I did
  not. Recorded as a manifest bug in `FINDINGS.md` with the evidence.
- **Two DoD sub-criteria are genuinely unimplemented** and would have needed new
  test code, which I judged to be out of scope for a phase whose stated defect
  does not exist (AGENTS.md rule 7):
  1. nothing asserts the commit's file list against phase-041's REPORT "What
     changed" table by name;
  2. nothing asserts `git status --porcelain` is empty *before* running
     `cargo test --test text_split_join_test`, which is precisely the guard that
     would have caught the original defect class.
  → `FINDINGS.md`, "Residue". Recommended as a small `must_touch: ["tests/"]`
  phase, shaped like phase-020.
- **The DoD's "checkpoint count is still 4" cannot be met as written.** It is 1.
  `3fbf739` (phase-078, the commit directly below HEAD `c8156ab`) took the
  checkpoint-ancestry question as its subject and moved it into
  `tests/checkpoint_ancestry_ack.txt` + `tests/checkpoint_ancestry_test.rs`. The
  "4" predates that work and is a snapshot number, not an invariant.
  → `FINDINGS.md`. The auditor should retire phase-079 rather than re-run it.