# Findings — phase-046

Work found while doing phase-046 that belongs to another phase. Recorded here so
the auditor can promote them with file:line evidence. None of it was done in this
phase.

## 1. The two VMs' runtime `Unknown variable` carries no suggestion

- `src/interpreter.rs:1832` — `Error::Runtime(format!("Unknown variable '{}'", name), self.span())`
- `src/bytecode/vm.rs:1717` — the same, in the bytecode VM

Phase-046 adds `; did you mean 'x'?` in the analyzer only
(`src/analyzer.rs:244`, `add_unknown_variable`). The runtime path is reached where
the analyzer did not see the read — a `might fail` body, or a name the walk
deferred — and it still reports the bare name.

It was not done here on purpose: `differential_test::every_corpus_program_agrees_between_the_two_vms`
compares the two engines' outcomes, so a hint added to one VM and not the other
would show up as a divergence that is an artefact of the change rather than a
defect. Doing it needs both sites in one change.

Each VM's locals are `Vec<CapturedScope>` where `CapturedScope = IndexMap<String, Value>`
(`src/value.rs:23`) — insertion-ordered, so enumerating the names for candidates
is deterministic in both. `redblue::suggest::did_you_mean` takes them as they come.

Severity: minor. Same severity as the phase this came out of.

## 2. Builtin and global names are not suggestion candidates

`src/analyzer.rs:246` builds the candidate list from `self.scopes` plus
`crate::lexer::Lexer::keywords()`. `stdlib::is_builtin` / `stdlib::is_global`
(`src/analyzer.rs`, `builtin_global`) names are not in it.

So a program with a typo'd name of its own — `set coun to 1`, with no `count`
anywhere — gets no hint, even though `count` is not a builtin either, so this is
only visible where a builtin name is the nearest match. The nearest name is
computed against what the program actually declares, which is the conservative
choice: it cannot suggest a name the program does not have. Adding the builtin
table would widen it.

Severity: minor.

## 3. An unclosed block is still reported as a single line

`src/parser.rs:417` — `fn expect(&mut self, kind: &TokenKind)` reports
`Expected End but got Eof` at the end-of-input token's span, so `set x to 1\nif x then\nsay 2\n`
reports at 4:1 with a caret on a line the file does not have.

Phase-046 gave `Span` an end and taught `render` to draw a range
(`src/error.rs:180` onwards), but a block's range needs the *opening* keyword's
position, which `expect` does not have. There are ten `self.expect(&TokenKind::End)?`
call sites (`src/parser.rs:809, 843, 1012, 1036, 1151, 1356, 1455, 1576, 1651, 1697`),
each in a different block form (`if`, `for`, `repeat`, `object`, `module`, `test`,
function literal, record). Giving each its own opening span is a parser change, not
a diagnostic change, and it would touch every block form — too wide for a phase
whose evidence is about suggestions.

Severity: minor.

## 4. A mistyped keyword in keyword position has no suggestion

```
$ printf 'iff 1 is 1 then\nsay "y"\nend\n' > h.rb && ./target/debug/rb run h.rb
Error: ParserError: Unexpected token Then
  --> h.rb:1:12
```

Phase-046 covers the case where a mistyped keyword lands where a *name* is
expected (`sya "hi"` → `did you mean 'say'?`), because the analyzer sees it as an
unbound read. In keyword position the parser fails first, at `Expected Then but
got Identifier("iff")`, and the parser does not know what word would have fitted
there — suggesting `if` for `iff` needs the parser to carry a candidate set per
expected token kind.

Severity: minor.

## 5. `Span::extended_to` has one production caller

`src/error.rs:35` is called from exactly one place: the lexer's unterminated
string (`src/lexer.rs`, `Lexer::unterminated`). Every branch of `render_range` is
golden-tested through the constructor (`tests/diagnostics_test.rs`,
`test_a_two_line_range_draws_two_lines` and the five tests beside it), but a
second production range — finding 3 above is the obvious candidate — is the honest
proof that the rendering matches what the language needs.

Severity: informational.
