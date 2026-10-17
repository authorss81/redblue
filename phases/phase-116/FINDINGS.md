# Phase 116 — FINDINGS

**Status: BLOCKED. Not implemented. Nothing was changed outside this directory.**

The phase cannot be executed by an agent under the contract it ships with. Three
independent blockers, each measured below.

---

## B1 — `must_touch` contradicts the phase's own Constraints

The phase Goal says:

> Implement nothing in Redblue; this is entirely rbops dispatch logic, so
> `must_touch` is `rbops/`.

The same prompt's Constraints say:

> Do not touch `.github/workflows/`, `rbops/`, `.opencode/agent/`, or
> `rbops/phases.json`. The gate rejects these paths.

The injected AGENTS.md §1 HARD RULE 1 repeats it:

> **Do not touch RBOPS itself.** Never edit `.github/workflows/`, `rbops/`,
> `rbops/phases.json`, or any `.opencode/agent/*.md`.

§3.5 says the gate fails a phase when *no* changed file falls under `must_touch`.
So the gate simultaneously requires a changed file under `rbops/` and forbids
changing any file under `rbops/`. No diff can pass. The last sentence of the Goal
("Declare this phase by hand: it changes rbops, which the bot token cannot
push") acknowledges the delivery problem but does not remove the constraint, and
it does not tell the agent where the by-hand patch is supposed to land.

**Precedent that does not apply.** phase-093 is titled "Applied by hand. Changes
`.github/workflows/`" (`phases/phase-093/REPORT.md:3-5`) and it worked because
`.github/workflows/` exists in *this* checkout — `git ls-files -- .github/workflows`
returns 2 tracked files. The equivalent target for 116 does not exist here.

## B2 — `rbops/` is not in this checkout, so the fix is not physically writable

The dispatched phase directs every agent to the Redblue checkout only. Measured:

```
$ ls -d */
bootstrap/ corpus/ docs/ examples/ modules/ phases/ src/ target/ tests/ tooling/ wasm/

$ git ls-files -- rbops | wc -l
0
$ git ls-files -- '*phases.json' | wc -l
0
$ git ls-files -- '.opencode/agent' | wc -l
0

$ ls phases | sort -V | tail -3
phase-089
phase-092
phase-093
```

`rbops/dispatch.sh` does not exist in the working tree, so there is nothing to
edit even if B1 were resolved. Creating a new `rbops/` directory here would
produce a shadow copy of the pipeline that the real dispatcher never executes and
that no gate can reach — fake capability, which §5 of the contract lists as a
BLOCKER-class finding. I did not do it.

## B3 — the cited evidence is outside the working directory

The finding's proof is phase-053 attempt 1 leaving a 0-byte
`work-phase-053.patch` and a 0-byte `phase-053.checkpoint.log`.

```
$ find . -name '*.patch' -o -name 'checkpoint.log'   # excluding target/
(no output)
$ ls -d phases/phase-053
ls: cannot access 'phases/phase-053': No such file or directory
```

I was instructed not to inspect the pipeline checkout that invoked me, so I
cannot confirm or refute the evidence. **The defect itself is plausible and I am
not claiming it is stale** — a 90-minute attempt killed at the timeout with an
empty patch is a real waste, and a cheap mid-budget evidence check is the right
shape of fix. What I could not do is verify it, and per the phase's own
instruction a stale finding must not be "fixed" by inventing a change.

## B4 — gate 4 cannot run

```
$ ./rbops/verify.sh phase-116
bash: ./rbops/verify.sh: No such file or directory
(exit 127)
```

The project's own machine gate — "the only authority on whether work is real" —
is absent from the checkout, so no claim in this directory can be gated.

---

## What the phase *should* become

The design is sound and worth keeping. For whoever can edit the dispatcher:

1. **Check after a fixed fraction of the attempt budget, not on wall-clock
   guessing.** Read the attempt's elapsed time and the budget constant the
   dispatcher already holds; fire the check at e.g. 40% of budget.
2. **Evidence, not inference.** The check is satisfied by *any* of: a test file
   newer than the attempt start, or a changed tracked file under that phase's
   `must_touch` paths. Git gives this for free (`git diff --name-only
   <start-sha>`), so it needs no agent cooperation and cannot be spoofed by
   prose.
3. **Fail loud on uncertainty.** If the workspace cannot be inspected (no git
   metadata, unreadable path, missing budget constant), log
   `watchdog: could not determine whether work happened; continuing` and let the
   attempt run. Never kill on an unknown.
4. **A distinct end state.** The kill must not write `.deferred`, `.failed` or
   `.blocked`, so a killed-for-no-output attempt cannot be miscounted as a
   deferral. It needs its own marker (e.g. `.starved` is taken; something like
   `.nooutput`) plus a log line naming the fraction, elapsed time, budget, and
   the paths checked.
5. **Budget unchanged.** The timeout constant must not be edited. Assert that in
   a test by reading the constant back and comparing it to its recorded value —
   not by a comment.
6. **Two fixture tests**, both directions: an agent that only reads and verifies
   gets killed; an agent that writes its test file inside the first 40% survives
   a run that sleeps well past the watchdog point. The second fixture is the one
   that prevents this phase from becoming a blanket time cut, which is the
   stated non-goal.

**Manifest recommendation to the auditor:** either (a) split 116 into a
human-declared phase on the phase-093 pattern — "Applied by hand; changes
`rbops/`" — with `must_touch` dropped or pointed at the phase directory
(`phases/phase-116/`), or (b) give agents a sanctioned, delegated way to land
dispatch changes so `must_touch: rbops/` is not self-contradictory. Option (b) is
the one that stops this class of phase from recurring.

I did not attempt items 1-6 as code. Writing a watchdog whose caller, budget
constant and marker set I cannot read or reach, tested by tests I would have to
invent alongside it, would be an untested claim dressed as a fix.