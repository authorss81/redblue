# phase-017 — FINDINGS

Work outside this phase's scope, recorded per AGENTS.md rule 7. Each item is
file:line anchored.

## 1. `rbops/verify.sh` and `rbops/phases.json` are not in this checkout

```
$ ls rbops/verify.sh rbops/phases.json
ls: cannot access 'rbops/verify.sh': No such file or directory
ls: cannot access 'rbops/phases.json': No such file or directory
```

The fourth gate named in the phase prompt cannot be executed here, and
phase-017's `must_touch` entry cannot be read. Work therefore landed in
`src/` (`src/lsp.rs`, `src/lexer.rs`, `src/lib.rs`, `src/error.rs`) plus
`tooling/vscode/`, which satisfies every `must_touch` list in AGENTS.md
section 3.5 that includes `src/`. **The gate still has to be run by the
dispatcher**; REPORT.md records it as not run, not as passed.

## 2. `src/linter.rs:46-49` reports lint findings at `0:0`

```rust
self.warnings.push(LintWarning {
    message: ...,
    line: 0,
    column: 0,
});
```

Every whole-program lint finding (unused variable, unused function, …) is
reported at line 0, column 0, which no editor can place. `src/lsp.rs` folds
such a position to `1:1` (`fn known_position`) rather than emitting `0:0` into
a JSON diagnostic, but the real fix is for the linter to carry the statement's
span. Out of scope here; it is a linter change, not a tooling one.

## 3. `tooling/repl/README.md` documents a directory that does not exist

`tooling/repl/README.md:46-57` gives the layout `tooling/repl/src/{main,lexer,
evaluator,completer,history,output,commands}.rs`. The REPL actually lives at
`src/repl/mod.rs` (397 lines), `src/repl/commands.rs`, `src/repl/completer.rs`
and `src/repl/history.rs` (730 lines total), and it *does* implement `:quit`,
`:history` and tab completion (`src/repl/commands.rs:32-35`). So the README is
misplaced and misleading rather than aspirational. It belongs to a REPL phase;
this phase rewrote `tooling/vscode/README.md` only, and added one factual
pointer to it.

## 4. `Cargo.lock` is gitignored for a library crate

`.gitignore:5` lists `Cargo.lock`. Nothing is broken today because every
dependency resolves to the same tree in CI, but a released library with an
unpinned lock can pick up a new minor of `reqwest`/`chrono`/`regex` without
review. Deliberate or accidental is a maintainer decision → promote to a
phase if unintended.

## 5. No LSP transport yet

`rb diagnostics <file>` is a one-shot command: it reports every problem in a
file and exits. A real language server needs JSON-RPC over stdin/stdout
(`initialize`, `textDocument/didOpen`, `publishDiagnostics`,
`textDocument/completion`). `src/lsp.rs` is the seam for that — `Diagnostic`,
`Severity`, `diagnostics()` and `keyword_pattern()` are the pieces a server
would need — but no transport is written. Future phase, `src/repl/` is not
involved.
