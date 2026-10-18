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

### 3b. `unreviewed_work_in_head_history_is_acknowledged` failed on HEAD itself

The test selects every commit in HEAD's history whose subject matches the
unreviewed-work selector and requires a written acknowledgement for each. The
phase's own resume commit `99037af` was unacknowledged, and the entry then in the
file named `8a5587d`, which is *not* an ancestor of HEAD and was therefore stale
— and a stale entry is a failure too. `cargo test --all-targets` was therefore
failing on this test at the start of the run.

Fixed by repointing the entry at `99037af` with a reason, the same data repair
phase-089 and earlier phases made. One data row, no assertion touched.

**This is structurally self-perpetuating:** every resume checkpoint commit lands
unacknowledged the moment it is committed, so the gate fails on the very next
run unless the phase that adopted it repoints the list. Worth a phase that makes
the acknowledgement part of the checkpoint commit itself.

## 4. Gate 4 was not run

`./rbops/verify.sh phase-110` — `rbops/` is not in this checkout and this agent
is confined to the working tree, so the script could not be executed. The first
three gates were run and are reported in `REPORT.md` as observed. Noted here so
it is not mistaken for a pass.
