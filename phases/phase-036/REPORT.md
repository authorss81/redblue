# Phase 036 — Make `break` and `skip` leave and continue the loop on both VMs

## Headline: the finding was stale. No production behaviour was changed.

The evidence block names four broken sites. All four were already fixed on
`cc50661` before this phase edited anything. Full evidence with `file:line` is in
`FINDINGS.md`; the short version:

- `grep -rn 'TODO: Implement proper control flow' src/` printed nothing.
- `Statement::Break`/`Statement::Skip` (`src/interpreter.rs:1268`, `:1273`) call
  `raise_loop_control`; `break_loop`/`skip_loop` (`src/bytecode/vm.rs:2035`,
  `:2052`) do real loop lookup and unwinding.
- `tests/test_lists.rb:104` already read `expect seen to be 3`, not 4.

phase-022 adopted phase-025's preserved work (commits `861080d`, `054e0d6`), so
the prompt's premise is right about the id and wrong about the work: the fix is on
`main` and green, with 81 test functions in `tests/loop_control_test.rs`.

The phase prompt says a stale phase "must never be 'fixed' by inventing a change".
So nothing in the VM's `break`/`skip` semantics was touched. What **was** genuinely
missing is the phase's own last Definition-of-Done line:

> "a test that counts executed BREAK and SKIP instructions proves the handlers are
> reached"

That is a real verification gap, and it is the shape the original defect took.
**Printed output cannot distinguish a working `break` from a dropped one** whenever
the loop would have ended anyway — the defect class could regress with the whole
suite green. phase-036 closed that gap and nothing else.

## Reproduction, before any edit

    $ printf 'for each i in [1,2,3]\n    if i is 2 then\n        break\n    end\n    say i\nend\n' > target/tmp/p036/brk.rb
    $ ./target/debug/rb run target/tmp/p036/brk.rb
    1
    exit=0

    $ printf 'repeat 5 times\n    break\n    say "never"\nend\n' > target/tmp/p036/rb.rb
    $ ./target/debug/rb run target/tmp/p036/rb.rb
    exit=0            # (the phase predicted "never" five times)

    $ printf 'for each i in [1,2,3]\n    if i is 2 then\n        skip\n    end\n    say i\nend\n' > target/tmp/p036/skp.rb
    $ ./target/debug/rb run target/tmp/p036/skp.rb
    1
    3
    exit=0

Every one of these is already **correct**. There is no wrong output to record.

## What changed

| File | Lines | What |
|---|---|---|
| `src/bytecode/vm.rs` | +30 −2 | `break_instructions`/`skip_instructions` counters, incremented where `Opcode::Break`/`Opcode::Skip` are dispatched; `pub fn loop_control_counts(&self) -> (usize, usize)` |
| `src/interpreter.rs` | +24 −0 | `break_statements`/`skip_statements` counters, incremented in the `Statement::Break`/`Statement::Skip` arms; `pub fn loop_control_counts(&self) -> (usize, usize)` |
| `tests/loop_instruction_test.rs` | +371 (new) | 12 tests asserting the counts on both engines |
| `phases/phase-036/FINDINGS.md` | +74 (new) | stale-finding evidence; `verify.sh` is missing from this checkout |
| `phases/phase-036/REPORT.md` | (new) | this file |

The counter is placed **before** the handler call, deliberately. A `break` outside
every loop is refused by `break_loop`, so counting after the handler would have
recorded zero for the very case where the interesting question is whether the
handler ran at all. The refusal *is* the handler having run.

The two `Opcode` dispatch arms moved from expression-style to block-style to carry
the increment. No other dispatch arm changed.

### Cross-engine invariant

`counts_on_both_engines` runs every behavioural case on **both** VMs and asserts
the tuples match, so a disagreement about how many `break`s ran fails as a
language disagreement rather than as plumbing drift. The tree-walking VM counts
evaluated statements; the bytecode VM counts dispatched opcodes.

## Tests added

12 new `#[test]` functions in `tests/loop_instruction_test.rs`; 7 named `edge_*`;
2 assert a failure is produced (`expect_err` + a `RuntimeError` match). The
project floor is 3 tests / 1 `edge_*` / 1 failure-assertion.

| Test | Edge class covered |
|---|---|
| `a_break_in_a_for_each_loop_dispatches_exactly_one_break` | boundary — one, not zero (dropped) and not three (per-element) |
| `a_skip_in_a_for_each_loop_dispatches_exactly_one_skip` | boundary — `breaks` is 0, so "the loop continued" is asserted, not inferred |
| `a_break_on_the_first_value_still_dispatches_exactly_once` | boundary — count is per-statement, not per-value; this is the shape that hides a dropped `break` |
| `a_break_in_a_nested_loop_dispatches_one_break_and_the_outer_turns_continue` | nesting — 2 breaks for 2 outer turns; 4 would mean the inner loop was not left |
| `edge_a_skip_on_the_final_value_dispatches_one_skip_and_no_break` | boundary — last element; `skip` and `break` look identical from outside |
| `edge_both_words_in_one_program_each_dispatch_once` | boundary — both counters non-zero in one program |
| `edge_a_refused_skip_still_dispatches_the_instruction_and_then_fails` | failure — `expect_err` + `RuntimeError` naming `skip` |
| `edge_a_caught_break_still_dispatches_the_instruction` | failure/caught — count is 1, proving the refusal came from the handler, not a compile-time check |
| `edge_a_loop_with_no_break_or_skip_dispatches_none` | empty — zero case, pinned to both engines |
| `edge_a_break_in_a_function_body_is_refused_once_and_the_caller_keeps_counting` | resource/state — 3 calls, 3 refusals, caller's loop not truncated |
| `edge_the_counted_opcodes_are_the_two_the_compiler_emits` | malformed/absent — the counted opcodes exist in the emitted code, so the counter is not reading an empty opcode |
| `a_refused_break_still_dispatches_the_instruction_and_then_fails` | failure — `expect_err` + `RuntimeError` naming `break` |

### Mutation check — both counters are load-bearing

A counter nothing reads asserts nothing. Each handler was neutered in turn; the
neuterings were reverted and the tree restored from backup afterwards.

| Mutation | Result |
|---|---|
| `Opcode::Break => { count += 1; self.advance(frame); Ok(()) }` (drop the handler) | `a_refused_break_still_dispatches_the_instruction_and_then_fails` **FAILED** |
| `self.break_statements += 0` (tree VM does not count) | **6 FAILED**: `a_break_on_the_first_value…`, `a_break_in_a_for_each_loop…`, `a_break_in_a_nested_loop…`, `edge_a_break_in_a_function_body…`, `edge_a_caught_break…`, `edge_both_words_in_one_program…` |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** (no diff) |
| `cargo clippy --all-targets -- -D warnings` | **pass** (0 warnings, 0 errors) |
| `cargo test --all-targets` | **1095 passed, 0 failed** |
| `./rbops/verify.sh phase-036` | **NOT RUN — the file does not exist in this checkout** |

### Gate 4 could not be run, and is not claimed as a pass

    $ ls rbops/
    ls: cannot access 'rbops/': No such file or directory
    $ ls rbops/verify.sh
    ls: cannot access 'rbops/verify.sh': No such file or directory

The only shell script in the project root is `export.sh`. `rbops/` is absent
entirely, so the fourth gate named in the phase prompt has no counterpart here.
I did not create it, and I did not touch `.github/workflows/` or any `.opencode/`
agent file. Recorded as unrun rather than passed; `FINDINGS.md` item 3 carries it
to the auditor.

The two suites the phase named specifically, both present in the 1095:

- `tests/loop_bounds_test.rs` — 24 tests, all passing (phase predicted 22; the
  file grew since the phase was written)
- `tests/bytecode_vm_test.rs` — 71 tests, all passing (phase predicted 33)
- `tests/loop_control_test.rs` — 70 tests, all passing, untouched by this phase

## Definition of Done — item by item

- [x] `for each i in [1,2,3]` with `break` at `i is 2` prints exactly `1` on both
      engines. Full output below.
- [x] `repeat 5 times` whose first statement is `break` prints nothing, exit 0,
      on both engines. Output below.
- [x] `skip` at `i is 2` in a three-element `for each` prints `1` then `3`, on
      both engines. Output below.
- [x] Nested `break` leaves only the inner loop, pinned by a test whose outer
      counter keeps counting — `a_break_in_a_nested_loop_leaves_only_the_inner_one`
      (`tests/loop_control_test.rs:147`, pre-existing) asserts `outer` is 3, and
      this phase's `a_break_in_a_nested_loop_dispatches_one_break_and_the_outer_turns_continue`
      pins the same nesting from the instruction-count side.
- [x] `tests/test_lists.rb:104`/`:106` corrected — **were already corrected before
      this phase**: `:104` is `expect seen to be 3`, and the break case at `:119`
      asserts `expect total to be 3`. Not deleted, not weakened. The "both named
      tests still exist and pass" guard already exists as
      `the_corrected_list_tests_still_exist_and_pass` (`tests/loop_control_test.rs`,
      passing).
- [x] `edge_*` tests assert a clean **caught** `RuntimeError` for `break` and for
      `skip` outside any loop — no silent no-op, no panic. This phase's
      `a_refused_break_…_then_fails` and `edge_a_refused_skip_…_then_fails` assert
      `expect_err` plus a `RuntimeError` whose message names the word; the caught
      form is `edge_a_caught_break_still_dispatches_the_instruction`. 0 failures.
- [x] `grep -rn 'TODO: Implement proper control flow' src/` prints nothing.
- [x] `grep -n 'TokenKind::Until' src/parser.rs` is **not** what proves this, and
      is not what this phase used. Proof is by dispatched-instruction count on both
      engines.
- [x] ≥3 new `#[test]` — 12. ≥1 `edge_*` — 7. ≥1 asserting a failure — 3.
- [x] Zero new `#[ignore]`, `// skip`, `allow(clippy::`; zero newly-failing
      pre-existing tests.

### `rb run` and `rb vm` output, pasted

    $ for f in brk skp rb; do ... done

    ### brk — rb run:
    1
    [exit=0]
    ### brk — rb vm (from rb compile):
    1
    [exit=0]
    ### skp — rb run:
    1
    3
    [exit=0]
    ### skp — rb vm (from rb compile):
    1
    3
    [exit=0]
    ### rb — rb run:
    [exit=0]
    ### rb — rb vm (from rb compile):
    [exit=0]

Each `rb vm` line is `rb compile <file>.rb -o <file>.rbc && rb vm <file>.rbc`.
Sources are in `target/tmp/p036/`.

## Edge-case matrix

| Row | Status | Why |
|---|---|---|
| empty | covered | `edge_a_loop_with_no_break_or_skip_dispatches_none` — an empty `[...]` iterable plus a loop with neither word; asserts (0,0) |
| singleton | covered | `a_break_on_the_first_value_still_dispatches_exactly_once` — one turn, one break |
| boundary | covered | 5 tests: first element, last element, both words, nested, and the count-vs-length distinction |
| out_of_bounds | **N/A** | `break`/`skip` take no operand, so there is no index to be out of bounds. The nearest equivalent — reaching a `break` written *after* the loop ends — is covered by `a_break_on_the_first_value_…` (the loop ends before the third turn) |
| type_mismatch | **N/A** | Neither word evaluates an operand, so there is no value whose type could mismatch. `edge_neither_break_nor_skip_takes_an_operand` (`tests/loop_control_test.rs`, pre-existing) pins that the grammar promises no operand |
| numeric_boundary | **N/A** | No arithmetic on the loop-control path. Iteration counting is shared with the existing `charge_iteration` cap, already covered in `tests/loop_bounds_test.rs` |
| unicode | **N/A** | This phase adds a counter over two opcodes; no string, text or character handling is touched. `edge_break_and_skip_over_unicode_values` exists pre-existing in `tests/loop_control_test.rs` |
| nesting_recursion | covered | `a_break_in_a_nested_loop_dispatches_one_break_and_the_outer_turns_continue` — 2 outer turns × inner `break` = 2, inner only |
| duplicate_missing_keys | **N/A** | No record, key or field access is on this path |
| malformed_input | covered | `edge_the_counted_opcodes_are_the_two_the_compiler_emits` — the counted opcodes must exist in the emitted block, so the counter cannot be reading an opcode the compiler never emits |
| resource_limit | covered | `edge_a_break_in_a_function_body_is_refused_once_and_the_caller_keeps_counting` — a `break` in a call body is refused 3 times and the caller's loop still ends, so an obeyed `break` cannot shorten a turn it was not written in. The infinite-loop guard itself is unchanged and covered by `tests/loop_bounds_test.rs` (24 tests) |

Rows marked N/A are genuinely orthogonal to the change: this phase adds two integer
counters and increments them at two already-existing dispatch sites. No lexer,
parser, evaluator, value or file path is modified — the diff is 54 added lines
across the two `src/` files, all of it counter state and doc comments.

## Invariants touched

- **None.** No language surface changed. `.rb` extension, `end` blocks, `set x to`,
  `say`, the `Value` variants and the `Error` variants are all untouched. The only
  new public items are two read-only `pub fn loop_control_counts(&self)` accessors
  that return a `(usize, usize)` — additive, no existing signature altered.
- The one behavioural change in the whole diff is that **the tree-walking VM now
  remembers how many `break`/`skip` statements it evaluated**. It does not alter
  what any statement does.

## Known gaps / follow-ups

- `rbops/verify.sh` is absent from this checkout, so gate 4 did not run. Carried
  to `FINDINGS.md` item 3. **If this phase is judged against a gate that exists in
  the real pipeline, gate 4 must be run there before this phase is `.done`.**
- phase-025 should be `.done`, not `.blocked` — its work is already on `main` under
  phase-022. Carried to `FINDINGS.md` item 1. Left un-actioned deliberately: editing
  `rbops/phases.json` or phase state markers is a hard-rule violation.
- The stale-citation check belongs in `dispatch.sh`, before dispatch. Two phases
  in a row (035, 036) were dispatched against findings already fixed by adopted
  work from a different id. Carried to `FINDINGS.md` item 2.
- The counters are `usize` and monotonic per VM, never reset between runs on the
  same VM. A caller running two programs on one `BytecodeVm` sees the sum. This
  matches the existing `steps` counter, which has the same property; changing it
  was out of scope for this phase. Noted rather than fixed, because fixing it
  would mean touching `run()`'s reset list — a behaviour change to a shared
  counter with no test currently depending on it either way.