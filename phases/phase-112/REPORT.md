# Phase 112 — Make `else if` parse, as SPEC.md and docs/GRAMMAR.md both promise

## Finding re-verified before anything changed

`else if` was a `ParserError` in every position on `main`: `parse_if` consumed
`TokenKind::Else` and dropped into a plain statement loop, so the `if` after it
parsed as a *nested* `if`, that nested `if` ate the `end` belonging to the outer
one, and the outer `parse_if` then failed `self.expect(TokenKind::End)`.

Re-verified against the built binary before any change, per the phase prompt:
as the last arm it reported `ParserError: Expected 'end' but got Eof`; as a middle
branch with a trailing `else`, and as two chained arms with a trailing `else`, the
same error at the later line.

## What changed

Two commits of work. The first (`c5e010b`) is the preserved attempt, adopted as-is.
The second is this run, and it is **mostly gate repair** — see the note below.

| File | Lines | What |
|---|---|---|
| src/parser.rs | +126 −42 | `parse_if` desugars an `else if` chain into nested `if`s, one block level per arm; `parse_if_arm_body` is the shared arm-body loop; `open_block_depth` reads a chain as the one block its single `end` closes |
| bootstrap/compiler.rb | +44 −17 | the stage-2 Redblue compiler parses the same chain (`parse_if_else` / `parse_if_arm`), so stage 1 and stage 2 stay byte-identical |
| tests/else_if_test.rs | +605 | new — 20 tests, 13 of them `edge_*` |
| tests/differential_test.rs | +63 −19 | `edge_the_control_flow_family_covers_the_if_else_it_has_and_pins_the_else_if_it_does_not` → `..._runs_the_else_if_it_writes_on_both_engines`: the same three corpus programs, no longer pinned as refusals |
| tests/bootstrap_selfhost_test.rs | +18 −15 | frontend refusal count 9 → 6 (`control-flow-000{3,4,5}` now parse) |
| corpus/control-flow-000{3,4,5}.expected | +9 −9 | regenerated off the now-running programs (`a`, `b`, `c`) |
| SPEC.md | +20 −0 | the `else if` rule, worked |
| docs/GRAMMAR.md | +22 −0 | `else if` written into the `if_statement` production |
| tests/doc_blocks_{runs,known_fail,fragments}.txt | +61 −60 | re-pinned for the SPEC.md shift |
| **tests/unreviewed_work_ack_test.rs** | **+512 −17** | **this run: 12 deleted tests restored, stale ack entry corrected** |
| **tests/memory_model_test.rs** | **+31 −14** | **this run: pre-existing clippy failure repaired** |

### This run was mostly gate repair, not new feature work

Continuing from the interrupted attempt turned up two things that were wrong in
what was already committed, both of which would have failed review as fake
completion. Full detail in `FINDINGS.md`; the short form:

1. **`tests/unreviewed_work_ack_test.rs` had been reduced to a stub.**
   `cargo fmt --check` was red on it, and it ended in
   `assert!(true, "placeholder test should be replaced with real assertions")`.
   The 12 tests it used to hold were recoverable only from
   `git show e8bc726:tests/unreviewed_work_ack_test.rs`. All 13 are restored with
   their original assertions, unmodified. Commit `222aa83` (human, not the bot) is
   what introduced the stub; the resume commit re-imported the helpers but not the
   tests.

2. **`ACKNOWLEDGED` named a commit that is not in this history.** It listed
   `e8bc726`, the *abandoned* attempt that `c5e010b` replaced —
   `git merge-base --is-ancestor e8bc726 HEAD` exits 1. The shipped REPORT claimed
   this "fixed" a stale entry; it inverted the failure. Corrected to name
   `c5e010b` with measured line counts, and `c95062e` ("ack: resume phase-088",
   whose subject merely *contains* `resume phase-`) is now acknowledged too.

3. **A pre-existing clippy failure blocked the gate** —
   `tests/memory_model_test.rs:22`, `arc_with_non_send_sync` on
   `Arc<Node> where Node { peer: RefCell<...> }`. From commit `2948f23`, unrelated
   to this phase, surfacing on `clippy 0.1.99`. `cargo clippy --all-targets -- -D
   warnings` exited 101 on arrival, so no phase could pass. Repaired minimally:
   the `Node` scaffolding is `Rc`; the assertions about Redblue's own `Value` stay
   `Arc`. This is test-only, inside this phase's `must_touch`, and it is the reason
   that file appears in a diff about `else if` — flagged for the auditor as
   deserving its own phase rather than being buried here.

### How the chain is parsed

`else if` is only the chain's next arm when the `if` is on the **same line** as
the `else`. `else\n    if ... then` stays a statement of the else block with its
own `end`, exactly as it always parsed. The two spellings are the same tokens, so
the line between them is what tells them apart — this is the only place in the
grammar where a line break carries meaning, and it is the one thing that had to be
decided rather than merely implemented. Verified both ways by hand:

```
if x > 3 then say "big" else say "small"        end   -> big
if x > 3 then say "big" else
    if x > 1 then say "mid" else say "small"
    end
end                                             -> big
```

Each arm costs one `self.enter_block()` / `leave_block()`, because the arm desugars
to an `if` nested in the `else` of the arm before it and the nesting budget has to
reach it. A chain past the budget is refused like any other deep nesting rather
than recursing until it breaks. Parse is iterative in both the arm loop and the
desugar loop, so a long chain is bounded by `MAX_BLOCK_DEPTH`, not by the stack.

## Tests added

`tests/else_if_test.rs`, 20 `#[test]` functions, 13 named `edge_*`. Every one
asserts on output or on the error message; none is a smoke test.

| Test | Edge class covered |
|---|---|
| `else_if_as_the_last_arm_runs_its_branch` | singleton / boundary — the documented `if x is 9 ... else if x is 1` form |
| `else_if_in_the_middle_is_reached_when_the_if_arm_is_false` | ordering |
| `a_trailing_else_is_the_arm_nothing_matched` | branch coverage, trailing `else` |
| `three_chained_else_if_arms_take_the_first_that_holds` | multi-arm chain |
| `a_two_arm_if_else_with_no_else_if_is_unchanged` | regression: the two-arm form is untouched |
| `a_chain_desugars_to_nested_if_statements_one_end_apart` | AST shape — one `end` per chain, not per arm |
| `edge_an_empty_then_branch_in_an_else_if_arm_is_not_the_end_of_the_chain` | **empty** — `else if c then` with nothing before the next arm runs nothing and the chain continues |
| `edge_an_else_if_chain_inside_a_loop_body_takes_its_branch_per_iteration` | **nesting** — chain inside a `for` body |
| `edge_an_else_if_chain_closes_inside_a_while_body_and_a_repeat_body` | **nesting** — chain inside `while` and `repeat`; the chain's `end` must not swallow the loop's |
| `edge_a_nested_if_inside_an_else_if_arm_keeps_its_own_end` | **nesting** — an `if` written inside an arm body |
| `edge_an_else_if_arm_condition_uses_the_same_truth_rule_as_if` | **type_coercion / unicode** — `nothing`, `0`, `""`, `[]`, `1`, `"x"`, `[1]`, `"日本"`, `"🙂"`, `"日本 and 🙂"` read through the `if` position and the `else if` position give the same answer |
| `edge_an_if_on_the_line_after_else_is_a_statement_of_the_else_block_not_an_arm` | boundary between the two spellings |
| `edge_a_chain_within_the_block_budget_takes_the_first_arm_and_the_last` | **resource_limit** — the deepest accepted chain |
| `edge_a_chain_past_the_block_budget_is_refused_rather_than_recursing_until_it_breaks` | **resource_limit** — over-budget chain → positioned `ParserError`, not a stack overflow |
| `edge_the_repl_block_depth_reads_a_chain_as_one_block_and_a_nested_if_as_two` | REPL block accounting: a chain is 1, `else` + `if` is 2 |
| `edge_an_else_if_arm_with_no_then_names_the_missing_then` | **malformed_input** |
| `edge_an_else_if_chain_with_no_end_names_the_missing_end` | **malformed_input** |
| `edge_an_else_with_nothing_after_it_is_refused_at_eof` | **malformed_input** |
| `edge_an_else_if_arm_that_starts_with_a_stray_token_is_refused_positioned` | **malformed_input** — stray token |
| `both_engines_agree_on_every_position_of_an_else_if_chain` | tree-walking VM ≡ bytecode VM, every arm |

Plus, in `src/parser.rs`'s own test module:
`open_block_depth_gives_an_else_if_chain_one_end` and
`open_block_depth_counts_the_block_inside_an_else` (the latter is the regression:
`else` + `if` on the next line is still two blocks).

And in `tests/differential_test.rs`:
`edge_the_control_flow_family_runs_the_else_if_it_writes_on_both_engines` — the
three corpus `else if` programs run to completion on both engines and print what
their goldens record; it also asserts ≥2 *distinct* outputs, so one covered
position is not enough.

**Restored in `tests/unreviewed_work_ack_test.rs`** (this run, 13 `#[test]`, 12
`edge_*`) — `unreviewed_work_in_head_history_is_acknowledged`,
`edge_phase_078_commits_are_ancestors_of_head_not_absent`,
`edge_empty_log_selects_nothing`, `edge_a_fully_acknowledged_log_is_clean`,
`edge_exactly_one_unacknowledged_commit_is_reported`,
`edge_a_longer_phrase_containing_checkpoint_is_still_selected`,
`edge_a_two_tab_delimited_log_line_is_malformed`,
`edge_malformed_log_lines_are_refused_rather_than_skipped`,
`edge_unicode_and_escaped_subjects_are_handled_deterministically`,
`edge_an_acknowledgement_list_must_be_data`,
`edge_an_acknowledgement_that_points_at_nothing_is_stale`,
`edge_a_shallow_history_is_refused_rather_than_audited_clean`,
`edge_measured_list_names_every_audit_measured_commit`. Assertions are byte-identical
to `e8bc726`; only the two data entries in `ACKNOWLEDGED`/`MEASURED` changed.

### The failure channel, verified

Each of these is a clean positioned `ParserError`, never a panic — reproduced by
hand against the binary, not only in the test:

```
missing then:   Error: ParserError: Expected 'then' but got 'say'   --> e1.rb:3:5
missing end:    Error: ParserError: Expected 'end' but got Eof      --> e2.rb:3:1
empty else:     Error: ParserError: Expected 'end' but got Eof      --> e3.rb:2:5
over budget:    Error: ParserError: Blocks nest more than N levels deep
```

### Edge-case matrix rows not applicable

- **out_of_bounds** — N/A: this change adds no index arithmetic, no container
  access and no new runtime value. There is nothing to index.
- **numeric_boundary** — N/A: no arithmetic, coercion or float path is touched.
  Arm conditions go through the existing expression parser, unchanged;
  `edge_an_else_if_arm_condition_uses_the_same_truth_rule_as_if` pins that they
  hit the *same* truth rule as `if` over `0`, `""`, `[]` and `nothing`.
- **duplicate_missing_keys** — N/A: no record is constructed or read.

**unicode** is covered, not skipped:
`edge_an_else_if_arm_condition_uses_the_same_truth_rule_as_if` reads `"日本"`,
`"🙂"` and `"日本 and 🙂"` through both positions.

**type_mismatch** is covered by the malformed-input rows above plus the truth-rule
test: a non-expression after `then` is a positioned `ParserError`, not a coercion.

Every other row is covered above.

## Gates

Run in this checkout, in order, on the tree as it stands:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no output |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0, zero warnings |
| `cargo test --all-targets` | **pass** — 1820 passed, 0 failed, 0 ignored |
| `cargo test --doc` | **pass** — 2 passed, 0 failed |
| `./rbops/verify.sh phase-112` | **not run — the file does not exist in this checkout.** `ls rbops/` → `No such file or directory`; the tree root has no `rbops/` entry. The gate lives in the pipeline that invoked this run, which the task forbids inspecting. |

Both the fmt and the clippy gate were **red on arrival** and were fixed in this
run; see `FINDINGS.md` §1 and §3. Neither was muted: the stub test was replaced by
the real assertions it stood in for, and the clippy error was repaired in the type
it actually names rather than suppressed with `#[allow]`.

### Honest correction to the previous run's report

The report shipped in `c5e010b` claimed all four gates green. Two were not:
`cargo fmt --check` failed on the stub test, and `cargo clippy --all-targets` had
never been run against `tests/memory_model_test.rs`. It also claimed the stale
acknowledgement had been fixed; that change made the failure worse, and the
restored test is what caught it. Test count differs (1820, not 1806) because 12
deleted tests are back.

## Definition of done, checked

- [x] `if x is 9 then say "nine" else if x is 1 then say "one" end` prints `one`;
      with `x` set to 9 the trailing-`else` form prints `nine`. Run by hand on
      both `rb run` and `rb compile` + `rb vm`.
- [x] a four-arm chain with a trailing `else` runs on both engines and both print
      `three` (verified by hand).
- [x] plain `if`/`else` is unchanged — pinned by
      `a_two_arm_if_else_with_no_else_if_is_unchanged`,
      `open_block_depth_counts_the_block_inside_an_else` and
      `edge_an_if_on_the_line_after_else_is_a_statement_of_the_else_block_not_an_arm`,
      and by running both spellings by hand.
- [x] missing `then`, missing `end`, and an `else` with nothing after it each give
      a positioned `ParserError` naming what was expected. No panic on any of them.
- [x] `corpus/control-flow-000{3,4,5}` are off the must-fail list. Verified by
      hand: all three run and print `a`, `b`, `c` respectively, matching their
      regenerated `.expected` files. `tests/differential_test.rs:713` now asserts
      the chain **parses** on both engines rather than pinning the refusal, and
      `cargo test --test differential_test` passes.
- [x] `edge_*` coverage for: else-if as the last arm, else-if in the middle, three
      arms, an empty then-branch in an else-if arm, inside a `for`/`while`/`repeat`
      body, and a nested `if` inside an arm — each named in the table above.
- [x] `docs/GRAMMAR.md:227,234` and `SPEC.md:431,438` state the one-`end` rule and
      the same-line rule. Checked against the running parser, not written from the
      design.
- [x] 0 failures, 0 ignored, clippy clean, fmt clean.

## Invariants touched

- None. `.rb` is still the extension; `to … end` / `if … end` still use `end` (a
  chain closes on **one** `end`, which is what the docs already promised);
  `set x to <expr>` is still assignment; `say` still prints; the `Value` variants
  and `Error::{Lexer,Parser,Analyzer,Runtime,Io}` are untouched.
- No public type renamed, no grammar production removed, no architecture change.
- `Statement::If` keeps its existing three fields. The chain is desugared at parse
  time into nested `If` nodes, so the interpreter, the bytecode codegen and the
  `Value` model needed no change — which is why the feature diff is one function.

## Known gaps / follow-ups

- `rb format` and `rb lint` were not exercised against a chain. The formatter walks
  the AST and a chain is now an ordinary nested `If`, so it formats as one, but no
  test pins that. → `FINDINGS.md` §4.
- `else if` across a line continuation is not supported and is not stated next to the
  production. → `FINDINGS.md` §5.
- `tests/memory_model_test.rs` was repaired here to unblock the gate and deserves
  its own phase. → `FINDINGS.md` §3.
- The six corpus programs the frontend still refuses are unchanged by this phase →
  `FINDINGS.md` §6, and `phases/phase-021/FINDINGS.md` §2.