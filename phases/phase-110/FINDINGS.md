# phase-110 — FINDINGS

Work outside this phase's concern, recorded so the auditor can promote it.

## 1. The table-parity test can only catch a one-sided omission

`tests/this_operand_test.rs::edge_the_two_operand_tables_name_the_same_token_kinds`
compares `Parser::kind_starts_operand` (`src/parser.rs:1851`) against
`is_operand_start` (`bootstrap/compiler.rb:1146`) by reading both as source text.
That catches the shape this phase fixed *going forward*: a token added to one
table and not the other.

It cannot catch a token missing from **both** — which is the shape that actually
bit, and the shape no other test in the suite can catch, because
`tests/differential_test.rs` and the byte-identity tests in
`tests/bootstrap_selfhost_test.rs` compare two engines that omit it identically.
A test comparing two wrong answers stays green; that is stated in the finding for
this phase and it remains true of the whole differential layer.

The fix that would actually close this is a *positive* list of what a worded
operand must be, derived from the primary table rather than restated beside it —
i.e. generate `kind_starts_operand` from `kind_starts_expression` minus an
explicitly excluded set, and assert the excluded set is exactly what it claims
(`MightFail`, and nothing else). Today the exclusion is documented in a doc
comment (`src/parser.rs:1836`) and enforced by `edge_the_operand_table_is_still_the_expression_table_less_might_fail`,
but only for the one token it names.

Worth its own phase. Not attempted here: it is a grammar-table refactor, and
AGENTS.md §1.8 forbids reworking the grammar inside a bug-fix phase.

## 2. Only `at` and `of` are covered; the other worded guards are unexamined

This phase widened the operand table, which gates the operand of `at` and of `of`
and nothing else. Any *other* place that decides "is this word an index or a
name" consults a different table, and those were not read. If a third worded
guard exists with its own operand table, `this` is probably still blind there.

Concretely: grep `src/parser.rs` for functions that test a token kind to decide
whether a word is a name or an operator, other than `kind_starts_operand`,
`at_index_ahead` (`src/parser.rs:2409`) and `parse_primary`. Each one found is a
candidate for the same bug, and each needs its own red test before any fix.

## 3. Two gates were already red before this phase touched anything

Both were cleared so the phase could be gated at all; both are recorded here
because they are not this phase's work and the reviewer should judge them as
second-phase edits.

### 3a. `tests/memory_model_test.rs` did not compile under clippy

`cargo clippy --all-targets -- -D warnings` failed with
`clippy::arc_with_non_send_sync` at `tests/memory_model_test.rs:22`, introduced by
commit `2948f23` ("Document the memory model"), one commit before this phase's
resume. It is a build-stopping error, not a warning, so it failed gate 2 and
`cargo test --all-targets` could not compile that target.

Fixed by moving the file's toy cycle helper (`struct Node`, `fn node()`) from
`Arc` to `Rc`. Every count assertion is unchanged and nothing was skipped or
allowed. The `Arc` at `tests/memory_model_test.rs:92-107` was deliberately left
alone: that one pins `Shared<T> = std::sync::Arc<T>` (`src/value.rs:276`) and
MEMORY.md line 26 claims it, so it must stay `Arc`.

**Candidate finding for the auditor:** any test that builds a non-`Send`/`Sync`
type inside an `Arc` trips this lint. `grep -rn "Arc::new" tests/` should be
audited for scaffolding that never crosses a thread; those are `Rc` candidates.

### 3b. `tests/unreviewed_work_ack_test.rs` had been replaced by a placeholder

The state on arrival at this run was worse than "one test failing":

* `cargo fmt --all -- --check` **failed** — the last function in the file was not
  rustfmt-shaped.
* The file's entire 927-line audit (13 tests) had been collapsed. Commit
  `222aa83` ("tests: replace invalid placeholder causing compile failure") wrote
  the whole file as `<the file content updated>` and then as a seven-line stub
  whose only test was
  `assert!(true, "placeholder test should be replaced with real assertions")`.
  The resume checkpoint `19f3f28` restored the module doc, the helpers and the
  data lists, but left that placeholder as the body of
  `unreviewed_work_in_head_history_is_acknowledged`. So the audit compiled, ran
  1 test, asserted nothing, and the file's fmt was wrong.

That is the "tests that cannot fail" shape AGENTS.md §5 tells the reviewer to
hunt for, and it was sitting in the test tree in the phase that was supposed to
be reviewing its own work.

Fixed by restoring the file from `e008f0b` (the last commit where it was whole),
which brings back the module doc, all 13 tests and their assertions, and removes
the placeholder. Two data rows were then needed in `ACKNOWLEDGED`, both because
they are commits the selector genuinely selects on HEAD and neither is named:

* `19f3f28` — this phase's own resume checkpoint.
* `c95062e` — "ack: resume phase-088: add resume commit to ACKNOWLEDGED", which
  is selected only because the selector is a case-insensitive substring test on
  `resume phase-` and the subject is *about* resume commits. Same shape as the
  `3fbf7396` entry that already carries a note saying so.

`99037af`, which the previous attempt acknowledged, is **not** an ancestor of
HEAD (`git merge-base --is-ancestor 99037af HEAD` → false), so that entry was
stale and was not carried forward. One data row added, one removed, no assertion
touched.

**Candidate finding for the auditor:** a commit subject ending in
"causing compile failure", or a test file reduced to `assert!(true, ...)`, is the
signature of a process that broke the tree and then silenced the alarm. `git log
--oneline -- tests/` should be audited for commits that *shrink* a test file by
more than a few lines without a matching production change.

**This is structurally self-perpetuating:** every resume checkpoint commit lands
unacknowledged the moment it is committed, so the gate fails on the very next run
unless the phase that adopts it repoints the list. Worth a phase that makes the
acknowledgement part of the checkpoint commit itself.

## 4. `tests/preflight_assert_failure.rs` is a test that cannot fail

`tests/preflight_assert_failure.rs` (commit `3d85942`, "ci: add minimal
`#[should_panic]` test to satisfy rbops preflight (phase-090)") is the whole file:

```rust
#[test]
#[should_panic]
fn rbops_gate_requires_a_test_that_asserts_failure() {
    panic!("intentional panic to satisfy rbops preflight detection");
}
```

Its own comments say it "exists solely to satisfy rbops' preflight heuristic".
It exercises no Redblue code, asserts nothing about the language, and passes
whatever the product does. It is exactly what AGENTS.md §3.1 calls a non-test and
§5 tells the reviewer to look for.

It is **not** deleted here: it is not this phase's concern (AGENTS.md §1.7), it
was in HEAD before this phase started, and deleting a green test would itself be
invisible to the gate. Recorded for the auditor.

Note that phase-110 does **not** depend on it. The phase's own
"asserts a failure is produced" requirement is met by
`edge_length_of_this_and_length_of_nothing_are_refused_by_the_same_rule` and
`edge_this_as_the_operand_of_at_is_an_index_with_a_bad_index`, which assert on
`Err` values the product actually returned.

## 5. Gate 4 was not run

`./rbops/verify.sh phase-110` — `rbops/` is not in this checkout and this agent
is confined to the working tree, so the script could not be executed. The first
three gates were run and are reported in `REPORT.md` as observed. Noted here so
it is not mistaken for a pass.
