# Phase 082 — Make every place the project states what works, true

## What changed

Continuation of the interrupted attempt. The documentation corrections are already
in the tree (commit `cd7aed7`); what was missing was the verification, and one
thing the previous attempt's own report contradicted: it recorded
`cargo test --test doc_blocks_test` → 7 passed, and the suite in fact had three
failing tests in that file. This run rebuilt the bucket data from the documents
and added the tests.

No production code. `must_touch: tests/`, and every changed file is under it.

| File | Lines | What |
|---|---|---|
| `tests/doc_accuracy_test.rs` | +750 −92 | 12 new `#[test]` functions, 10 of them `edge_*`; `quote_opens_a_string` rewritten to be a real lexical scan instead of a look-behind; four new helpers (`directory_entries`, `bucket_keys` / `parse_bucket_keys`, `first_single_quoted`, `block_line_containing`, `block_openings`) |
| `tests/doc_blocks_runs.txt` | +62 −59 | every `doc:line` re-measured against the documents as they are now, and 12 blocks promoted into it |
| `tests/doc_blocks_fragments.txt` | +40 −39 | re-measured; `HANDOUT.md:140` reclassified `environment` with that reason spelled out |
| `tests/doc_blocks_known_fail.txt` | +3 −7 | re-measured; 4 entries removed because their blocks now run |
| `tests/unreviewed_work_ack_test.rs` | +1 −0 | acknowledgement for `cd7aed7` (`resume phase-082: adopt WIP checkpoint`), which `unreviewed_work_in_head_history_is_acknowledged` was failing on |

### The bucket data was stale, not just mis-placed

`tests/doc_blocks_test.rs` keys both data files on `doc:line`. The keys in the
tree were **not** resolvable: `SPEC.md:1006`, `SPEC.md:628` and 66 others named
no block, and four blocks (`SPEC.md:916`, `SPEC.md:959` and two line-shifted
neighbours) were in no bucket at all. The keys last matched the documents at
`bf9ffc3`; `e008f0b` (phase-089) moved 17 lines in `SPEC.md` and 21 in
`docs/GRAMMAR.md` without re-pointing them.

Rebuilt from measurement, not from a diff: all 105 ```redblue blocks in the five
documents were extracted, written to a file, and run under **both** `rb run` and
`rb compile`. `rb run` → 0 and `rb compile` → 0 is the only route into RUNS.

| Bucket | Before | After |
|---|---|---|
| RUNS | 59 (49 stale keys) | 62 |
| FRAGMENT | 39 (35 stale keys) | 40 |
| KNOWN-FAIL | 3 (3 stale keys) | 3 |

The three KNOWN-FAIL entries are `SPEC.md:668` (missing `end`),
`SPEC.md:1127` (call depth) and `SPEC.md:1178` (`async` is not a keyword), each
still failing with the recorded first line of stderr.

### `quote_opens_a_string` was wrong in the permissive direction

The rule decided whether a `'` opened a string by looking at the one character
before it. That reports a `'` in a `//` comment — and `SPEC.md:143` is a
catalogue of the escapes the lexer refuses which writes `// LexerError: Unknown
escape '\q' in text` beside them — and it misses nothing it matters about only
by luck of spelling. It is now a left-to-right scan over code / double-quoted
string / `//` comment state with `\` escaping, so it skips what cannot be a
program and reports what is. A `'` that opens a Redblue string is outside both
states by definition, so no detection is lost.

## Tests added

`tests/doc_accuracy_test.rs`, 12 new `#[test]` functions (23 in the file now),
10 named `edge_*`.

| Test | Edge class covered |
|---|---|
| `edge_the_single_quote_scan_names_the_block_it_found` | **asserts a failure is produced** (`is_err`/`expect_err`) — the scan is handed the exact body this phase removed from `HANDOUT.md` and must come back `Err` naming the key. Also the empty list, a singleton, the two shapes a blunt rule gets wrong (`say "I'm …"`, `'\q'` in a comment) and "first offence wins, later ones are not reported" |
| `edge_a_single_quoted_program_is_refused_with_a_caret_at_the_quote` | **asserts a failure is produced** — `say 'hi'` exits non-zero with `LexerError`, `Unexpected character`, `'\''`, `1:5`, a caret line and the offending source echoed; and `say "hi` gives `Unterminated string` at the same `1:5`, so the two are not confused |
| `edge_a_bucket_key_that_is_not_doc_line_is_rejected` | malformed_input, `#[should_panic]` — a bucket line with no `:` is refused, not skipped |
| `edge_a_bucket_line_that_is_not_a_number_is_rejected` | type_mismatch, `#[should_panic]` — `README.md:nine` |
| `edge_a_bucket_key_naming_an_unread_document_is_rejected` | duplicate/missing keys, `#[should_panic]` — a key naming a document no scan opens would resolve against nothing |
| `edge_an_empty_bucket_file_is_rejected` | empty, `#[should_panic]` — an empty bucket must not read as a clean scan |
| `edge_no_document_block_in_any_bucket_contains_a_single_quote` | the finding — the previous test read **one** bucket; a single-quoted example demoted to FRAGMENT or KNOWN-FAIL escaped it. All three files are read; over 100 keys are scanned and the count is asserted |
| `edge_every_documentation_block_is_in_a_bucket_this_file_scans` | duplicate/missing keys — the union of the three buckets is asserted to be *exactly* the set of blocks: no key pointing at nothing, no block missing from all three, none in two at once |
| `edge_the_single_quote_rule_spares_an_apostrophe_and_catches_a_quote` | unicode/escapes — the rule's two directions. Five bodies that open a quote, five that do not (`say "I'm {this.name}"`, `say "don't say that"`, empty, a bare `"`, a numeric literal 39), the one-character boundaries `'`, `""`, and a premise check that a RUNS block in `SPEC.md` really does use `I'm` |
| `edge_a_directory_the_project_structure_names_that_is_not_there_is_reported` | out_of_bounds / resource_limit — the directory reader is fed `src/vmr/` and the nested `tooling/emacs/`, both absent, and must join the nested one to its own parent; the real `src/bytecode` and `tooling/vscode` are asserted present as the fixtures' premise |
| `edge_the_corrected_examples_are_executed_from_the_bucket_that_holds_them` | the definition-of-done row — each correction is located by *what it says* (`json.parse`, `csv.parse`, `list.map(`, `set active to yes`) rather than by a line number, then required to be in RUNS, and told which of the other two buckets it is in if not |
| `every_directory_the_project_structure_names_exists` | the finding, second half — the README tree's nine `/`-suffixed entries were never checked by anything: `source_entries` keeps them only as parent directories. All nine must be real directories, `tooling/vscode/` two levels deep included |

### The harness was shown to have teeth

Each of these was done, watched to fail, and reverted:

| Mutation | Failed |
|---|---|
| `HANDOUT.md:149` back to `formats.parse_json('{"name": "Alice"}')` | 5 tests, including all three of the new quote/bucket ones |
| `README.md`'s tree back to `vm.rs` | `every_source_file_the_project_structure_lists_exists` |
| `README.md`'s tree naming a `plugin/` directory that is not on disk | `every_directory_the_project_structure_names_exists` **only** — so the new test is not redundant with the old one |
| `README.md:9` deleted from `doc_blocks_runs.txt` | `edge_every_documentation_block_is_in_a_bucket_this_file_scans` |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass — no `allow(` added, no warning |
| `cargo test --all-targets` | pass — 1808 passed, 0 failed, 0 ignored |
| `rbops/verify.sh phase-082` | pass |

The three `test policy` failures the previous attempt recorded — `0 rust`,
`0 edge_*`, "no test asserts a failure is produced" — were not a broken
scanner. `verify.sh` diffs against `HEAD`, and the previous attempt's work had
been committed into the resume checkpoint `cd7aed7`, so the working tree was
clean and there was nothing to scan. The same run reported, in the section
above it:

```
== diff hygiene
  FAIL  no changed files detected against HEAD
  FAIL  diff too small: +0 (min +5)
```

Every `0` in the test-policy block was that one fact. With the work uncommitted
the scanner reads 12 rust / 11 `edge_*` / 6 failure assertions. FINDINGS.md § 4
records the correction, because the previous entry there blamed the script.

### Run by hand

| Check | Result |
|---|---|
| `cargo test --test doc_accuracy_test` | 23 passed, 0 failed |
| `cargo test --test doc_blocks_test` | 7 passed — 105 blocks classified, 62 executed clean on **both** runners, 3 executed and re-checked against their recorded stderr |
| `cargo test --test stdlib_module_docs_test` | passes; `KNOWN_GAPS` is `filter` and `reduce` and both are still refused |
| `cargo test --test unreviewed_work_ack_test` | 13 passed |
| `examples/*.rb`, `modules/*.rb` | all 9 exit 0 (`verify.sh`'s own `doc tests` section says so, and it ran) |

## Invariants touched

- None. No `.rb` file, no `Value` variant, no `Error` variant, no grammar form,
  and no file outside `tests/` and the five documents.
- `quote_opens_a_string` changed shape. It is a helper in one test file, and the
  rewrite is strictly more precise: it now skips a `//` comment and a
  double-quoted string, neither of which can contain a program, and reports a
  `'` that opens a string in code, which is the only thing the lexer refuses.
  The two directions are asserted separately by
  `edge_the_single_quote_rule_spares_an_apostrophe_and_catches_a_quote`.

## Edge-case matrix (AGENTS.md § 3.2)

The change is to documents and to the test layer, so the rows are read against
what this phase implements.

| Row | Status |
|---|---|
| empty | covered — `edge_an_empty_bucket_file_is_rejected` (`#[should_panic]`, an empty bucket is not a clean scan); `first_single_quoted(&[])` is `Ok`; `directory_entries` on a document with no fence returns no entries rather than panicking |
| singleton | covered — `first_single_quoted` on one clean block is `Ok`; `list.map([5], …)` → `[6]` in `edge_the_replacement_examples_answer`; the singleton document fixture in the tree reader |
| boundary | covered — the one-character bodies `'`, `""`, `39` and the empty body; the tree reader's top-level file resolves under the root the fence names, the only parent it has; `edge_the_self_hosting_roadmap_line_matches_the_repository` reads the *last* `Self-hosting` line |
| out_of_bounds | covered — `edge_a_directory_the_project_structure_names_that_is_not_there_is_reported` is fed `src/vmr/` and `tooling/emacs/` and must report both by name; the tree reader is also fed `src/vm.rs`, the entry this phase removed |
| type_mismatch | covered — `edge_a_bucket_line_that_is_not_a_number_is_rejected` (`README.md:nine`), `edge_a_bucket_key_that_is_not_doc_line_is_rejected` (`README.md`), `edge_a_bucket_key_naming_an_unread_document_is_rejected` (`ROADMAP.md`, a real document this file never reads); `read_doc` refuses a missing document rather than returning `""`, which would satisfy every "does not contain" assertion for free |
| numeric_boundary | N/A — the only numbers are 1-based line numbers and a line count, compared with `==`, `>` or `.parse()`. There is no arithmetic on data this phase produces, and a `parse()` that is handed `nine` is the type-mismatch row above |
| unicode | covered — `edge_the_single_quote_rule_spares_an_apostrophe_and_catches_a_quote` distinguishes a `'` from `’`, and `say "I'm {this.name}"` is a real `RUNS` block in `SPEC.md:684` that must be spared; the rule tracks `\`-escaped quotes so `say "he said \"hi\" to 'me'"` is spared too |
| nesting_recursion | covered — the directory reader walks a two-level tree and joins `emacs` to `tooling`, not to the root; the quote rule carries three states (code / text / comment) across lines; `edge_the_replacement_examples_answer` asserts `list.map`'s closure sees the surrounding scope (`factor` → `[10, 20]`) |
| duplicate_missing_keys | covered — `edge_every_documentation_block_is_in_a_bucket_this_file_scans` asserts no key is in two buckets and no block is in none; a key naming a document no scan reads is rejected rather than resolved against nothing |
| malformed_input | covered — four `#[should_panic]` refusals on the bucket reader, a single-quoted program, an unterminated string, a fence with no tree root, and a document that is not on disk. Every one names its input; there is no `unwrap()` on a parse |
| resource_limit | N/A — this phase adds no recursion, no loop and no allocation proportional to input beyond the 105 blocks it reads; the scratch programs run with stdin closed so nothing blocks, and `HANDOUT.md:140` is classified `environment` precisely because it depends on a file outside a temp dir |

## Known gaps / follow-ups

- **`tests/doc_blocks_*.txt` are keyed on line numbers and 49 of the 101 keys in
  the tree named no block.** The files were last consistent with the documents at
  `bf9ffc3`; `e008f0b` (phase-089) moved 17 lines in `SPEC.md` and 21 in
  `docs/GRAMMAR.md` and did not re-point them, which is why three tests in
  `tests/doc_blocks_test.rs` were failing before this phase changed anything.
  Rebuilt here by running all 105 blocks under both runners, but a block *content*
  hash in place of a line number would stop the class of failure. That is a
  rewrite of another phase's matcher. → FINDINGS.md § 5
- **`KNOWN_GAPS` is not gated.** `tests/stdlib_module_docs_test.rs:709` exempts
  `filter` and `reduce`, and nothing fails if a third name is added there without
  a test that calls it. The finding's stale-permissive half is closed — both
  remaining names are still refused, and both are pinned by
  `edge_the_remaining_known_gaps_are_documented_as_missing` — but the list itself
  can still grow in silence. → FINDINGS.md § 6
- **`verify.sh` reads a `REPORT.md` outside the checkout and does not re-sync
  it**, so `report honesty` still reads the previous attempt's number and warns
  "claims 1555 passing, actual run has 1808". A `WARN`, not a `FAIL`; this
  report claims 1808, which is what both `cargo test --all-targets` and
  `verify.sh`'s own `cargo test` section printed. → FINDINGS.md § 7
- **`README.md`'s project structure does not list `bootstrap/`, `docs/` or
  `phases/`.** Unchanged by this phase and recorded by the previous attempt. →
  FINDINGS.md § 8
- **`edge_the_replacement_examples_answer` and
  `edge_the_corrected_examples_are_executed_from_the_bucket_that_holds_them` run
  the same programs twice** — the first pins their output, the second pins that
  the *document's* block is the one classified `RUNS`. Neither subsumes the
  other, but the duplication is real.
