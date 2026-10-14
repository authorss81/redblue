# Phase 084 — Resolve the reviewer agent's permissions where opencode can actually see them

## Reproduction (before any change)

```
$ test -f opencode.json && echo EXISTS || echo MISSING
MISSING
$ ls -d .opencode
ls: cannot access '.opencode': No such file or directory
$ ls tests/ | grep -E 'opencode|agent'
(no output)
```

The checkout ships no opencode config at any project scope. Per the opencode
config documentation, project config is `./opencode.json`, `./opencode.jsonc`
or `.opencode/opencode.json`, found by walking up from the working directory to
the worktree root; global config is `~/.config/opencode/opencode.json`; the two
are deep-merged with the project overriding the global. With no project file,
`--agent reviewer` and `--agent verifier` find nothing in the project scope and
inherit the global `default_agent: build`, whose `edit` and `bash` are `allow`.
The finding reproduces.

The contract the reviewer is supposed to live under — `rbops/agents/reviewer.md`
opening "You are READ-ONLY" — is prose passed in as a prompt, not a permission.
Nothing enforced it. Reproduced, and it reproduces in the sense that matters:
no file in the resolution path carries a permission for either name.

## What changed

| File | Lines | What |
|---|---|---|
| `opencode.json` | +20 −0 | **New**, at the repository root. Declares `agent.reviewer` with `permission.edit = "deny"`, `permission.bash = "deny"`, and `agent.verifier` with `permission.edit = "allow"`, `permission.bash = "allow"`. No `default_agent`. |
| `tests/agent_permissions_test.rs` | +359 −0 | **New**. 12 `#[test]` functions. Reads the shipped file and asserts all four permission values; 8 of them also drive the pure checker over deliberately broken documents. |

No file under `src/`, `rbops/`, `.github/workflows/`, `.opencode/agent/` or
`rbops/phases.json` was touched. `cargo fmt --all` reformatted only the new
test file — `git status` after formatting shows exactly two untracked files and
no modified ones.

### Why the file is at the root and not under `.opencode/agent/`

`opencode.json` is the project-config filename opencode looks for first at the
working directory. `.opencode/agent/` is the directory for *agent definition
files* (`<name>.md`), not for a config; a config there would not be in the
resolution path for a top-level config read, and this project's gate treats
`.opencode/agent/` as a forbidden path. `project_config_ships_at_the_repository_root`
asserts the file's parent is the repository root, and that no competing copy
exists under `.opencode/agent/` or `.opencode/`.

### The configuration-resolution rule this relies on

opencode resolves configuration in two scopes, project first:

> | Scope | Path |
> |---|---|
> | Project config | `./opencode.json`, `./opencode.jsonc`, or `.opencode/opencode.json` (opencode walks up from the cwd to the worktree root) |
> | Global config | `~/.config/opencode/opencode.json` or `~/.config/opencode/opencode.jsonc` |
>
> Configs from each scope are deep-merged. Project overrides global.

Citation: opencode's configuration documentation, "Where files live"
(<https://opencode.ai/config.json> is the published JSON Schema the
documentation points at; the same directory-of-configs table appears in the
`customize-opencode` reference shipped with opencode). The schema at that URL
was fetched and the shipped file was validated against it with
`jsonschema.validate` — **SCHEMA VALID**, no error. `$schema`, `agent`,
`description`, `mode` and `permission` are all in the published
`Config`/`AgentConfig`/`PermissionConfig` definitions, and `"allow"`/`"deny"`
are members of the `PermissionActionConfig` enum.

Because the merge is *deep*, the global file's permissive top-level
`permission` block still contributes `edit`/`bash` keys underneath this file.
Per-agent `permission:` overrides the top-level block, and it is declared here
for **both** tools on **both** agents rather than relying on an override for one
— which is why the tests assert all four values individually.

**What these tests cannot prove.** They read a file and assert its contents.
They do not run `opencode`, so they cannot prove which file opencode opened, or
that a given opencode build resolves the name `reviewer` to this definition. A
future opencode that changed its resolution order would leave this suite green
while the reviewer again ran with edit access. The test can prove the file says
the right thing and fails the moment a `deny` is dropped, an agent is renamed,
or a `default_agent` is reintroduced. Closing the remaining gap needs
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
| `edge_reviewer_without_permission_block_is_rejected` | nesting — `agent.reviewer` declared, `permission` absent |
| `edge_permission_value_of_wrong_type_is_rejected` | type mismatch — boolean, number, nested object, null |
| `edge_duplicate_permission_key_resolves_to_the_last_value` | duplicate keys — repeated `edit`, last value wins |
| `edge_malformed_config_is_rejected` | malformed — empty file, truncated, trailing comma, BOM, non-object |

Eight of twelve assert a **failure** is produced (`expect_err` / `expect` on a
`Result`). Five distinct failing documents are required by the phase and all
five are present as separate tests: file missing, `agent` key missing,
`reviewer` missing, `verifier` missing, one permission flipped to allow.

### Mutation check — the claim, verified

A single-value change to the shipped file, reviewer `edit` `deny` → `allow`:

```
$ cargo test --test agent_permissions_test
test edge_permission_flipped_to_allow_is_rejected ... FAILED
test project_config_pins_reviewer_read_only_and_verifier_privileged ... FAILED
test project_config_ships_at_the_repository_root ... FAILED
the error must name the flipped key, got: `agent.reviewer.permission.edit` is "allow", expected "deny"
assertion `left == right` failed: `agent.reviewer.permission.edit` must be "deny"
the shipped opencode.json must satisfy the agent permission contract: ".../opencode.json: `agent.reviewer.permission.edit` is \"allow\", expected \"deny\""
test result: FAILED. 9 passed; 3 failed
```

Three tests fail and every message names the exact key that is wrong. The file
was restored immediately afterwards; the final suite state is green.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — clean, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — clean, no warnings, no `allow(` added |
| `cargo test --all-targets` | **1569 passed, 0 failed, 0 ignored** (12 of them new in `agent_permissions_test`) |
| `./rbops/verify.sh phase-084` | **NOT RUN — cannot run from this checkout** |

On the fourth gate: `./rbops/verify.sh` does not exist in this checkout.
`test -x rbops/verify.sh` → absent; `ls -d rbops .opencode` → both absent. The
pipeline that dispatched this phase lives outside the working directory and the
phase brief instructs that it not be inspected. I am reporting this as not run
rather than claiming a pass I did not observe. The gates it would enforce that I
can check by hand are satisfied: no path under `.github/workflows/`, `rbops/`,
`.opencode/agent/` or `rbops/phases.json` is in the diff; no test is skipped,
ignored, deleted or weakened; no `allow(clippy::` was added.

## Test-requirement matrix

| Row | Status | Where / why |
|---|---|---|
| empty | covered | `edge_missing_agent_key_is_rejected` — `{}`; `edge_malformed_config_is_rejected` — empty file |
| singleton | covered | `edge_missing_reviewer_is_rejected` and `edge_missing_verifier_is_rejected` — exactly one agent declared |
| boundary | covered | `edge_duplicate_permission_key_resolves_to_the_last_value` — the boundary between first and last occurrence of a repeated key, which is the boundary that decides whether the reviewer is writable |
| out_of_bounds | N/A | nothing here is indexable or ranged. The nearest analogue — a tool key that is not one of the two declared — is covered by `edge_reviewer_without_permission_block_is_rejected` (absent key) and `edge_permission_value_of_wrong_type_is_rejected` (present but unusable). No index is ever computed, so there is no out-of-bounds to reach. |
| type_mismatch | covered | `edge_permission_value_of_wrong_type_is_rejected` — boolean, number, nested object, and `null` where a string action belongs |
| numeric_boundary | N/A | the config schema contains no numeric value. `edge_permission_value_of_wrong_type_is_rejected` passes `0` as a permission and proves it is rejected as a number rather than compared as a value |
| unicode | N/A | no string is transformed by anything under test — `check_document` compares four literal action strings against a JSON document. There is no normalisation, no case folding, no encoding step that a unicode input could exercise. The one non-ASCII byte that matters is handled instead: the BOM case in `edge_malformed_config_is_rejected` |
| nesting_recursion | covered | `edge_reviewer_without_permission_block_is_rejected` cuts the three-level `agent` → `reviewer` → `permission` chain at the last level; `edge_permission_value_of_wrong_type_is_rejected` cuts it at the fourth (`permission.edit`). No recursion: the walk is a fixed three-level descent |
| duplicate_missing_keys | covered | `edge_duplicate_permission_key_resolves_to_the_last_value` (repeated key) plus the two missing-agent tests |
| malformed_input | covered | `edge_malformed_config_is_rejected` — empty file, truncated JSON, trailing comma, BOM, top-level array |
| resource_limit | N/A | the checker reads one file of a fixed size once, with no loop over unbounded input, no recursion, and no accumulation. The only bound worth pinning is the file's own size, and pinning it would be asserting the file is small rather than that the code is safe |

## Invariants touched

None. This phase adds a configuration file and a test file. No change to the
`.rb` extension, to `end`-delimited blocks, to `set x to <expr>`, to `say`, to
the `Value` variants, to the `Error` enum, to trailing-comma or `{interp}`
string syntax, and no change to anything under `src/`. `README.md` and
`ROADMAP.md` are untouched.

## Known gaps / follow-ups

- The suite proves the file's contents, not opencode's resolution. Closing that
  requires `rbops/dispatch.sh` to resolve `--agent reviewer` and assert its
  permissions are `deny`/`deny` before handing it the diff. → FINDINGS.md
- `reviewer` is declared `mode: "primary"`, matching a CLI `--agent`
  invocation. If dispatch ever invokes it as a subagent instead, the mode is
  wrong. Nothing in this checkout can tell which it does; → FINDINGS.md
- `reviewer` is granted neither `webfetch` nor `write`, so it falls back to the
  global defaults for those. This phase pins only `edit` and `bash`, which is
  what the finding names. Tightening the rest is a separate concern. →
  FINDINGS.md