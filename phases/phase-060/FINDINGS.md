# Phase 060 — FINDINGS

Work that this phase found but did not do, each one with the place to look. None
of these belongs to phase-060: they are recorded so the auditor can decide
whether to promote them to phases.

## F1 — `./rbops/verify.sh` cannot run in this checkout

`ls rbops` → *No such file or directory*, and `./rbops/verify.sh phase-060` →
*No such file or directory*. The tree carries `phases/`, `AGENTS.md` and
`src/`, and the pipeline that dispatches the phase lives elsewhere.

Recorded identically by phases 057, 058 and 059, so this is the third phase to
hit it and not a one-off. What was run here instead: `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets` (1781
passed, 0 failed) and `cargo test --doc` (2 passed), plus every file in
`examples/` and `modules/` through `rb run` by hand.

**Needed:** a decision about whether a phase is allowed to claim the gate when
the gate is not present in the tree it was given, or whether the dispatching
pipeline must mount `rbops/` into the working directory. Right now three
reports in a row say the same thing and a reviewer has to take it on trust each
time.

## F2 — A filename typed inside a Redblue program is still typed blind

TAB now completes the *argument* of `:load`, `:save`, `:run`, `:restore` and
`.export` (`src/repl/completer.rs`: `path_argument` + `complete_path_in`). What
it does not do is complete anything that names a file *inside a program*, and
there is a short list of those in the tree, all verified on this tree:

- `files.read("…")`, `files.write("…", …)`, `files.lines(…)` and the rest of the
  files module (`AGENTS.md` §Standard Library) take paths as string arguments to
  a call, not as arguments to a command, so the gate added here deliberately
  leaves them alone.
- `import MathUtils` names a *module*, not a path, and the parser rejects a
  quoted one: `printf 'import "modules/MathUtils"\n' > t.rb && rb run t.rb`
  answers `ParserError: Expected module name`. The module then resolves through
  `module_search_paths` (`src/interpreter.rs:516`) — `modules/<name>.rb`,
  `./modules/<name>`, `<name>` — so what a user types is a *name*, and the names
  that resolve are the `.rb` files in `modules/` (2 in this checkout:
  `MathUtils.rb`, `SuiteKit.rb`). TAB could offer those; it cannot do it today,
  because the completer knows nothing about the module search path.

Completing a string argument *inside a Redblue expression* is a different piece of
work from completing a command argument: it has to know where the string ends,
what is inside it, and that the user is typing text rather than a filename at
all. That is why it is not in this phase.

**Needed:** a phase that extends the completer to string arguments of the files
module and to `import` names, with the three properties this one has — the offset
the completion replaces, the quoting rule, and a test per shape. `path_argument`
is the seam: it answers "where is the cursor in something that is a path", and a
second predicate over expressions and over the module table would reuse it.

## F3 — An exported block has no indentation, because the line that ran had none

The read loop trims each line before running it: `let input = line.trim();` at
`src/repl/mod.rs:667`, and `execute_line(&expanded)` is handed that trimmed
text. So a block typed with indentation

```
repeat 2 times
    say "tick"
end
```

is recorded — in the transcript *and* in the history, which takes `line.trim_start()`
at `:676` — as

```
repeat 2 times
say "tick"
end
```

Redblue does not read indentation, so nothing about the run changes. But an
exported block that a reader can see was indented and is not reads as a defect
of the export, and `transcript::a_block_is_one_exchange_holding_every_line_it_was_typed_on`
pins the *with*-indentation shape only because it builds a `Transcript` directly
rather than going through the read loop.

**Needed:** a decision, not code. Either leave it — the transcript is the source
that ran, and the history has always been trimmed the same way — or record the
line as typed rather than as trimmed, which changes `:history`, `!!` and
`keeps_argument_whitespace` along with it. The second is a phase of its own and
is not this one.
