# Phase 063 — Findings

Work that this phase found but did not do. Each entry is anchored to a
`file:line` in the tree this phase left behind, so the auditor can turn it
into a phase without re-reading the code.

## 1. The hand-written terminal-width table in `src/error.rs` is incomplete

**Where:** `src/error.rs:263` (the `WIDE` range table) and `src/error.rs:296`
(`char_cells`), reached from the new `pub(crate) fn text_cells` at
`src/error.rs:334`.

**What:** `WIDE` covers the East Asian Wide/Fullwidth blocks and three emoji
blocks (`U+1F300–U+1F64F`, `U+1F680–U+1F6FF`, `U+1F900–U+1F9FF`), but a
terminal draws several other emoji two cells wide: `U+2B50` (⭐), `U+2728`
(✨), `U+1F004`, `U+1F0CF`, and the regional indicators `U+1F1E6–U+1F1FF`.
`char_cells` answers 1 for all of them, so a line drawn with a caret under
one of them is one cell short, and a `.table` column holding one is padded
one cell short.

**Why it was not done here:** the width table belongs to the error renderer
(phase-059), not to this phase, and the honest fix is not more ranges — it is
`unicode-width`, which is already in `Cargo.lock:1207` as a transitive
dependency of `rustyline` but is not a direct dependency of this crate
(`Cargo.toml`, `[dependencies]`). Promoting it to a direct dependency is a
decision about the crate's dependency surface, and this phase is about
printing values.

**A phase would:** add `unicode-width` to `[dependencies]`, replace `WIDE` and
`char_cells` with it, and keep `text_cells` as the one place the REPL asks
how wide a text is drawn.

## 2. A `.table` cell holds a nested value on one line

**Where:** `src/repl/render.rs:188` — `cell_of` writes the cell with
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

## 3. The session's own answer to a program is still one line

**Where:** `src/repl/mod.rs:1370` — `let line = format!("= {}", value);`, the
single-line `Display` every value the session prints goes through (the
debug sub-REPL has its own copy at `src/repl/debug_repl.rs:380`).

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
