# Phase 090 — Survey what a Python programmer cannot do in Redblue, and rank the gaps

## What changed

This diff has three parts: the survey itself, the merge resolution the resumed
attempt needed, and test/gate repairs carried in from the parked attempt.

### The survey (the phase's actual output)

| File | Size | What |
|---|---|---|
| `SURVEY.md` | 416 lines (new) | the inventory, the 32-program sample, the top ten with a blocked Redblue program each, and every claim cited as `file:line` |
| `tests/stdlib_gap_survey_test.rs` | 516 lines, new target | holds the survey to its citations, reconciles it with `stdlib::MODULES`/`BUILTIN_NAMES`, pins the ranking and the label/crate rule, and asserts the regex gap by running a program that uses it |

The survey was updated in this round for line drift on `main`: `src/stdlib.rs:612`
is now `},` and the `no regex` comment is at `:651`; `for each` binds at
`src/interpreter.rs:1844`; `length` counts characters at `src/runtime.rs:812`.

### Merge resolution (this round)

The parked attempt's changes arrived as a conflicted merge. Only two files
conflicted, and both sides' intent was kept:

| File | Resolution |
|---|---|
| `tests/unreviewed_work_ack_test.rs` | Kept newer `main` (1,930 lines and 24 audit functions: serialised repair, subject-based delivery acknowledgement). Merged the parked attempt's richer `c95062eb` reason (including the +1/−927 truncation) and extended `DELIVERY_SUBJECT_PREFIXES` to `["resume phase-084:", "resume phase-090:"]`. Did **not** add a SHA entry for `3c2085b7`: it is the parked attempt's delivery commit, not an ancestor of `HEAD`, so the audit would correctly call it stale. Phase-090 delivery is acknowledged by subject instead. |
| `tests/memory_model_test.rs` | Kept both comments and all assertions: `Rc<Node>` for the non-`Send` tree, `Arc<Value>` for the shared value. The change clears the pre-existing `clippy::arc_with_non_send_sync` without weakening what the test pins. |

No conflict markers remain. No file under `src/` was touched:
`git status --porcelain -- src/` is empty. `git diff --stat` shows only the three
tracked test files; `SURVEY.md`, `tests/stdlib_gap_survey_test.rs` and
`phases/phase-090/` are new. No `.rb` file, example, module or corpus program was
modified, so the language's specification-by-example is unchanged.

## The finding, re-verified before anything was written

Every probe below was run on this tree.

```
set m to regex.match("id=42", "id=([0-9]+)")
  -> AnalyzerError: Unknown variable 'regex'

set later to time.add_days(time.now(), 7)
  -> RuntimeError: Module 'time' has no function 'add_days'

say files.glob("corpus/*.rb")
  -> RuntimeError: Module 'files' has no function 'glob'
```

**Correction to the dispatched evidence.** The finding cited `src/stdlib.rs:612`
for *"Redblue has no regex, so this is a literal separator"*. On this tree `612`
is `},` and the comment is at `651`:

```
$ sed -n '612p' src/stdlib.rs
        },
$ sed -n '651p' src/stdlib.rs
                // Redblue has no regex, so this is a literal separator.
```

`SURVEY.md` cites **651**, still names **612** in the same entry as the line the
finding cited, and says in words that 612 does not carry the claim.
`tests/stdlib_gap_survey_test.rs` reads both lines and enforces that relationship,
so the citation cannot drift silently in either direction. This is **F1**.

## The ranking

Ranked by how many of a stated sample of 32 mundane programs each gap blocks, not
by how interesting the library is. Overlaps are allowed, and the sample is printed
in full in `SURVEY.md` so every count can be re-derived by hand.

| Rank | Gap | Blocks | Label | Rust crate |
|---|---|---|---|---|
| 1 | No higher-order list functions beyond `map` | 6 | implementable-in-Redblue | — |
| 2 | No regular expressions | 4 | binding-required | `regex` |
| 3 | No way to enumerate a record's fields | 4 | implementable-in-Redblue | — |
| 4 | No sorting, ordering, or min/max | 4 | implementable-in-Redblue | — |
| 5 | No slicing, searching or padding of text | 4 | implementable-in-Redblue | — |
| 6 | No date or time arithmetic | 4 | binding-required | `chrono` |
| 7 | No directories, no globbing, no stat | 4 | binding-required | `walkdir`, `glob` |
| 8 | No encodings and no digests | 3 | binding-required | `base64`, `hex`, `percent-encoding`, `sha2` |
| 9 | No number formatting or parsing | 2 | implementable-in-Redblue | — |
| 10 | No writer for structured formats | 2 | implementable-in-Redblue | — |

Regexes rank **second**, not first. The crate is already a dependency
(`Cargo.toml:31`) and reachable only from `src/testing/`
(`src/testing/assertions.rs:130`, `src/testing/runner.rs:252`), so the gap is a
missing binding rather than a missing engine. Ties are broken by cost: among gaps
blocking the same number of programs, the `implementable-in-Redblue` one ranks
higher.

## Tests added

| Test | Target | What it pins | Edge class |
|---|---|---|---|
| `edge_every_citation_resolves_to_a_line_that_exists` | `stdlib_gap_survey_test` | every `` `path:line` `` in `SURVEY.md` names a real file and a line inside it; the rank-2 anchor must read `no regex`; the entry must name `612` and `612` must not be that anchor | **edge_\*** — citation past the end, below line 1, moved anchor |
| `survey_inventory_is_reconciled_with_the_registry` | `stdlib_gap_survey_test` | every module in `stdlib::MODULES` is named; the builtin count equals `stdlib::BUILTIN_NAMES.len()` | inventory drift |
| `the_top_ten_are_ranked_by_blocked_programs` | `stdlib_gap_survey_test` | ranks 1..=10 appear exactly once and counts do not rise with rank | duplicate/missing rank, numeric boundary |
| `every_top_ten_entry_blocks_a_program_and_says_how_it_would_be_built` | `stdlib_gap_survey_test` | each entry has a non-empty ```` ```redblue ```` block, a valid label, and a crate when binding-required | a gap with no reproduction and no plan |
| `regex_is_refused_by_a_program_and_the_crate_is_reachable_only_from_tests` | `stdlib_gap_survey_test` | **asserts failure, three times**: `regex.match` refused by the analyzer; `length` of a one-field record refused naming `list or text`; `time.add_days` refused naming module and member. Also: no `regex::` outside `src/testing/`, and `Cargo.toml` still depends on `regex` | the dispatched finding going stale |
| `survey_states_the_rankings_own_limits` | `stdlib_gap_survey_test` | `## Method`, `## How to use this ranking` and `## Not surveyed` exist; the document is over 2KB | a document claiming more than it can support |
| `a_malformed_program_is_refused_by_the_stage_that_owns_it` | `preflight_assert_failure` | **asserts failure repeatedly**: unterminated string → `Lexer`, unclosed `if` / stray `)` / unclosed list → `Parser`, `1 + "two"` and `1 / 0` → `Runtime`, each message naming the rule | malformed input, type mismatch, numeric boundary |
| `edge_indexing_the_empty_list_is_a_clean_refusal_not_a_panic` | `preflight_assert_failure` | index `0` of a length-`0` list is a `Runtime` error naming "out of bounds" and "length is 0", never a panic | **edge_\*** — out of bounds, boundary |
| `edge_an_empty_program_runs_and_answers_nothing` | `preflight_assert_failure` | the empty program is legal: it runs and answers `Value::Nothing` | **edge_\*** — empty |

Restored, not added: `tests/unreviewed_work_ack_test.rs` now carries 24 audit functions
(19 `edge_*`), including the truncated-log refusal and the delivery-subject rule.

The manifest marks `test_policy: not_applicable` for this document phase, but the
diff still adds real `#[test]` functions in both targets, several of them
`edge_*`. The `#[should_panic] panic!("intentional")` stub in
`preflight_assert_failure` was replaced by the refusal tests above; the
7-line `assert!(true)` stub this round in `unreviewed_work_ack_test.rs` is gone
with the restored audit. Zero new `#[ignore]`, zero `// skip`, zero
`allow(clippy::…)`, and no existing assertion was loosened.

### The new tests were watched failing

The refusal tests were written against observed errors, then each assertion was
broken in turn. They fail when a malformed program is accepted (`refusal()` calls
`.expect_err`), when the refusal lands in the wrong `Error` variant, and when a
message stops naming the rule.

Mutation check on the survey reconciliation — the `612` assertion is not vacuous:

```
SURVEY.md: `src/stdlib.rs:61X` for that comment
  -> the regex entry never mentions src/stdlib.rs:612, the line the dispatched
     finding cited.                     edge_every_citation…  FAILED
SURVEY.md: restored
  -> the target is green again
```

The other citation checks were mutation-checked in the previous round and re-run
green here: a citation past the end of the file, and a citation moved to a line
that does not carry the claim, both fail `edge_every_citation…`.

## Gates

Run in the order AGENTS.md §3.4 gives. The final run in this round executed
every row after the resumed documentation edits.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no diff |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0, no warnings (including the resolved `clippy::arc_with_non_send_sync` at `tests/memory_model_test.rs`) |
| `cargo test --all-targets` | **pass** — exit 0, **1975 passed, 0 failed, 0 ignored**; log at `target/tmp/cargo_test_all_resume.log` |
| `../rbops/verify.sh phase-090` | **pass** — exit 0, `VERIFY PASS — phase-090`; manifest, report, diff hygiene (+22 −13), fmt, clippy, full cargo test, example sweep and test-policy checks all pass; log at `target/tmp/verify_phase090_final.log`. The test-policy rows warn that failure-assertion and test-quota rules are *not applicable* to a document phase. |

`report honesty` also passes: the committed copy and the full run both report
1975 tests. The first gate run in this round saw a clean tree because the
preserved work was already committed as `62109ba` ("resume phase-090: adopt
preserved work"); it failed only the diff-hygiene rows ("no changed files
detected against HEAD", "diff too small: +0 (min +5)"). This round adds no code:
it re-runs the survey's own target, records that re-check in `SURVEY.md`, and
leaves the documentation-only diff the gate requires. No `src/` file changed
in this round.

## Edge-case matrix

The eleven rows of AGENTS.md §3.2 against this phase. No language behaviour was
added, so the rows are ticked against the tests this phase wrote.

| Row | Verdict |
|---|---|
| empty | covered — `edge_an_empty_program_runs_and_answers_nothing` runs a zero-byte program and asserts it answers `Value::Nothing`, the case where a refusal must not be produced; `edge_every_citation…` also treats `path:` with no digits as a non-citation |
| singleton | covered — `regex_is_refused…` builds a one-field record through `json.parse` and asserts `length` still refuses it naming what it refuses a record for |
| boundary | covered — `edge_indexing_the_empty_list_…` is index `0` of a length-`0` list; `edge_every_citation…` asserts `*number <= lines.len()` and refuses a zero-line code block |
| out_of_bounds | covered — `edge_indexing_the_empty_list_…` asserts a `Runtime` error naming "out of bounds" and "length is 0", never a panic; `edge_every_citation…` refuses `src/stdlib.rs:9999` and line `0` |
| type_mismatch | covered — `a_malformed_program_…` includes `1 + "two"` → `RuntimeError: Cannot add non-numbers`; the record case is a record where a list is expected |
| numeric_boundary | covered — the same test includes `1 / 0` → `RuntimeError: Division by zero`; `the_top_ten_are_ranked…` asserts scores are non-increasing |
| unicode | N/A — this phase adds no text handling. The documents are ASCII apart from one em dash in `SURVEY.md`; `SURVEY.md` records that `length` counts **characters** at `src/runtime.rs:812`, which is a consequence of rank 5, not behaviour added here |
| nesting_recursion | covered — `refusal()` runs a four-stage pipeline per case, so a parser refusal is never mistaken for a runtime one; each survey test re-reads `SURVEY.md` from disk; `top_ten()` nests section bodies under headings |
| duplicate_missing_keys | covered — `the_top_ten_are_ranked…` refuses a duplicate rank (`BTreeSet::insert` returning false); `survey()` panics with the path when the document is missing; a stage returning `Ok` moves to the next stage rather than skipping |
| malformed_input | covered — malformed programs (unterminated string, unclosed `if`, stray `)`, unclosed list, `1 + "two"`, `1 / 0`) must each be refused by the named `Error` variant with the message naming the rule; `citations()` guards `start == index` and `end == index + 1`, so `:`, `x:`, `:/1` and `src/:1` are non-citations rather than panics |
| resource_limit | N/A — the phase writes one document and test files and reads a 408-line Markdown file; its only loop is linear in that document. `memory_model_test.rs` continues to cover the interpreter's recursion bound |

## Invariants touched

- **None.** No file under `src/` changed. No `Value` variant, `Error` variant,
  grammar production, `.rb` extension, `to … end` / `set x to …` / `say` form,
  trailing-comma rule or `{interp}` behaviour changed. `examples/`, `modules/` and
  `corpus/` were not modified.

## Known gaps / follow-ups

- **F2** — `for each` over a record runs to completion having iterated nothing
  (`src/interpreter.rs:1844`). A silent wrong answer, and the reason rank 3 could
  not be shown with a *failing* program (it prints nothing and exits 0).
- **F3** — the `record { … }` literal in `SPEC.md:112` and `docs/GRAMMAR.md:99`
  does not parse (`ParserError: Expected field name`), so `json.parse` is the only
  way a program builds a record. Needs a reviewer ruling on which side is wrong.
- **F5** — the unreviewed-work audit had been truncated by a prior resume run
  (`c95062eb`, +1/−927). It is intact again in the newer `main` form, and the
  parked attempt's intent is merged as described above rather than pinned to a
  non-ancestor SHA. The underlying defect — a resume tool that overwrites a file
  with `<the file content updated>` instead of appending — is outside this
  repository and is not fixed.
- **F6** — CI runs `cargo clippy -- -D warnings`, which lints no test target, so a
  lint in any file under `tests/` cannot fail the build today. One such lint is
  fixed here; the CI command is the finding.
- **F7** — `say "a{nothing}b"` prints `anothingb` and yields `Nothing`: a silent
  wrong answer in interpolation, the F2 class, and an interpreter change this
  document phase must not make.
- **The sample is 32 programs, not the world's software.** A gap blocking nothing
  in it can still be the only thing missing from someone's project; the ranking is
  a tie-break, not a proof of importance. `SURVEY.md` states this and three other
  limits under `## Not surveyed`.
- **Nothing in the survey was implemented.** Six of the ten are labelled
  `implementable-in-Redblue` and would be one file in `modules/`; that is a
  recommendation, and `SURVEY.md` says so rather than doing it.
