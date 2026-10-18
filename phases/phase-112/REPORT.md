# Phase 112 — Make `else if` parse, as SPEC.md and docs/GRAMMAR.md both promise

## Finding re-verified before anything changed

`else if` was a `ParserError` in every position on `main` before this work:
`parse_if` consumed `TokenKind::Else` and dropped straight into a statement loop,
so the `if` after it parsed as a *nested* `if`, that nested `if` ate the `end`
that belonged to the outer one, and the outer `parse_if` then failed its
`self.expect(TokenKind::End)`.

Reproduced against the built binary:

```
$ ./target/debug/rb run target/tmp/p112/a.rb   # else if as the last arm
Error: ParserError: Expected 'end' but got Eof
  --> target/tmp/p112/a.rb:7:1
```

## What changed

| File | Lines | What |
|---|---|---|
| src/parser.rs | +126 −42 | `parse_if` desugars an `else if` chain into nested `if`s, one block level per arm; `parse_if_arm_body` is the shared arm-body loop; `open_block_depth` reads a chain as the one block its single `end` closes |
| bootstrap/compiler.rb | +44 −17 | the stage-2 Redblue compiler parses the same chain (`parse_if_else` / `parse_if_arm`), so stage 1 and stage 2 stay byte-identical |
| tests/else_if_test.rs | +605 | new — 20 tests, 13 of them `edge_*` |
| tests/differential_test.rs | +63 −19 | `edge_the_control_flow_family_covers_the_if_else_it_has_and_pins_the_else_if_it_does_not` → `..._runs_the_else_if_it_writes_on_both_engines`: the same three corpus programs, no longer pinned as refusals |
| tests/bootstrap_selfhost_test.rs | +18 −15 | frontend refusal count 9 → 6 (`control-flow-000{3,4,5}` now parse) |
| corpus/control-flow-000{3,4,5}.expected | +9 −9 | regenerated off the now-running programs |
| SPEC.md | +20 −0 | the `else if` rule, worked |
| docs/GRAMMAR.md | +22 −0 | `else if` written into the `if_statement` production |
| tests/doc_blocks_{runs,known_fail,fragments}.txt | +61 −60 | re-pinned for the SPEC.md shift |
| tests/unreviewed_work_ack_test.rs | +4 −2 | acknowledgement names the commit actually in HEAD (`e8bc726`), not the superseded WIP checkpoint `1ef37cf`, which is not an ancestor and was reported stale |

### How the chain is parsed

`else if` is only the chain's next arm when the `if` is on the **same line** as
the `else`. `else\n    if ... then` stays a statement of the else block with its
own `end`, exactly as it always parsed. The two spellings are the same tokens, so
the line between them is what tells them apart — this is the only place in the
grammar where a line break carries meaning, and it is the one thing that had to
be decided rather than merely implemented.

Each arm costs one `self.enter_block()` / `leave_block()`, because the arm
desugars to an `if` nested in the `else` of the arm before it and the nesting
budget has to reach it. A chain past the budget is refused like any other deep
nesting rather than recursing until it breaks.

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
| `edge_an_else_if_chain_closes_inside_a_while_body_and_a_repeat_body` | **nesting** — chain inside `while` and `repeat` bodies; the chain's `end` must not swallow the loop's |
| `edge_a_nested_if_inside_an_else_if_arm_keeps_its_own_end` | **nesting** — an `if` written inside an arm body |
| `edge_an_else_if_arm_condition_uses_the_same_truth_rule_as_if` | **type_coercion / unicode** — `nothing`, `0`, `""`, `[]`, `1`, `"x"`, `[1]`, `"日本"`, `"🙂"`, `"日本 and 🙂"` read through the `if` position and the `else if` position must give the same answer |
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

| Test | Edge class covered |
|---|---|
| `open_block_depth_gives_an_else_if_chain_one_end` | block accounting: a chain is one block |
| `open_block_depth_counts_the_block_inside_an_else` | regression: `else` + `if` on the next line is still two |

And in `tests/differential_test.rs`:

| Test | Edge class covered |
|---|---|
| `edge_the_control_flow_family_runs_the_else_if_it_writes_on_both_engines` | the three corpus `else if` programs run to completion on both engines and print what their goldens record; also asserts ≥2 *distinct* outputs, so one covered position is not enough |

### The failure channel, verified

Each of these is a clean positioned `ParserError`, never a panic:

```
missing then:   ParserError: Expected 'then' but got 'say'      --> 5:3
missing end:    ParserError: Expected 'end' but got Eof         --> 6:1
empty else:     ParserError: Expected 'end' but got Eof         --> 5:1
stray token:    ParserError: Unexpected token Newline            --> 4:8
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

Every other row is covered above.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — exit 0, no output |
| `cargo clippy --all-targets -- -D warnings` | pass — exit 0, zero warnings |
| `cargo test --all-targets` | **1806 passed, 0 failed, 0 ignored** |
| `cargo test --doc` | 2 passed, 0 failed |
| `./rbops/verify.sh phase-112` | **not run — the file does not exist in this checkout.** `ls rbops/` → `No such file or directory`; the tree root has no `rbops/` entry. The gate lives in the pipeline that invoked this run, which the task forbids inspecting. Run locally by: all three of the above, plus `cargo fmt --check` on any file the phase added. |

### One pre-existing failure was fixed, not muted

`cargo test` was red on arrival: `unreviewed_work_in_head_history_is_acknowledged`.
The cause was data, not behaviour — the acknowledgement list added by the
preserved work named commit `1ef37cf`, the WIP checkpoint that was **not** an
ancestor of HEAD (`git merge-base --is-ancestor` → not-ancestor), so `scan`
reported it stale while the commit actually in HEAD, `e8bc726`, went
unacknowledged. Both `ACKNOWLEDGED` and `MEASURED` now name `e8bc726`. No
assertion was changed, loosened or removed; the test is strictly as strict as it
was and now passes.

## Definition of done, checked

- [x] `if x is 9 then say "nine" else if x is 1 then say "one" end` prints
      `nine`; with `x` set to 1 the trailing-`else` form prints `one`
      (run on both `rb run` and `rb compile` + `rb vm`).
- [x] three chained `else if` arms with a trailing `else` work on both engines
      and the two agree.
- [x] plain `if`/`else` is unchanged — pinned by
      `a_two_arm_if_else_with_no_else_if_is_unchanged`,
      `open_block_depth_counts_the_block_inside_an_else` and
      `edge_an_if_on_the_line_after_else_is_a_statement_of_the_else_block_not_an_arm`.
- [x] missing `then`, missing `end`, and an `else` with nothing after it each
      give a positioned `ParserError` naming what was expected. No panic on any
      of them.
- [x] `corpus/control-flow-000{3,4,5}` are off the must-fail list, their
      `.expected` files regenerated off what the programs print (`a`, `b`, `c`),
      and `tests/differential_test.rs:713` now asserts the chain **parses** on
      both engines rather than pinning the refusal.
- [x] `edge_*` coverage for: else-if as the last arm
      (`else_if_as_the_last_arm_runs_its_branch`), else-if in the middle
      (`else_if_in_the_middle_is_reached_when_the_if_arm_is_false`), three arms
      (`three_chained_else_if_arms_take_the_first_that_holds`), an empty
      then-branch in an else-if arm
      (`edge_an_empty_then_branch_in_an_else_if_arm_is_not_the_end_of_the_chain`),
      inside a loop body
      (`edge_an_else_if_chain_inside_a_loop_body_takes_its_branch_per_iteration`),
      and a nested `if` inside an arm
      (`edge_a_nested_if_inside_an_else_if_arm_keeps_its_own_end`).
- [x] `docs/GRAMMAR.md:234` and `SPEC.md:438` agree with the implementation.
      Both state the one-`end` rule and the same-line rule, and both were
      checked against the running parser rather than written from the design.
- [x] 0 failures, clippy clean.

## Invariants touched

- None. `.rb` is still the extension; `to … end` / `if … end` still use `end`;
  `set x to <expr>` is still assignment; `say` still prints; the `Value` variants
  and `Error::{Lexer,Parser,Analyzer,Runtime,Io}` are untouched.
- No public type renamed, no grammar production removed, no architecture change.
- `Statement::If` keeps its existing three fields. The chain is desugared at
  parse time into nested `If` nodes, so the interpreter, the bytecode codegen and
  the `Value` model needed no change at all — which is why the diff is one
  function in the parser.

## Known gaps / follow-ups

- **`rb format` and `rb lint` were not exercised against a chain.** The
  formatter walks the AST, and a chain is now an ordinary nested `If`, so it
  formats as one — but no test in this phase pins that. If the formatter has
  line-sensitive handling for `else`, that is unverified. Worth a phase.
- **`else if` across a line continuation is not supported and is not
  documented as unsupported.** `else` then a newline then `if` is an else-block
  statement, always. A user who writes it expecting a chain gets a nested `if`
  that still means what they wanted, so this is a surprise of shape rather than
  of behaviour — but SPEC.md and GRAMMAR.md do not spell the rule out at the
  point of use, only in the added paragraph.
- **The six corpus programs the frontend still refuses** are unchanged by this
  phase and still listed in `phases/phase-021/FINDINGS.md` section 2.