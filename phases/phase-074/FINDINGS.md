# Findings outside phase-074

## The self-hosted compiler's parser hits the call-depth limit near a thousand statements

**Evidence.** Compiling a flat program with 1000 `set` statements and 1000 `say`
statements through `bootstrap/compiler.rb` fails with:

```
Error: RuntimeError: Maximum call depth of 1000 reached while calling 'token_kind'
  --> bootstrap/compiler.rb:795:5
795 |     if token_kind(tokens, pos) is "id" then
```

The parser's `parse_statements` recurses once per statement, so a source with
more than the interpreter's call-depth budget of 1000 flat statements is refused
even though no expression or block is deeply nested. This is a pre-existing
limit (the same source fails with the compiler from `HEAD`); it is not caused by
the intern-table change. The name-heavy fixture in
`tests/bootstrap_selfhost_test.rs` stays below it so the phase tests byte
equality rather than the parser limit.

A future phase could make `parse_statements` iterative, or raise the limit for
the compiler, but that is a separate concern.
