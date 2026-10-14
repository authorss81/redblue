# Phase 084 — Resolve the reviewer agent's permissions where opencode can actually see them

## Reproduction (before any change)

```
$ git show 55f7187^:opencode.json
fatal: path 'opencode.json' exists on disk, but not in '55f7187^'
$ git log --oneline --diff-filter=A -- opencode.json
55f7187 resume phase-084: adopt preserved work
$ git ls-files | grep -i opencode
opencode.json
```

The checkout shipped no opencode config at any project scope. Project config is
`./opencode.json` (or `.jsonc`, or `.opencode/opencode.json`) read from the
working directory outward; global config is `~/.config/opencode/opencode.json`;
the two are deep-merged with the project overriding the global. With no project
file, `--agent reviewer` and `--agent verifier` resolve nothing in the project
scope and inherit the global `default_agent: build`, whose `edit` and `bash` are
`allow`. The reviewer is contractually read-only, and the only thing expressing
that was prose in a prompt. The finding reproduces.

The fix landed in `55f7187` (adopted from the previous attempt). This run
verified that fix and closed the gaps the previous REPORT.md left.

## What changed

| File | Lines | What |
|---|---|---|
| `opencode.json` | +21 −0 | **New**, at the repository root (commit `55f7187`). `agent.reviewer.permission` = `edit: "deny"`, `bash: "deny"`; `agent.verifier.permission` = `edit: "allow"`, `bash: "allow"`. No `default_agent`. |
| `tests/agent_permissions_test.rs` | +44 −4 | 14 `#[test]` functions (2 added this run). Reads the shipped file and asserts all four permission values; 10 drive the pure checker over deliberately broken documents. |
| `tests/unreviewed_work_ack_test.rs` | +1 −0 | Acknowledgement entry for `55f71878…`, the resume commit that carries this phase's unreviewed work. See "Pre-existing red test" below. |

No file under `src/`, `rbops/`, `.github/workflows/`, `.opencode/agent/` or
`rbops/phases.json` was touched. `git status --short` after this run lists
exactly two modified files, both under `tests/`.

### Why the file is at the root and not under `.opencode/agent/`

`opencode.json` is the project-config filename opencode looks for at the working
directory. `.opencode/agent/` holds *agent definition* files (`<name>.md`), not
config, and this project's gate treats it as a forbidden path.
`project_config_ships_at_the_repository_root` asserts the file's parent is the
repository root and that no competing copy exists under `.opencode/agent/` or
`.opencode/`.

### The configuration-resolution rule this relies on

opencode resolves configuration in two scopes, project first:

> | Scope | Path |
> |---|---|
> | Project config | `./opencode.json`, `./opencode.jsonc`, or `.opencode/opencode.json`, walked up from the working directory to the worktree root |
> | Global config | `~/.config/opencode/opencode.json` or `~/.config/opencode/opencode.jsonc` |
>
> Configs from each scope are deep-merged. Project overrides global.

Citation: opencode's configuration reference, "Where files live" — the same
directory-of-configs table that ships with the `customize-opencode` reference
bundled with opencode, and the schema it publishes at
<https://opencode.ai/config.json>. The shipped file was validated against that
schema with `jsonschema.validate` before the commit: **SCHEMA VALID**.
`$schema`, `agent`, `description`, `mode` and `permission` are all members of
the published `Config`/`AgentConfig`/`PermissionConfig` definitions, and
`"allow"`/`"deny"` are members of the `PermissionActionConfig` enum.

Because the merge is *deep*, the global file's permissive top-level `permission`
block still contributes keys underneath this file. Per-agent `permission:`
overrides the top-level block, and it is declared here for **both** tools on
**both** agents rather than relying on an override for one — which is why the
tests assert all four values individually. The file declares no `build` agent and
no top-level `permission` block, so the implementer agent still resolves
entirely from the global config, exactly as before this phase.

**What these tests cannot prove.** They read a file and assert its contents.
They do not run `opencode`, so they cannot prove which file opencode opened, or
that a given opencode build resolves the name `reviewer` to this definition. A
future opencode that changed its resolution order would leave this suite green
while the reviewer again ran with edit access. The test proves the file says the
right thing and fails the moment a `deny` is dropped, an agent is renamed, or a
`default_agent` is reintroduced. Closing the remaining gap needs
`rbops/dispatch.sh` to assert the resolved agent's permissions at startup —
recorded in FINDINGS.md, not attempted here, since `rbops/` is off-limits.

## Tests added

| Test | Edge class covered |
|---|---|
| `project_config_ships_at_the_repository_root` | file present, at root, no forbidden-path or duplicate copy |
| `project_config_pins_reviewer_read_only_and_verifier_privileged` | asserts all four permission values individually |
| `project_config_declares_no_default_agent` | `default_agent` absent — implementer agent cannot be repointed |
| `edge_missing_config_file_is_rejected` | the reproduction itself: no file → failure |
| `edge_missing_agent_key_is_rejected` | empty — `{}` and `{"$schema":"x"}`, both lack `agent` |
| `edge_missing_reviewer_is_rejected` | duplicate/missing keys — only verifier declared |
| `edge_missing_verifier_is_rejected` | duplicate/missing keys — only reviewer declared |
| `edge_permission_flipped_to_allow_is_rejected` | the damaging edit — all four values flipped, each caught by name |
| `edge_default_agent_key_is_rejected` **(new)** | missing keys — a `default_agent` naming `build` or `reviewer` |
| `edge_agent_block_of_wrong_type_is_rejected` **(new)** | type mismatch — `agent` holding an array or a string |
| `edge_reviewer_without_permission_block_is_rejected` | nesting — `agent.reviewer` declared, `permission` absent |
| `edge_permission_value_of_wrong_type_is_rejected` | type mismatch — boolean, number, nested object, null |
| `edge_duplicate_permission_key_resolves_to_the_last_value` | duplicate keys — repeated `edit`, last value wins |
| `edge_malformed_config_is_rejected` | malformed — empty file, truncated, trailing comma, BOM, array, bare string |

Ten of the fourteen assert a **failure** is produced (`expect_err` on a
`Result`). The five failing documents the phase requires are present as separate
tests: file missing, `agent` key missing, `reviewer` missing, `verifier`
missing, one permission flipped to allow.

### Mutation check — run and observed this session

Reviewing the suite found three branches of the checker with no test reaching
them: the `default_agent` rejection, the "`agent` must be an object" rejection,
and `kind(Value::String)`. Two tests and one malformed case were added for
them, then the checker was stubbed with `return Ok(());` at the top of
`check_document` to prove the new tests can fail:

```
$ cargo test --test agent_permissions_test      # with the checker stubbed
test edge_agent_block_of_wrong_type_is_rejected ... FAILED
test edge_duplicate_permission_key_resolves_to_the_last_value ... FAILED
test edge_malformed_config_is_rejected ... FAILED
test edge_default_agent_key_is_rejected ... FAILED
test edge_missing_agent_key_is_rejected ... FAILED
test edge_missing_reviewer_is_rejected ... FAILED
test edge_missing_verifier_is_rejected ... FAILED
test edge_permission_flipped_to_allow_is_rejected ... FAILED
test edge_permission_value_of_wrong_type_is_rejected ... FAILED
test edge_reviewer_without_permission_block_is_rejected ... FAILED
test result: FAILED. 4 passed; 10 failed
```

Every `expect_err` test fails when the checker stops rejecting, so none of them
can pass vacuously. `edge_missing_config_file_is_rejected` correctly still
passed: it exercises `check_file`'s read branch, which runs before
`check_document`. The stub was removed immediately; `cargo test --test
agent_permissions_test` is 14/14 green again.

## Pre-existing red test this run had to fix

`cargo test --all-targets` at commit `55f7187` was **red**, before any edit in
this run:

```
---- unreviewed_work_in_head_history_is_acknowledged stdout ----
unreviewed-work commits selected: 23 | acknowledged: 22 | unacknowledged: 1 | stale entries: 0 | UNACKNOWLEDGED: 55f71878d905105dfbdd715365f3f74aa6d88bb5	resume phase-084: adopt preserved work
```

`tests/unreviewed_work_ack_test.rs` requires every commit whose subject matches
`resume phase-` or `checkpoint` to be listed in `ACKNOWLEDGED` with a reason.
The resume commit that carries this phase's own work was not listed. The test's
own failure message prescribes the remedy — "Add each one with a reason saying
what it carries and that it was deliberately not rewritten" — so the entry was
added with `git show --numstat` figures. No assertion was touched, nothing was
skipped, and no threshold moved. After the entry:
`selected: 23 | acknowledged: 23 | unacknowledged: 0 | stale entries: 0`.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — clean, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — clean, no warnings, no `allow(` added |
| `cargo test --all-targets` | **1571 passed, 0 failed, 0 ignored** across 60 test binaries, exit 0 (14 in `agent_permissions_test`) |
| `./rbops/verify.sh phase-084` | **NOT RUN — absent from this checkout** |

On the fourth gate: `./rbops/verify.sh` does not exist here. `ls -d rbops
.opencode` → `No such file or directory` for both; the pipeline that dispatched
this phase lives outside the working directory and the phase brief instructs
that it not be inspected. Reported as not run rather than claimed as passed.
What it enforces that I can check by hand: no path under `.github/workflows/`,
`rbops/`, `.opencode/agent/` or `rbops/phases.json` is in the diff; no test is
skipped, ignored, deleted or weakened; no `allow(clippy::` was added; the diff
touches `tests/`, the area this phase declares.

## Test-requirement matrix

| Row | Status | Where / why |
|---|---|---|
| empty | covered | `edge_missing_agent_key_is_rejected` — `{}`; `edge_malformed_config_is_rejected` — empty file |
| singleton | covered | `edge_missing_reviewer_is_rejected` and `edge_missing_verifier_is_rejected` — exactly one agent declared |
| boundary | covered | `edge_duplicate_permission_key_resolves_to_the_last_value` — the boundary between the first and last occurrence of a repeated key, which is the boundary that decides whether the reviewer is writable |
| out_of_bounds | N/A | nothing here is indexable or ranged; no index is computed, so there is no out-of-bounds to reach. The nearest analogue — a tool key that is not one of the two declared — is covered by `edge_reviewer_without_permission_block_is_rejected` (key absent) and `edge_permission_value_of_wrong_type_is_rejected` (key present but unusable) |
| type_mismatch | covered | `edge_permission_value_of_wrong_type_is_rejected` — boolean, number, nested object and `null` where a string action belongs; `edge_agent_block_of_wrong_type_is_rejected` — array and string where an object belongs |
| numeric_boundary | N/A | the config schema contains no numeric value. `edge_permission_value_of_wrong_type_is_rejected` passes `0` as a permission and proves it is rejected as a number rather than compared as a value |
| unicode | N/A | no string is transformed by anything under test — `check_document` compares four literal action strings against a JSON document, with no normalisation, case folding or encoding step a unicode input could exercise. The one non-ASCII byte that matters is handled instead: the BOM case in `edge_malformed_config_is_rejected` |
| nesting_recursion | covered | `edge_reviewer_without_permission_block_is_rejected` cuts the `agent` → `reviewer` → `permission` chain at the last level; `edge_permission_value_of_wrong_type_is_rejected` cuts it one level deeper. No recursion: the walk is a fixed three-level descent |
| duplicate_missing_keys | covered | `edge_duplicate_permission_key_resolves_to_the_last_value` (repeated key), `edge_missing_reviewer_is_rejected` / `edge_missing_verifier_is_rejected` / `edge_missing_agent_key_is_rejected` / `edge_default_agent_key_is_rejected` (absent keys) |
| malformed_input | covered | `edge_malformed_config_is_rejected` — empty file, truncated JSON, trailing comma, BOM, top-level array, bare string |
| resource_limit | N/A | the checker reads one file of a fixed size once: no loop over unbounded input, no recursion, no accumulation. The only bound worth pinning would be the file's own size, and pinning that asserts the file is small rather than that the code is safe |

## Invariants touched

None. This phase adds a configuration file, a test file, and one line of
acknowledgement data. No change to the `.rb` extension, to `end`-delimited
blocks, to `set x to <expr>`, to `say`, to the `Value` variants, to the `Error`
enum, to trailing-comma or `{interp}` string syntax, and no change to anything
under `src/`. `README.md` and `ROADMAP.md` are untouched.

## Known gaps / follow-ups

- The suite proves the file's contents, not opencode's resolution. Closing that
  requires `rbops/dispatch.sh` to resolve `--agent reviewer` and assert its
  permissions are `deny`/`deny` before handing it the diff. → FINDINGS.md
- `reviewer` is declared `mode: "primary"`, matching a CLI `--agent`
  invocation. If dispatch ever invokes it as a subagent instead, the mode is
  wrong. Nothing in this checkout can tell which it does. → FINDINGS.md
- `reviewer` is granted neither `webfetch` nor `write`, so it falls back to the
  global defaults for those. This phase pins only `edit` and `bash`, which is
  what the finding names. Tightening the rest is a separate concern. →
  FINDINGS.md
- The commit the pipeline makes for *this* run, if its subject contains
  `resume phase-` or `checkpoint`, will need its own entry in `ACKNOWLEDGED`,
  exactly as `55f7187…` needed one above. → FINDINGS.md
