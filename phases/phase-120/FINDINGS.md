# Phase 120 — Findings

## `:plugin` — considered and deliberately declined

Ipp's `:plugin` (REPL_AUDIT.md 2.33) loads code into a live session at runtime.
It is **not** implemented by this phase, on purpose.

- It executes arbitrary code with no gate: the code is not parsed against the
  language invariants, not analyzed, and not limited by the REPL's line/step
  budgets before it runs. AGENTS.md section 1, rule 2 says a phase may never
  weaken a gate, and building a runtime code-loading surface is the strongest
  possible form of that weakening.
- It also crosses the phase's single concern (rule 7): this phase adds two
  read-only conveniences (`:pipe`, `:bind`), not a dynamic code loader.

A later phase must not re-propose `:plugin` without first addressing the gate
question above. This phase adds no loader, no eval of untrusted source, and no
way for `:bind`/`:pipe` to reach one.

## `:pipe` scope

`:pipe` runs the command the user typed with the session's last result on its
standard input and prints both output streams plus the exit status. The help
text says it is for **read-only inspection** and is not a shell replacement;
`!<command>` is the general host-shell escape that already exists. `:pipe` is
intentionally not extended into a shell replacement by this phase. The read-only
scope is a documented usage contract, not a sandbox: arbitrary host commands are
still run as typed, and building a sandbox around the host shell is a separate
concern that this phase does not attempt.
