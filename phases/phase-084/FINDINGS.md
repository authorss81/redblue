# Phase 084 — findings for the auditor

## What this delivery changed, in one place

The phase's subject is the reviewer's permissions. The first delivery shipped
`opencode.json` at the project root with `edit` and `bash` denied, and a test
file that asserted the file's contents. The review of that delivery returned
eleven findings; round 2 fixed all eleven. The second review of that round
returned seven more; round 2 fixed all seven too. The third review returned
eight more, and **this round** fixed all eight: the first two were about the
pipeline repo, which did not carry the fixes round 2's report claimed it had;
the rest were about this document's own numbers, citations and claims. They
are the last section of this file, and the report has the same table.

The five that are closed, in the order the first review listed them:

| # | Finding | Fixed by |
|---|---|---|
| 1 | Only `edit` and `bash` were declared, so `task`, `webfetch` and `external_directory` were left to opencode's permissive default | `opencode.json` declares every widening key as `deny`; `project_config_denies_every_widening_permission_for_the_reviewer` asserts each one is present |
| 4 | `cmd_review` re-derived the diff base from `HEAD`, so every review of a committed phase was handed an empty change set | `cmd_run` records the pre-phase base in `phases/<phase>/.base`; `cmd_review` reads it back. **Re-done in round 3**: the pipeline repo did not carry it when this run started |
| 5 | The review preamble told the reviewer the checkout has no `opencode.json`, and nothing asserted the resolved permissions | The preamble states what is true; `assert_reviewer_read_only` resolves `agent.reviewer.permission` and refuses the review when it does not say `deny`. **Re-done in round 3**, for the same reason as the second review's finding 3 |
| 7 | "Only one project config may be shipped" was asserted only for `opencode.json` | `project_config_ships_in_exactly_one_form` asserts the `.jsonc` variants absent too |
| 11 | `tests/unreviewed_work_ack_test.rs` still opened with a stale placeholder marker | Removed; the file says what it is |

The seven that the second review returned, and what each one got:

| # | Finding | Fixed by |
|---|---|---|
| 1 | `check_document` never inspected `agent.*.mode`, so the one field that decides whether `--agent reviewer` binds to the read-only agent was unpinned — the phase argued `mode` was load-bearing but closed the gap by argument, not by assertion | `check_mode` requires `mode: "primary"` for both agents and is called from `check_document`; `project_config_declares_primary_mode_for_both_agents` asserts the shipped file; `edge_reviewer_mode_subagent_is_rejected` mutates the shipped mode to `subagent`, `all` and `nonsense` in turn and requires each to fail |
| 2 | The deny set covered 5 of opencode's permission keys; `websearch` and `skill` were undeclared and therefore allowed, while the report asserted the opposite | The shipped file now denies every widening key (11 of them); `REVIEWER_DENIED` covers them; `check_no_grants_beyond` is an allow-list — `read`, `glob`, `grep` and `list` may be allowed, everything else must deny |
| 3 | The report said the dispatch preamble no longer claimed the checkout had no `opencode.json`, but the request handed to the reviewer still said so | The preamble in `rbops/dispatch.sh` was corrected, the audit contract's header said the same thing and was corrected too, and the pipeline's own smoke suite now ASSERTS the request: it must name the shipped config, must not claim there is none, and must name the denied keys. **Re-done in round 3**: when this run started the pipeline repo still carried the stale claim in three files, and this round's review request carried it verbatim |
| 4 | Three irreconcilable accountings of one change set | Every number in the report now names the commit it is measured against, and both numstats are pasted in full |
| 5 | The pasted `verify.sh` output was not the output for the tree being shipped | The gate was re-run after the last edit to these documents and the fresh output is pasted, with one sentence about the drift that pasting it creates |
| 6 | The "a third tool granted to the reviewer" case listed `write`, which is not an opencode permission key | The case now uses opencode's own keys: `webfetch`, `websearch`, `skill`, `lsp` and `todowrite` — plus `*`, which is **not** a schema key either but is kept on purpose: it pins the allow-list's own edge, that the list is closed, so a key opencode does not even have is refused |
| 7 | The checks required every permission value to be a bare string, though opencode accepts the object form for most keys | `permission_action` reads both shapes, and only for the keys opencode's schema types as rules; `edge_a_permission_object_form_is_rejected_or_accepted` covers both branches |

The three that were about this phase's own paperwork — the impossible test
count, the citation to a section that does not exist, and the "mode is
unconfirmed" gap — are the three sections immediately below.

## The resolution rule is quoted from a source that can be located

The first delivery of this report quoted a "Where files live" table and
attributed it to opencode's configuration reference, "published alongside the
JSON Schema at <https://opencode.ai/config.json>". That URL serves a JSON
Schema: no prose, no sections, no table. The rule the whole fix rests on was
therefore cited to a place where it cannot be found.

The rule is real; it lives in two places that can be opened:

- opencode's configuration reference (<https://opencode.ai/docs/config>,
  section **"Locations"**): *"Configuration files are merged together, not
  replaced. Settings from the following config locations are combined. Later
  configs override earlier ones only for conflicting keys. Non-conflicting
  settings from all configs are preserved."* and, under **"Per project"**:
  *"When OpenCode starts up, it first looks for a config file in the current
  directory, then traverses up to the nearest Git directory."*
- the `customize-opencode` skill opencode ships inside its own binary
  (section **"Where files live"**), which carries the table itself:
  *"Project config | `./opencode.json`, `./opencode.jsonc`, or
  `.opencode/opencode.json` (opencode walks up from the cwd to the worktree
  root)"*, *"Global config | `~/.config/opencode/opencode.json` or
  `~/.config/opencode/opencode.jsonc`"*, and *"Configs from each scope are
  deep-merged. Project overrides global."*

The second source is also where the `.jsonc` variants come from, which is why
the test file now asserts that none of them is shipped.

## "opencode hard-fails to start on a config that fails this schema" — measured, not assumed

The first delivery asserted this twice (here and in `REPORT.md`) on the
strength of the documentation, which does not say it: the configuration
reference presents the schema as editor support — *"Your editor should be able
to validate and autocomplete based on the schema"* — and documents no startup
validation against it.

The claim was settled by measurement instead. An invalid config was written on
purpose and opencode was run in that directory:

```
$ mkdir -p /tmp/badcfg && cd /tmp/badcfg
$ printf '%s\n' '{"$schema":"https://opencode.ai/config.json","agent":{"reviewer":{"mode":"nonsense"}}}' > opencode.json
$ opencode debug config
Error: Configuration is invalid at /tmp/badcfg/opencode.json
↳ Expected "subagent" | "primary" | "all", got "nonsense" agent.reviewer.mode
$ echo $?
1
```

So opencode does refuse to start on a config whose **values** are wrong, and
it names the offending value. What it does *not* do on this build is reject
an unknown top-level key: the same build accepted `{"totally_unknown_key":
true}` with exit 0, even though the shipped skill says *"Unknown top-level
keys in `opencode.json` are rejected with `ConfigInvalidError`"*. So the
honest statement is: **value validation is enforced at startup; total schema
conformance is not.** The shipped file is schema-valid regardless
(`jsonschema` 4.10.3: **SCHEMA VALID**), so nothing about this phase's file
depends on the difference.

## `mode: "primary"` is confirmed, asserted, and load-bearing

The first delivery filed this as an open gap: *"`mode: "primary"` is
unconfirmed … `rbops/` is not in this checkout and the phase brief forbids
inspecting the pipeline, so the invocation could not be verified here."* and
suggested `mode: "all"` as a hedge.

`rbops/` is one directory up, and the invocation is visible:

```
$ grep -n -- "--agent" ../rbops/dispatch.sh
797:    --dir "$PROJECT_DIR" --agent build --title "rbops-${phase}"
1070:        --dir "$PROJECT_DIR" --agent reviewer --title "rbops-review-${phase}-${round}"
1186:        --dir "$PROJECT_DIR" --agent build --title "rbops-fix-${phase}-${round}"
```

`--agent reviewer` on the command line names a **top-level** agent, not a
subagent — opencode's agents reference reserves `@`-mentioning for subagents
and says a `default_agent` "must point to a non-hidden, primary-mode agent".
The gap is closed and the one-word change is not made.

The second review then pointed at the other half of that sentence: the mode is
not a formality, and nothing pinned it. opencode's `run` path resolves the
named agent and, when it is missing or its mode is `subagent`, prints

```
agent "reviewer" is a subagent, not a primary agent. Falling back to default agent
```

and runs the **default** agent — which here is `build`, whose `edit` and
`bash` are `allow`. The permission block would have been read and never
applied, and the phase's own argument is that the mode is load-bearing. So it
is now pinned by assertion and not by argument:

- `check_mode` requires `mode: "primary"` for `reviewer` and `verifier`, and
  `check_document` calls it;
- `project_config_declares_primary_mode_for_both_agents` asserts the shipped
  file;
- `edge_reviewer_mode_subagent_is_rejected` mutates the shipped mode to
  `subagent`, `all` and `nonsense` in turn and requires each to be rejected.

The dispatcher's half is closed too: `assert_reviewer_read_only` now resolves
`agent.reviewer.mode` out of the same file it resolves the permissions from,
and refuses the review when the mode is not `primary`. A reviewer that would
never bind is a review that would run as the writable build agent, so the
dispatcher stops it before it starts.

The pipeline's own `rbops/opencode.json` declared its `reviewer` agent with
mode `subagent` and `webfetch` allowed. That file is not in the project's
resolution path — the review runs with `--dir` at the project checkout, which
is its own Git root — but it is the same footgun in the same repository, and
the phase's argument applies to it as much as to the project's file, so
**round 3** changed it to declare `mode: "primary"` and the same eleven
denies. (Round 2's delivery of this findings file said round 2 had already
made that change. It had not — the pipeline repo carried the old block when
this run started.)

The same grep is the reason the test module's doc comment was wrong: there is
no `--agent verifier` anywhere in `rbops/`. The gates run as scripts
(`verify.sh`), not as an agent. `agent.verifier` is declared in
`opencode.json` so that a dispatcher which *does* run the gates as an agent
resolves to those permissions rather than to the global build agent, and the
test file pins the declaration — but the doc no longer claims the pipeline
launches it.

## The reviewer is now denied every key that could widen it

The first delivery declared `edit` and `bash` and said the rest was "a phase
of its own", on the grounds that *"the reviewer is granted only what is
named"*. Two things were wrong with that:

1. **`write` is not a permission key.** opencode's permissions reference
   lists `edit` as covering `write`, `edit` and `apply_patch`; the opencode
   source maps the three tool names onto the `edit` key. So `edit: "deny"`
   already denies the write tool, and naming `write` as a gap named a key that
   does not exist.
2. **The keys that were genuinely open are now closed.** The second review
   found that the deny set covered five of opencode's permission keys, and
   that `websearch` and `skill` were among the ones left out — so the
   reviewer still had web search and skill loading while the report asserted
   the opposite.

The published schema is the authority on which keys exist. `AgentConfig.permission`
is typed with fifteen of them:

| key | schema type | reviewer's value | why |
|---|---|---|---|
| `read` | action or object | *not declared* | opencode allows it by default, and its default rules still deny `.env` files — declaring `read: "allow"` would append a later rule and re-allow them |
| `glob` | action or object | *not declared* | allowed by default |
| `grep` | action or object | *not declared* | allowed by default |
| `list` | action or object | *not declared* | allowed by default |
| `edit` | action or object | `deny` | covers `write`, `edit` and `apply_patch` |
| `bash` | action or object | `deny` | the shell the reviewer has no business running |
| `task` | action or object | `deny` | launching subagents — the built-in `general` subagent "has full tool access" |
| `external_directory` | action or object | `deny` | the global config grants it as `{"**": "allow"}` |
| `skill` | action or object | `deny` | loads a skill file into the reviewer's context — instructions the reviewer then follows, fetched from disk or from a URL |
| `lsp` | action or object | `deny` | language-server queries: no use, no reason to hold |
| `todowrite` | action only | `deny` | writes to the session, not the tree — but it is still a grant |
| `question` | action only | `deny` | asks a human; a pipeline review has no human, and the answer is the only way to widen the reviewer |
| `webfetch` | action only | `deny` | the global config grants it |
| `websearch` | action only | `deny` | the web, without a URL |
| `doom_loop` | action only | `deny` | opencode's repeated-call guard; it defaults to `ask`, and the pipeline runs `opencode run` with nobody to answer a prompt, so denying it makes the guard deterministic instead of a question nobody can answer |

"Everything else denied" is now the rule the checker enforces, and it is an
**allow-list**: `read`, `glob`, `grep` and `list` may be allowed and every
other key must deny. The previous check was the mirror image of that — it
rejected `read: "allow"`, which forbade the one shape that closes this class
of hole for good — and it policed only the keys that were *present*, so an
undeclared key passed it while its own doc comment claimed "The reviewer is
read-only in full". The other half of the contract is
`project_config_denies_every_widening_permission_for_the_reviewer`, and the
edge case that holds the *absence* in place is `edge_a_dropped_deny_key_is_rejected`:
deleting one of the eleven keys is a deleted deny, because opencode's defaults
are permissive and an undeclared key is an allowed key.

**The object form is read, not assumed away.** opencode accepts two shapes
for a permission value: a bare action (`"deny"`) and an object of pattern →
action (`{"**": "deny"}`), and the schema types ten of the fifteen keys as
accepting either. A deny spelled as an object is the same deny, so
`permission_action` reads both and `edge_a_permission_object_form_is_rejected_or_accepted`
covers both branches: the object form of a deny is accepted, the object form
of an allow is rejected, an object that mixes the two is rejected as not
being one action, and an object on one of the five "action only" keys is
rejected because opencode cannot apply it. The dispatcher's
`assert_reviewer_read_only` reads the same two shapes.

The remaining half of the contract — that opencode resolved the file at all —
is now checked by the dispatcher rather than by a test:
`assert_reviewer_read_only` (`../rbops/dispatch.sh`) reads
`agent.reviewer.permission` and `agent.reviewer.mode` out of the project's
`opencode.json` and refuses to start a review that would not be read-only.

## The reviewer was being handed an empty change set

Not a permissions finding, but the review of this phase found it and it
invalidates every review this pipeline has run: `cmd_review` computed
`base="$(in_project git rev-parse HEAD)"` **at review time**, after the
pipeline had committed the phase's work. `git diff "$base"..HEAD` is
therefore empty for every phase whose work was committed, and the review
request's file list and diff sections were both blank.

Measured on the smoke suite's fixture, before and after — the same fixture,
the same run, the only difference being which base the review uses:

```
# before — the committed section is empty, so the reviewer is handed nothing
### File list (always complete — content below may be capped)

```
19	0	phases/phase-001/REPORT.md
16	0	src/vm.rs
```

## 1. Committed changes (7180e154491812e5b0cab6f8751a1cf08fa82100..HEAD)

```diff
```

# after — the recovered work is in the diff
### File list (always complete — content below may be capped)

```
6	0	src/vm.rs
5	0	target/debug/rb
11	0	tests/resumed_probe.rs
19	0	phases/phase-001/REPORT.md
16	0	src/vm.rs
```

## 1. Committed changes (b5c5d69aa68f6f456a282b3a8bb402435a7059b3..HEAD)

```diff
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
```

(The SHAs are the fixture's own — a fresh temporary clone per run.)

`cmd_run` already captured the right base — it captures it *before* any
resume merge, "so the gate judges the phase's END STATE" — and then threw it
away at the end of the function. It is now recorded in `phases/<phase>/.base`
and read back by `cmd_review`, which runs in a separate process from the run.
The base is committed with the rest of the pipeline state, because the review
happens in a different job.

## The pipeline's merge of the parked work left the acknowledgement list conflicted, and the parked entry was stale

This delivery's tree was delivered as an unresolved merge: `main` at `e0a3686`
(`rbops: phase-061`) with the parked phase-084 branch squashed in. The
acknowledgement list `tests/unreviewed_work_ack_test.rs` existed on **both**
sides — added on `main` by `222aa83` and re-added by the parked branch — so it
came back with conflict markers in four places, and the index held an empty
blob for it.

The two sides were not two versions of the same data:

| side | what its acknowledgement list held |
|---|---|
| `main` (`e0a3686`) | 31 entries, one for every commit in `git log HEAD` whose subject matches the selector |
| parked branch (`c1ebb59`) | 23 entries — the same list minus the **nine** commits `main` gained since, plus one entry naming `48d5bf0…` |

The nine entries `main` had gained and the parked branch had never seen are
the deliveries for phases 054, 056, 058, 086, 087, 089, 059 and 061 and the
`ack: resume phase-088` commit. Those are kept.

The parked branch's extra entry named `48d5bf0…`, the SHA of the commit that
carried the *previous* delivery of this phase. That commit lives on
`origin/rbops-wip/phase-084` and is **not** an ancestor of `main`:

```
$ git merge-base --is-ancestor 48d5bf0183fac60b08e50633aab0aee1b8e846d9 HEAD
$ echo $?
1
```

`scan` reports an acknowledgement naming a commit this history does not
contain as *stale*, and a stale entry is a failure — so keeping that line would
have made `cargo test` red for a reason unrelated to this phase. It was
dropped rather than repointed, because the commit that will carry *this* run's
work does not exist yet: the pipeline creates it after this run, so its SHA
cannot be known by the run that produces the work. The list is now exactly the
set of matching commits in `git log HEAD`, which is what the audit requires in
both directions:

```
$ grep -oE '\("[0-9a-f]{40}"' tests/unreviewed_work_ack_test.rs | tr -d '("' | sort -u > ack.txt
$ git log --format='%H%x09%s' HEAD \
    | awk -F'\t' '{s=tolower($2); if (index(s,"checkpoint")||index(s,"resume phase-")) print $1}' \
    | sort -u > head.txt
$ diff ack.txt head.txt && echo IDENTICAL
IDENTICAL
```

Nothing is lost: `48d5bf0…` and `c1ebb59…` are still on their own branches
(`origin/rbops-wip/phase-084`, `origin/rbops-recovery/phase-084`), and
`48d5bf0…` is acknowledged in that branch's own copy of the file.

## Every resume commit costs one line in `ACKNOWLEDGED`

`tests/unreviewed_work_ack_test.rs` fails on any commit in `HEAD`'s history
whose subject contains `resume phase-` or `checkpoint` and that is not listed in
its `ACKNOWLEDGED` data. **This has now cost four one-line repairs across four
deliveries of this phase**, one per run, and the cost is structural rather than
accidental. Each delivery's resume commit has a different SHA, the test rejects
both unlisted *and* stale entries, so exactly one line must be rewritten every
time and it can only be rewritten by whoever knows the SHA in advance — which is
the dispatcher, not the phase.

The sequence, recorded so the pattern is visible:

| delivery | commit carrying this phase's work | what the entry had to become |
|---|---|---|
| 1 | `55f71878d905105dfbdd715365f3f74aa6d88bb5` | first recorded |
| 2 | `d1f21633394cc27b5bcd35ec73bc4a14a16e5b3f` | repointed; `55f71878` went stale |
| 3 | `ffa1f29ab36c05f452eca84b69f93b039c295abd` | repointed; `d1f21633` went stale |
| 4 | `05c77d1a0e2860b1970b4cee93577fefd462aa02` | repointed again; `ffa1f29` went stale |
| 5 | `48d5bf0183fac60b08e50633aab0aee1b8e846d9` | repointed again; `05c77d1` went stale |
| 6 | `c1ebb59e10eb1da8f9ec03e9c1a7aea4c7326f9a` (the parked branch this delivery inherited) | **dropped, not repointed**: the entry it carried named `48d5bf0`, which is not in this history, and the commit that will carry this run's work does not exist yet |

At every delivery `cargo test` was red *before any phase code ran*: once because
the new SHA was unlisted, and (from delivery 2 on) simultaneously because the
previous SHA was stale. Through delivery 5 the repair was to repoint the entry
at the SHA this history actually contains, measured with
`git merge-base --is-ancestor <sha> HEAD`. Delivery 6 could not use that
repair: the history this delivery runs against is `main` at `e0a3686`, which
contains **no** phase-084 commit at all, and the commit that will carry this
run's work is created by the pipeline *after* this run. There is therefore no
SHA that can be written, and the stale line had to be dropped instead — which
is the same defect as before, only now visible in the opposite direction:
the pipeline commits the agent's work and then asks the next run to acknowledge
the commit it just made.

Delivery 6 is the sixth consecutive observation of the same defect: this is a
measured pattern, not a prediction. This round is the seventh delivery and it
did **not** have to pay it — the list it inherited was already exactly the
set of matching commits in the history it ran against, because delivery 6 had
dropped the stale line instead of repointing it. The next delivery that runs
against a history containing this round's commit will have to pay it again,
unless the line is written at commit time.

**The fix is not another one-line repair.** Either the resume commit's subject
should avoid the `resume phase-` / `checkpoint` shapes the selector matches, or
the dispatcher should add the `ACKNOWLEDGED` line at commit time, where the SHA
is known. The second is a dispatcher change, and this round did touch the
dispatcher — but a line written at commit time is written by the commit, and
the run that produces the work still cannot know the SHA its own commit will
get. The repair is therefore still repeated by hand, and flagged. Evidence: the
module docs at `tests/unreviewed_work_ack_test.rs:1`, the list at
`tests/unreviewed_work_ack_test.rs:72`, the selector
`is_unreviewed_subject`, and the assertion that compares the two sets.

## The scratch directory's mode can present as six red tests in a file this phase does not own

The first `cargo test --all-targets` of the previous run reported 6 failures in
`src/repl/history.rs`, every one of them `Os { code: 13, kind: PermissionDenied }`
on the scratch path. The cause was the mode of `target/tmp/repl-history`
itself, which was on disk as `drw-------` — writable and traversable, **not
readable** — before the run started.

`fs::create_dir_all` at `src/repl/history.rs:136` creates that directory `0755`,
and nothing in `src/` or `tests/` ever sets permissions on it:
`grep -rn 'set_permissions\|from_mode' src/ tests/` matches exactly one line,
`tests/stdlib_modules_test.rs:451`, an unrelated `0o600` on its own fixture. So
the mode is an environment artefact, not a code defect. `chmod 700` on that
gitignored directory makes the binary report `18 passed; 0 failed`.

No test was changed, skipped or weakened to get here, and this cannot reproduce
in CI, where the directory is created by `create_dir_all`. It is recorded because
it presents as six red tests pointing at a file this phase did not touch, and
the next implementer will lose time the same way. A harness that seeds `target/tmp`
should create scratch directories readable, or the unit tests should assert the
directory's mode before using it.

## A transient shallow-history reading that was not a defect

`edge_phase_078_commits_are_ancestors_of_head_not_absent`
(`tests/unreviewed_work_ack_test.rs:571`) failed on that same first run with
"history is shallow and completing the clone did not make it complete". It passed
on every subsequent run, in isolation and in the full suite, and the repository
is not shallow:

```
$ git rev-parse --is-shallow-repository
false
$ ls .git/shallow
ls: cannot access '.git/shallow': No such file or directory
```

`is_shallow()` at `tests/unreviewed_work_ack_test.rs:296` shells out to that
exact command, so the only reading consistent with the panic is a moment when
the answer was `true`. Recorded because it is what the first output said, and
because "the audit refused to measure a truncated history" is a message that
should not be dismissed without saying so.

The test also rejects *stale* entries, so the next one cannot be pre-registered:
the dispatcher, not the phase, is the component that can add the line, because
only it knows the SHA the commit will get. If a phase is gated on `cargo test`
after its own commit lands, either the commit subject avoids `resume phase-` /
`checkpoint`, or `ACKNOWLEDGED` gains that SHA's line first.

## Verification note for whoever runs `rbops/verify.sh`

`rbops/verify.sh` is one directory up from the project checkout, not inside
it: from the project root it is `../rbops/verify.sh`, and it is run as

```
RBOPS_PROJECT_DIR="$PWD" RBOPS_PHASES="$PWD/../rbops/phases.json" \
  ../rbops/verify.sh phase-084
```

An earlier delivery of this file said the gate "does not exist in this
checkout" and reported it as NOT RUN. It does exist, and it passed this run.

The shipped `opencode.json` was validated against the live published schema at
<https://opencode.ai/config.json> using `jsonschema` 4.10.3 (Draft 2020-12,
with the remote `models.dev` `$ref` this document does not reach resolved from
<https://models.dev/model-schema.json>): **SCHEMA VALID**. Re-run this
session against the file as it now stands, same result. The schema confirms
the claims the fix rests on — `Config.agent.additionalProperties` is
`AgentConfig` (arbitrary agent names legal), `AgentConfig.mode` enumerates
`["subagent","primary","all"]`, and `AgentConfig.permission` is typed with
fifteen keys: `read`, `edit`, `glob`, `grep`, `list`, `bash`, `task`,
`external_directory`, `todowrite`, `question`, `webfetch`, `websearch`, `lsp`,
`doom_loop` and `skill`. Ten of them are typed `PermissionRuleConfig` (a bare
action **or** an object of pattern → action) and five — `todowrite`,
`question`, `webfetch`, `websearch` and `doom_loop` — are typed
`PermissionActionConfig`, a bare action and nothing else. That split is why
the checker accepts the object form for some keys and refuses it for the
others.

## Round 3: two ways the shipped file could reach past the agent it is about

The suite that round 2 left behind was strong about the *reviewer's*
permissions and weak about the file's *reach*. opencode deep-merges the
project config over the global one, per key, so two shapes of a project config
move a permission for an agent the file does not declare — and neither was
pinned by any assertion:

| # | Hole | Now |
|---|---|---|
| 1 | A **top-level `permission` block** in the project config applies to every agent that declares no block of its own — including `build`, the agent every phase's implementer runs as. The shipped file has none, and the report said so, but nothing asserted it: `check_document` only refused a top-level `default_agent`. | `check_document` refuses a top-level `permission` block, and `project_config_declares_no_top_level_permission_block` asserts the shipped file has none. `edge_a_top_level_permission_block_is_rejected` drives the rule. |
| 2 | An **agent this file does not own** — `build` above all — would replace the global block for that name, because the project config is consulted first. The report claimed "the file declares no `build` agent", and nothing asserted it: `check_document` looked up `reviewer` and `verifier` and never looked at the *set* of names. | `check_agent_set` refuses any agent outside `{reviewer, verifier}`, and `project_config_declares_exactly_the_two_pipeline_agents` asserts the shipped map holds exactly those two. `edge_an_agent_this_file_does_not_own_is_rejected` drives the rule with `build` and `auditor`. |

A third, smaller, hole was closed at the same time: the shipped file names
the schema it was validated against (`$schema`), and nothing asserted it —
`project_config_declares_the_schema_it_is_validated_against` does now. The
URL is the one the file was checked against in the section above; a change to
it is a change to the schema the file claims to satisfy, and it should fail
loudly rather than silently stop being the schema.

Measured, by mutating the shipped file and re-running the suite:

```
$ # a `build` agent added
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

The file was restored from `target/tmp/opencode.json.bak` and confirmed
byte-for-byte (`cmp` → identical, `git diff -- opencode.json` → empty)
before the gate run.

## Round 3: the acknowledgement sequence, seventh time

| delivery | commit carrying this phase's work | what the entry had to become |
|---|---|---|
| 7 | `10f40c6a437ea44032e639616e40ff7bd1027a3f` | **added**: the list this run inherited named every matching commit in the history except this one, which is the commit that carried round 2's work |

The repair is the same one line as before, and the same reason it is
structural: the pipeline commits the agent's work and only then asks the next
run to acknowledge the commit it just made, so the SHA cannot be written by
the run that produces the work. The list is again exactly the set of matching
commits in `git log HEAD`, measured both ways:

```
$ grep -oE '\("[0-9a-f]{40}"' tests/unreviewed_work_ack_test.rs | tr -d '("' | sort -u > ack.txt
$ git log --format='%H%x09%s' HEAD \
    | awk -F'\t' '{s=tolower($2); if (index(s,"checkpoint")||index(s,"resume phase-")) print $1}' \
    | sort -u > head.txt
$ diff ack.txt head.txt && echo IDENTICAL
IDENTICAL
```

## Round 3: the shallow-history reading, again

The first run of `unreviewed_work_ack_test` in this round failed with

```
history is shallow and completing the clone did not make it complete: the
audit refuses to read a truncated `git log HEAD`, because a truncated log
reads as a clean one. Run `git fetch --unshallow` first.
```

and every run after it passed, in isolation and in the full suite. The
repository is not shallow:

```
$ git rev-parse --is-shallow-repository
false
$ ls .git/shallow
ls: cannot access '.git/shallow': No such file or directory
```

`is_shallow()` at `tests/unreviewed_work_ack_test.rs` shells out to exactly
that command, so the only reading consistent with the panic is a moment when
the answer was `true`. Round 2 recorded the same thing and it is recorded
again here, because the message is one a reader should not dismiss without
saying so. Nothing in the phase changed the repository's history, and the
full suite is green with 1924 passing tests.

## Round 3: the third review's eight findings

The third review's findings were all about **claims** — about this file, about
`REPORT.md`, and about the pipeline repo. Two of them were the serious kind:
the report said the pipeline's half of the contract was shipped, and it was
not. The pipeline repo is a separate repository, and the round-2 changes to it
were not in it when this run started.

| # | Finding | What this round did |
|---|---|---|
| 1 | The report claimed the stale "the checkout has no `opencode.json`" statement had been removed from the review request and was gated by the pipeline's smoke suite. The request handed to that reviewer still carried it, verbatim, and named no shipped config and no denied keys | Found the claim where it actually lived: `grep -rn "has no opencode.json" rbops/` matches `rbops/agents/reviewer.md`, `rbops/agents/verifier.md` and `rbops/agents/auditor.md`, and the dispatch preamble said the same thing. All four are corrected. The pipeline's own `reviewer` agent block is now `mode: "primary"` with the eleven denies. The smoke suite asserts the request's text — it must name the shipped config, must not claim there is none, must name the denied keys — and each of those assertions was checked against the old text, where it fails. The three assertions were also run against the **real** request this checkout produces, and it passes them. |
| 2 | The pasted `git diff 10f40c6 --numstat` block and the `diff substance` figure the gate reports disagreed by one line | The block is re-pasted from the final tree and the figure refreshed to the same sum. The report says, once, that pasting the number moves it, and that is still true. |
| 3 | The acknowledgement entry's `+2055/-2, 5 files` could not be reconciled with the report's two numstats | The entry is right: `git show --numstat 10f40c6` gives exactly that. The report's *bases* section was the thing that was wrong — it did not say that `10f40c6`'s first parent is `b21821d`, so `git show` (which measures against the parent) and a diff against `64390ff` looked like two accountings of one change set. The bases section now names `b21821d` and says what each of the five commits measures. |
| 4 | The report said this round changed the pipeline's own `opencode.json` in one place and that this round changed nothing outside this repository in another | **This round changed the pipeline repo**, and both places now say so. The pipeline section carries the real diff, and the round-2 review records say the pipeline half was re-done rather than inherited. |
| 5 | This file's round-3 section closed with "1919 passing tests" while the report said 1924 | Restated as 1924, the figure the suite reports. 1919 was the count before this round's five new tests were written. |
| 6 | A test's doc comment claimed "opencode refuses to start on a config that fails it" — stronger than the phase's own measurement, which found the same build accepted an unknown top-level key with exit 0 | The doc comment now states the measured rule: value validation is enforced at startup, total schema conformance is not. |
| 7 | A test's doc comment claimed `*` is one of opencode's permission keys | `*` is not a schema key — the schema types fifteen, and `*` is not among them; in opencode's resolved ruleset it is a *pattern* on a wildcard rule. The doc comment says so, and says why the case is kept anyway: it pins the allow-list's closed edge. The two documents that repeated the claim are corrected too. |
| 8 | Two citations in this file pointed at the wrong lines | `edge_phase_078_commits_are_ancestors_of_head_not_absent` is at `tests/unreviewed_work_ack_test.rs:571`; `is_shallow()` is at `tests/unreviewed_work_ack_test.rs:296`. Both corrected. |

None of the eight was fixed by weakening a gate, deleting a test or moving a
threshold. The fourteen new smoke checks are new assertions, not relaxed ones.
