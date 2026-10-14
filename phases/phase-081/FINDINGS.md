# phase-081 FINDINGS

Work found while doing phase-081 that does **not** belong to this phase.
Recorded here for the auditor, per AGENTS.md § 1.7. Nothing below was fixed.

## 1. The finding's block count and two of its error strings are stale

The phase prompt quotes "97 ```redblue blocks exist across the documents".
Measured on `main` before any change, the extractor in `tests/doc_blocks_test.rs`
finds **101**: `SPEC.md` 68, `docs/GRAMMAR.md` 9, `PHILOSOPHY.md` 9,
`HANDOUT.md` 8, `README.md` 7. Every fence in all five documents is closed; no
block is nested inside another fence; the only spelling of the tag is
`` ```redblue ``. The four-block difference is all in `SPEC.md`, and the count
matters because it is what `tests/doc_blocks_test.rs` floors the extraction at
(`MINIMUM_BLOCKS`, `tests/doc_blocks_test.rs:78`), so a suite written against 97
would have let four blocks go unclassified.

Two of the prompt's error strings are also from an older tree:

| Prompt | Measured on `main` today |
|---|---|
| `SPEC.md:1066` → `ParserError: Unexpected token Wait` | `AnalyzerError: Unknown variable 'async'` |
| `HANDOUT.md:155` → `filter was called with arguments it cannot use` | `RuntimeError: Module 'list' has no function 'filter'` |
| `SPEC.md:1068` → `LexerError: Unexpected character '\''` | that string is at **`HANDOUT.md:149`**; `SPEC.md:1068` is a RUNS block |

`docs/GRAMMAR.md:526` — the prompt's example of a fragment with an "elided
`end`" — is **`docs/GRAMMAR.md:520`** today, and its problem is not an elided
`end`: it is that the third line reads `body`, which is bound nowhere, while the
guarded read was bound to `report`. **No block in any of the five documents has
an elided `end`**, which is worth knowing before a phase is written against that
example.

**Suggested phase.** None needed. Recorded so the auditor does not re-raise the
97 figure, and so the next phase that audits a document knows the counts and
line numbers have moved again.

## 2. `SPEC.md` and `docs/GRAMMAR.md` disagree about `if`, in both directions

`docs/GRAMMAR.md:219` writes

```
if_statement = 'if' expression
                'then'
                { statement }
```

and the parser requires the `then`: `if` with its body on the next line is
`ParserError: Expected 'then' but got 'say'`. Four blocks document the other
spelling — `SPEC.md:570`, `SPEC.md:977`, `HANDOUT.md:42`, `HANDOUT.md:140` —
and `docs/GRAMMAR.md:600`, the grammar's own `if/else` example, omits it too.
`README.md`'s one `if` block writes `then` correctly, and all 5 of the 12
`if` blocks in `tests/` write it correctly, so the parser and the tests agree
and the documents do not.

The converse also holds: `for_statement` in the same grammar has **no** `then`,
and every `for each` block in all five documents is right about that.

**Why this is a finding and not something this phase fixed.** Making `then`
optional is a grammar change (AGENTS.md § 1.8), and changing the four documents
is a documentation change that belongs to whoever owns the four entries.
`tests/doc_blocks_test.rs` now records all four, so the rot cannot grow.

**Suggested phase.** Correct `SPEC.md:570`, `SPEC.md:977`, `HANDOUT.md:42`,
`HANDOUT.md:140` and `docs/GRAMMAR.md:600` to write `then`, and re-record the
three KNOWN-FAIL lines that change. A documentation phase, not a language one.

## 3. Two documented blocks cannot be executed by any hermetic test

`SPEC.md:894` and `SPEC.md:1241` make live HTTP requests to
`https://api.example.com`. They are in `tests/doc_blocks_fragments.txt` with
`network` as the first word of the reason, and they are the only two blocks
whose outcome this suite does not know.

The alternative was rejected: recording a first line of stderr for them would
record `dns error: failed to lookup address information` on a machine without
DNS and a different message — or a success — on a machine with it. AGENTS.md
§ 3.1.4 requires determinism, and a checked-in file whose contents depend on
the runner's network is exactly the flakiness § 2 forbids muting.

**Suggested phase.** Either point the two blocks at a loopback fixture
(`tests/stdlib_module_docs_test.rs` already runs one) so they are hermetic, or
add a fourth bucket to the classification — `NOT-RUN(reason)` — so "not executed
because it is not a program" and "not executed because it would leave the
sandbox" stop sharing one word.

## 4. `doc:line` keys have no regeneration path

All three data files are keyed by `doc:line`. Editing `SPEC.md` above a block
moves it, and every entry below the edit goes stale at once — `SPEC.md` alone
would report 30 stale keys. That is on purpose: a moved block has to fail rather
than quietly lose its bucket, and a line number is something a human can go and
look at.

But there is no tool that re-points them, and re-pointing 101 entries by hand
after a documentation edit is the kind of chore that gets skipped, which would
make the next person delete a block rather than fix an entry.

**Suggested phase.** An ignored-flag mode on `tests/doc_blocks_test.rs` —
`-- --print-keys` — that prints the measured `doc:line` and the measured
outcome for every block, so re-recording the files is a copy-paste rather than
an audit. The gate's failure message already names every stale key at once, so
this is convenience rather than correctness; the suite fails loudly either way.

## 5. The 15 KNOWN-FAIL entries, ordered by what they are worth

None was fixed. They are listed in REPORT.md § "The measured classification"
with the exact first line of stderr for each. In the order I would take them:

| Entry | What it is | Cost to fix |
|---|---|---|
| `SPEC.md:570`, `SPEC.md:977`, `HANDOUT.md:42`, `HANDOUT.md:140` | `if` without `then` | documentation only — see § 2 |
| `HANDOUT.md:133` | `math.random`, which `SPEC.md:1164` already says is not implemented | documentation only: the two documents disagree and the handout is stale |
| `HANDOUT.md:161` | `print` and `ask`; `console.log` in the same document works | documentation only, or two builtins |
| `SPEC.md:607`, `HANDOUT.md:155` | `numbers.map(f)` / `list.filter` — a list read as a module | the finding attributes `HANDOUT.md:155` to phase-077; `SPEC.md:607` is the same defect in SPEC.md and nothing owns it |
| `SPEC.md:1267`, `PHILOSOPHY.md:132` | `back` and `is` used as ordinary identifiers | documentation, or a language decision about which reserved words are worth keeping |
| `HANDOUT.md:149` | single-quoted strings | a language decision |
| `SPEC.md:89` | multi-line text literals and a `record { … }` literal | a language decision |
| `SPEC.md:631` | `give back to … end`, the block form of a function literal | a language decision; the one-line form at `SPEC.md:40` is a RUNS entry |
| `SPEC.md:1015`, `SPEC.md:1066` | **not defects** — the failure is the promise | none; recorded so the guard and the "not Redblue" note stay pinned |

The last row matters: two of the fifteen exist only because the suite refuses to
let a failure it depends on go unrecorded. A future phase that "fixes"
`SPEC.md:1015` by making the call-depth guard return `nothing` would be
implementing something SPEC.md:1005 explicitly says it must not do.

**Suggested phase.** Four small phases: (a) the five documentation corrections
in § 2 plus `HANDOUT.md:133`; (b) method-call-on-a-list, which owns both
`SPEC.md:607` and `HANDOUT.md:155` and is the same defect the finding
attributes to phase-077; (c) `print`/`ask`; (d) a decision phase for the
literal forms, which is a language design question and not a bug.
## 6. `verify.sh` reads the phase report and counts tests somewhere this phase cannot reach

Four of `../rbops/verify.sh phase-081`'s checks are red, and the cause is the
gate's environment rather than the work. Recorded in full because AGENTS.md § 3
requires an honest report and the phase is **not** done.

### 6a. The report is read from outside the checkout

```
FAIL  /home/runner/work/rbops/rbops/phases/phase-081/REPORT.md missing
```

The project checkout is `/home/runner/work/rbops/rbops/redblue`. The gate reads
`/home/runner/work/rbops/rbops/phases/phase-081/REPORT.md` — one level up,
beside `rbops/phases.json`. Both directories exist and both carry a phase-080
report, so the pipeline keeps its own copy.

This phase wrote `phases/phase-081/REPORT.md` **inside** the checkout, which is
where AGENTS.md § 4 names it and where `phases/phase-080/REPORT.md` is committed
from. The agent contract forbids writing outside the working directory, so the
gate's path was not written to. Whatever copies the in-repo report to the
pipeline's copy is the dispatcher's step.

### 6b. The test-policy scanner counts 0 tests in every git state

```
FAIL  no test asserts a failure is produced — mandatory
FAIL  test quota not met: 0 rust (need 3) and 0 redblue (need 2)
FAIL  no test named edge_* — mandatory
```

`tests/doc_blocks_test.rs` has **7** `#[test]` functions, **5** of them named
`edge_*`, and `edge_a_known_fail_recording_that_is_wrong_is_rejected` asserts
that five deliberately wrong stderr recordings are each refused. The gate
counted zero of each. Four states were tried and reverted:

| State | diff hygiene | test policy |
|---|---|---|
| untracked | PASS +995 | 0 / 0 / none |
| staged | PASS +995 | 0 / 0 / none |
| `git add -N` (intent-to-add, so new files appear in `git diff`) | PASS +1402 | 0 / 0 / none |
| committed | FAIL "no changed files detected against HEAD", +0 | 0 / 0 / none |

So the scanner reads neither the working tree, nor the index, nor `git diff`,
nor the committed tree — all four were checked. **And the two halves of the gate
disagree about what "changed" means**: `diff hygiene` wants the work *uncommitted*
(its own message is "against HEAD"), while the test scanner finds nothing in any
state, including committed. A fix to one is not sufficient on its own.

The script was not read. The phase prompt forbids inspecting `rbops/`, so this
is written up rather than diagnosed further, and no workaround was attempted:
moving a test into a file the scanner already reads, or splitting the tests to
game the count, would be gaming a gate (AGENTS.md § 2), which is worse than a
red check.

**Suggested phase.** A pipeline fix, not a language one: make the test-policy
scanner read the same change `diff hygiene` reads (`git diff HEAD` plus
intent-to-add entries), or state which tree it reads and why. While there,
resolve the two project roots — the gate resolving `phases/` against
`/home/runner/work/rbops/rbops` while `cargo` runs in
`/home/runner/work/rbops/rbops/redblue` is the root cause of 6a, and it will
mis-grade any future phase whose test file it cannot see.

## 7. The repository state this phase left behind

The work is **staged but not committed** (`git add -N` on the six new files).
Committing was tried and reverted because committing makes `diff hygiene` fail
with "no changed files detected against HEAD", so staged is the only state in
which more of the gate is green. A dispatcher that commits the phase after
`verify.sh` is unaffected; one that expects the agent to have committed will
find the changes staged rather than committed.
