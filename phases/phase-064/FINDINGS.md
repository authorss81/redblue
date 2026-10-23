# Phase 064 — findings

## 1. `.format` cannot format an incomplete block

`src/repl/mod.rs` — `.format` formats `multiline_buffer.join("\n")` through
`crate::formatter::format`, which parses a whole program. While a block is being
typed the buffer is by definition missing its `end`, so the formatter answers
`Parser error: Expected 'end' but got Eof` and the command reports that rather
than printing a canonical fragment.

This phase deliberately did not add a fragment parser or synthetic `end`s:
the phase requires reusing the formatter unchanged ("none changes what the
formatter accepts"). A future phase that wants `.format` mid-block should decide
whether the formatter gains an explicit fragment mode, rather than the REPL
inventing one.

Reproduce:

```text
$ printf 'if yes then\nset   x    to   1\n.format\nend\n:quit\n' | ./target/debug/rb
..  ...
Error: could not format the buffer: Parser error: ...
```

## 2. Format-on-enter is per line, not per completed block

A block opener typed under `.format on` is left as typed because the formatter
rejects it in isolation; each later line is formatted only if it is a valid
program on its own. This is intentional (the toggle must not change what the
session accepts), but it means multi-line blocks are not wholesale normalised.
