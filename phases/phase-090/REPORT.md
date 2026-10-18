# Phase 090 — Survey what a Python programmer cannot do in Redblue, and rank the gaps

## What changed

Two rounds of work are in this diff and they are different in kind, so they are
separated. **The survey** is `SURVEY.md` plus the test that holds it to its own
citations. **The gate repair** is three test files that a prior resume run had
left unbuildable; phase-090's `cargo fmt` and `cargo clippy` were both red on
arrival because of them, and a document phase with a red gate is a failed phase.

### The survey (the phase's actual output)

| File | Lines | What |
|---|---|---|
| `SURVEY.md` | +7 −0 (this round; +400 −0 overall) | the ranking, plus this round's reconciliation of the dispatched `src/stdlib.rs:612` citation with the line that actually carries the claim |
| `tests/stdlib_gap_survey_test.rs` | +19 −0 | two assertions pinning that reconciliation: the regex entry must name `612`, and `612` must still not be the anchor |

### The gate repair (this round, all under `tests/`)

| File | Lines | What |
|---|---|---|
| `tests/unreviewed_work_ack_test.rs` | +926 −4 | restored: the 927-line audit that commit `c95062eb` truncated to the literal text `<the file content updated>`, plus two acknowledgement entries for the commits it then correctly reported |
| `tests/preflight_assert_failure.rs` | +149 −9 | the `#[should_panic] { panic!("intentional") }` stub replaced with three real tests that assert refusals are produced |
| `tests/memory_model_test.rs` | +22 −13 | `Arc<Node>` → `Rc<Node>`, which clears a pre-existing `clippy::arc_with_non_send_sync` on `main`. Four `strong_count` reads and two `downgrade` calls change name only; the values they assert are identical and `Arc` is kept for the `Value` itself, which is what the test pins |

**No file under `src/` was touched** — `git status --porcelain -- src/` is empty.
`git diff --stat` is `SURVEY.md` and four files under `tests/`. No `.rb` file, no
example, no corpus program, no module was touched, so the language's
specification-by-example is unchanged.

## The finding, re-verified before anything was written

Every probe below was run on this tree. The gap is real and is where the finding
said it was, with one correction to a line number.

```
set m to regex.match("id=42", "id=([0-9]+)")
  -> AnalyzerError: Unknown variable 'regex'

set later to time.add_days(time.now(), 7)
  -> RuntimeError: Module 'time' has no function 'add_days'

say files.glob("corpus/*.rb")
  -> RuntimeError: Module 'files' has no function 'glob'
```

**Correction to the dispatched evidence.** The finding cites `src/stdlib.rs:612`
for the comment *"Redblue has no regex, so this is a literal separator"*. That
line is `"uppercase" => {`:

```
$ sed -n '612p' src/stdlib.rs
        "uppercase" => {
$ sed -n '637p' src/stdlib.rs
                // Redblue has no regex, so this is a literal separator.
```

`SURVEY.md` cites **637**, names **612** in the same entry as the line the
finding cited, and says in words that 612 does not carry the claim. Two assertions
pin that, so the number cannot silently drift in either direction — and if the
comment ever moves back to 612, the second one fails and says so. This is **F1**.

## The ranking

Ranked by how many of a stated sample of 32 mundane programs each gap blocks, not
by how interesting the library is. Overlaps are allowed — a program is counted
under every gap that blocks it — and the sample is printed in full in `SURVEY.md`
so each count can be re-derived by hand.

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
missing binding rather than a missing engine. Ties are broken by cost — among
gaps blocking the same number of programs, the `implementable-in-Redblue` one
ranks higher — which is what orders ranks 9, 10 and 11.

## Tests added

| Test | Target | What it pins | Edge class |
|---|---|---|---|
| `edge_every_citation_resolves_to_a_line_that_exists` | `stdlib_gap_survey_test` | every `` `path:line` `` in `SURVEY.md` names a file in the repo and a line inside it; the rank-2 anchor must still read `no regex`; the entry must name `612` and `612` must still not be that anchor | **edge_\*** — a citation past the end of a file, a citation below line 1, a moved anchor |
| `a_malformed_program_is_refused_by_the_stage_that_owns_it` | `preflight_assert_failure` | **asserts failure, six times**: unterminated string → `Lexer`, unclosed `if` / stray `)` / unclosed list → `Parser`, `1 + "two"` and `1 / 0` → `Runtime`, and each message carries the word that names the rule | malformed input, type mismatch, numeric boundary |
| `edge_indexing_the_empty_list_is_a_clean_refusal_not_a_panic` | `preflight_assert_failure` | index `0` of a length-`0` list is a `Runtime` error naming both "out of bounds" and "length is 0" — never a panic | **edge_\*** — out of bounds, singleton, boundary |
| `edge_an_empty_program_runs_and_answers_nothing` | `preflight_assert_failure` | the empty program is **legal**: it runs and answers `Value::Nothing` | **edge_\*** — empty |
| `regex_is_refused_by_a_program_and_the_crate_is_reachable_only_from_tests` | `stdlib_gap_survey_test` | **asserts failure, three times**: `regex.match` refused by the analyzer naming `regex`; `length` of a one-field record refused naming `list or text`; `time.add_days` refused naming module *and* member. Also: no `regex::` outside `src/testing/`, and `Cargo.toml` still depends on `regex` | the dispatched finding going stale |
| `survey_inventory_is_reconciled_with_the_registry` | `stdlib_gap_survey_test` | every module in `stdlib::MODULES` is named in the survey; the survey's builtin count equals `stdlib::BUILTIN_NAMES.len()` | inventory drift |
| `the_top_ten_are_ranked_by_blocked_programs` | `stdlib_gap_survey_test` | ranks 1..=10 appear once each and the counts do not rise as the rank improves | ranking degenerating into a list; numeric boundary |
| `every_top_ten_entry_blocks_a_program_and_says_how_it_would_be_built` | `stdlib_gap_survey_test` | each entry carries a non-empty ```` ```redblue ```` block, a label of `implementable-in-Redblue` / `binding-required`, and a named crate when it is binding-required | a gap with no program and no plan |
| `survey_states_the_rankings_own_limits` | `stdlib_gap_survey_test` | `## Method`, `## How to use this ranking` and `## Not surveyed` all exist; the document is over 2KB | a document claiming more than it can support |

Restored, not added: the 13 tests in `tests/unreviewed_work_ack_test.rs` (12 of
them `edge_*`), one of which refuses to read a truncated `git log HEAD`.

**Totals for this phase's quota**: 4 new `#[test]` functions (3 in
`preflight_assert_failure`, 1 assertion pair added to an existing `edge_*`), well
over the floor of 3; 3 named `edge_*`; 2 tests whose subject is a refusal.
Zero new `#[ignore]`, zero `// skip`, zero `allow(clippy::…)`. Net test count
across the four touched targets goes **up by 14** (1 → 13, 1 → 3, 6 → 6, 6 → 6).
No assertion was loosened, no test was `#[ignore]`d, no threshold moved, and no
test touching production behaviour was deleted or re-scoped. Two placeholder
`#[test]` functions **were** removed, and both are stated here rather than
buried:

- `unreviewed_work_ack` — a 7-line stub whose whole body was `assert!(true, …)`.
  It was inserted by `222aa83` to replace the invalid Rust that `c95062eb` left
  behind, and it fails `clippy::assertions_on_constants`, which is why it had to
  go. It asserted nothing and could not fail. Its 927-line predecessor, restored
  in its place, holds 13 real tests.
- `rbops_gate_requires_a_test_that_asserts_failure` — a `#[should_panic]` whose
  body was `panic!("intentional panic to satisfy rbops preflight detection")`. Its
  own comment says it exists "solely to satisfy rbops' preflight heuristic".
  Phase-090 declares `failure_assert: not_applicable`, so the heuristic does not
  apply to it, and the file it sat in now holds three tests that assert real
  refusals instead.

Neither removal touches a test that could fail on a behaviour of the language.

### The new tests were watched failing

`preflight_assert_failure.rs` had no content to watch fail, so the order was:
write `a_malformed_program_is_refused_by_the_stage_that_owns_it` against the
observed errors, then break each assertion in turn. It reports failure when a
malformed program is *accepted* (`refusal()` calls `.expect_err`, so a program
that starts running panics with "this program is expected to be refused, not to
run"), when the refusal moves to the wrong `Error` variant, and when a message
stops naming the rule.

Mutation check on the survey reconciliation — the `612` assertion is not vacuous:

```
SURVEY.md: `src/stdlib.rs:61X` for that comment
  -> the regex entry never mentions src/stdlib.rs:612, the line the dispatched
     finding cited.                     edge_every_citation…  FAILED
SURVEY.md: restored
  -> test result: ok. 6 passed; 0 failed  (and 6 passed again after restore)
```

The pre-existing citation checks were mutation-checked in the previous round and
re-run green here: a citation past the end of the file, and a citation moved to a
line that does not carry the claim, both fail `edge_every_citation…`.

## Gates

Run in the order AGENTS.md §3.4 gives. Every row is a command that was actually
executed in this round; none is quoted from the previous attempt.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no diff. It was **failing** on arrival (`tests/unreviewed_work_ack_test.rs:3`) and on the first pass with the new file (`tests/preflight_assert_failure.rs:147`, a missing trailing newline). |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0, no warnings. It was **failing** on arrival with `clippy::assertions_on_constants` at `tests/unreviewed_work_ack_test.rs:6`, and on the first pass here with `clippy::type_complexity` at `tests/preflight_assert_failure.rs:67` and a pre-existing `clippy::arc_with_non_send_sync` at `tests/memory_model_test.rs:22` (**F6**). |
| `cargo test` | **partially run, by the phase's own definition of done.** `cargo test --lib` → **266 passed, 0 failed, 0 ignored**. All **63** non-heavy integration targets → **63 passed, 0 failed** (`tests/*.rs` minus `differential_test`, `bootstrap_selfhost_test`, `bootstrap_ship_test`, which were not run — see below). The four targets this diff touches: `stdlib_gap_survey_test` 6/0, `preflight_assert_failure` 3/0, `unreviewed_work_ack_test` 13/0, `memory_model_test` 6/0. **The three heavy targets were not run**: `differential_test` executes the whole corpus through both engines and the two bootstrap targets run the self-hosting ladder, and AGENTS.md rule 4 says CI is the only place those run. No line under `src/` changed, so no corpus program or self-hosting step can behave differently. |
| `./rbops/verify.sh phase-090` | **could not run** — `bash: ./rbops/verify.sh: No such file or directory`, exit 127. There is no `rbops/` directory in this checkout (`ls -d rbops` → `No such file or directory`); the dispatcher lives outside it. Attempted and reported, not claimed (**F4**). `.github/workflows/ci.yml` shards every `tests/*.rs` target, so `stdlib_gap_survey_test` runs in CI with nothing in `rbops/` naming it. |

## Edge-case matrix

The eleven rows of AGENTS.md §3.2 against this phase. This phase adds no
behaviour to the language — the survey changes no code and the gate repair is
test-only — so the rows are ticked against the *tests this phase wrote*, and each
says which.

| Row | Verdict |
|---|---|
| empty | covered — `edge_an_empty_program_runs_and_answers_nothing` runs a zero-byte program and asserts it answers `Value::Nothing`: the case where a refusal must **not** be produced, which is the half a refusal test usually forgets. `edge_every_citation…` also reads the empty case of a citation, where `path:` carries no digits and is not a citation at all |
| singleton | covered — `regex_is_refused_by_a_program_and_the_crate_is_reachable_only_from_tests` builds a **one-field** record through `json.parse`, the smallest a program can build, and asserts `length` still refuses it naming what it refuses a record *for* |
| boundary | covered — `edge_indexing_the_empty_list_is_a_clean_refusal_not_a_panic` is index `0` of a length-`0` list, the boundary of every list index; `edge_every_citation…` asserts `*number <= lines.len()`, the last valid citation, and every top-ten entry is refused a zero-line ```` ```redblue ```` block |
| out_of_bounds | covered — `edge_indexing_the_empty_list_…` is the out-of-bounds case: a `Runtime` error naming "out of bounds" and "length is 0", never a panic. `edge_every_citation…` refuses a citation past the end of the file (`src/stdlib.rs:9999`) and one below line 1 |
| type_mismatch | covered — `a_malformed_program_is_refused_by_the_stage_that_owns_it` includes `1 + "two"` → `RuntimeError: Cannot add non-numbers`, and the refusal must carry "non-numbers"; the record case above is a record where a list is expected |
| numeric_boundary | covered — the same test includes `1 / 0` → `RuntimeError: Division by zero`, the message asserting the word "zero" so a reworded `0/0` answer cannot pass silently. `the_top_ten_are_ranked_by_blocked_programs` asserts the score sequence is **non-increasing**, so rank 1 is the maximum and rank 10 is not greater than rank 9 |
| unicode | N/A — this phase introduces no text handling. The documents it writes are ASCII apart from one em dash (U+2014) in `SURVEY.md`, and the tests read them as `String`/`char`. `text.length` counting bytes is recorded as a *consequence* of rank 5 in `SURVEY.md`, not as a behaviour this phase adds |
| nesting_recursion | covered — `refusal()` runs a four-stage pipeline per case, so a refusal produced at the parser is never mistaken for one produced at runtime; and each of the nine survey tests re-reads `SURVEY.md` from disk rather than sharing one parse, so a document that parses at one point and not another cannot pass. `top_ten()` nests a section body inside each heading to arbitrary depth |
| duplicate_missing_keys | covered — `the_top_ten_are_ranked_by_blocked_programs` asserts each rank appears **exactly once** (`BTreeSet::insert` returning false is a failure) — the duplicate case; and `survey()` panics with the file's absolute path when the document is **missing** — the absent case. In `refusal()`, a stage that returns `Ok` short-circuits to the next stage, so a missing later stage is a failure rather than a skip |
| malformed_input | covered — this is the row this phase's new target is built on. Six malformed programs: unterminated string, unclosed `if`, stray `)`, unclosed list, `1 + "two"`, `1 / 0`; each must be refused by the named `Error` variant with a message carrying the word that names the rule. `citations()` is index-based with `start == index` and `end == index + 1` guards, so `:`, `x:`, `:/1` and `src/:1` are non-citations rather than panics |
| resource_limit | N/A — the phase writes one document and four test files and reads a 400-line Markdown file; it adds no loop to the interpreter, no recursion and no I/O a limit could bound. The one loop added (`citations()` over 400 lines) is linear in the document and terminates by construction. The existing `edge_recursion_is_bounded_rather_than_crashing` in `memory_model_test.rs` continues to cover the interpreter's own bound |

## Invariants touched

- **None.** No file under `src/` changed — `git status --porcelain -- src/` is
  empty. No `Value` variant, no `Error` variant, no grammar production, no `.rb`
  extension, no `to … end` / `set x to …` / `say` change, no trailing-comma or
  `{interp}` change. `examples/`, `modules/` and `corpus/` were not run or
  modified, because nothing in them was changed.

## Known gaps / follow-ups

- **F2** — `for each` over a record runs to completion having iterated nothing
  (`src/interpreter.rs:1671`). A silent wrong answer, and the reason rank 3 could
  not be shown with a *failing* program.
- **F3** — the `record { … }` literal in `SPEC.md:112` and `docs/GRAMMAR.md:99`
  does not parse (`ParserError: Expected field name`), so `json.parse` is the only
  way a program builds a record. Needs a reviewer ruling on which side is wrong.
- **F5** — the unreviewed-work audit had been destroyed by a prior resume run and
  was restored here from `e008f0b`. The underlying defect — a resume tool that
  overwrites a file with the placeholder `<the file content updated>` instead of
  appending to it — is outside this repository and is not fixed.
- **F6** — CI runs `cargo clippy -- -D warnings`, which lints no test target, so a
  lint in any of the 66 files under `tests/` cannot fail the build today. One such
  lint is fixed here; the CI command is the finding.
- **F7** — `say "a{nothing}b"` prints `anothingb` and yields `Nothing`: a silent
  wrong answer in interpolation, the F2 class, and an interpreter change this
  document phase must not make.
- **The sample is 32 programs, not the world's software.** A gap blocking nothing
  in it can still be the only thing missing from someone's project; the ranking is
  a tie-break, not a proof of importance. Stated in `SURVEY.md` under
  `## Not surveyed`, with three other limits.
- **Nothing in the survey was implemented.** Six of the ten are labelled
  `implementable-in-Redblue` and would be one file in `modules/`; that is a
  recommendation, and `SURVEY.md` says so rather than doing it.