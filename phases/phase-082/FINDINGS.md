# Phase 082 — Findings

Findings this phase did **not** fix, each with the file:line it was measured at.
Nothing here was worked around, and nothing here was used to justify a weaker
test.

## 1. `SPEC.md:628` — `give back to` with a block body is still a parser error

```
ParserError: Expected 'end' but got Eof
```

The block is the closure example under "### Closures":

```redblue
to make_counter(start)
    set count to start
    give back to
        give back count
        add 1 to count
    end
end
```

`SPEC.md:614` says a function literal "can be held in a name, passed as an
argument, stored in a list or a record, and read back out again", and a literal
written as a multi-line block with its own `end` is exactly what the block above
attempts to `give back`. `tests/function_literal_test.rs` covers the one-line
form; nothing covers this one.

This is a question about the language, not a false sentence in a document:
either the parser learns the block form after `give back to`, or SPEC.md's
example changes. AGENTS.md § 1.7 — one phase, one concern — and § 1.8 — no
grammar redesign without a phase that says so. Left in
`tests/doc_blocks_known_fail.txt` with the exact stderr, so it fails loudly
rather than rotting.

## 2. `HANDOUT.md:140` is not a false statement, it is a block that needs a file

```
IoError: Failed to read 'data.txt': No such file or directory (os error 2)
```

The `files` example reads `data.txt` and writes `output.txt`; the block creates
neither and a clean checkout has neither. The binary is answering correctly
about a file that is not there — AGENTS.md § 3.1.4 forbids a test that depends
on a file outside a temp dir, so it is classified FRAGMENT with the reason
written beside it rather than executed.

A maintainer who wants it executed needs the block to create what it reads
(`files.write("data.txt", …)` first), which is a documentation change this
phase could have made and did not, because the current example is the shape a
reader actually writes.

## 3. Two of the finding's five claims were already fixed on `main`

Recorded because a future phase that reads the phase prompt rather than the
tree will re-do work that is done, and may "fix" something that is already
right.

- `KNOWN_GAPS` at `tests/stdlib_module_docs_test.rs:709` holds only `filter` and
  `reduce`. The five stale exemptions the finding names are rows of
  `answerable_builtins()` — `tests/stdlib_module_docs_test.rs:582`, `:597`,
  `:602`, `:607`, `:628` — each pinned to an exact stdout.
- `SPEC.md:1068` is the block its own prose introduces as "Not Redblue. Nothing
  below parses." The single-quote example the finding cites is `SPEC.md:1256`,
  in prose that already says it is a `LexerError`.

Every line number in the finding is also off by between 6 and 38, and two of the
`edge_*` test names it quotes do not exist under those names.

## 4. `verify.sh`'s test-policy scanner reads the working tree, not history

**Corrected by measurement. The previous entry in this file was wrong.**

The previous attempt recorded the scanner returning `0 rust / 0 edge_* / no
failure assertion` against a file with 11 `#[test]` functions and four failure
assertions, and concluded the scanner was broken. It was not. It reads the diff
against `HEAD`, and the previous attempt's work had been committed into the
resume checkpoint `cd7aed7`, so the working tree was clean:

```
$ ../rbops/verify.sh phase-082          # immediately after the checkpoint landed
== diff hygiene
  FAIL  no changed files detected against HEAD
  FAIL  diff too small: +0 (min +5)
  FAIL  no implementation change - must_touch requires a change under one of: tests/
== test policy
  FAIL  no test asserts a failure is produced - mandatory
  FAIL  test quota not met: 0 rust (need 3) and 0 redblue (need 2)
  FAIL  no test named edge_* - mandatory
```

Every `0` in the test-policy block was the **same fact** as the diff-hygiene
failure: no changed files. With the work uncommitted the same script printed

```
== diff hygiene
  PASS  diff substance: +974 -311 (min +5)
  PASS  implementation touched tests/ (must_touch: tests/)
== test policy
  PASS  failure-asserting test present: 6
  PASS  test quota met: 12 rust / 0 redblue (need 3 or 2)
  PASS  edge case test present: 11
```

So there is no conflict between the two checks and nothing to fix in
`verify.sh`. The rule for whoever runs a phase next: **the gate runs before the
dispatcher commits**, and a phase whose work is already committed is measured
against an empty diff. Phase-081 recorded the identical `0 / 0 / 0` against the
identical cause.

## 5. `tests/doc_blocks_*.txt` were keyed to line numbers that no longer resolved

The bucket files match blocks on `<doc>:<line>`. At the start of this phase
**49 of the 101 keys named no block at all** — `SPEC.md:117`, `SPEC.md:1006`,
`HANDOUT.md:160`, `docs/GRAMMAR.md:541` and 45 others — and four blocks were in
no bucket, so `cargo test` failed on
`every_documentation_block_is_classified_exactly_once`,
`running_every_block_agrees_with_the_bucket_it_is_in` and
`edge_a_known_fail_recording_that_is_wrong_is_rejected`.

The keys last matched the documents at `bf9ffc3`. `e008f0b` (phase-089) moved
17 lines in `SPEC.md` and 21 in `docs/GRAMMAR.md` without re-pointing them. This
is the third time a document edit has silently invalidated the whole file.

Rebuilt by measurement this phase: all 105 blocks extracted, written to a file,
run under `rb run` **and** `rb compile`, and classified from the result.

**Follow-up for the auditor:** the fix is a block *content* hash in place of a
line number. A key that survives an edit above it removes this whole failure
class, and it is a change to `tests/doc_blocks_test.rs`'s matcher and the two
data files — no production code. Not done here because it is a rewrite of
another phase's harness and this phase's mandate is the documents.

## 6. `KNOWN_GAPS` is not gated, so it can grow silently

`tests/stdlib_module_docs_test.rs:709` exempts `filter` and `reduce`. Nothing
fails if a third name is added there without a test that calls it and asserts
its value. Both names *are* still refused, and both are pinned by
`edge_the_remaining_known_gaps_are_documented_as_missing` in
`tests/doc_accuracy_test.rs`, so a name that starts working is caught — but a
name that is added as an exemption for a working builtin is not.

This is the finding's worst sentence, in the one direction it is still open: the
list is no longer stale in the permissive direction, but it is unpoliceable. A
gate would be a test asserting every `KNOWN_GAPS` entry is refused *and* named in
some `answerable_builtins()`-style row's negative case.

## 7. `verify.sh` reads a `REPORT.md` this phase may not write, and does not re-sync

`verify.sh` reads `/home/runner/work/rbops/rbops/phases/phase-082/REPORT.md` —
one level above the project root. The dispatcher copies the in-checkout
`phases/phase-082/REPORT.md` there at phase start, which is how the previous
attempt's report was found. It does **not** re-copy after that, so the report
honesty check still reads the previous attempt's number:

```
== report honesty
  WARN  report claims 1555 passing, actual run has 1808 - under-claimed, fix the number
  PASS  reported test count (1555) does not exceed actual (1808)
```

`WARN`, not `FAIL`, so the gate is green. The report this phase wrote claims
1808, which is what `cargo test --all-targets` printed and what `verify.sh`
itself printed in its `cargo test` section. Syncing it is the dispatcher's step;
the phase contract forbids writing outside the checkout, so it was not done
here.

## 8. `README.md` has no `docs/` or `bootstrap/` entry in its project structure

`README.md:266-289` lists `src/`, `tests/`, `corpus/`, `examples/`, `modules/`
and `tooling/vscode/`. It does not list `bootstrap/`, `docs/`, `phases/` or
`rbops/`. Those directories exist and `bootstrap/compiler.rb` is the subject of
the self-hosting line this phase corrected two entries below the tree, so the
tree and the roadmap now describe the project at different depths.

Not fixed: the tree check in `tests/doc_accuracy_test.rs` asserts that what the
tree names exists, not that the tree is complete, and completeness is a judgement
about what belongs in a README's front page. Recorded for the auditor.