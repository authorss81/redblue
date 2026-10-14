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

The finding reproduces: the parent of this phase's own commit shipped **no**
opencode config at any project scope. Project config is `./opencode.json` (or
`.jsonc`, or `.opencode/opencode.json`) read from the working directory outward;
global config is `~/.config/opencode/opencode.json`; the two are deep-merged with
the project overriding the global. With no project file, `--agent reviewer` and
`--agent verifier` resolve nothing in the project scope and inherit the global
`default_agent: build`, whose `edit` and `bash` are `allow`. The reviewer is
contractually read-only, and the only thing expressing that was prose in a
prompt. A reviewer that can edit the phase it reviews defeats its own purpose,
and no gate notices, because nothing the gate runs asks the reviewer to write
something.

The fix landed in `ffa1f29`. **This run made no production change.** It ran the
gates on the tree as delivered, fixed the one thing that was red, and closed the
one reporting gap the previous `REPORT.md` carried.

## What changed

| File | Lines | What |
|---|---|---|
| `opencode.json` | +21 −0 | **New**, at the repository root (commit `ffa1f29`). `agent.reviewer.permission` = `edit: "deny"`, `bash: "deny"`; `agent.verifier.permission` = `edit: "allow"`, `bash: "allow"`. No `default_agent`. |
| `tests/agent_permissions_test.rs` | +395 −0 | 14 `#[test]` functions (carried from `ffa1f29`). Reads the shipped file and asserts all four permission values; 11 drive the pure checker over deliberately broken documents and assert a failure is produced. |
| `tests/unreviewed_work_ack_test.rs` | +1 −1 | Acknowledgement entry repointed at `ffa1f29ab36c05f452eca84b69f93b039c295abd`. See "The one red test this run had to fix" below. |
| `phases/phase-084/REPORT.md` | rewritten | This file. |
| `phases/phase-084/FINDINGS.md` | amended | Two new entries: the scratch-directory mode that makes the suite look red, and the transient shallow-history reading. |

This run's own diff is **one line in one file**, under `tests/`, satisfying this
phase's `must_touch: ["tests/"]`. No file under `src/`, `rbops/`,
`.github/workflows/`, `.opencode/agent/` or `rbops/phases.json` was touched.

```
$ git diff --stat
 tests/unreviewed_work_ack_test.rs | 2 +-
 1 file changed, 1 insertion(+), 1 deletion(-)
$ git diff --name-only | grep -c '^src/'
0
```

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

Citation: opencode's configuration reference, section **"Where files live"** —
the directory-of-configs table, published alongside the JSON Schema at
<https://opencode.ai/config.json>. The machine-checkable half of the citation is
that schema, which the shipped file was validated against: `Config.agent` is an
object whose additional properties are `AgentConfig`, so arbitrarily named
agents are legal; `AgentConfig.mode` enumerates `["subagent","primary","all"]`;
and `AgentConfig.permission` accepts `edit` and `bash` with values from the
`PermissionActionConfig` enum `["ask","allow","deny"]`. opencode hard-fails to
start on a config that fails this schema, so it is checked rather than assumed.

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
(`expect_err` on a `Result`) — counted, not estimated. The five failing documents
the phase requires are present as separate tests: file missing, `agent` key
missing, `reviewer` missing, `verifier` missing, one permission flipped to allow.

**This run added no new `#[test]` function.** It adopted the 14 above from
`ffa1f29` and verified by mutation that they bite (below). Adding more would
have been padding, not coverage.

### Mutation check — the shipped file itself, mutated, run, observed, restored

Because the phase's definition of done is phrased about the *file*, the shipped
`opencode.json` was mutated on disk and the suite was run against it.

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
test edge_permission_flipped_to_allow_is_rejected ... FAILED
test project_config_ships_at_the_repository_root ... FAILED
test result: FAILED. 11 passed; 3 failed; 0 ignored
```

The third casualty in both runs is `check_file` seeing a document that no longer
satisfies the contract — the redundant path by design. Restored and confirmed
byte-for-byte:

```
$ sha1sum opencode.json target/tmp/opencode.json.bak
6155ee48d7f18ef30a483f9dcb74a2515ef92c07  opencode.json
6155ee48d7f18ef30a483f9dcb74a2515ef92c07  target/tmp/opencode.json.bak
$ cargo test --test agent_permissions_test
test result: ok. 14 passed; 0 failed; 0 ignored
```

Every `expect_err` test likewise fails when the checker stops rejecting, so none
of them can pass vacuously.

## The one red test this run had to fix

`cargo test --all-targets` on the tree as delivered was **red**, before any edit
in this run:

```
unreviewed-work commits selected: 23 | acknowledged: 23 | unacknowledged: 1
  | stale entries: 1
  | UNACKNOWLEDGED: ffa1f29ab36c05f452eca84b69f93b039c295abd  resume phase-084: adopt preserved work
  | STALE: d1f21633394cc27b5bcd35ec73bc4a14a16e5b3f
```

Two defects, one line. The entry at `tests/unreviewed_work_ack_test.rs:88`
named `d1f2163…` — a commit of the same subject from an earlier delivery of this
phase, which is not in this history (`git merge-base --is-ancestor
d1f21633394cc27b5bcd35ec73bc4a14a16e5b3f HEAD` → exit 1). So the entry was
unacknowledged *and* the old one had gone stale, and the test rejects stale
entries precisely because "an entry pointing at nothing is how the list decays
into a comment".

The entry now names the SHA this history actually contains, with figures from
`git show --numstat ffa1f29` (+757/−0, 5 files, none under `src/`), and the
reason records both SHAs it replaces. No assertion was touched, nothing was
skipped, no threshold moved. After the change:

```
selected: 23 | acknowledged: 23 | unacknowledged: 0 | stale entries: 0
test result: ok. 13 passed; 0 failed; 0 ignored
```

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no output, no diff |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0, `Finished dev profile in 16.69s`, no warnings; no `allow(clippy::` added (`git diff \| grep -c "allow(clippy::"` → `0`) |
| `cargo test --all-targets` | **pass** — exit 0, **1571 passed, 0 failed, 0 ignored** across 60 test binaries (`grep -c FAILED` on the log → `0`) |
| `./rbops/verify.sh phase-084` | **NOT RUN — absent from this checkout** |

On the fourth gate: `./rbops/verify.sh` does not exist here.
`ls -d rbops .opencode` reports `No such file or directory` for both, and
`find . -name verify.sh -not -path './target/*'` returns nothing. The pipeline
that dispatched this phase lives outside the working directory and the phase
brief instructs that it not be inspected. Reported as not run rather than claimed
as passed.

What `verify.sh` enforces that can be checked by hand, all verified above: no
path under `.github/workflows/`, `rbops/`, `.opencode/agent/` or
`rbops/phases.json` is in the diff; no test is skipped, ignored, deleted or
weakened (the diff contains no `#[ignore]`, `// skip` or `.skip(`); no
`allow(clippy::` was added; the diff touches `tests/`, the area this phase
declares; `REPORT.md` exists at the path the contract requires.

The `error` lines in the `cargo test` log are a Redblue test fixture's own
program output (a minesweeper-style example printing its error token), not
build errors — the run exits 0 with `0` occurrences of `FAILED`.

## A second red run that was the environment, not the code

The first `cargo test --all-targets` also reported 6 failures in
`src/repl/history.rs`:

```
thread 'repl::history::tests::save_to_file_and_load_from_file_round_trip' panicked at src/repl/history.rs:251:14:
history should be writable to the scratch path: Os { code: 13, kind: PermissionDenied, ... }
```

The cause is the scratch directory itself, which was on disk as `drw-------` —
writable and traversable, **not readable** — before this run started:

```
$ ls -ld target/tmp/repl-history
drw------- 2 runner runner 4096 Oct 14 17:26 target/tmp/repl-history
```

`fs::create_dir_all` at `src/repl/history.rs:136` creates it `0755`; nothing in
`src/` or `tests/` ever sets permissions on it (`grep -rn 'set_permissions'
src/ tests/` matches only `tests/stdlib_modules_test.rs:451`, an unrelated
`0o600` on its own fixture). The mode is a session-setup artefact. After
`chmod 700`, the same binary reports `18 passed; 0 failed`. **No test was
changed, skipped or weakened** — the mode on a gitignored scratch directory was
restored so the suite could run. This cannot reproduce in CI, where the directory
is created by `create_dir_all`. Recorded in FINDINGS.md because it presents as
six red tests in a file this phase does not own.

One other transient, recorded for the same reason:
`edge_phase_078_commits_are_ancestors_of_head_not_absent` failed on that first
run with "history is shallow and completing the clone did not make it complete".
It passed on every subsequent run, and the repository is not shallow:

```
$ git rev-parse --is-shallow-repository
false
$ ls .git/shallow
ls: cannot access '.git/shallow': No such file or directory
```

That test lives in `tests/unreviewed_work_ack_test.rs`, not in this phase's
code, and it is not modified here.

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

None. No language surface changed: no change to `.rb`, to `to … end`, to
`set x to …`, to `say`, to the `Value` variants, or to the `Error` variants. No
file under `src/` was modified at all. `examples/*.rb` and `modules/*.rb` are
untouched and keep working.

## Known gaps / follow-ups

- The test cannot prove opencode's *resolution*, only the file's contents →
  FINDINGS.md
- `reviewer` is declared `mode: "primary"`; `rbops/dispatch.sh` is not in this
  checkout, so the `--agent reviewer` invocation could not be confirmed →
  FINDINGS.md
- Only `edit` and `bash` are pinned; `write`, `webfetch` and
  `external_directory` still fall through to the global file → FINDINGS.md
- Every resume commit costs one line in `ACKNOWLEDGED`, and the next one cannot
  be pre-registered because only the dispatcher knows the SHA → FINDINGS.md
- `target/tmp/repl-history` may arrive with mode `0200` and present as six red
  tests in a file this phase does not own → FINDINGS.md