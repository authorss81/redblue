# Phase 084 — findings for the auditor

## The test cannot prove opencode's resolution, only the file's contents

`tests/agent_permissions_test.rs` reads `opencode.json` and asserts its
contents. It never runs `opencode`, so it cannot prove which config opencode
opened or that a given build maps `--agent reviewer` to the declaration in this
checkout. A future opencode that reordered its resolution would leave the suite
green with the reviewer writable again.

Closing it means a change in `rbops/dispatch.sh`, which this phase may not
touch. The check belongs where the name is bound: before handing the diff to
the reviewer, resolve the agent and assert `permission.edit == "deny"` and
`permission.bash == "deny"`. That turns a silent fallback into a loud failure.

## `reviewer` is declared `mode: "primary"`; nothing here can confirm that

`opencode.json` declares both `reviewer` and `verifier` as `mode: "primary"`,
on the reading that `--agent <name>` on the command line names a primary agent.
The phase evidence quotes `rbops/dispatch.sh:786` as invoking
`--agent reviewer`, but `rbops/` is not in this checkout and the phase brief
forbids inspecting the pipeline, so the invocation could not be verified here.

If dispatch actually invokes these as subagents, the mode is wrong and opencode
would not offer the agent in the path being used. `mode: "all"` covers both
readings at the cost of precision. This is a one-word change to
`opencode.json` once the invocation is confirmed.

## Only `edit` and `bash` are pinned

The phase names four values and this phase pins exactly those. `reviewer` is
granted no `webfetch` and no `write`, so both fall back to the global
`~/.config/opencode/opencode.json` defaults — which, per the phase evidence,
are permissive for `edit` and `bash` and unmeasured for everything else. A
read-only reviewer arguably should also `deny` `write`, `webfetch` and
`external_directory`, but that is broader than this phase's finding and would
be a change to what the reviewer can do, which belongs in its own phase.

## Every resume commit costs one line in `ACKNOWLEDGED`

`tests/unreviewed_work_ack_test.rs` fails on any commit in `HEAD`'s history
whose subject contains `resume phase-` or `checkpoint` and that is not listed in
its `ACKNOWLEDGED` data. The resume commit carrying this phase's own work,
`d1f21633394cc27b5bcd35ec73bc4a14a16e5b3f`, was unlisted, so `cargo test` was
red at that commit before any phase code ran. The same line had carried an
earlier SHA, `55f71878d905105dfbdd715365f3f74aa6d88bb5`, from a previous delivery
of this phase; that SHA is not in this history, so the entry was *also* stale and
both halves of the test failed at once. The entry at
`tests/unreviewed_work_ack_test.rs:88` now names the SHA this history actually
contains.

The test also rejects *stale* entries, so the next one cannot be pre-registered:
the dispatcher, not the phase, is the component that can add the line, because
only it knows the SHA the commit will get. If a phase is gated on `cargo test`
after its own commit lands, either the commit subject avoids `resume phase-` /
`checkpoint`, or `ACKNOWLEDGED` gains that SHA's line first. Evidence: the
module docs at `tests/unreviewed_work_ack_test.rs:1`, the list at
`tests/unreviewed_work_ack_test.rs:66`, the selector
`is_unreviewed_subject`, and the assertion in
`tests/unreviewed_work_ack_test.rs:466`.

## Verification note for whoever runs `rbops/verify.sh`

`./rbops/verify.sh` does not exist in this checkout — `ls -d rbops .opencode`
finds neither directory. The fourth gate was reported as NOT RUN in `REPORT.md`
rather than claimed as passing. The gates that were run, this session:
`cargo fmt --all -- --check` clean, `cargo clippy --all-targets -- -D warnings`
clean, `cargo test --all-targets` 1571 passed / 0 failed / 0 ignored, exit 0.

The shipped `opencode.json` was validated against the live published schema at
<https://opencode.ai/config.json> using `jsonschema` 4.10.3 (Draft 2020-12, with
the remote `models.dev` `$ref` this document does not reach stripped for offline
validation): **SCHEMA VALID**. Re-run independently this session, same result.
The schema confirms the three claims the fix rests on — `Config.agent.additionalProperties`
is `AgentConfig` (arbitrary agent names legal), `AgentConfig.mode` enumerates
`["subagent","primary","all"]`, and `AgentConfig.permission` accepts `edit` and
`bash` from the `PermissionActionConfig` enum `["ask","allow","deny"]`. opencode
hard-fails to start on an invalid config, so this is checked before the file was
committed rather than assumed.