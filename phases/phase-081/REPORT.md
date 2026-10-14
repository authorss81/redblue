# Phase 081 — Execute every ```redblue block in the five documents that define the language

## Reproduction (before any change)

The finding was reproduced by extracting every fenced `redblue` block from the
five documents and running each one through `target/debug/rb run` from a scratch
working directory:

```
$ python3 target/tmp/p081/extract.py
total 101 run-ok 46 fail 55
SPEC.md:89   Error: ParserError: Expected field name
SPEC.md:1066 Error: AnalyzerError: Unknown variable 'async'
HANDOUT.md:149 Error: LexerError: Unexpected character '\''
HANDOUT.md:155 Error: RuntimeError: Module 'list' has no function 'filter'
…
```

101 blocks, 46 of which ran and **55 that do not**. Nothing in `tests/` executed
any of them: the only references to documentation anywhere in the suite are the
two hand-quoted citations in `tests/for_range_test.rs:2` and
`tests/formatter_test.rs:2008`.

Two measurements in the finding differ from today's tree, and both are recorded
rather than quietly adopted — see FINDINGS.md § 1 and § 2. **101 blocks, not
97** (`SPEC.md` 68, not 64), and `SPEC.md:1066` today answers
`AnalyzerError: Unknown variable 'async'`, not `ParserError: Unexpected token
Wait`. The finding reproduces; its line numbers are stale.

## The measured classification — the number is 15, not 58

The first deliverable is an honest classification, and the honest classification
is much smaller than "55 failures". Every block is in exactly one of three
checked-in files under `tests/`:

| Bucket | Count | What it means |
|---|---|---|
| **RUNS** (`tests/doc_blocks_runs.txt`) | **48** | executed by the suite, exits 0 |
| **FRAGMENT** (`tests/doc_blocks_fragments.txt`) | **38** | not executed, with the reason written beside it |
| **KNOWN-FAIL** (`tests/doc_blocks_known_fail.txt`) | **15** | executed, exited non-zero, exact first line of stderr recorded |
| total | **101** | |

Per document — RUNS · FRAGMENT · KNOWN-FAIL:

| Document | Blocks | RUNS | FRAGMENT | KNOWN-FAIL |
|---|---|---|---|---|
| `SPEC.md` | 68 | 32 | 28 | 8 |
| `docs/GRAMMAR.md` | 9 | 7 | 2 | 0 |
| `PHILOSOPHY.md` | 9 | 0 | 8 | 1 |
| `HANDOUT.md` | 8 | 2 | 0 | 6 |
| `README.md` | 7 | 7 | 0 | 0 |

**These 15 are not 15 features, and this phase implemented none of them.** They
are 15 sentences in five documents that `rb` contradicts. What follows is the
list a human can act on, in the order it is worth acting in.

### The 15, grouped by what they are

**`if` without `then` — 4 entries, and the grammar document contradicts itself
on it.** `docs/GRAMMAR.md:219` writes `if_statement = 'if' expression 'then'`,
and the parser requires it; `if` with the body on the next line is
`ParserError: Expected 'then' but got 'say'`. Four blocks write it the other way:

| Entry | First line of stderr |
|---|---|
| `SPEC.md:570` | `Error: ParserError: Expected 'then' but got 'give'` |
| `SPEC.md:977` | `Error: ParserError: Expected 'then' but got 'say'` |
| `HANDOUT.md:42` | `Error: ParserError: Expected 'then' but got 'say'` |
| `HANDOUT.md:140` | `Error: ParserError: Expected 'then' but got 'say'` |

`docs/GRAMMAR.md:600` — the *grammar's own* `if/else` example — omits `then`
too, and is classified FRAGMENT because it also depends on a binding from the
preceding block. Fixing the four above is a documentation change, not a
language change; adding an optional `then` would be a grammar change and is not
this phase's to make.

**A method call on a list value — 2 entries.** `numbers.map(double)` reads a
list as if it were a module: `RuntimeError: Module 'numbers' has no function
'map'`. `SPEC.md:607` documents it; `HANDOUT.md:155` documents `list.filter`,
which fails the same way (`RuntimeError: Module 'list' has no function
'filter'`). The dotted *module* spelling `list.map(...)` works —
`docs/GRAMMAR.md:541` is a RUNS entry and prints nothing and exits 0 — so the
language has one of the two spellings the documents use and not the other. The
finding attributes `HANDOUT.md:155` to phase-077; `SPEC.md:607` is the same
defect in SPEC.md and is recorded here for the auditor.

**Literals the lexer does not accept — 1 entry.** `SPEC.md:89` is the value
catalogue, and it binds every name it reads: a text literal spanning two lines
and a `record { name: …, age: … }` literal. It fails with
`Error: ParserError: Expected field name`.

**Reserved words used as names — 2 entries.** `set back to …`
(`SPEC.md:1267`, `Error: ParserError: Expected variable name, found reserved
word 'back'`) and `set is active to yes` (`PHILOSOPHY.md:132`,
`Error: ParserError: Expected variable name, found reserved word 'is'`). Both
documents use a reserved word as an ordinary identifier in a block where the
surrounding prose gives no hint that a different name was meant.

**Functions the documents name that do not exist — 3 entries.** `math.random`
(`HANDOUT.md:133`, `RuntimeError: Module 'math' has no function 'random'` — and
`SPEC.md:1164` says in as many words that "`math.random` is not implemented",
so the two documents disagree and the handout is the stale one); `print` and
`ask` (`HANDOUT.md:161`, `AnalyzerError: Unknown variable 'ask'; did you mean
'as'?`); a single-quoted string (`HANDOUT.md:149`, `LexerError: Unexpected
character '\''`).

**Two entries that are recorded because their *failure* is the promise:**

- `SPEC.md:1066` is a block SPEC.md itself introduces as "Not Redblue. Nothing
  below parses." Its recorded value is
  `Error: AnalyzerError: Unknown variable 'async'`. If that block ever starts
  parsing, the suite fails — which is the correct outcome, because the sentence
  above it would then be a lie.
- `SPEC.md:1015` is the example under "A resource limit is not a failure of the
  guarded call". It prints `Error: RuntimeError: Maximum call depth of 1000
  reached while calling 'endless'`, which is the documented behaviour of the
  call-depth guard. Recording it pins the guard: a guard that stopped firing
  would silently turn a runaway recursion into `nothing`.

**The 38 fragments are mostly not defects**, and the file says which is which
on every line. Two reasons appear:

- `excerpt` (36 entries) — the block is not a program: an elided construct, a
  placeholder body (`// statements`, `// ...`, `// Implementation`), or names
  the surrounding prose introduces and the block never binds. `PHILOSOPHY.md`
  is 8 of 9 this way, and three of its blocks (`PHILOSOPHY.md:16`, `:228`, and
  `:9`) are the pseudo-English the document itself is arguing *against*.
- `network` (2 entries: `SPEC.md:894`, `SPEC.md:1241`) — the block is a program
  and performs a live HTTP request to `api.example.com`. AGENTS.md § 3.1.4
  forbids a test that depends on the network, so these two are classified rather
  than executed and **their outcome is unverified**. A reader must not take the
  fragment bucket as "runs".

## What changed

No production code. This phase is `must_touch: tests/`, and every changed file
is under it.

| File | Lines | What |
|---|---|---|
| `tests/doc_blocks_test.rs` | +784 | the extractor, the three-bucket table reader, the sandbox runner, and 7 `#[test]` functions |
| `tests/doc_blocks_runs.txt` | +79 | the RUNS bucket: 48 `doc:line` keys, and the promise that each exits 0 |
| `tests/doc_blocks_fragments.txt` | +81 | the FRAGMENT bucket: 38 `doc:line` keys, each with a written reason |
| `tests/doc_blocks_known_fail.txt` | +50 | the KNOWN-FAIL bucket: 15 `doc:line` keys, each with the exact first line of stderr it prints today |

### Design decisions, stated

- **The buckets are DATA, matched by `doc:line`, not a pattern re-derived at
  test time.** `tests/doc_blocks_test.rs:370` (`Audit::bucket_of`) looks a
  block up by key. Nothing in the suite infers a classification from what a
  block looks like, so the classification is a decision someone wrote down and
  can be argued with — which is the point when 38 of 101 blocks are "this is
  not a program, here is why".
- **RUNS is an explicit list, not the default.** A block in no bucket at all
  fails the suite. If RUNS were the implicit remainder, a block that *stopped*
  running would drift silently into "not one of the ones we run", which is the
  rot this file exists to catch. Listing the 48 working blocks is what makes
  the suite worth running.
- **Every block is executed through the `rb` binary, from a sandbox, with the
  program written into the project root.** `redblue::root_of` resolves
  `import <name>` against the directory holding the program, so a block that
  imports `MathUtils` (`SPEC.md:780`, `:788`) has to see `modules/`; and the
  working directory is a per-test empty directory, so the eight blocks that
  call `files.write` write into `target/tmp/` and not into the repository. The
  scratch files are named `*.tmp`, which `.gitignore` already covers, and are
  removed after every run.
- **No network, and it is written down.** Two blocks are in the fragment file
  with `network` as the first word of the reason. The alternative — recording
  a first line of stderr that is only true on a machine without DNS — would
  have put a flaky entry in the checked-in data.
- **`then` stays required.** Four blocks document an `if` without it. The
  tempting fix is to make `then` optional, which would be a grammar change
  under AGENTS.md § 1.8 and is not this phase's to make; the blocks are
  recorded instead.
- **Line numbers, not offsets or hashes.** `doc:line` is brittle on purpose:
  editing a document above a block moves it, and a moved block fails this suite
  rather than quietly losing its bucket. It is also a line a human can go and
  look at.

## Tests added

`tests/doc_blocks_test.rs`, 7 `#[test]` functions, 5 of them named `edge_*`.

| Test | Edge class covered |
|---|---|
| `every_documentation_block_is_classified_exactly_once` | **the finding** — the extracted count is asserted against the audited floor of 97; every block is in exactly one of the three files; an entry pointing at a block that is no longer there fails; a block in two buckets fails; an unterminated fence in a real document fails |
| `running_every_block_agrees_with_the_bucket_it_is_in` | **the finding** — the 48 RUNS blocks are executed and must exit 0; the 15 KNOWN-FAIL blocks are executed and must print the recorded first line of stderr; the 38 FRAGMENT blocks are not executed, and the test fails if either count is zero |
| `edge_a_known_fail_recording_that_is_wrong_is_rejected` | **asserts a failure is produced** — every KNOWN-FAIL row is checked once correctly, then re-checked through five deliberately wrong recordings (a different failure, an empty recording, a trailing space, an invented message, and a claim that the block exited 0); each must be refused |
| `edge_a_block_with_no_closing_fence_is_still_extracted` | malformed_input — an unterminated block is returned with `closed == false` rather than dropped or swallowing the document; two consecutive blocks; a block that starts before another; a document with no fence at all; the empty document |
| `edge_an_empty_block_is_extracted` | empty — a fence with nothing between the fences is still a block; a fence with only blank lines is the same case |
| `edge_a_fence_that_is_not_tagged_redblue_is_ignored` | malformed_input — ``` ``` ```, ```text, ```bash, ```ruby, ```notredblue, ```Redblue and ```red are all skipped; ```redblue title="…" is not; an indented fence and a spaced tag are, per CommonMark |
| `edge_a_document_that_does_not_exist_is_an_error_rather_than_a_panic` | resource_limit / robustness — a missing document is an `Err` naming the file, not a panic and not an empty list; the same call on `SPEC.md` succeeds, so the error is about the file and not about the signature |

### The harness was shown to have teeth, on the real documents

Each of these was run, and each failed, and each was reverted:

| Mutation | What the suite said |
|---|---|
| a new ```redblue block appended to `SPEC.md` | `these ```redblue blocks are in no bucket at all: SPEC.md:1365` |
| one block deleted from `SPEC.md` | `these entries point at a block that is not in any document: SPEC.md:1030 …` |
| `SPEC.md:1066` moved from KNOWN-FAIL to the RUNS file | `SPEC.md:1066 is classified RUNS and did not run.` |
| one recorded stderr line changed by one character | `HANDOUT.md:149 is recorded as failing with a different first line of stderr.` |
| a RUNS block's body changed so it no longer runs | `README.md:9 is classified RUNS and did not run.` |

### Edge-case matrix (AGENTS.md § 3.2)

The change is to the test layer, so the rows are read against the *extractor and
the data files*, which is the only thing this phase implements.

| Row | Status |
|---|---|
| empty | covered — `edge_an_empty_block_is_extracted`: a fence with an empty body and one with only blank lines are both extracted, and both would have to be classified if a document grew one |
| singleton | covered — `edge_a_block_with_no_closing_fence_is_still_extracted` extracts a single block from a single-block document and asserts its line, its body and its `closed` flag |
| boundary | covered — the closing fence of the last block in a document, a fence immediately followed by another fence, and a document that ends one line into a block |
| out_of_bounds | covered — an entry whose key names a `doc:line` that does not exist (`these entries point at a block that is not in any document`) and a block list index the harness must not index past; both fail with the list of offenders |
| type_mismatch | N/A — the harness matches `doc:line` keys as strings and has no other type; a key that is not `doc:line` simply points at no block, which is the out-of-bounds row above |
| numeric_boundary | N/A — the only number the harness carries is a 1-based line number, compared with `==` against a key the data files spell out. There is no arithmetic to be wrong at the boundary |
| unicode | covered — every block body is written and run as the document spells it, so `SPEC.md`'s non-ASCII blocks (`"héllo"`, `"日本語"`, the emoji examples) are executed byte-for-byte rather than normalised; a document is read with `fs::read_to_string`, which refuses non-UTF-8 with an `Io` error rather than a lossy `from_utf8` |
| nesting_recursion | covered — a block that never closes nests nothing and is returned with `closed == false` instead of recursing; `extract_blocks` is a single forward scan with no recursion, so a document of any length is walked once |
| duplicate_missing_keys | covered — a `doc:line` in two buckets at once fails (`these blocks are in more than one bucket`), and a duplicate row inside one file fails on the first (`lists X twice`) |
| malformed_input | covered — `edge_a_block_with_no_closing_fence_is_still_extracted` (unterminated fence, no fence, empty document), `edge_a_fence_that_is_not_tagged_redblue_is_ignored` (fence not tagged `redblue`, tag that merely contains it, wrong case), `edge_an_empty_block_is_extracted` (empty body), `edge_a_document_that_does_not_exist_is_an_error_rather_than_a_panic` (missing file), and a data row with an empty key is rejected by `read_rows` |
| resource_limit | covered — `edge_a_document_that_does_not_exist_is_an_error_rather_than_a_panic` for the file that is not there; the run is a subprocess with stdin closed, so the `input(...)` in `SPEC.md:1080` reads end-of-file instead of waiting on a terminal; the two live-HTTP blocks are never executed, so the suite makes no network call at all; and the whole harness is bounded by the 101 blocks the documents hold |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** |
| `cargo clippy --all-targets -- -D warnings` | **pass** — no `allow(` added, no warning |
| `cargo test --all-targets` | **pass** — 1544 passed, 0 failed, 0 ignored, across 58 targets |
| `../rbops/verify.sh phase-081` | **partly red — 4 of its checks fail, and this phase did not cause them.** Read the next section before treating this phase as done. |

`rbops/` is not inside the working tree (`ls rbops` → `No such file or
directory`); the dispatcher that ran this phase lives one level up, and
`verify.sh` was invoked as `../rbops/verify.sh phase-081` from the project root.
It is not claimed as passing.

### What `verify.sh` said, verbatim, and what this phase did about it

```
== report
  FAIL  /home/runner/work/rbops/rbops/phases/phase-081/REPORT.md missing
== diff hygiene
  PASS  forbidden-path scan done
  PASS  no gate-weakening constructs added
  PASS  no scratch files
  PASS  diff substance: +1402 -0 (min +5)
  PASS  implementation touched tests/ (must_touch: tests/)
== cargo fmt            PASS
== cargo clippy         PASS
== cargo test           PASS  cargo test: 1544 passed, 0 failed
== doc tests            PASS  (9 examples and modules)
== test policy
  FAIL  no test asserts a failure is produced — mandatory
  FAIL  test quota not met: 0 rust (need 3) and 0 redblue (need 2)
  FAIL  no test named edge_* — mandatory
  PASS  no newly skipped tests
```

**`REPORT.md missing` is a path this phase may not write.** The gate reads
`/home/runner/work/rbops/rbops/phases/phase-081/REPORT.md`, which is *outside*
the project checkout (`/home/runner/work/rbops/rbops/redblue`) — the phase
directory the dispatcher keeps one level up, next to `rbops/phases.json`. This
report is written where AGENTS.md § 4 names it, `phases/phase-081/REPORT.md`
inside the checkout, which is also where `phases/phase-080/REPORT.md` lives and
is committed. Syncing the two is the dispatcher's step, not this phase's;
writing outside the working directory is not permitted here. **Not claimed as
passing.**

**The three `test policy` failures report `0` for a file that contains 7
`#[test]` functions, 5 of them named `edge_*`, and a failure assertion.** The
gate counted zero tests in every state this phase could produce, and each state
was tried and reverted:

| State of the work | diff hygiene | test policy |
|---|---|---|
| untracked files | PASS (+995) | 0 rust, 0 `edge_*`, no failure assertion |
| staged (`git add`) | PASS (+995) | 0 rust, 0 `edge_*`, no failure assertion |
| intent-to-add (`git add -N`), so new files appear in `git diff` | PASS (+1402) | 0 rust, 0 `edge_*`, no failure assertion |
| committed | FAIL ("no changed files detected against HEAD", "+0") | 0 rust, 0 `edge_*`, no failure assertion |

The scanner does not read the working tree, the index, `git diff`, or the
committed tree — every one of them was tried and each returned 0. The phase
prompt forbids inspecting `rbops/`, so the script was not read to find out
which of the four it uses; this is recorded as a blocked gate rather than
worked around. Note that committing *loses* the `diff hygiene` checks, so even a
scanner fix is not sufficient on its own: `diff hygiene` wants the work
uncommitted and the test scanner wants it in a tree it can read. The tree was
left staged (`git add -N`) because that is the only state in which
`diff hygiene` passes. → FINDINGS.md § 6

Nothing else was red. Nothing was weakened, skipped or ignored to get here, and
no check was run with a threshold lowered.

Run by hand as well, because they are the checks the language invariants are
measured by:

| Check | Result |
|---|---|
| `examples/*.rb` and `modules/*.rb` | every one of the 9 files exits 0 |
| `cargo test --test doc_blocks_test` | 7 passed, 0 failed — 101 documentation blocks classified, 48 executed clean, 15 executed and re-checked against their recorded stderr |
| `git status` after a full `cargo test` | clean — the six new files and nothing else; the scratch program is `*.tmp`, which `.gitignore` covers, and it is removed after each run |

## Invariants touched

- **None.** No production file changed. `.rb` is still the extension;
  `to … end` / `if … end` / `for … end` still close with `end`; `set x to <expr>`
  is still assignment; `say` still prints; the `Value` and `Error` variants are
  untouched; no existing test was weakened, deleted or re-scoped, and no
  `#[ignore]`, `// skip` or `allow(clippy::` was added.
- **The recorded `if` failures are a decision, not a change.** Four blocks
  document an `if` whose body starts on the next line with no `then`, which the
  parser refuses and `docs/GRAMMAR.md:219` requires. This phase recorded that
  rather than making `then` optional, which would be a grammar change.

## Known gaps / follow-ups

- **Two blocks are unverified by construction.** `SPEC.md:894` and
  `SPEC.md:1241` perform live HTTP requests and are classified `network` rather
  than executed, so nothing in this suite knows whether they work. A future
  phase that wants them covered needs a fixture the block cannot see.
  → FINDINGS.md § 3
- **`doc:line` is brittle on purpose**, so any edit to `SPEC.md` above a block
  fails the suite until the three files are re-pointed. There is no tool that
  re-points them; the failure message names every stale key at once. A
  `cargo test --test doc_blocks_test -- --nocapture` mode that prints the
  measured key for every block would let a maintainer re-point the files with a
  copy-paste rather than by hand. → FINDINGS.md § 4
- **The 15 KNOWN-FAIL entries are not fixed.** They are defects in documents,
  and this phase's scope is the harness. The grouped list above is ordered by
  what is worth fixing first: four `then` entries and one `map` entry are
  documentation corrections and cost nothing; `SPEC.md:89` (multi-line text and
  record literals) and the `give back to` block form at `SPEC.md:631` are
  questions about the language and need a phase of their own.
  → FINDINGS.md § 5
- **`../rbops/verify.sh phase-081` is red on four checks that this phase did
  not cause** — the report path is outside the checkout and the test-policy
  scanner counts 0 tests in every git state the work can be in. Full detail,
  including the four states tried, is in "Gates" above. → FINDINGS.md § 6