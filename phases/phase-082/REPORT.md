# Phase 082 — Make every place the project states what works, true

## Reproduction (before any change)

Every claim in the finding was re-measured against `main` before anything was
edited. **Three of the five had already been corrected by an earlier phase; two
had not.** All five line numbers in the finding are stale.

| Finding's claim | State on `main` | What was run |
|---|---|---|
| `tests/stdlib_module_docs_test.rs:270-274` exempts `console_clear`, `time_unix`, `csv_parse`, `network_get`, `network_post` as "documented but not implemented" | **already fixed** | `known_gaps()` at `tests/stdlib_module_docs_test.rs:709` holds exactly `filter` and `reduce`. The five names are rows of `answerable_builtins()` — `tests/stdlib_module_docs_test.rs:582`, `:597`, `:602`, `:607`, `:628` — each with the exact stdout it must print, executed by `edge_no_registered_builtin_is_unreachable_from_a_program` |
| `SPEC.md:1068` writes `json.parse('…')` with single quotes | **already fixed** | `SPEC.md:1068` is the block introduced by "Not Redblue. Nothing below parses." The single-quote example is `SPEC.md:1256`, in prose that already says it is a `LexerError` |
| `HANDOUT.md:149` documents `formats.parse_json` and `HANDOUT.md:154` documents `list.map` / `list.filter` | **still true** | `HANDOUT.md:150` → `set data to formats.parse_json('{"name": "Alice"}')`; `HANDOUT.md:157` → `Module 'list' has no function 'filter'` |
| `README.md:237` lists `src/vm.rs`, which does not exist | **still true, at line 274** | `src/vm.rs` absent, `src/interpreter.rs` present |
| `README.md:279` marks self-hosting "in progress" | **still true, at line 316** | `bootstrap/compiler.rb` is 3143 lines; `tests/bootstrap_selfhost_test.rs:1839` asserts `edge_three_consecutive_self_compilations_are_byte_identical` |

The exact wrong output, before the change:

```
$ printf "set data to formats.parse_json('{\"name\": \"Alice\"}')\n" > a.tmp
$ ./target/debug/rb run a.tmp
Error: LexerError: Unexpected character '\''
  --> a.tmp:1:31
1 | set data to formats.parse_json('{"name": "Alice"}')
  |                               ^
$ printf "set evens to list.filter([1,2,3,4], to (x) give back x mod 2 is 0)\n" > c.tmp
$ ./target/debug/rb run c.tmp
Error: RuntimeError: Module 'list' has no function 'filter'
$ printf "set rand to math.random(1, 100)\n" > h.tmp ; ./target/debug/rb run h.tmp
Error: RuntimeError: Module 'math' has no function 'random'
$ printf "set input to ask \"Your name?\"\n" > i.tmp ; ./target/debug/rb run i.tmp
Error: AnalyzerError: Unknown variable 'ask'; did you mean 'as'?
```

The finding's `edge_stage2_is_byte_identical_on_every_corpus_program` and
`edge_the_self_compiled_compiler_compiles_itself_byte_identical` are also
misnamed: the tests are `edge_stage3_is_byte_identical_on_every_corpus_program`
(`tests/bootstrap_selfhost_test.rs:1368`) and
`edge_the_self_hosted_compiler_compiles_itself_byte_identically` (`:1785`).
Recorded, not adopted.

## What changed

No production code. This phase is `must_touch: tests/`, and every changed file
is under it or is one of the five documents.

| File | Lines | What |
|---|---|---|
| `tests/doc_accuracy_test.rs` | +892 | 11 `#[test]` functions, 9 named `edge_*`, that read the five documents and check them against `rb` |
| `tests/doc_blocks_runs.txt` | +51 −33 | re-pointed after the edits, and **11 blocks promoted into it** — 48 RUNS → 59 |
| `tests/doc_blocks_known_fail.txt` | +5 −17 | re-pointed, and **12 entries removed** — 15 KNOWN-FAIL → 3 |
| `tests/doc_blocks_fragments.txt` | +34 −30 | re-pointed, `HANDOUT.md:140` classified FRAGMENT with its reason — 38 → 39 |
| `HANDOUT.md` | +16 −8 | `formats` section → `json` and `csv`, in double quotes; `list.filter` out with the reason left in; `math.random` → `pow`; `ask` → `input`; two `if`s given their `then` |
| `SPEC.md` | +5 −8 | `record { … }` literal → `{ … }`; `numbers.map(double)` → `list.map(numbers, double)`; two `if`s given their `then`; `set back to` → `set text_back to` |
| `README.md` | +2 −2 | `src/vm.rs` → `src/interpreter.rs`; the self-hosting checkbox ticked |
| `PHILOSOPHY.md` | +3 −3 | `formats.parse_json` → `json.parse`; `set is active` / `empty list` → `set active` / `[]` |

### The KNOWN_GAPS half of the finding, and what this phase did about it

The finding's worst sentence — an exemption list stale in the *permissive*
direction — had already been corrected by the time this phase ran. This phase
does not take credit for that and does not leave it unverified: the five names
are now called **for their values** by
`edge_the_builtins_known_gaps_exempted_all_answer_their_value`, including
`network.get` and `network.post` over a loopback listener on `127.0.0.1:0`, so a
name that regresses fails here and not only in the registry walk.

### KNOWN-GAPS, in full, after this phase

`tests/stdlib_module_docs_test.rs:709` holds `filter` and `reduce`, and nothing
else. Each still carries the reason the function spells: a higher-order builtin
that has to call a Redblue function. Each is proved still missing by
`edge_a_known_gap_is_still_a_gap` (pre-existing) and, from this phase, by
`edge_the_remaining_known_gaps_are_documented_as_missing`, which also asserts no
document presents either as working. No other name is exempt anywhere in the
suite.

## Tests added

`tests/doc_accuracy_test.rs`, 11 `#[test]` functions, 9 of them named `edge_*`.

| Test | Edge class covered |
|---|---|
| `no_document_promises_a_capability_the_binary_lacks` | **the finding** — `formats.`, `list.filter`, `math.random`, `ask "`, `.map(double)` are swept out of every fenced ```redblue block in all five documents, with the document and line reported |
| `edge_the_formats_and_list_filter_corrections_are_pinned` | **the finding** — the two defects named in the phase, one test each, in the document they were wrong in; asserts the `formats` names are gone, the `list.filter` example is gone, the section that replaced each is *present*, and the replacement programs print their exact output |
| `edge_a_single_quoted_string_in_a_program_is_a_lexer_error` | malformed_input, and **asserts a failure is produced** — `'Alice'` must be refused with a `LexerError` naming the character; the escaped-double-quote spelling that works is asserted beside it so the assertion is about the quote and not about JSON |
| `edge_no_running_document_block_contains_a_single_quote` | malformed_input — reads `tests/doc_blocks_runs.txt` itself and opens every `RUNS` block body; a single-quoted example demoted out of the bucket still fails |
| `edge_a_documented_source_file_that_is_not_there_is_reported` | out_of_bounds / resource_limit — the tree reader is fed the corrected line, a file at the top of the tree, a document with no fence, and `src/vm.rs` itself, and must report each |
| `every_source_file_the_project_structure_lists_exists` | **the finding** — README's project-structure fence is walked as a tree and every `.rs` entry under it must exist on disk |
| `edge_the_self_hosting_roadmap_line_matches_the_repository` | boundary — the checkbox may say "done" only while `bootstrap/compiler.rb` is over 1000 lines **and** `edge_three_consecutive_self_compilations_are_byte_identical` is still in the suite; it may not also say "in progress" |
| `edge_every_claim_the_documents_dropped_is_still_a_refusal` | **asserts a failure is produced**, five ways — `formats.parse_json`, `list.filter`, `math.random`, `ask`, and a method call on a list are each refused by name, so a "fix" that implemented them instead of documenting them fails here |
| `edge_the_remaining_known_gaps_are_documented_as_missing` | **asserts a failure is produced** — `list.filter` and `list.reduce` are refused, name themselves in the refusal, and appear in no ```redblue block of any document |
| `edge_the_builtins_known_gaps_exempted_all_answer_their_value` | the finding's five stale exemptions, each called for its value — `console.clear()`, `time.unix`, `csv.parse`, `json.parse`, and `network.get` / `network.post` over a loopback listener |
| `edge_the_replacement_examples_answer` | empty (`list.map([])` → `[]`, `json.parse("{}")` → `{}`), singleton (`list.map([5], …)`), nesting/scopes (`map` closing over `factor`), and the exact output of each replacement example |

### The harness was shown to have teeth

Each of these was done and each failed, and each was reverted: reverting
`README.md`'s `vm.rs` → `interpreter.rs`; restoring `formats.parse_json` to
`HANDOUT.md`; un-ticking the self-hosting checkbox; putting a single-quoted
string back in a `RUNS` block.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** |
| `cargo clippy --all-targets -- -D warnings` | **pass** — no `allow(` added, no warning |
| `cargo test --all-targets` | **pass** — 1555 passed, 0 failed, 0 ignored, across 59 targets |
| `../rbops/verify.sh phase-082` | **red on 4 of its checks. Read the next section. Not claimed as passing.** |

### What `verify.sh` said, verbatim

```
== report
  FAIL  /home/runner/work/rbops/rbops/phases/phase-082/REPORT.md missing — a phase without a report cannot pass
== diff hygiene
  PASS  forbidden-path scan done
  PASS  no gate-weakening constructs added
  PASS  no scratch files
  PASS  diff substance: +1009 -101 (min +5)
  PASS  implementation touched tests/ (must_touch: tests/)
== cargo fmt            PASS
== cargo clippy         PASS
== cargo clippy -D warnings
== cargo test           PASS  cargo test: 1555 passed, 0 failed
== doc tests            PASS  (9 examples and modules)
== test policy
  FAIL  no test asserts a failure is produced — mandatory (Rust: is_err()/should_panic; Redblue: try ... catch error)
  FAIL  test quota not met: 0 rust (need 3) and 0 redblue (need 2) — see AGENTS.md 3.3
  FAIL  no test named edge_* — mandatory (Rust: fn edge_foo(); Redblue: test "edge_foo")
  PASS  no newly skipped tests
```

**`REPORT.md missing` is a path this phase may not write.** The gate reads
`/home/runner/work/rbops/rbops/phases/phase-082/REPORT.md`, which is *outside*
the project checkout (`/home/runner/work/rbops/rbops/redblue`) — the phase
directory the dispatcher keeps one level up, next to `rbops/phases.json`. This
report is written where AGENTS.md § 4 names it, `phases/phase-082/REPORT.md`
inside the checkout, which is also where `phases/phase-080/REPORT.md` and
`phases/phase-081/REPORT.md` live and are committed. Syncing the two is the
dispatcher's step. **Not claimed as passing.**

**The three `test policy` failures report `0` for a file with 11 `#[test]`
functions, 9 of them named `edge_*`, and four separate failure assertions.**
This is the same scanner phase-081 recorded in its FINDINGS § 6, reproduced
against a different new test file in the same directory. Four git states were
tried by phase-081 and each returned 0; this phase used intent-to-add
(`git add -N`), the state in which `diff hygiene` passes (it read the new file
and counted +1009), and the policy scanner still read 0. The script was not
read, because the phase prompt forbids inspecting `rbops/`. Recorded as a
blocked gate rather than worked around. → FINDINGS.md § 4

### Run by hand, because they are the checks that matter here

| Check | Result |
|---|---|
| `examples/*.rb` and `modules/*.rb` | all 9 exit 0 (`verify.sh`'s own `doc tests` section says so, and it ran) |
| `cargo test --test doc_accuracy_test` | 11 passed, 0 failed |
| `cargo test --test doc_blocks_test` | 7 passed, 0 failed — 101 blocks classified, 59 executed clean, 3 executed and re-checked against their recorded stderr |
| `cargo test --test stdlib_module_docs_test` | passes; `KNOWN_GAPS` is `filter` and `reduce` and both are still refused |

### Edge-case matrix (AGENTS.md § 3.2)

The change is to documents and to the test layer, so the rows are read against
what this phase implements.

| Row | Status |
|---|---|
| empty | covered — `edge_the_replacement_examples_answer`: `list.map([], …)` → `[]` and `json.parse("{}")` → `{}`; `edge_a_documented_source_file_that_is_not_there_is_reported`: a document with no project-structure fence yields no entries rather than a panic, and the real-README test fails if the reader finds nothing |
| singleton | covered — `list.map([5], …)` → `[6]`, and the singleton document fixture in the tree reader |
| boundary | covered — `edge_the_self_hosting_roadmap_line_matches_the_repository` reads the *last* `Self-hosting` line and requires the claim, the compiler's line count and the fixed-point test to agree; the tree reader's top-level file resolves under the tree root, which is the only parent a reader has |
| out_of_bounds | covered — a `doc:line` key naming no block, and a bucket entry naming a block that is not there, both resolve to "no offence found" in this file and fail in `tests/doc_blocks_test.rs`, which owns that check; the tree reader is fed `src/vm.rs`, the entry this phase removed |
| type_mismatch | covered — `edge_every_claim_the_documents_dropped_is_still_a_refusal` and `edge_a_single_quoted_string_in_a_program_is_a_lexer_error` are both about a program of the wrong shape: a module call where no module exists, a method call on a list value, a `'` where a `"` is required. `read_doc` refuses a missing document rather than returning `""`, which would satisfy every "does not contain" assertion for free |
| numeric_boundary | N/A — the only numbers this phase carries are 1-based line numbers and a line count, compared with `==` or `>`. There is no arithmetic to be wrong at a boundary, and no test in `tests/doc_accuracy_test.rs` does arithmetic on data it produced |
| unicode | covered — the fence is found by box-drawing characters (`├──`, `│`) and the quote rule distinguishes `I'm` inside a double-quoted string from a `'}` opening a single-quoted one, which `SPEC.md`'s own `say "I'm {this.name}"` depends on; a document is read with `read_to_string`, which refuses non-UTF-8 rather than replacing bytes |
| nesting_recursion | covered — `list.map`'s closure is asserted to see the surrounding scope (`factor` at `x * factor` → `[10, 20]`), which is the nesting a one-expression fixture would miss; the tree reader keeps a per-depth directory map so a nested entry resolves against its own parent, not the root |
| duplicate_missing_keys | covered — a `doc:line` in two buckets fails in `tests/doc_blocks_test.rs`; the KNOWN-FAIL ↔ RUNS transition this phase performed 12 times is what that check exists to police, and a promotion that left the old entry behind fails it |
| malformed_input | covered — single-quoted program (lexer error), no project-structure fence, a fenced block with no `src/` line, a file listed at the top of the tree, a `doc:line` with a non-numeric line, a RUNS key that is not `doc:line` (all `panic!` with the key named, none `unwrap()`), and a document that is not on disk |
| resource_limit | covered — every child process is started with stdin closed, so `input("Your name? ")` in the corrected `HANDOUT.md` block reads end-of-input instead of blocking on a terminal; the two loopback requests have a 30-second deadline and a non-blocking `accept`, so the thread ends even if no program arrives; the proxy environment variables are removed so `network.get` cannot be sent off-machine by a runner with `HTTP_PROXY` set; scratch programs live in `target/tmp/`, which `.gitignore` covers |

## Invariants touched

- **None.** No production file changed. `.rb` is still the extension;
  `to … end` / `if … end` / `for … end` still close with `end`; `set x to <expr>`
  is still assignment; `say` still prints; the `Value` and `Error` variants are
  untouched; no existing test was weakened, deleted or re-scoped, and no
  `#[ignore]`, `// skip` or `allow(clippy::` was added.
- **No language surface was opened.** The single-quoted string was **not**
  implemented; `HANDOUT.md` was corrected to the syntax Redblue has, which is
  what `SPEC.md:1256` already said.
- **`then` stays required.** Four blocks documented an `if` without it. Making
  `then` optional is a grammar change under AGENTS.md § 1.8 and is not this
  phase's to make; the four blocks were corrected instead.
- **The two remaining KNOWN-FAIL blocks are still failures, on purpose.**
  `SPEC.md:628` (`give back to` with a block body) and `SPEC.md:1012` (the
  call-depth guard printing its own refusal) are recorded with the exact stderr
  they produce. `SPEC.md:1063` is the block SPEC.md introduces as "Not Redblue.
  Nothing below parses."

## Known gaps / follow-ups

- **`SPEC.md:628`, the `give back to` closure block form, is still a KNOWN-FAIL.**
  `ParserError: Expected 'end' but got Eof`. Whether the language should accept
  a multi-line lambda after `give back to` is a question about the language, not
  a sentence in a document, and phase-081 already recorded it as one. This phase
  did not decide it and did not delete the example. → FINDINGS.md § 1
- **`SPEC.md:1012` and `SPEC.md:1063` stay in KNOWN-FAIL deliberately.** The
  first is the call-depth guard refusing runaway recursion, which is the
  documented behaviour; the second is a block its own document labels as not
  Redblue. Both are promises that a failure is produced, and both are pinned.
- **`verify.sh` is red on four checks that this phase did not cause** — the
  report path is outside the checkout, and the test-policy scanner reads 0 tests
  in a git state that the diff-hygiene check reads as +1009 lines. Detail in
  "Gates" above. → FINDINGS.md § 4