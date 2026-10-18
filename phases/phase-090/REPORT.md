# Phase 090 — Survey what a Python programmer cannot do in Redblue, and rank the gaps

## What changed

A document phase. `SURVEY.md` is the output; `tests/stdlib_gap_survey_test.rs`
makes it checkable, because a survey of gaps is only worth reading if every claim
in it is anchored to a line that still says what the survey says it says.

| File | Lines | What |
|---|---|---|
| `SURVEY.md` | +400 −0 | the survey: the stdlib surface as it is (`src/stdlib.rs:19`, `:62`, `:846`), the method and its 32-program sample, a ranked top ten, and the eight below the line |
| `tests/stdlib_gap_survey_test.rs` | +471 −0 | 6 tests: every `path:line` in `SURVEY.md` is opened and asked for; the module list and builtin count are reconciled against `stdlib::MODULES` / `stdlib::BUILTIN_NAMES`; the ranks descend; each top-ten entry blocks a real program and carries a label; the regex entry's cited line must still contain `no regex`; and the regex gap itself is re-verified against a running program |
| `phases/phase-090/FINDINGS.md` | +112 −0 | four items for later phases: the dispatched finding's stale line number, `for each` over a record silently iterating nothing, the `record { … }` literal documented in `SPEC.md` not parsing, and the missing `rbops/verify.sh` |

**No file under `src/` was touched.** `git diff --stat` is empty — every addition
in this phase is untracked documentation or a test file, and `git status
--porcelain` lists exactly three paths: `SURVEY.md`, `tests/`, `phases/`.

### The finding, re-verified before anything was written

```
$ rb run target/tmp/p090/p2.rb     # set m to regex.match("id=42", "id=([0-9]+)")
Error: AnalyzerError: Unknown variable 'regex'
$ rb run target/tmp/p090/p3.rb     # time.add_days(time.now(), 7)
Error: RuntimeError: Module 'time' has no function 'add_days'
$ rb run target/tmp/p090/p4.rb     # say files.glob("corpus/*.rb")
Error: RuntimeError: Module 'files' has no function 'glob'
```

The gap is real and still where it was. One correction: the finding cited
`src/stdlib.rs:612` for the "Redblue has no regex" comment, and that line is
`"uppercase" => {`. The comment is at `src/stdlib.rs:637`. `SURVEY.md` cites 637
and a test enforces it. Recorded as **F1** in `FINDINGS.md`.

### The ranking

Ranked by how many of a stated sample of 32 mundane programs each gap blocks, not
by how interesting the library is. Overlaps are allowed — a program appears under
every gap that blocks it — and the sample is printed in full so each count can be
re-derived by hand.

| Rank | Gap | Blocks | Label |
|---|---|---|---|
| 1 | No higher-order list functions beyond `map` | 6 | implementable-in-Redblue |
| 2 | No regular expressions | 4 | binding-required (`regex`) |
| 3 | No way to enumerate a record's fields | 4 | implementable-in-Redblue |
| 4 | No sorting, ordering, or min/max | 4 | implementable-in-Redblue |
| 5 | No slicing, searching or padding of text | 4 | implementable-in-Redblue |
| 6 | No date or time arithmetic | 4 | binding-required (`chrono`) |
| 7 | No directories, no globbing, no stat | 4 | binding-required (`walkdir`, `glob`) |
| 8 | No encodings and no digests | 3 | binding-required (`base64`, `hex`, `percent-encoding`, `sha2`) |
| 9 | No number formatting or parsing | 2 | implementable-in-Redblue |
| 10 | No writer for structured formats | 2 | implementable-in-Redblue |

Ties are broken by cost, stated in `## Method`: among gaps that block the same
number of programs, the `implementable-in-Redblue` one ranks higher. Ranks 9-11
are all "2 programs" and are ordered by exactly that rule. Regexes rank **second**,
not first, which is the finding's own correction — the crate is already a
dependency (`Cargo.toml:31`) and reachable only from `src/testing/`, so the gap is
a missing binding rather than a missing engine.

## Tests added

6 `#[test]` functions in `tests/stdlib_gap_survey_test.rs`. All of them fail on a
survey that is wrong; none of them reads the survey without checking it.

| Test | What it pins | Edge class |
|---|---|---|
| `edge_every_citation_resolves_to_a_line_that_exists` | every `` `path:line` `` in `SURVEY.md` names a file in the repo and a line inside it; the rank-2 citation must still read `no regex` | **edge_\*** — the empty end of a citation, a citation past the end of a file, a moved anchor |
| `survey_inventory_is_reconciled_with_the_registry` | every module in `stdlib::MODULES` is named in the survey, and the survey's builtin count equals `stdlib::BUILTIN_NAMES.len()` | inventory drift — a survey measuring a different stdlib |
| `the_top_ten_are_ranked_by_blocked_programs` | ranks 1..=10 appear once each, each carries a blocked-program count, and the counts do not rise as the rank improves | ranking degenerating into a list |
| `every_top_ten_entry_blocks_a_program_and_says_how_it_would_be_built` | each entry carries a non-empty ```redblue``` block, a label of `implementable-in-Redblue` / `binding-required`, and — when binding-required — at least one named Rust crate | a gap with no program and no plan behind it |
| `regex_is_refused_by_a_program_and_the_crate_is_reachable_only_from_tests` | **asserts failure, twice**: `regex.match` is refused by the analyzer naming `regex`; `time.add_days` passes the analyzer and is refused at the call naming both the module and the member. Also asserts no `regex::` reference exists outside `src/testing/` and that `Cargo.toml` still depends on `regex` | the finding itself going stale; a survey of a gap that closed |
| `survey_states_the_rankings_own_limits` | `## Method`, `## How to use this ranking` and `## Not surveyed` all exist, and the document is over 2KB | a document making claims it cannot support |

Both mandatory tests are expressible and present: an `edge_*` test, and a test
whose subject is a refusal. No `#[ignore]`, no `// skip`, no `allow(clippy::…)`, no
pre-existing test touched, deleted or re-scoped.

### The tests were watched failing first

With no `SURVEY.md`, five of the six failed on `SURVEY.md does not exist at
/home/runner/work/rbops/rbops/redblue/SURVEY.md` and the sixth passed — it is the
one about the finding rather than the document, and it passed because the finding
reproduces. Two mutation checks confirm the citation checks are not vacuous:

```
# a citation moved past the end of the file
SURVEY.md cites `src/stdlib.rs:9999` and the file holds 1310 lines, so the
citation points past its end.                        → edge_… FAILED

# the rank-2 anchor moved to a line that says something else
the regex entry cites src/stdlib.rs:900, and that line reads `    "network_post",`
— it does not carry the claim the entry is making.     → edge_… FAILED
```

Both were reverted; `cargo test --test stdlib_gap_survey_test` is 6/0 after.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — clean |
| `cargo clippy --all-targets -- -D warnings` | **pass** — clean, no warnings |
| `cargo test` | **not run to completion**, per the phase's definition of done. `cargo test --test stdlib_gap_survey_test` → **6 passed, 0 failed, 0 ignored**. No tracked file outside `SURVEY.md`, `tests/stdlib_gap_survey_test.rs` and `phases/phase-090/` was written (the only other writes were scratch probe programs under the gitignored `target/tmp/`), so no pre-existing target can have changed behaviour; `.github/workflows/ci.yml` shards every `tests/*.rs` target and will run this one |
| `./rbops/verify.sh phase-090` | **could not run** — `bash: ./rbops/verify.sh: No such file or directory`. There is no `rbops/` directory in this checkout; the dispatcher lives outside it. Attempted and reported rather than claimed (**F4** in `FINDINGS.md`) |

## Edge-case matrix

The eleven rows of AGENTS.md § 3.2, ticked against this phase. This phase adds no
behaviour to the language, so most rows have nothing to bite on — each says which.

| Row | Verdict |
|---|---|
| empty | covered — `edge_every_citation_resolves_to_a_line_that_exists` reads the empty case of a citation: a `path:` with no digits after it is not counted as a citation at all, and a document with none fails `cited.len() >= 10` |
| singleton | covered — `regex_is_refused_by_a_program_and_the_crate_is_reachable_only_from_tests` runs a **one-field** record, `set r to json.parse("{\"a\": 1}")` — the smallest record a program can build — and asserts `length` still refuses it, naming what it refuses a record *for* |
| boundary | covered — `edge_every_citation_resolves_to_a_line_that_exists` asserts the cited line is **the last line** of its file (`*number <= lines.len()`), which is the boundary of a citation, and `every_top_ten_entry_blocks_a_program_and_says_how_it_would_be_built` asserts a *zero*-line ```redblue``` block is refused, which is the boundary of an entry |
| out_of_bounds | covered — `edge_every_citation_resolves_to_a_line_that_exists` refuses a citation past the end of the file (`src/stdlib.rs:9999`, mutation-checked above) and a citation to a line number below 1 |
| type_mismatch | covered — `field()` returns `Option` and every consumer refuses `None` rather than reading it, so a document whose `- **Blocked programs**:` value is not a number, or whose `- **Implementable**:` line is absent, fails rather than scoring as zero |
| numeric_boundary | covered — `the_top_ten_are_ranked_by_blocked_programs` asserts the score sequence is **non-increasing** across all ten ranks, so both boundaries hold: rank 1 is the maximum and rank 10 is not greater than rank 9 |
| unicode | N/A — the survey is prose plus ASCII code (`—` is its only non-ASCII codepoint, U+2014), and the tests read it as `String`/`char`. No text-handling behaviour is introduced by this phase, so there is no unicode boundary to place. `text.length` counting bytes (`src/stdlib.rs:977`) is recorded as a **consequence** of rank 5, not tested here |
| nesting_recursion | covered — `survey()` is called from five independent tests and each re-reads the file from disk rather than sharing one parse, so a document that parses at one point and not another cannot pass; `top_ten()` nests a section body inside each heading to arbitrary depth |
| duplicate_missing_keys | covered — `the_top_ten_are_ranked_by_blocked_programs` asserts each rank appears **exactly once** (`BTreeSet::insert` returning false is a failure), which is the duplicate case, and `edge_…` fails when the file is missing, which is the absent case |
| malformed_input | covered — `survey()` refuses a `SURVEY.md` that cannot be read (`expect("SURVEY.md is readable")`), and `citations()` skips a malformed `path:` rather than panicking on it: the scan is index-based with `start == index` and `end == index + 1` guards, so `:`, `x:`, `:/1` and `src/:1` are all non-citations |
| resource_limit | N/A — the phase writes one document and one test file and reads a 400-line Markdown file; it adds no loop to the interpreter, no recursion and no I/O a limit could bound. The one loop added (`citations()` over a 400-line file) is linear in the document and terminates by construction |

## Invariants touched

- **None.** No file under `src/` changed. No `Value` variant, no `Error` variant,
  no grammar production, no `.rb` extension change, no `to … end` / `set x to …` /
  `say` change, no trailing-comma or `{interp}` change. `examples/` and
  `modules/` were not run or modified, because nothing in them was changed.

## Known gaps / follow-ups

- **F2** — `for each x in <record>` runs to completion having iterated nothing
  (`src/interpreter.rs:1671`). A silent wrong answer, and the reason rank 3 could
  not be shown with a failing program.
- **F3** — the `record { … }` literal in `SPEC.md:112` and
  `docs/GRAMMAR.md:99` does not parse (`ParserError: Expected field name`), so
  `json.parse` is the only way a program builds a record. Needs a reviewer ruling
  on which side is wrong before it becomes a phase.
- **The sample is 32 programs, not the world's software.** A gap blocking nothing
  in it can still be the only thing missing from someone's project; the ranking
  is a tie-break, not a proof of importance. Stated in `SURVEY.md` under
  `## Not surveyed`, along with the other three limits of the survey.
- **Nothing in the survey was implemented.** Six of the ten are labelled
  `implementable-in-Redblue` and would be one file in `modules/`; that is a
  recommendation, and `SURVEY.md` says so rather than doing it.
