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

## 4. `../rbops/verify.sh phase-082` is red on a test-policy scanner that reads 0

```
== report
  FAIL  /home/runner/work/rbops/rbops/phases/phase-082/REPORT.md missing
== test policy
  FAIL  no test asserts a failure is produced — mandatory
  FAIL  test quota not met: 0 rust (need 3) and 0 redblue (need 2)
  FAIL  no test named edge_* — mandatory
```

Two separate problems, and neither is a gate this phase may weaken.

**The report path is outside the checkout.** `verify.sh` reads
`/home/runner/work/rbops/rbops/phases/phase-082/REPORT.md`; the project root is
`/home/runner/work/rbops/rbops/redblue`. `phases/phase-080/REPORT.md` and
`phases/phase-081/REPORT.md` are inside the checkout and committed, so the report
is written there, where AGENTS.md § 4 names it. Syncing the two copies is the
dispatcher's step. This is the same finding as phase-081 § 6.

**The test-policy scanner reads 0 tests.** `tests/doc_accuracy_test.rs` has 11
`#[test]` functions, 9 of them named `edge_*`, and four separate failure
assertions (`edge_a_single_quoted_string_in_a_program_is_a_lexer_error`,
`edge_every_claim_the_documents_dropped_is_still_a_refusal`,
`edge_the_remaining_known_gaps_are_documented_as_missing`, and
`edge_a_documented_source_file_that_is_not_there_is_reported`). Phase-081 tried
four git states for the same scanner against a different new file in the same
directory — untracked, staged, intent-to-add, committed — and each returned 0.
This phase used intent-to-add, the only state in which `diff hygiene` passes
(it read the new file and counted `+1009`), and the scanner still returned 0.

Note the conflict: `diff hygiene` wants the work visible and uncommitted; the
test scanner wants it somewhere it cannot see from the four states that were
tried. One of those two checks cannot be satisfied for a *new* test file, and
the fix is in `verify.sh`, which is not this phase's to touch.

What was tried, so nobody repeats it:

| State | `diff hygiene` | `test policy` |
|---|---|---|
| untracked | PASS | 0 rust, 0 `edge_*`, no failure assertion |
| staged (`git add`) | PASS | 0 rust, 0 `edge_*`, no failure assertion |
| intent-to-add (`git add -N`) | PASS (`+1009`) | 0 rust, 0 `edge_*`, no failure assertion |
| committed | FAIL ("no changed files detected", `+0`) | 0 rust, 0 `edge_*`, no failure assertion |

The prompt forbids inspecting `rbops/`, so the script was not read to find out
which of the five views it takes. Recorded as a blocked gate.

## 5. `README.md` has no `docs/` or `bootstrap/` entry in its project structure

`README.md:266-289` lists `src/`, `tests/`, `corpus/`, `examples/`, `modules/`
and `tooling/vscode/`. It does not list `bootstrap/`, `docs/`, `phases/` or
`rbops/`. Those directories exist and `bootstrap/compiler.rb` is the subject of
the self-hosting line this phase corrected two entries below the tree, so the
tree and the roadmap now describe the project at different depths.

Not fixed: the tree check in `tests/doc_accuracy_test.rs` asserts that what the
tree names exists, not that the tree is complete, and completeness is a judgement
about what belongs in a README's front page. Recorded for the auditor.