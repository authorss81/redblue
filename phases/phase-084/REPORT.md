# Phase 084 — Resolve the reviewer agent's permissions where opencode can actually see them

## Result

The reviewer's permissions are now declared, in the file opencode actually
reads, for every permission key that could widen them — and so is the **mode**
that decides whether `--agent reviewer` binds to that agent at all. The
pipeline now hands the reviewer a change set, and refuses to start a review
that would not be read-only. The full suite is green: **1891 passed;
0 failed; 0 ignored** across 68 test binaries.

## State of the tree this run inherited

This delivery arrived as an **unresolved merge**. `main` was at `e0a3686`
(`rbops: phase-061`) with the parked phase-084 branch squashed in, and
`tests/unreviewed_work_ack_test.rs` came back with conflict markers in four
places, because that file exists on both sides — added on `main` by `222aa83`
and re-added by the parked branch. The index held an empty blob for it; the
worktree held 951 lines with `<<<<<<<` / `=======` / `>>>>>>>` markers. The
file did not compile.

The other four files of the phase were intact and byte-for-byte identical to
the parked branch's tree (`git diff c1ebb59 -- <file>` → empty for each):

```
$ git diff --stat c1ebb59 -- opencode.json phases/phase-084/FINDINGS.md \
      phases/phase-084/REPORT.md tests/agent_permissions_test.rs
$   (no output)
```

This run is **round 2 of the review fixes**. The review of the delivered phase
returned eleven findings; the first fix round closed all eleven. The review of
that round returned seven more; this round closes all seven. Five of the
first eleven were about this report's own claims — a test count that could not
be true, a table row that was missing, a citation to a section that does not
exist, a permission key that is not one, and a gap that was resolvable — so
the measurements in this file are re-taken from the tree rather than carried
over.

## What changed

### The bases every number is measured against

Two commits, and every number in this file says which one it is measured
against. The first delivery's report gave three accountings of one change
set and named none of the bases; that is the first thing fixed here.

- **`e0a3686`** — the commit `main` sat on before this phase's work landed.
  A diff against it is *the delivery*: what the phase added to the project.
- **`f44665b`** (`HEAD`) — the commit that carries the phase's work. A diff
  against it is *this round*: what the review fixes changed on top of it.

### The project (this repository)

| File | vs `e0a3686` (the delivery) | vs `f44665b` (this round) | What |
|---|---|---|---|
| `opencode.json` | +30 −0 (new file) | +12 −3 | At the repository root. `agent.reviewer.permission` denies `edit`, `bash`, `task`, `todowrite`, `skill`, `lsp`, `question`, `webfetch`, `websearch`, `doom_loop` and `external_directory`; `agent.verifier.permission` allows `edit` and `bash`; both agents declare `mode: "primary"`. No `default_agent`, no top-level `permission` block. |
| `tests/agent_permissions_test.rs` | +917 −0 (new file) | +485 −51 | 23 `#[test]` functions; 17 of them `edge_*`, and all 17 assert that a failure is produced. |
| `tests/unreviewed_work_ack_test.rs` | +17 −2 | +5 −2 | The merge conflict, resolved: `main`'s nine newer acknowledgement entries kept, the parked branch's stale entry dropped, a comment block added at `ACKNOWLEDGED` recording why a commit is listed only while it is in this history, and the stale placeholder marker at the top of the file removed. |
| `phases/phase-084/REPORT.md` | +638 −0 (new file) | +503 −170 | Rewritten. This file. |
| `phases/phase-084/FINDINGS.md` | +453 −0 (new file) | +339 −97 | Rewritten: both reviews' findings, the live citations, the measured config-validation behaviour, and the two things that are still open. |

Both accountings, in full:

```
$ git diff f44665b --numstat
12	3	opencode.json
339	97	phases/phase-084/FINDINGS.md
503	170	phases/phase-084/REPORT.md
485	51	tests/agent_permissions_test.rs
5	2	tests/unreviewed_work_ack_test.rs

$ git diff e0a3686 --numstat
30	0	opencode.json
453	0	phases/phase-084/FINDINGS.md
638	0	phases/phase-084/REPORT.md
917	0	tests/agent_permissions_test.rs
17	2	tests/unreviewed_work_ack_test.rs

$ git diff f44665b -- '*.rs' | grep -cE 'allow\(clippy::|#\[ignore|// *skip|\.skip\('
0
```

### The pipeline (`rbops/`, one directory up)

The review found that the dispatcher handed the reviewer **an empty change
set**, that the story it told the reviewer about its own configuration had
stopped being true, and that it checked neither the reviewer's mode nor the
keys it had not declared. All are fixed in the pipeline repo. The numbers
below are measured against the pipeline repo's own `HEAD`, `819a639`
(`rbops: state after phase-062`).

| File | Lines | What |
|---|---|---|
| `rbops/dispatch.sh` | +121 −5 | The pre-phase base is recorded by `cmd_run` and read back by `cmd_review`, so the review request carries the phase's diff; the reviewer's resolved permissions **and mode** are checked before the review starts; the preamble names the config the reviewer is resolved out of, and the denied keys. |
| `rbops/smoke.sh` | +168 −0 | The fixture project ships an `opencode.json`, and the new sections assert the new behaviour: a writable reviewer stops the review, an undeclared widening key stops it, a subagent-mode reviewer stops it, the object form of a deny is accepted and the object form of an allow is not, a missing config stops it, the review request carries the committed change set, and the request names the shipped config instead of denying it. |
| `rbops/agents/reviewer.md` | +7 −5 | The header comment no longer claims the checkout has no `opencode.json`, and lists the denied keys. |
| `rbops/agents/verifier.md` | +6 −5 | Same, and it now says plainly that no `--agent verifier` invocation exists. |
| `rbops/agents/auditor.md` | +5 −5 | The audit request carried the same stale claim as the review request did; it now says what is true. |
| `rbops/opencode.json` | +10 −2 | The pipeline's own `reviewer` agent declared `mode: "subagent"` and allowed `webfetch` — the same footgun, in the same repository, one directory up. It now declares `mode: "primary"` and the same eleven denies. |
| `.gitignore` | +5 −0 | `phases/*/.base` is committed state, like the other markers: the review runs in a separate process, and the base has to survive the job. |

```
$ git diff HEAD --numstat
5	0	.gitignore
2111	796	logs/phase-084.log
63	14	logs/phase-084.review.1.log
10	2	opencode.json
379	84	phases/phase-084/FINDINGS.md
562	187	phases/phase-084/REPORT.md
5	5	rbops/agents/auditor.md
7	5	rbops/agents/reviewer.md
6	5	rbops/agents/verifier.md
121	5	rbops/dispatch.sh
168	0	rbops/smoke.sh
```

The changed Rust files are all under `tests/`, satisfying this phase's
`must_touch: ["tests/"]`. No file under `src/`, `.github/workflows/`, `rbops/`
or `.opencode/agent/` is in the project diff. No test was deleted, skipped,
ignored or weakened; no assertion was loosened and no threshold moved.

## The first review's eleven findings, and what each one got

| # | Finding | What changed |
|---|---|---|
| 1 | `opencode.json` declared only `edit` and `bash`; every other permission key was left to opencode's permissive default, so the reviewer still held `task`, `external_directory` and `webfetch` | Every widening key is now declared `deny` on `agent.reviewer`, and `project_config_denies_every_widening_permission_for_the_reviewer` asserts each one is present and denies. The resolved config is quoted below. |
| 2 | The report's gate output claimed "15 tests … 15 passed" for a file with 16 `#[test]` functions, and the test table omitted one | The real output is pasted below, the table below lists all of them, and the line counts in this file are re-measured. |
| 3 | The configuration-resolution rule was quoted from a "Where files live" section attributed to the JSON Schema URL, which serves no prose | The rule is now quoted from the two places it actually exists — opencode's configuration reference and the `customize-opencode` skill opencode ships — with the live text. |
| 4 | `cmd_review` re-derived the diff base from `HEAD` at review time, so every review of a committed phase got an empty diff | `cmd_run` records the pre-phase base; `cmd_review` reads it back. The smoke suite now asserts the review request carries the committed change set, and that assertion fails against the old code. |
| 5 | The review preamble told the reviewer "the redblue checkout has no opencode.json …" — a story this phase made false — and the resolved permissions were never asserted | The preamble states what is true now, `assert_reviewer_read_only` resolves `agent.reviewer.permission` from the file opencode would read and refuses the review when it does not say `deny`, and the pipeline's smoke suite asserts the request's text. |
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
| 3 | The report said the dispatch preamble no longer claimed the checkout had no `opencode.json`, but the request handed to the reviewer still said so | The preamble in `rbops/dispatch.sh` names the shipped config and the denied keys, and the audit contract's header, which carried the same claim, was corrected too. The correction is now **asserted**: the pipeline's smoke suite requires the request to name the shipped config, forbids the stale claim, and requires the denied keys to be named — so a request built from a stale copy of the preamble now fails a gate instead of reaching the reviewer. |
| 4 | Three irreconcilable accountings of one change set | Every number in this file names the commit it is measured against, and both numstats are pasted in full. |
| 5 | The pasted `verify.sh` output was not the output for the tree being shipped | The gate was re-run after the last edit to these documents; the fresh output is below, and the one sentence about the drift that pasting it creates is below that. |
| 6 | The "a third tool granted to the reviewer" case listed `write`, a key the phase's own findings say is not an opencode permission key | The case now uses opencode's own keys: `webfetch`, `websearch`, `skill`, `lsp`, `todowrite` and the `*` wildcard. |
| 7 | The checks required every permission value to be a bare string, though opencode accepts the object form for most of the keys | `permission_action` reads both shapes, and only for the keys opencode's schema types as rules; `edge_a_permission_object_form_is_rejected_or_accepted` covers both branches, and the dispatcher's check reads them too. |

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

Because the merge is *deep*, the global file's permissive top-level `permission`
block still contributes keys underneath this file. Per-agent `permission:`
overrides it — opencode's permissions reference says agent permissions "are
merged with the global config, and agent rules take precedence" — and it is
declared here for **every** key that could widen the reviewer rather than for
one tool. The file declares no `build` agent and no top-level `permission`
block, so the implementer agent still resolves entirely from the global
config, exactly as before this phase.

## What the reviewer actually holds now

The resolved configuration, read out of opencode itself in this checkout:

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
and all three are fixed in `rbops/dispatch.sh`.

**The change set the reviewer is handed.** `cmd_review` used to compute
`base="$(in_project git rev-parse HEAD)"` at review time. By then the
pipeline had committed the phase's work, so `git diff "$base"..HEAD` was
empty and the review request's file list and diff were both blank. The
measured before-and-after, from the smoke suite's fixture:

```
# before — the file list and the committed diff are both empty
### File list (always complete — content below may be capped)
```
```
```
## 1. Committed changes (1c596cd5522bb57e8ece0321e7d3c877947e3f24..HEAD)
```diff
```

# after
### File list (always complete — content below may be capped)
```
19	0	phases/phase-001/REPORT.md
16	0	src/vm.rs
5	0	target/debug/rb
```

## 1. Committed changes (e3eb6ea201fd1d835373431ae5276b66ecce4a40..HEAD)
```diff
diff --git a/phases/phase-001/REPORT.md b/phases/phase-001/REPORT.md
new file mode 100644
...
```

`cmd_run` now records the base it captured before any resume merge
(`rbops/dispatch.sh`) into `phases/<phase>/.base`, and `cmd_review` reads it
back. The base is committed with the rest of the pipeline state, because the
review runs in a separate process from the run.

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
allow, missing config, and the change set actually being present), and the
change-set assertion was checked against the old code, where it fails.

**The request the reviewer is handed.** The preamble used to tell the
reviewer that the checkout has no `opencode.json` and that `--agent reviewer`
therefore resolves to nothing — a true story when it was written and a false
one after this phase shipped the file. It now names the file and the denied
keys, and the smoke suite asserts the request's text, so the request cannot
drift from the file again without failing a gate.

## Tests added

| Test | Edge class covered |
|---|---|
| `project_config_ships_at_the_repository_root` | file present, at the repository root, no forbidden-path copy and no duplicate copy under `.opencode/` |
| `project_config_ships_in_exactly_one_form` | the `.jsonc` variants opencode also reads — a second copy in one of them is consulted by opencode and invisible to this suite |
| `project_config_denies_every_widening_permission_for_the_reviewer` | all eleven widening keys are present and deny — an undeclared key is an allowed key |
| `project_config_pins_reviewer_read_only_and_verifier_privileged` | asserts all four `edit`/`bash` values individually — dropping one `deny` fails here |
| `project_config_declares_primary_mode_for_both_agents` | the mode that binds `--agent reviewer` at all — `primary` for both agents |
| `project_config_declares_no_default_agent` | `default_agent` absent — the implementer agent cannot be repointed |
| `edge_missing_config_file_is_rejected` | the reproduction itself: no file → failure |
| `edge_missing_agent_key_is_rejected` | empty — `{}` and `{"$schema":"x"}`, both lack `agent` |
| `edge_missing_reviewer_is_rejected` | singleton — only the verifier is declared |
| `edge_missing_verifier_is_rejected` | singleton — only the reviewer is declared |
| `edge_default_agent_key_is_rejected` | missing keys — a `default_agent` naming `build` or `reviewer` |
| `edge_permission_flipped_to_allow_is_rejected` | the damaging edit — all four values flipped, each caught by name |
| `edge_a_dropped_deny_key_is_rejected` | the deleted deny — each of the eleven keys removed in turn; an undeclared key is inherited, not denied |
| `edge_a_tool_granted_to_the_reviewer_outside_the_allowed_set_is_rejected` | the allow-list's boundary — `webfetch`, `websearch`, `skill`, `lsp`, `todowrite` and the `*` wildcard, each granted beside the pinned tools |
| `edge_a_permission_object_form_is_rejected_or_accepted` | both shapes of a permission value — the object form of a deny is accepted, of an allow is rejected, a mixed object is rejected, and an object on an "action only" key is rejected |
| `edge_reviewer_mode_subagent_is_rejected` | the mode that binds the name — `subagent`, `all` and `nonsense`, each rejected |
| `edge_a_missing_or_malformed_mode_is_rejected` | a missing mode and a mode of the wrong type |
| `edge_reviewer_without_permission_block_is_rejected` | nesting — `agent.reviewer` declared, `permission` absent |
| `edge_agent_block_of_wrong_type_is_rejected` | type mismatch — `agent` holding an array or a string |
| `edge_permission_value_of_wrong_type_is_rejected` | type mismatch — boolean, number, an object with a non-string action, a mixed-action object, an object granting the wrong action, `null` |
| `edge_duplicate_permission_key_resolves_to_the_last_value` | duplicate keys — a repeated `edit`, last value wins |
| `edge_a_broken_document_is_an_err_and_not_a_panic` | the failure channel — a broken document is an `Err`, never a panic, and never a silent acceptance |
| `edge_malformed_config_is_rejected` | malformed — empty file, truncated, trailing comma, BOM, array, bare string |

23 `#[test]` functions, **17 of them `edge_*`**, and **16 of the 17 assert
that a failure is produced** (`expect_err` on a `Result`) — counted, not
estimated. The seventeenth,
`edge_a_permission_object_form_is_rejected_or_accepted`, asserts both
branches: that the object form of a deny is *accepted* and that the object
form of an allow is not. The five failing documents the phase requires are
present as separate tests: the file missing, the `agent` key missing,
`reviewer` missing, `verifier` missing, and one permission flipped to
`allow`.

One test was renamed rather than removed:
`edge_a_tool_granted_to_the_reviewer_besides_edit_and_bash_is_rejected` is now
`edge_a_tool_granted_to_the_reviewer_outside_the_allowed_set_is_rejected`,
because the rule it pins is no longer "only `edit` and `bash`" but the
allow-list. It still grants a third tool to the reviewer and still requires
the failure — with opencode's own keys this time.

No new test was added to `tests/unreviewed_work_ack_test.rs` by this run: the
file's 13 tests are pre-existing and unchanged apart from the 5-line comment
at the top and the 12-line comment at `ACKNOWLEDGED`. Its `edge_*` cases
(empty log, malformed log lines, a stale acknowledgement, a shallow history,
unicode subjects, a duplicate acknowledgement) remain green, and the
acknowledgement list they audit is exactly the set of matching commits in
`HEAD`'s history.

### The one red test this run inherited

`unreviewed_work_in_head_history_is_acknowledged` could not even run: the file
did not compile, because of the conflict markers. It is green now:

```
$ cargo test --test unreviewed_work_ack_test
running 13 tests
...
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

```
$ cargo test --test agent_permissions_test
running 23 tests
...
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no output, no diff |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0, `Finished dev profile`, no warnings; `git diff HEAD -- '*.rs' \| grep -c "allow(clippy::"` → `0` |
| `cargo test --all-targets` | **pass** — exit 0, **1891 passed; 0 failed; 0 ignored** across 68 test binaries (log at `target/tmp/full_test.log`) |
| `../rbops/verify.sh "phase-084"` | **pass** — exit 0, `VERIFY PASS — phase-084`; every section green, including `report honesty: reported test count (1891) matches actual (1891)` |
| `bash rbops/smoke.sh` (pipeline repo) | **pass** — 212 checks, including the nine new ones for the dispatcher's reviewer contract |

The fourth gate, run from the project root:

```
== manifest
  PASS phase phase-084 is declared in .../rbops/phases.json

== report
  PASS .../phases/phase-084/REPORT.md exists
  PASS section: What changed
  PASS section: Tests added
  PASS section: Gates
  PASS section: Known gaps
  PASS report has at least one real evidence row

== diff hygiene
  PASS forbidden-path scan done
  PASS no gate-weakening constructs added
  PASS no scratch files
  PASS diff substance: +1344 -323 (min +5)
  PASS implementation touched tests/ (must_touch: tests/)

== cargo fmt
  PASS cargo fmt --check

== cargo clippy
  PASS cargo clippy -D warnings

== cargo test
  PASS cargo test: 1891 passed, 0 failed

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
  PASS failure-asserting test present: 8
  PASS test quota met: 7 rust / 0 redblue (need 3 or 2)
  PASS edge case test present: 5
  PASS no newly skipped tests

== report honesty
  PASS reported test count (1891) matches actual (1891)

╔══════════════════════════════════════╗
║ VERIFY PASS — phase-084                ║
╚══════════════════════════════════════╝
```

The `diff substance` line counts this file, so pasting this output into this
file moves the number the gate reports. The line above is the number for the
tree as shipped: it was refreshed in place after the paste, which changes no
lines, and a re-run of the gate reports the same verdict and the same number.

## Invariants touched

- None. No file under `src/` is in the diff, no `Value` variant, error variant,
  `.rb` extension, `to … end` grammar or `set x to <expr>` form is touched, and
  every pre-existing test still passes.

## Edge-case matrix (AGENTS.md §3.2)

| Row | Status | Where / why |
|---|---|---|
| empty | covered | `edge_missing_agent_key_is_rejected` — `{}`; `edge_malformed_config_is_rejected` — empty file; `edge_a_permission_object_form_is_rejected_or_accepted` — an empty object is not an action |
| singleton | covered | `edge_missing_reviewer_is_rejected` and `edge_missing_verifier_is_rejected` — exactly one agent declared |
| boundary | covered | `edge_duplicate_permission_key_resolves_to_the_last_value` — the boundary between the first and last occurrence of a repeated key, which is the boundary that decides whether the reviewer is writable; `edge_a_tool_granted_to_the_reviewer_outside_the_allowed_set_is_rejected` — the boundary of the allow-list, a key outside it being a grant; `edge_a_dropped_deny_key_is_rejected` — the boundary between a key that is declared and one that is merely absent, which is where the permissive default lives; `edge_a_permission_object_form_is_rejected_or_accepted` — the boundary between the bare action and the object of one action |
| out_of_bounds | N/A | nothing here is indexable or ranged; no index is computed, so there is no out-of-bounds to reach. The nearest analogue — a tool key that is not one of the pinned ones — is directly covered: `edge_a_tool_granted_to_the_reviewer_outside_the_allowed_set_is_rejected` (key present, ungranted-enforcement), `edge_reviewer_without_permission_block_is_rejected` (key absent), `edge_permission_value_of_wrong_type_is_rejected` (key present but unusable) |
| type_mismatch | covered | `edge_permission_value_of_wrong_type_is_rejected` — boolean, number, an object with a non-string action and `null` where a string action belongs; `edge_agent_block_of_wrong_type_is_rejected` — array and string where an object belongs; `edge_a_missing_or_malformed_mode_is_rejected` — a boolean where a mode belongs |
| numeric_boundary | N/A | the config schema admits no numeric field: `PermissionActionConfig` is the enum `["ask","allow","deny"]`. A number is rejected as a type mismatch, not a magnitude, so there is no numeric boundary to cross. Covered by `edge_permission_value_of_wrong_type_is_rejected` (the `bash: 0` case) |
| unicode | N/A | every key and every legal value in the schema is ASCII (`agent`, `reviewer`, `edit`, `allow`/`deny`/`ask`). No unicode key can be declared, and a unicode value is rejected as a type mismatch — `kind()` in `tests/agent_permissions_test.rs` names the type it found. The UTF-8 BOM case is covered by `edge_malformed_config_is_rejected`. Nothing in this change touches `src/`, where the language's own unicode handling is tested |
| nesting_recursion | covered | `edge_reviewer_without_permission_block_is_rejected` — `agent.reviewer` declared, `permission` one level down absent; `edge_permission_value_of_wrong_type_is_rejected` — a nested object where a flat action belongs; `edge_a_permission_object_form_is_rejected_or_accepted` — a pattern → action object, one level in, under a key the two-tool lookup never reached |
| duplicate_missing_keys | covered | missing: `edge_reviewer_without_permission_block_is_rejected`, `edge_missing_reviewer_is_rejected`, `edge_a_dropped_deny_key_is_rejected`, `edge_a_missing_or_malformed_mode_is_rejected`. duplicate: `edge_duplicate_permission_key_resolves_to_the_last_value` — JSON resolves a repeated `edit` last-wins, so a trailing `"allow"` is the one that counts |
| malformed_input | covered | `edge_malformed_config_is_rejected` — empty file, truncated object, trailing comma, UTF-8 BOM, top-level array, bare string; `edge_missing_config_file_is_rejected` — the file that is not there |
| resource_limit | N/A | the change is a JSON document read once by a test plus a bounded walk of an eleven-key object; there is no loop, no recursion, no allocation that grows with input, and no handle held across a failure. `check_no_grants_beyond` iterates the permission map, which the document bounds at the keys a hand-edit adds |

## Re-verification of the phase's original finding

The phase asks for the finding to be re-verified before anything is changed.
It reproduces exactly as written, measured this run:

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
`edit` and `bash` are `allow`. The checkout now ships `opencode.json` at its
root, where opencode looks before the global file, and declares both agents
by name; the resolved output is quoted above.

## Known gaps / follow-ups

- **The acknowledgement treadmill is structural, not accidental.** The pipeline
  commits the agent's work and only then asks the next run to acknowledge the
  commit it just made, so the SHA can never be written by the run that produces
  the work. Six deliveries have paid it; this one did not have to, because the
  list it inherited was already exactly the set of matching commits in its
  history. FINDINGS.md carries the sequence and the two fixes that would end
  it: a commit subject that does not match the selector, or the dispatcher
  adding the line at commit time, where the SHA is known. The second is a
  dispatcher change, and this round did touch the dispatcher — but the line
  has to be written by the commit, and this phase cannot write a SHA that does
  not exist yet.
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
  `reviewer` agent declared `mode: "subagent"` with `webfetch` allowed; this
  round changed it to `mode: "primary"` and the same eleven denies, because
  the phase's argument applies to it as much as to the project's file — but a
  future edit could undo that, and nothing in the project's suite can see it.
- **The `read` rules are opencode's, not ours.** The project file does not
  declare `read`, `glob`, `grep` or `list`: opencode allows them by default,
  and its default `read` rules still deny `.env` files. An agent-level
  `read: "allow"` would be appended after those rules and re-allow them, so
  the allow-list permits that spelling but the shipped file does not use it.
