# Phase 030 — FINDINGS

Work discovered while implementing `unless` that does **not** belong to this
phase. Each entry is file:line anchored so the auditor can promote it.

## 1. `then` is documented as optional in SPEC.md but required by the parser

**Severity:** major (spec drift, affects `if` and `unless` equally)
**Evidence:**

- `SPEC.md:339` — `if condition` with no `then`, and `SPEC.md:368` —
  `unless is_valid` with no `then`.
- `src/parser.rs:645` — `self.expect(&TokenKind::Then)?` makes `then`
  mandatory for `if`. My `parse_unless()` at `src/parser.rs:712` does the same,
  because `docs/GRAMMAR.md:203` and `docs/GRAMMAR.md:220` both list `'then'`
  as non-optional.
- So every SPEC.md code block for `if`/`unless` that omits `then` is
  **rejected**. Verified:

  ```
  $ printf 'unless no\n    say "x"\nend\n' > u.rb && rb run u.rb
  Error: ParserError: Expected Then but got Newline
  ```

**Why not fixed here:** making `then` optional is a grammar change that alters
every `if` and `unless` program, and `docs/GRAMMAR.md` (the normative grammar)
disagrees with `SPEC.md` about it. Which document is authoritative is a
language-design decision, not a bug-fix. AGENTS.md rule 8 forbids redesigning
the grammar without a phase that says so.

**Suggested phase:** "Decide whether `then` is optional, and make SPEC.md,
docs/GRAMMAR.md and the parser agree" — whichever way it lands, one of the two
documents must change.

## 2. `unless` + `else` is undocumented in SPEC.md

**Severity:** minor (ambiguity, not a defect)
**Evidence:**

- `SPEC.md:364-370` documents `unless` with a single body and says nothing
  about `else`.
- `docs/GRAMMAR.md:220` gives `unless_statement` exactly one body and no `else`
  arm, so `else` is not part of the construct.
- I implemented the GRAMMAR.md reading: `unless … else … end` is a spanned
  `Error::Parser`, pinned by `edge_unless_rejects_else_because_its_body_has_no_alternative`
  in `tests/unless_test.rs`.

**Why not fixed here:** supporting `unless/else` means either a new AST node or
desugaring to `If { condition: not c }` with branches swapped, plus a decision
about whether `unless c … else … end` is sugar or a distinct form. Too big for a
correctness phase, and if the intent is sugar, GRAMMAR.md needs a note.

**Suggested phase:** "Decide and document whether `unless` accepts `else`; if so,
add the production to docs/GRAMMAR.md and implement it with a round-trip test
through `rb format`."

## 3. `tests/redblue_suite_test.rs` hand-maintains a block-keyword list

**Severity:** minor (silent false-positive harness bug, fixed in passing)
**Evidence:**

- `tests/redblue_suite_test.rs:28` held
  `const BLOCK_OPENERS: [&str; 9] = ["test", "if", "for", …]`.
- `opens_a_block()` (same file, line 32) drives the depth counter in
  `test_blocks()`. Any block keyword missing from the list makes the counter hit
  zero at the *inner* `end`, so the rest of the body — including its
  `expect` — is silently dropped.
- The symptom is not "harness bug", it is a wrong accusation about the author's
  test. This is what happened the moment I added a Redblue `unless` test:

  ```
  tests/test_control_flow.rb:248 `test "control: unless skips its body when the
  condition is true"` has no assertion - a smoke test proves nothing
  ```

  That test did have an assertion; the harness had thrown it away.

**Fixed in this phase** (`unless` added, and a comment at
`tests/redblue_suite_test.rs:37` now states the invariant and its failure mode),
because I could not otherwise add a Redblue-level test for the feature I was
building.

**Residual risk, not fixed:** the next block-opening keyword added to the
grammar needs the same one-line edit or the harness will quietly mis-report
suites. Two options worth a phase: derive `BLOCK_OPENERS` from the lexer instead
of hardcoding it, or have the harness walk the parsed AST rather than count
`end` lines.

## 4. `x is not y` reads oddly in a nested condition — verified NOT a bug

**Severity:** none (investigated, no action needed — recorded so it is not re-investigated)
**Evidence:** while writing `unless_uses_the_same_condition_grammar_as_if_and_picks_opposite_branches`
I mis-read `unless v is not "skipped"` and suspected a precedence bug in `src/parser.rs`.
It is not one. With `v` bound to `"skipped"`:

```
$ printf 'set v to "skipped"\nif v is not "skipped" then\n say "taken"\nelse\n say "skipped"\nend\n' > q.rb && rb run q.rb
skipped
```

and the same condition under `unless` takes the body, both of which are correct.
`SPEC.md:331` already documents the related `is not in` restriction. No change.