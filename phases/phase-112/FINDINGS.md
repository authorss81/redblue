# phase-112 — FINDINGS

Work discovered while finishing this phase that does **not** belong to it, recorded
per AGENTS.md §1.7. The first item was repaired anyway, because the gate could not
be green otherwise; that decision is stated explicitly so the reviewer can judge it.

---

## 1. MAJOR — a real test file had been replaced by `assert!(true)`

**Where:** `tests/unreviewed_work_ack_test.rs`.

**What happened.** The file in `HEAD` before this run contained 438 lines of helper
machinery and exactly one test, at the bottom:

```rust
#[test]
fn unreviewed_work_ack() {
    assert!(true, "placeholder test should be replaced with real assertions");
}
```

`git show c5e010b^:tests/unreviewed_work_ack_test.rs` is that same 7-line stub.
Its own comment says `TODO: replace with real assertions that exercise the
intended behavior` — the task was left undone, not completed.

The 12 tests that file used to hold were recoverable only from
`git show e8bc726:tests/unreviewed_work_ack_test.rs` (932 lines, 13 `#[test]`).
Commit `222aa83` — authored by `authorss81`, i.e. **not** the rbops bot — is what
replaced the file with the stub:

```
-<the file content updated>\ No newline at end of file
+// Minimal replacement for an invalid placeholder that introduced a syntax error
```

So the history reads: the file was authored, then a human replaced all 932 lines
with a 7-line placeholder to work around a syntax error, and the resume commit
`c5e010b` re-imported the helpers but **not** the tests.

This is the AGENTS.md §5 "fake completion" pattern — a test that cannot fail — even
though phase-112 did not author it. It would have been a BLOCKER review finding
against this phase had the reviewer not diffed it.

**Repaired here** because `unreviewed_work_in_head_history_is_acknowledged` was
missing entirely, and restoring it is what surfaced finding 2 below. All 13 tests
are back with their original assertions, unmodified.

## 2. MAJOR — the acknowledgement list named a commit that is not in this history

**Where:** `tests/unreviewed_work_ack_test.rs`, `const ACKNOWLEDGED`.

**What happened.** The resume commit `c5e010b` added an entry for
`e8bc7262bd0a4d8220832909aba145737910a85b`, and the `REPORT.md` it shipped claimed
this "fixed" a stale acknowledgement. It did not — it inverted the failure:

```
$ git merge-base --is-ancestor e8bc726 HEAD   # -> exit 1, NOT an ancestor
$ git merge-base --is-ancestor c5e010b HEAD   # -> exit 0
```

`e8bc726` is the *abandoned* attempt `c5e010b` replaced. With it in the list the
restored test fails exactly as the list's own design intends:

```
1 entry/entries name a commit HEAD's history does not contain: e8bc7262bd0a4...
```

**Repaired here:** the `e8bc726` entry is gone from both `ACKNOWLEDGED` and
`MEASURED`; `c5e010bb3126d150ba24d354c69faf676fd4b912` is listed instead, with its
measured line counts. `c95062eb02a6e336f011a918daac7938f0efee6d` ("ack: resume
phase-088") is also newly acknowledged — its subject merely *contains*
`resume phase-`, so the substring selector picks it up, and it was being missed.

This is a note about the selector, not a fix to it: `is_unreviewed_subject` is
documented as a deliberate substring test and I have not touched it.

## 3. MAJOR — pre-existing clippy failure blocks the gate

**Where:** `tests/memory_model_test.rs:22`.

```
error: usage of an `Arc` that is not `Send` and `Sync`
  --> tests/memory_model_test.rs:22:5
   |
22 | /     Arc::new(Node {
   | |         peer: RefCell::new(None),
   | |     })
   = note: `Arc<Node>` is not `Send` and `Sync` as `Node` is neither
   = note: `-D clippy::arc-with-non-send-sync` implied by `-D warnings`
```

Introduced by `2948f23 "Document the memory model, and pin every claim in it"`,
which is **not** phase-112 work, and it is not caused by anything this phase
touched — the file does not appear in this phase's diff. It surfaces on
`clippy 0.1.99 (2026-10-13)`; the lint is newer than the commit.

`cargo clippy --all-targets -- -D warnings` therefore exits 101 on arrival, so no
phase can currently pass the gate on this branch. **That is the reason this was
repaired in phase-112 rather than deferred** — a red gate fails every phase, and
leaving it red while claiming otherwise would be worse. The repair is confined to
`tests/`, which is in this phase's declared `must_touch`.

**Repair:** the `Node` cycle scaffolding is now `Rc` (`RefCell` is not `Send`, so
the `Arc` was wrong), while the assertions about Redblue's own `Value` stay `Arc`
— those are the claims about the shipped representation and are untouched. No
assertion changed, no test removed, no `#[allow]` added.

**For the auditor:** this deserves its own phase. It is a test-only fix with no
bearing on `else if`, and it is currently buried in phase-112's diff where a
reviewer will read it as scope creep.

## 4. `rb format` and `rb lint` were not exercised against a chain

The formatter walks the AST, and a chain is now an ordinary nested `If`, so it
should format as one — but no test in this phase pins that, and
`edge_the_repl_block_depth_reads_a_chain_as_one_block_and_a_nested_if_as_two` shows
`else` *does* have line-sensitive handling in at least one place (the REPL's block
accounting). Worth measuring before anyone writes `else if` into `examples/`.

## 5. `else if` across a line continuation is not supported, and that is undocumented at the point of use

`else` then a newline then `if` is always a statement of the else block with its own
`end`. A user who writes it expecting a chain gets a nested `if` that still means
what they wanted, so it is a surprise of shape rather than of behaviour — but
SPEC.md and docs/GRAMMAR.md state the same-line rule only in the added paragraph,
not next to the production. Both files do state it; the placement is the gap.

## 6. The six corpus programs the frontend still refuses are untouched

Unchanged by this phase; still recorded in `phases/phase-021/FINDINGS.md` §2.