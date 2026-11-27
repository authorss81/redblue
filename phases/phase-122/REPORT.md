# Phase 122 — Make SPEC.md's control-flow examples parse: `then`, and the `when` marker

## Finding re-verification

The finding reproduces on `main`:

```bash
$ printf 'if x > 0\n    say "positive"\nend\n' > target/tmp/repro.rb
$ ./target/debug/rb run target/tmp/repro.rb
Error: ParserError: Expected 'then' but got 'say'
  --> target/tmp/repro.rb:2:5
exit=1
```

Four documentation blocks were recorded in `tests/doc_blocks_known_fail.txt`
with that exact error. `SPEC.md` § When also had no `Not implemented` marker
while the grammar already treats `when` as an ordinary identifier.

Decision: `then` stays **required** on `if`, `else if` and `unless`. No grammar change.
The documents are made to agree with the parser, and the rule is stated in
prose. `§ When` is kept and marked `Not implemented`, matching `async`.

## What changed

| File | Lines | What |
|---|---|---|
| SPEC.md | +30 −21 | adds `then` to every `if`/`unless` example that lacked it; states in prose that `then` is required on `if`, `else if` and `unless` headers; marks `§ When (Pattern Matching)` **Not implemented** |
| HANDOUT.md | +2 −2 | adds `then` to the `if name is "World"` and `if files.exists("config.txt")` examples |
| docs/GRAMMAR.md | +1 −1 | adds `then` to the `If Statement` example, so the defining grammar document no longer teaches the invalid spelling |
| tests/doc_blocks_test.rs | +394 −3 | the phase tests below, plus a repaired format string whose missing `{}` made the file fail to compile |
| tests/doc_blocks_runs.txt | +44 −41 | `SPEC.md:624`, `SPEC.md:1106`, `HANDOUT.md:42` moved from KNOWN-FAIL to RUNS; `SPEC.md:1693` (zip/tar) classified RUNS; all keys renumbered after the document edits |
| tests/doc_blocks_fragments.txt | +24 −21 | entries renumbered after the edits; no entry excuses a block on a missing `then` |
| tests/doc_blocks_known_fail.txt | +7 −10 | the four `Expected 'then'` entries are gone; `HANDOUT.md:140` is recorded honestly as a whole program that fails on its missing input file, not as a fragment |

Every moved entry, with its new location:

| Old key | New key | Moved to | Why |
|---|---|---|---|
| `SPEC.md:615` | `SPEC.md:624` | RUNS | `to add` / `to max` now carry `then`; `rb run` and `rb compile` both exit 0 |
| `SPEC.md:1097` | `SPEC.md:1106` | RUNS | `if config is nothing then`; exits 0 |
| `HANDOUT.md:42` | `HANDOUT.md:42` | RUNS | `if name is "World" then`; exits 0 |
| `HANDOUT.md:140` | `HANDOUT.md:140` | KNOWN-FAIL | now carries `then`; it is a whole program that compiles and fails at runtime because `data.txt` is never created, so it is not a fragment |
| — | `SPEC.md:1693` | RUNS | zip/tar block creates the archives it reads; `rb run` and `rb compile` both exit 0 |

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_no_documentation_block_is_recorded_as_failing_on_missing_then` | malformed input / documentation drift: no KNOWN-FAIL row may record `Expected 'then'`; a synthetic offending row proves the matcher has teeth |
| `edge_missing_then_is_a_parser_error_not_silently_accepted` | malformed input / failure channel: a bare `if` and a bare `unless` must return `Err` containing `Expected 'then'` |
| `spec_states_that_then_is_required_after_if_and_unless` | missing prose: SPEC.md must state the rule in words |
| `edge_spec_when_section_is_marked_not_implemented` | missing marker: the `when` section must say `Not implemented` and explain that `when` is an ordinary identifier |
| `edge_every_conditional_header_in_documents_carries_then` | malformed input: scans every `if`/`else if`/`unless` header in SPEC.md, HANDOUT.md and docs/GRAMMAR.md blocks and counts them so it cannot pass vacuously |
| `edge_handout_files_example_is_a_known_failure_on_the_missing_input` | resource limit / failure channel: the file example must run and fail with `IoError`, not the parser error, and be in KNOWN-FAIL |
| `edge_spec_zip_and_tar_block_is_runs` | boundary / success: the zip/tar block must be RUNS and must both run and compile |
| `edge_then_is_required_after_else_if` | malformed input / failure channel: an `else if` without `then` must return `Err` containing `Expected 'then'`, while the same form with `then` runs |

Edge-case matrix: empty, singleton, out-of-bounds, type mismatch, numeric
boundary, unicode, nesting, duplicate keys are **N/A** — this phase changes
documentation classification and adds no runtime code path. The applicable rows
are malformed input (missing `then`), missing documentation text, and the
missing-input resource case, all covered above.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 3303 passed, 0 failed |
| `../rbops/verify.sh phase-122` | pass |

## Invariants touched

- None. The grammar still requires `then`; no keyword, `Value` variant, error
  type, `.rb` extension or example behaviour changed.

## Known gaps / follow-ups

- Pre-existing KNOWN-FAIL blocks unrelated to `then` remain (e.g.
  `SPEC.md:685` `Expected 'end' but got Eof`, `SPEC.md:1144` call-depth guard).
  They are outside this phase's single concern and stay recorded.
- `PHILOSOPHY.md` still contains `if` spellings without `then`, but those
  blocks are explicitly illustrative pseudo-code whose reasons name a different
  construct; they are left for a future documentation phase.
