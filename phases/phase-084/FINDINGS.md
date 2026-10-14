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
its `ACKNOWLEDGED` data. **This has now cost three one-line repairs across three
deliveries of this phase**, one per run, and the cost is structural rather than
accidental. Each delivery's resume commit has a different SHA, the test rejects
both unlisted *and* stale entries, so exactly one line must be rewritten every
time and it can only be rewritten by whoever knows the SHA in advance — which is
the dispatcher, not the phase.

The sequence, recorded so the pattern is visible:

| delivery | commit carrying this phase's work | what the entry had to become |
|---|---|---|
| 1 | `55f71878d905105dfbdd715365f3f74aa6d88bb5` | first recorded |
| 2 | `d1f21633394cc27b5bcd35ec73bc4a14a16e5b3f` | repointed; `55f71878` went stale |
| 3 | `ffa1f29ab36c05f452eca84b69f93b039c295abd` | repointed; `d1f21633` went stale |
| 4 | `05c77d1a0e2860b1970b4cee93577fefd462aa02` | repointed again; `ffa1f29` went stale |

At every delivery `cargo test` was red *before any phase code ran*: once because
the new SHA was unlisted, and (from delivery 2 on) simultaneously because the
previous SHA was stale. The entry at `tests/unreviewed_work_ack_test.rs:88` now
names the SHA this history actually contains (`05c77d1…`, measured this run with
`git merge-base --is-ancestor ffa1f29 HEAD` → exit 1), its reason records the
three SHAs it replaces, and the same SHA was added to `MEASURED`, which is the
list that file requires to name every commit it was measured on. Delivery 4 is
the fourth consecutive observation of the same defect: this is now a measured
pattern, not a prediction.

**The fix is not another one-line repair.** Either the resume commit's subject
should avoid the `resume phase-` / `checkpoint` shapes the selector matches, or
the dispatcher should add the `ACKNOWLEDGED` line at commit time, where the SHA
is known. Both are outside `rbops/`, which this phase may not touch, so the
repair is repeated here and flagged. Evidence: the module docs at
`tests/unreviewed_work_ack_test.rs:1`, the list at
`tests/unreviewed_work_ack_test.rs:66`, the selector
`is_unreviewed_subject` at `tests/unreviewed_work_ack_test.rs:357`, and the
assertion at `tests/unreviewed_work_ack_test.rs:466`.

## The scratch directory's mode can present as six red tests in a file this phase does not own

The first `cargo test --all-targets` of this run reported 6 failures in
`src/repl/history.rs`, every one of them `Os { code: 13, kind: PermissionDenied }`
on the scratch path. The cause was the mode of `target/tmp/repl-history`
itself, which was on disk as `drw-------` — writable and traversable, **not
readable** — before the run started.

`fs::create_dir_all` at `src/repl/history.rs:136` creates that directory `0755`,
and nothing in `src/` or `tests/` ever sets permissions on it:
`grep -rn 'set_permissions\|from_mode' src/ tests/` matches exactly one line,
`tests/stdlib_modules_test.rs:451`, an unrelated `0o600` on its own fixture. So
the mode is an environment artefact, not a code defect. `chmod 700` on that
gitignored directory makes the binary report `18 passed; 0 failed`.

No test was changed, skipped or weakened to get here, and this cannot reproduce
in CI, where the directory is created by `create_dir_all`. It is recorded because
it presents as six red tests pointing at a file this phase did not touch, and the
next implementer will lose time the same way. A harness that seeds `target/tmp`
should create scratch directories readable, or the unit tests should assert the
directory's mode before using it.

## A transient shallow-history reading that was not a defect

`edge_phase_078_commits_are_ancestors_of_head_not_absent`
(`tests/unreviewed_work_ack_test.rs:519`) failed on that same first run with
"history is shallow and completing the clone did not make it complete". It passed
on every subsequent run, in isolation and in the full suite, and the repository
is not shallow:

```
$ git rev-parse --is-shallow-repository
false
$ ls .git/shallow
ls: cannot access '.git/shallow': No such file or directory
```

`is_shallow()` at `tests/unreviewed_work_ack_test.rs:243` shells out to that
exact command, so the only reading consistent with the panic is a moment when the
answer was `true`. Recorded because it is what the first output said, and because
"the audit refused to measure a truncated history" is a message that should not
be dismissed without saying so.

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
finds neither directory, and `find . -name verify.sh -not -path './target/*'`
returns nothing. The fourth gate was reported as NOT RUN in `REPORT.md` rather
than claimed as passing. The gates that were run: `cargo fmt --all -- --check`
clean, `cargo clippy --all-targets -- -D warnings` clean, `cargo test --all-targets`
1571 passed / 0 failed / 0 ignored across 60 binaries, exit 0.

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