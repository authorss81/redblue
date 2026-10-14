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

## Verification note for whoever runs `rbops/verify.sh`

`./rbops/verify.sh` does not exist in this checkout — `test -x rbops/verify.sh`
returns absent, and `ls -d rbops .opencode` finds neither directory. The fourth
gate was reported as NOT RUN in `REPORT.md` rather than claimed as passing. The
gates that were run: `cargo fmt --all -- --check` clean, `cargo clippy
--all-targets -- -D warnings` clean, `cargo test --all-targets` 1569 passed /
0 failed / 0 ignored.

The shipped `opencode.json` was validated against the live published schema at
<https://opencode.ai/config.json> using `jsonschema.validate`: **SCHEMA VALID**.
opencode hard-fails to start on an invalid config, so this was checked before
the file was committed rather than assumed.