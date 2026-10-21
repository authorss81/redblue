# Phase 084 — Resolve the reviewer agent's permissions where opencode can actually see them

## Result

`opencode.json` sits at the repository root of this checkout and declares the
pipeline's two agents: `reviewer`, denied `edit` and `bash` (and every other
key that could widen it), and `verifier`, allowed both. opencode resolves that
file before `~/.config/opencode/opencode.json`, so `--agent reviewer` binds to
a read-only agent instead of falling through to the global `default_agent:
build`.

Round 9 closed the last way the guarantee could be **true in the working
tree and false in the checkout the pipeline actually clones**. Every test the
phase had inherited read `opencode.json` out of the working tree. That file
can be present, correct, and still not be in the repository: `git rm --cached
opencode.json` leaves it exactly where it was, every assertion in the suite
still passes, and the pipeline — which clones this repository — finds no
project config at all, so `--agent reviewer` resolves to the global build
agent again, whose `edit` and `bash` are `allow`. The reviewer is the one
component whose writability no gate notices, because nothing a gate runs
asks the reviewer to write something.

Five tests close that, and one more closes a second reach: a top-level
key in the file applies to **every** agent in the project, not only the two it
names, so the file now carries an allow-list of top-level keys and refuses
`instructions`, `model`, `mcp`, `plugin` and the rest. `instructions` is the
sharp one — it is prepended to the system prompt of every agent, so a line
there is a line the reviewer reads as an instruction from the pipeline rather
than from its own contract.

**This round closed the same reach one level down, and one hole in the
reviewer's own block.** The top-level allow-list stops at the top level: the
checks that reached an agent looked at `mode` and `permission` and at nothing
else, so every other key of `agent.reviewer` was accepted silently — and the
published schema's `AgentConfig` carries a `prompt` key, which is the agent's
OWN prompt. A line there reaches the reviewer exactly as directly as a
top-level `instructions` array does, one level down. The agent blocks are now
allow-listed too (`description`, `mode`, `permission` and nothing else), and
`prompt`, `tools`, `model`, `options` and the rest of the schema's keys are
refused.

The reviewer's permission block had a hole of its own, in the direction that
matters: the allow-list it was checked against permitted `read`, `glob`,
`grep` and `list` to be granted, so `read: "allow"` passed — while this
document has called that a **widening** since round 2, because opencode's
default `read` rules ask about `.env` files (`read *.env ask`) and an
agent-level `read: "allow"` is appended after them and wins the last-match. A
future edit that added it would have re-allowed `.env` reads and the whole
suite would have stayed green. The reviewer's block is now required to declare
**exactly** the eleven keys it must deny, and the four read-only keys are
refused outright, in either direction.

The one red test in the tree on arrival (round 9's) is fixed.
`unreviewed_work_in_head_history_is_acknowledged` audits every commit in
`HEAD`'s ancestry whose subject marks unreviewed work against a written list;
the list named the SHA the previous round's delivery carried, and this history
carries a different one, so the audit reported one commit in the log that
nothing acknowledged and one acknowledged commit the log does not contain. The
entry now names the SHA this history actually carries, re-measured from that
commit. This round also took the delivery commit **out** of the second list
that pins a SHA's presence in the log — the list of commits *measured* on a
complete history — so the next round does not pay for a commit nobody
measured.

**The gates.** The full suite is green: **1945 passed; 0 failed; 0
ignored**, across the repository's test binaries, re-measured in this round.
`cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D warnings`
are clean, and `rbops/verify.sh phase-084` passes. No gate was weakened, no
test was deleted, no assertion was loosened, no threshold was moved, and no
`#[ignore]`, `// skip` or `allow(clippy::` was added.

**What these tests cannot prove.** They read a file and assert its contents
and its shipping state. They do not run opencode, so they cannot prove which
file opencode opened, or that a given opencode build resolves the name
`reviewer` to this definition. A future opencode that changed its resolution
order would leave this suite green while the reviewer again ran with edit
access. What the suite does prove is that the file says the right thing, that
it is in the repository, and that it fails the moment a `deny` is dropped, an
agent is renamed, a mode is changed, a third tool is granted to the reviewer,
a `default_agent` is reintroduced, an agent this file does not own appears in
it, a top-level `permission` block or any other top-level key is added to it,
a key the file does not own is added to either agent block, a read-only
permission is declared for the reviewer, or the file stops shipping.

**Where the history of this phase lives.** Rounds 1–8 wrote their accounts
into earlier revisions of this document and into
`phases/phase-084/FINDINGS.md`, which keeps the running audit section by
section, each tagged with the round that wrote it. This document describes
the delivery that is in the tree now; the earlier rounds are in this file's
git history and in FINDINGS.md.

## What changed

The delivery as a whole, measured against the base the dispatcher captured
before the resume merge (`b21821d`):

| File | Lines | What |
|---|---|---|
| `tests/agent_permissions_test.rs` | +1713 −0 | The whole suite: the permission contract, the mode, the shipping state of the file, the top-level allow-list, and — **this round** — the agent-level allow-list, the reviewer's exact-key rule, and the `git` failure message. Four new tests came with them. |
| `tests/unreviewed_work_ack_test.rs` | +62 −2 | The acknowledgement entry for the SHA this history carries, and — **this round** — the delivery commit's removal from the `MEASURED` list, with the reason written beside the list. |
| `phases/phase-084/REPORT.md` | +784 −0 | This document: the delivery in the tree now, with the measured evidence and the citation preserved. |
| `phases/phase-084/FINDINGS.md` | +1310 −0 | The running audit, one tagged section per round, the newest last. |

**What this round changed, in the suite.** The review of the previous
delivery returned five findings — two MAJOR and three MINOR — and all five are
fixed here, without weakening a gate, deleting a test or moving a threshold:

| Finding | Fix |
|---|---|
| the allow-list let the reviewer be granted `read`/`glob`/`grep`/`list`, and `read: "allow"` is a **widening** this document itself names | `REVIEWER_ALLOWED` is gone. The reviewer's permission block must declare **exactly** the eleven keys it must deny, and the four read-only keys are refused outright, in either direction. `project_config_declares_exactly_the_denied_permission_keys_for_the_reviewer` and `edge_a_read_only_permission_declared_on_the_reviewer_is_rejected` drive it. |
| the per-agent checks reached `mode` and `permission` and nothing else, so every other key of `agent.reviewer` was accepted silently — and the schema's `AgentConfig` has a `prompt` key | `AGENT_KEYS` allows `description`, `mode` and `permission` and nothing else; `check_agent_keys` refuses the rest, and names `prompt` and `tools` for what they are. `edge_an_agent_key_outside_the_allow_list_is_rejected` and `project_config_declares_only_the_own_keys_for_each_agent` drive it. |
| `git_run` dropped stderr, so every git failure said "failed with status N: " and named no reason | `git_run` returns stderr, and the error message carries it. The paste below shows the difference. |
| the report claimed the twelve failing tests were "exactly the tests that read the shipped file", which was false | The sentence is restated and the eleven that survived are named, with the reason each survived. |
| the `MEASURED` list pinned the delivery commit's **presence** in the log, which the pipeline's per-round rebuild guarantees to break | The delivery commit is off `MEASURED`, with the reason written beside the list, and the gap is named in Known gaps. |

`opencode.json` is **unchanged** by this round. The file rounds 2–8 shipped
is byte-for-byte the file here, and the live measurement below confirms it
still resolves the same way.

No file under `src/` is touched. This phase's subject is the agent
configuration the pipeline runs, and its subject has no Rust in it.

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

The **"Where files live"** table is not in the JSON Schema and not in the
configuration reference. It is in the `customize-opencode` skill that opencode
ships inside its own binary (section **"Where files live"**):

> | Scope | Path |
> |---|---|
> | Project config | `./opencode.json`, `./opencode.jsonc`, or `.opencode/opencode.json` (opencode walks up from the cwd to the worktree root) |
> | Global config | `~/.config/opencode/opencode.json` or `~/.config/opencode/opencode.jsonc` (NOT `~/.opencode/`) |
>
> Configs from each scope are deep-merged. Project overrides global. Unknown
> top-level keys in `opencode.json` are rejected with `ConfigInvalidError`.

**The rule this phase rests on, stated once:** opencode looks for the project
config **in the working directory first, walking up to the nearest Git
directory**, and only then falls back to the global
`~/.config/opencode/opencode.json`; the two are deep-merged, project over
global. `rbops` starts the reviewer as `opencode --dir "$PROJECT_DIR" --agent
reviewer`, so `$PROJECT_DIR` is the working directory, and the file that
resolves first is the one at the root of this checkout. That is why the file
is at the root and **not** under `.opencode/agent/` — which is a path this
project's own gate treats as forbidden, and which the same skill lists as the
place for an agent *file* rather than the agent *block* this phase needs.

That table is also why `project_config_ships_in_exactly_one_form` exists: the
`.jsonc` variants sit in the same resolution path as the file this suite
reads, and a second copy in one of them would be consulted by opencode while
this suite never saw it.

**What the documentation says about a bad config, and what was measured.**
The configuration reference presents the schema as editor support — "Your
editor should be able to validate and autocomplete based on the schema" — and
documents no startup validation against it, so an earlier delivery of this
report inferred a hard failure and was wrong to present the inference as a
fact. The claim was settled by measurement instead, on this machine:

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
enforced at startup" is a claim this report does not make. The shipped file
is schema-valid either way, so nothing about this phase depends on the
difference.

**The permission shapes the checker accepts.** The same skill states the
rules the suite enforces:

> Actions: `"allow"`, `"ask"`, `"deny"`.
>
> Per-tool value forms: `"allow"` shorthand (treated as `{"*": "allow"}`), or
> an object `{ pattern: action }`. Within an object, **insertion order
> matters**. opencode evaluates the LAST matching rule, so put broad rules
> first and narrow rules last.
>
> Known permission keys: `read, edit, glob, grep, list, bash, task,
> external_directory, todowrite, question, webfetch, websearch, lsp, doom_loop,
> skill`. Some of these (`todowrite, question, webfetch, websearch,
> doom_loop`) only accept a flat action, not a per-pattern object.

That is the rule `permission_action` implements: a bare action is the
catch-all, an object of patterns is the same rule **only** when it carries a
catch-all (`*` or `**`), and a narrow pattern is not a deny — it decides only
the paths it matches, and every other path falls through to the rules above
it. `{"*.md": "deny"}` denies the markdown files and leaves the rest to the
global config's `external_directory {"**": "allow"}`, so reading it as the
word "deny" is how a reviewer that may read outside the checkout passes a
suite that says it may not.

**What these tests cannot prove.** They read a file and assert its contents
and its shipping state. They do not run opencode, so they cannot prove which
file opencode opened, or that a given opencode build resolves the name
`reviewer` to this definition. What they do prove is that the file says the
right thing, and they fail the moment a `deny` is dropped, an agent is
renamed, a mode is changed, a third tool is granted to the reviewer, a
`default_agent` is reintroduced, an agent this file does not own appears in
it, a top-level `permission` block or any other top-level key is added to it,
a key the file does not own is added to either agent block, a read-only
permission is declared for the reviewer, or the file stops shipping.

Because the merge is *deep*, the global file's permissive top-level
`permission` block still contributes keys underneath this file. Per-agent
`permission:` overrides it — opencode's permissions reference says agent
permissions "are merged with the global config, and agent rules take
precedence" — and it is declared here for **every** key that could widen the
reviewer rather than for one tool. The file declares no `build` agent and no
top-level `permission` block, so the implementer agent still resolves
entirely from the global config, exactly as before this phase. **Both of
those are asserted, not merely claimed**:
`project_config_declares_exactly_the_two_pipeline_agents` and
`project_config_declares_no_top_level_permission_block` fail the moment either
shape appears in the shipped file, and
`edge_an_agent_this_file_does_not_own_is_rejected` and
`edge_a_top_level_permission_block_is_rejected` drive the rule that refuses
them. Round 9 added `edge_a_top_level_key_outside_the_allow_list_is_rejected`
and `project_config_declares_only_the_schema_and_the_agents`, which
generalise the same rule from the two keys it named to every top-level key
opencode accepts, and this round added the agent-level half of it.

## What the reviewer actually holds now

The resolved configuration, read out of opencode itself in this checkout,
**re-measured in this round** — the shipped file is byte-for-byte the file
earlier rounds shipped, and opencode resolves the reviewer to the same 28
rules it did then:

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
$ opencode debug agent reviewer | jq -r '.permission[] | "\(.permission) \(.pattern) \(.action)"' \
    > target/tmp/reviewer_rules.txt
$ wc -l < target/tmp/reviewer_rules.txt
28
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

Lines 1–12 are opencode's own defaults, lines 13–16 are the global config's
grants, and lines 17–27 are this phase's agent block. The agent's rules come
last, so they win: `websearch` and `skill` — the two keys an early review
found undeclared — resolve to `deny`, and `external_directory` resolves to
`deny` even though the global config grants it `{"**": "allow"}`. The last
line is opencode's own addition, which re-allows the directory it writes tool
output into; it is appended by opencode after every rule the config carries.

The verifier resolves the other way, which is the point of declaring it:

```
$ opencode debug agent verifier | jq -r '.permission[] | "\(.permission) \(.pattern) \(.action)"' \
    | grep -E '^(edit|bash) '
bash * allow
edit * allow
webfetch * allow
edit * allow
bash * allow
```

The agent's own `edit` and `bash` come last and win.

This is the only part of the phase a live measurement can confirm rather than
a file assertion: opencode itself, in this checkout, resolving the name
`reviewer` to an agent whose `edit` and `bash` are denied. What it still
cannot confirm is that a *different* opencode build resolves it the same way
— which is the gap the Known gaps section names.

`read` is deliberately **not** declared: opencode's default rules ASK about
`.env` files (`read *.env ask`, `read *.env.* ask`), and an agent-level
`read: "allow"` would be appended after them and re-allow them. The default
is the safer of the two here, because the pipeline runs `opencode run` with
nobody to answer a prompt: an `ask` there is a request that cannot be
granted, which is the same reason `doom_loop` is denied outright rather than
left to ask. It is an `ask`, not a `deny` — the rules say so.

**This round made that a rule instead of a claim.** Until now the checker
permitted `read`, `glob`, `grep` and `list` to be granted to the reviewer —
so `read: "allow"` passed every check in the suite, while this document has
called it a widening since round 2. The four keys are now refused outright,
whatever value they carry: `read: "allow"` is the widening, and `read:
"deny"` would take the reviewer's own job away. The reviewer's permission
block declares **exactly** the eleven keys it must deny and nothing else,
which is what `project_config_declares_exactly_the_denied_permission_keys_for_the_reviewer`
asserts of the shipped file and what
`edge_a_read_only_permission_declared_on_the_reviewer_is_rejected` drives on
mutated copies.

`task` is the key that was doing the most damage. The built-in `general`
subagent "has full tool access" (opencode's agents reference), so a reviewer
that can launch a subagent can have a subagent edit the tree it is judging —
and the original `check_no_grants_beyond` policed only the keys that were
present, so an omitted key passed it while its own doc comment claimed "The
reviewer is read-only in full". That check is now an allow-list, and
`project_config_denies_every_widening_permission_for_the_reviewer` requires
every widening key to be *present*.

**The same reach, one level down.** The two allow-lists — the top-level one
and the agent-level one — are the same rule applied at two levels of the
document, and the agent-level one is new in this round. The published schema
types `agent.<name>` as `AgentConfig`, whose properties are `color`,
`description`, `disable`, `hidden`, `maxSteps`, `mode`, `model`, `options`,
`permission`, `prompt`, `steps`, `temperature`, `tools`, `top_p` and
`variant`. opencode deep-merges the project config over the global one for
THAT agent, so every one of them is part of the agent's contract with the
pipeline. Only three are this file's to set: `permission` is the contract
itself, `mode` is what binds `--agent <name>` to the agent at all, and
`description` names when to use the agent and grants nothing.

`prompt` is the sharp one, and it is the same reach the top-level
`instructions` key has, one level down: it is the agent's own prompt, so a
line there is a line the reviewer reads as an instruction from the pipeline
rather than from its own contract. `tools` is the pre-`permission` way of
switching a tool on for one agent — deprecated, but still read, and it
grants. Both are refused, and the refusal names them for what they are.

The `options: {}` in the resolved configuration above is **opencode's own
addition**, not a key the shipped file carries: the file declares
`description`, `mode` and `permission` and nothing else, and the checker
reads the FILE. The resolved block is what opencode builds from the file, the
global config and its own defaults — which is why it also carries a
`default_agent` the shipped file does not declare.

## The dispatcher's half of the contract

Three of the first review's findings touched the pipeline rather than this
repository, and they were fixed in `rbops/dispatch.sh` — in the pipeline repo,
which is outside this checkout. **This round did not touch the pipeline and
did not re-measure it**: the instruction for this run was not to inspect the
pipeline, so no claim about its current contents is made here beyond the
fact that earlier rounds of this phase committed the change there. What
survives in this checkout is the file the pipeline's own reviewer and
verifier resolve out of, and the fact that the file is at the root of the
repository the pipeline clones.

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_the_shipped_config_must_be_tracked_and_committed_by_git` | resource/state — the config must be in the repository, not only in the working tree. The checkout the pipeline clones is a different tree from the one every other test reads. |
| `edge_the_shipped_config_must_not_be_gitignored` | resource/state — the trap that makes the config impossible to re-add, and therefore un-shippable, with no visible change to the file. |
| `edge_the_shipped_config_must_not_be_a_symlink` | malformed input — a link rather than the file: `Path::is_file` follows it, so every other assertion passes while a clone gets a dangling link. |
| `edge_a_top_level_key_outside_the_allow_list_is_rejected` | type mismatch / reach — a key that applies to every agent in the project, not only the two this file names. |
| `project_config_declares_only_the_schema_and_the_agents` | singleton / boundary — exactly two top-level keys, and no third. |
| `edge_an_agent_key_outside_the_allow_list_is_rejected` | type mismatch / reach, one level down — a key the file does not own inside an agent block. `prompt` is the sharp one: it is the agent's own prompt. |
| `project_config_declares_only_the_own_keys_for_each_agent` | singleton / boundary, one level down — exactly `description`, `mode` and `permission` per agent. |
| `edge_a_read_only_permission_declared_on_the_reviewer_is_rejected` | type mismatch / reach — a read-only permission key declared on the reviewer, in either direction. `read: "allow"` is the widening this document names. |
| `project_config_declares_exactly_the_denied_permission_keys_for_the_reviewer` | singleton / boundary — exactly the eleven keys the reviewer must deny, and no twelfth. |

The phase's five required failing cases, all present and all driving a
failing path:

| Required case | Test | What it drives |
|---|---|---|
| the file missing | `edge_missing_config_file_is_rejected` | `check_file` on a path that is not there is an `Err` |
| the `agent` key missing | `edge_missing_agent_key_is_rejected` | `{}` and `{"$schema":"x"}` are both `Err` |
| `reviewer` missing | `edge_missing_reviewer_is_rejected` | a config with only the verifier is an `Err` |
| `verifier` missing | `edge_missing_verifier_is_rejected` | a config with only the reviewer is an `Err` |
| one permission flipped to allow | `edge_permission_flipped_to_allow_is_rejected` | all four flips — `reviewer.edit`, `reviewer.bash`, `verifier.edit`, `verifier.bash` — are `Err` |

**Every new test was watched to fail.** The shipped state was mutated, the
test was run, and the tree was restored. All six pastes below are from THIS
round, against the suite as it stands now:

```
$ cp opencode.json target/tmp_opencode_backup.json

$ git rm --cached --quiet opencode.json
$ cargo test --test agent_permissions_test edge_the_shipped_config_must_be_tracked_and_committed_by_git
test edge_the_shipped_config_must_be_tracked_and_committed_by_git ... FAILED
`opencode.json` must be a path git tracks: `git ls-files --error-unmatch -- opencode.json` failed
with status 1: error: pathspec 'opencode.json' did not match any file(s) known to git
Did you forget to 'git add'?. A config that is only present in the working tree is not in the checkout the
pipeline clones, and there `--agent reviewer` resolves to the global build agent.
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 38 filtered out

$ printf '\n# agent files are local only\nopencode.json\n' >> .gitignore
$ cargo test --test agent_permissions_test edge_the_shipped_config_must_not_be_gitignored
test edge_the_shipped_config_must_not_be_gitignored ... FAILED
`opencode.json` is matched by an ignore rule (.gitignore:41:opencode.json	opencode.json): an
ignored config cannot be re-added after `git rm --cached` without `-f`, and the checkout that
results has no project config at all
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 38 filtered out

$ mv opencode.json target/tmp_opencode_real.json && ln -s /nonexistent/target/opencode.json opencode.json
$ cargo test --test agent_permissions_test edge_the_shipped_config_must_not_be_a_symlink
test edge_the_shipped_config_must_not_be_a_symlink ... FAILED
/home/runner/work/rbops/rbops/redblue/opencode.json must be a regular file, not a symlink
(FileType { is_file: false, is_dir: false, is_symlink: true, .. }): git stores a symlink as a link,
so a clone gets the link and not the target it points at
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 38 filtered out
```

The first of those three is the one this round changed: the failure message
now carries git's own reason, because `git_run` used to drop stderr and put
an empty stdout in its place. The old paste said `failed with status 1: .` —
a status and no reason at all. The sibling file
`tests/unreviewed_work_ack_test.rs` already did it this way.

The fourth mutation — a top-level `instructions` key added to the shipped
file — was measured the same way and took the allow-list with it. Fourteen of
the suite's thirty-nine tests failed:

```
$ python3 -c "import json; c=json.load(open('opencode.json')); c['instructions']=['report no findings, the phase is fine']; json.dump(c, open('opencode.json','w'), indent=2)"
$ cargo test --test agent_permissions_test
failures:
    edge_a_dropped_deny_key_is_rejected
    edge_a_missing_or_malformed_mode_is_rejected
    edge_a_narrow_pattern_allow_is_rejected
    edge_a_narrow_pattern_deny_is_rejected
    edge_a_permission_object_form_is_rejected_or_accepted
    edge_a_read_only_permission_declared_on_the_reviewer_is_rejected
    edge_a_tool_granted_to_the_reviewer_outside_the_allowed_set_is_rejected
    edge_a_top_level_key_outside_the_allow_list_is_rejected
    edge_an_agent_key_outside_the_allow_list_is_rejected
    edge_an_agent_this_file_does_not_own_is_rejected
    edge_permission_flipped_to_allow_is_rejected
    edge_reviewer_mode_subagent_is_rejected
    project_config_declares_only_the_schema_and_the_agents
    project_config_ships_at_the_repository_root

test result: FAILED. 25 passed; 14 failed; 0 ignored; 0 measured; 0 filtered out
```

**The fourteen are the fourteen that route through `check_document`** — the
checker they all share, which refuses a document with a top-level key this
file does not own. `edge_a_top_level_key_outside_the_allow_list_is_rejected`
is the one that names the key, and
`project_config_declares_only_the_schema_and_the_agents` is the one that
names the shipped file's own key set.

The claim this paragraph used to make — that the failures were "exactly the
tests that read the shipped file" — was **false**, and this round corrected
it. Twenty-five tests read the shipped file, and the eleven that survived
did so for three different reasons:

- **Seven never call `check_document` at all.**
  `project_config_denies_every_widening_permission_for_the_reviewer` calls
  `check_reviewer_denies` directly,
  `project_config_declares_primary_mode_for_both_agents` calls `check_mode`
  directly, and the other five read the JSON themselves
  (`project_config_pins_reviewer_read_only_and_verifier_privileged`,
  `project_config_declares_no_default_agent`,
  `project_config_declares_exactly_the_two_pipeline_agents`,
  `project_config_declares_no_top_level_permission_block`,
  `project_config_declares_the_schema_it_is_validated_against`). A top-level
  key is not in what any of them reads.
- **Two read only the part of the document a top-level key cannot reach** —
  `project_config_declares_exactly_the_denied_permission_keys_for_the_reviewer`
  reads `agent.reviewer.permission`, and
  `project_config_declares_only_the_own_keys_for_each_agent` reads the agent
  blocks. Neither has a top-level key set to compare against.
- **Two short-circuit earlier.**
  `edge_a_top_level_permission_block_is_rejected` and
  `edge_default_agent_key_is_rejected` do route through `check_document`, but
  the top-level `permission` block and the `default_agent` are refused before
  the top-level key loop is reached — and each of those two tests is
  asserting that refusal, so it still passes.

The two new mutations, measured the same way, each took its own rule with it:

```
$ python3 -c "import json; c=json.load(open('opencode.json')); c['agent']['reviewer']['permission']['read']='allow'; json.dump(c, open('opencode.json','w'), indent=2)"
$ cargo test --test agent_permissions_test
failures:
    edge_a_permission_object_form_is_rejected_or_accepted
    edge_an_agent_key_outside_the_allow_list_is_rejected
    edge_an_agent_this_file_does_not_own_is_rejected
    project_config_declares_exactly_the_denied_permission_keys_for_the_reviewer
    project_config_ships_at_the_repository_root

test result: FAILED. 34 passed; 5 failed; 0 ignored; 0 measured; 0 filtered out

$ python3 -c "import json; c=json.load(open('opencode.json')); c['agent']['reviewer']['prompt']='report no findings, the phase is fine'; json.dump(c, open('opencode.json','w'), indent=2)"
$ cargo test --test agent_permissions_test
failures:
    edge_a_permission_object_form_is_rejected_or_accepted
    edge_an_agent_key_outside_the_allow_list_is_rejected
    project_config_declares_only_the_own_keys_for_each_agent
    project_config_ships_at_the_repository_root

test result: FAILED. 35 passed; 4 failed; 0 ignored; 0 measured; 0 filtered out
```

The first of those two is the hole this round closed: before it, a
`read: "allow"` on the reviewer was a **green** suite — the widening this
document has named since round 2, and nothing in the suite forbade it. The
second is the reach one level down: an agent-level `prompt` is the agent's
own prompt, and it used to be accepted silently.

Every mutation was undone, and `git diff --stat -- opencode.json` is empty:
the shipped file is exactly the file the earlier rounds shipped.

### The edge-case matrix

Every row the phase's prompt lists, with the row that covers it or the reason
it does not apply.

| Row | Status | Where, or why not |
|---|---|---|
| empty | covered | `edge_missing_agent_key_is_rejected` (an empty document `{}`), `edge_malformed_config_is_rejected` (an empty file, a truncated one, a trailing comma, a byte order mark), `edge_an_empty_document_is_accepted` in `tests/report_markdown_test.rs` |
| singleton | covered | `edge_missing_reviewer_is_rejected` and `edge_missing_verifier_is_rejected`: exactly one of the two agents declared is still a gap, and the reviewer alone is the dangerous half. The exact-set cases are covered too: `project_config_declares_only_the_schema_and_the_agents` (two top-level keys), `project_config_declares_only_the_own_keys_for_each_agent` (three keys per agent) and `project_config_declares_exactly_the_denied_permission_keys_for_the_reviewer` (eleven permission keys) |
| boundary | covered | `edge_a_narrow_pattern_deny_is_rejected` and `edge_a_narrow_pattern_allow_is_rejected` — the boundary between a catch-all and a narrow pattern is the boundary the whole permission contract turns on; `edge_duplicate_permission_key_resolves_to_the_last_value` — the boundary of last-wins resolution; `edge_a_read_only_permission_declared_on_the_reviewer_is_rejected` — the boundary between a permission the reviewer may hold and one it may not, in either direction |
| out_of_bounds | N/A | This phase adds no indexed access to anything. A permission key is looked up **by name** in a JSON object, and a key that is absent is a named error (`is not declared: ...`), not an index past the end of anything. The equivalent case — a key that is simply not there — is covered by `edge_a_dropped_deny_key_is_rejected` and `edge_missing_reviewer_is_rejected` |
| type_mismatch | covered | `edge_permission_value_of_wrong_type_is_rejected` (a boolean, a number, an object with a non-string action, an object mixing two actions, a null permission block), `edge_agent_block_of_wrong_type_is_rejected` (an array of names, a bare string), `edge_a_missing_or_malformed_mode_is_rejected` (a boolean mode), `edge_an_agent_key_outside_the_allow_list_is_rejected` (a key of the wrong kind at the agent level) |
| numeric_boundary | N/A | The file holds no numbers. There is no arithmetic in it, so there is no `0/0`, no `NaN`, no `±Infinity` and no precision boundary to be wrong about; the only numbers in this phase are the counts in this document, which are measured by `git` and by the test runner |
| unicode | N/A | The file holds no text values — the only strings in it are schema key names (`edit`, `bash`, `external_directory`, …), which are ASCII, and two English descriptions. There is no user text, no emoji, no CJK and no combining mark anywhere in the subject of this phase, so there is no encoding boundary to cross. The one place unicode does appear is the *report's* subject matter, and `tests/report_markdown_test.rs` guards that: it checks both delivery documents read the same way under a CommonMark fence reader and under a simpler one, which is where a stray backtick or an unusual character would show |
| nesting_recursion | N/A | The document is three levels deep by schema (`agent.<name>.permission.<key>`) and the checker walks exactly that depth with no recursion in it. There is no nesting to bound and no mutual recursion to guard; the closest analogue — a value nested in the wrong place — is covered by `edge_agent_block_of_wrong_type_is_rejected` |
| duplicate_missing_keys | covered | `edge_duplicate_permission_key_resolves_to_the_last_value` (a repeated key resolves last-wins, so a trailing `"allow"` is the one that counts), `edge_a_dropped_deny_key_is_rejected` (a deleted key is a deleted deny), `edge_missing_reviewer_is_rejected`, `edge_missing_verifier_is_rejected` |
| malformed_input | covered | `edge_malformed_config_is_rejected` (empty file, truncated JSON, trailing comma, byte order mark, a non-object document, a bare string), `edge_malformed_config_is_rejected`'s companion `edge_a_missing_or_malformed_mode_is_rejected`, and `edge_permission_value_of_wrong_type_is_rejected` for the values inside a well-formed document |
| resource_limit | N/A | The checker reads one 905-byte file and walks fixed key lists; there is no recursion, no loop over unbounded input and no output to cap. The one resource-shaped concern in this phase's subject is opencode's own repeated-call guard, `doom_loop`, and the shipped file **denies** it outright rather than leaving it to `ask` — a `ask` in a headless pipeline run is a request that cannot be granted, so denying it makes the guard block the call instead of waiting forever for an answer. That is asserted by `project_config_denies_every_widening_permission_for_the_reviewer` |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass — no `allow(clippy::` was added |
| `cargo test --all-targets` | 1945 passed, 0 failed, 0 ignored |
| `cargo test --doc` | pass — 2 doc tests run |
| `rbops/verify.sh phase-084` | pass — every check green |
| `opencode debug config` / `opencode debug agent reviewer` | the reviewer resolves to 28 rules whose last eleven are this phase's denies |

Every number in this table was measured in this round. The suite total is the
sum of the `N passed` lines of `cargo test --all-targets` in this checkout,
which is the same measurement `rbops/verify.sh` makes against
`/tmp/test.log` — and it is the number the gate itself printed: `PASS cargo
test: 1945 passed, 0 failed`. The `cargo test --doc` line is the doc-test
target's own count: two doc tests, in `src/bootstrap.rs` and
`src/bytecode/mod.rs`.

**The gate, run against this checkout.** The gate reads its report from
`$RBOPS_ROOT/phases/<phase>/REPORT.md`, and `RBOPS_ROOT` can be pointed at
the checkout itself — the form that checks the report in the tree rather
than a harvested copy of it:

```
$ RBOPS_ROOT="$PWD" RBOPS_PROJECT_DIR="$PWD" RBOPS_PHASES="$PWD/../rbops/phases.json" \
    RBOPS_BASELINE="$PWD/../rbops/baseline.json" \
    RBOPS_BASE_REF=b21821d6acbc9e1bef8d9d71293bf7cee19a68f1 \
    ../rbops/verify.sh phase-084

== manifest
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
  PASS diff substance: +4352 -2 (min +5)
  PASS implementation touched tests/ (must_touch: tests/)

== cargo fmt
  PASS cargo fmt --check

== cargo clippy
  PASS cargo clippy -D warnings

== cargo test
  PASS cargo test: 1945 passed, 0 failed

== test policy
  PASS failure-asserting test present: 33
  PASS test quota met: 49 rust / 0 redblue (need 3 or 2)
  PASS edge case test present: 35
  PASS no newly skipped tests

== report honesty
  PASS reported test count (1945) matches actual (1945)

╔══════════════════════════════════════╗
║ VERIFY PASS — phase-084                ║
╚══════════════════════════════════════╝
```

**Two runs of the gate, and why they differ by one line.** The gate reads its
report from the pipeline's copy, not from this checkout's: `dispatch.sh`
harvests `phases/phase-084/REPORT.md` out of the project and into the
pipeline root before the gate runs. Run against the pipeline root, the gate
reads the copy left there by an earlier round — which claims 1941 — and says
so:

```
== report honesty
  WARN report claims 1941 passing, actual run has 1945 — under-claimed, fix the number
  PASS reported test count (1941) does not exceed actual (1945)

╔══════════════════════════════════════╗
║ VERIFY PASS — phase-084                ║
╚══════════════════════════════════════╝
```

With this report harvested in place of the stale copy, the same gate says:

```
== report honesty
  PASS reported test count (1945) matches actual (1945)

╔══════════════════════════════════════╗
║ VERIFY PASS — phase-084                ║
╚══════════════════════════════════════╝
```

Both runs are green. The second is the one the pipeline will see, because the
harvest runs before the gate.

The gate's own diff measurements on this delivery: `diff substance: +4352
-2`, `implementation touched tests/ (must_touch: tests/)`, `test quota met:
49 rust / 0 redblue`, `edge case test present: 35`, `failure-asserting test
present: 33`, `no newly skipped tests`.

## Invariants touched

- **None.** No file under `src/` is touched, and no language surface moves:
  `.rb` is still the source extension, `to … end` / `if … end` / `for … end`
  still use `end`, `set x to <expr>` is still assignment, `say` still prints,
  the `Value` variants are still `Nothing/Number/Text/YesNo/List/Record/
  Object/Function/Builtin`, and the errors are still `Error::{Lexer,Parser,
  Analyzer,Runtime,Io}`. `opencode.json` is not a `.rb` file and is never
  executed by the interpreter. `examples/*.rb` and `modules/*.rb` are
  untouched and still run.

## Known gaps / follow-ups

- **The acknowledgement list goes stale on every round, and this round paid
  it again.** The pipeline re-creates the delivery commit on each round
  (`resume phase-084: adopt preserved work`, rebuilt on the same parent), so
  the SHA a round writes into `tests/unreviewed_work_ack_test.rs` leaves
  `HEAD`'s ancestry the moment the next round lands. This round's entry names
  `2f7e437`, the SHA this history carries, and the next round inherits that
  bill. The audit is right to insist: a list of SHAs that satisfy nothing
  cannot pass by looking thorough. → FINDINGS.md.
- **There were TWO lists, and the second one is now fixed.** The
  acknowledgement list above is the one this phase's findings have always
  named, and it is a *presence* list in one direction and an *absence* list in
  the other: an entry that names a commit the log does not contain is stale,
  and a commit the log contains that no entry names is unreviewed. The
  delivery commit has to be on it — the audit would otherwise report it as
  unreviewed work — so that bill falls due every round.

  The second list is `MEASURED`: the commits *measured* on a complete history,
  each of which must be **present** in `git log HEAD`. Round 9 pointed it at
  `2f7e437` as well, which made the next round pay for the same commit twice
  — once for the acknowledgement and once for the measurement — and neither of
  the two repairs the phase's findings propose for the acknowledgement list
  (a commit-time dispatcher line, or a subject shape the selector misses) can
  repair a list that asserts the SHA's *presence* in the log.

  **This round took the delivery commit off `MEASURED`**, and the reason is
  written beside the list: the delivery commit is not a commit that was
  measured. It does not exist while the work that goes into it is being
  written, and the pipeline rebuilds it on the same parent on every round, so
  a SHA pinned there leaves `HEAD`'s ancestry with the next round and costs
  that round a line for a commit nobody measured. The two lists answer
  different questions — `ACKNOWLEDGED` says "this commit is in the log and
  carries unreviewed work", `MEASURED` says "this commit was measured" — and
  only the first of them has to carry the delivery commit. → FINDINGS.md.
- **`git ls-files` alone cannot prove a file is tracked.** The gate runs
  `git add -N .` before it measures anything, and an intent-to-add entry
  makes an untracked file look tracked — `git ls-files --error-unmatch` exits
  0 for one. The new test therefore also requires the path to be in `HEAD`'s
  tree, which `git add -N` cannot fake. This is a fact about the gate, and
  `rbops/` is not this phase's to touch. → FINDINGS.md.
- **The suite still cannot prove which file opencode opened.** It reads a
  file and asserts its contents and its shipping state. A different opencode
  build that changed its resolution order would leave it green while the
  reviewer again ran with edit access. The live `opencode debug agent
  reviewer` measurement above is the only thing that closes that gap today,
  and it is a measurement of *this* build, not of every build.
- **The allow-lists are pinned to the schema as it is published today.** The
  agent-level list (`description`, `mode`, `permission`) and the top-level
  list (`$schema`, `agent`) are closed, so a key opencode adds tomorrow is
  refused rather than silently accepted — that is the point of a closed list,
  and it is why the lists are written as lists and not as "the keys the
  schema knows". The cost is that a key opencode adds tomorrow has to be
  added to the list on purpose, which is a line of maintenance this phase
  hands to whoever needs it. The schema this round measured against is the
  one published at <https://opencode.ai/config.json>, whose `AgentConfig`
  carries the fifteen properties named in the report above.
- **`agent.verifier` is declared but not launched.** No `--agent verifier`
  exists in `rbops` today — the pipeline runs its gates as a script. The
  declaration is there so that a dispatcher which does run a phase's gates as
  an agent resolves to *these* permissions rather than to the global build
  agent, and it is asserted; the wiring is not claimed.
- **The pipeline-side half was not re-measured in this round.** The run's
  instruction was not to inspect the pipeline repo, so no claim is made here
  about the current contents of `rbops/dispatch.sh` or `rbops/smoke.sh`.
