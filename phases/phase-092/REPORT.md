# Phase 092 — Shard the test suite across parallel CI jobs

> Applied by hand. This phase changes `.github/workflows/`, and the rbops bot
> token deliberately has NO `workflows` permission (`.github/workflows/rbops.yml:10`)
> so that an agent cannot edit its own gate. An agent CANNOT land this phase.
> This is the intended lock, not a workaround.

## What changed
| File | Lines | What |
|---|---|---|
| .github/workflows/ci.yml | +58 −177 | Split the single serial job into `fast`, a 5-way `shard` matrix, a dedicated `corpus` job, and a `result` gate. Cache `target/` on the Cargo.lock hash. |

## Why the suite was slow

One job ran `cargo build` → `cargo test` → `cargo clippy` → the README block
executor → the corpus count pin, in series. The suite is 1,472 `#[test]`
functions across 53 integration targets plus 361 corpus programs.

The wall clock was set almost entirely by one target: `differential_test.rs`
runs the whole corpus through both engines, so the other 52 targets sat idle
behind it. The parallelism existed in the hardware and not in the workflow.

## The partition

Targets are assigned by `sha256(target_name) mod 5`. Verified against the real
`tests/` tree before pushing:

| Check | Result |
|---|---|
| Total integration targets | 53 |
| Sharded | 50 (6/9/9/13/13) |
| Dedicated corpus job | 3 (`differential_test`, `bootstrap_selfhost_test`, `bootstrap_ship_test`) |
| Each target in exactly one shard | yes |
| No duplicates | yes |
| Full coverage | yes |
| Adding a target disturbs only its own shard | yes (pure hash) |

A hash and **not** a round-robin: round-robin shifts every later target when a
file is added. With a hash, a new test changes only its own shard's membership.

The three heavy targets get a dedicated job rather than sharing a shard —
they cost enough to be the critical path alone, and leaving them in a shard
would serialise the suite behind them again, the same problem in a smaller box.

## Gates
| Gate | Result |
|---|---|
| Partition total / non-overlapping / stable | verified by script over the real tree |
| `yaml.safe_load` parses the workflow | pass (4 jobs) |
| CI run `38362122114` | **pass — 8/8 jobs green** |

## Measured result: 8m25s → 8m8s. That is 17 seconds.

The sharding works, and it is worth having — but it did **not** make the suite
meaningfully faster, and this phase should not be read as a speed win.

| | Before (`38354299013`) | After (`38362122114`) |
|---|---|---|
| Wall clock | 8m25s | **8m8s** |
| Jobs | 1 serial | 8 (fast, 5 shards, corpus, result) |

All 8 jobs concluded `success`: `fast`, `shard (0..4)`, `corpus`, `result`.

**Why so little.** The suite was already bounded by `corpus` — the 361-program
differential harness run through both engines. Sharding gave the other 50
targets their own lanes, but they were all finishing well inside the corpus
job's time, so there was no slack to recover. The parallelism was real; there
was nothing to win. `corpus` is still the critical path and will remain so as
the corpus grows.

**What the phase actually bought:** resilience, not speed. A slow or failing
target no longer blocks the other 52, a failure now names the shard it came
from, and there is no longer a single serial job whose wall clock is set by one
target. That removes a ceiling on how large the suite may become — which was
this phase's stated purpose — but "removed the ceiling" and "made it faster"
are different claims and only the first is true.

A future phase that wants real speed on the suite has to attack `corpus`
itself (parallelise the corpus harness, or split the corpus), not redistribute
work that was never the bottleneck.

## Honest record: this phase broke the build twice before it worked

Both failures were mine, and the first was invisible:

1. **`d1380a4` failed in 0s with no log.** The `result` job read the matrix
   outcome through a dynamic `needs` lookup inside a loop. GitHub expressions
   have no dynamic property access, so the workflow did not parse. The run page
   said only "likely failed because of a workflow file issue" — no jobs, no
   steps, no log. Fixed in `589d8ca` by naming each `needs` job literally.

   The lesson: valid YAML is not a valid workflow. The first version passed a
   YAML parse and still could not start a build.

2. **The partition test in smoke would have missed this class entirely.** The
   workflow is not exercised by any local test; only CI can catch it.

## Known gaps / follow-ups

- **`cargo fmt --check` is deliberately NOT in CI.** The tree is not currently
  fmt-clean, so adding it would turn every build red. That is a separate
  decision (format the tree, or accept drift) and is recorded rather than
  silently omitted.
- **`actions/cache` is keyed on the commit SHA as well as the lockfile.** A
  second push on the same lockfile restores the most recent matching cache via
  `restore-keys`, so the common case hits; an exact-lockfile hit on a
  different SHA will miss. A future phase can drop the SHA from the key if the
  cache-hit rate proves poor.
- **Shard balance is uneven by hash** (6/9/9/13/13). Fine while every shard is
  far below the corpus job; worth revisiting if a shard ever becomes the long
  pole.
- **The `result` gate is untested in its failing direction.** It has only ever
  run with every job green, so the branch that actually fails the workflow has
  never executed. That is the one part of this commit CI has not proven.

## Invariants touched

None. No language, grammar, public type or test was changed — this phase touched
exactly one file, `.github/workflows/ci.yml`.