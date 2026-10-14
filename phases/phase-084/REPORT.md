# Phase 084 — Resolve the reviewer agent's permissions where opencode can actually see them

## Reproduction (before any change)

```
$ git show ffa1f29^:opencode.json
fatal: path 'opencode.json' exists on disk, but not in 'ffa1f29^'
$ git ls-tree --name-only ffa1f29^ | grep opencode
(no output)
$ git log --oneline --diff-filter=A -- opencode.json
ffa1f29 resume phase-084: adopt preserved work
```

The parent of this phase's own commit shipped **no** opencode config at any
project scope. Project config is `./opencode.json` (or `.jsonc`, or
`.opencode/opencode.json`), read from the working directory outward to the
worktree root; global config is `~/.config/opencode/opencode.json`; the two are
deep-merged with the project overriding the global. With no project file,
`--agent reviewer` and `--agent verifier` resolve nothing in the project scope
and inherit the global `default_agent: build`, whose `edit` and `bash` are
`allow`. The reviewer is contractually read-only, and the only thing expressing
that was prose in a prompt. A reviewer that can edit the phase it reviews
defeats its own purpose, and no gate notices, because nothing the gate runs asks
the reviewer to write something.

The fix is already in the tree, carried in `05c77d1`. **This run made no
production change.** It ran the gates on the tree as delivered, fixed the one
test that was red, and re-proved by mutation that the shipped assertions bite.

## What changed

| File | Lines | What |
|---|---|---|
| `opencode.json` | +21 −0 | **New**, at the repository root (commit `05c77d1`). `agent.reviewer.permission` = `edit: "deny"`, `bash: "deny"`; `agent.verifier.permission` = `edit: "allow"`, `bash: "allow"`. No `default_agent`, no top-level `permission` block. |
| `tests/agent_permissions_test.rs` | +395 −0 | 14 `#[test]` functions (carried from `05c77d1`). Reads the shipped file and asserts all four permission values; 11 drive the pure checker over deliberately broken documents and assert a failure is produced. |
| `tests/unreviewed_work_ack_test.rs` | +5 −1 | Acknowledgement entry repointed at `05c77d1ab…` and its SHA added to `MEASURED`. This run's entire diff — see "The one red test this run had to fix". |
| `phases/phase-084/REPORT.md` | rewritten | This file. |
| `phases/phase-084/FINDINGS.md` | +9 −0 | One entry: the acknowledgement treadmill, now observed a second time. |

This run's own diff is **one file under `tests/`**, satisfying this phase's
`must_touch: ["tests/"]`:

```
$ git diff --stat
 tests/unreviewed_work_ack_test.rs | 6 +++++-
 1 file changed, 5 insertions(+), 1 deletion(-)
$ git diff --name-only | grep -c '^src/'
0
```

No file under `src/`, `rbops/`, `.github/workflows/`, `.opencode/agent/` or
`rbops/phases.json` is in the diff. No test was skipped, ignored, deleted or
weakened; the diff contains no `#[ignore]`, `// skip` or `.skip(`; no
`allow(clippy::` was added (`git diff | grep -c "allow(clippy::"` → `0`).

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

Citation: opencode's configuration reference, section **"Where files live"**,
published alongside the JSON Schema at <https://opencode.ai/config.json>. The
machine-checkable half of that citation is the schema, which the shipped file was
validated against with `jsonschema` 4.10.3 (see FINDINGS.md): **SCHEMA VALID**.
It confirms the three claims the fix rests on — `Config.agent`'s additional
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
or a `default_agent` is reintroduced. Closing the remaining gap needs
`rbops/dispatch.sh` to assert the resolved agent's permissions at startup —
recorded in FINDINGS.md, not attempted here, since `rbops/` is off-limits.

## Tests added

| Test | Edge class covered |
|---|---|
| `project_config_ships_at_the_repository_root` | file present, at root, no forbidden-path or duplicate copy |
| `project_config_pins_reviewer_read_only_and_verifier_privileged` | asserts all four permission values individually |
| `project_config_declares_no_default_agent` | `default_agent` absent — the implementer agent cannot be repointed |
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
(`expect_err` on a `Result`) — counted, not estimated. The five failing documents
the phase requires are present as separate tests: file missing, `agent` key
missing, `reviewer` missing, `verifier` missing, one permission flipped to allow.

**This run added no new `#[test]` function.** It adopted the 14 above from
`05c77d1` and re-verified by mutation, below, that they bite. Adding more would
have been padding, not coverage.

### Mutation check, re-run this session — the shipped file itself, mutated, run, observed, restored

Because the phase's definition of done is phrased about the *file*, the shipped
`opencode.json` was mutated on disk and the suite run against it. Both mutations
were performed in this run; the outputs are not copied from the previous
delivery's report.

**Mutation 1 — one `deny` becomes `allow`** (`"bash": "deny"` → `"allow"`):

```
test edge_permission_flipped_to_allow_is_rejected ... FAILED
test project_config_ships_at_the_repository_root ... FAILED
test project_config_pins_reviewer_read_only_and_verifier_privileged ... FAILED
test result: FAILED. 11 passed; 3 failed; 0 ignored
```

**Mutation 2 — a `default_agent` is inserted** pointing at `reviewer`:

```
test edge_permission_flipped_to_allow_is_rejected ... FAILED
test project_config_declares_no_default_agent ... FAILED
test project_config_ships_at_the_repository_root ... FAILED
test result: FAILED. 11 passed; 3 failed; 0 ignored
```

The third casualty in both runs is `check_file` seeing a document that no longer
satisfies the contract — the redundant path by design. The file was restored
from `target/tmp/opencode.json.bak` and confirmed byte-for-byte
(`cmp` → identical) before the final gate run. Every `expect_err` test likewise
fails when the checker stops rejecting, so none of them can pass vacuously.

## The one red test this run had to fix

`cargo test --all-targets` on the tree as delivered was **red**, before any edit
in this run:

```
unreviewed-work commits selected: 23 | acknowledged: 23 | unacknowledged: 1
  | stale entries: 1
  | UNACKNOWLEDGED: 05c77d1a0e2860b1970b4cee93577fefd462aa02  resume phase-084: adopt preserved work
  | STALE: ffa1f29ab36c05f452eca84b69f93b039c295abd
```

One defect, one line. The entry at `tests/unreviewed_work_ack_test.rs:88` named
`ffa1f29…` — a commit of the same subject from an earlier delivery of this phase,
which is not in this history (`git merge-base --is-ancestor
ffa1f29ab36c05f452eca84b69f93b039c295abd HEAD` → exit 1, measured this run). So
the entry was unacknowledged *and* the old one had gone stale, and the test
rejects stale entries precisely because "an entry pointing at nothing is how the
list decays into a comment".

The entry now names the SHA this history actually contains, with figures from
`git show --numstat 05c77d1` (+863/−0, 5 files, none under `src/`), and its
SHA is added to `MEASURED`, which is the list the same file requires to name
every commit it was measured on. No assertion was touched, nothing was skipped,
no threshold moved. After the change:

```
selected: 23 | acknowledged: 23 | unacknowledged: 0 | stale entries: 0
test result: ok. 13 passed; 0 failed; 0 ignored
```

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no output, no diff |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0, `Finished dev profile`, no warnings; no `allow(clippy::` added |
| `cargo test --all-targets` | **pass** — exit 0, **1571 passed, 0 failed, 0 ignored** across 60 test binaries (run with `--no-fail-fast` so nothing after a failure was hidden) |
| `./rbops/verify.sh phase-084` | **NOT RUN — absent from this checkout** |

On the fourth gate: `./rbops/verify.sh` does not exist here.
`ls -d rbops .opencode` reports `No such file or directory` for both, and
`find . -name verify.sh -not -path './target/*'` returns 0 results (both measured
this run). The pipeline that dispatched this phase lives outside the working
directory and the phase brief instructs that it not be inspected. Reported as
not run rather than claimed as passed.

What `verify.sh` enforces that can be checked by hand, all verified above: no
path under `.github/workflows/`, `rbops/`, `.opencode/agent/` or
`rbops/phases.json` is in the diff; no test is skipped, ignored, deleted or
weakened; no `allow(clippy::` was added; the diff touches `tests/`, the area this
phase declares; `REPORT.md` exists at the path the contract requires.

The `error` lines in the `cargo test` log are a Redblue test fixture's own
program output (an example printing its error token), not build errors — the run
exits 0 with `0` occurrences of `FAILED`.

## Test-requirement matrix

| Row | Status | Where / why |
|---|---|---|
| empty | covered | `edge_missing_agent_key_is_rejected` — `{}`; `edge_malformed_config_is_rejected` — empty file |
| singleton | covered | `edge_missing_reviewer_is_rejected` and `edge_missing_verifier_is_rejected` — exactly one agent declared |
| boundary | covered | `edge_duplicate_permission_key_resolves_to_the_last_value` — the boundary between the first and last occurrence of a repeated key, which is the boundary that decides whether the reviewer is writable |
| out_of_bounds | N/A | nothing here is indexable or ranged; no index is computed, so there is no out-of-bounds to reach. The nearest analogue — a tool key that is not one of the two declared — is covered by `edge_reviewer_without_permission_block_is_rejected` (key absent) and `edge_permission_value_of_wrong_type_is_rejected` (key present but unusable) |
| type_mismatch | covered | `edge_permission_value_of_wrong_type_is_rejected` — boolean, number, nested object and `null` where a string action belongs; `edge_agent_block_of_wrong_type_is_rejected` — array and string where an object belongs |
| numeric_boundary | N/A | the config schema admits no numeric field: `PermissionActionConfig` is the enum `["ask","allow","deny"]`. A number is rejected as a type mismatch, not a magnitude, so there is no numeric boundary to cross. Covered by `edge_permission_value_of_wrong_type_is_rejected` (the `bash: 0` case) |
| unicode | N/A | every key and every legal value in the schema is ASCII (`agent`, `reviewer`, `edit`, `allow`/`deny`/`ask`). No unicode key can be declared, and a unicode value is rejected as a type mismatch — `kind()` in `tests/agent_permissions_test.rs:56` names the type it found. Nothing in this change touches `src/`, where the language's own unicode handling is tested |
| nesting_recursion | covered | `edge_reviewer_without_permission_block_is_rejected` — `agent.reviewer` declared, `permission` one level down absent; `edge_permission_value_of_wrong_type_is_rejected` — a nested object where a flat action belongs |
| duplicate_missing_keys | covered | missing: `edge_reviewer_without_permission_block_is_rejected`, `edge_missing_reviewer_is_rejected`. duplicate: `edge_duplicate_permission_key_resolves_to_the_last_value` — JSON resolves a repeated `edit` last-wins, so a trailing `"allow"` is the one that counts |
| malformed_input | covered | `edge_malformed_config_is_rejected` — empty file, truncated object, trailing comma, UTF-8 BOM, top-level array, bare string; `edge_missing_config_file_is_rejected` — the file that is not there |
| resource_limit | N/A | the change is 21 lines of static JSON read once by a test; there is no loop, no recursion, no allocation that grows with input, and no external process. The only filesystem interaction is `std::fs::read_to_string` on a fixed 583-byte path. The phase's `must_touch` area is `tests/`, so no resource guard belongs here |

## Invariants touched

None. No file under `src/` was changed, no `.rb` example or module was touched,
and no test in `tests/` other than the two named above was modified. The
language surface in `phases/INVARIANTS.md` is untouched.

## Known gaps / follow-ups

- The tests assert a file's contents, not opencode's resolution of it. A
  dispatcher-side assertion on the resolved agent's permissions is what would
  close this, and `rbops/` is out of this phase's reach → FINDINGS.md.
- The acknowledgement treadmill: this phase's own resume commit has to be added
  to `ACKNOWLEDGED` after it lands, and it happened again this run → FINDINGS.md.