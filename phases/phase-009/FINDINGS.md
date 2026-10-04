# Phase 009 — FINDINGS

Work that did not belong to this phase, recorded for the auditor.

## 1. `modules/MathUtils.rb` does not parse — pre-existing, unrelated

```
$ ./target/debug/rb run modules/MathUtils.rb
Error: ParserError: Expected function name
  --> modules/MathUtils.rb:4:16
4 | constant PI to 3.14159
  |                ^
```

Line 4 uses `constant PI to 3.14159`, and `constant` is not a keyword in
`src/lexer.rs` or a statement in `src/parser.rs`. Verified pre-existing: I
stashed this phase's diff, rebuilt, and got the identical error at the identical
span, then restored the diff.

AGENTS.md section 2 says `modules/*.rb` is specification-by-example, so this file
is currently a specification the parser does not meet. Either add a `constant`
declaration or correct the example. Out of scope here: it is a parser/grammar
question, not lexer robustness.

## 2. `rbops/` is not in the checkout, so `verify.sh` could not be run

`ls rbops` → `No such file or directory`. Only `.github/workflows/ci.yml` is
present. Hard rule 1 forbids creating or editing anything under `rbops/`, so the
fourth gate was not run. I ran the other three plus every `examples/*.rb` by
hand; see the Gates table in `REPORT.md`. If `verify.sh` performs checks beyond
build/test/clippy/examples, they are unverified by me.

## 3. A raw newline does not terminate a string

`say "a\nb"` lexes to one string containing a newline. With this phase's change
such a string is still accepted as long as a closing quote appears later in the
file, and the *unterminated* error is reported at the opening quote rather than
at the end of input. Reporting both the opening quote and the position where the
input ran out would need a second span, and terminating a string at a newline
would change the string grammar. Both are their own phase.

## 4. No bound on source size or string length

`Lexer::new` (src/lexer.rs:123) allocates one `char` per source character, so
memory use is linear in input. The lexer neither recurses nor loops without
advancing, so it cannot be made to hang by malformed input — but there is no
maximum file size and no maximum string length that produces a clean error
instead of an allocation failure.

## 5. Combining marks terminate an identifier

`read_identifier` (src/lexer.rs:237) accepts `char::is_alphanumeric() || '_'`, and
`U+0301 COMBINING ACUTE ACCENT` is category `Mn`, which is not alphanumeric. So
an identifier written in decomposed form (`e` + `U+0301`) is split into `e`
followed by a lexer error on the mark. Needs `char::is_mark()` in the same
predicate. Strings are unaffected and round-trip correctly.