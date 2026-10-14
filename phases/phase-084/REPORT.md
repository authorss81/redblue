# Phase 084 — Resolve the reviewer agent's permissions where opencode can actually see them

## Reproduction (before any change)

```
$ git show d1f2163^:opencode.json
fatal: path 'opencode.json' exists on disk, but not in 'd1f2163^'
$ git ls-files | grep -i opencode
opencode.json
$ git log --oneline --diff-filter=A -- opencode.json
d1f2163 resume phase-084: adopt preserved work
```

`opencode.json` was absent from every parent of this phase's own commit: the
checkout shipped no opencode config at any project scope. Project config is
`./opencode.json` (or `.jsonc`, or `.opencode/opencode.json`) read from the
working directory outward; global config is `~/.config/opencode/opencode.json`;
the two are deep-merged with the project overriding the global. With no project
file, `--agent reviewer` and `--agent verifier` resolve nothing in the project
scope and inherit the global `default_agent: build`, whose `edit` and `bash` are
`allow`. The reviewer is contractually read-only, and the only thing expressing
that was prose in a prompt. The finding reproduced.

The fix landed in `d1f2163`. This run re-verified it, repaired one stale
acknowledgement the gate caught, and closed the one reporting gap the previous
REPORT.md carried.

## What changed

| File | Lines | What |
|---|---|---|
| `opencode.json` | +21 −0 | **New**, at the repository root (commit `d1f2163`). `agent.reviewer.permission` = `edit: "deny"`, `bash: "deny"`; `agent.verifier.permission` = `edit: "allow"`, `bash: "allow"`. No `default_agent`. |
| `tests/agent_permissions_test.rs` | +395 −0 | 14 `#[test]` functions (carried from `d1f2163`). Reads the shipped file and asserts all four permission values; 11 drive the pure checker over deliberately broken documents and assert a failure is produced. |
| `tests/unreviewed_work_ack_test.rs` | +1 −1 | Acknowledgement entry repointed at `d1f21633394cc27b5bcd35ec73bc4a14a16e5b3f`, the resume commit that carries this phase's unreviewed work. See "Pre-existing red test" below. |
| `phases/phase-084/REPORT.md` | rewritten | Prior report cited `55f7187`, a commit that is not in this history, and overstated one count. Corrected. |

This run's own diff is **one line in one file**, both under `tests/`. No file
under `src/`, `rbops/`, `.github/workflows/`, `.opencode/agent/` or
`rbops/phases.json` was touched. `git diff --name-only d1f2163` lists exactly
`tests/unreviewed_work_ack_test.rs`, which satisfies this phase's
`must_touch: ["tests/"]`.

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

Citation: opencode's configuration reference, "Where files live" — the
directory-of-configs table that ships with the `customize-opencode` reference
bundled with opencode. The machine-checkable half of the citation is the
published JSON Schema at <https://opencode.ai/config.json>, fetched and applied
to the shipped file **this session** with `jsonschema` 4.10.3 (Draft 2020-12,
the remote `models.dev` `$ref` that this document does not reach stripped for
offline validation):

```
ERRORS: none
SCHEMA VALID
agent keys: ['reviewer', 'verifier'] | default_agent present: False
```

The schema confirms the three claims the fix rests on: `Config.agent` is an
object whose `additionalProperties` are `AgentConfig`, so arbitrarily named
agents are legal; `AgentConfig.mode` enumerates `["subagent","primary","all"]`,
so `primary` is legal; and `AgentConfig.permission` accepts `edit` and `bash`
with values from the `PermissionActionConfig` enum `["ask","allow","deny"]`.
opencode hard-fails to start on a config that fails this schema, so this is
checked rather than assumed.

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
| `edge_missing_reviewer_is_rejected` | singleton — only verifier declared |
| `edge_default_agent_key_is_rejected` | missing keys — a `default_agent` naming `build` or `reviewer` |
| `edge_agent_block_of_wrong_type_is_rejected` | type mismatch — `agent` holding an array or a string |
| `edge_missing_verifier_is_rejected` | singleton — only reviewer declared |
| `edge_permission_flipped_to_allow_is_rejected` | the damaging edit — all four values flipped, each caught by name |
| `edge_reviewer_without_permission_block_is_rejected` | nesting — `agent.reviewer` declared, `permission` absent |
| `edge_permission_value_of_wrong_type_is_rejected` | type mismatch — boolean, number, nested object, null |
| `edge_duplicate_permission_key_resolves_to_the_last_value` | duplicate keys — repeated `edit`, last value wins |
| `edge_malformed_config_is_rejected` | malformed — empty file, truncated, trailing comma, BOM, array, bare string |

14 `#[test]` functions, 11 of them `edge_*`. **11 assert a failure is produced**
(`expect_err` on a `Result`) — counted this run, not estimated. The five failing
documents the phase requires are present as separate tests: file missing,
`agent` key missing, `reviewer` missing, `verifier` missing, one permission
flipped to allow.

### Mutation check — the shipped file itself, mutated, run and observed this session

The previous run proved the tests fail by stubbing the checker. This run proved
the stronger claim, because the phase's definition of done is phrased about the
*file*: it mutated the shipped `opencode.json` on disk and ran the suite against
it, then restored the file byte-for-byte.

**Mutation 1 — one `deny` becomes `allow`** (`"bash": "deny"` → `"allow"`):

```
test edge_permission_flipped_to_allow_is_rejected ... FAILED
test project_config_pins_reviewer_read_only_and_verifier_privileged ... FAILED
test project_config_ships_at_the_repository_root ... FAILED
test result: FAILED. 11 passed; 3 failed; 0 ignored
```

**Mutation 2 — a `default_agent` is inserted** pointing at `reviewer`:

```
test project_config_declares_no_default_agent ... FAILED
test project_config_ships_at_the_repository_root ... FAILED
test edge_permission_flipped_to_allow_is_rejected ... FAILED
test result: FAILED. 11 passed; 3 failed; 0 ignored
```

The third casualty in both runs is `check_file` seeing a document that no longer
satisfies the contract, which is the redundant path by design. Restored:

```
$ cp target/tmp/opencode.json.bak opencode.json && git diff --stat -- opencode.json
OPENCODE_RESTORED_CLEAN          # no output from git diff --stat
$ cargo test --test agent_permissions_test
test result: ok. 14 passed; 0 failed; 0 ignored
```

Every `expect_err` test likewise fails when the checker stops rejecting, so none
of them can pass vacuously.

## Pre-existing red test this run had to fix

`cargo test --all-targets` at commit `d1f2163` was **red**, before any edit in
this run:

```
---- unreviewed_work_in_head_history_is_acknowledged stdout ----
unreviewed-work commits selected: 23 | acknowledged: 23 | unacknowledged: 1 | stale entries: 1 | UNACKNOWLEDGED: d1f21633394cc27b5bcd35ec73bc4a14a16e5b3f	resume phase-084: adopt preserved work | STALE: 55f71878d905105dfbdd715365f3f74aa6d88bb5
```

Two defects, one line. The previous run had added an entry for `55f7187…`; this
tree's HEAD carries `d1f2163…` instead, a commit of the same subject from an
earlier delivery of this phase. So the entry was unacknowledged *and* the old
one had gone stale — and the test rejects stale entries precisely because "an
entry pointing at nothing is how the list decays into a comment".

The entry at `tests/unreviewed_work_ack_test.rs:88` was repointed at the SHA that
is actually in this history, with figures taken from `git show --numstat
d1f2163` (+699/−0, 5 files, none under `src/`), and the reason records what it
replaces. No assertion was touched, nothing was skipped, and no threshold
moved. After the change: `selected: 23 | acknowledged: 23 | unacknowledged: 0 |
stale entries: 0`, 13/13 green.

A first run of that binary instead reported `history is shallow and completing
the clone did not make it complete`. That was a transient reading taken while
another git process held the repository; `git rev-parse
--is-shallow-repository` returns `false` on this clone and every re-run since has
taken the non-shallow branch. Not a product defect, but recorded because it is
what the first output said.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no output, no diff |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0, clean, no warnings; no `allow(clippy::` added (`git diff \| grep -c "allow(clippy::"` → `0`) |
| `cargo test --all-targets` | **pass** — exit 0, **1571 passed, 0 failed, 0 ignored** across 60 test binaries (14 in `agent_permissions_test`) |
| `./rbops/verify.sh phase-084` | **NOT RUN — absent from this checkout** |

On the fourth gate: `./rbops/verify.sh` does not exist here.
`find . -name verify.sh -not -path "./target/*"` returns nothing, and
`ls -d rbops .opencode` reports `No such file or directory` for both. The
pipeline that dispatched this phase lives outside the working directory and the
phase brief instructs that it not be inspected. Reported as not run rather than
claimed as passed. What it enforces that I can check by hand: no path under
`.github/workflows/`, `rbops/`, `.opencode/agent/` or `rbops/phases.json` is in
the diff; no test is skipped, ignored, deleted or weakened (`git diff -U0`
contains no `#[ignore]`, `// skip` or `.skip(`); no `allow(clippy::` was added;
the diff touches `tests/`, the area this phase declares.

The `error` lines in the `cargo test` log are a Redblue test fixture's own
program output (a minesweeper-style example printing its error token), not build
errors — the suite exits 0 with 0 `FAILED` occurrences.

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
  wrong — `mode: "all"` would cover both readings at the cost of precision.
  Nothing in this checkout can tell which it does. → FINDINGS.md
- `reviewer` is granted neither `webfetch` nor `write`, so it falls back to the
  global defaults for those. This phase pins only `edit` and `bash`, which is
  what the finding names. Tightening the rest is a separate concern. →
  FINDINGS.md
- The commit the pipeline makes for *this* run, if its subject contains
  `resume phase-` or `checkpoint`, will need its own entry in `ACKNOWLEDGED`,
  exactly as `d1f2163…` needed one above, and the entry for `d1f2163…` will go
  stale if the history is rewritten. Only the dispatcher, which knows the SHA,
  can add the line. → FINDINGS.md