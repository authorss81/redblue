# phase-025 — findings

Recorded during and after the phase. §1–§3 and §7 were recorded in round 0 and
are unchanged in substance; §7 is closed in round 2. §4 and §5 are defects the
round-1 fixes uncovered, both fixed in round 1. §6 was found in round 1 and is
fixed in round 2, which is what this phase's `break` work turns on. §8 was found
in round 2, and the round-3 review made it a BLOCKER; it is fixed in round 3.
§9 and §10 were found in round 3 and are open. Nothing here is worked around to
make a gate pass.

## 1. `rbops/verify.sh` is not in this checkout

`./rbops/verify.sh phase-025` → `No such file or directory`. `ls -a` of the
project root shows `.github`, `.gitignore`, `AGENTS.md`, `phases/`, `src/`,
`tests/`, `examples/`, `modules/`, `docs/`, `tooling/` and no `rbops/`. The
pipeline lives outside the working directory and the phase prompt forbids
inspecting it, so the fourth gate could not be run here. The other three were,
plus the checks the phase's definition of done names by hand:

- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — clean
- `cargo test --all-targets` — 516 passed, 0 failed, 0 ignored
- `rb test` — 252 run, 252 passed
- `examples/*.rb`, `modules/*.rb` — 8/8 exit 0
- `tests/loop_bounds_test.rs` — 22 passed
- `tests/bytecode_vm_test.rs` — 33 passed, including both differential tests

Recorded so a human can run the real gate; nothing was fabricated in its place.
(The counts are round 0's; rounds 2 and 3 are in REPORT.md's Gates table.)

## 2. Corpus entry names that stated the defect

`tests/bytecode_vm_test.rs` carried two corpus programs whose *keys* asserted
the no-op:

- `flow/a-skip-inside-a-bounded-loop-is-not-a-jump-yet`
- `flow/skip-the-last-element` (this one is fine — it does not mention jumping)
- `shape/break-and-skip-inside-a-loop` (fine)

Only the first is a claim that is now false, and round 2 renamed it to
`flow/a-skip-inside-a-bounded-loop-leaves-its-own-iteration-out` rather than
leaving the file to a later phase: the program text is unchanged and still
compared across both VMs, so the rename costs no coverage. Its sibling
`flow/a-break-inside-a-bounded-while-is-not-a-jump-yet` said the same about
`break` and was renamed with it.

## 3. `break` / `skip` and the bytecode VM were two defects, not one

The bytecode VM's `BREAK`/`SKIP` handlers were a separate documented no-op
(`src/bytecode/vm.rs`, `// BREAK: no effect`, with a comment saying the two VMs
disagreeing is worse than the feature being unfinished). Fixing only
`src/vm.rs` turns `a_corpus_of_programs_runs_identically_on_both_vms` red on four
programs. This phase fixed both, which is why `src/bytecode/vm.rs` is in the
diff. If a future phase revisits the bytecode VM's control flow it should know
the loop-exit machinery it needed was already there and is now used.

## 4. ~~`break_loop` panicked when the `finally` it ran failed~~ — fixed

Found while writing the round-1 tests for the bytecode VM, and reachable before
round 1: `try { break } finally { <a failure that a handler catches> } end`.

`break_loop` took the loop's entry, then ran the `finally` of every `try` inside
the body being left, then removed the entry. Handling the failure unwinds the
loops the handler's `try` was written inside
(`self.loops.truncate(handler.loop_base)`), so the entry was gone before the
removal, and `Vec::remove` on an empty vector panicked:

```
thread 'main' panicked at src/bytecode/vm.rs:1462:37:
removal index (is 0) should be < len (is 0)
```

Fixed by `loop_index`, which re-finds the entry after the `finally` bodies have
run, and by leaving the loop only when it is still there: a handler that handled
the failure has already put the frame where it belongs. Pinned by the corpus
program `flow/a-failing-finally-on-the-way-out-of-a-break-does-not-end-the-next-loop`,
which panics the differential test without the fix.

## 5. ~~The bytecode VM left an abandoned loop's variable bound~~ — fixed

Found by the same round-1 tests, and pre-existing: a failure that abandoned a
loop dropped its entry with `self.loops.truncate(...)`, which throws away the
binding the loop's variable displaced without putting it back. The tree-walking
VM pops each turn's scope however the turn ends, so the two read different values
for a name the program bound outside a loop:

```
$ printf 'set i to "outer"\ntry\n    repeat 3 times\n        for each i in [1, 2]\n            set bad to 1 + "one"\n        end\n    end\ncatch error\nend\nsay i\n' > target/tmp/shadow.rb
$ cargo run --bin rb -- run target/tmp/shadow.rb          # tree:      outer
$ cargo run --bin rb -- compile target/tmp/shadow.rb -o target/tmp/shadow.rbc
$ cargo run --bin rb -- vm target/tmp/shadow.rbc          # bytecode:  1
```

(Both print `outer` after the fix below; the two lines are what they printed
before it.)

Fixed by `unwind_loops`, used everywhere loops are dropped rather than left
(`handle_failure`, `discard_frames_above`, `unwind_frame`), which restores each
one's shadowed binding innermost first — what `leave_loop` already did on a
`break`. Pinned by `tests/test_loop_control.rb`'s
`loop_control_edge_a_failed_turn_does_not_leak_the_scope_of_its_variable`, which
the differential test runs on both VMs.

## 6. ~~The two VMs disagree about `break` in a block with its own frame~~ — fixed

Found in round 1, fixed in round 2. A `break` or a `skip` written in a `test`,
`catch`, `finally` or `object` body that is lexically inside a loop, but has no
loop of its own:

```
$ printf 'set n to 0\nrepeat 3 times\n    set n to n + 1\n    try\n        set x to 1 + "one"\n    catch error\n        if n is 2 then\n            break\n        end\n        say "caught"\n    end\nend\nsay n\n' > target/tmp/catch_break.rb
$ cargo run --bin rb -- run target/tmp/catch_break.rb      # tree:      caught / 2, exit 0
$ cargo run --bin rb -- compile target/tmp/catch_break.rb -o target/tmp/catch_break.rbc
$ cargo run --bin rb -- vm target/tmp/catch_break.rbc      # bytecode:  Error: 'break' is only valid inside a loop
```

Both now print `caught` and `2`.

The tree-walking VM answered from `Vm::loop_depth`, which counts the loops the
statement is lexically inside, so a block inside a loop is inside that loop. The
bytecode VM answered from `BytecodeVm::loop_at`, which looked only at the loop
sites of the *frame the instruction is running in* — and a `test`, `catch`,
`finally` or `object` body is a separate block with its own frame, so the loop in
the enclosing block was invisible to it. `docs/BYTECODE.md` records the structure
that makes this so ("A block is entered with an empty stack", and each of these
bodies compiles to a child block), so the bytecode VM was self-consistent and the
*tree* answer is the language's.

The fix is `LoopOwner` (`src/bytecode/vm.rs`): a frame records the loop it was
written inside, and it is recorded from the instruction that entered the frame —
the `TEST` or `DEF_OBJECT` for a block, the `TRY` for a `catch` or a `finally` —
so ownership is lexical rather than positional. `loop_at` answers from that when
the frame has no loop of its own, `break_loop`/`skip_loop` leave the loop it names
and finish the frames between the instruction and it (`unwind_frames_above`), and
the frames above the loop are dropped the way any other exit drops them: their
loops unwound, their handlers and scopes released, and an `object` body
registering the type it declared.

A call frame records nothing. That is the exemption the tree-walking VM draws
too — `call_user_function` zeroes `loop_depth` — and it is what keeps
`edge_a_break_in_a_function_body_is_refused_and_the_caller_survives` true: a
`break` in a function body, or in a block of one, is still a `break` in no loop.

Four things the fix uncovered, each pinned by a corpus program the differential
test compares on both VMs:

- **The `finally` of an interrupted `try` is still owed.** The tree-walking VM
  passes the signal out through the `try` statement whose protected code it
  stopped, wherever that statement was written, so a `try` in the `catch` body
  the `break` was written in runs its `finally` too. `abandon_handlers` counts a
  handler the exit passes through as one installed above the owning frame as well
  as one written inside the loop body.
- **The frames are finished before the loop entry is created.** A frame's
  `unwind_loops` drops every loop above the base that frame recorded, and a block
  frame was pushed before the loop it is inside drew its entry — so an entry made
  before the frames above were finished went with them and the loop kept running.
  On a `while`'s first turn, where the entry does not exist until the jump back,
  that turned the `break` into a no-op: `while n is not 9 / set n to n + 1 / test
  "t" / break / end / end` printed 2 instead of 1.
- **The loop's variable is still bound while the `finally` runs.** A turn's scope
  is popped when the turn ends, and the turn a `break` stopped does not end until
  the `finally` the signal passed through has run — so
  `put_back_binding` holds the binding back while a failure is being handled, and
  `handle_failure` puts it back once the `catch` and the `finally` are both done.
- **The signal is read, not taken.** An exit that crosses frames stops every
  nested driver on the way out, so the first one to stop must not consume the
  record: it is cleared by the driver that carries on, and `drive` reports
  `Exit::Left` to each caller in turn. A `catch` body that broke while a `finally`
  inside it also broke reported itself as having run to the end, and the frame
  that owns the region was told to resume inside it.

What this costs today: nothing in the corpus, which is why
`a_corpus_of_programs_runs_identically_on_both_vms` is green — and that is
recorded here as a constraint on writing tests, not as a reassurance.
`tests/test_loop_control.rb` is read by the differential test, and it now carries
the shapes round 1 could not: a `break` in a `catch` body, in a `finally` body, in
a `test` body and in an `object` body, a `skip` in a `catch` body, a block inside a
block, the function-body refusal beside them, and the loop variable still bound
inside the `finally`. Thirteen corpus programs in `tests/bytecode_vm_test.rs` and
nine `test` blocks were added for the same shapes; taking the ownership lookup
back out turns fourteen of the corpus programs red, `tests/test_loop_control.rb`
among them.

## 7. ~~`SPEC.md` documents `break` but never says what happens with no loop~~ — fixed

`SPEC.md:417-433` and `docs/GRAMMAR.md:245-246` gave worked examples and no
error case. Round 2 states the refusal this phase added
(`'break' is only valid inside a loop`), that it is a runtime error `try ...
catch error` catches, and the two placements refused for the same reason — a
function body, and an `object` body written outside any loop — in both files.
The section also states the two rules the fix had to pin down to make the two VMs
agree: a block written inside a loop is inside that loop, and the loop's variable
is still bound while a `finally` runs.

## 8. ~~An `object` declared inside an `object` panics the bytecode VM~~ — fixed

Found in round 2 while writing probes for §6, pre-existing, and left open there
because it was not part of that finding and not this phase's defect. The round-3
review made it a BLOCKER, and it was right to: it is a panic on a program the
tree-walking VM runs, so `rb vm` aborts the process (exit 101) on ordinary user
input rather than reporting a failure.

```
$ printf 'object A\n    object B\n        has c\n    end\nend\nsay "done"\n' > target/tmp/nestobj2.rb
$ cargo run --bin rb -- run target/tmp/nestobj2.rb          # before: done, exit 0
$ cargo run --bin rb -- compile target/tmp/nestobj2.rb -o target/tmp/nestobj2.rbc
$ cargo run --bin rb -- vm target/tmp/nestobj2.rbc
thread 'main' panicked at src/bytecode/vm.rs:1048:14:      # before: an object declaration being assembled
an object declaration being assembled

$ cargo run --bin rb -- run target/tmp/nestobj2.rb          # after:  done, exit 0
$ cargo run --bin rb -- vm target/tmp/nestobj2.rbc          # after:  done, exit 0
```

`BytecodeVm::pending_object` was one `Option`, and `DEF_OBJECT` overwrote it, so
the inner declaration took the outer's place; when the outer body then finished,
`finish_object` found nothing left to assemble and the `expect` panicked.

Fixed by holding the declarations being assembled in a stack
(`BytecodeVm::pending_objects`) rather than in one slot. The innermost is the one
a `has`, a `to can` and `finish_object` mean, which is the same one the
tree-walking VM's `declare_object` collects into — the body it is written in is
the body being declared. Four things fell out of it, each pinned by a test:

- **`has` and `to can` take the innermost.** A field declared in a nested body
  belongs to the nested type, not the enclosing one.
- **`finish_object` reports rather than panics.** An `object` body that finishes
  with nothing to register is now a `RuntimeError`; the `expect` is gone, so no
  path out of the bytecode VM aborts the process.
- **A name an open declaration has taken is refused.** `def_object` asks
  `object_is_declared`, which looks at the pending stack as well as `objects`, so
  `object A` written inside `object A`'s body is refused with
  `Object 'A' is already declared` — the message the tree-walking VM gives,
  which has registered the outer type by then. With `objects` alone the inner
  declaration would have been accepted and then silently overwritten it.
- **A parent chain may walk a pending declaration.** The tree-walking VM registers
  a type *before* it runs the statements after its declarations, so
  `object B extends A` nested inside `object A` finds `A`; this VM runs the nested
  declaration first, so `A` is a declaration still being assembled, and
  `resolve_object` looks there after `objects`. Without it the nested declaration
  was refused with `which is not declared` — a disagreement about a program the
  other VM runs, one rule away from the panic.

`discard_frames_above` pops the declaration of an `object` body it drops, which is
what keeps the stack the same length as the number of open `object` bodies: an
abandoned body stops being assembled, so the frame that finishes the body
enclosing it takes its own rather than this one's. `BytecodeVm::run` clears the
stack, so a run that failed part-way through does not leave a half-declared body
behind for the next one.

What it costs today: nothing. `git show HEAD:src/bytecode/vm.rs` panics on the
program above, so no program that ran before round 3 gets a different answer from
it — it gets an answer instead of a panic. Pinned by
`tests/bytecode_vm_test.rs::objects_an_object_declared_inside_an_object_body_declares_both_types`
and the twelve `object/…` corpus programs beside it, all compared across both VMs,
by seven `test` blocks in `tests/test_object_model.rb` (which the differential
reads) and by nine tests in `tests/object_model_test.rs`.

## 9. An `object` body the program leaves through a failure is not registered

Open, found in round 3 while writing the corpus programs for §8, and pre-existing
— `git show HEAD:src/bytecode/vm.rs` disagrees on the same program. It has nothing
to do with nesting: the same disagreement is there for a body with no nested
declaration in it.

```
$ printf 'set reached to "no"\ntry\n    object Outer\n        has o default 1\n        set bad to 1 + "one"\n    end\ncatch error\nend\nset Outer.o to 5\nsay Outer.o\n' > target/tmp/abandoned.rb
$ cargo run --bin rb -- run target/tmp/abandoned.rb          # tree: 5, exit 0
$ cargo run --bin rb -- compile target/tmp/abandoned.rb -o target/tmp/abandoned.rbc
$ cargo run --bin rb -- vm target/tmp/abandoned.rbc          # bytecode: Unknown variable 'Outer', exit 1
```

`handle_failure` drops the frames between the failing one and the frame that
installed the handler, and `discard_frames_above` releases their loops, handlers,
scopes and call depth — but neither registers the type of an `object` body among
them. The tree-walking VM's `declare_object` has already collected the `has` and
`to can` declarations and registered the type by the time it runs the statements
after them, so a failure in one of those leaves the type declared and its name
bound.

Round 2 added the same shape for the *abrupt* exit — `unwind_frames_above`
registers the type of an `object` body an exit passes through — so the two paths
that leave a body are not even consistent with each other today. Fixing it means
deciding what an abandoned body's declarations are worth, and the honest answer is
"the ones that were collected", which the bytecode VM does not record: it would
have to know whether the failure happened in a `has` default (the tree-walking VM
registers nothing, because it had not collected the rest) or after the last
declaration (it registers the type), and it does not track where a body's
declarations end. `unwind_frames_above` has the same half-answer, and round 2
recorded it as "registers the type however it is left" rather than as a rule
derived from the tree-walking VM.

Recorded rather than fixed here, and no corpus program depends on it: the twelve
added in round 3 read the nested body's field from *inside* the outer body rather
than the outer type's name afterwards, and the one test that would have needed the
outer name was written the other way instead. A phase that owns the object model's
registration order should take it.

## 10. `for each i from a to b [by s]` is documented and cannot be parsed

Open, found in round 3 while writing the break/skip tests the review asked for.
`SPEC.md` §"For Loop (Range)" and `docs/GRAMMAR.md`'s `for_statement` rule both
give the range form, `docs/GRAMMAR.md` §"For Loop" gives it as the *worked example*
for `for`, and `src/parser.rs`'s `parse_for` accepts `for each x in <expression>`
and nothing else:

```
$ printf 'for each i from 1 to 10\n    say i\nend\n' > target/tmp/range.rb
$ cargo run --bin rb -- run target/tmp/range.rb
Error: ParserError: Expected In but got From
```

`Statement::ForRange` is therefore unreachable from source. It is not dead code —
the analyzer, the bytecode compiler, the formatter and the linter all carry it —
and `tests/numeric_edge_test.rs` says so in a comment where it builds one by hand.
Two consequences worth naming:

- **The corpus's range programs compare two parse errors.**
  `tests/bytecode_vm_test.rs` holds entries such as `singleton/range-of-one`
  written as `for each i from 1 to 1`; both VMs refuse them identically, so the
  differential is green and nothing ran. That is a weaker guarantee than the entry
  reads as giving.
- **The round-3 review's finding 3 asks for a test that cannot be written.** The
  requested `for each i from 1 to 10 / if i is 5 then break` is a parse error. The
  coverage it asks for is delivered instead through the AST —
  `range_loop` in `tests/loop_control_test.rs` builds the `ForRange` the way
  `tests/numeric_edge_test.rs` does, and five tests run `break`, `skip`, the
  stepped form and the empty range through it on **both** VMs, which is what pins
  the `src/vm.rs` change that routes this loop form through `run_iteration`.

Adding the parser production is a language change rather than a review fix, and it
would make a documented form real for the first time, so it belongs to the phase
that adds it — the same call this phase made about `repeat … until` (REPORT.md,
"Known gaps"). What is recorded here is that the two documents and the parser
disagree, so a later reader does not have to find it again.
