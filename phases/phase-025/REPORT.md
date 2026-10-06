# Phase 025 — Make `break` and `skip` leave the loop instead of silently doing nothing

## Reproduction

Commands run before any change, against the binary already built in `target/debug`:

```
$ cat repro_break.rb
for each i in [1, 2, 3]
    if i is 2 then
        break
    end
    say i
end

$ ./target/debug/rb run repro_break.rb ; echo "exit=$?"
1
2
3
exit=0
```

```
$ ./target/debug/rb run repro_skip.rb ; echo "exit=$?"     # same program with `skip`
1
2
3
exit=0

$ ./target/debug/rb run repro_repeat.rb ; echo "exit=$?"   # break when n is 3
5
exit=0

$ ./target/debug/rb run repro_outside.rb ; echo "exit=$?"  # the file is exactly `break`
exit=0
```

Four programs that exit 0 with the wrong answer: `break` printed the two values
after the one that broke, `skip` printed the value it skipped, the `repeat` loop
reached 5 instead of stopping at 3, and a `break` in no loop at all succeeded.

## Round 1 review

The reviewer found four BLOCKERs. All four are fixed in this round, and two more
defects the fixes uncovered are fixed with them (FINDINGS §4 and §5). The theme
of three of the four: the signal was only consulted at the top level of a loop
body, so every *block* inside a loop kept running the statements after a
`break`.

| # | Finding | Fix |
|---|---|---|
| 1 | **BLOCKER** `src/vm.rs` `Statement::If` ran each branch as a plain `for stmt in …`, so `if c then break; say "leaked" end` still printed — the program left the loop having done what the `break` was for. | `Vm::run_block` is the one way a block runs its statements: `execute_statements` stops at a raised signal and leaves it pending, and the `If` arm, the `Try` arm, the `Test` arm and `declare_object` all run through it. |
| 2 | **BLOCKER** `Statement::Try` ran its body, catch and finally through `execute_statements`, which never looked at the signal — the same leak inside a protected region, and an abrupt exit that did not abort the region. | The `Try` arm runs all three through `run_block`; the catch moved to `Vm::run_catch_body`, which also truncates its scope instead of popping it so a `break` cannot leave it behind. |
| 3 | **BLOCKER** `run_iteration_body` popped its per-turn scope only on the success path, and `loop_control` was taken only on the success path, so `try { break } finally { fail }` leaked the scope *and* left a `Break` pending — which the next loop's first turn consumed, ending a loop that never contained a `break`. | The turn truncates `locals` to the height it found however it ends, clears the signal on the failing path in `run_iteration`, and `call_user_function` saves and restores `loop_control` alongside `loop_depth`. |
| 4 | **BLOCKER** `src/bytecode/vm.rs` `abandon_handlers` popped to `frames[frame].handler_base`, so a `break` out of a loop *inside* a `try` also dropped and ran the `finally` of the `try` enclosing the loop — and stopped that `try` catching anything. | `Handler` records the frame and the offset of its `TRY`; `abandon_handlers` drops only the handlers whose region starts inside the body being left. The `try` the loop is written inside stays installed and its `finally` is owed at the end of its own region, as the tree-walking VM has it. |

Each fix is pinned by a test that was watched failing without it — the "Fails
without" column under "Tests added" — and by corpus programs that the
differential test compares across both VMs.

Three more things came out of writing those tests, each fixed here and recorded
in FINDINGS rather than left as a surprise:

- `break_loop` **panicked** when the `finally` it ran failed and was handled:
  the handler unwinds the loops its `try` was written inside, so the entry the
  jump was leaving was gone by the time it was removed (FINDINGS §4).
- the bytecode VM **left a loop's variable bound** when a failure abandoned the
  loop, where the tree-walking VM puts the outer binding back (FINDINGS §5).

## Round 2 review

The reviewer found one BLOCKER and one MINOR.

| # | Finding | Fix |
|---|---|---|
| 1 | **BLOCKER** `loop_at` searched only the sites of the frame the instruction ran in, so a `break`/`skip` in a `catch`, `finally`, `test` or `object` body lexically inside a loop was refused there and honoured by the tree-walking VM (FINDINGS §6). | `LoopOwner`: a frame records the loop it was written inside, read from the instruction that entered it, and `break`/`skip` leave that loop and finish the frames between. A call frame records nothing, which is the function-body exemption. Four ordering rules the fix uncovered — an interrupted `finally` is still owed, the frames are finished before the loop entry is created, the loop variable stays bound until the `finally` has run, and the signal is read rather than taken — are recorded in FINDINGS §6. |
| 2 | **MINOR** `SPEC.md` and `docs/GRAMMAR.md` gave worked examples and no error case for the refusal the phase added. | Both state the refusal, that it is a runtime error `try ... catch error` catches, and the two placements refused for the same reason: a function body, and an `object` body outside any loop. |

Taking the ownership lookup back out turns fourteen corpus programs red —
`tests/test_loop_control.rb` among them — so the shapes are pinned by the
differential rather than described.

## Round 3 review

The reviewer found one BLOCKER and two MAJORs. The BLOCKER is FINDINGS §8, which
round 2 recorded as an open pre-existing defect; the reviewer's escalation is the
right one, because it is a panic rather than a wrong answer and `rb vm` takes the
process down with it on ordinary user input.

| # | Finding | Fix |
|---|---|---|
| 1 | **BLOCKER** `src/bytecode/vm.rs:1048` — `finish_object` did `pending_object.take().expect("an object declaration being assembled")`, and `pending_object` was one `Option`, so `object A / object B / has c / end / end` let the inner declaration take the outer's place and the outer finish panicked. The tree-walking VM prints `done` and exits 0 on the same program (FINDINGS §8). | `BytecodeVm::pending_objects` is a `Vec`: a declaration is pushed by `DEF_OBJECT` and popped by `finish_object`, which reports a `RuntimeError` rather than panicking. `has` and `to can` mean the innermost, `def_object` asks `object_is_declared` so a name an open declaration has taken is refused with the message the tree gives, and `resolve_object` lets a parent chain walk a declaration still being assembled, which is what makes `object B extends A` nested in `object A` work here as it does there. `discard_frames_above` pops the declaration of a body it drops and `run` clears the stack. |
| 2 | **MAJOR** `docs/GRAMMAR.md:255` — `skip_statement = 'skip' [ expression ]` promised an operand `src/parser.rs:447` does not parse: `TokenKind::Skip` advances once and returns, so `skip 1` is two statements. | The grammar was the thing that was wrong — there is nothing for an operand to say, since the loop a `skip` acts on is the one around it and the turn it goes to is the next one — so it reads `'skip'`, and `SPEC.md` and the prose around the rule now say neither statement takes one. `edge_neither_break_nor_skip_takes_an_operand` pins the behaviour the grammar describes. |
| 3 | **MAJOR** `src/vm.rs:732` — `Statement::ForRange` runs through `run_iteration`, but no break/skip test covered the range form on either VM. | The premise needed correcting: `for each i from a to b [by s]` does not parse (`parse_for` accepts `for each x in <expression>` and nothing else), so the requested program is a `ParserError` and `Statement::ForRange` is unreachable from source — SPEC.md and `docs/GRAMMAR.md` both document a form the parser does not have (FINDINGS §10). The coverage is delivered where the loop form exists: `range_loop` in `tests/loop_control_test.rs` builds the AST the way `tests/numeric_edge_test.rs` does, and five tests run `break`, `skip`, the stepped form and the empty range through it on **both** VMs. |

Each fix was checked by taking it back out and watching the tests that name it
fail; the numbers are in "Tests added" below.

## What changed

| File | Lines | What |
|---|---|---|
| `src/vm.rs` | +199 −52 | `LoopControl` (`Break`/`Skip`), `Vm::loop_depth`, `Vm::loop_control`, `run_iteration` / `run_iteration_body` / `run_block` / `run_catch_body` / `raise_loop_control`; all five loop forms (`for each`, `for each from`, `repeat`, `while` — plus the range form) run one turn through `run_iteration` and consume the signal; `Statement::Break` / `Statement::Skip` raise it or are refused; `call_user_function` zeroes `loop_depth` and clears the signal for a call |
| `src/vm.rs` | round 1 | every block — `if`, `try`, `test`, `object` body, function body — runs its statements through `run_block`, so a `break` ends the block it is written in and not only the loop; the per-turn and per-catch scopes are truncated rather than popped, so a failing turn and a `break`ed-out-of catch leave nothing behind |
| `src/bytecode/vm.rs` | +165 −18 | `BREAK` and `SKIP` are wired up: `loop_at` finds the innermost loop from the instruction offset, `break_loop` leaves it, `skip_loop` jumps to its `top`, `abandon_handlers` runs and drops the `finally` of every `try` the jump left behind; round 1 adds the region test in `abandon_handlers`, `loop_index` (FINDINGS §4) and `unwind_loops` (FINDINGS §5) |
| `src/bytecode/vm.rs` | round 2, +332 −30 | `LoopOwner` and `Frame::loop_owner`: a frame records the loop it was written inside, read from the instruction that entered it, and `loop_at` answers from it when the frame has no loop of its own; `unwind_frames_above` finishes the frames between the instruction and the loop; `Exit::Left` and `BytecodeVm::abrupt_exit` carry the record out through the nested drivers; `put_back_binding` holds a loop variable back until the `finally` a signal passed through has run; `abandon_handlers` counts the handlers of crossed frames as passed through |
| `tests/test_lists.rb` | +10 −8 | the two tests that pinned the no-op rewritten to the real semantics |
| `tests/loop_control_test.rs` | +1143 | new: 38 Rust `#[test]` functions (32 in round 0–1, 6 in round 2) |
| `tests/test_loop_control.rb` | +601 | new: 38 Redblue `test` blocks (29 in round 0–1, 9 in round 2) |
| `tests/bytecode_vm_test.rs` | +97 | round 1: six generated corpus programs for the loop-inside-a-`try` shapes and the failing-`finally` one; round 2: thirteen for the shapes of FINDINGS §6, all compared across both VMs, and the two keys that stated the old no-op renamed |
| `SPEC.md`, `docs/GRAMMAR.md` | round 2 | the refusal, that it is catchable, the two placements refused for the same reason, and the two rules the fix had to pin down (FINDINGS §7) |
| `src/bytecode/vm.rs` | round 3, +131 −26 | `pending_objects`: the `object` declarations the open bodies are assembling are a stack rather than one slot, `finish_object` pops the innermost and reports rather than panicking, `resolve_object` lets a parent chain walk a declaration still being assembled, `object_is_declared` refuses a name an open declaration has taken, `discard_frames_above` and `run` keep the stack the same length as the open bodies (FINDINGS §8) |
| `tests/object_model_test.rs` | round 3, +106 | nine tests for a declaration nested in another declaration's body: both types declared, a nested declaration extending its own body and one two levels up, the two refusals, a failure caught outside both, a nested body that recovers, and a body in a loop left by a `break` |
| `tests/test_object_model.rb` | round 3, +84 | the same seven shapes in Redblue, which the differential test therefore runs on **both** VMs |
| `tests/loop_control_test.rs` | round 3, +238 | `range_loop` and the five tests that run `break`, `skip`, the stepped form and the empty range through `Statement::ForRange` on both VMs, plus the two tests pinning that neither jump takes an operand |
| `tests/bytecode_vm_test.rs` | round 3, +160 | four named tests for the nested-declaration fix and twelve `object/…` corpus programs for its shapes, all compared across both VMs |
| `SPEC.md`, `docs/GRAMMAR.md` | round 3 | `skip_statement` loses the operand it never had, and both files say neither `break` nor `skip` takes one |

### Why `src/bytecode/vm.rs` is in this phase

The bytecode VM's `BREAK`/`SKIP` handlers were deliberate no-ops
(`// BREAK: no effect`), documented as such, and
`tests/bytecode_vm_test.rs::a_corpus_of_programs_runs_identically_on_both_vms`
compares the two VMs over a 200-program corpus that contains `skip` programs.
Making the tree-walking VM honest while leaving the bytecode VM's no-op in place
turns that gate red on four programs, which is the "two VMs disagreeing about
what a program means" the old comment called worse than an unfinished feature.
Both now implement the same semantics and the differential is green again.

Two details worth naming, because both are ways the obvious implementation is
wrong:

- **`loop_at`, not the top of the loop stack.** A `while` draws its loop entry
  when it turns over, so on its *first* turn there is no entry for it. Taking
  `self.loops.last()` there would leave an **outer** loop — the wrong loop,
  silently shortened. The loop is therefore found from the instruction offset,
  which is inside exactly one loop body.
- **`abandon_handlers`.** The instruction that pops a `try` handler is the `NOP`
  closing its protected region, and a jump out of a loop never reaches it. An
  abandoned handler would go on catching failures raised long after the program
  had left the region. The `finally` runs, as it does in the tree-walking VM.

Round 2 adds three more, all from FINDINGS §6:

- **`LoopOwner`, not the frame's own sites.** A `test`, `catch`, `finally` or
  `object` body runs in a child frame, so the loop around it is in another
  frame's block. Ownership is recorded when the frame is pushed, from the
  instruction that entered it, which is what makes it lexical.
- **The frames above the loop are finished *before* the loop entry is created.**
  A frame's `unwind_loops` drops every loop above the base that frame recorded,
  and a block frame was pushed before the loop it is inside drew its entry — so
  an entry made first went with them, and on a `while`'s first turn the `break`
  became a no-op.
- **The exit is recorded, and read on the way out.** Crossing frames stops every
  nested driver, so the first one to stop must not consume the record: `drive`
  reports `Exit::Left` to each caller in turn and the driver that carries on
  clears it.

### Semantics decided, and why

- **`break`/`skip` with no enclosing loop is a `RuntimeError`**
  (`'break' is only valid inside a loop`). A no-op there is a program that runs
  to completion reporting success; that is the defect being fixed, not a
  tolerance. The error names the statement, and `try ... catch error` catches it.
- **A `break` in a function body is refused.** A function body is not lexically
  inside the loop that calls it, so letting the signal reach out would end the
  *caller's* iteration from two scopes away. `call_user_function` saves
  `loop_depth`, sets it to 0 and restores it, including on the failing path, so a
  `break` there is refused and the caller's loop survives the refusal.
- **A `break`/`skip` inside a `try` body still works**, and the `finally` of that
  `try` runs — an abrupt exit from a protected region is not a failure.
  Pinned by `loop control: a break inside a try still leaves the loop and runs the
  finally`.
- **A `break`/`skip` ends the block it is written in, not only the loop** (round
  1). The statements after it in the same `if` branch, `try` body, `catch` body,
  `finally` body or `test` body are part of the turn the signal stopped. A block
  that kept running them would leave the signal raised while the program went on
  past the loop, and the next loop to reach a `break` of its own would find it
  already set.
- **A turn that fails leaves nothing behind** (round 1) — neither its scope nor
  its signal. A failure unwinds out of the loop on its own, so a signal raised
  before it has no loop to act on, and the next loop in the program must not
  inherit it.
- **A `break` out of a loop inside a `try` does not end that `try`** (round 1).
  The jump lands back inside the region the `try` protects, so its handler stays
  installed and its `finally` is owed at the end of its own region.
- **A `finally` is owed in full, and is a block like any other** (round 1). The
  pending signal is held across the `finally` body rather than left in place while
  it runs: in place it would stop the block after its first statement, and a
  `finally` that cleans up one thing and not the other is not a cleanup. A
  `break` written in a `finally` ends the statements after it in that block and
  still names the loop already being left.

## Tests added

`tests/loop_control_test.rs`, 38 tests. The "Fails without" column names what
each round-1 test fails on when its fix is taken back out — every one of them was
watched failing, and each of the four findings has at least one. The round-2 tests
are in their own table below.

| Test | Edge class covered | Fails without |
|---|---|---|
| `break_leaves_a_for_each_loop` | the reproduction: prints `1`, exit 0 | — | — |
| `skip_goes_on_to_the_next_value` | the reproduction: prints `1`, `3` | — |
| `break_and_skip_work_in_repeat_and_while` | both statements × `repeat`/`while` | — |
| `a_break_in_a_nested_loop_leaves_only_the_inner_one` | nesting: outer counter keeps counting (3 outer turns, 1 inner turn each) | — |
| `a_skip_in_a_nested_loop_advances_only_the_inner_one` | nesting: 2 × 2 turns, a skip is still a turn | — |
| `break_outside_a_loop_is_a_clean_runtime_error` | **asserts a failure**: error names `break` | — |
| `skip_outside_a_loop_is_a_clean_runtime_error` | **asserts a failure**: error names `skip` | — |
| `edge_a_refused_break_is_catchable_with_try_catch_error` | **asserts a failure**: caught, and the program carries on | — |
| `edge_a_break_in_a_function_body_is_refused_and_the_caller_survives` | **asserts a failure**: refused, caller's loop runs to its end | — |
| `edge_break_as_the_only_statement_of_a_loop_body` | body is a single statement | — |
| `edge_break_as_the_first_and_last_statement_of_a_body` | first and last position | — |
| `edge_break_two_blocks_deep_inside_a_conditional` | `if` nested two blocks deep | — |
| `edge_skip_on_the_final_iteration_of_a_for_each` | `skip` on the last value | — |
| `edge_a_break_stops_the_loop_before_the_iteration_cap` | resource: a broken-out-of turn stops the loop instead of spending the cap | — |
| `edge_a_skip_is_charged_as_one_iteration` | resource: exactly the cap allowed, cap+1 is a clean error | — |
| `edge_break_and_skip_in_a_loop_over_an_empty_list` | empty | — |
| `edge_singleton_list_runs_one_turn_and_ends` | singleton | — |
| `edge_break_in_a_loop_over_a_non_list_never_runs` | type mismatch (`for each i in 5`, `repeat "five" times`) | — |
| `edge_a_break_stops_a_loop_whose_count_is_beyond_i64` | numeric boundary (`repeat 99999999999999999999 times`) | — |
| `edge_break_and_skip_over_unicode_values` | unicode (combining acute, CJK, emoji) | — |
| `edge_break_in_an_object_body_is_refused` | **asserts a failure**: a block that is not a loop | — |
| `the_corrected_list_tests_still_exist_and_pass` | the two corrected `tests/test_lists.rb` tests still exist, still assert the real answers, and the file has 0 failures | — |
| `a_break_in_an_if_branch_does_not_run_the_statements_after_it` | round 1, finding 1: both branches, with each turn printing its value first so a leaked line is visible | `Statement::If` running its branch as a plain `for` |
| `a_skip_in_an_if_branch_does_not_run_the_statements_after_it` | round 1, finding 1 for `skip` — the value after the skipped one is printed, and it is printed once | the same |
| `a_break_in_a_try_body_does_not_run_the_statements_after_it` | round 1, finding 2: the protected code, the catch and what follows the `try`, with the `finally` still owed | `Statement::Try` running its blocks through `execute_statements` |
| `a_break_in_a_conditional_inside_a_try_body_ends_that_turn` | round 1, finding 2: the `break` inside an `if` inside the protected code — the shape the report was written against | the same |
| `a_break_in_a_catch_body_leaves_the_loop_and_runs_the_finally` | round 1, finding 2: the catch is a block too, and the `finally` is owed on the turn that broke | the same |
| `a_failed_turn_leaves_no_signal_for_the_next_loop` | round 1, finding 3: a `finally` that fails after a `break`; the *next* loop must run all three of its turns | the signal surviving a failed turn |
| `a_failed_turn_does_not_leak_the_scope_of_its_loop_variable` | round 1, finding 3 from the scope side: the name read after the `try` is the program's own, not the loop's last value | the per-turn scope surviving a failed turn |
| `a_catch_body_does_not_leave_its_scope_behind` | the invariant the two above rest on: a catch's binding dies with its scope, whether the body ran, failed or was left by a `break` | — (guards both) |
| `a_finally_runs_every_statement_on_the_way_out_of_a_break` | round 1: the cleanup a `break` passes through is not half a cleanup — both statements run | the signal left in place across the `finally` |
| `a_break_in_a_finally_ends_that_block_and_still_ends_the_loop` | round 1: the `finally` is a block, and its signal reaches the loop being left | the same |
| `tests/test_loop_control.rb` — 29 `test` blocks | the same matrix written in Redblue, including `loop_control_edge_*` for first/last statement, two blocks deep, single-statement body, `skip` on the final iteration, `skip` on every turn, empty list, both refusals caught with `try`/`catch error` inside a loop, the nested case, and the nine round-1 blocks — which the differential test therefore runs on **both** VMs |
| `tests/bytecode_vm_test.rs` — 6 corpus programs | round 1: `break`/`skip` out of a loop inside a `try` (a plain loop, a nested one and a `while`), a `finally` that fails on the way out of a `break`, and a `finally` that must run every statement of itself — each compared across both VMs |

### Round 2 tests

`tests/loop_control_test.rs` grew by six, `tests/test_loop_control.rb` by nine
`test` blocks and `tests/bytecode_vm_test.rs` by thirteen corpus programs. The
Rust tests pin what the tree-walking VM answers for each shape; the Redblue blocks
and the corpus programs are what pin that the *bytecode* VM answers the same, since
the differential test and `rb test` run them on both VMs.

| Test | Edge class covered | Fails without |
|---|---|---|
| `a_break_in_a_catch_body_leaves_the_loop_the_catch_is_written_in` | round 2, finding 1: the `catch` is a block written inside a loop; the `finally` is still owed and the turn stops | `loop_at` answering from the frame's own sites |
| `a_skip_in_a_catch_body_advances_the_loop_the_catch_is_written_in` | the same for `skip`: the next value is drawn and the rest of the turn abandoned | the same |
| `a_break_in_a_test_body_leaves_the_loop_the_test_is_written_in` | a `test` body, the other block with a frame of its own | the same |
| `a_break_in_an_object_body_leaves_the_loop_the_declaration_is_written_in` | an `object` body, and the companion to `edge_break_in_an_object_body_is_refused` — the same body outside a loop is refused | the same |
| `edge_a_break_in_a_block_in_a_function_body_is_still_refused` | **asserts the exemption**: the refusal still holds, and the caller's loop runs to its end | the function-body exemption being dropped |
| `a_finally_a_catch_break_passed_through_still_reads_the_loop_variable` | round 2: the turn has not ended while the `finally` runs, so the loop's variable is the turn's own and the outer binding is back afterwards | `put_back_binding` restoring before the `finally` ran |
| `tests/test_loop_control.rb` — 9 `test` blocks | the same six shapes written in Redblue, plus the loop variable, the interrupted `finally` inside a `catch`, and a block inside a block — read by the differential test, so both VMs run them | the same |
| `tests/bytecode_vm_test.rs` — 13 corpus programs | one per shape, plus a `while`, a loop inside a function, a nested loop inside a `test`, and the three ordering rules above — each compared across both VMs | the same |

### Round 3 tests

`tests/loop_control_test.rs` grew by seven, `tests/object_model_test.rs` by nine,
`tests/test_object_model.rb` by seven `test` blocks and `tests/bytecode_vm_test.rs`
by four named tests and twelve corpus programs. Three of the four findings have a
test that fails with its fix taken back out, and the numbers in the last column are
what was watched failing.

| Test | Edge class covered | Fails without |
|---|---|---|
| `objects_an_object_declared_inside_an_object_body_declares_both_types` | round 3, finding 1: the reviewer's program. Both types are registered, each with its own field | `pending_object` as one `Option` — the bytecode VM **panics** at `finish_object` |
| `objects_a_nested_declaration_may_extend_the_body_it_is_written_in` | the same with `extends`, one rule from the panic: a parent chain that cannot see a declaration still being assembled | the same (`Object 'Inner' extends 'Outer', which is not declared` against the tree's two printed lines) |
| `edge_objects_a_nested_declaration_reusing_the_enclosing_name_is_refused` | **asserts a failure**: three placements of a name an open declaration has taken, each refused with `Object 'X' is already declared` on both VMs | the same (panics) |
| `edge_objects_an_object_body_nested_in_a_loop_can_break_out_of_it` | nesting/recursion: the abrupt-exit path, where the declaration stack and the frame stack have to agree about which declaration is whose | the same (panics) |
| `tests/object_model_test.rs` — 9 tests | the tree-walking VM's answers for each shape, which is what the bytecode VM above is held to: both types declared, `extends` its own body, `extends` two levels up, the two refusals, the cycle, a failure caught outside both bodies, a nested body that recovers, and a body in a loop left by a `break` | — (the tree already ran these; they pin the answers the bytecode VM must match) |
| `tests/test_object_model.rb` — 7 `test` blocks | the same shapes in Redblue, so the differential test runs them on **both** VMs | `pending_object` as one `Option` — `rb vm tests/test_object_model.rb` panics |
| `tests/bytecode_vm_test.rs` — 12 `object/…` corpus programs | one per shape, plus a three-deep nesting, two declarations in one body, a failure caught outside both, a failure caught inside the outer one, and a body in a loop — each compared across both VMs | the same (the differential aborts on the panic) |
| `a_break_in_a_range_loop_ends_it` | round 3, finding 3: the reviewer's program, over the AST the parser cannot build | `Statement::ForRange` running its body as a plain statement list — 3 of the 7 new tests go red |
| `a_skip_in_a_range_loop_advances_to_the_next_value` | the `skip` companion | the same |
| `edge_break_and_skip_in_a_stepped_range_loop` | numeric boundary / step: the `by` form, both statements | the same |
| `edge_break_and_skip_in_a_range_loop_that_runs_no_turns` | empty: a range whose start is past its end runs no turns, so neither statement runs | — (guards the empty range the four above count on) |
| `edge_both_vms_answer_the_same_for_break_and_skip_in_a_range_loop` | the finding itself: three bodies × two strides, compiled and run through **both** VMs | the same |
| `edge_neither_break_nor_skip_takes_an_operand` | malformed_input for the jumps: `skip 1` and `break 1` are a jump and a separate statement, which is what `docs/GRAMMAR.md` now says | — (pins the behaviour the corrected grammar describes; a document has no code to fail) |
| `edge_a_skip_leaves_the_statements_after_it_unreached` | the block rule applied to a body that is a single statement | round 1's block fix |

### The two corrected `tests/test_lists.rb` tests

Both were rewritten, not deleted. Their names changed because the old names
stated the defect — `lists: break inside a loop does not truncate it` cannot
assert `total is 3`:

| Was | Now | Was | Now |
|---|---|---|---|
| `lists: a conditional inside a loop does not disturb the iteration` | `lists: a skip inside a loop leaves its own iteration out` | `expect seen to be 4` | `expect seen to be 3` |
| `lists: break inside a loop does not truncate it` | `lists: break inside a loop ends it at the value that broke` | `expect total to be 10` | `expect total to be 3` / `expect total to be 8` |

`the_corrected_list_tests_still_exist_and_pass` pins the new names, the new
assertions and a clean run of the file, so the correction cannot be reverted by
editing the numbers back.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test --all-targets` | 552 passed, 0 failed, 0 ignored (24 binaries) |
| `./rbops/verify.sh phase-025` | **not run — `rbops/` is not in this checkout** |

The first three were re-run after the round-3 fixes; the count moved from 532 to
552: `tests/loop_control_test.rs` +7, `tests/bytecode_vm_test.rs` +4,
`tests/object_model_test.rs` +9.

`rbops/` is absent from the working directory (`ls` shows `.github`, `phases`,
`src`, `tests`, `examples`, `modules`, `target`, and no `rbops/`), so
`./rbops/verify.sh phase-025` exits `No such file or directory`. The pipeline
lives outside this checkout and I was instructed not to inspect it. What was run
instead, by hand, from the same phase prompt:

| Check | Result |
|---|---|
| `rb test` (every `.rb` under `tests/`) | 276 run, 276 passed, 0 failed |
| `./target/debug/rb run examples/*.rb` | 6/6 exit 0 |
| `./target/debug/rb run modules/*.rb` | 2/2 exit 0 (`MathUtils.rb` passes as well) |
| `tests/loop_bounds_test.rs` | 22 passed, 0 failed |
| `tests/bytecode_vm_test.rs` (both differential tests) | 37 passed, 0 failed, over a corpus of 380 programs |
| `cargo test --doc` | 1 passed, 0 failed |

Beyond the gates, every fix above was checked by taking it back out and watching
the test that names it fail: the `if`/`try` block fixes take down 13 tests, the
failed-turn fixes take down the two that pin them, and each bytecode fix takes
down one of the six round-1 corpus programs (the fourth one panics the
differential test without the `loop_index` guard). Round 2's was checked the same
way: with `loop_at` answering from the frame's own sites again — the one line the
fix adds — the differential reports **14 of 368 programs disagree**, the thirteen
new corpus programs and `tests/test_loop_control.rb` among them. Round 3's two code
fixes were checked the same way: `git checkout -- src/bytecode/vm.rs` puts the
`expect` back, and each of the four new `tests/bytecode_vm_test.rs` tests fails —
three of them by panicking inside `src/bytecode/vm.rs` rather than by an assertion,
and `tests/test_object_model.rb` panics under `rb vm` with it. On the tree side,
taking `Statement::ForRange` off `run_iteration` — first dropping only the `Break`
check, then running the body as a plain statement list — takes down 3 and 4 of the
seven new `tests/loop_control_test.rs` tests respectively. The VERIFY gates are
unaffected by that dance — the fixes are in, not out.

If `verify.sh` enforces anything beyond these four commands and the examples,
this phase is unverified on that axis.

## Test-requirement matrix

- **empty** — covered: `edge_break_and_skip_in_a_loop_over_an_empty_list`
  (a `for each` over `[]` with a `skip` runs no turns),
  `edge_break_and_skip_in_a_range_loop_that_runs_no_turns` (round 3: a range
  whose start is past its end), and
  `loop control: a loop over an empty list is unaffected by skip and break`.
- **singleton** — covered: `edge_singleton_list_runs_one_turn_and_ends` (a
  one-element list, both statements).
- **boundary** — covered: `edge_skip_on_the_final_iteration_of_a_for_each` (the
  last value) and `break_leaves_a_for_each_loop` (the first value, index 0);
  `edge_a_skip_is_charged_as_one_iteration` is exactly-cap / cap+1; round 3 adds
  `a_break_in_a_range_loop_ends_it` (the loop stops at the value that broke, and
  that value's own turn is gone) and `edge_break_and_skip_in_a_stepped_range_loop`
  (the `by` form, whose stride the two statements do not consult).
- **out_of_bounds** — N/A. `break` and `skip` are statements that take no
  operand and index nothing, so they have no out-of-range case of their own. The
  nearest thing — the ends of a `for each` — is the empty-list and final-iteration
  tests above. List indexing bounds are `tests/index_bounds_test.rs`, untouched.
- **type_mismatch** — covered: `edge_break_in_a_loop_over_a_non_list_never_runs`
  (`for each i in 5`, `repeat "five" times` — the body never runs, and that stays
  true rather than becoming an error).
- **numeric_boundary** — covered:
  `edge_a_break_stops_a_loop_whose_count_is_beyond_i64`
  (`repeat 99999999999999999999 times` with a `break` on the first turn: bounded,
  no panic, no hang). Zero, negative and fractional counts are
  `edge_fractional_and_negative_counts_run_zero_times` in
  `tests/loop_bounds_test.rs`, unchanged and still green; the range form's own
  counter is `edge_a_numeric_for_range_cannot_step_into_a_non_finite_number` in
  `tests/numeric_edge_test.rs`, unchanged, and round 3 adds the two statements
  *around* that counter in `edge_break_and_skip_in_a_stepped_range_loop`.
- **unicode** — covered: `edge_break_and_skip_over_unicode_values` (combining
  acute, CJK, emoji values through both statements).
- **nesting_recursion** — covered: `a_break_in_a_nested_loop_leaves_only_the_inner_one`,
  `a_skip_in_a_nested_loop_advances_only_the_inner_one`,
  `edge_a_break_in_a_function_body_is_refused_and_the_caller_survives` (a third
  scope, a function, called from a loop), the round-1 corpus program
  `flow/a-break-in-a-nested-loop-inside-a-try-leaves-the-outer-try-installed`
  (a loop nested in a loop nested in a `try`, on both VMs), the round-2 corpus
  programs `flow/a-break-in-a-block-within-a-block-inside-a-loop-leaves-that-loop`
  (a `test` inside a `test` inside a loop) and
  `flow/a-break-in-a-catch-inside-a-loop-inside-a-test-leaves-the-inner-loop` (a
  `try` inside a `catch` inside a loop inside a `test`, on both VMs), and —
  unchanged and
  still green — `edge_deeply_nested_loops_terminate` (16 nested loops) and
  `edge_nested_recursion_is_bounded_by_the_step_budget` in
  `tests/loop_bounds_test.rs`, which is where nesting at depth and through a
  function call are already owned. Round 3 adds a fifth scope: a declaration
  nested in a declaration's body, three deep in
  `object/three-nested-object-bodies` and in
  `edge_object_a_nested_declaration_may_extend_two_levels_up`, on both VMs, and an
  `object` body written in a loop and left by a `break` in
  `edge_objects_an_object_body_nested_in_a_loop_can_break_out_of_it`.
- **duplicate_missing_keys** — N/A. `break` and `skip` read no record field, bind
  no name and write no record, so there is no key to duplicate or miss. Record
  key handling is `tests/record_order_test.rs`, untouched by this change.
- **malformed_input** — covered by the structural form: there is no operand that
  can be malformed, and the one invalid *placement* — a `break` in a block that
  is not a loop — is asserted in four shapes:
  `break_outside_a_loop_is_a_clean_runtime_error`,
  `edge_break_in_an_object_body_is_refused` (an `object` body runs once, at
  declaration, and is not a loop),
  `edge_a_break_in_a_function_body_is_refused_and_the_caller_survives` and, from
  round 2, `edge_a_break_in_a_block_in_a_function_body_is_still_refused` (a block
  of the function's own, which is the case the frame model made ambiguous). All
  four are `Error::Runtime`, never a panic and never a silent no-op. Round 3 adds
  the operand question the corrected grammar settles:
  `edge_neither_break_nor_skip_takes_an_operand` asserts that `skip 1` and
  `break 1` are a jump followed by a separate statement, and
  `edge_a_skip_leaves_the_statements_after_it_unreached` asserts the block rule for
  a body that is a single statement. Unterminated
  `end`, stray tokens, BOM and CRLF are lexical/parser concerns owned by
  `tests/lexer_robustness_test.rs` and `tests/parser_hardening_test.rs`, which
  this change does not touch.
- **resource_limit** — covered: `edge_a_break_stops_the_loop_before_the_iteration_cap`
  (a `break` ends the loop instead of running a 10-turn loop into a cap of 2),
  `edge_a_skip_is_charged_as_one_iteration` (a `skip` costs one iteration, so a
  loop of skipped turns cannot outlive its cap), and
  `edge_a_break_stops_a_loop_whose_count_is_beyond_i64`. The whole of
  `tests/loop_bounds_test.rs` (22 tests) passes, including the step-budget tests,
  so the new per-turn path is inside the existing budget. A `break` that runs
  `finally` blocks on the way out costs those bodies one step each, which is
  charged the same way as any other statement.

## Invariants touched

- None. `Value`'s variants, `Error`'s variants, `.rb`, `to … end`, `set x to …`,
  `say`, the trailing-comma and `{interp}` syntax and every test in `tests/` that
  did not pin the defect are unchanged. `examples/*.rb` and `modules/*.rb` all
  still exit 0.
- `break` outside any loop is now an error where it was a silent success. That
  is the point of the phase, and it is the one place a previously-accepted
  program is now rejected — deliberately, with a message naming the statement.
- Round 1 narrowed two things a caller could observe. A loop variable is now
  released however its turn ends, so a name the program bound outside a loop
  reads its own value again after a turn that *failed* where it used to read the
  loop's last value. And a `catch` body leaves nothing behind when it fails or is
  left by a `break`, where its binding used to survive into the enclosing block.
  Both are the same rule the tree-walking VM already followed on the paths that
  did not fail, and the bytecode VM's `unwind_loops` was changed to match, so the
  two VMs agree on both.
- Round 2 narrowed one thing on the bytecode VM, and only there: a `break` or a
  `skip` in a `catch`, `finally`, `test` or `object` body written inside a loop is
  a jump now, where it was the refusal `'break' is only valid inside a loop`. A
  caller reading the exit code was already getting a failure there, so no program
  that ran before this phase gets a different answer from it — but a program whose
  `catch` body broke out of a loop and was caught by a `try` of its own no longer
  stops with that message.
- Round 2 also pins two rules the tree-walking VM already followed, so nothing
  changes for it: the loop's variable is still bound while a `finally` runs on the
  way out of a `break`, and an interrupted `finally` runs even when the signal
  passed through a block with a frame of its own.
- Round 3 changed no answer either VM gives. On the bytecode VM the only programs
  whose behaviour moved are the ones that *panicked* — `object A / object B / has
  c / end / end` and every shape around it — and they now run. `object A` written
  inside `object A`'s own body was also a panic before, and is now the refusal the
  tree-walking VM has always given. `docs/GRAMMAR.md` no longer documents an
  operand `skip` never had; `SPEC.md` says in words that neither statement takes
  one.

## Known gaps / follow-ups

- A `repeat … until` loop does not exist in the parser, so there is no fifth
  loop form to wire up. `SPEC.md:412-417` documents it; it belongs to a phase
  that adds the statement, not this one.
- `return` inside a `break`ed-out-of region is unchanged: `return` yields a value
  and the next statement runs, which is a separate pre-existing gap recorded in
  `src/bytecode/vm.rs` (`return_value`'s comment). Not in scope here.
- A `break` or a `skip` in a block that has its own frame in the bytecode VM — a
  `test`, `catch`, `finally` or `object` body with no loop of its own, written
  inside a loop — is refused there and honoured by the tree-walking VM. Recorded
  in FINDINGS §6 in round 1 and **fixed in round 2**: a frame now records the loop
  it was written inside, and an abrupt exit finishes the frames between the
  instruction and it. What is left is the shape below.
- An abrupt exit that crosses frames is one-way: a `catch` or `finally` body that
  the exit has passed cannot be resumed, because the handler that owns it has
  already been told the region was left (`Exit::Left`). That is what the
  tree-walking VM does too — the signal propagates out through each statement
  rather than back into one — but it means the bytecode VM cannot resume a
  `catch` body at an offset, and nothing in the language asks it to.
- ~~An `object` declared inside an `object` body **panics the bytecode VM**~~ —
  **fixed in round 3.** Recorded as open in FINDINGS §8, escalated to a BLOCKER by
  the round-3 review, and closed here: the declarations being assembled are a
  stack rather than one slot, `finish_object` reports rather than panicking, a name
  an open declaration has taken is refused with the tree's message, and a parent
  chain may walk a declaration still being assembled.
- An `object` body the program leaves through a **failure** is still neither
  registered nor bound on the bytecode VM, where the tree-walking VM has already
  registered it. Found in round 3 while writing §8's corpus programs and
  pre-existing (`git show HEAD:src/bytecode/vm.rs` disagrees on the same program);
  it has nothing to do with nesting. FINDINGS §9 records why the honest fix needs
  to know whether a failure landed in a `has` default or after a body's last
  declaration, which the bytecode VM does not track. No corpus program depends on
  it.
- `for each i from a to b [by s]` is documented in `SPEC.md` and
  `docs/GRAMMAR.md` and **cannot be parsed**, so `Statement::ForRange` is
  unreachable from source and the corpus's range programs compare two parse
  errors. Found in round 3; FINDINGS §10 records it. The phase's own change to
  that loop form is covered through the AST instead — `range_loop` in
  `tests/loop_control_test.rs` and the five tests that run `break`, `skip`, the
  stepped form and the empty range through it on both VMs. Adding the parser
  production is a language change and belongs to the phase that adds it, the same
  call made above about `repeat … until`.
- `./rbops/verify.sh` could not be run in this checkout; see the Gates table.
