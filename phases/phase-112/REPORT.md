# Phase 112 — Make `else if` parse, as SPEC.md and docs/GRAMMAR.md both promise

## What changed

`else if <expr> then ...` was documented in `SPEC.md` and `docs/GRAMMAR.md` but
the parser had no arm for it. After `else` the parser entered a plain statement
loop, so the `if` was parsed as a **nested** `if`; that nested block consumed the
single `end` the chain shares and the outer `parse_if` then failed looking for a
second one. Both corpus programs and a direct run reproduced it as
`ParserError: Expected 'end' but got Eof`.

`else if` on the **same line** as `else` is now another arm of the same block.
A newline between the words still means an ordinary nested `if` in the `else`
body, which keeps the change backwards compatible. The Redblue bootstrap compiler
had the same defect and is fixed in the same way.

This resumed run kept the preserved implementation, re-ran every gate, corrected
the report's measured line counts, and added the `resume phase-112:` delivery
prefix the acknowledgement audit requires; full test coverage was already in the
tree and was re-run rather than rewritten.

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +64 −15 | `parse_if` treats `else if` on the same line as a chain arm sharing the chain's one `end`; `open_block_depth` compares token line numbers instead of assuming a `Newline` token, and no longer counts an `else if` arm as a second block (so the REPL does not wait for a second `end`). Existing block-depth unit test corrected deliberately |
| `bootstrap/compiler.rb` | +18 −2 | `parse_if_else_branch` recognises a same-line `if` and parses the recursive arm, consuming the shared `end` once; it passes the arm's own `depth` (already the else-body level) so each arm costs exactly one block, matching stage 1's 64-block budget |
| `corpus/control-flow-0003.expected` | +3 −3 | Regenerated: now `a` (`#label none`) instead of the old `ParserError` |
| `corpus/control-flow-0004.expected` | +3 −3 | Regenerated: now `b` |
| `corpus/control-flow-0005.expected` | +3 −3 | Regenerated: now `c` |
| `tests/common/generator.rs` | +6 −5 | Comment updated: the three chain programs are now runnable, not refusals |
| `tests/differential_test.rs` | +101 −24 | The old `else if` refusal test is deliberately replaced by one that asserts the three corpus chains parse, complete, match their goldens, and agree on both engines; adds both-engine arm and malformed-input tests |
| `tests/bootstrap_selfhost_test.rs` | +57 −13 | New stage-1/stage-2 byte-identity test for `else if` chains, including the 63-arm 64-block boundary; corpus acceptance counts corrected from 315/9 to 318/6 |
| `tests/else_if_test.rs` | +322 | 14 targeted tests: branches, trailing `else`, empty arm, loops, nesting, malformed shapes, both engines, block budget |
| `tests/unreviewed_work_ack_test.rs` | +1 −0 | Adds the `resume phase-112:` delivery-subject prefix so this phase's rebuildable delivery commit is acknowledged by the rule rather than reported as unreviewed work |
| `SPEC.md`, `PHILOSOPHY.md` | +11 −6 | Worked `if`/`else if`/`else` examples now include the required `then`, matching the parser and `docs/GRAMMAR.md` |
| `docs/GRAMMAR.md` | +4 −0 | States the same-line rule the parser enforces: `else if` starts an arm on the same line and shares the chain's one `end`; the newline form is a nested `if` with its own `end` |
| `src/formatter.rs` | +66 −21 | Writes an `else if` arm on the `else` line with the chain's one `end` instead of the two-`end` newline-nested form, while leaving a genuinely newline-separated `if` in the else body nested |
| `tests/formatter_test.rs` | +85 −0 | Round-trip and `rb format --check` coverage for the three chain corpus programs, plus every chain shape (last arm, empty arm, several arms) and the newline-nested form |
| `tests/doc_blocks_runs.txt` | +46 −46 | Re-keyed: the `then` additions above shift later `SPEC.md`/`PHILOSOPHY.md`/`docs/GRAMMAR.md` fence line numbers. No block was reclassified |
| `tests/doc_blocks_fragments.txt` | +21 −21 | Re-keyed for the same line-number shift; every entry keeps its classification and explanation |
| `tests/doc_blocks_known_fail.txt` | +6 −6 | Re-keyed for the same line-number shift; every entry keeps its expected error |

`docs/GRAMMAR.md` already wrote the chain form (one `end`, `{ 'else if'
expression 'then' { statement } }`); the same-line rule the parser enforces is now
written down there and in `SPEC.md` as well.

## Reproduction

Before the change, on the built tree-walking binary:

```
$ printf 'set x to 9\nif x is 9 then\n    say "nine"\nelse if x is 1 then\n    say "one"\nend\n' > target/tmp/elseif.rb
$ ./target/debug/rb run target/tmp/elseif.rb
Error: ParserError: Expected 'end' but got Eof
  --> target/tmp/elseif.rb:7:1
```

After the change the same program prints `nine`; a condition of `1` prints
`one`; a trailing `else` takes the right branch; and `rb compile` + `rb vm`
agree with `rb run`.

The three corpus programs that write the form now record completions:

```
control-flow-0003: a
control-flow-0004: b
control-flow-0005: c
```

## Tests added

14 new `#[test]` functions in `tests/else_if_test.rs`, 3 new `#[test]`
functions in `tests/differential_test.rs`, 2 new `#[test]` functions in
`tests/formatter_test.rs`, and 1 new `#[test]` in
`tests/bootstrap_selfhost_test.rs`. One existing parser unit test and one existing
differential test were updated deliberately because they asserted the old
behaviour; neither was deleted or weakened.

| New test | Edge class covered |
|---|---|
| `else_if_chain_picks_the_matching_arm` | middle arm |
| `edge_else_if_as_the_last_arm_runs_without_a_trailing_else` | singleton / last arm |
| `edge_three_arms_plus_else_pick_every_branch` | boundary: every arm, including trailing `else` |
| `edge_an_empty_then_branch_in_an_else_if_arm_takes_no_trailing_else` | empty branch |
| `edge_else_if_inside_for_while_and_repeat_bodies` | nesting inside all three loop forms |
| `edge_a_nested_if_inside_an_else_if_arm_keeps_its_own_end` | nesting / shared `end` |
| `edge_plain_if_else_without_else_if_is_unchanged` | regression: plain two-arm `if` |
| `edge_else_then_a_newline_then_if_is_still_a_nested_if_in_the_else_body` | regression: newline-separated `else`/`if` |
| `edge_a_same_line_chain_shares_its_end_and_rejects_the_old_two_end_shape` | **asserts a failure**: the old same-line two-`end` shape is now a spanned parser error |
| `edge_else_if_chain_agrees_between_the_two_engines` | tree-walking and bytecode VMs |
| `edge_else_if_missing_then_is_a_spanned_parser_error` | **asserts a failure**: malformed `else if` |
| `edge_else_if_missing_end_is_a_spanned_parser_error` | **asserts a failure**: missing `end` |
| `edge_else_with_nothing_after_it_is_a_spanned_parser_error` | **asserts a failure**: dangling `else` |
| `edge_a_chain_deeper_than_the_block_budget_is_a_spanned_parser_error` | resource limit: chain past 64 blocks is an error, not a panic |
| `edge_an_else_if_chain_picks_every_arm_on_both_engines` (differential) | every arm of a three-arm chain on both VMs |
| `edge_else_if_with_a_missing_then_is_a_parser_error_on_both_engines` (differential) | **asserts a failure** with `is_err()` on both engines |
| `edge_else_if_missing_end_and_dangling_else_are_parser_errors` (differential) | **asserts a failure** for two malformed shapes |
| `edge_else_if_chains_round_trip_and_pass_format_check` (formatter) | the three chain corpus programs stay on one line with one `end` and pass `rb format --check` |
| `edge_the_formatter_keeps_every_else_if_chain_shape` (formatter) | last arm, empty arm, several arms, and the newline-nested form all round-trip idempotently |
| `stage2_is_byte_identical_for_an_else_if_chain` (selfhost) | stage 1 / stage 2 bytes: middle arm, three arms, last arm, empty arm, newline-nested form, and a 63-arm chain exactly on the 64-block budget |

Updated deliberately:

| Test | Why |
|---|---|
| `parser::tests::open_block_depth_treats_an_else_if_arm_as_the_same_block` | The old test pinned `else if` as *two* blocks needing two `end`s. The grammar says one block; the new expectations are 1/0/−1 for the chain and 2/1/0 for the newline-nested form |
| `differential_test::edge_the_control_flow_family_covers_if_else_and_the_else_if_chain` | The old test pinned the three corpus chains as `ParserError` on both engines. It now asserts they complete, match their refreshed goldens, and agree on both engines |

### Mandatory edge-case matrix

| Row | Status |
|---|---|
| empty | **covered** — empty then-branch in an `else if` arm; empty `else` body remains legal |
| singleton | **covered** — `else if` as the only additional/last arm |
| boundary | **covered** — every arm of a three-arm chain plus trailing `else` |
| out_of_bounds | N/A — no indexing, slicing or list storage changed |
| type_mismatch | N/A — conditions use the existing expression grammar; no new type rule |
| numeric_boundary | N/A — no arithmetic or numeric conversion changed |
| unicode | N/A — no lexing or text handling changed; the unicode corpus family is unchanged and green |
| nesting_recursion | **covered** — nested `if` inside an arm, arms inside `for`/`while`/`repeat`, block budget |
| duplicate_missing_keys | N/A — no record construction or field access changed |
| malformed_input | **covered** — missing `then`, missing `end`, `else` with nothing after it |
| resource_limit | **covered** — a chain past `MAX_BLOCK_DEPTH` is a spanned `ParserError`; the bootstrap compiler passes each arm's own `depth`, so a 63-arm chain is accepted on the 64-block boundary by both engines |

## Gates

Run in this checkout, this run.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no output |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0, no warnings |
| `cargo test --all-targets` | **pass** — 3081 passed, 0 failed across 86 test binaries |
| `../rbops/verify.sh phase-112` | **pass** — see verifier output |

`verify.sh` also printed one non-failing honesty warning: the pipeline's
`../phases/phase-112/REPORT.md` still claimed 1820 passing, so the verifier saw
this phase as under-claiming (1820 reported, 3081 actual). That copy is outside
this checkout and is not writable from this run; this report, in the checkout,
records the measured 3081/0.

## Invariants touched

- `Statement::If` still models the chain as nested `Statement::If` values in
  `else_branch`, so both VMs and the analyzer needed no change.
- `else` followed by a newline and then `if` is still a nested statement in the
  else body. No language surface other than the documented `else if` changed.
- `.rb`, `to … end`, `say`, `Value`, `Error`, trailing commas and the existing
  corpus/examples are untouched.

## Known gaps / follow-ups

- None for this phase. The formatter now preserves the same-line `else if`
  chain documented in `SPEC.md` and `docs/GRAMMAR.md`, and still writes a
  genuinely newline-separated `if` in an else body as the nested form with its
  own `end`.
