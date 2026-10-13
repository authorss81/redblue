# phase-051 FINDINGS — the phase-051 finding is stale; it does not reproduce

## Verdict

**STALE. Not fixed, because there is nothing to fix.** Every load-bearing clause of
phase-051's evidence is false against `main` at `7395c24`. The uncommitted S3 work it
describes was adopted into history by *later* phases, which is precisely the outcome
the phase was created to force. Per the phase prompt's own instruction — *"If the
finding no longer reproduces on `main`, stop and write that to `FINDINGS.md` — a stale
phase must never be 'fixed' by inventing a change"* — no source file was touched.

## Clause-by-clause re-verification

All commands run in `/home/runner/work/rbops/rbops/redblue` at `HEAD = 7395c24`.

| # | Evidence clause | Expected if true | Actual output | Reproduces? |
|---|---|---|---|---|
| 1 | "`git -C redblue status --short` shows SEVEN modified files" | 7 lines | *(no output)* | **no** |
| 2 | "941 insertions and 335 deletions that are NOT in any commit" | large diff | `git diff HEAD --stat` → *(empty)* | **no** |
| 3 | "`cargo test` is green partly BECAUSE the working tree carries 393 lines of uncommitted test code" | — | clean tree → **1463 passed, 0 failed**, exit 0 | **no** |
| 4 | "`git log --oneline main \| grep phase-022` prints exactly one line" | 1 line | prints **2**: `054e0d6`, `dff04f6` | **no** |
| 5 | "its Tests-added table lists four tests that appear nowhere in dff04f6's 13-file diff" | absent from history | all four present in history, see below | **partly** |
| 6 | "no test named in its REPORT is present in it [the commit history]" | absent | all four present, passing from a fresh clone | **no** |
| 7 | "The S3 fixed point that phase-022 exists to prove is not in the commit history at all" | absent | `edge_the_self_hosted_compiler_compiles_itself_byte_identically` is in history | **no** |

Clauses 1–4 and 6–7 are flatly false. Clause 5 is true but was never the defect: the
tests are absent from `dff04f6` specifically, because `dff04f6` is the *resume* commit
for a checkout whose test file already existed. The finding generalised "not in that one
commit" into "not in any commit", and that generalisation is where it went wrong.

### Clause 1 and 2, verbatim

```console
$ git status --short
$ git status --short --untracked-files=all
$ git diff HEAD --stat
$ git stash list
```

All four are empty. `git status --short --ignored` returns exactly one line, `!! target/`,
which is build output and is `.gitignore`d. There is no uncommitted, no untracked and no
stashed work in this checkout.

### Clause 5's four tests — present, committed, and passing

The names in `phases/phase-022/REPORT.md:38-41`, checked in a **fresh clone** of `7395c24`:

| Test | `grep -c "fn <name>"` in the clone |
|---|---|
| `edge_the_self_hosted_compiler_compiles_itself_byte_identically` | 1 |
| `edge_three_consecutive_self_compilations_are_byte_identical` | 1 |
| `edge_the_self_compilation_still_obeys_the_step_budget` | 1 |
| `edge_a_while_loop_nested_in_another_patches_its_own_jump` | 1 |

Run there, not in the original working tree:

```console
$ git clone --no-local . target/tmp/freshclone
$ cd target/tmp/freshclone && git log --oneline -1
7395c24 rbops: phase-050 (2026-10-12T21:46:56Z)
$ CARGO_TARGET_DIR=.../freshclone-target cargo test --test bootstrap_selfhost_test -- \
    edge_the_self_hosted_compiler_compiles_itself_byte_identically \
    edge_three_consecutive_self_compilations_are_byte_identical \
    edge_the_self_compilation_still_obeys_the_step_budget \
    edge_a_while_loop_nested_in_another_patches_its_own_jump

running 4 tests
test edge_the_self_compilation_still_obeys_the_step_budget ... ok
test edge_a_while_loop_nested_in_another_patches_its_own_jump ... ok
test edge_the_self_hosted_compiler_compiles_itself_byte_identically ... ok
test edge_three_consecutive_self_compilations_are_byte_identical ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 16 filtered out; finished in 136.69s
```

### What actually happened (the adoption the phase asked for)

The work reached `main` through the pipeline's own `resume phase-NNN: adopt preserved
work` path, one phase at a time, instead of in the single phase-022-named commit the
finding expected:

```
054e0d6 rbops: phase-022                       <- tests/bootstrap_selfhost_test.rs (+393)
dff04f6 resume phase-022: adopt preserved work <- bootstrap/compiler.rb, src/interpreter.rs,
                                                 src/runtime.rs, phases/phase-022/*
564fdf2 rbops: phase-021
8910456 resume phase-037: adopt preserved work <- tests/bootstrap_selfhost_test.rs
e26659d resume phase-048: adopt preserved work <- bootstrap/compiler.rb, tests/bootstrap_selfhost_test.rs
10f9750 resume phase-050: adopt preserved work <- src/interpreter.rs, src/runtime.rs
```

Every file in `phases/phase-022/REPORT.md`'s *What changed* table is committed:

| File | Last commit touching it |
|---|---|
| `bootstrap/compiler.rb` | `e26659d` resume phase-048: adopt preserved work |
| `src/interpreter.rs` | `10f9750` resume phase-050: adopt preserved work |
| `src/runtime.rs` | `10f9750` resume phase-050: adopt preserved work |
| `tests/bootstrap_selfhost_test.rs` | `e26659d` resume phase-048: adopt preserved work |
| `docs/BOOTSTRAP.md` | `daeb73b` rbops: phase-023 |
| `phases/phase-022/FINDINGS.md` | `054e0d6` rbops: phase-022 |

So the *outcome* phase-051 exists to secure is already true. Only its *timing* differed
from what the finding assumed.

## §1 — The process defect behind this finding

**phase-022 is the phase that added a REPORT row about an uncommitted file.** It is
recorded here so the next one does not repeat it.

`phases/phase-022/REPORT.md:13` lists `tests/bootstrap_selfhost_test.rs | +393 −0`, and
lines 38–41 name four tests, while the commit it shipped under (`dff04f6`) touches 13
files, none of them the test file. The REPORT was written against the **working tree**,
not against **HEAD**. The tests were real — they were in the tree and they passed — but
they were not yet in history, so `dff04f6` does not contain them.

Three things let that stand:

1. **No check ties a REPORT to a diff.** `AGENTS.md` §4 asks for a *What changed* table
   and gates `cargo test`/fmt/clippy. Nothing verifies that each named file has a diff in
   the phase's own commit. A green gate on an uncommitted tree satisfies every check
   there is.
2. **The test quota and the gates both read the working tree.** So the working tree is
   the only thing a phase can be graded against, which makes "committed" and "real"
   indistinguishable to the gate.
3. **Adoption is by a later phase, invisibly.** The "resume phase-NNN: adopt preserved
   work" commits do land the files, but nothing records that they were late, so the
   original REPORT keeps reading as though `dff04f6` contained them.

The gate is cheap and mechanical and cannot be smarter than this in the pass direction.
The reviewer is the ceiling — and this is a case where only a reviewer comparing
`REPORT.md` against the commit diff catches it, which is why §5 lists *"REPORT.md claims
a gate passed that it did not"* as a hunt target. This finding is that review working,
arriving eleven phases late.

## §2 — Manifest bug: `must_touch: ["bootstrap/"]` is unsatisfiable for phase-051

`AGENTS.md` §3.5 fails a phase whose diff touches no declared area. phase-051 declares
`must_touch: ["bootstrap/"]`. Since the S3 work is already committed and verified, the
**only** way to touch `bootstrap/` is to change `bootstrap/compiler.rb` without cause —
i.e. to invent a change, which §1 rule 8 and the phase prompt both forbid, and which
would rewrite verified bootstrap bytes for no reason.

This is a manifest bug, named here per `AGENTS.md` §3.5. The auditor should either:

- **retire phase-051** — its goal is met; or
- **re-scope it** to a defect that genuinely exists in `bootstrap/`, if one does; or
- **add the check it wants** — see §3 — rather than a `must_touch` that can only be
  satisfied by churn.

I did not bend the phase to reach the area. That is the explicit instruction in
`AGENTS.md` §3.5 and I am following it, at the cost of failing the `must_touch` gate.

## §3 — Follow-up the auditor should promote

**A phase-051-era gate: `REPORT.md`'s *What changed* rows must exist in the phase's own
commit.** Mechanical, and it would have caught this at phase-022 instead of phase-051:

```bash
# for each path in the phase REPORT's "What changed" table
git show --stat --format= HEAD -- "$path" | grep -q "$path" || echo "UNCOMMITTED: $path"
```

This is a real gap, not a restatement: it is the check whose absence let `phase-022`'s
REPORT and `dff04f6` disagree for eleven phases. It belongs to the pipeline, which I may
not touch (`AGENTS.md` §1 rule 1), so it goes here for the auditor.

## §4 — A typo in phase-051's own prompt

The prompt names `edge_a_while_loop_nested_in_another_patches_itself_byte_identically`.
No such test exists and none ever has. The test named in
`phases/phase-022/REPORT.md:41` is
`edge_a_while_loop_nested_in_another_patches_its_own_jump`, and it is present and green.
A grep for the prompt's spelling returns 0 and would have been read as "still missing".
The remaining three names in the prompt are exact.