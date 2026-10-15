# Findings — phase-054

Work that this phase found and did **not** do, because it does not belong to it
(AGENTS.md §1 rule 7).

## 1. `tests/doc_blocks_test.rs` fails on clean `main` — two tests

Reproduced with the working tree stashed (`git stash -u`, i.e. exactly `main`):

```
running 7 tests
test every_documentation_block_is_classified_exactly_once ... FAILED
test running_every_block_agrees_with_the_bucket_it_is_in ... FAILED
test result: FAILED. 5 passed; 2 failed; 0 ignored
```

The message names three block coordinates that no longer match a block and one
block in no bucket:

```
these entries point at a block that is not in any document:
  README.md:117
  README.md:128
  README.md:140
README.md:139 is in no bucket; classify it
```

- Test: `tests/doc_blocks_test.rs:517` (`every_documentation_block_is_classified_exactly_once`)
- Test: `tests/doc_blocks_test.rs:566` (`running_every_block_agrees_with_the_bucket_it_is_in`)

Somebody edited `README.md` and the classifier's line coordinates did not move
with the block they describe. Phase-054 touched no markdown file and cannot
repair this without editing `README.md` — which the phase's own constraints
forbid until the gate is green and the examples have been run. Needs its own
phase against `README.md` + `tests/doc_blocks_test.rs`.

## 2. `rbops/verify.sh` is not present in the checkout

`ls rbops` → `No such file or directory`. The checkout has `phases/`,
`AGENTS.md`, `.github/` and `phases/INVARIANTS.md`, but not `rbops/`, so the
fourth gate of AGENTS.md §3.4 cannot be executed from here. The three Rust
gates were run and are green; the project-specific gate is outstanding and the
phase is not verified until it runs where `rbops/` exists.
