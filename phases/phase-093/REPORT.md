# Phase 093 — Stop rebuilding from cold on every push, and run the fast tier first

> Applied by hand. Changes `.github/workflows/`; the rbops bot token has NO
> `workflows` permission (`.github/workflows/rbops.yml:10`) so an agent cannot
> land this. The intended lock.

## Status: PARTIALLY COMPLETE — one half landed, one half did not

This phase bundled two things. Only one of them is done, and the honest thing is
to say which rather than close it as finished.

### Done: `target/` caching

`actions/cache` is in every job, keyed on the Cargo.lock hash with the commit SHA
as a second component and `restore-keys` falling back to the lockfile alone. A
second push on the same lockfile restores rather than recompiling cold.

Not separately measured. Caching was folded into the same commit as the sharding
(092), so its effect cannot be isolated from the sharding's in that one run — and
the sharding's own effect turned out to be 17 seconds. Claiming a caching win
from that run would be inventing a number.

### NOT done: separating the fast tier by trigger

The phase asked for the full suite to run on `pull_request` and on push to
`main`, but **not** on every branch push. That is still not implemented — both
triggers still run everything.

What *is* true now is a different, weaker thing: the `fast` job runs build, clippy,
unit tests, the README blocks and the corpus-count pin, and it does not wait for
the shards. So a fmt/lint/type error surfaces in the fast job while the slow tier
is still running. That is real value and it is what the matrix produced, but it is
**not** the same as "a branch push skips the corpus".

I left it undone rather than half-implementing a trigger matrix, because the
trigger split needs a decision I should not make unilaterally: whether a push to a
branch with an open PR should run the corpus (it currently does, via
`pull_request`, and duplicating it on push would double the cost).

## Gates
| Gate | Result |
|---|---|
| `cargo fmt --check` in CI | **deliberately not added** — the tree is not fmt-clean, so adding it turns every build red |
| CI run `38362122114` | pass — 8/8 jobs green |
| Full suite on both triggers | unchanged: `push` to main and `pull_request` both run everything, as before |

## Invariants touched

None. One file changed: `.github/workflows/ci.yml`. No test, language or public
type was touched.

## Known gaps / follow-ups

- **The trigger split is still open** and needs one decision: does a branch push
  with an open PR also run the corpus, or only the PR event? Recorded in the
  rbops phase rather than guessed.
- **`cargo fmt --check` remains absent.** This is the standing gap from 092: the
  tree must be formatted before the check can be added, or CI goes permanently
  red. Worth its own phase.
- **Caching has never been measured in isolation.** If it matters, measure it on
  a commit that changes only a source file and compare against the cold-run time.