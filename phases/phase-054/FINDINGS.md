# Findings — phase-054

Work that this phase found and did **not** do, because it does not belong to it
(AGENTS.md §1 rule 7).

## 1. RESOLVED — `tests/doc_blocks_test.rs` failed on `main`, fixed by commit `6f336c6`

The first attempt of this phase found two pre-existing failures and correctly
attributed them to somebody else's `README.md` edit rather than to its own
diff:

```
running 7 tests
test every_documentation_block_is_classified_exactly_once ... FAILED
test running_every_block_agrees_with_the_bucket_it_is_in ... FAILED
test result: FAILED. 5 passed; 2 failed; 0 ignored
```

with

```
these entries point at a block that is not in any document:
  README.md:117
  README.md:128
  README.md:140
README.md:139 is in no bucket; classify it
```

The classifier's line coordinates in `tests/doc_blocks_runs.txt` had not moved
with the blocks they describe. This is now fixed — `6f336c6` re-pointed three
entries (+3/-3 lines in `tests/doc_blocks_runs.txt`) and `cargo test --test
doc_blocks_test` reports `7 passed; 0 failed`. Recorded here because the
original finding was real and the fact that it is closed should not depend on
reading this file's history.

## 2. `rbops/verify.sh` is not present in the checkout — still open

`ls rbops` → `No such file or directory`. The checkout has `phases/`,
`AGENTS.md`, `.github/` and `phases/INVARIANTS.md`, but not `rbops/`, so the
fourth gate of AGENTS.md §3.4 cannot be executed from here. The three Rust
gates were run and are green; the project-specific gate is outstanding and the
phase is not verified until it runs where `rbops/` exists.

## 3. `tests/unreviewed_work_ack_test.rs` selected this phase's own resume commit

`unreviewed_work_in_head_history_is_acknowledged` fails on any commit whose
subject reads `resume phase-NNN: adopt preserved work` until that SHA is on the
`ACKNOWLEDGED` list in that file. Commit `7e75dc1` — the pipeline's resume of
this very phase — landed on HEAD carrying the highlighter, so the audit named
it. That is the test working as designed: the tolerance is explicit, per
commit, and carries a reason. Fixed by adding `7e75dc1` to both `ACKNOWLEDGED`
and `MEASURED` with its `git show --numstat` figures. Not a defect, but worth
recording that the cost of a resume commit is one line in a test file and that
the audit is unforgiving by design.