# Phase 026 — findings

Work outside this phase's scope, with file:line evidence. Not fixed here; the
auditor should promote these to phases.

## 1. BLOCKER — the word forms of comparison do not parse

`SPEC.md:271-275` and `docs/GRAMMAR.md:93-98` document comparison in English
words as the *primary* form, with the symbols as the alternative:

```redblue
if x is greater than 10
if x is less than or equal to 100
```

Neither parses today:

```
$ printf 'if 5 is greater than 3 then\n    say "y"\nend\n' > /tmp/x.rb
$ cargo run --bin rb -- run /tmp/x.rb
Error: ParserError: Expected Then but got Identifier("than")
  --> /tmp/x.rb:1:17
```

`greater`, `less`, `equal` are not keywords (`src/lexer.rs:14-69`, the
`KEYWORDS` table), so they lex as `Identifier` and `parse_comparison`
(`src/parser.rs:1207-1312`) stops after `Is`, leaving `greater` where the
parser wants `Then`.

Same class, same evidence:

| Form | Documented at | Actual |
|---|---|---|
| `is greater than` | `SPEC.md:272`, `GRAMMAR.md:97` | `ParserError: Expected Then but got Identifier` |
| `is less than` | `SPEC.md:273`, `GRAMMAR.md:95` | `ParserError: Expected Then but got Identifier` |
| `is less than or equal to` | `SPEC.md:273`, `GRAMMAR.md:96` | `ParserError: Expected Then but got Identifier` |
| `is greater than or equal to` | `SPEC.md:279` | `ParserError: Expected Then but got Identifier` |
| `is equal to` | `GRAMMAR.md:93` | `ParserError: Expected Then but got To` |
| `is not equal to` | `GRAMMAR.md:94` (implied) | `ParserError: Expected Then but got To` |
| `isnt` | `GRAMMAR.md:94` | `ParserError: Expected Then but got Identifier` |
| `&&` | `GRAMMAR.md:101` | `LexerError: Unexpected character '&'` |
| `\|\|` | `GRAMMAR.md:102` | `LexerError: Unexpected character '|'` |

What *does* work today, verified:

- `x is y` — `Parser::parse_comparison` `TokenKind::Is` arm, `src/parser.rs:1213-1218`
- `x is not y` — same block, `src/parser.rs:1226-1233`
- `and` / `or` — `parse_and` `src/parser.rs:1182`, `parse_or` `src/parser.rs:1157`
- `<`, `<=`, `>`, `>=`, `==`, `!=` — fixed by this phase

A fix needs `greater`/`less`/`equal` handling in `parse_comparison` plus
`isnt` as a keyword — a grammar change, which phase-026's brief does not open.
`README.md:18` shows `if count is greater than 10`, so this is the first thing a
reader of the README will hit.

Suggested phase: "Parse the word forms of comparison" — `must_touch:
["src/", "SPEC.md", "docs/"]`, severity blocker.

## 2. MAJOR — `push` is a declared builtin with no implementation

`src/stdlib.rs:62`:

```rust
globals.insert("push".to_string(), Value::Builtin("push".to_string()));
```

There is no `call_builtin` arm for `push` anywhere in `src/` (the only other
occurrence of the string is `src/repl/completer.rs:81`, the REPL completion
list). The program does not get "Unknown function"; it fails earlier, in the
parser:

```
$ printf 'set big to []\npush 1 to big\n' > /tmp/p.rb
$ cargo run --bin rb -- run /tmp/p.rb
Error: ParserError: Expected End but got Eof
  --> /tmp/p.rb:3:1
```

`to` is read as the assignment form, so `push 1 to big` parses as two
statements and the second has no expression. A REPL completion that offers a
name the language cannot execute is worse than no completion. Needs both a
parser form and a `call_builtin` arm, or the declaration removed.

Suggested phase: "Implement `push`, or stop advertising it" — `must_touch:
["src/"]`, severity major.

## 3. MINOR — `rb format` is not idempotent on a one-line `if`

```
$ printf 'if x is 3 then say "a"\nend\n' > /tmp/f.rb
$ cargo run --bin rb -- format --check /tmp/f.rb
File would be reformatted
$ cargo run --bin rb -- format /tmp/f.rb    # writes back
$ cargo run --bin rb -- format --check /tmp/f.rb
File would be reformatted                     # ...and still reports it
```

`format --check` says the file needs formatting, `format` writes byte-identical
output, and `--check` still says it. The bytes never converge, so a
format-in-CI gate can never go green on such a file. Pre-existing and
independent of this phase — reproduced on `main` before the lexer change, with
no comparison symbol in the source at all.

Related, and worth the same phase: `format` normalises `==` to `is` and `!=` to
`is not` (`src/formatter.rs:641-647`), which are the same operators, but it does
not do the reverse for the word forms because those do not parse (finding 1).