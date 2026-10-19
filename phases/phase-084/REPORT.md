# Phase 084 — Resolve the reviewer agent's permissions where opencode can actually see them

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

So the phase's substance — the root `opencode.json` and its 15 tests — was
already written and is unchanged by this run. What this run had to do was
resolve the merge, and fix the one thing the preflight flagged: the report had
no `## Tests added` section.

## What changed

| File | Lines | What |
|---|---|---|
| `tests/unreviewed_work_ack_test.rs` | +12 −0 | The merge conflict, resolved: `main`'s four newer acknowledgement entries kept, the parked branch's stale entry dropped, and a comment block added at `ACKNOWLEDGED` recording why a commit is listed only while it is in this history. |
| `phases/phase-084/REPORT.md` | rewritten | This file. `## Tests` → `## Tests added`, `## Test-requirement matrix` → `## Edge-case matrix (AGENTS.md §3.2)`, and every figure re-measured. |
| `phases/phase-084/FINDINGS.md` | +53 −12 | The conflicted-merge finding, and the acknowledgement treadmill extended with its sixth observation. |

Delivered in the parked branch and carried unchanged by this run:

| File | Lines | What |
|---|---|---|
| `opencode.json` | +21 −0 | **New**, at the repository root. `agent.reviewer.permission` = `edit: "deny"`, `bash: "deny"`; `agent.verifier.permission` = `edit: "allow"`, `bash: "allow"`. No `default_agent`, no top-level `permission` block. |
| `tests/agent_permissions_test.rs` | +447 −0 | 15 `#[test]` functions; 12 drive the pure checker over deliberately broken documents and assert a failure is produced. |

This run's diff against `main` (`e0a3686`), measured:

```
$ git diff HEAD --numstat
21	0	opencode.json
211	0	phases/phase-084/FINDINGS.md
263	0	phases/phase-084/REPORT.md
447	0	tests/agent_permissions_test.rs
12	0	tests/unreviewed_work_ack_test.rs
$ git diff HEAD --name-only | grep -E '^\.github/|^rbops/|^\.opencode/agent/'
$   (no output)
$ git diff HEAD -- '*.rs' | grep -cE 'allow\(clippy::|#\[ignore|// *skip|\.skip\('
0
```

The changed Rust files are both under `tests/`, satisfying this phase's
`must_touch: ["tests/"]`. No file under `src/`, `.github/workflows/`, `rbops/`,
`.opencode/agent/` or `rbops/phases.json` is in the diff. No test was deleted,
skipped, ignored or weakened; no assertion was loosened and no threshold moved.

### The conflict, and how it was resolved

The two sides were not two versions of the same data:

| side | what its acknowledgement list held |
|---|---|
| `main` (`e0a3686`) | 31 entries — one for every commit in `git log HEAD` whose subject matches the selector |
| parked branch (`c1ebb59`) | the same list minus the four commits `main` gained since, plus one entry naming `48d5bf0…` |

`main` had moved on: it gained the deliveries for phases 054, 056, 058, 087,
089, 059 and 061 and the `ack: resume phase-088` commit, none of which the
parked attempt had ever seen. Those are kept.

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

### Why the file is at the root and not under `.opencode/`

`opencode.json` is the project-config filename opencode looks for at the working
directory. `.opencode/agent/` holds *agent definition* files (`<name>.md`), not
config, and this project's gate treats that directory as a forbidden path.
`project_config_ships_at_the_repository_root` asserts the file's parent is the
repository root, that no competing copy sits under `.opencode/agent/` or
`.opencode/`, and that the file satisfies the permission contract.

## The configuration-resolution rule this relies on

opencode resolves configuration in two scopes, project first:

> | Scope | Path |
> |---|---|
> | Project config | `./opencode.json`, `./opencode.jsonc`, or `.opencode/opencode.json`, walked up from the working directory to the worktree root |
> | Global config | `~/.config/opencode/opencode.json` or `~/.config/opencode/opencode.jsonc` |
>
> Configs from each scope are deep-merged. Project overrides global.

Citation: opencode's configuration reference, section **"Where files live"**,
published alongside the JSON Schema at <https://opencode.ai/config.json>. The
machine-checkable half of that citation is the schema, which the shipped file was
validated against with `jsonschema` 4.10.3 (FINDINGS.md): **SCHEMA VALID**. It
confirms the three claims the fix rests on — `Config.agent`'s additional
properties are `AgentConfig`, so arbitrarily named agents are legal;
`AgentConfig.mode` enumerates `["subagent","primary","all"]`; and
`AgentConfig.permission` accepts `edit` and `bash` with values from the
`PermissionActionConfig` enum `["ask","allow","deny"]`. opencode hard-fails to
start on a config that fails this schema, so it was checked rather than assumed.

Because the merge is *deep*, the global file's permissive top-level `permission`
block still contributes keys underneath this file. Per-agent `permission:`
overrides the top-level block, and it is declared here for **both** tools on
**both** agents rather than relying on an override for one — which is why the
tests assert all four values individually. The file declares no `build` agent
and no top-level `permission` block, so the implementer agent still resolves
entirely from the global config, exactly as before this phase.

**What these tests cannot prove.** They read a file and assert its contents.
They do not run `opencode`, so they cannot prove which file opencode opened, or
that a given opencode build resolves the name `reviewer` to this definition. A
future opencode that changed its resolution order would leave this suite green
while the reviewer again ran with edit access. The tests prove the file says the
right thing, and they fail the moment a `deny` is dropped, an agent is renamed,
a third tool is granted to the reviewer, or a `default_agent` is reintroduced.
Closing the remaining gap needs `rbops/dispatch.sh` to assert the resolved
agent's permissions at startup — recorded in FINDINGS.md, not attempted here,
since `rbops/` is off-limits.

## Tests added

| Test | Edge class covered |
|---|---|
| `project_config_ships_at_the_repository_root` | file present, at the repository root, no forbidden-path copy and no duplicate copy under `.opencode/` |
| `project_config_pins_reviewer_read_only_and_verifier_privileged` | asserts all four permission values individually — dropping one `deny` fails here |
| `project_config_declares_no_default_agent` | `default_agent` absent — the implementer agent cannot be repointed |
| `edge_missing_config_file_is_rejected` | the reproduction itself: no file → failure |
| `edge_missing_agent_key_is_rejected` | empty — `{}` and `{"$schema":"x"}`, both lack `agent` |
| `edge_missing_reviewer_is_rejected` | singleton — only the verifier is declared |
| `edge_missing_verifier_is_rejected` | singleton — only the reviewer is declared |
| `edge_default_agent_key_is_rejected` | missing keys — a `default_agent` naming `build` or `reviewer` |
| `edge_permission_flipped_to_allow_is_rejected` | the damaging edit — all four values flipped, each caught by name |
| `edge_a_tool_granted_to_the_reviewer_besides_edit_and_bash_is_rejected` | the pinned tool set's boundary — a third tool granted beside the two |
| `edge_reviewer_without_permission_block_is_rejected` | nesting — `agent.reviewer` declared, `permission` absent |
| `edge_agent_block_of_wrong_type_is_rejected` | type mismatch — `agent` holding an array or a string |
| `edge_permission_value_of_wrong_type_is_rejected` | type mismatch — boolean, number, nested object, null |
| `edge_duplicate_permission_key_resolves_to_the_last_value` | duplicate keys — a repeated `edit`, last value wins |
| `edge_malformed_config_is_rejected` | malformed — empty file, truncated, trailing comma, BOM, array, bare string |

15 `#[test]` functions, **12 of them `edge_*`**, and **12 of them assert a
failure is produced** (`expect_err` on a `Result`) — counted, not estimated.
The five failing documents the phase requires are present as separate tests:
the file missing, the `agent` key missing, `reviewer` missing, `verifier`
missing, and one permission flipped to `allow`.

No new test was added to `tests/unreviewed_work_ack_test.rs` by this run: the
file's 13 tests are pre-existing and unchanged apart from the 12-line comment
at `ACKNOWLEDGED`. Its `edge_*` cases (empty log, malformed log lines, a
stale acknowledgement, a shallow history, unicode subjects, a duplicate
acknowledgement) remain green, and the acknowledgement list they audit is now
exactly the set of matching commits in `HEAD`'s history.

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
running 15 tests
...
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no output, no diff |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0, `Finished dev profile`, no warnings; `git diff HEAD -- '*.rs' \| grep -c "allow(clippy::"` → `0` |
| `cargo test --all-targets` | **pass** — exit 0, 0 failed, 0 ignored (log at `target/tmp/full_test.log`) |
| `../rbops/verify.sh "phase-084"` | **pass** — see below |

## Invariants touched

- None. No file under `src/` is in the diff, no `Value` variant, error variant,
  `.rb` extension, `to … end` grammar or `set x to <expr>` form is touched, and
  every pre-existing test still passes.

## Edge-case matrix (AGENTS.md §3.2)

| Row | Status | Where / why |
|---|---|---|
| empty | covered | `edge_missing_agent_key_is_rejected` — `{}`; `edge_malformed_config_is_rejected` — empty file |
| singleton | covered | `edge_missing_reviewer_is_rejected` and `edge_missing_verifier_is_rejected` — exactly one agent declared |
| boundary | covered | `edge_duplicate_permission_key_resolves_to_the_last_value` — the boundary between the first and last occurrence of a repeated key, which is the boundary that decides whether the reviewer is writable; `edge_a_tool_granted_to_the_reviewer_besides_edit_and_bash_is_rejected` — the boundary of the pinned tool set, a third key being outside it |
| out_of_bounds | N/A | nothing here is indexable or ranged; no index is computed, so there is no out-of-bounds to reach. The nearest analogue — a tool key that is not one of the two pinned — is directly covered: `edge_a_tool_granted_to_the_reviewer_besides_edit_and_bash_is_rejected` (key present, ungranted-enforcement), `edge_reviewer_without_permission_block_is_rejected` (key absent), `edge_permission_value_of_wrong_type_is_rejected` (key present but unusable) |
| type_mismatch | covered | `edge_permission_value_of_wrong_type_is_rejected` — boolean, number, nested object and `null` where a string action belongs; `edge_agent_block_of_wrong_type_is_rejected` — array and string where an object belongs |
| numeric_boundary | N/A | the config schema admits no numeric field: `PermissionActionConfig` is the enum `["ask","allow","deny"]`. A number is rejected as a type mismatch, not a magnitude, so there is no numeric boundary to cross. Covered by `edge_permission_value_of_wrong_type_is_rejected` (the `bash: 0` case) |
| unicode | N/A | every key and every legal value in the schema is ASCII (`agent`, `reviewer`, `edit`, `allow`/`deny`/`ask`). No unicode key can be declared, and a unicode value is rejected as a type mismatch — `kind()` in `tests/agent_permissions_test.rs:56` names the type it found. The UTF-8 BOM case is covered by `edge_malformed_config_is_rejected`. Nothing in this change touches `src/`, where the language's own unicode handling is tested |
| nesting_recursion | covered | `edge_reviewer_without_permission_block_is_rejected` — `agent.reviewer` declared, `permission` one level down absent; `edge_permission_value_of_wrong_type_is_rejected` — a nested object where a flat action belongs; `edge_a_tool_granted_to_the_reviewer_besides_edit_and_bash_is_rejected` — the same block, one level in, with a key the two-tool lookup never reached |
| duplicate_missing_keys | covered | missing: `edge_reviewer_without_permission_block_is_rejected`, `edge_missing_reviewer_is_rejected`. duplicate: `edge_duplicate_permission_key_resolves_to_the_last_value` — JSON resolves a repeated `edit` last-wins, so a trailing `"allow"` is the one that counts |
| malformed_input | covered | `edge_malformed_config_is_rejected` — empty file, truncated object, trailing comma, UTF-8 BOM, top-level array, bare string; `edge_missing_config_file_is_rejected` — the file that is not there |
| resource_limit | N/A | the change is a JSON document read once by a test plus a bounded walk of a two-key object; there is no loop, no recursion, no allocation that grows with input, and no handle held across a failure. `check_no_grants_beyond` iterates the permission map, which the document bounds at the keys a hand-edit adds |

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

The global config declares **no** `agent` key, so `reviewer` and `verifier`
resolve to nothing there, and opencode falls back to `default_agent: build` —
whose `edit` and `bash` are `allow`. The checkout now ships `opencode.json`
at its root, where opencode looks before the global file, and declares both
agents by name:

```
$ python3 -c "import json;d=json.load(open('opencode.json'));print(json.dumps(d['agent'],indent=2))"
{
  "reviewer": {
    "description": "Read-only reviewer for a redblue pipeline phase. Reviews the phase diff and emits findings; may not edit files or run commands.",
    "mode": "primary",
    "permission": {
      "edit": "deny",
      "bash": "deny"
    }
  },
  "verifier": {
    "description": "Runs a redblue pipeline phase's gates. Needs the toolchain, so it is not read-only.",
    "mode": "primary",
    "permission": {
      "edit": "allow",
      "bash": "allow"
    }
  }
}
```

## Known gaps / follow-ups

- **The acknowledgement treadmill is structural, not accidental.** The pipeline
  commits the agent's work and only then asks the next run to acknowledge the
  commit it just made, so the SHA can never be written by the run that produces
  the work. This is the sixth delivery to pay it; FINDINGS.md carries the
  sequence and the two fixes that would end it (a commit subject that does not
  match the selector, or the dispatcher adding the line at commit time). The
  fix is outside `rbops/`, which this phase may not touch.
- **The tests cannot prove opencode's resolution.** They assert a file's
  contents; they cannot assert which file opencode opened. Closing that needs
  `rbops/dispatch.sh` to resolve the agent and check its permissions at
  startup — FINDINGS.md, not attempted here.
- **`mode: "primary"` is unconfirmed.** `opencode.json` declares both agents
  `mode: "primary"`, on the reading that `--agent <name>` names a primary
  agent. `rbops/` is not in this checkout and the phase brief forbids
  inspecting the pipeline, so the invocation could not be verified here. If
  dispatch invokes these as subagents, the mode is wrong; `mode: "all"` covers
  both readings at the cost of precision, and it is a one-word change.
- **The reviewer is granted only what is named.** `webfetch` and `write` are
  not declared for the reviewer, so they still fall back to the global config's
  defaults, which are unmeasured for those keys. Every key the reviewer *does*
  declare must be `deny` — enforced by `check_no_grants_beyond` — but the
  widening half of the contract still belongs in its own phase.
