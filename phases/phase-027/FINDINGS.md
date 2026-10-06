# Phase 027 — findings

Work outside this phase's scope, with evidence. Not fixed here; the auditor
should decide whether any of it becomes a phase.

Phase 026 filed §1 "the word forms of comparison do not parse", listing seven
spellings. This phase fixed five of them. One remains (`isnt`), three facts
turned up while testing, and one stale comment is noted. All are below.

## 1. `isnt` is still not a spelling of `is not`

`docs/GRAMMAR.md:94` lists three spellings of inequality:

```
neq_op       = 'is not' | 'isnt' | '!='
```

`is not` and `!=` work. `isnt` does not:

```
$ printf 'set x to 5\nif x isnt 3 then\n    say "a"\nend\n' > target/tmp/i.rb
$ cargo run --quiet --bin rb -- run target/tmp/i.rb
Error: ParserError: Expected Then but got Identifier("isnt")
  --> target/tmp/i.rb:2:6
2 | if x isnt 3 then
  |      ^
```

`isnt` is not in the `KEYWORDS` table (`src/lexer.rs:14-68`), so it lexes as
`TokenKind::Identifier("isnt")` and `parse_is_operator`
(`src/parser.rs:1250-1294`) has no arm for it: an identifier that is not the
head of a complete `equal to` / `greater than` / `less than` phrase falls
through to `BinaryOp::Equal`, leaving `isnt` to be read as the right operand.
`3` is then a stray token and the `if` fails.

Fixing it is a one-arm change (`Identifier(w)` where `w == "isnt"` →
`NotEqual`) of exactly the kind this phase just made for the other five words,
but it is a seventh spelling that this phase's goal does not name, so it is
left for the auditor rather than smuggled in.

## 2. `expect x is equal to be <value>` cannot be written

`is equal to` and `expect … to be …` both want a `to`, so the operator eats
the one `expect` needs and the literal becomes the right operand:

```
$ printf 'set x to 5\nexpect x is equal to be yes\n' > /tmp/x.rb
$ cargo run --quiet --bin rb -- run /tmp/x.rb
Error: ParserError: Expected To but got YesNo(true)
```

`expect x is equal to 5 to be yes` is fine — the ambiguity is only when `be`
(or any bare word) follows the `to` of `is equal to`. `expect 5 is greater than
3 to be yes` and `expect 5 is greater than or equal to 3 to be yes` both parse,
because those phrases end at `than`, not at `to`.

This is a grammar question, not a parser bug: `expect` is defined
(`AGENTS.md` §Testing) as `expect <expr> to be <value>`, and `is equal to` is
defined (`docs/GRAMMAR.md:93`) as an operator. Re-opening either is out of this
phase's scope. `tests/test_comparisons.rb` works around it with `set … ` plus
`if`, and says so in a comment above the block.

## 3. `rb format --check` now normalises the word forms to the symbols

`Formatter::format_binary_op` (`src/formatter.rs:635-651`) prints `Less` as
`<`, `Greater` as `>`, `LessEqual` as `<=`, `GreaterEqual` as `>=`,
`Equal` as `is` and `NotEqual` as `is not`. It prints the AST it is given, and
by this phase the word forms build the same AST as the symbols, so a file
written in English is rewritten in symbols:

```
$ printf 'set x to 1\nif x is greater than 0 then\n    say "a"\nend\n' > /tmp/f.rb
$ cargo run --quiet --bin rb -- format /tmp/f.rb
set x to 1
if x > 0 then
    say "a"
end
$ cargo run --quiet --bin rb -- format --check /tmp/f.rb
File would be reformatted
```

Before this phase that file was a `Format error: Parser error: Expected Then
but got Identifier("than")`, so it failed either way — nothing regressed, and
the output is semantically identical (`tests/formatter_test.rs:177`
`format_preserves_the_meaning_of_every_corpus_file` and `:165`
`format_is_idempotent_over_the_whole_corpus` both still pass over
`tests/test_comparisons.rb`, which now contains word forms). But a formatter
that rewrites `README.md:18`'s own example into `>` is arguably wrong for a
language whose thesis is readable English, and `SPEC.md:271-275` presents the
words as the primary form. Choosing which spelling is canonical is a formatter
design decision belonging to whichever phase owns `src/formatter.rs`, not here.

## 4. A `test` block cannot see variables set above it

Found while writing the Redblue blocks, and confirmed to be pre-existing and
symbol-independent:

```
$ printf 'set x to 5\ntest "t"\n    expect x > 3 to be yes\nend\n' > /tmp/s.rb
$ cargo run --quiet --bin rb -- test /tmp/s.rb
  Error: RuntimeError: Unknown variable 'x'
```

The same program with `x > 3` fails exactly the same way, so this is not about
the word forms. It may well be deliberate isolation — a `test` block is meant
to be self-contained — but it means a test cannot set up data and then assert
on it, which is the shape most of `tests/*.rb` wants. Not investigated: the
execution of a `test` block lives in `src/testing/`, which this phase does not
touch.

## 5. A comment in `src/formatter.rs` is now false

`Formatter::format_binary_op`'s doc comment (`src/formatter.rs:629-633`) says:

> Only `is` and `is not` are spellings the lexer produces today: `%` is the
> remainder operator, and `<`, `<=`, `>`, `>=` and `in` have no token at all, so
> those variants cannot reach this formatter from source text …

That was true until phase 026 added the operator arms to the lexer; `<`, `<=`,
`>`, `>=`, `=` and `!=` all lex now, and this phase added the six word
spellings, so twelve spellings reach that function rather than two. The code is
right and the tests are green — only the comment is stale, and only the author
of the neighbouring change could say which spelling it *should* print. Left
alone here, and it is the same decision as §3.

`BinaryOp::In` is still unreachable from source text, so that half of the
comment remains true.