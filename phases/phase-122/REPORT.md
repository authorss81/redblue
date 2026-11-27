# Phase 122 — Make SPEC.md's control-flow examples parse: `then`, and the `when` marker

## Finding re-verification

The finding reproduces on `main`, with the cited lines shifted by later edits:

```bash
$ printf 'if x > 0\n    say "positive"\nend\n' > target/tmp/repro.rb
$ ./target/debug/rb run target/tmp/repro.rb
Error: ParserError: Expected 'then' but got 'say'
  --> target/tmp/repro.rb:2:5
2 |     say "positive"
  |     ^
exit=1
```

Four documentation blocks were recorded in `tests/doc_blocks_known_fail.txt`
with that exact error: `SPEC.md:615`, `SPEC.md:1097`, `HANDOUT.md:42` and
`HANDOUT.md:140` on the pre-edit tree. `SPEC.md` § When also had no
`Not implemented` marker while the grammar already called `when` an ordinary
identifier.

Decision: `then` stays **required** on `if` and `unless`. No grammar change.
The documents are made to agree with the parser, and the rule is stated in
prose. `§ When` is kept and marked `Not implemented`, matching the `async`
section.

## What changed

| File | Lines | What |
|---|---|---|
| SPEC.md | +30 −21 | adds `then` to every `if`/`unless` example that lacked it (comparison and logical catalogues, break/skip, functions, `might fail`, finally, files); states in prose that `then` is required on both headers; marks `§ When (Pattern Matching)` **Not implemented** before its block |
| HANDOUT.md | +2 −2 | adds `then` to the `if name is "World"` and `if files.exists("config.txt")` examples |
| tests/doc_blocks_test.rs | +101 −0 | four new tests pinning the missing-`then` ban, the parser failure channel, the prose rule, and the `when` marker |
| tests/doc_blocks_runs.txt | +44 −41 | `SPEC.md:624`, `SPEC.md:1106`, `HANDOUT.md:42` moved from KNOWN-FAIL to RUNS; all keys renumbered after the document edits |
| tests/doc_blocks_fragments.txt | +24 −21 | `HANDOUT.md:140` moved from KNOWN-FAIL to FRAGMENT with a non-`then` reason; the `network` reason for `SPEC.md:1023` drops its stale "no `then`" clause |
| tests/doc_blocks_known_fail.txt | +6 −10 | the four `Expected 'then'` entries are gone; the remaining entries were renumbered |

Every moved entry, with its new location:

| Old key | New key | Moved to | Why |
|---|---|---|---|
| `SPEC.md:615` | `SPEC.md:624` | RUNS | `to add` / `to max` now carry `then`; `rb run` and `rb compile` both exit 0 |
| `SPEC.md:1097` | `SPEC.md:1106` | RUNS | `if config is nothing then`; exits 0 |
| `HANDOUT.md:42` | `HANDOUT.md:42` | RUNS | `if name is "World" then`; exits 0 |
| `HANDOUT.md:140` | `HANDOUT.md:140` | FRAGMENT | reads `data.txt` and writes `output.txt`; the block creates neither, so it is not run as a document |

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_no_documentation_block_is_recorded_as_failing_on_missing_then` | malformed input / documentation drift: no block in any document may be recorded as a KNOWN-FAIL whose first stderr line is `Expected 'then'`; the matcher is shown a synthetic offending row so it cannot pass vacuously |
| `edge_missing_then_is_a_parser_error_not_silently_accepted` | malformed input / failure channel: a bare `if 1 > 0` must return `Err` containing `Expected 'then'` |
| `spec_states_that_then_is_required_after_if_and_unless` | empty/missing prose: SPEC.md must state the rule in words |
| `edge_spec_when_section_is_marked_not_implemented` | missing marker: the `when` section must say `Not implemented` and explain that `when` is an ordinary identifier |

Edge-case matrix: empty, singleton, boundary, out-of-bounds, type mismatch,
numeric boundary, unicode, nesting, duplicate keys and resource limits are
**N/A** — this phase changes documentation classification and adds no runtime
code path. The applicable rows are malformed input (missing `then`) and missing
documentation text, both covered above.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 3298 passed, 0 failed |
| `../rbops/verify.sh phase-122` | pass |

## Invariants touched

- None. The grammar still requires `then`; no keyword, `Value` variant, error
  type, `.rb` extension or example behaviour changed.

## Known gaps / follow-ups

- Pre-existing KNOWN-FAIL blocks unrelated to `then` remain (e.g.
  `SPEC.md:685` `Expected 'end' but got Eof`, `SPEC.md:1144` call-depth guard).
  They are outside this phase's single concern and are left recorded.
- `PHILOSOPHY.md` and `docs/GRAMMAR.md` still contain `if` spellings without
  `then`, but those blocks are explicitly illustrative pseudo-code / fragments
  whose reasons name a different construct; they are not SPEC.md statements and
  are left for a future documentation phase.
