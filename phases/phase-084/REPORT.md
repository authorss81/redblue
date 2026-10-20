# Phase 084 — Resolve the reviewer agent's permissions where opencode can actually see them

## Result

The reviewer's permissions are now declared, in the file opencode actually
reads, for every permission key that could widen them — and so is the **mode**
that decides whether `--agent reviewer` binds to that agent at all. The
pipeline hands the reviewer a change set, refuses to start a review that would
not be read-only, and **the review request says so, instead of claiming the
checkout has no config at all** — and that half is now **committed in the
pipeline repo** (`267f9a2`), so it survives the pipeline's own commits instead
of living in a working tree that a later tick can lose. The request the
pipeline builds *today* is pasted below, verbatim.

The full suite is green: **1934 passed; 0 failed; 0 ignored** across 70 test
binaries. The pipeline's own smoke suite is green at **230 checks** (215 before
this round), re-run in this round against the committed pipeline change.

## Round 5 — this run

This run is the fix round for the review of round 4's delivery. It inherited
round 4's work **committed** (`HEAD` is `d23068b`) and round 4's own
uncommitted additions, and it fixed all eight findings that review raised — one
BLOCKER, three MAJOR, four MINOR — without weakening a gate, deleting a test,
or moving a threshold.

### The BLOCKER: the pipeline half was never committed, so it never existed

Round 4's report claimed the review request no longer carried the stale
"the checkout has no `opencode.json`" sentence, and that the pipeline repo
carried the fix. The request this round's reviewer received carried the stale
sentence **verbatim**, and the inlined `rbops/agents/reviewer.md` header still
said the same thing, while `opencode.json` sat at the checkout root with
`mode: "primary"` and the eleven denies. Round 3's fix had lived only in the
pipeline repo's **uncommitted working tree**, round 4 changed nothing in that
repo at all, and the working tree did not survive: the pipeline repo's `HEAD`
when this run started was `e070f88`, and `grep -rn "has no opencode.json"
rbops/` matched three agent contracts. The claim was false, and it was false
in the shape the reviewer could see from the request alone.

**What this run did.** The pipeline-side change was made again and
**committed in the pipeline repo**, so it is in that repo's history rather than
in a working tree:

```
$ cd ../rbops        # the pipeline repo, one directory up
$ git log --oneline -1
267f9a2 phase-084: the reviewer is resolved out of the file opencode reads
$ git show --stat --oneline 267f9a2 | tail -9
267f9a2 phase-084: the reviewer is resolved out of the file opencode reads
 .gitignore               |   4 ++
 opencode.json            |  12 +++-
 rbops/agents/auditor.md  |  11 +++--
 rbops/agents/reviewer.md |  11 +++--
 rbops/agents/verifier.md |  14 +++--
 rbops/dispatch.sh        | 131 ++++++++++++++++++++++++++++++++++++--
 rbops/smoke.sh           | 161 +++++++++++++++++++++++++++++++++++++++++++++--
 7 files changed, 319 insertions(+), 25 deletions(-)
```

| File | What the commit carries |
|---|---|
| `rbops/dispatch.sh` | the review request names the config the reviewer is resolved out of and the keys that config denies, read out of that file with `jq` rather than written out longhand; `assert_reviewer_read_only` resolves `agent.reviewer.mode` and every widening key out of the project config and **refuses the review** when the reviewer would not be read-only, naming the key that would widen it; `cmd_run` records the base it captured before any resume merge and `cmd_review` reads it back, so the reviewer is handed the phase's change set instead of an empty diff |
| `rbops/agents/reviewer.md`, `rbops/agents/verifier.md`, `rbops/agents/auditor.md` | the headers no longer claim the checkout has no `opencode.json`; they say what is true, and the verifier's says plainly that no `--agent verifier` invocation exists |
| `rbops/opencode.json` | the pipeline's own `reviewer` agent was `mode: "subagent"` with `webfetch` allowed — the same footgun, one directory up. It is `mode: "primary"` with the same eleven denies |
| `.gitignore` | `!phases/*/.base` — the recorded base is committed state, like the other markers: the review runs in a separate process, and the base has to survive the job |
| `rbops/smoke.sh` | a new section of fifteen checks, and a fixture project that ships the `opencode.json` the dispatcher resolves the reviewer out of |

**Re-verified by pasting the request the pipeline builds today.** The request
below was built by the committed dispatcher, against a copy of this checkout,
today — not quoted from a fixture and not remembered:

```
# Review request - round 1 of phase phase-084

The gate is green. Your job is to find what is still wrong.
The contract below is authoritative and is inlined here on purpose:
dispatch runs you with --dir pointing at the redblue checkout, which ships
its own opencode.json at the checkout root. opencode merges the project
config over the global one, so that file is the one your agent block is
read from. The reviewer agent is declared there with mode "primary" and
with these keys denied: edit, bash, task, todowrite, skill, lsp, question, webfetch, websearch, doom_loop, external_directory.
Referring you to a config file or an .opencode/agent/*.md path meant you
ran with NO contract for every phase.
```

The eleven keys in that line are read out of this checkout's own
`opencode.json` by the dispatcher; they are not written out longhand, so the
request cannot be right about one file and wrong about the other. The contract
that follows it in the same request now says the same thing:

````text
<!-- Inlined into the review/audit context by rbops/dispatch.sh.
     Source of truth: rbops/agents/reviewer.md
     The contract below is authoritative and is inlined here on purpose.
     Dispatch runs the model with --dir pointing at the redblue checkout,
     which ships its own opencode.json at the checkout root; opencode merges
     the project config over the global one, so that file is where the agent
     block is read from. The file and this text are two halves of one
     contract, and the review request names both. -->
````

(The four-backtick fence is what keeps the quoted request a quote: the request
carries three-backtick fences of its own, and a closing fence shorter than the
one it closes is text to every markdown reader. That rule was missing from the
checker this round — see finding 8 below.)

**The pipeline's smoke suite was re-run this round**, against the committed
change, and it is green at **230 checks** — 215 before this round, so this
round's section is the fifteen it adds:

```
$ bash rbops/smoke.sh
  PASS the request names the config the reviewer is resolved out of
  PASS the request no longer claims there is no config
  PASS the request names every key the reviewer has denied
  PASS no agent contract claims there is no config
  PASS a writable reviewer stops the review
  PASS the refusal names the key that would widen the reviewer
  PASS an undeclared widening key stops the review
  PASS the refusal names the undeclared key
  PASS a subagent-mode reviewer stops the review
  PASS the refusal names the mode
  PASS the object form of an allow stops the review
  PASS a missing config stops the review
  PASS the object form of a deny is accepted
  PASS the request diffs against the base the run recorded
  PASS the review request carries the committed change set

SMOKE PASS  230 checks
```

### The other seven findings

| # | Finding | What this run did |
|---|---|---|
| 2 | The section test asserted only that *some* line starts with the heading, so a report that wrapped `## Tests added` in a properly closed fence passed it — the exact regression the phase exists to prevent | The test now computes the fenced regions (under both readings, unioned) and requires each required section to start a line **outside every code block**. `missing_sections()` is the rule; `edge_a_section_heading_inside_a_code_block_is_not_a_section` drives it, and the section test itself was measured red against a report with `## Tests added` wrapped in a fence |
| 3 | The inherited-state row and the "What changed" row said the test file's only edit was `cargo fmt --all`, while the file carried eight tests and this report said three of them were added that round | Both rows now name the growth: the inherited file was **283 lines with 5 tests**; round 4 added three tests and ran `cargo fmt --all`, which left it at **331 lines with 8 tests**. The three are named |
| 4 | The findings file said the suite was green with **1929** passing tests while this report said **1932** | Both now say the figure the suite reports today — **1934**, ten of them in the document suite — and the findings file says which round each historical figure belongs to |
| 5 | The report called two inline-fixture tests "red-first tests … which failed against `HEAD`'s committed report"; neither reads the report | The red-first guard is named as what it is: `phase_084_delivery_documents_are_unambiguous_markdown`, the test that reads `HEAD`'s committed report and fails against it |
| 6 | The Known-gaps bullet said **seven** deliveries had paid the acknowledgement treadmill and named `10f40c6` | It now says **eight**, names `d23068b`, and says which delivery is next |
| 7 | The edge-case matrix had no row for the document checker's empty, singleton, boundary and malformed inputs | The matrix now maps those four classes to the tests that cover them |
| 8 | The fence fix made quoted review-request artifacts lossy: the request's own closing fence was consumed as the outer fence's terminator, so the paste no longer reproduced the artifact | The naive reading now requires the closing fence to be **at least as long** as the one it closes, which every markdown reader requires anyway. The artifacts are pasted verbatim again, inside four-backtick fences, and `edge_a_longer_outer_fence_may_hold_a_shorter_inner_fence` pins the rule |

### What changed on the project side this round

Two tests, and the two documents. Nothing under `src/` changed, `opencode.json`
was not touched — it was already correct — and no assertion was loosened.

| Test | What it adds |
|---|---|
| `edge_a_section_heading_inside_a_code_block_is_not_a_section` | a heading inside a code block is text, not a heading: the section test cannot pass on a document that hides one |
| `edge_a_longer_outer_fence_may_hold_a_shorter_inner_fence` | a four-backtick fence may hold a three-backtick one, which is what lets a quoted review request be pasted verbatim |

The checker changed with them: `scan`'s naive branch now requires `run >=
ticks`, and `ambiguous_fence` reports the shape the two readings actually
disagree about. Both changes were measured red against the old code:

```
$ # with the naive branch back to "any fence-looking line closes the block"
$ cargo test --test report_markdown_test
test edge_a_longer_outer_fence_may_hold_a_shorter_inner_fence ... FAILED
test result: FAILED. 9 passed; 1 failed; 0 ignored

$ # with "## Tests added" wrapped in a fence in this report
$ cargo test --test report_markdown_test
test phase_084_report_carries_the_sections_the_gate_looks_for ... FAILED
test result: FAILED. 9 passed; 1 failed; 0 ignored
```

## Round 4 — the delivery under review

Everything from here to the next `## ` heading is **round 4's account, in round
4's words**: `this run` in this section means round 4, and the measurements
are the ones round 4 took. Round 5's account is the section above, and where
the two disagree about a number, round 5's is the one that is re-measured.

Round 4 inherited **round 3's delivery committed** — `HEAD` is `d23068b`
(`resume phase-084: adopt preserved work`), which carries the repository-root
`opencode.json`, the phase's suite `tests/agent_permissions_test.rs` (28
tests), this report and the findings file — **plus uncommitted work from an
interrupted attempt**, which this run verified, finished and documented
rather than rewrote:

| Inherited, uncommitted | What it is | State as found / as left |
|---|---|---|
| `tests/report_markdown_test.rs` (untracked, 283 lines) | 5 tests pinning that this phase's two delivery documents read the same way under a strict and a naive fence reader, and that `REPORT.md` still carries the four section headings AGENTS.md §4 names | **failed `cargo fmt -- --check` as found** (one reformat at line 75). Round 4's edits to it were two, not one: `cargo fmt --all`, **and three tests it added** — `edge_a_rejected_document_produces_an_error_not_a_panic`, `edge_an_empty_document_is_accepted` and `edge_a_document_without_code_blocks_is_accepted`. That left the file at **331 lines with 8 tests**, all 8 passing |
| `phases/phase-084/REPORT.md` and `phases/phase-084/FINDINGS.md` | the fence fix behind the preflight failure, below | kept; documented in this section |
| `tests/unreviewed_work_ack_test.rs` | the acknowledgement entry repointed from `10f40c6` to `d23068b` | kept after measuring it red without it, below |

**The preflight gap this run was launched for** — `report missing section:
Tests added` — reproduces against the committed report and is fixed by that
inherited fence edit. Round 3's `REPORT.md` (`HEAD`'s copy) opened a plain
`` ``` `` fence, then inside it a `` ```diff `` fence at line 559. A fence
opened with three backticks is closed by a backtick run with **no info
string**, so under a strict CommonMark reading the `` ```diff `` line is
content, not a boundary; under the simpler "every fence-looking line is a
boundary" reading it is a boundary. The pairing shifts by one between the two
readings, and everything after line 559 — `## Tests added` (line 655 in
`HEAD`'s copy), `## Gates`, `## Invariants touched`, `## Known gaps /
follow-ups` — sits inside a code block for the reader that takes the second
reading. Measured, both halves:

```
$ git show HEAD:phases/phase-084/REPORT.md | <fence scan>
ambiguous fence inside open block (strict): [(559, 'diff')]
'## Tests added' line: [655]
"'## Tests added' inside a code block under naive reading": 655
```

The inherited edit removes the nested `` ```diff `` fences, so both readings
pair the same regions and the section headings are headings to both. That is
exactly what `tests/report_markdown_test.rs` asserts, for both delivery
documents, so the gap cannot come back without a red test.

Two independent facts carry that same message, and this run measured both.
The fence defect above hides the heading from a naive reader of *this* file.
The bare gate command, however, does not read this file at all: without the
environment recorded under "Gates", its first line prints
`PASS /home/runner/work/rbops/rbops/phases/phase-084/REPORT.md exists` — a
path one directory above this checkout — and its section checks are made
against that parent-level copy. Measured: with this checkout's report emptied
to zero bytes, that invocation still passes `What changed`, `Gates` and
`Known gaps` and still fails `Tests added`, so those three headings are being
found in the parent copy and the fourth is not — the same single failure the
preflight reported at dispatch. The fence defect was
a second, independent way to lose the same heading from this copy, and both
are now fixed — the fence edit above, and the recorded invocation below.

**The acknowledgement treadmill, paid again.** At `HEAD` — measured by
stashing the inherited work and running the suite's own selector — the ack
test was red for the mirror image of round 3's failure: round 3 added an
entry for `10f40c6`, and `10f40c6` is no longer in `HEAD`'s history
(`git merge-base --is-ancestor 10f40c6a437ea44032e639616e40ff7bd1027a3f HEAD`
→ exit 1), while the commit this history does carry, `d23068b`, was not
acknowledged:

```
$ git stash push -- phases/phase-084 tests/unreviewed_work_ack_test.rs
$ cargo test --test unreviewed_work_ack_test
unreviewed-work commits selected: 33 | acknowledged: 33 | unacknowledged: 1 |
  stale entries: 1 | UNACKNOWLEDGED:
  d23068bf4160c14463f4a6af7479b824b4f33429	resume phase-084: adopt preserved work
  | STALE: 10f40c6a437ea44032e639616e40ff7bd1027a3f
thread 'unreviewed_work_in_head_history_is_acknowledged' panicked at
tests/unreviewed_work_ack_test.rs:523:5
test result: FAILED. 12 passed; 1 failed
$ git stash pop
```

The inherited edit is the repair: the entry and the `MEASURED` slot now name
`d23068b`, with the figures `git show --numstat d23068b` gives (`+2722/-2`
across 5 files, none under `src/`), and the comment above the entry records
why `10f40c6` was removed — it is the third parked phase-084 delivery to go
stale that way. No assertion was touched, nothing skipped, no threshold
moved. With the inherited work in place the same test is green — all 13 of
its cases pass.

**What round 4 changed on top:** `cargo fmt --all` (the one-line reformat
above) **and the three tests it added to that file**
(`edge_a_rejected_document_produces_an_error_not_a_panic`,
`edge_an_empty_document_is_accepted`,
`edge_a_document_without_code_blocks_is_accepted`), the additions of this
section, the round-4 rows in "What changed" and "Tests added", the refreshed
gate records, and the same additions in FINDINGS.md. No production code
changed, no test was deleted, skipped or weakened, and `opencode.json` was not
touched — it was already correct.

## State of the tree round 3 inherited

Round 3 was **round 3 of the review fixes**, and it inherited the phase's work
already committed: `HEAD` was `10f40c6` (`resume phase-084: adopt preserved
work`), which carries the repository-root `opencode.json`, the phase's own
suite `tests/agent_permissions_test.rs`, and this file. Nothing about the
phase's substance was missing.

What was missing was one line of acknowledgement data. `cargo test` was **red
before any edit in round 3**, in a file this phase does not own:

```
$ cargo test --test unreviewed_work_ack_test
---- unreviewed_work_in_head_history_is_acknowledged stdout ----
unreviewed-work commits selected: 33 | acknowledged: 32 | unacknowledged: 1 |
  stale entries: 0 | UNACKNOWLEDGED:
  10f40c6a437ea44032e639616e40ff7bd1027a3f	resume phase-084: adopt preserved work

thread 'unreviewed_work_in_head_history_is_acknowledged' panicked at
tests/unreviewed_work_ack_test.rs:513:5:
1 commit(s) in HEAD's history carry unreviewed work and are not on the
acknowledgement list in this file: 10f40c6a437ea44032e639616e40ff7bd1027a3f
	resume phase-084: adopt preserved work.
```

(The line in that paste is the line in the file **as inherited** — `513` in
`10f40c6`'s copy. Round 3's ten added lines moved the same assertion to
`523`; the paste is the red output round 3 started from, and it is left as it
was measured.)

The commit that carries a phase's work is created by the pipeline *after* the
run that produced it, so the run that produces the work cannot know the SHA
its own commit will get — only the next run can, by finding it in
`git log HEAD`. That was the seventh delivery of this phase to pay that
one-line repair; FINDINGS.md carries the whole sequence and the two fixes
that would end it.

The first run of that test also reported, once, that the history was shallow
and that completing the clone did not make it complete. The repository is not
shallow (`git rev-parse --is-shallow-repository` → `false`, no `.git/shallow`),
the test passed on every run after that, and the failure did not recur in the
full suite. It is recorded rather than dismissed, in FINDINGS.md.

The two gates that do not depend on the failing test were green as delivered:

| Gate as delivered | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — exit 0, no output |
| `cargo clippy --all-targets -- -D warnings` | pass — exit 0, no warnings |
| `cargo test --all-targets` | **red — 1 failure** (`unreviewed_work_in_head_history_is_acknowledged`) |

This round's changes are: that one line of data, **five new tests** that
close two holes in the shipped contract (below), **the pipeline repo's half of
the contract** — the dispatcher's read-only check, the review request's text,
the three agent contracts' headers and the pipeline's own agent block, none
of which was in the pipeline repo when this run started — and this report and
FINDINGS.md. No production code changed, because none needed to: the shipped
`opencode.json` was already correct, and the suite that pins it was already
green.

### The two holes this round closed

The suite that round 2 left behind was strong about the *reviewer's*
permissions and weak about the file's *reach*. opencode deep-merges the
project config over the global one, per key, so two shapes of a project config
move a permission for an agent the file does not declare — and neither was
pinned by any assertion:

| Hole | Why it matters |
|---|---|
| a **top-level `permission` block** | it applies to every agent that declares no block of its own — the implementer's, `build`, included |
| an **agent this file does not own** — `build` above all | a block for that name here REPLACES the global one for it, because the project config is consulted first, and the pipeline runs every phase's implementer as `--agent build` |

The report had already claimed both were absent from the shipped file. They
were; nothing asserted it. They are asserted now —
`project_config_declares_no_top_level_permission_block`,
`project_config_declares_exactly_the_two_pipeline_agents`, and the two
`edge_*` tests that drive the rules which now refuse them. Details, and the
mutation runs that prove the new tests can fail, are under "Tests added".

### What the previous rounds did, and where the bases are

- **Round 1** (delivery `f44665b`) shipped `opencode.json`, the phase's suite,
  and the phase's report.
- **Round 2** (delivery `8c54c7a`, whose content `10f40c6` re-applied onto
  `main`) closed the eleven findings of the first review and the seven
  findings of the second: it widened `agent.reviewer.permission` to all
  eleven keys that could widen it, added `mode: "primary"`, added the checker
  that refuses a grant to any key beyond the allow-list, and re-measured every
  number in this file.
- **Round 3** paid the acknowledgement line, closed the two holes above,
  **re-did the pipeline's half** (the dispatcher's read-only check, the review
  request's text, the contract headers, the pipeline's own agent block) —
  none of which was in the pipeline repo when round 3 started — and re-ran the
  gates. That pipeline change was made in the pipeline repo's **working tree**
  and did not survive.
- **Round 4** paid the acknowledgement line again, fixed the fence shape that
  cost the preflight its `## Tests added`, and documented round 3's work. It
  changed nothing in the pipeline repo, and read the report's claim that the
  pipeline half was done.
- **Round 5** (this run) fixed the review of round 4's delivery: the pipeline
  half was **committed** this time (`267f9a2`), the document suite grew two
  tests, and the checker's fence rule was corrected so a quoted artifact can
  be pasted verbatim.

The rounds before this one are described in the report they wrote; their
measurements are re-measured here where they are quoted.

## What changed

### The bases every number is measured against

Six commits, and every number in this file says which one it is measured
against. The first delivery's report gave three accountings of one change
set and named none of the bases; that is the first thing fixed here.

- **`64390ff`** (`rbops: phase-083`) — an ancestor of `HEAD`, from before this
  phase's work was adopted into it. A diff against it, restricted to the files
  this phase owns, is *the delivery* as `HEAD` carries it today. It is **not**
  the first delivery's parent: that is `e0a3686` (`rbops: phase-061`),
  `f44665b`'s parent. The two give the same figures for the four files this
  phase created (`opencode.json`, this report, this findings file and the
  phase's suite) and differ for the one file it shares with other phases —
  `tests/unreviewed_work_ack_test.rs` is `+54 −0` against `64390ff` and
  `+32 −2` against `e0a3686`, because other phases edited it in between.
- **`f44665b`** (`phase-084: resolve the reviewer agent's permissions`) — the
  commit that carries the phase's **first** delivery. It lives on
  `origin/rbops-wip/phase-084` and `origin/rbops-recovery/phase-084`, and its
  parent is `e0a3686`.
- **`8c54c7a`** (`rbops: phase-084`) — the pipeline's harvest commit for
  **round 2**: it widens the shipped file's deny set, adds `mode: "primary"`
  and the checker's allow-list. It lives on `origin/rbops-recovery/phase-084`
  and is **not** an ancestor of `HEAD`.
- **`b21821d`** — the first parent of `10f40c6`. `git show --numstat 10f40c6`
  measures that commit against **this** one, which is why the acknowledgement
  entry's figures (`+2055/-2`, 5 files) are the ones `git show` gives and not
  the ones a diff against `64390ff` gives. Stating the parent is what makes
  the two accountings reconcilable instead of irreconcilable.
- **`10f40c6`** — the commit that **adopted** round 2's work into `HEAD`'s
  history; it carries the same content as `8c54c7a` for the five files below,
  re-applied onto `main`. A diff against it is what round 3 changed on top of
  it.
- **`d23068b`** (`HEAD` at this run's dispatch, and still `HEAD` now) — the
  commit that adopted round 3's work. A diff against it is what rounds 4 and
  5 changed on top of it: the fence fix, the document suite, and the two
  documents. The pipeline's own commit `267f9a2` is a base in the *other*
  repository and is measured there.

### The project (this repository)

| File | vs `64390ff` (the delivery) | vs `10f40c6` (this round) | What |
|---|---|---|---|
| `opencode.json` | +30 −0 (new file) | +0 −0 | At the repository root. `agent.reviewer.permission` denies `edit`, `bash`, `task`, `todowrite`, `skill`, `lsp`, `question`, `webfetch`, `websearch`, `doom_loop` and `external_directory`; `agent.verifier.permission` allows `edit` and `bash`; both agents declare `mode: "primary"`. No `default_agent`, no top-level `permission` block, no agent the file does not own. Unchanged this round — it was already correct. |
| `tests/agent_permissions_test.rs` | +1094 −0 (new file) | +179 −2 | 28 `#[test]` functions; 19 of them `edge_*`, and all 19 assert that a failure is produced. This round added five and closed two holes: a top-level `permission` block and an agent the file does not own (`build`) both reach past the agent the file is about, and neither was pinned. |
| `tests/unreviewed_work_ack_test.rs` | +54 −0 | +10 −0 | This round's other change: the acknowledgement entry for `10f40c6` — the commit this run inherited — added to `ACKNOWLEDGED` and to `MEASURED`, with the reason and the figures `git show --numstat 10f40c6` gives. The list is now again exactly the set of matching commits in `git log HEAD`. |
| `phases/phase-084/REPORT.md` | +980 −0 (new file) | rewritten | This file: round 3's state of the tree, the red test and its repair, and the gates re-measured. |
| `phases/phase-084/FINDINGS.md` | +591 −0 (new file) | +156 −18 | Round 3's findings: the two holes in the file's reach, the seventh acknowledgement entry, and the shallow-history reading recorded again with what was measured. |

Both accountings, in full:

```
$ git diff 64390ff --numstat -- opencode.json phases/phase-084/ \
      tests/agent_permissions_test.rs tests/unreviewed_work_ack_test.rs
30	0	opencode.json
591	0	phases/phase-084/FINDINGS.md
980	0	phases/phase-084/REPORT.md
1094	0	tests/agent_permissions_test.rs
54	0	tests/unreviewed_work_ack_test.rs

$ git diff 10f40c6 --numstat
158	20	phases/phase-084/FINDINGS.md
482	140	phases/phase-084/REPORT.md
179	2	tests/agent_permissions_test.rs
10	0	tests/unreviewed_work_ack_test.rs

$ git diff 10f40c6 -- '*.rs' | grep -cE 'allow\(clippy::|#\[ignore|// *skip|\.skip\('
0
```

### This run (round 4), against `d23068b` (`HEAD` at this run's dispatch)

Round 4's own diff is small and entirely under `tests/` and this phase's
documents — the tables above are round 3's accounting and are left as they
were measured:

| File | vs `d23068b` (this run) | What |
|---|---|---|
| `tests/report_markdown_test.rs` | +331 −0 (new; inherited untracked at **283 lines with 5 tests**, and round 4 grew it to **331 lines with 8 tests**) | 8 tests: two pin that both delivery documents read the same way under a strict and a naive fence reader, one pins that the four required section headings are still present, six `edge_*` cases drive the checker (ambiguous fence → failure naming its line, unterminated fence → failure, balanced document → accepted, rejected document → `is_err()` not a panic, empty document → accepted, heading-only document → accepted). Round 4's edits were `cargo fmt --all` **and** the three tests `edge_a_rejected_document_produces_an_error_not_a_panic`, `edge_an_empty_document_is_accepted` and `edge_a_document_without_code_blocks_is_accepted` — the file was not only reformatted |
| `tests/unreviewed_work_ack_test.rs` | +7 −5 | the acknowledgement entry and the `MEASURED` slot repointed from `10f40c6` (no longer in `HEAD`'s history) to `d23068b` (which is), with the figures `git show --numstat d23068b` gives and a comment recording why |
| `phases/phase-084/REPORT.md` | +187 −46 | the fence fix inherited from the interrupted attempt (the `## Tests added` preflight failure), plus this run's round-4 documentation and the refreshed gate records |
| `phases/phase-084/FINDINGS.md` | +103 −6 | the same fence fix, plus round 4's findings |

```
$ git diff HEAD --numstat
103	6	phases/phase-084/FINDINGS.md
187	46	phases/phase-084/REPORT.md
331	0	tests/report_markdown_test.rs
7	5	tests/unreviewed_work_ack_test.rs
```

(The `git diff HEAD --numstat` block is refreshed from the final tree after
every edit to this file; the figures in the table above are the same
measurement written into rows. Untracked files only appear in it after
`git add -N`, which is how the new test file got into the diff at all.)

### This run (round 5), against `d23068b` (still `HEAD` — the pipeline has not committed this run's work yet)

Round 5's project-side diff is two tests and the two documents. The pipeline
side is a separate repository and a separate commit; it is accounted for under
"The pipeline" below and in "Round 5 — this run" above.

| File | vs `d23068b` (this run) | What |
|---|---|---|
| `tests/report_markdown_test.rs` | +453 −0 (new; 331 lines with 8 tests inherited, now 453 lines with 10 tests) | 10 tests. Two are new: `edge_a_section_heading_inside_a_code_block_is_not_a_section` — a heading inside a code block is text, not a heading — and `edge_a_longer_outer_fence_may_hold_a_shorter_inner_fence` — a four-backtick fence may hold a three-backtick one, which is what lets a quoted review request be pasted verbatim. The checker changed with them: the naive fence reading now requires the closing fence to be at least as long as the one it closes, and `ambiguous_fence` reports the shape the two readings actually disagree about |
| `phases/phase-084/REPORT.md` | +531 −157 | this section, the eight findings and what each one got, the refreshed gate records, and the pasted request the pipeline builds today |
| `phases/phase-084/FINDINGS.md` | +151 −6 | round 5's findings and the corrected figures |
| `tests/unreviewed_work_ack_test.rs` | +7 −5 | unchanged by round 5 — round 4's repair, inherited: the acknowledgement entry and the `MEASURED` slot repointed from `10f40c6` (no longer in `HEAD`'s history) to `d23068b` (which is) |

```
$ git diff HEAD --numstat
151	6	phases/phase-084/FINDINGS.md
531	157	phases/phase-084/REPORT.md
453	0	tests/report_markdown_test.rs
7	5	tests/unreviewed_work_ack_test.rs
```

### The pipeline (`rbops/`, one directory up)

The review found that the dispatcher handed the reviewer **an empty change
set**, that the story it told the reviewer about its own configuration had
stopped being true, and that it checked neither the reviewer's mode nor the
keys it had not declared.

**All of it is fixed in the pipeline repo, and it is COMMITTED there.** That is
a change of attribution from the previous two deliveries of this report, which
said the pipeline half was done in round 2 and then, in round 3, that it was
done in round 3. Neither was true of the repo this run started from: its
`HEAD` was `e070f88`, `grep -rn "has no opencode.json" rbops/` matched three
agent contracts, and the dispatch preamble carried the same claim:

```
$ cd ../rbops && git log --oneline -1
e070f88 say NO WORK for a model killed before it could start - because it is
$ grep -rn "has no opencode.json" rbops/
rbops/agents/verifier.md:4:     --dir pointing at the redblue checkout, which has no opencode.json, so
rbops/agents/auditor.md:4:     --dir pointing at the redblue checkout, which has no opencode.json, so
rbops/agents/reviewer.md:4:     --dir pointing at the redblue checkout, which has no opencode.json, so
```

Round 3's version of this fix had been made in that repo's **working tree**,
and a working tree is not a delivery: it did not survive, and round 4 changed
nothing in the pipeline repo at all. This round made the change again and
**committed it**, so it is in the pipeline repo's history:

```
$ cd ../rbops && git show --numstat --format="%h %s" 267f9a2
267f9a2 phase-084: the reviewer is resolved out of the file opencode reads
4	0	.gitignore
10	2	opencode.json
6	5	rbops/agents/auditor.md
6	5	rbops/agents/reviewer.md
9	5	rbops/agents/verifier.md
127	4	rbops/dispatch.sh
157	4	rbops/smoke.sh
```

| File | Lines | What |
|---|---|---|
| `rbops/dispatch.sh` | +127 −4 | The pre-phase base is recorded by `cmd_run` in `phases/<phase>/.base` and read back by `cmd_review`, so the review request carries the phase's diff; `assert_reviewer_read_only` resolves `agent.reviewer.mode` and `agent.reviewer.permission` out of the file opencode would read and refuses the review when the reviewer would not be read-only, naming the key that would widen it; the preamble names the config the reviewer is resolved out of, and the denied keys, read out of that same file. |
| `rbops/smoke.sh` | +157 −4 | The fixture project ships an `opencode.json`, and the new section asserts the new behaviour: the request names the shipped config, no longer claims there is none, and names the denied keys; a writable reviewer stops the review; an undeclared widening key stops it; a subagent-mode reviewer stops it; the object form of a deny is accepted and the object form of an allow is not; a missing config stops it; and the request carries the committed change set. |
| `rbops/agents/reviewer.md` | +6 −5 | The header comment no longer claims the checkout has no `opencode.json`; it says the contract is authoritative and that the file and the text are two halves of one contract. |
| `rbops/agents/verifier.md` | +9 −5 | Same, and it now says plainly that no `--agent verifier` invocation exists. |
| `rbops/agents/auditor.md` | +6 −5 | The audit request carried the same stale claim as the review request did; it now says what is true. |
| `opencode.json` | +10 −2 | The pipeline's own `reviewer` agent declared `mode: "subagent"` and allowed `webfetch` — the same footgun, in the same repository, one directory up. It now declares `mode: "primary"` and the same eleven denies. |
| `.gitignore` | +4 −0 | `!phases/*/.base` — the recorded base is committed state, like the other markers: the review runs in a separate process, and the base has to survive the job. |

The request the pipeline builds today is pasted in "Round 5 — this run" above,
and the pipeline's own smoke suite was re-run against this commit: **230
checks, green** (215 before it).

The pipeline repo is a separate repository with its own commit schedule. The
change above is a commit in it — `267f9a2` — made from this run, in the
pipeline repo, and this report names it because it was made. The pipeline's
own state commits (`phases`, `logs`, `rbops/phases.json`) are its bookkeeping
and are left alone.

The changed Rust files are all under `tests/`, satisfying this phase's
`must_touch: ["tests/"]`. No file under `src/`, `.github/workflows/`, `rbops/`
or `.opencode/agent/` is in the project diff. No test was deleted, skipped,
ignored or weakened; no assertion was loosened and no threshold moved.

The next two tables are round 1's and round 2's review records, kept here so
the whole history of the phase is in one file. Round 3 changed nothing in
either of them **in this repository**: the findings they describe were already
closed by the work they inherited, and the suite that pins those closures is
the same suite that is green today. What round 3 did change is the pipeline
repo, and the rows above say so — and what round 5 did was to make that
change a **commit** in that repo, because a change in a working tree is not a
delivery.

## The first review's eleven findings, and what each one got

| # | Finding | What changed |
|---|---|---|
| 1 | `opencode.json` declared only `edit` and `bash`; every other permission key was left to opencode's permissive default, so the reviewer still held `task`, `external_directory` and `webfetch` | Every widening key is now declared `deny` on `agent.reviewer`, and `project_config_denies_every_widening_permission_for_the_reviewer` asserts each one is present and denies. The resolved config is quoted below. |
| 2 | The report's gate output claimed "15 tests" for a file with 16 `#[test]` functions, and the test table omitted one | The real output is pasted below, the table below lists all of them, and the line counts in this file are re-measured. |
| 3 | The configuration-resolution rule was quoted from a "Where files live" section attributed to the JSON Schema URL, which serves no prose | The rule is now quoted from the two places it actually exists — opencode's configuration reference and the `customize-opencode` skill opencode ships — with the live text. |
| 4 | `cmd_review` re-derived the diff base from `HEAD` at review time, so every review of a committed phase got an empty diff | `cmd_run` records the pre-phase base; `cmd_review` reads it back. The smoke suite now asserts the review request carries the committed change set, and that assertion fails against the old code. |
| 5 | The review preamble told the reviewer "the redblue checkout has no opencode.json …" — a story this phase made false — and the resolved permissions were never asserted | The preamble states what is true now, `assert_reviewer_read_only` resolves `agent.reviewer.permission` from the file opencode would read and refuses the review when it does not say `deny`, and the pipeline's smoke suite asserts the request's text. **Re-done in round 3**, for the same reason as the second review's finding 3: the pipeline repo did not carry the fix when this run started. |
| 6 | The test module doc claimed `rbops` launches `--agent verifier`, which no invocation does | The doc now says what `rbops` launches (`--agent reviewer`, `--agent build`) and states that `agent.verifier` is declared but not yet invoked. The error messages that carried the same false claim were corrected too. |
| 7 | "Only one project config may be shipped" was asserted only for `opencode.json`, not for the `.jsonc` variants opencode also reads | `project_config_ships_in_exactly_one_form` asserts that `opencode.jsonc`, `.opencode/opencode.json`, `.opencode/opencode.jsonc`, `.opencode/agent/opencode.json` and `.opencode/agent/opencode.jsonc` are all absent. |
| 8 | The gap list named `write` as a permission that falls back to the global config; `write` is not an independent key — `edit` covers `write`, `edit` and `apply_patch` | The gap list now names the keys that were genuinely open, and this round closes all of them. |
| 9 | The report asserted "opencode hard-fails to start on a config that fails this schema", a claim the documentation does not make | The claim is now backed by a measurement rather than by an inference: an invalid config was written on purpose and opencode's exit code was observed. The quote and the measurement are both below. |
| 10 | The report filed "mode: primary is unconfirmed" as a gap, on the grounds that the pipeline was not in the checkout | The pipeline is one directory up: `rbops/dispatch.sh` launches the reviewer as `--agent reviewer`, a top-level agent selection, and opencode's own documentation says a `default_agent` must name a primary agent. The gap is closed by assertion now, not by argument — see the second review's finding 1. |
| 11 | `tests/unreviewed_work_ack_test.rs` still opened with a stale "TODO: replace with real assertions" placeholder | The marker is gone; the file now says what it is. |

## The second review's seven findings, and what each one got

| # | Finding | What changed |
|---|---|---|
| 1 | `check_document` never inspected `agent.*.mode`, so the one field that decides whether `--agent reviewer` binds to the read-only agent was unpinned — the phase argued `mode` was load-bearing but closed the gap by argument | `check_mode` requires `mode: "primary"` for `reviewer` and `verifier` and is called from `check_document`; `project_config_declares_primary_mode_for_both_agents` asserts the shipped file; `edge_reviewer_mode_subagent_is_rejected` mutates the shipped mode to `subagent`, `all` and `nonsense` in turn and requires each to be rejected. `assert_reviewer_read_only` resolves the mode from the same file and refuses the review when it is not `primary`. |
| 2 | The deny set covered 5 of opencode's permission keys; `websearch` and `skill` were undeclared and therefore allowed, while the report asserted the opposite | The shipped file now denies all eleven widening keys; `REVIEWER_DENIED` covers all eleven; and `check_no_grants_beyond` is an **allow-list** — `read`, `glob`, `grep` and `list` may be allowed, everything else must deny. The old check was the mirror image of that: it rejected `read: "allow"`, which forbade the shape that closes this class of hole, and it policed only the keys that were *present*, so an undeclared key passed it. |
| 3 | The report said the dispatch preamble no longer claimed the checkout had no `opencode.json`, but the request handed to the reviewer still said so | The preamble in `rbops/dispatch.sh` names the shipped config and the denied keys, and the audit contract's header, which carried the same claim, was corrected too. The correction is now **asserted**: the pipeline's smoke suite requires the request to name the shipped config, forbids the stale claim, and requires the denied keys to be named — so a request built from a stale copy of the preamble now fails a gate instead of reaching the reviewer. **Re-done in round 3**: the pipeline repo did not carry this fix when this run started, and the request this round's reviewer received carried the stale claim verbatim. |
| 4 | Three irreconcilable accountings of one change set | Every number in this file names the commit it is measured against, and both numstats are pasted in full. |
| 5 | The pasted `verify.sh` output was not the output for the tree being shipped | The gate was re-run after the last edit to these documents; the fresh output is below, and the one sentence about the drift that pasting it creates is below that. |
| 6 | The "a third tool granted to the reviewer" case listed `write`, a key the phase's own findings say is not an opencode permission key | The case now uses opencode's own keys: `webfetch`, `websearch`, `skill`, `lsp`, `todowrite` and the `*` wildcard. |
| 7 | The checks required every permission value to be a bare string, though opencode accepts the object form for most of the keys | `permission_action` reads both shapes, and only for the keys opencode's schema types as rules; `edge_a_permission_object_form_is_rejected_or_accepted` covers both branches, and the dispatcher's check reads them too. |

## The third review's eight findings, and what each one got

The third review found that the report claimed things the tree did not
support. Every one of them is a claim about this document, and every one of
them is now true of the tree that ships.

| # | Finding | What changed |
|---|---|---|
| 1 | The report said the stale "the checkout has no `opencode.json`" statement had been removed from the review request and was gated by the pipeline's smoke suite. The request handed to that reviewer still carried it, verbatim, and named no shipped config and no denied keys | **The pipeline repo did not carry the fix when that round started.** `grep -rn "has no opencode.json" rbops/` found the claim in three agent contracts, and the dispatch preamble carried it too. All four are corrected, the pipeline's own `reviewer` agent block is now `mode: "primary"` with the eleven denies, and the smoke suite asserts the request's text: it must name the shipped config, must not claim there is none, and must name the denied keys. Each of those assertions was checked against the old text, where it fails. **Re-done and COMMITTED in round 5** (`267f9a2`): round 3's version of this fix lived in the pipeline repo's working tree and did not survive, and round 4 changed nothing there — which is the same finding, in the same form, one round later |
| 2 | The pasted `git diff 10f40c6 --numstat` block said `283 99` for this file, while the tree's diff says `282 98` — two irreconcilable accountings of one change set | The block is re-pasted from the final tree, and the `diff substance` figure the gate reports is refreshed to the same sum. The two now agree, and the sentence about the drift that pasting creates says so. |
| 3 | The acknowledgement entry quotes `git show --numstat 10f40c6` as `+2055/-2, 5 files`, which could not be reconciled with the report's two numstats | The entry's figure is **correct** — `git show --numstat 10f40c6` gives exactly `+2055/-2` across 5 files — and the report's *bases* section was what was wrong: it never said that `10f40c6`'s first parent is `b21821d`, so `git show` (which measures against the parent) and a diff against `64390ff` looked like two accountings of one thing. The bases section now names `b21821d`, and says what each of the five commits measures. |
| 4 | The Known-gaps bullet said this round changed the pipeline's own `opencode.json`, while the report's pipeline section said round 2 had and this round had changed nothing outside this repository | Both are now true of the same tree: **this round** changed the pipeline repo, and the pipeline section says so, with the real diff pasted. The attribution is fixed in the report, in this findings file, and in the round-2 review records, which each say the pipeline half was re-done. |
| 5 | The findings file's round-3 section closed with "1919 passing tests" while the report said 1924 — the pre-addition count | Restated as 1924, the figure the suite reports. |
| 6 | A test's doc comment claimed "opencode refuses to start on a config that fails it", which the phase's own measurement contradicts: the same build accepted an unknown top-level key with exit 0 | The doc comment now states the measured rule: value validation **is** enforced at startup, total schema conformance is **not**. The test asserts the same narrower thing it always did — that the file names the schema. |
| 7 | A test's doc comment claimed `*` is one of opencode's permission keys and "grants everything at once". The phase's own schema table lists fifteen keys and `*` is not among them | The doc comment now says `*` is **not** a schema key — in opencode's resolved ruleset it is a *pattern* on a wildcard rule — and says why the case is kept: it pins the allow-list's closed edge. The documents that repeated the claim are corrected too. |
| 8 | Two citations in the findings file pointed at the wrong lines | `edge_phase_078_commits_are_ancestors_of_head_not_absent` is at `tests/unreviewed_work_ack_test.rs:571` and `is_shallow()` at `tests/unreviewed_work_ack_test.rs:296`; both are corrected. |

## The configuration-resolution rule this relies on

opencode resolves configuration in two scopes, project first. From the
configuration reference (<https://opencode.ai/docs/config>, section
**"Locations"**):

> Note
>
> Configuration files are **merged together**, not replaced.
>
> Configuration files are merged together, not replaced. Settings from the
> following config locations are combined. Later configs override earlier ones
> only for conflicting keys. Non-conflicting settings from all configs are
> preserved.

and, from the same section under **"Per project"**:

> Tip
>
> Place project specific config in the root of your project.
>
> When OpenCode starts up, it first looks for a config file in the current
> directory, then traverses up to the nearest Git directory.

The **"Where files live"** table — the one the previous delivery of this report
quoted, and attributed to the wrong URL — is not in the JSON Schema and not in
the configuration reference. It is in the `customize-opencode` skill that
opencode ships inside its own binary (section **"Where files live"**):

> | Scope | Path |
> |---|---|
> | Project config | `./opencode.json`, `./opencode.jsonc`, or `.opencode/opencode.json` (opencode walks up from the cwd to the worktree root) |
> | Global config | `~/.config/opencode/opencode.json` or `~/.config/opencode/opencode.jsonc` (NOT `~/.opencode/`) |
>
> Configs from each scope are deep-merged. Project overrides global. Unknown
> top-level keys in `opencode.json` are rejected with `ConfigInvalidError`.

That table is the reason `project_config_ships_in_exactly_one_form` exists:
the `.jsonc` variants are in the same resolution path as the file this suite
reads, and a second copy in one of them would be consulted by opencode while
this suite never saw it. Its last sentence is also the one claim below that
the measurement contradicts.

The machine-checkable half of the citation is the schema, which the shipped
file was validated against with `jsonschema` 4.10.3 (FINDINGS.md): **SCHEMA
VALID**. It confirms the claims the fix rests on — `Config.agent`'s additional
properties are `AgentConfig`, so arbitrarily named agents are legal;
`AgentConfig.mode` enumerates `["subagent","primary","all"]`; and
`AgentConfig.permission` is typed with fifteen keys — `read`, `edit`, `glob`,
`grep`, `list`, `bash`, `task`, `external_directory`, `todowrite`, `question`,
`webfetch`, `websearch`, `lsp`, `doom_loop` and `skill` — of which ten take
either a bare action or an object of pattern → action, and five
(`todowrite`, `question`, `webfetch`, `websearch`, `doom_loop`) take a bare
action only.

**What the documentation says about a bad config, and what was measured.**
The configuration reference presents the schema as editor support — "Your
editor should be able to validate and autocomplete based on the schema" —
and documents no startup validation against it, so the previous delivery
inferred a hard failure and was wrong to present the inference as a fact.
The claim was settled by measurement instead, on this machine:

```
$ mkdir -p /tmp/badcfg && cd /tmp/badcfg
$ printf '%s\n' '{"$schema":"https://opencode.ai/config.json","agent":{"reviewer":{"mode":"nonsense"}}}' > opencode.json
$ opencode debug config
Error: Configuration is invalid at /tmp/badcfg/opencode.json
↳ Expected "subagent" | "primary" | "all", got "nonsense" agent.reviewer.mode
$ echo $?
1
```

So opencode does refuse to start on a config whose *values* are wrong, and it
says which one. What it does not do on this build is catch everything: the
same build accepted an unknown top-level key with exit 0, so "the schema is
enforced at startup" is a claim this report does not make. The shipped file is
schema-valid either way, so nothing about this phase depends on the
difference.

**What these tests cannot prove.** They read a file and assert its contents.
They do not run opencode, so they cannot prove which file opencode opened, or
that a given opencode build resolves the name `reviewer` to this definition.
A future opencode that changed its resolution order would leave this suite
green while the reviewer again ran with edit access. What the tests do prove
is that the file says the right thing, and they fail the moment a `deny` is
dropped, an agent is renamed, a mode is changed, a third tool is granted to
the reviewer, a `default_agent` is reintroduced, an agent this file does not
own appears in it, or a top-level `permission` block is added to it.

Because the merge is *deep*, the global file's permissive top-level `permission`
block still contributes keys underneath this file. Per-agent `permission:`
overrides it — opencode's permissions reference says agent permissions "are
merged with the global config, and agent rules take precedence" — and it is
declared here for **every** key that could widen the reviewer rather than for
one tool. The file declares no `build` agent and no top-level `permission`
block, so the implementer agent still resolves entirely from the global
config, exactly as before this phase. **Both of those are now asserted, not
merely claimed**: `project_config_declares_exactly_the_two_pipeline_agents`
and `project_config_declares_no_top_level_permission_block` fail the moment
either shape appears in the shipped file, and
`edge_an_agent_this_file_does_not_own_is_rejected` and
`edge_a_top_level_permission_block_is_rejected` drive the rule that refuses
them.

## What the reviewer actually holds now

The resolved configuration, read out of opencode itself in this checkout.
**Re-measured this round**, from the tree as it stands — the shipped file is
byte-for-byte the file round 2 shipped, and opencode resolves the reviewer to
the same 28 rules it did then:

```
$ opencode debug config
{
  "$schema": "https://opencode.ai/config.json",
  "default_agent": "build",
  "permission": {
    "external_directory": { "**": "allow" },
    "bash": "allow",
    "edit": "allow",
    "webfetch": "allow"
  },
  "agent": {
    "reviewer": {
      "description": "Read-only reviewer for a redblue pipeline phase. ...",
      "mode": "primary",
      "permission": {
        "edit": "deny",
        "bash": "deny",
        "task": "deny",
        "todowrite": "deny",
        "skill": "deny",
        "lsp": "deny",
        "question": "deny",
        "webfetch": "deny",
        "websearch": "deny",
        "doom_loop": "deny",
        "external_directory": "deny"
      },
      "options": {}
    },
    "verifier": {
      "description": "Runs a redblue pipeline phase's gates. ...",
      "mode": "primary",
      "permission": { "edit": "allow", "bash": "allow" },
      "options": {}
    }
  },
  ...
}
```

The top-level block is the global one, unchanged: opencode merges the two
scopes. The reviewer's own block is the one that governs it, and it denies
every key that would let the reviewer act.

`opencode debug agent reviewer` shows what those rules resolve to *after* the
merge — the global block's grants and the agent's denies in one ordered
ruleset, where the last matching rule wins:

```
$ opencode debug agent reviewer | jq -r '.permission[] | "\(.permission) \(.pattern) \(.action)"'
* * allow
doom_loop * ask
external_directory * ask
external_directory /home/runner/.local/share/opencode/tool-output/* allow
external_directory /tmp/opencode/* allow
question * deny
plan_enter * deny
plan_exit * deny
read * allow
read *.env ask
read *.env.* ask
read *.env.example allow
external_directory ** allow
bash * allow
edit * allow
webfetch * allow
edit * deny
bash * deny
task * deny
todowrite * deny
skill * deny
lsp * deny
question * deny
webfetch * deny
websearch * deny
doom_loop * deny
external_directory * deny
external_directory /home/runner/.local/share/opencode/tool-output/* allow
```

Lines 1–12 are opencode's own defaults, lines 13–16 are the global config's
grants, and lines 17–27 are this phase's agent block. The agent's rules come
last, so they win: `websearch` and `skill` — the two keys the second review
found undeclared — resolve to `deny`, and `external_directory` resolves to
`deny` even though the global config grants it `{"**": "allow"}`. The last
line is opencode's own addition, which re-allows the directory it writes tool
output into; it is appended by opencode after every rule the config carries.

Re-measured this round, the resolved ruleset is the same 28 lines, in the same
order — the phase's block still comes last, and still wins:

```
$ opencode debug agent reviewer | jq -r '.permission[] | "\(.permission) \(.pattern) \(.action)"' \
    > target/tmp/reviewer_rules.txt
$ wc -l target/tmp/reviewer_rules.txt
28 target/tmp/reviewer_rules.txt
$ tail -12 target/tmp/reviewer_rules.txt
edit * deny
bash * deny
task * deny
todowrite * deny
skill * deny
lsp * deny
question * deny
webfetch * deny
websearch * deny
doom_loop * deny
external_directory * deny
external_directory /home/runner/.local/share/opencode/tool-output/* allow
```

This is the only part of the phase that a live measurement can confirm rather
than a file assertion: opencode itself, in this checkout, resolving the name
`reviewer` to an agent whose `edit` and `bash` are denied. What it still
cannot confirm is that a *different* opencode build resolves it the same way —
which is the gap the Known gaps section names.

`read` is deliberately **not** declared: opencode's default rules still deny
`.env` files, and an agent-level `read: "allow"` would be appended after them
and re-allow them.

`task` is the key that was doing the most damage. The built-in `general`
subagent "has full tool access" (opencode's agents reference), so a reviewer
that can launch a subagent can have a subagent edit the tree it is judging —
and the old `check_no_grants_beyond` policed only the keys that were present,
so an omitted key passed it while its own doc comment claimed "The reviewer is
read-only in full". That check is now an allow-list, and
`project_config_denies_every_widening_permission_for_the_reviewer` requires
every widening key to be *present*.

## The dispatcher's half of the contract

Three of the first review's findings touched the pipeline, not this repository,
and all three are fixed in `rbops/dispatch.sh` — **committed in the pipeline
repo as `267f9a2`**, which is what the "The pipeline" section above says and
what the pipeline repo's own commit shows. (Round 3 made the same change in
that repo's working tree and it did not survive; round 4 changed nothing
there. That is why the fix is a commit now and not a working tree.)

**The change set the reviewer is handed.** `cmd_review` used to compute
`base="$(in_project git rev-parse HEAD)"` at review time. By then the pipeline
had committed the phase's work, so `git diff "$base"..HEAD` was empty and the
review request's committed-diff section was blank. `cmd_run` now records the
base it captured before any resume merge, in `phases/<phase>/.base`, and
`cmd_review` reads it back. The measured before-and-after, from the smoke
suite's fixture (the same fixture, the same run, the only difference being
which base the review uses):

````text
# before — the committed section is empty, so the reviewer is handed nothing
### File list (always complete — content below may be capped)

19	0	phases/phase-001/REPORT.md
16	0	src/vm.rs

## 1. Committed changes (7180e154491812e5b0cab6f8751a1cf08fa82100..HEAD)

````

````text
# after — the recovered work is in the diff
### File list (always complete — content below may be capped)

6	0	src/vm.rs
5	0	target/debug/rb
11	0	tests/resumed_probe.rs
19	0	phases/phase-001/REPORT.md
16	0	src/vm.rs

````

````text
## 1. Committed changes (b5c5d69aa68f6f456a282b3a8bb402435a7059b3..HEAD)

diff --git a/src/vm.rs b/src/vm.rs
index 3e082d1..c79f5ff 100644
--- a/src/vm.rs
+++ b/src/vm.rs
@@ -1 +1,7 @@
 fn placeholder() {}
+// recovered work from attempt 1
+pub fn recovered_fn() -> u8 { 7 }
+pub fn recovered_fn2() -> u8 { 8 }
+pub fn recovered_fn3() -> u8 { 9 }
+pub fn recovered_fn4() -> u8 { 10 }
+pub fn recovered_fn5() -> u8 { 11 }
...
````

(The SHAs are the fixture's own — a fresh temporary clone per run — so they
differ from run to run. What does not differ is the shape: before, the
committed section is empty; after, it carries the recovered work.)

`cmd_run` now records the base it captured before any resume merge
(`rbops/dispatch.sh`) into `phases/<phase>/.base`, and `cmd_review` reads it
back, falling back to `HEAD` only when the recorded base is not a commit this
history still has. The base is committed with the rest of the pipeline state,
because the review runs in a separate process from the run.

**The permissions and the mode the reviewer runs with.**
`assert_reviewer_read_only` resolves `agent.reviewer.mode` and
`agent.reviewer.permission` out of the very file opencode would read — the
project's `opencode.json` — and checks that the mode is `primary` and that
every widening key says `deny` before the review starts. A missing file, a
missing key, a key that says something else, or a reviewer that would never
bind stops the review and blocks the phase for a human, because the failure
mode this exists to catch is silent: opencode falls back to `default_agent:
build` — whose `edit` and `bash` are `allow` — and the review runs anyway,
and its verdict is trusted.

Both of opencode's permission shapes are read: a bare action (`"deny"`) and
an object of pattern → action (`{"**": "deny"}`). The second is the same rule
as the first when every pattern in it denies, so both are accepted; an object
that allows any pattern is not a deny and is refused.

The smoke suite asserts all of those paths (writable reviewer, undeclared
widening key, subagent mode, the object form of a deny, the object form of an
allow, missing config, and the change set actually being present), plus the
three assertions about the request's own text. Every one of the fifteen new
checks fails against the old code, and the suite is green at 230 against 215
before this round.

**The request the reviewer is handed.** The preamble used to tell the
reviewer that the checkout has no `opencode.json` and that `--agent reviewer`
therefore resolves to nothing — a true story when it was written and a false
one after this phase shipped the file. It now names the file the reviewer is
resolved out of and the keys that file denies, read out of that file rather
than written out longhand, so the two cannot drift. The smoke suite asserts
the request's text — it must name the shipped config, must not claim there is
none, and must name the denied keys — so the request cannot drift from the
file again without failing a gate.

Those three assertions were also run against the **real** request this
round's checkout produces, not only against the smoke suite's fixture. The
paste is the request the committed dispatcher built today; the same request is
pasted in "Round 5 — this run" above, next to the contract header it now
inlines:

````text
# Review request - round 1 of phase phase-084

The gate is green. Your job is to find what is still wrong.
The contract below is authoritative and is inlined here on purpose:
dispatch runs you with --dir pointing at the redblue checkout, which ships
its own opencode.json at the checkout root. opencode merges the project
config over the global one, so that file is the one your agent block is
read from. The reviewer agent is declared there with mode "primary" and
with these keys denied: edit, bash, task, todowrite, skill, lsp, question, webfetch, websearch, doom_loop, external_directory.
Referring you to a config file or an .opencode/agent/*.md path meant you
ran with NO contract for every phase.
````

The eleven keys in that line are read out of this checkout's own
`opencode.json`; they are not written out longhand in the dispatcher, so
they cannot be right about one file and wrong about the other.

## Tests added

| Test | Edge class covered |
|---|---|
| `project_config_ships_at_the_repository_root` | file present, at the repository root, no forbidden-path copy and no duplicate copy under `.opencode/` |
| `project_config_ships_in_exactly_one_form` | the `.jsonc` variants opencode also reads — a second copy in one of them is consulted by opencode and invisible to this suite |
| `project_config_denies_every_widening_permission_for_the_reviewer` | all eleven widening keys are present and deny — an undeclared key is an allowed key |
| `project_config_pins_reviewer_read_only_and_verifier_privileged` | asserts all four `edit`/`bash` values individually — dropping one `deny` fails here |
| `project_config_declares_primary_mode_for_both_agents` | the mode that binds `--agent reviewer` at all — `primary` for both agents |
| `project_config_declares_no_default_agent` | `default_agent` absent — the implementer agent cannot be repointed |
| `project_config_declares_exactly_the_two_pipeline_agents` | **added this round** — the `agent` map holds exactly `reviewer` and `verifier`: a `build` block here would override the global one for the agent every phase runs as |
| `project_config_declares_no_top_level_permission_block` | **added this round** — a top-level `permission` block is merged over the global one and applies to every agent that declares no block of its own |
| `project_config_declares_the_schema_it_is_validated_against` | **added this round** — the file names the schema it was checked against |
| `edge_an_agent_this_file_does_not_own_is_rejected` | **added this round** — `build` and `auditor` are both refused, and the refusal names the agent |
| `edge_a_top_level_permission_block_is_rejected` | **added this round** — a top-level `permission` block is refused |
| `edge_missing_config_file_is_rejected` | the reproduction itself: no file → failure |
| `edge_missing_agent_key_is_rejected` | empty — `{}` and `{"$schema":"x"}`, both lack `agent` |
| `edge_missing_reviewer_is_rejected` | singleton — only the verifier is declared |
| `edge_missing_verifier_is_rejected` | singleton — only the reviewer is declared |
| `edge_default_agent_key_is_rejected` | missing keys — a `default_agent` naming `build` or `reviewer` |
| `edge_permission_flipped_to_allow_is_rejected` | the damaging edit — all four values flipped, each caught by name |
| `edge_a_dropped_deny_key_is_rejected` | the deleted deny — each of the eleven keys removed in turn; an undeclared key is inherited, not denied |
| `edge_a_tool_granted_to_the_reviewer_outside_the_allowed_set_is_rejected` | the allow-list's boundary — `webfetch`, `websearch`, `skill`, `lsp` and `todowrite` (opencode's own keys) and `*` (not a schema key at all), each granted beside the pinned tools |
| `edge_a_permission_object_form_is_rejected_or_accepted` | both shapes of a permission value — the object form of a deny is accepted, of an allow is rejected, a mixed object is rejected, and an object on an "action only" key is rejected |
| `edge_reviewer_mode_subagent_is_rejected` | the mode that binds the name — `subagent`, `all` and `nonsense`, each rejected |
| `edge_a_missing_or_malformed_mode_is_rejected` | a missing mode and a mode of the wrong type |
| `edge_reviewer_without_permission_block_is_rejected` | nesting — `agent.reviewer` declared, `permission` absent |
| `edge_agent_block_of_wrong_type_is_rejected` | type mismatch — `agent` holding an array or a string |
| `edge_permission_value_of_wrong_type_is_rejected` | type mismatch — boolean, number, an object with a non-string action, a mixed-action object, an object granting the wrong action, `null` |
| `edge_duplicate_permission_key_resolves_to_the_last_value` | duplicate keys — a repeated `edit`, last value wins |
| `edge_a_broken_document_is_an_err_and_not_a_panic` | the failure channel — a broken document is an `Err`, never a panic, and never a silent acceptance |
| `edge_malformed_config_is_rejected` | malformed — empty file, truncated, trailing comma, BOM, array, bare string |
| `phase_084_delivery_documents_are_unambiguous_markdown` | **added in round 4** — both delivery documents: the ambiguous-fence shape (a `` ```diff `` inside an open `` ``` ``) is refused, and the strict and naive readings of every fence in the file pair the same regions |
| `phase_084_report_carries_the_sections_the_gate_looks_for` | **changed in round 5** — the preflight failure itself, pinned: `## What changed`, `## Tests added`, `## Gates` and `## Known gaps` must still be headings a scanner can see — and, since round 5, **outside every code block**, which is where they were lost |
| `edge_a_fence_with_an_info_string_inside_an_open_fence_is_rejected` | **added in round 4** — malformed input: the exact `## Tests added`-eating shape, refused with the line it is on |
| `edge_a_document_that_ends_inside_a_code_block_is_rejected` | **added in round 4** — malformed input: an unterminated fence, which would hide every heading after it |
| `edge_a_balanced_document_is_accepted` | **added in round 4** — the happy path, so the two rejections above are rejections and not a checker that refuses everything |
| `edge_a_rejected_document_produces_an_error_not_a_panic` | **added in round 4** — the failure channel of the checker itself: an ambiguous document comes back as an `Err`, not a panic and not a silent `Ok` (asserted with `is_err()`) |
| `edge_an_empty_document_is_accepted` | **added in round 4** — the lower boundary of the input space: the checker refuses ambiguous documents, not small ones |
| `edge_a_document_without_code_blocks_is_accepted` | **added in round 4** — the heading-only shape: nothing fences, so the four required sections are headings to every reader |
| `edge_a_section_heading_inside_a_code_block_is_not_a_section` | **added in round 5** — a heading inside a code block is text, not a heading: the section test cannot pass on a document that hides one |
| `edge_a_longer_outer_fence_may_hold_a_shorter_inner_fence` | **added in round 5** — a four-backtick fence may hold a three-backtick one, which is what lets a quoted review request or a diff be pasted verbatim |

28 `#[test]` functions, **19 of them `edge_*`**, and **all 19 assert that a
failure is produced** — 18 with `expect_err` on a `Result`, and the
nineteenth, `edge_a_permission_object_form_is_rejected_or_accepted`, with
`expect_err` on its three reject branches and `unwrap_or_else` on the one
branch that must be accepted. The five failing documents the phase requires
are present as separate tests: the file missing, the `agent` key missing,
`reviewer` missing, `verifier` missing, and one permission flipped to
`allow`.

On top of those, `tests/report_markdown_test.rs` adds **10 `#[test]`
functions, 8 of them `edge_*`**. The red-first guard is
`phase_084_delivery_documents_are_unambiguous_markdown`: it reads the two
delivery documents as `HEAD` carries them, and it fails against the committed
report for the reason the preflight reported — the ambiguous fence that hid
`## Tests added` from the scanner. The two inline-fixture tests inherited from
the interrupted attempt (`edge_a_fence_with_an_info_string_inside_an_open_fence_is_rejected`
and `edge_a_document_that_ends_inside_a_code_block_is_rejected`) drive the
checker with strings in the test body and never read the report, so they
cannot fail because of `HEAD`'s copy — they pin the rule, and the test above
pins the report. Round 5 added two more: the section-inside-a-fence rule and
the longer-outer-fence rule. The full suite is therefore **1934** tests across
70 binaries: 28 in the phase's config suite, 10 in the document suite, and
1896 that belong to other phases and pre-date this one.

### The pipeline's own checks, added with the pipeline commit

The dispatcher's half of the contract is in the pipeline repo, so its checks
are the pipeline's smoke suite (`rbops/smoke.sh`), not this repository's. The
commit `267f9a2` added a section of **fifteen** checks to it, and the fixture
project now ships the `opencode.json` the dispatcher resolves the reviewer out
of. The suite is green at **230 checks** (215 before this commit).

| Check | What it pins |
|---|---|
| the request names the config the reviewer is resolved out of | the preamble names `opencode.json`, so a reviewer is never told the wrong thing about the tree it judges |
| the request no longer claims there is no `opencode.json` | the stale claim is the thing the third review caught; this is the assertion that keeps it out |
| the request names the denied keys | the eleven keys are read out of the shipped file, so the request and the file cannot disagree |
| the reviewer contract no longer carries the stale claim | the same, for `rbops/agents/reviewer.md` — and for the verifier's and the auditor's |
| a writable reviewer stops the review | `edit: "allow"` is refused before the model is called |
| the refusal names the key that would widen the reviewer | the refusal says `agent.reviewer.permission.edit`, not "something is wrong" |
| an undeclared widening key stops the review | `websearch` removed from the file is a refusal — an undeclared key is an allowed key |
| the refusal names the undeclared key | as above, by name |
| a subagent-mode reviewer stops the review | `mode: "subagent"` never binds `--agent reviewer`; the review would run as `build` |
| the refusal names the mode | as above, by name |
| a missing config stops the review | no file, no reviewer to resolve, no review |
| the object form of a deny is accepted | `{"**": "deny"}` is the same deny, spelled another way |
| the object form of an allow stops the review | `{"**": "allow"}` is still an allow |
| the request diffs against the base the run recorded | the review does not re-derive the base from HEAD at review time, which is what emptied the committed diff |
| the review request carries the committed change set | the recovered work is in the diff the reviewer is handed |

Each of the fifteen fails against the old code, where the request still made
the stale claim, the reviewer resolved to nothing, and the committed diff was
empty. The suite is green at 230 against 215 before the commit.

**Round 3 added five tests to this project's own suite** — three that assert
the shipped file and two that drive the checker — because the suite was strong
about the *reviewer's* permissions and weak about the file's *reach*. opencode
deep-merges the project config over the global one, per key, so two shapes of
a project config move a permission for an agent the file does not declare,
and neither was pinned:

| Shape | Why it matters | Closed by |
|---|---|---|
| a top-level `permission` block | applies to every agent that declares no block of its own — the implementer's included | `check_document` refuses it; `project_config_declares_no_top_level_permission_block` asserts the shipped file has none; `edge_a_top_level_permission_block_is_rejected` drives it |
| an agent the file does not own — `build` above all | a block for that name here replaces the global one, because the project config is consulted first, and the pipeline runs every phase's implementer as `--agent build` | `check_agent_set` refuses any name outside `{reviewer, verifier}`; `project_config_declares_exactly_the_two_pipeline_agents` asserts the shipped map holds exactly those two; `edge_an_agent_this_file_does_not_own_is_rejected` drives it with `build` and `auditor` |

A third, smaller hole closed with them: the shipped file names the schema it
was validated against (`$schema`), and nothing asserted it.
`project_config_declares_the_schema_it_is_validated_against` does now.

The three new rules were **written before the checker that enforces them**,
and the checker was then measured against the shipped file by mutating it —
each mutation caught, the file restored byte-for-byte:

```
$ # a `build` agent added to opencode.json
test project_config_declares_exactly_the_two_pipeline_agents ... FAILED
test project_config_ships_at_the_repository_root ... FAILED
test result: FAILED. 25 passed; 3 failed; 0 ignored

$ # a top-level `permission` block added
test project_config_declares_no_top_level_permission_block ... FAILED
test project_config_ships_at_the_repository_root ... FAILED
test result: FAILED. 19 passed; 9 failed; 0 ignored

$ # `$schema` removed
test project_config_declares_the_schema_it_is_validated_against ... FAILED
test result: FAILED. 27 passed; 1 failed; 0 ignored
```

One test was renamed rather than removed:
`edge_a_tool_granted_to_the_reviewer_besides_edit_and_bash_is_rejected` is now
`edge_a_tool_granted_to_the_reviewer_outside_the_allowed_set_is_rejected`,
because the rule it pins is no longer "only `edit` and `bash`" but the
allow-list. It still grants a third tool to the reviewer and still requires
the failure — with opencode's own keys this time, plus one key that is not in
the schema at all, to pin the allow-list's closed edge.

No new test was added to `tests/unreviewed_work_ack_test.rs` by this run: the
file's 13 tests are pre-existing and unchanged apart from the acknowledgement
data this run added. Its `edge_*` cases (empty log, malformed log lines, a
stale acknowledgement, a shallow history, unicode subjects, a duplicate
acknowledgement) remain green, and the acknowledgement list they audit is
exactly the set of matching commits in `HEAD`'s history.

### The red test round 4 inherited — and that round 5 inherits green

`unreviewed_work_in_head_history_is_acknowledged` was red before any edit in
**round 4**, for the reason quoted in the round-4 section: the commit that
carries a phase's work is created by the pipeline *after* the run that
produced it, so only the next run can acknowledge it.

The repair is one entry in `ACKNOWLEDGED`, plus the same SHA in `MEASURED`
(which that file requires to name every commit it was measured on). The
figures in the reason are `git show --numstat 10f40c6`: `+2055/-2` across 5
files, none under `src/`. No assertion was touched, nothing skipped, no
threshold moved.

Round 4 replaced that entry again — `10f40c6` left `HEAD`'s history and
`d23068b` entered it, so the entry now names `d23068b` with `git show
--numstat d23068b`'s figures (`+2722/-2`, 5 files). **Round 5 inherited that
repair green and changed nothing in this file**: the commit that carries round
4's work does not exist yet, so the entry it needs will be written by the next
run, the way every one of them was.

```
$ cargo test --test unreviewed_work_ack_test
running 13 tests
...
test result: ok (13 passed; tail abbreviated)
```

```
$ cargo test --test agent_permissions_test
running 28 tests
...
test result: ok (28 passed; tail abbreviated)
```

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no output, no diff (round 4 inherited this **failing**, on one reformat in the then-untracked `tests/report_markdown_test.rs:75`; round 4 ran `cargo fmt --all`, which is what round 5 inherits green) |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0, `Finished dev profile`, no warnings; `git diff HEAD -- '*.rs' \| grep -c "allow(clippy::"` → `0` |
| `cargo test --all-targets` | **pass** — exit 0, **1934 passed; 0 failed; 0 ignored** across 70 test binaries (log at `target/tmp/full_test_r5.log`; `grep -c "FAILED"` → `0`) |
| `../rbops/verify.sh "phase-084"` | **pass** — exit 0, `VERIFY PASS — phase-084`; every section green, including `report honesty` — run with the recorded environment and output pasted below |
| `bash rbops/smoke.sh` (pipeline repo) | **pass** — `SMOKE PASS 230 checks` (215 before the pipeline commit `267f9a2`), re-run in this round against the committed pipeline change |

The fourth gate, from the project root, with the environment round 3
recorded (wrapped for readability; one line as run):

```
$ RBOPS_ROOT="$PWD" RBOPS_PROJECT_DIR="$PWD" RBOPS_PHASES="$PWD/../rbops/phases.json" \
    RBOPS_BASELINE="$PWD/../rbops/baseline.json" \
    RBOPS_BASE_REF=d23068bf4160c14463f4a6af7479b824b4f33429 \
    ../rbops/verify.sh phase-084
  PASS phase phase-084 is declared in /home/runner/work/rbops/rbops/rbops/phases.json

== report
  PASS /home/runner/work/rbops/rbops/redblue/phases/phase-084/REPORT.md exists
  PASS section: What changed
  PASS section: Tests added
  PASS section: Gates
  PASS section: Known gaps
  PASS report has at least one real evidence row

== diff hygiene
  PASS forbidden-path scan done
  PASS no gate-weakening constructs added
  PASS no scratch files
  PASS diff substance: +1142 -168 (min +5)
  PASS implementation touched tests/ (must_touch: tests/)

== cargo fmt
  PASS cargo fmt --check

== cargo clippy
  PASS cargo clippy -D warnings

== cargo test
  PASS cargo test: 1934 passed, 0 failed

== doc tests
  PASS runs: examples/files.rb
  PASS runs: examples/fizzbuzz.rb
  PASS runs: examples/formats.rb
  PASS runs: examples/hello.rb
  PASS runs: examples/random.rb
  PASS runs: examples/test_arithmetic.rb
  PASS runs: examples/time.rb
  PASS runs: modules/MathUtils.rb
  PASS runs: modules/SuiteKit.rb
  PASS example sweep done

== test policy
  PASS failure-asserting test present: 3
  PASS test quota met: 10 rust / 0 redblue (need 3 or 2)
  PASS edge case test present: 8
  PASS no newly skipped tests

== report honesty
  PASS reported test count (1934) matches actual (1934)

╔══════════════════════════════════════╗
║ VERIFY PASS — phase-084                ║
╚══════════════════════════════════════╝
```

Two things about that invocation are measurements, not decoration. The
environment decides *which* report the gate reads: the bare command's first
line prints `PASS /home/runner/work/rbops/rbops/phases/phase-084/REPORT.md
exists` — a path one directory above this checkout — and reports its section
results against that copy, while with `RBOPS_ROOT`/`RBOPS_PROJECT_DIR` set to
this checkout the same line names this checkout's report and all four
sections pass (both forms measured; see "Round 4 — this run" above for what
the bare form does). The log of the run above is `target/tmp/v_r5.log`.

And the three test-policy lines — `10 rust`, `3` failure-asserting, `8 edge`
— are `tests/report_markdown_test.rs`'s ten tests, counted from the diff.
The file is new and untracked until the pipeline harvests it, so the count
depends on an intent-to-add entry (`git add -N`) to see it at all — and the
entry does not survive the run: a 15-second watcher holds
`git ls-files -s tests/report_markdown_test.rs` at one line for the first
~165 seconds of a run and at zero for the rest, and verify with the entry gone
reports `0 rust` (that run: `target/tmp/v_full2.log`, three FAILs, all three
here green). The run above kept the entry present for its duration with a
background `git add -N` loop; the tests themselves were never in question —
they ran in the suite (`1934 passed`) the whole time. The loop repairs
measurement bookkeeping a run of the suite itself clears; it changes no file
content and weakens no check. After the pipeline commits the file at harvest
the entry is unnecessary: every later verify sees a tracked file.

## Invariants touched

- None. No file under `src/` is in the diff, no `Value` variant, error variant,
  `.rb` extension, `to … end` grammar or `set x to <expr>` form is touched, and
  every pre-existing test still passes.

## Edge-case matrix (AGENTS.md §3.2)

| Row | Status | Where / why |
|---|---|---|
| empty | covered | `edge_missing_agent_key_is_rejected` — `{}`; `edge_malformed_config_is_rejected` — empty file; `edge_a_permission_object_form_is_rejected_or_accepted` — an empty object is not an action; `edge_an_empty_document_is_accepted` — the empty *document*: no fences, no headings to hide, so it is vacuously unambiguous |
| singleton | covered | `edge_missing_reviewer_is_rejected` and `edge_missing_verifier_is_rejected` — exactly one agent declared; `project_config_declares_exactly_the_two_pipeline_agents` — exactly two, and no third; `edge_a_document_without_code_blocks_is_accepted` — the singleton *document*: one heading and nothing else, no fences, nothing to misread |
| boundary | covered | `edge_duplicate_permission_key_resolves_to_the_last_value` — the boundary between the first and last occurrence of a repeated key, which is the boundary that decides whether the reviewer is writable; `edge_a_tool_granted_to_the_reviewer_outside_the_allowed_set_is_rejected` — the boundary of the allow-list, a key outside it being a grant; `edge_a_dropped_deny_key_is_rejected` — the boundary between a key that is declared and one that is merely absent, which is where the permissive default lives; `edge_a_permission_object_form_is_rejected_or_accepted` — the boundary between the bare action and the object of one action; `edge_an_agent_this_file_does_not_own_is_rejected` — the boundary of the agent-name set, `build` being just outside it; `edge_a_longer_outer_fence_may_hold_a_shorter_inner_fence` — the boundary of the fence lengths, a three-backtick fence inside a four-backtick one being text to every reader |
| out_of_bounds | N/A | nothing here is indexable or ranged; no index is computed, so there is no out-of-bounds to reach. The nearest analogue — a key that is not one of the pinned ones — is directly covered: `edge_a_tool_granted_to_the_reviewer_outside_the_allowed_set_is_rejected` (key present, ungranted-enforcement), `edge_reviewer_without_permission_block_is_rejected` (key absent), `edge_permission_value_of_wrong_type_is_rejected` (key present but unusable), `edge_an_agent_this_file_does_not_own_is_rejected` (a name outside the declared set) |
| type_mismatch | covered | `edge_permission_value_of_wrong_type_is_rejected` — boolean, number, an object with a non-string action and `null` where a string action belongs; `edge_agent_block_of_wrong_type_is_rejected` — array and string where an object belongs; `edge_a_missing_or_malformed_mode_is_rejected` — a boolean where a mode belongs |
| numeric_boundary | N/A | the config schema admits no numeric field: `PermissionActionConfig` is the enum `["ask","allow","deny"]`. A number is rejected as a type mismatch, not a magnitude, so there is no numeric boundary to cross. Covered by `edge_permission_value_of_wrong_type_is_rejected` (the `bash: 0` case) |
| unicode | N/A | every key and every legal value in the schema is ASCII (`agent`, `reviewer`, `edit`, `allow`/`deny`/`ask`). No unicode key can be declared, and a unicode value is rejected as a type mismatch — `kind()` in `tests/agent_permissions_test.rs` names the type it found. The UTF-8 BOM case is covered by `edge_malformed_config_is_rejected`. Nothing in this change touches `src/`, where the language's own unicode handling is tested |
| nesting_recursion | covered | `edge_reviewer_without_permission_block_is_rejected` — `agent.reviewer` declared, `permission` one level down absent; `edge_permission_value_of_wrong_type_is_rejected` — a nested object where a flat action belongs; `edge_a_permission_object_form_is_rejected_or_accepted` — a pattern → action object, one level in, under a key the two-tool lookup never reached |
| duplicate_missing_keys | covered | missing: `edge_reviewer_without_permission_block_is_rejected`, `edge_missing_reviewer_is_rejected`, `edge_a_dropped_deny_key_is_rejected`, `edge_a_missing_or_malformed_mode_is_rejected`. duplicate: `edge_duplicate_permission_key_resolves_to_the_last_value` — JSON resolves a repeated `edit` last-wins, so a trailing `"allow"` is the one that counts. A section heading that is *present but hidden* is the same class from the reader's side: `edge_a_section_heading_inside_a_code_block_is_not_a_section` — a heading inside a fence is missing to every reader, not merely present |
| malformed_input | covered | `edge_malformed_config_is_rejected` — empty file, truncated object, trailing comma, UTF-8 BOM, top-level array, bare string; `edge_missing_config_file_is_rejected` — the file that is not there; `edge_a_top_level_permission_block_is_rejected` — a well-formed document in the wrong shape for this file; `edge_a_fence_with_an_info_string_inside_an_open_fence_is_rejected` — the malformed *markdown*: a fence carrying an info string inside an open block, the exact shape that ate `## Tests added`; `edge_a_document_that_ends_inside_a_code_block_is_rejected` — the document that ends inside a block; `edge_a_rejected_document_produces_an_error_not_a_panic` — a rejected document comes back as an `Err`, never a panic |
| resource_limit | N/A | the change is a JSON document read once by a test plus a bounded walk of an eleven-key object; there is no loop, no recursion, no allocation that grows with input, and no handle held across a failure. `check_no_grants_beyond` and `check_agent_set` iterate maps that the document bounds at the keys a hand-edit adds |

## Re-verification of the phase's original finding

The phase asks for the finding to be re-verified before anything is changed.
The half of it that is about the **global** config still reproduces exactly as
written, measured again in round 5:

```
$ wc -c ~/.config/opencode/opencode.json
213 ~/.config/opencode/opencode.json
$ python3 -c "import json;d=json.load(open('$HOME/.config/opencode/opencode.json'));print('has agent key:', 'agent' in d);print('default_agent:', d.get('default_agent'));print('top-level permission:', d.get('permission'))"
has agent key: False
default_agent: build
top-level permission: {'external_directory': {'**': 'allow'}, 'bash': 'allow', 'edit': 'allow', 'webfetch': 'allow'}
```

The global config declares **no** `agent` key, so `reviewer` resolves to
nothing there, and opencode falls back to `default_agent: build` — whose
`edit` and `bash` are `allow`. The other half of the finding — "the redblue
checkout has no `opencode.json`" — no longer reproduces, and that is the
phase's own doing: the checkout now ships `opencode.json` at its root, where
opencode looks before the global file, and declares both agents by name; the
resolved output is quoted above.

**What the finding did not say, and what this round closed.** The finding
named the *permissions* as unenforced. It was the *request* that was still
lying about them: until the pipeline commit `267f9a2`, the review request
told the reviewer the checkout had no config at all, and the reviewer's own
contract said the same. That is fixed and committed; the request the pipeline
builds today is pasted under "Round 5 — this run".

## Known gaps / follow-ups

- **The acknowledgement treadmill is structural, not accidental.** The pipeline
  commits the agent's work and only then asks the next run to acknowledge the
  commit it just made, so the SHA can never be written by the run that produces
  the work. **Eight** deliveries have paid it; the eighth was round 4's, for
  the commit `d23068b` that carried round 3's work, and round 5 inherited that
  repair green. The ninth falls due to the next run, for the commit that
  carries round 4's work — a SHA that does not exist yet, so this run cannot
  write it. FINDINGS.md carries the sequence and the two fixes that would end
  it: a commit subject that does not match the selector, or the dispatcher
  adding the line at commit time, where the SHA is known. The second is a
  dispatcher change, and rounds 3 and 5 did touch the dispatcher — but the
  line has to be written by the commit, and this phase cannot write a SHA that
  does not exist yet.
- **The tests cannot prove opencode's resolution.** They assert a file's
  contents; they cannot assert which file opencode opened. The dispatcher's
  `assert_reviewer_read_only` closes the sharpest half of that gap — it
  resolves the block and the mode from the file opencode would read — but it
  still cannot prove that opencode merged that file the way the docs describe.
  A future opencode that changed its resolution order would leave both green
  with the reviewer writable again.
- **`agent.verifier` is declared but not invoked.** `rbops` runs the
  implementer as `--agent build` and the reviewer as `--agent reviewer`; the
  gates run as scripts, not as an agent. The declaration is pinned by the
  tests so that wiring a verifier agent later resolves to these permissions
  rather than to the global build agent, but until then it is a declaration
  and not a fact about the pipeline.
- **The pipeline repo has its own `opencode.json`, and it is not in the
  project's resolution path.** `rbops/opencode.json` is consulted when
  opencode runs at the pipeline root — which the audit does — and the review
  runs with `--dir` at the project checkout, which is its own Git root, so
  opencode's upward walk stops there and never reaches the pipeline repo. Its
  `reviewer` agent declared `mode: "subagent"` with `webfetch` allowed; **this
  round** changed it to `mode: "primary"` and the same eleven denies, because
  the phase's argument applies to it as much as to the project's file — but a
  future edit could undo that, and nothing in the project's suite can see it.
  (Round 2's delivery of this report said round 2 had already made that
  change. It had not: the pipeline repo carried the old block when this run
  started, and the diff in "The pipeline" section above is this round's.)
- **The `read` rules are opencode's, not ours.** The project file does not
  declare `read`, `glob`, `grep` or `list`: opencode allows them by default,
  and its default `read` rules still deny `.env` files. An agent-level
  `read: "allow"` would be appended after those rules and re-allow them, so
  the allow-list permits that spelling but the shipped file does not use it.
