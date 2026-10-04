# Phase 015 — FINDINGS

Found while verifying phase-015. None of these are in scope for this phase
(`must_touch: ["src/"]`), so none were fixed here. Each is anchored to a line
I actually read.

## F1 — `modules/MathUtils.rb` does not parse, and does not run

**Severity: major.** `AGENTS.md` §2 states `modules/*.rb` are the language's
specification-by-example and "the gate runs them". This one cannot run.

`rb run modules/MathUtils.rb`:

```
Error: ParserError: Expected function name
  --> modules/MathUtils.rb:4:16
4 | constant PI to 3.14159
  |                ^
```

`modules/MathUtils.rb:4` and `:5` use `constant PI to 3.14159` /
`constant TAU to 6.28318`. There is no `constant` production in the grammar;
`set x to <expr>` is the assignment form per `docs/GRAMMAR.md`. The caret sits
on the numeric literal, i.e. the parser consumed `constant PI to` and then
found a value where it wanted a function name.

Pre-existing, not a formatter bug: `git log --oneline -1 -- modules/MathUtils.rb`
→ `0eb5f90 v0.1.1 - Build cleanup and warning fixes`. Phase-015 does not touch
this file. The formatter behaves correctly — it propagates the parse error
rather than guessing at a rewrite (`formatter_rejects_malformed_input_instead_of_guessing`).

Two candidate fixes, for whoever owns it:
1. Rewrite lines 4–5 as `set PI to 3.14159` / `set TAU to 6.28318`, or
2. Add a `constant` statement to the grammar as a language-design change —
   which needs its own phase per `AGENTS.md` §1 rule 8.

Also note the file ends `end` with no trailing newline (last byte is `d`),
which is why `rb format --check` would flag it even once it parses.

## F2 — `formatter_test.rs` corpus omits `modules/`

`tests/formatter_test.rs:13` iterates `["examples", "tests"]` only. Adding
`"modules"` would extend the corpus-wide idempotence and
`format_preserves_the_meaning_of_every_corpus_file` properties to the module
corpus, which `AGENTS.md` §2 designates as specification-by-example.

Blocked on F1: `modules/MathUtils.rb` would fail those property tests for a
reason unrelated to the formatter. Fix F1, then widen the list. The test
already skips a missing directory (`tests/formatter_test.rs:15-17`), so the
change is one string once the file parses.

## F3 — `rb format` does not rewrite the file in place

`src/lib.rs:129` reads the file and `print!`s the formatted result to stdout:

```rust
"format" => match fs::read_to_string(path) {
    Ok(source) => match formatter::format(&source) {
        Ok(formatted) => print!("{}", formatted),
```

So `rb format foo.rb` prints; `rb format foo.rb > foo.rb` truncates the file
first, so the shell idiom destroys it. `--check` is the only in-place-oriented
mode and it does not write.

This bit me during verification and cost a cycle: my first idempotence sweep
ran `rb format a.rb` expecting in-place rewrite, concluded "check fails after
format" for 20 files, and it was my harness that was wrong. A user will make
the same mistake. Options: add `rb format --write`, or make `rb format` write
in place and require `--stdout` to print. Either is a CLI change, not a
phase-015 change.

## F4 — no `rbops/` in the checkout

`ls rbops/` → `No such file or directory`; `.opencode/` is also absent. So
`./rbops/verify.sh phase-015`, the gate `AGENTS.md` §1 rule 2 makes the only
authority on whether work is real, could not be run. Noted in `REPORT.md`
rather than claimed as passing. The other three gates were run directly.
