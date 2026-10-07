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

## Round 4 review

The reviewer found one BLOCKER and two MAJORs. The BLOCKER is a block the round-1
fix missed; one of the MAJORs is the same defect in a second block, and the other
is FINDINGS §9, which did not hold up under the check the review asked for.

| # | Finding | Fix |
|---|---|---|
| 1 | **BLOCKER** `src/vm.rs` `Statement::Unless` ran its body as a plain `for stmt in body { execute_statement }`, so `break`/`skip` in an `unless` body did not stop the rest of that body — the tree VM printed the statement after the `break` where the bytecode VM, whose `unless` body compiles into the enclosing block, jumped over it. It was the only block in the file still run that way; the `if` branches were converted in round 1. | The body runs through `run_block`, like every other block. |
| 2 | **MAJOR** `src/vm.rs` `declare_module` ran the module body through a loop that consulted the failure and not the signal, and `BytecodeVm::module` gave the module's frame no `loop_owner`, so a `break` in a module body written in a loop leaked the rest of the body on the tree VM and was **refused** (`'break' is only valid inside a loop`) on the bytecode one. | A module body runs where the declaration is written, so it is inside that loop exactly as an `object` body is — the function-body exemption is for a body that runs when it is called, and this one does not. The tree body goes through the block rule and publishes nothing when a jump leaves it; `module` records the loop it was written inside, and `unwind_frames_above` rolls the module back the way a failed body is rolled back. |
| 3 | **MAJOR** `src/bytecode/vm.rs` `discard_frames_above` dropped an `object` body abandoned by failure without registering the type, where `Vm::declare_object` registers it before running the trailing statements (FINDINGS §9). | The premise does not reproduce — the compiler splits an `object` body into a declaration block and the enclosing block, so a failure in the trailing statements happens with the type already registered on both VMs — and what *was* wrong in those lines is fixed: `truncate(frame.pending_base)` already drops the abandoned body's own entry, so the `pop` after it took the **enclosing** body's entry whenever the frame's base was above zero, which is what a `has` default that opens a declaration produces. FINDINGS §9 records the rule both VMs already follow, with the program that shows the old claim was stale. |

Fix 2 uncovered a third defect, which is the same rule seen from the other side and
is fixed with it: a `finally` a jump passes through has to run *after* the frames
the jump crossed are finished, because that is the order the tree-walking VM
unwinds in — the signal passes out through one block at a time. The bytecode VM ran
every crossed `finally` first and finished the frames afterwards, so a `set` in a
`finally` written around a module declaration landed in the module's scope instead
of the program's and was lost with it (`tf` against `t`). `abandon_handlers` now
finishes the frames above each handler's own frame before running that handler's
`finally`; `unwind_frames_above`'s loop body is the one-frame `finish_top_frame`.

## Round 5 review

The reviewer found one BLOCKER and two MAJORs. The BLOCKER is a scope removed
twice; one MAJOR is a test gap that is closed here, and the other is a language
question the specification does not answer, which FINDINGS §14 records with the
program that shows today's behaviour instead of changing it in a review round.
A race in the corpus harness turned up while re-running the gates and is fixed
with them (FINDINGS §15).

| # | Finding | Fix |
|---|---|---|
| 1 | **BLOCKER** `src/bytecode/vm.rs` `run_catch` pushed a scope for the `catch` body, pointed the catch frame's `locals_base` at it — and popped it *again* after driving the frame. Every path that pops a frame already truncates `locals` to that base, so a `catch` took two scopes and the second was the **enclosing** frame's: a `try`/`catch` inside a function body left the function without its parameter scope, and every name it read or assigned afterwards was `Unknown variable 'x'`. | The `pop()` is gone; the truncate is the whole rule, and it already covers all three ways the body can end. A top-level program never showed it — its names live in globals — and a loop's variable never showed it either, because that binding is not in `locals`. FINDINGS §13. |
| 2 | **MAJOR** a `skip` in a `test` or `object` body written inside a loop had no test on either VM — only the `break` shape did — and `Skip` through `LoopOwner` is a different instruction reaching the same lookup by a different path. | Four new pins: `a_skip_in_a_test_body_advances_the_loop_the_test_is_written_in` and `a_skip_in_an_object_body_advances_the_loop_the_declaration_is_written_in` in `tests/loop_control_test.rs`, `edge_both_vms_advance_the_loop_for_a_skip_in_a_test_or_an_object_body` for the two VMs together, and the two `test` blocks in `tests/test_loop_control.rb` plus two corpus programs the both-VMs differential reads. |
| 3 | **MAJOR** a `try`/`catch`/`finally` whose `catch` fails skips that `try`'s `finally` on both VMs, which the review read as contradicting `SPEC.md:464`'s "a `finally` is still owed on the way out". | **Not changed, and the reason is that the premise does not reproduce.** `SPEC.md:464` is about an *abrupt exit* — a `break` or a `skip` passing through a protected region is not a failure, and both VMs run that `finally`, which §6 and §12 pin. A `catch` that itself fails *is* a failure, and nothing in `SPEC.md` or `docs/GRAMMAR.md` says what a `finally` is owed after one; changing it needs three decisions the spec does not make (whether, which failure propagates when both fail, and what an abrupt exit out of that `finally` names). Both VMs already agree, so the differential is green either way. FINDINGS §14 records it with the program, the same call §10 records about `for each i from a to b`. |

Finding 1's fix is a deletion, and its tests were watched failing without it:
`edge_a_catch_body_gives_back_only_the_scope_it_pushed` goes red, and **2 of 407**
programs in the both-VMs corpus disagree, one of them `tests/test_control_flow.rb`.
Finding 2's coverage was checked the same way: taking the ownership fallback out of
`skip_loop` alone — the one line, so that `skip` no longer asks the frame what loop
it was written inside — turns **5 of 407** programs red, three of them the `catch`,
`finally` and `test` bodies that already had them.

## Round 6 review

The reviewer found three BLOCKERs, all in the `finally` and the iteration cap, and
all three were live: the first two are the two halves of one promise the language
makes in `SPEC.md`'s "Finally" and neither engine kept on both paths, and the
third is a `skip` the cap never saw. Fixing them uncovered two more defects that
were only unreachable *because* the first two were broken, both in the same two
functions; FINDINGS §16 has the transcripts and the reasoning.

| # | Finding | Fix |
|---|---|---|
| 1 | **BLOCKER** `src/vm.rs` `Statement::Try` returned `Err(failure)` **above** the `run_finally_body` call whenever the parser had found no `catch`, so a `try` with only a `finally` — the form `SPEC.md`'s "Finally" section documents, and the one where the cleanup is wanted most — skipped its cleanup on the failing path. The bytecode VM ran the cleanup there, so the two engines disagreed about which promise to keep. | The `finally` runs first and the failure is propagated after it, which is also what makes the failure the *enclosing* `try`'s to catch rather than this one's. `SPEC.md` and `docs/GRAMMAR.md` now say the order. |
| 2 | **BLOCKER** `src/bytecode/vm.rs` `handle_failure` answered `Ok(true)` — "handled" — for a handler with no `catch`: it ran the `finally` and resumed past the marked `NOP`. `try / set bad to 1 + "one" / end / say "after"` printed `after` and exited 0 on the bytecode VM and failed on the tree-walking one. A program that had failed ran to its end reporting success. | The search **loops**: a handler with no `catch` runs the `finally` it is owed and the search carries on from what is left, which is what the tree-walking VM's `?` reaches. Only a search that runs out of handlers answers `false`. Two things the loop needed, both fixed with it: a body that failed and was taken by a handler *outside* the region spends the original failure (so `true` where the handler stack has lost an enclosing one), and `end_try_after` now scans from the `TRY` rather than from the failing instruction, because a handler reached by skipping an inner `try` used to stop at that inner region and resume inside the protected code it had left. |
| 3 | **BLOCKER** `src/bytecode/vm.rs` `turn_over_to` (`SKIP`) jumps a `while` to its `top`, stepping over the backward `JUMP` that charges a turn — and unlike a sequence loop, a `while` has no `STORE` at its `top` to charge with. So a `while` that skipped every turn spent no iterations at all and was stopped by the ten-million-statement step budget, where the tree-walking VM reports `Maximum of N iterations`. | `turn_over_to` charges the turn it starts when the loop is a `while`, and clears the exit flag when that charge fails — the way `prepare_exit` reports a `finally` that could not run. |

The fourth change was not in the review and was found by the test written for
finding 3: a cap of N was N turns on the tree-walking VM and N + 1 on the
bytecode one, because that VM charges a `while`'s turn at the *end* of the previous
one and draws the loop's entry when it turns over rather than before it — so a
`while` under a cap of 3 ran four turns of body on the bytecode VM and three on the
other. Fixed by starting a `while`'s entry at one turn (an entry is drawn when the
loop turns over or an exit leaves it, which is after the turn it is leaving has
run), which is the same up-front charge the tree-walking VM makes and the one
finding 3's fix had to match.

Every fix here was watched failing with it taken back out, and each was taken out
alone:

| Taken out | What goes red |
|---|---|
| the `if !has_catch` branch in `src/vm.rs` | **3 of 411** programs: the corpus program `try/a-failure-with-nothing-around-it-stops-the-program-after-the-finally` (the tree no longer prints `cleaned`) and the two `test` blocks this round added to `tests/test_control_flow.rb` |
| the `continue` in `handle_failure` | **4 of 411** programs: the three `try/…` corpus programs added for findings 1 and 2, and `tests/test_control_flow.rb` again |
| the charge in `turn_over_to` | `edge_a_cap_counts_turns_on_both_vms_and_a_skipped_turn_is_one` and `edge_a_while_that_skips_every_turn_is_stopped_by_the_iteration_cap`, the second quoting the review's own symptom — the bytecode VM reporting `Step budget of 10000000` where the tree reports `Maximum of 5 iterations` |
| `iterations: usize::from(!site.iterator)` | `edge_a_cap_counts_turns_on_both_vms_and_a_skipped_turn_is_one`: a cap of 1 allowed 2 turns of a `while` |

The cap is invisible to the corpus, which runs at the published million and cannot
lower it from inside a program, so this round added
`tree_walk_capped`/`bytecode_capped`/`assert_agrees_capped` — the same
comparison the both-VMs corpus makes, at a cap a test can reach — and
`edge_a_skip_in_a_while_is_charged_as_one_iteration` in `tests/loop_control_test.rs`
for the tree-walking VM alone, which is where the rule was already right.

## Merge resolution — the resumed tree

This round was not a review round. The parked attempt came back merged onto a newer
`main`, and the merge left nine conflict hunks across two files. Every hunk was a
**disjoint** addition on the two sides rather than a disagreement about the same
lines, so each was resolved by keeping both, and no round-3 code was dropped.

| File | Conflict | Both intents kept |
|---|---|---|
| `src/vm.rs` `Statement::ForRange` | `main` had replaced the range loop with `expect_range_number` / `range_has_next` / `finite_number` and run the body as a plain statement list; the attempt had wrapped the same loop in `run_iteration` over the older `while i <= end`. | `main`'s numerics — a signed step, an overflow check on the stepped value, and a named error for a non-number bound — **and** the attempt's `run_iteration` call, which is what makes a `break`/`skip` in a range body leave the loop and releases the loop variable on every path. The result is `main`'s loop with one turn of the body run through `run_iteration`. `tests/for_range_test.rs` (31 tests, including the descending and fractional steps `main` added) and the five range tests of this phase are both green, which is the check that neither side was lost. |
| `src/bytecode/vm.rs` `pending_objects` | `main` had already made it a `Vec` for the `has`-default case; the attempt had made it a `Vec` for nested `object` bodies. | One field, and one doc comment naming **both** reasons: a `has` field's `default` is compiled as an expression and can open a declaration of its own, and an `object` body can nest inside another's. |
| `src/bytecode/vm.rs` `finish_object` | `main` had the `name: &str` parameter and the message `Object '<name>' lost its declaration before its body finished`; the attempt had the no-parameter form and the parent-chain rationale. | `main`'s signature and its better message, with the attempt's `resolve_object`/`object_is_declared` rationale and behaviour. The second call site — `discard_frames_above`, added by `main` — now passes the block's name, so the panic-free report reaches that path too. |
| `src/bytecode/vm.rs` `BytecodeVm::run` | `main` cleared `pending_objects` here and printed `self.output` under `self.echo`; the attempt reset `abrupt_exit` and mapped `drive`'s `Exit` away. | Both. The `pending_objects.clear()` stays, `abrupt_exit = false` is reset beside it, the result is still `drive(0).map(|_| ())` and the printing is still gated on `self.echo`. |
| `src/bytecode/vm.rs` struct fields / `new()` | `main` added `echo`; the attempt added `abrupt_exit`, `handling_failure` and `deferred_bindings`. | All four fields, initialised in `new()`. |
| `src/bytecode/vm.rs` `unwind_frame` / `discard_frames_above` | `main` truncated `self.loops` and had the module-body rollback; the attempt called `unwind_loops`, which is that truncation plus the bindings each abandoned loop gave back. | `unwind_loops` is called; the module-body rollback `main` added is untouched above it. |
| `src/bytecode/vm.rs` `DEF_OBJECT` | `main` set `pending_base`; the attempt set `new_frame.loop_owner`. | Both fields, and the attempt's comment about reading the owner before the advance. |

### One thing the merge turned up that no round had

Making the two statements work moved two **corpus goldens**, and phase-020 had left a
test that pinned the old no-op deliberately — `corpus::KNOWN_DEFECT_PROGRAMS` and
`edge_a_break_and_a_skip_are_pinned_as_the_defect_they_are`, whose own comment said
"The day either engine implements `break`, this fails and the two goldens are expected
to change". So this is the day, and the tree went red on three tests:

- `every_corpus_program_prints_what_its_expected_file_records` and
  `edge_regenerating_the_corpus_reproduces_the_checked_in_one_byte_for_byte` —
  `corpus/loop-forms-0012.expected` recorded `found` / `after`, the no-op's output;
  the loop now ends on its first turn and prints `after`. The golden was regenerated
  with the harness's own `RB_WRITE_CORPUS=1` path (which writes to
  `target/tmp/differential-refreshed`, never to `corpus/`) and copied over, so it is
  byte-for-byte what the generator produces. `loop-forms-0013` prints `2` / `after`
  either way and did not move.
- `edge_a_break_and_a_skip_are_pinned_as_the_defect_they_are` — **rewritten, not
  deleted**, and stronger than what it replaced. The table it iterated,
  `KNOWN_DEFECT_PROGRAMS`, is now `BREAK_AND_SKIP_PROGRAMS`, and instead of two names
  and a prose reason it carries **the lines the fix produces** (`["after"]` and
  `["2", "after"]`). The test asserts those exact lines on **both** engines, so a
  golden edited back to the no-op's output fails instead of passing quietly — which
  the old test, comparing each engine only with its own golden, would not have caught.
  It keeps the same shape: the programs exist, each holds a keyword, each completes,
  both engines agree, and the count floor of 2 is still asserted.

That is a third file outside the phase's own test set to touch, and the honest
reason is the same as the one the phase already gave for `src/bytecode/vm.rs`: the
fix changes what a program *means*, and a golden that recorded the old meaning had to
move with it.

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
| `src/vm.rs` | merge round | the range loop keeps `main`'s signed step, `expect_range_number` and `finite_number` overflow check, and runs one turn through `run_iteration` — `main`'s loop with the phase's control flow on top |
| `src/bytecode/vm.rs` | merge round | the nine conflict hunks resolved with both sides kept: `echo` beside `abrupt_exit`/`handling_failure`/`deferred_bindings`, `pending_objects.clear()` beside the `abrupt_exit` reset, `unwind_loops` for both of `main`'s `loops.truncate` sites, `finish_object(&name)` at the `discard_frames_above` call site `main` added, and `pending_base` beside `loop_owner` in `DEF_OBJECT` |
| `corpus/loop-forms-0012.expected` | merge round, −1 | regenerated: the loop now ends on its first turn, so the no-op's `found` line is gone and the file records `after` |
| `tests/common/corpus.rs` | merge round, +16 −26 | `KNOWN_DEFECT_PROGRAMS` becomes `BREAK_AND_SKIP_PROGRAMS`, carrying the lines the fix produces |
| `tests/differential_test.rs` | merge round, +33 −22 | `edge_a_break_and_a_skip_are_pinned_as_the_defect_they_are` becomes `edge_a_break_and_a_skip_leave_the_loop_on_both_engines`, pinning those lines on both engines |
| `tests/common/generator.rs` | merge round, +6 −6 | the comment beside the two `break`/`skip` corpus programs, which said the goldens record a no-op |
| `src/vm.rs` | round 4, +33 −8 | `Statement::Unless` runs its body through `run_block`; `declare_module` stops at a pending signal, publishes nothing when one is left, and unregisters the module — the same rollback its failure path already had |
| `src/bytecode/vm.rs` | round 4, +54 −13 | `module` records the loop the declaration is written inside; `unwind_frames_above` rolls a module body back; `abandon_handlers` finishes the frames above each handler's own frame before running that handler's `finally`, with `finish_top_frame` split out of `unwind_frames_above`'s loop; `discard_frames_above` drops the abandoned body's own declaration and only its own |
| `SPEC.md`, `docs/GRAMMAR.md`, `docs/BYTECODE.md` | round 4 | the loop a statement is written inside now says `module` (and an `unless` body) beside `catch`, `finally`, `test` and `object`; a module body is in the loop it is written in and inherits the function-body exemption from a function it is written in; the crossed-block ordering, and what finishing a module body leaves behind, are written down for a second VM to match |
| `tests/loop_control_test.rs` | round 4, +9 tests | `unless` bodies × `break`/`skip` × `for each`/`while`, and the module-body matrix: the two jumps, the abandoned module, the two refusals and the `finally` ordering — the last two compared on **both** VMs |
| `tests/object_model_test.rs` | round 4, +4 tests | a declaration opened by a `has` default, abandoned by a `catch` and by success, and the two ends of the registration rule FINDINGS §9 asks about |
| `tests/test_loop_control.rb` | round 4, +8 blocks | the same shapes in Redblue, which the bytecode corpus runs on **both** VMs |
| `tests/bytecode_vm_test.rs` | round 4, +14 corpus programs | two `unless`, seven `module` and five `object` programs, each compared across both VMs |
| `src/bytecode/vm.rs` | round 5, −3 +11 | `run_catch` stops popping the scope it pushed: the frame's own `locals_base` truncate already gives it back, and the extra removal took the enclosing function's scope with it (FINDINGS §13) |
| `tests/loop_control_test.rs` | round 5, +3 tests | a `skip` in a `test` body and in an `object` body — the two blocks with a frame of their own that had a `break` test and no `skip` one — and the pair compared across both VMs |
| `tests/bytecode_vm_test.rs` | round 5, +3 corpus programs, +1 test | the two `skip` programs, one `catch`-in-a-function program, and `edge_a_catch_body_gives_back_only_the_scope_it_pushed` |
| `tests/bytecode_vm_test.rs` | round 5, +34 | a `static CORPUS_WALK` mutex held by the three tests that walk the corpus: `examples/files.rb` writes files by relative path, so two walks at once read each other's residue and reported a disagreement about the filesystem as one about the two VMs (FINDINGS §15) |
| `tests/test_control_flow.rb` | round 5, +1 block | `edge: a catch leaves the function body it was written in alone`, which the bytecode corpus runs on both VMs |
| `tests/test_loop_control.rb` | round 5, +2 blocks | the two `skip` shapes, so the differential runs them on both VMs |
| `tests/loop_control_test.rs` | resumed round, +2 tests | the `skip` half of the definition of done's catchable refusal: `edge_a_refused_skip_is_catchable_with_try_catch_error` and `edge_a_refused_skip_inside_a_loop_leaves_the_loop_running`, each compared across both VMs |

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
- **Every block stops at the signal, `unless` bodies included** (round 4). An
  `unless` body is a block by the same argument as an `if` branch: the statements
  after a `break` in it are part of the turn the signal stopped. It was the one
  block left running its statements one at a time, and the bytecode VM — which
  compiles an `unless` body inline, so its jump goes over the rest — disagreed.
- **A module body is inside the loop it is written in** (round 4). It runs where
  the declaration is written, which is the whole of `SPEC.md`'s rule ("the loop is
  the one *around* the statement, wherever the statement was written") and the same
  answer an `object` body gets. The function-body exemption is for a body that runs
  when it is *called*, and a module body is not that; a module declared inside a
  function body inherits the exemption, because it is written inside one. A body a
  jump leaves publishes nothing and declares nothing — the rollback its failure
  path already had on both sides — so a later `module` of that name is a fresh
  declaration and a later `import` of it is a miss.
- **The crossed blocks are finished one at a time** (round 4). The tree-walking VM
  passes the signal out through one block at a time, so a `finally` written inside
  a block that is still on the stack runs while that block's scope is live, and a
  `finally` written around all of them runs once they are gone. The bytecode VM ran
  all of them first and finished the frames afterwards, which is observable wherever
  a crossed block holds a scope the `finally` can see — a module body, whose scope
  is still live while a `finally` around the declaration ran, so a `set` there went
  into the module and died with it.
- **A `finally` is owed however its region is left, and a `try` with no `catch` is
  not a handler** (round 6). The cleanup runs on the way out of a region that
  *failed* — which is what left the thing to clean up — and the failure is
  reported after it rather than instead of it, so it is the enclosing `try`'s to
  catch and the program's own to stop with. The tree-walking VM returned above
  `run_finally_body` on that path and skipped the cleanup; the bytecode VM ran the
  cleanup and then answered "handled" and resumed past the region's `NOP`, so a
  program that had failed ran to its end reporting success. `SPEC.md`'s "Finally"
  section and `docs/GRAMMAR.md` say the order.
- **A cap is a number of turns, on both engines, and a skipped turn is a turn**
  (round 6). The tree-walking VM charges at the top of every turn. The bytecode VM
  charged a `while`'s turn at the backward `JUMP` that ends the previous one — so a
  cap of N was N + 1 turns there, and its first turn was free — and `SKIP` steps
  over that jump, so a `while` that skipped every turn spent nothing at all and was
  stopped by the step budget rather than the cap.

## Tests added

`tests/loop_control_test.rs`, 61 tests (59 after round 6, plus the two in the
resumed round above). The "Fails without" column names what
each round-1 test fails on when its fix is taken back out — every one of them was
watched failing, and each of the four findings has at least one. The round-2,
round-3, round-4, round-5 and round-6 tests are in their own tables below.

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

### Merge-round test change

One test changed, and it changed in the direction of asserting more.

| Test | Edge class covered | Fails without |
|---|---|---|
| `edge_a_break_and_a_skip_leave_the_loop_on_both_engines` (was `edge_a_break_and_a_skip_are_pinned_as_the_defect_they_are`) | boundary: the two corpus programs that write these statements, on **both** engines, against the exact lines `["after"]` and `["2", "after"]` rather than against their own goldens | `corpus/loop-forms-0012.expected` written back to the no-op's `found` / `after` — watched failing, `left: ["found", "after"]` against `right: ["after"]` |

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

### Round 4 tests

`tests/loop_control_test.rs` grew by nine, `tests/object_model_test.rs` by four,
`tests/test_loop_control.rb` by eight `test` blocks and `tests/bytecode_vm_test.rs`
by fourteen corpus programs. The last column is what was watched failing with each
fix taken back out, counted over the Rust tests in the row's file.

| Test | Edge class covered | Fails without |
|---|---|---|
| `a_break_in_an_unless_body_does_not_run_the_statements_after_it` | round 4, finding 1: the statement after the `break`, and the one after the `unless` | `Statement::Unless` running its body as a plain statement list |
| `a_skip_in_an_unless_body_leaves_the_statements_after_it_unreached` | the `skip` companion, with the rest of the turn after the `unless` counted too | the same |
| `edge_both_vms_answer_the_same_for_break_and_skip_in_an_unless_body` | the finding itself: `break` and `skip` in a `for each` and in a `while`, plus an `unless` whose body never runs, through **both** VMs | the same |
| `a_break_in_a_module_body_inside_a_loop_leaves_that_loop` | round 4, finding 2: the loop stops, and neither the rest of the module body nor the rest of the turn runs | the module body consulting only the failure |
| `edge_a_skip_in_a_module_body_advances_the_loop_it_is_written_in` | the `skip` companion over three turns | the same |
| `edge_a_module_body_left_by_a_break_publishes_nothing_and_declares_nothing` | state: the abandoned module is not declared, so a later `import` is a miss | the same |
| `edge_a_break_in_a_module_body_outside_a_loop_is_refused` | **asserts a failure**: in no loop at all, and inside a function body called from a loop, with the caller's loop surviving — the module-body counterpart of the function-body exemption | `BytecodeVm::module` recording a loop owner, which would make the second case a jump |
| `edge_a_finally_around_a_module_declaration_runs_after_its_scope_is_gone` | ordering: the `finally` written around the declaration runs once the module's scope is gone, so its `set` is a name of the program | `abandon_handlers` running the crossed `finally` bodies before finishing the crossed frames |
| `edge_both_vms_answer_the_same_for_a_jump_in_a_module_body` | the finding itself: all seven module shapes, through **both** VMs | `BytecodeVm::module` recording a loop owner (refuses five of the seven) |
| `edge_object_a_declaration_opened_by_a_has_default_a_failure_abandons_only_its_own` | round 4, finding 3: a `has` default that opens a declaration a `catch` then abandons — the enclosing body still registers | `discard_frames_above`'s extra `pop` (the bytecode VM answers `'has a' is only valid inside an object declaration`) |
| `edge_object_a_declaration_opened_by_a_has_default_that_succeeds_registers_both` | the same shape with nothing failing | — (guards the shape the four above count on) |
| `edge_object_an_object_body_left_through_a_failure_has_registered_its_type` | FINDINGS §9: a failure *after* the declarations leaves the type declared on both VMs | — (the premise no longer holds; this pins what does) |
| `edge_object_an_object_body_whose_declaration_failed_registers_nothing` | the other end: a failure *in* a `has` default leaves the name unbound | — (the guard for the row above) |
| `tests/test_loop_control.rb` — 8 `test` blocks | the same matrix in Redblue, run by `rb test` **and** by the bytecode corpus on both VMs | each fix, with the blocks above |
| `tests/bytecode_vm_test.rs` — 14 corpus programs | two `unless`, seven `module` (three loop forms, the abandoned module, the `finally` ordering, the two refusals) and five `object` (the two `has`-default shapes and the three ends of the registration rule), each compared across both VMs | each fix; without the `unless` fix 3 of 404 programs disagree, without the module fix 6, without the interleave 2, without the `pop` fix 1 |

### Round 5 tests

`tests/loop_control_test.rs` grew by three, `tests/bytecode_vm_test.rs` by three
corpus programs and one named test, `tests/test_loop_control.rb` by two `test`
blocks and `tests/test_control_flow.rb` by one. All five of the round's programs
were watched failing with the fix taken back out.

| Test | Edge class covered | Fails without |
|---|---|---|
| `edge_a_catch_body_gives_back_only_the_scope_it_pushed` | round 5, finding 1: a parameter and a name declared before the `try`, both read after the `catch` — plus the other end, that the `catch`'s own binding still dies with its body | the extra `pop` in `run_catch` (the bytecode VM answers `Unknown variable 'x'`) |
| `try/a-catch-inside-a-function-body-leaves-the-calls-own-scope-alone` (corpus program) | the same shape as a whole program, compared across both VMs | the same — one of the 2 programs that disagree |
| `edge: a catch leaves the function body it was written in alone` (`tests/test_control_flow.rb`) | the same, in Redblue, which the bytecode corpus reads and so runs on both VMs | the same |
| `a_skip_in_a_test_body_advances_the_loop_the_test_is_written_in` | round 5, finding 2: a `test` body with a `skip` on its second turn — the block never ran and neither did the `.` after it, and the loop ran all three turns | the ownership fallback in `skip_loop` |
| `a_skip_in_an_object_body_advances_the_loop_the_declaration_is_written_in` | the `object` body beside it, with the declaration made under a guard so the loop can go on to a second and third turn | the same |
| `edge_both_vms_advance_the_loop_for_a_skip_in_a_test_or_an_object_body` | the finding itself: both shapes through **both** VMs, each printing a `leaked` line that must not appear | the same (5 of 407 programs disagree, one of them `tests/test_loop_control.rb`) |
| `tests/test_loop_control.rb` — 2 `test` blocks | `loop control: a skip in a test body advances the loop the test is written in` and `…in an object body…`, so the differential runs both shapes on both VMs | the same |
| `flow/a-skip-in-a-test-body-inside-a-loop-advances-the-turn`, `flow/a-skip-in-an-object-body-inside-a-loop-advances-the-turn` (corpus programs) | one per shape, each compared across both VMs | the same |

### Round 6 tests

`tests/bytecode_vm_test.rs` grew by four corpus programs and six named tests,
`tests/loop_control_test.rs` by two, and `tests/test_control_flow.rb` by three
`test` blocks. The three corpus programs and the `.rb` blocks are read by the
both-VMs differential, so every one of them ran on **both** engines.

| Test | Edge class covered | Fails without |
|---|---|---|
| `try/a-failure-with-nothing-around-it-stops-the-program-after-the-finally` (corpus program) | finding 1 at its sharpest: nothing is written around the `try`, so the golden records the `say` the `finally` made *and* the `RuntimeError` that stopped the program — the tree ran the cleanup and failed, the bytecode VM ran the cleanup and carried on | the `if !has_catch` branch in `src/vm.rs` |
| `try/a-finally-runs-on-the-way-out-of-a-failure-with-no-catch` (corpus program) | the same rule with an enclosing `catch`, so the result is a completion: `fc`, not `f` | both — this is the corpus program that shows the tree's answer changing |
| `try/a-try-with-no-catch-hands-the-failure-to-the-try-around-it` (corpus program) | finding 2 with no `finally` in the way: the outer `catch` must run, and the `set reached to "yes"` after the inner `try` must not | the `continue` in `handle_failure`, and on its own the `end_try_after` fix — the bytecode VM prints `yes`, having resumed inside the protected region |
| `try/a-failing-finally-inside-a-try-with-no-catch-is-the-one-that-propagates` (corpus program) | which of two failures leaves: the two say different things, so the recorded message says it is the cleanup's | — (it pins the behaviour the two VMs agree on today; the tree side of it is asserted below) |
| `edge_a_failing_finally_is_the_failure_a_try_with_no_catch_reports` | the same, asserted rather than recorded: `Division by zero` is reported, `non-numbers` is not, and nothing after the `try` ran | a change that propagated the protected region's own failure instead |
| `edge_a_break_in_a_catch_around_a_catch_less_try_leaves_the_loop_and_cleans_up` | the two halves of `prepare_exit` where the crossed `try` has only a `finally`: the loop is left **and** the cleanup ran on both turns | the ownership work, if the `try` it crosses stops being a handler |
| `edge_a_catch_less_try_does_not_stop_the_loop_it_is_written_in` | a `try` with neither `catch` nor `finally`, written inside a loop: the loop keeps turning and the program stops with the failure rather than swallowing it | the `continue` in `handle_failure` |
| `edge_a_cap_counts_turns_on_both_vms_and_a_skipped_turn_is_one` | findings 3 and the off-by-one beside it: every loop form, plain, `skip`ped and `break`ing, under caps of 1, 3 and 4, asserting the number of turns each cap allows on **both** VMs | the `turn_over_to` charge, and `iterations: usize::from(!site.iterator)` |
| `edge_a_while_that_skips_every_turn_is_stopped_by_the_iteration_cap` | the review's own symptom, stated as an assertion: the per-loop cap stops it, not the step budget | the `turn_over_to` charge |
| `edge_a_break_stops_the_loop_at_the_same_turn_on_both_vms` | the other end: a `break` on the first turn is one turn whatever the cap, and the first turn of a `while` is the one whose entry does not exist until the exit needs it | the `iterations` seed |
| `edge_a_skip_in_a_while_is_charged_as_one_iteration` (`tests/loop_control_test.rs`) | the same on the tree-walking VM, where the rule was already right — the gap the review named, since `edge_a_skip_is_charged_as_one_iteration` beside it only covered `repeat` | nothing on this VM; it is the statement of the rule the bytecode half was brought back to |
| `edge_a_break_on_the_first_turn_of_a_while_is_one_turn` (`tests/loop_control_test.rs`) | the `break` half of the same accounting on the tree-walking VM | — |
| `tests/test_control_flow.rb` — 3 `test` blocks | `edge: a finally is owed when there is no catch to handle the failure`, `edge: a try with no catch hands the failure to the one around it` and `edge: a failing finally still leaves the outer try to catch the failure`, so the differential runs all three shapes on both VMs | the `if !has_catch` branch, and the `continue` |

## Resumed round — re-verification, and the one gap the definition of done named

This round was not a review round. The tree arrived with round 6's fixes already
merged and all three runnable gates already green, so the work was to re-verify
rather than re-derive: every count in the gates table below was re-run here, and
every box in the phase's definition of done was checked against the tree rather
than against the previous report.

Re-running found **no regression and no open red**: the three runnable gates pass
unchanged, `rb test` runs 332 blocks with 0 failures, all 6 `examples/*.rb` and
both `modules/*.rb` exit 0, and the reproduction through the binary answers
correctly. Five new loop-control shapes — a `skip` in a function called from a
loop, a loop variable released after a `break`, a `skip` on the final turn of a
`while` beside a `break`, a `break` and a `skip` in a nested `for each`, and a
`break` out of a `try` that has only a `finally` — were written and compared on
both engines, and all five agree.

What the re-verification did find is one place where the phase's own definition
of done asked for more than the Rust tests carried. The wording is "edge tests
assert a FAILURE for `break` **and for `skip`** outside any loop — a clean caught
RuntimeError naming the statement …; **both** must also be catchable with
`try`/`catch error` inside a loop". The Rust side had the `skip` half of the
*failure* (`skip_outside_a_loop_is_a_clean_runtime_error`, which asserts the
message names `skip`) but not the catchable half, so `skip` reached the loop
lookup through `Statement::Skip` with no Rust test on what happens when the
lookup comes back empty. `break` had both halves. `tests/test_loop_control.rb`
carried `loop_control_edge_a_refused_skip_inside_a_loop_leaves_the_loop_running`,
so the behaviour was pinned on both engines through the differential — but the
definition of done names the Rust tests, and a reviewer reading
`tests/loop_control_test.rs` would find the gap. Two tests close it, and both
compare the tree-walking VM with the bytecode VM, because a `skip` and a `break`
reach that lookup by different paths and only the disagreement between the two
engines would show it.

| Test | Edge class covered | Fails without |
|---|---|---|
| `edge_a_refused_skip_is_catchable_with_try_catch_error` | **asserts a failure**: the `skip` companion of `edge_a_refused_break_is_catchable_with_try_catch_error` — a `skip` in no loop is caught by `try ... catch error`, the program carries on, and the bytecode VM answers the same. Both halves were checked against wrong expectations and watched fail | — (nothing in `src/` changed; this pins behaviour both engines already had) |
| `edge_a_refused_skip_inside_a_loop_leaves_the_loop_running` | **asserts a failure**: the `skip` companion of `edge_a_break_in_a_function_body_is_refused_and_the_caller_survives` — a `skip` in a called function has no loop of its own even when a loop is one frame away, so it is refused and caught on all 3 turns rather than obeyed. Both halves were checked against wrong expectations and watched fail | the same |

`tests/loop_control_test.rs` is 61 tests, up from 59. `cargo test --all-targets`
is 819, up from 817. `rb test` is unchanged at 332 because both additions are
Rust; the Redblue half of this shape already existed.

Nothing under `src/` changed this round, because nothing under `src/` was wrong.
`must_touch: ["src/"]` is satisfied by the merged work this round inherited
(`src/vm.rs` and `src/bytecode/vm.rs`, both in the "What changed" table above),
and the phase diff as a whole touches both.

## Gates

Re-run after round 6's fixes and this round's two tests, on the tree as it now
stands.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test --all-targets` | **819 passed, 0 failed, 0 ignored** (31 binaries) |
| `./rbops/verify.sh phase-025` | **not run — `rbops/` is not in this checkout** |

This round's two tests are the difference between 817 and 819, both in
`tests/loop_control_test.rs` (59 → 61). `rb test` is unchanged at 332.

`rbops/` is absent from the working directory, so the fourth gate exits
`No such file or directory` and the manual substitute below stands.

| Check | Result |
|---|---|
| `rb test` (every `.rb` under `tests/`) | 332 run, 332 passed, 0 failed |
| `./target/debug/rb run examples/*.rb` | 6/6 exit 0 |
| `./target/debug/rb run modules/*.rb` | 2/2 exit 0 (`MathUtils.rb` passes as well) |
| `tests/loop_bounds_test.rs` | 22 passed, 0 failed |
| `tests/loop_control_test.rs` | 61 passed, 0 failed |
| `tests/object_model_test.rs` | 39 passed, 0 failed |
| `tests/differential_test.rs` | 81 passed, 0 failed |
| `tests/bytecode_vm_test.rs` (the both-VMs differential) | 58 passed, 0 failed, over a corpus of 411 programs |
| `tests/for_range_test.rs` | 31 passed, 0 failed |
| `cargo test --doc` | 1 passed, 0 failed |
| the reproduction, through the binary | `break` prints `1`, `skip` prints `1` then `3`, `repeat` stops at 3, a `break` in no loop exits 1 with `'break' is only valid inside a loop` |

### Round 5's gates, kept for the count

Re-run after round 5's fixes, on the tree as it then stood.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test --all-targets` | **809 passed, 0 failed, 0 ignored** (31 binaries) |
| `./rbops/verify.sh phase-025` | **not run — `rbops/` is not in this checkout** |

Round 5's four tests are the difference between 805 and 809 — three in
`tests/loop_control_test.rs` and one in `tests/bytecode_vm_test.rs` — and its three
`test` blocks are the difference between 326 and 329. Nothing else moved: the other
four new artifacts are corpus programs inside the existing both-VMs test, which
counts programs rather than tests (404 → 407).

`rbops/` is still absent from the working directory, so the fourth gate exits
`No such file or directory` and the manual substitute below stands.

| Check | Result |
|---|---|
| `rb test` (every `.rb` under `tests/`) | 329 run, 329 passed, 0 failed |
| `./target/debug/rb run examples/*.rb` | 6/6 exit 0 |
| `./target/debug/rb run modules/*.rb` | 2/2 exit 0 |
| `tests/loop_bounds_test.rs` | 22 passed, 0 failed |
| `tests/loop_control_test.rs` | 57 passed, 0 failed |
| `tests/object_model_test.rs` | 39 passed, 0 failed |
| `tests/differential_test.rs` | 81 passed, 0 failed |
| `tests/bytecode_vm_test.rs` (the both-VMs differential) | 52 passed, 0 failed, over a corpus of 407 programs |
| `tests/for_range_test.rs` | 31 passed, 0 failed |
| `cargo test --doc` | 1 passed, 0 failed |
| the reproduction, through the binary | `break` prints `1`, `skip` prints `1` then `3`, `repeat` stops at 3, a `break` in no loop exits 1 with `'break' is only valid inside a loop` |

One caveat about the corpus count, recorded because it is a gate fact and not a
detail: `examples/files.rb` writes files by relative path, so the two tests that
walk the whole corpus were reading each other's residue when they ran at the same
time, and one of them reported `examples/files.rb` as a disagreement between the
VMs (FINDINGS §15). It is serialised now, and the three runs that failed before
the lock are green after it. A red `a_corpus_of_programs_runs_identically_on_both_vms`
naming `examples/files.rb` is that, not a difference between the two engines.

The suite count is much larger than the 552 the round-3 round recorded, and none of
that is this phase: `main` has landed work since, and 240 of the difference are
tests the merged tree brought with it. What the merge round's own changes account
for is three — the two corpus tests and the rewritten pin, which are the same three
the merge turned red before they were fixed. Round 4's own thirteen tests are the
difference between 792 and 805, and its eight Redblue blocks are the difference
between 318 and 326.

The counts before round 5 are kept above for the rounds that recorded them;
everything below is round 5's own re-run. `rbops/` is absent from the working
directory (`ls` shows `AGENTS.md`, `SPEC.md`, `corpus`, `docs`, `examples`,
`modules`, `phases`, `src`, `tests`, `target`, and no `rbops/`), so
`./rbops/verify.sh phase-025` exits `No such file or directory`. The pipeline lives
outside this checkout and the phase prompt forbids inspecting it. What was run
instead, by hand, from the same phase prompt:

| Check | Result |
|---|---|
| `rb test` (every `.rb` under `tests/`) | 326 run, 326 passed, 0 failed |
| `./target/debug/rb run examples/*.rb` | 6/6 exit 0 |
| `./target/debug/rb run modules/*.rb` | 2/2 exit 0 (`MathUtils.rb` passes as well) |
| `tests/loop_bounds_test.rs` | 22 passed, 0 failed |
| `tests/loop_control_test.rs` | 54 passed, 0 failed |
| `tests/object_model_test.rs` | 39 passed, 0 failed |
| `tests/differential_test.rs` | 81 passed, 0 failed, over a corpus of 361 programs |
| `tests/bytecode_vm_test.rs` (the both-VMs differential) | 51 passed, 0 failed, over a corpus of 404 programs |
| `tests/for_range_test.rs` | 31 passed, 0 failed |
| `cargo test --doc` | 1 passed, 0 failed |
| the reproduction, through the binary | `break` prints `1`, `skip` prints `1` then `3`, `repeat` stops at 3, a `break` in no loop exits 1 with `'break' is only valid inside a loop` |

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
seven new `tests/loop_control_test.rs` tests respectively. This round's own change
was checked the same way: restoring `main`'s plain statement list in the range loop
takes down `a_break_in_a_range_loop_ends_it`,
`a_skip_in_a_range_loop_advances_to_the_next_value`,
`edge_break_and_skip_in_a_stepped_range_loop` and
`edge_both_vms_answer_the_same_for_break_and_skip_in_a_range_loop`. Round 4's four
fixes were checked the same way, one at a time: putting the `for stmt in body` loop
back into `Statement::Unless` takes down 3 of the 54 `tests/loop_control_test.rs`
tests, 2 of the 8 new `test` blocks and **3 of 404** programs in the both-VMs corpus
(the two new `unless` programs and `tests/test_loop_control.rb`); dropping
`module_frame.loop_owner` takes down 1 Rust test, 6 of the new `module/…` programs
and `tests/test_loop_control.rb` again; dropping the one line that finishes the
crossed frames before a crossed `finally` runs takes down 1 Rust test and **2 of
404**; and putting the extra `pop` back into `discard_frames_above` turns
`object/a-declaration-opened-by-a-has-default-a-failure-abandons` red with the
bytecode VM reporting `'has a' is only valid inside an object declaration`. Round 5's
were checked the same way: putting the `pop` back into `run_catch` — the whole fix is
that one line going away — takes down
`edge_a_catch_body_gives_back_only_the_scope_it_pushed` and makes **2 of 407**
programs disagree, one of them `tests/test_control_flow.rb`; and taking the
ownership fallback out of `skip_loop` alone turns **5 of 407** red
(`flow/a-skip-in-a-catch-body-inside-a-loop-advances-that-loop`,
`flow/a-skip-in-a-finally-inside-a-loop-advances-the-turn`,
`flow/a-skip-in-a-test-body-inside-a-loop-advances-the-turn`,
`module/a-skip-in-a-module-body-inside-a-loop-advances-that-loop` and
`tests/test_loop_control.rb`) plus the new Rust test. The
VERIFY gates are unaffected by that dance — the fixes are in, not out.

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
  `edge_a_skip_is_charged_as_one_iteration` is exactly-cap / cap+1, and round 6
  takes the same pair to `while` (`edge_a_skip_in_a_while_is_charged_as_one_iteration`
  in `tests/loop_control_test.rs`) and to both engines at once
  (`edge_a_cap_counts_turns_on_both_vms_and_a_skipped_turn_is_one`, under caps of
  1, 3 and 4 for every loop form — plain, `skip`ped and `break`ing); round 3 adds
  `a_break_in_a_range_loop_ends_it` (the loop stops at the value that broke, and
  that value's own turn is gone) and `edge_break_and_skip_in_a_stepped_range_loop`
  (the `by` form, whose stride the two statements do not consult).
  The merge round keeps that last pair pinned across `main`'s signed-step range
  loop: restoring its plain statement list takes down all four range tests, and
  `tests/for_range_test.rs` (31 tests) is green either side of that.
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
  `edge_objects_an_object_body_nested_in_a_loop_can_break_out_of_it`. Round 4 adds
  a module declaration nested in a `try` nested in a loop
  (`module/a-finally-around-a-module-declaration-runs-after-its-scope-is-gone`,
  on both VMs, where the crossed block is the module and the enclosing one is the
  `try`), and a declaration opened by a `has` default — which is a declaration
  nested inside a call nested inside a declaration's own assembly
  (`object/a-declaration-opened-by-a-has-default-a-failure-abandons`). Round 5 adds
  the `skip` end of two of those blocks — a `test` body and an `object` body, which
  had `break` tests and no `skip` ones — in
  `edge_both_vms_advance_the_loop_for_a_skip_in_a_test_or_an_object_body` and the two
  corpus programs beside it, and the round-5 BLOCKER's nesting of its own: a `catch`
  body inside a function body inside a `try` (`try/a-catch-inside-a-function-body-leaves-the-calls-own-scope-alone`,
  on both VMs).
- **duplicate_missing_keys** — N/A. `break` and `skip` read no record field, bind
  no name and write no record, so there is no key to duplicate or miss. Record
  key handling is `tests/record_order_test.rs`, untouched by this change.
- **malformed_input** — covered by the structural form: there is no operand that
  can be malformed, and the one invalid *placement* — a `break` in a block that
  is not a loop — is asserted in four shapes for `break` and, since the resumed
  round, in four for `skip` as well:
  `break_outside_a_loop_is_a_clean_runtime_error` and
  `skip_outside_a_loop_is_a_clean_runtime_error` (the message names the
  statement), `edge_a_refused_break_is_catchable_with_try_catch_error` and
  `edge_a_refused_skip_is_catchable_with_try_catch_error` (the refusal is
  catchable and the program carries on, on both engines),
  `edge_break_in_an_object_body_is_refused` (an `object` body runs once, at
  declaration, and is not a loop),
  `edge_a_break_in_a_function_body_is_refused_and_the_caller_survives` and
  `edge_a_refused_skip_inside_a_loop_leaves_the_loop_running` (a called body has
  no loop of its own even when one frame away, so both statements are refused and
  the caller's loop runs out, on both engines), and, from
  round 2, `edge_a_break_in_a_block_in_a_function_body_is_still_refused` (a block
  of the function's own, which is the case the frame model made ambiguous). All
  are `Error::Runtime`, never a panic and never a silent no-op. Round 3 adds
  the operand question the corrected grammar settles:
  `edge_neither_break_nor_skip_takes_an_operand` asserts that `skip 1` and
  `break 1` are a jump followed by a separate statement, and
  `edge_a_skip_leaves_the_statements_after_it_unreached` asserts the block rule for
  a body that is a single statement. Round 4 adds the module body's two refusals —
  `edge_a_break_in_a_module_body_outside_a_loop_is_refused` covers a body written
  where there is no loop and one written inside a function body called from a loop,
  both on both VMs. Unterminated
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
- The merge round changed no answer either VM gives. Both sides of every hunk were
  kept and the resolution is `main`'s behaviour with this phase's control flow on
  top of it: a range loop still steps backwards on a negative `by`, still refuses a
  non-number bound by name and still fails rather than looping on an overflowing
  step, and a bytecode VM that prints still prints only when `echo` is on. The one
  observable difference this round introduced is the refreshed
  `corpus/loop-forms-0012.expected`, which is the fix's own output and which the
  rewritten pin now asserts rather than merely comparing against.
- Round 4 changed two answers, and both are disagreements rather than behaviour
  anybody could have been relying on. A `break` or a `skip` in an `unless` body no
  longer runs the statements after it, on either VM — the bytecode VM already
  skipped them, so this is the tree-walking VM catching up. And a `break` or a
  `skip` in a module body written inside a loop is a jump on both VMs, where the
  bytecode VM used to stop with `'break' is only valid inside a loop` (exit 1) and
  the tree-walking VM leaked the rest of the body. A module body written where
  there is no loop, and one inside a function body called from a loop, are refused
  on both. `SPEC.md`, `docs/GRAMMAR.md` and `docs/BYTECODE.md` now say so.
- Round 5 changed one answer, on the bytecode VM and in the direction of the tree:
  a `try`/`catch` inside a function body no longer takes that function's own scope
  with it, so a program that handles a failure and then uses its own parameter or
  its own local runs to the end rather than stopping with
  `RuntimeError: Unknown variable 'x'`. Every program that ran before got a
  *failure* there, so this is a program the bytecode VM could not run becoming one
  it can, and a caller reading an exit code sees 0 where it saw 1. The tree-walking
  VM is untouched: it truncated the catch's scope rather than popping it, which is
  what this fix makes the bytecode VM do. Nothing else changed — the four new tests
  pin behaviour both VMs already agreed on, and the corpus lock changes no program's
  answer, only which test finds it.
- Round 6 changed three answers, and each is a case where one engine was telling
  the program something untrue. A `try` with no `catch` **stops the program** on
  the bytecode VM, where it used to run on past the region and exit 0 — so a
  caller reading an exit code sees 1 where it saw 0, on a program that had failed.
  A `try` with only a `finally` now runs that `finally` on the tree-walking VM's
  failing path, where it used to skip it: a program whose cleanup writes outside
  the VM now does that write, and one whose cleanup fails reports the cleanup's
  failure rather than the original. And a `while` under a lowered iteration cap
  stops one turn earlier on the bytecode VM, so a cap of N is N turns on both
  engines — and a `while` that skips every turn is stopped by the cap at all,
  rather than by the program's step budget. Nothing changes for a program that was
  inside every one of those limits.

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
- ~~An `object` body the program leaves through a **failure** is still neither
  registered nor bound on the bytecode VM~~ — **not reproducible; the real defect
  beside it fixed in round 4.** Recorded as open in FINDINGS §9 and escalated to a
  MAJOR by the round-4 review. The compiler splits an `object` body into a
  declaration block and the enclosing block (`src/bytecode/codegen.rs:382`), so a
  failure in the statements *after* the declarations happens with the type already
  registered on both VMs — the round-3 program prints `5` on both, and did before
  this phase. What *was* wrong in `discard_frames_above` is fixed: it truncated
  `pending_objects` to the frame's recorded height and then popped once more,
  which took the enclosing body's declaration with it whenever that height was
  above zero — which is what a `has` default that opens a declaration produces.
  FINDINGS §9 records the rule both VMs already follow, with the program that
  reproduces the old failure (`'has a' is only valid inside an object declaration`
  from `rb vm`).
- `for each i from a to b [by s]` is documented in `SPEC.md` and
  `docs/GRAMMAR.md` and **cannot be parsed**, so `Statement::ForRange` is
  unreachable from source and the corpus's range programs compare two parse
  errors. Found in round 3; FINDINGS §10 records it. The phase's own change to
  that loop form is covered through the AST instead — `range_loop` in
  `tests/loop_control_test.rs` and the five tests that run `break`, `skip`, the
  stepped form and the empty range through it on both VMs. Adding the parser
  production is a language change and belongs to the phase that adds it, the same
  call made above about `repeat … until`.
- `corpus/loop-forms-0013` records the same lines before and after the fix
  (`2`, `after`) — a `skip` on the value 2 prints the same line the no-op printed,
  because the skipped turns in that program print nothing. So its golden did not
  move, and only `-0012` needed regenerating. Its half of the rewritten pin still
  asserts the exact lines on both engines, which is why a program whose golden
  happens not to move is not thereby uncovered.
- ~~A `catch` takes the enclosing function's scope with it on the bytecode VM~~ —
  **fixed in round 5.** Recorded by the round-5 review as a BLOCKER and closed
  here: `run_catch` popped a scope its frame's own truncate had already given back,
  and the extra removal was the function's. A program with a handled failure and a
  local or parameter read after the `try` stopped with
  `RuntimeError: Unknown variable 'x'`; it now prints what the tree-walking VM
  prints. FINDINGS §13 has the program and the three things that hid the defect.
- A `finally` is **not** owed when the `catch` that handled the failure itself
  fails — both VMs stop at the catch's failure and never run the `finally`. Found
  by the round-5 review and **left as it is**, because `SPEC.md:464`'s "a
  `finally` is still owed on the way out" is about an *abrupt exit*, which is not a
  failure and which both VMs already handle, and because nothing in `SPEC.md` or
  `docs/GRAMMAR.md` says what a `finally` is owed after a failing `catch`. The
  second half of that question — a `try` whose body fails with **no** `catch` —
  was a BLOCKER in the round-6 review and is **fixed**: the `finally` is owed
  however the region is left, it runs before the failure is reported, and the
  failure is then the enclosing `try`'s to catch or the program's own to stop
  with. What is left is the failing-`catch` half, and the two questions that go
  with it: which failure propagates when both the `catch` and the `finally` fail,
  and what an abrupt exit out of that `finally` names. FINDINGS §14 records the
  program and today's behaviour on both VMs, so a later phase that decides it can
  see exactly which programs move. The same call was made about `repeat … until`
  and about `for each i from a to b`.
- `./rbops/verify.sh` could not be run in this checkout; see the Gates table.
