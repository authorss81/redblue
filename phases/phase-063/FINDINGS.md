# Phase 063 — Findings

Work that this phase found but did not do. Each entry is anchored to a
`file:line` in the tree this phase left behind, so the auditor can turn it
into a phase without re-reading the code.

## 1. A `.table` cell holds a nested value on one line

**Where:** `src/repl/render.rs` — `cell_of` writes the cell with
`value.to_string()`, the single-line `Display`.

**What:** a row whose cell holds a list or a record prints it as the REPL
prints any value — `{"a": 1}` — on one line, so a table of records with a
nested structure in a cell is a table whose cell cannot be read. `.pretty`
can show it, but a table cannot show a nested table.

**Why it was not done here:** a nested table is a second feature (a table of
tables, with its own column rules and its own boundaries), not a rendering
detail of this one, and the phase's own definition of done says `.table`
takes a list of records.

**A phase would:** decide what a nested value does in a cell — refuse it, or
render it as a sub-table — and write it down in `SPEC.md` before it writes
the code.

## 2. The session's own answer to a program is still one line

**Where:** `src/repl/mod.rs` — `let line = format!("= {}", value);`, the
single-line `Display` every value the session prints goes through (the
debug sub-REPL has its own copy at `src/repl/debug_repl.rs`).

**What:** `.pretty`, `.json` and `.table` are opt-in. The answer a user gets
for typing `set people to [...]` is still one line, because the default
answer is a *value*, not a *shape*.

**Why it was not done here:** changing what the session prints by default
changes what every example in `examples/` and every tutorial step prints, and
the phase's goal was additive — three commands, no change to the default.
The tutorial steps and the `expect`-style output are all written against the
current one-line answer.

**A phase would:** make the default answer multi-line for a value that does
not fit on one line, and re-check every example, tutorial step and test that
asserts on the session's output.
