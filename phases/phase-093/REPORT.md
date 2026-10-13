# Phase 093 — Cache target/ between CI runs

> Applied by hand. Changes `.github/workflows/`; the rbops bot token has NO
> `workflows` permission (`.github/workflows/rbops.yml:10`) so an agent cannot
> land this. The intended lock.

## Status: DONE. One thing landed; the rest was dead code and is abandoned.

## What landed

`actions/cache` on every job, keyed on the Cargo.lock hash, with `restore-keys`
falling back to the lockfile alone. A second push on the same lockfile restores
instead of recompiling cold.

Not separately measured. Caching was folded into the same commit as the
sharding (092), so its effect cannot be isolated from the sharding's in that one
run — and the sharding's own effect turned out to be 17 seconds. Claiming a
caching win from that run would be inventing a number.

## What was proposed and rejected: the trigger split

This phase originally also asked to run the corpus only on `pull_request` and on
push to `main`, not on every branch push.

**That premise was wrong.** `gh pr list` returns zero pull requests in both
`authorss81/rbops` and `authorss81/redblue`. Every change — the bot's phase
commits and human commits alike — goes straight to `main`. So:

- the `pull_request:` trigger in this workflow has never fired once
- the "branch push with no PR" case does not exist
- trigger logic to skip the slow tier would serve zero runs

It is dead code rather than backlog, which is why this phase is closed rather
than left partial. A phase that looks open-ended eventually gets "finished" by
someone adding trigger logic for a pattern that never occurs.

## Delivery model: direct-to-main

For a bot-driven pipeline the machine gate plus the read-only reviewer is the
review model. A PR would add merge latency and a new failure mode without adding
a reviewer. If PRs are introduced later, the trigger split becomes meaningful
and should be designed then against real traffic.

## Gates
| Gate | Result |
|---|---|
| CI run `38362122114` | pass — 8/8 jobs green, 8m8s |
| Full suite on both triggers | unchanged, and now known to be sufficient: there are no other triggers in use |
| `cargo fmt --check` in CI | deliberately not added — the tree is not fmt-clean, so it would turn every build red |

## Invariants touched

None. One file changed: `.github/workflows/ci.yml`. No test, language or public
type was touched.

## Known gaps / follow-ups

- **`cargo fmt --check` remains absent.** The tree must be formatted before the
  check can be added, or CI goes permanently red. Worth its own phase.
- **Caching has never been measured in isolation.** To measure it, compare a cold
  run against a warm run on a commit that changes only a source file.
- **The `result` gate's failing branch is still unproven** (carried from 092) —
  it has only run with every job green.