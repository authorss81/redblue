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

## F5 — the specification documents forms the parser never builds

**Severity: major.** This is spec drift of exactly the kind the reviewer hunts
for, and it is why six branches of the rewritten formatter cannot be reached by
any test. Found by auditing which `Stmt`/`Expr`/`BinaryOp` variants the parser
can actually produce.

### The `for … from … to … [by …]` loop

`SPEC.md:383`, `SPEC.md:388`, `SPEC.md:427` and `docs/GRAMMAR.md:472` all
document it:

```
for each i from 1 to 10
for each i from 0 to 100 by 5
```

The parser does not build it. `src/parser.rs:694` is the **only** place a loop
statement is constructed, and it constructs `Statement::ForEach`. Confirmed
against the binary:

```
$ rb run t.rb          # for each i from 1 to 3 / say i / end
Error: ParserError: Expected In but got From
  --> t.rb:1:12
```

So `Statement::ForRange` is unreachable from source text. It is declared at
`src/parser.rs:134` and handled in four places that can therefore never run:
`src/analyzer.rs:108`, `src/vm.rs:454`, `src/linter.rs:96`,
`src/formatter.rs:274`.

### The relational operators

`docs/GRAMMAR.md:368-371` documents `is less than`, `is greater than`,
`is less than or equal to`, `is greater than or equal to`; `:438` tabulates
`<`, `<=`, `>`, `>=` at precedence 4. None of them exist:

```
$ rb run t.rb          # say 1 is less than 2
Error: AnalyzerError: Unknown variable 'less' / Unknown variable 'than'
$ rb run t.rb          # say 1 < 2
Error: LexerError: Unexpected character '<'  --> t.rb:1:7
```

The lexer has no `<` token at all, so `BinaryOp::{Less, LessEqual, Greater,
GreaterEqual}` — and `BinaryOp::In` — cannot be produced. Their arms at
`src/formatter.rs:628-631` and `:634` are unreachable, as is the `is`-only
reading that `src/formatter.rs:614-618` documents.

### Why this is recorded rather than fixed

The formatter is behaving correctly: it reports what it cannot parse instead of
guessing (`edge_a_for_from_to_loop_is_reported_not_silently_rewritten`,
`edge_a_symbolic_relational_operator_is_reported_not_guessed_at`,
`edge_a_comparison_containing_or_is_reported_not_silently_rewritten`). Making
`for … from …` or `is less than` work is a **grammar** change, which needs its
own phase under `AGENTS.md` §1 rule 8 — not a formatter bug-fix.

Note the behaviour is not uniform across the two forms, which is why the tests
are not one loop:

- `say 1 is less than 2` parses into three statements and formats fine; the
  program is already rejected by the analyzer, and formatting does not change
  the exit code, stdout, or error identity
  (`edge_a_word_comparison_fails_the_same_way_before_and_after_formatting`).
- `say 2 is greater than or equal to 1` does **not** parse, because `or` is a
  real keyword, so `equal to 1` lands where a function name is expected
  (`edge_a_comparison_containing_or_is_reported_not_silently_rewritten`).

Whoever implements the operators must extend
`format_keeps_grouping_for_unary_and_every_operator_level` and
`format_covers_every_statement_form` in `tests/formatter_test.rs`; the four
`edge_*` tests above will start failing, which is the intended signal.

