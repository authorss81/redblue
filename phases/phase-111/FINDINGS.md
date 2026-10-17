# phase-111 — BLOCKED: the file this phase must change is not in this repository

## Status

**Blocked. No production change was made. Nothing was faked.**

`phase-111` declares `must_touch: ["rbops/dispatch.sh"]`. That path does not
exist in this checkout, and I am forbidden from reading or writing anywhere
outside it. The phase therefore cannot be implemented, tested or gated by an
agent. This is a dispatch manifest bug, not a code defect.

## What I tried, and the exact output

All commands run in `/home/runner/work/rbops/rbops/redblue`.

```
$ ls rbops/
ls: cannot access 'rbops': No such file or directory

$ git ls-files | grep -c '^rbops/'
0

$ find . -name 'dispatch.sh' -not -path './target/*'
(no output)

$ find . -name 'smoke.sh' -not -path './target/*'
(no output)

$ find . -maxdepth 2 -name 'verify.sh' -o -maxdepth 2 -name 'phases.json'
(no output)

$ ls -d logs
ls: cannot access 'logs': No such file or directory

$ ls phases | grep -E 'phase-(09[0-9]|1[01][0-9])'
phase-092
phase-093

$ grep -rn "REVIEW VERDICT" -l .   (excluding ./target)
(no output)

$ git log --oneline -3
e008f0b rbops: phase-089 (2026-10-16T20:52:46Z)
bf9ffc3 resume phase-089: adopt WIP checkpoint
ad19910 rbops: phase-087 (2026-10-16T15:30:00Z)
```

## What this means

1. **The change target is absent.** `rbops/` is not a directory in this repo and
   `git ls-files` tracks zero files under it. The RBOPS pipeline source lives in a
   different repository (`authorss81/rbops`, per `phases/phase-093/REPORT.md`).
   This repo receives only its *phase commits* — visible above — not the dispatcher.

2. **The mandated test location is absent.** The definition of done requires "a
   smoke check in `rbops/smoke.sh`". That file does not exist here either, so
   there is nowhere in this repository to put the required regression tests.

3. **The mandated pre-change verification is impossible.** The prompt says: *"If
   the finding no longer reproduces on `main`, stop and write that to FINDINGS.md —
   a stale phase must never be 'fixed' by inventing a change."* The cited
   evidence, `logs/phase-080.review.3.log`, is not in this checkout, so **I
   cannot confirm the finding is still live.** Under the phase's own instruction,
   an unverifiable finding must not be "fixed" by inventing a change. Inventing
   one here would produce a diff in the wrong repository that fixes nothing.

4. **The fourth gate cannot be run.** `./rbops/verify.sh phase-111` — the gate
   that decides whether this phase passed — does not exist. There is no way to
   produce the `Gates` table in REPORT.md with anything but "not run".

5. **Two hard rules forbid the only path that would work.** AGENTS.md hard rule 1
   and this phase's own Constraints section both forbid touching `rbops/`. So
   even in the pipeline repository this is off-limits to an agent by design.

## Why this phase was dispatched at all

The phase's own Evidence section already states the correct conclusion:

> Marked `manual: true` for the reason phase-092/093 are: fixing this means
> editing rbops/dispatch.sh, which AGENTS.md hard rule 1 forbids an agent to
> touch, so an agent cannot land it and declaring it runnable would just burn
> attempts.

`phases/phase-092/REPORT.md` and `phases/phase-093/REPORT.md` are the worked
examples: both were "Applied by hand", with the same lock recorded in prose.
phase-111 is a third instance of the same class, and it reached an agent anyway.
**The bug is in the dispatcher's eligibility filter, not in this repository.**
Specifically: `manual: true` was evidently not honoured when phase-111 was
selected — a phase whose own evidence says it cannot be run by an agent was
still handed to one.

## What I believe is required

Nothing in this repository. Three things, in two repositories:

1. **In the RBOPS repo, by hand:** change the `blocking` severity regex at
   `rbops/dispatch.sh:912` to count `MAJOR` alongside `BLOCKER|CRITICAL`, so a
   review log whose only finding is tagged `[MAJOR]` falls through to fix-and-
   re-gate instead of the "findings present, none blocking - shipping" branch at
   `dispatch.sh:923-925`. Add the two smoke cases (synthetic `[MAJOR]` log →
   no `.done`; `REVIEW VERDICT: CLEAN` → `.done`) to `rbops/smoke.sh`. I have
   **not** read those lines in this run and cannot confirm they are still at
   those line numbers on the RBOPS repo's current `main` — whoever applies this
   by hand must re-verify the cited line numbers first.
2. **In the RBOPS repo:** fix the eligibility filter so `manual: true` phases are
   never dispatched to an agent. That is the actual defect that put phase-111 in
   front of me, and it will recur.
3. **Then, in this repo:** either re-open `phases/phase-080` for review or record
   its `[MAJOR]` (cited at `src/parser.rs:1813`) as fixed by phase-110. Note that
   `phases/phase-110/` does not exist here yet either, so that item cannot be
   closed from this checkout.

## One thing a future auditor should check

The defect this phase describes is that phase-080 shipped with an open `[MAJOR]`
and that defect is "still live on main". If the parser defect at
`src/parser.rs:1813` is in fact a real language bug, it is a **runtime defect in
Redblue** and is fixable *from this repository*, inside `src/`. This phase is
aimed at the wrong target for that half of the problem: the dispatcher fix
belongs in the RBOPS repo, but the underlying `src/parser.rs` defect belongs to
a normal `src/` phase and needs no special permission. phase-110 appears to be
that phase, but its directory does not exist here — verify it was actually
dispatched and landed before assuming the parser defect is covered.
