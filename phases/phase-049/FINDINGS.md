# phase-049 — findings

Out-of-scope observations recorded here so the auditor can promote them to
phases rather than have them smuggled into phase-049.

## 1. The nine freed words are identifiers now, not features

`async`, `wait`, `parallel`, `done`, `when`, `that`, `new`, `ask`, `are` were
reserved by the lexer with no parser arm anywhere in `src/parser.rs`. This phase
frees them, so they lex as identifiers. `SPEC.md`'s `## Asynchronous
Programming`, `§ Instantiation` and `§ Constructor` sections say the forms are
not implemented, and `docs/GRAMMAR.md`'s `when_statement` (§4.3),
`async_function` / `async_call` / `parallel_block` (§4.8) and `object_literal`
(§7) are **kept as the shape a future implementation has to match**, each under
a **Not implemented** note. `object_literal` is no longer an alternative of
`primary` (§5.1), and no worked example in either document writes `new Person`.

Nothing is implemented for them here. If a future phase wants `async`/`wait`,
it must (a) re-add the words to `KEYWORDS` and `TokenKind` in `src/lexer.rs`,
(b) give `src/parser.rs` a real arm for each — otherwise
`every_reserved_keyword_has_a_parser_arm` in `tests/keyword_reservation_test.rs`
fails — (c) add a snippet to the `REACHABLE` table, which must write the word it
is filed under and survive a real `parse`, or
`edge_every_reserved_keyword_parses_in_the_position_its_grammar_gives_it` fails,
and (d) restore the SPEC/GRAMMAR text or
`the_documented_keyword_set_is_the_lexers_keyword_set`,
`spec_no_longer_promises_the_forms_this_phase_freed` and
`a_freed_word_is_only_spelled_as_a_keyword_under_a_not_implemented_note` fail.

`when` (pattern matching), `this`/`that` (object reference), `new` (object
instantiation), `is`/`are` (type checking) and `error` are listed in SPEC.md's
keyword appendix as *aspirational* features. Only `is` and `this` are
implemented. `when`, `new`, `that`, `are` and `error` are now ordinary
identifiers. A phase that implements object instantiation would re-reserve
`new`; a phase that implements pattern matching would re-reserve `when`.

## 2. `verify.sh` was not run

It lives at `../rbops/verify.sh`, outside this project checkout. The resume
instructions for this run instructed that the RBOPS pipeline not be inspected
and that no tool call be spent on it, so it was not invoked. The four gates that
can be run from inside the checkout — `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`, and
`rb run examples/*.rb modules/*.rb` — were run and their real output is quoted
in `REPORT.md`. `REPORT.md` claims nothing about `verify.sh`.

## 3. `expected_name` wording is shared and slightly awkward in one position

`import if` reports `Expected module name, found reserved word 'if'`. That is
accurate and is a strict improvement on `Expected module name`, which named no
word at all, but the phrasing was chosen for `set <word> to 1` and reads oddly
for an import. Thirteen call sites in `src/parser.rs` share it. A future
parser-diagnostics phase could give each site its own phrasing; this phase left
it alone to keep the diff on-topic.

## 4. `ROADMAP.md` still writes forms that do not parse

§ Object-Oriented writes `set my_dog to new Dog` and § Async HTTP Request writes
`async set response to wait network.get(...)`, neither of which parses, and §
Async HTTP Request also uses a `formats.parse_json` that `SPEC.md` says does not
exist. These are roadmap entries for unimplemented work, and the `formats` one
was already stale before this phase, so the phase left them alone. A roadmap
phase should either mark them aspirational or rewrite them against what parses.

## 5. A `TokenKind::By` variant existed with no producer

`src/lexer.rs` carried a `TokenKind::By` variant with no `KEYWORDS` entry and no
lexer arm that produced it; `by` is read positionally from its identifier text
(`Parser::at_range_step_marker`), so the variant was reachable only from a dead
branch. Removed by this phase, and the reason is written down at
`Parser::at_range_step_marker` so nobody promotes `by` to a reserved word
without noticing that it would refuse `to can grow(by)`.