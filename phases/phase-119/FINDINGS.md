# Phase 119 — findings

Work that this phase found but deliberately did not do. None of it belongs to
phase-119's scope; each is recorded so the auditor can decide whether it is a
phase.

## 1. `rbops/verify.sh` and `rbops/phases.json` are not in the checkout

`ls rbops` → `No such file or directory`.
`find . -maxdepth 2 -name verify.sh -not -path './target/*'` → no results.

The fourth gate could therefore not be run from inside this phase, and its
result is not claimed in `REPORT.md`. What was run instead:
`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets` (1802 passed, 0 failed) and every
`examples/*.rb` through the rebuilt binary.

**Needed:** either the `rbops/` tree checked out alongside the project, or a
phase note saying the gate runs in CI only.

## 2. `:restore <name>` does not accept a session name

`src/repl/commands.rs` gives `:restore` the argument `<filename>`, and
`src/repl/mod.rs` reads it as a path. `:session <name>` now records a name in
`~/.redblue/sessions`, and nothing joins the two: a session that was named is
still restored by a file name the user chose at `:save`.

Making `:restore` fall back to the name registry when the argument names
nothing on disk is a small change, but it changes what `:restore` means for a
user whose file and whose session name collide, so it wants its own phase.

**Anchor:** `src/repl/commands.rs` (`Restore(String)`), `src/repl/mod.rs`
(`restore_session`), `src/repl/labels.rs` (`used`).

## 3. The `:sessions` window depends on a clock that can jump

`src/repl/labels.rs:used` filters on `now - stamp < WEEK_SECONDS` with `now`
from `labels::now_epoch`. A machine whose clock steps backwards by more than a
week makes every recorded name vanish from the listing; one that steps forward
expires them. The filters are deliberately absolute (`stamp <= now`), so a
future-dated record is never evidence of anything, but that does not help the
first case.

**Anchor:** `src/repl/labels.rs` — `used`, `now_epoch`.

## 4. `$_` is not highlighted or completed as a language name

`src/repl/highlight.rs` classifies tokens by the lexer's own rules, and `$_` is
an ordinary `Identifier` to it, so it is coloured as a variable once it is
bound and is completed through `Vm::user_names`. Before a session has produced
a value there is nothing to complete and nothing to highlight, which is
correct — but a user who has just typed `$` gets no suggestion that `$_` exists.

Deciding whether the last value is a *language* name (and therefore belongs in
the highlighter and the completer ahead of the first value) is a language
surface question, not a REPL fix.

**Anchor:** `src/repl/highlight.rs`, `src/repl/completer.rs`,
`src/lexer.rs:LAST_VALUE_NAME`.