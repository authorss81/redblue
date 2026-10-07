# phase-025 — findings

Recorded during and after the phase. §1–§3 and §7 were recorded in round 0 and
are unchanged in substance; §7 is closed in round 2. §4 and §5 are defects the
round-1 fixes uncovered, both fixed in round 1. §6 was found in round 1 and is
fixed in round 2, which is what this phase's `break` work turns on. §8 was found
in round 2, and the round-3 review made it a BLOCKER; it is fixed in round 3.
§9 was found in round 3 and closed in round 4 — its registration premise turned out
not to reproduce, and the defect beside it is fixed. §10 was found in round 3 and
is open. §12 was found by the round-4 review and is fixed here. §13 was found by
the round-5 review, is the BLOCKER it found, and is fixed in round 5; §14 is the
MAJOR beside it, half of which the round-6 review's BLOCKER (§16) settled — the
other half is open. §15 is a race in the corpus harness that made the
both-VMs gate red for reasons that had nothing to do with either VM, and is fixed
in round 5. §16 is the round-6 review: three BLOCKERs, all fixed here, one of
which uncovered two more. Nothing here is worked around to make a gate pass.

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

## 9. ~~An `object` body the program leaves through a failure is not registered~~ — not reproducible, and the real defect beside it fixed in round 4

Recorded as open in round 3. The round-4 review escalated it to a MAJOR on the
strength of this text; the review was right that `discard_frames_above` had
something wrong in it, and wrong that the wrong thing was registration.

**The program above does not disagree any more, and did not before this phase
either.** `git show HEAD:src/bytecode/vm.rs` disagrees with *itself* here rather
than with the tree-walking VM:

```
$ printf 'try\n    object Outer\n        has o default 1\n        set bad to 1 + "one"\n    end\ncatch error\n    say "caught"\nend\nset Outer.o to 5\nsay Outer.o\n' > target/tmp/abandoned.rb
$ cargo run --bin rb -- run target/tmp/abandoned.rb          # tree:     caught / 5, exit 0
$ cargo run --bin rb -- compile target/tmp/abandoned.rb -o target/tmp/abandoned.rbc
$ cargo run --bin rb -- vm target/tmp/abandoned.rbc          # bytecode: caught / 5, exit 0
```

The reason is in the compiler, not in either VM: an `object` body compiles as two
halves, and the split is `declare_object`'s (`src/bytecode/codegen.rs:382`). The
`has` and `to can` go into the block that assembles the type, which is a frame of
its own; **everything else compiles into the enclosing block, after the `STORE`
that binds the name.** So a failure in a statement after the declarations happens
long after the frame that would have discarded it was finished — with the type
registered by both VMs. What is left to decide is the other direction, and both
VMs already agree on it:

| where the failure lands | tree-walking VM | bytecode VM |
|---|---|---|
| a `has` default, while the declarations are still being collected | registers nothing — `declare_object` returns the failure before `objects.insert` | the frame is discarded and `pending_objects` is truncated |
| a statement after the last declaration | the type is registered and its name bound | the same, in the enclosing block |
| the `try ... catch` encloses the whole declaration | the catch finds nothing registered | the same |

`unwind_frames_above` registering the type of a body an *abrupt* exit passes
through is not a different rule from this; it is the second row, reached from the
other direction. So there is nothing here for a phase to decide, and the honest
answer to the round-3 question — "which of the collected declarations are worth
registering?" — is *all of them, or none of them, and which one depends only on
whether the failure landed in a `has` default* — is a rule both VMs already
follow, one because the compiler put the declarations in their own block and one
because it did not.

**What was wrong beside it, and is fixed.** `discard_frames_above` truncated
`pending_objects` to the height the frame recorded and then popped once more:

```rust
self.pending_objects.truncate(frame.pending_base);
if frame.object_body {
    self.pending_objects.pop();     // one too many, whenever pending_base > 0
}
```

`truncate` already removes the abandoned body's own entry — it is at the index the
frame recorded — so the `pop` took the **enclosing** body's entry with it. That
needs a body whose `pending_base` is above zero, which a nested `object` cannot
produce (§8: a nested declaration compiles into the enclosing block, so its frame
is never inside another's). A `has` default can: it is compiled as an expression,
so a call in one runs a body that declares a type while the enclosing declaration
is still being assembled.

```
$ printf 'to maker()\n    try\n        object Inner\n            has x default 1 + "one"\n        end\n        set reached to "no failure"\n    catch error\n        set reached to "caught"\n    end\n    give back reached\nend\n\nobject A\n    has a default maker()\nend\n\nsay A.a\n' > target/tmp/opened.rb
$ cargo run --bin rb -- run target/tmp/opened.rb                       # before: caught, exit 0
$ cargo run --bin rb -- compile target/tmp/opened.rb -o target/tmp/opened.rbc
$ cargo run --bin rb -- vm target/tmp/opened.rbc                       # before:
Error: RuntimeError: 'has a' is only valid inside an object declaration
$ cargo run --bin rb -- vm target/tmp/opened.rbc                       # after:  caught, exit 0
```

`A` is registered, its `has a` finds nothing left to assemble, and the program
stops with a message about a field declaration that is perfectly well placed.
Pinned by the corpus program
`object/a-declaration-opened-by-a-has-default-a-failure-abandons`, which the
differential test compares on both VMs and which fails without the fix, and named
by four tests in `tests/object_model_test.rs`; the three shapes the table above
lists are pinned by `object/a-declaration-opened-by-a-has-default-that-succeeds-registers-both`,
`object/an-object-body-left-through-a-failure-has-registered-its-type` and
`object/an-object-body-whose-declaration-failed-registers-nothing`.


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

## 11. The corpus had a test pinning the no-op on purpose

Found in the merge round, when making the statements work turned
`tests/differential_test.rs` red on three tests that no earlier round had seen.

Phase-020 built the corpus harness with an explicit escape hatch for exactly this
phase: `corpus::KNOWN_DEFECT_PROGRAMS` (then at `tests/common/corpus.rs:306`, now
`corpus::BREAK_AND_SKIP_PROGRAMS` at `tests/common/corpus.rs:312`) named
`loop-forms-0012` and `-0013`, and
`edge_a_break_and_a_skip_are_pinned_as_the_defect_they_are` (then at
`tests/differential_test.rs:768`, now
`edge_a_break_and_a_skip_leave_the_loop_on_both_engines` at
`tests/differential_test.rs:774`) asserted on **both** engines that the keyword is in
the program, that the loop runs to its end anyway, and that each engine prints what
its own golden records. Its own comment said what would happen next: "The day either
engine implements `break`, this fails and the two goldens are expected to change —
which is the whole reason the no-op is pinned rather than ignored."

So this is a defect of the *fix* being invisible to the golden files, and it was
designed to be loud rather than quiet. Two programs, two files:

- `corpus/loop-forms-0012.expected` recorded `found` / `after`, the no-op's output.
  The loop now ends on its first turn, so the `if` never reaches the value 2 and the
  file records `after`.
- `corpus/loop-forms-0013.expected` records `2` / `after` both before and after,
  because in that program the skipped turns print nothing. It needed no change.

The resolution is recorded in REPORT.md, "Merge resolution". The short version: the
golden was regenerated with the harness's own `RB_WRITE_CORPUS=1` path into
`target/tmp/` and copied over, and the pin was rewritten rather than removed, into
`edge_a_break_and_a_skip_leave_the_loop_on_both_engines` over
`corpus::BREAK_AND_SKIP_PROGRAMS` — a table that carries the lines the fix produced
rather than a prose reason, so each engine is checked against those lines and not
merely against its own golden. Checked by putting the no-op's lines back in
`corpus/loop-forms-0012.expected`: `left: ["found", "after"]`, `right: ["after"]`,
FAILED.

Recorded here because a later phase that changes what a program means will hit the
same thing: this corpus records behaviour, and behaviour that changes has to move
the goldens with it. The escape hatch is the right design, but it is a promise to
refresh them, not a permanent waiver.

## 12. Two blocks ran past a signal the bytecode VM had already acted on

Found in round 4, by the review, and both pre-existing in the sense that the two
VMs have disagreed since round 1 made the statements work. Neither is new
behaviour; each is a block the round-1 fix left out.

**An `unless` body** ran as a plain `for stmt in body { execute_statement }`
(`src/vm.rs`, `Statement::Unless`), so a `break` or a `skip` in one set the signal
and then let the rest of the body run anyway. The bytecode VM compiles an `unless`
body into the enclosing block, so its `BREAK`/`SKIP` is a jump over the rest:

```
$ printf 'for each i in [1, 2, 3]\n    unless i is 2 then\n        say i\n        break\n        say "unreachable"\n    end\n    say "after"\nend\nsay "done"\n' > target/tmp/unless_break.rb
$ cargo run --bin rb -- run target/tmp/unless_break.rb       # tree: 1 / unreachable / done
$ cargo run --bin rb -- compile target/tmp/unless_break.rb -o target/tmp/unless_break.rbc
$ cargo run --bin rb -- vm target/tmp/unless_break.rbc       # bytecode: 1 / done
```

Fixed by running the body through `run_block`, like the `if` branches above it.
The `if` branches were converted in round 1 and this one was missed, which is the
whole of the defect: `Statement::Unless` is the only block in `src/vm.rs` that
still ran its statements one at a time.

**A `module` body** ran through a statement loop that consulted the failure and
not the signal, so the same leak applied, *and* the bytecode VM's module frame
recorded no `loop_owner`, so it refused the `break` outright where the tree-walking
VM honoured it:

```
$ printf 'set count to 0\nfor each i in [1, 2, 3]\n    set count to count + 1\n    module M\n        set held to i\n        break\n        say "unreachable"\n    end\n    say "unreachable"\nend\nsay count\n' > target/tmp/module_break.rb
$ cargo run --bin rb -- run target/tmp/module_break.rb       # tree:     1, exit 0
$ cargo run --bin rb -- vm target/tmp/module_break.rbc       # bytecode: Error: 'break' is only valid inside a loop, exit 1
```

A module body runs where the declaration is written, so it is inside the loop
around that declaration exactly as an `object` body is — it is *not* exempt the
way a function body is, which runs when it is called rather than where it is
written. Fixed by routing the body through the same block rule and giving the
module frame the loop the `MODULE` instruction sits in, so the two VMs answer
alike. A body left that way publishes nothing and declares nothing, which is the
rollback a failed body already had on both sides.

The module fix uncovered a third thing, which is the same rule seen from the other
side: a `finally` a jump passes through has to run *after* the crossed frames are
finished. The tree-walking VM gets that for free — the signal passes out through
one block at a time — and the bytecode VM ran every crossed `finally` first and
finished the frames afterwards, so a `set` in a `finally` written around a module
declaration landed in the module's scope instead of the program's and was lost
when the scope went (`tf` against `t`). `abandon_handlers` now finishes the frames
above each handler's own frame before running that handler's `finally`, which is
the tree-walking VM's order; the one-frame half of `unwind_frames_above` is
`finish_top_frame`.

Both are pinned by corpus programs the differential test compares across both VMs
(`flow/a-break-in-an-unless-body-ends-the-block-it-is-written-in`,
`flow/a-skip-in-an-unless-body-leaves-the-rest-of-it-unreached` and seven
`module/…` programs), by eight `test` blocks in `tests/test_loop_control.rb`, and
by nine tests in `tests/loop_control_test.rs`. Taking each fix back out turns 3 of
the 9 Rust tests, 1 of them and 1 of them red respectively, and **3**, **6** and
**2** of the 404 programs in the both-VMs corpus — `tests/test_loop_control.rb`
among the last two, since the bytecode corpus runs it too.

## 13. A `catch` took the enclosing function's scope with it — fixed

Found by the round-5 review, and pre-existing in the shape the phase itself wrote:
`run_catch` is part of the `try` machinery §6 extended, and the extra removal
predates the ownership work.

```
$ printf 'to f(x)\n    try\n        set y to 1 + "one"\n    catch error\n        say "caught"\n    end\n    say x\n    give back x + 1\nend\n\nsay f(7)\n' > target/tmp/catchscope.rb
$ cargo run --bin rb -- run target/tmp/catchscope.rb            # tree:      caught / 7 / 8, exit 0
$ cargo run --bin rb -- compile target/tmp/catchscope.rb -o target/tmp/catchscope.rbc
$ cargo run --bin rb -- vm target/tmp/catchscope.rbc            # before:   RuntimeError: Unknown variable 'x'
$ cargo run --bin rb -- vm target/tmp/catchscope.rbc            # after:    caught / 7 / 8, exit 0
```

`run_catch` pushed a scope for the body, pointed the catch frame's `locals_base`
at it — which is the arrangement a module body uses, and the right one — and then
popped it again once the frame had been driven:

```rust
self.frames.push(catch_frame);
let outcome = self.drive(self.frames.len() - 1);
self.locals.pop();          // one too many: the frame already truncated to its base
outcome
```

Every path that pops a frame truncates `locals` to that frame's base —
`unwind_frame`, `finish_top_frame`, `discard_frames_above` — so the body finished
had already taken the scope the `pop` reached for, and the second removal took the
**enclosing** frame's scope: a function's parameter scope, or a closure's. Every
name the body read or assigned after the `try` was then unbound, and the program
stopped with `Unknown variable 'x'` where the tree-walking VM printed the value.

Three things hid it:

- **A program's own names live in globals**, which no scope removal reaches, so
  every top-level program behaved.
- **A loop's variable is not in `locals`.** It lives in a `LoopOwner` entry, so a
  `catch` inside a loop over a function's own list still read `i` correctly — the
  program in the reviewer's report has that shape, and it runs.
- **The `catch` body still had to fail to reach the line.** Every `catch` takes the
  scope, so this was not a narrow path: it was every handled failure inside a
  function body, which is to say most error handling.

Fixed by deleting the `pop`. The truncate *is* the rule, and it already covers
the three ways the body can end — run to the end, failed and taken by an outer
handler, or left early by a `break` — because all three go through a path that
truncates to the frame's base.

Pinned by three programs: the corpus entry
`try/a-catch-inside-a-function-body-leaves-the-calls-own-scope-alone` and the
`test` block `edge: a catch leaves the function body it was written in` in
`tests/test_control_flow.rb`, both read by the both-VMs corpus, and
`edge_a_catch_body_gives_back_only_the_scope_it_pushed` in
`tests/bytecode_vm_test.rs`, which also asserts the end the fix rests on — that the
`catch`'s *own* binding still dies with its body. Putting the `pop` back turns the
named test red and makes **2 of 407** programs disagree, one of them
`tests/test_control_flow.rb`.

## 14. A `finally` is not owed when the `catch` that handles the failure fails — open

Found by the round-5 review, recorded rather than changed. **The round-6 review
settled the second half of it** — a `try` whose body fails with no `catch` to
handle it runs the `finally` and then propagates the failure, which is §16 — so
only the failing-`catch` half is still open, and it is written below with that
change marked.

```
$ printf 'try\n    set a to 1 + "one"\ncatch error\n    say "caught"\n    set b to 1 + "two"\nfinally\n    say "finally ran"\nend\nsay "after"\n' > target/tmp/fin.rb
$ cargo run --bin rb -- run target/tmp/fin.rb          # tree:     Cannot add non-numbers at 5:5, exit 1
$ cargo run --bin rb -- vm target/tmp/fin.rbc          # bytecode: Cannot add non-numbers, exit 1
```

Both VMs stop there, with the `finally`'s line never printed. `Statement::Try`
propagates a failing `catch` with `?` before it reaches `run_finally_body`, and
`handle_failure` chains the same way on the bytecode VM
(`run_catch(&handler).and_then(|caught| run_finally(&handler))`), so the two agree
and the differential is green. That is still true after §16: the fix for the
no-`catch` half put the `finally` below that `?`, not above it.

**The reviewer's premise for this being a contradiction does not hold.**
`SPEC.md:464` — "A `finally` is still owed on the way out — an abrupt exit from a
protected region is not a failure" — is about an abrupt exit. A `break` or a
`skip` passing through a `try` body is not a failure, and both VMs run that
`finally`; §6 and §12 are the corpus programs that pin it. A `catch` that itself
fails *is* a failure, and nothing in `SPEC.md` or `docs/GRAMMAR.md` says what a
`finally` is owed after one.

So the decision is a language one, and what is left of it is two parts the
specification does not currently answer:

- **whether** the `finally` runs at all after a `catch` that itself failed.
  (Settled for the other case by §16: a `try` with no `catch` to handle the
  failure runs the `finally` before propagating it, and `SPEC.md`'s "Finally"
  section now says so.)
- **which** failure propagates when both the `catch` and the `finally` fail — the
  first, the second, or the second with the first attached. Java answers the last,
  which needs somewhere to attach it; this language has no such value. (The `catch`
  half is decided by §16 in the only way the existing code shape allows: a
  `finally` that fails while a failure is propagating *replaces* the failure being
  propagated, on both VMs. That falls out of `run_finally_body(...)?` sitting
  below `return Err(failure)`, and the corpus program
  `try/a-failing-finally-inside-a-try-with-no-catch-is-the-one-that-propagates`
  is what pins it.)
- what an abrupt exit *out of* a `finally` running because the `catch` failed means
  when no loop was entered, or when one was and the `catch` named it.

Changing that is the same call §10 records: adding the range production was a
language change and belonged to the phase that adds it, so this belongs to the
phase that decides it. What is pinned in the meantime is the behaviour both VMs
actually have, so a later phase that changes it will see exactly which programs
move.

## 15. The both-VMs gate was reading another test's files — fixed

Found in round 5 while re-running the gates, and pre-existing in
`tests/bytecode_vm_test.rs`: three tests walk the whole corpus, and one corpus
program — `examples/files.rb` — writes `output.txt`, `output_copy.txt` and
`renamed.txt` **by relative path**, in the test process's own working directory.
`cargo test` runs the tests in one binary on as many threads as there are cores, so
two corpus walks interleaved over one set of files, and the second run's answer
depended on the first's residue:

```
$ cargo test --test bytecode_vm_test -- a_corpus_of_programs_runs_identically_on_both_vms \
                                          edge_the_two_vms_report_the_same_failure_for_every_corpus_program
1 of 407 programs disagree between the tree-walking VM and the bytecode VM:
examples/files.rb
  tree:     Outcome { output: [ ..., "Updated contents:", "Hello from Redblue! - appended text", ... ], result: Ok("nothing") }
  bytecode: Outcome { output: [ ..., "IoError: Failed to rename 'output_copy.txt' to 'renamed.txt' ..." ], ... }
```

Three runs of that command failed; `--test-threads=1` was green every time. It was
a disagreement about the filesystem, reported as a disagreement about the two VMs,
and it had nothing to do with this phase — but it is in the gate the phase is
judged by, so it is fixed here.

Fixed by holding a `static CORPUS_WALK: Mutex<()>` for as long as a test is
walking the corpus, and serialising the three that do. Nothing is excluded and no
program stops being compared: dropping `examples/files.rb` from the comparison
would have made the gate green by taking a program out of it, which is the trade
this phase has refused twice already (REPORT.md, "Merge resolution"). The three
runs above are green with the lock in place.

## 16. The `finally` owed on the way out of a failure, and the turn a `skip` costs — fixed

Found by the round-6 review, which reported three BLOCKERs. All three are fixed
here, and fixing them uncovered two more, both of which are in the same two
functions and would have been new disagreements on their own.

### A `try` with no `catch` skipped its `finally` on the failing path

```
$ printf 'try\n    set bad to 1 + "one"\nfinally\n    say "cleanup"\nend\nsay "after"\n' > target/tmp/only_finally.rb
$ cargo run --bin rb -- run target/tmp/only_finally.rb          # tree:      Cannot add non-numbers, exit 1, no "cleanup"
$ cargo run --bin rb -- compile target/tmp/only_finally.rb -o target/tmp/only_finally.rbc
$ cargo run --bin rb -- vm target/tmp/only_finally.rbc          # bytecode: cleanup / after, exit 0
```

`SPEC.md`'s "Finally" section documents this form — the `try` writes no `catch`,
and that is the point of the section: the cleanup is wanted on the path where the
region failed. `Statement::Try` returned `Err(failure)` above the
`run_finally_body` call whenever the parser had found no `catch` to handle the
failure, so the tree-walking VM ran the cleanup on every path but that one, while
the bytecode VM ran it there and on the success path alike. The two engines
disagreed on which promise to keep, and the tree-walking VM's answer was the one
that skipped a promised cleanup.

Fixed by running the `finally` before propagating, which is also what makes the
failure the *enclosing* `try`'s to handle — the tree-walking VM's `?` reaches
one, and there is a behaviour for a `try` written around it to agree with. The
order matters and is now stated in `SPEC.md` and `docs/GRAMMAR.md`: the `finally`
runs on the way out and the failure is reported *after* it, not instead of it.

### The bytecode VM's `handle_failure` called a `try` with no `catch` a handler

```
$ printf 'try\n    set bad to 1 + "one"\nend\nsay "after"\n' > target/tmp/bare_try.rb
$ cargo run --bin rb -- run target/tmp/bare_try.rb              # tree:      Cannot add non-numbers, exit 1
$ cargo run --bin rb -- vm target/tmp/bare_try.rbc              # bytecode:  after, exit 0
```

Worse than a missing cleanup: `handle_failure` popped the handler, ran whatever
bodies it had, and then answered `true` — "handled" — which resumed execution
past the marked `NOP` that closes the region. A program that had failed ran to its
end and reported success. A `try` with no `catch` is not a handler, and the
`NOP` past which execution would carry on is never reached on a path that failed.

Fixed by looping the search: a handler with no `catch` runs the `finally` it is
owed and the search carries on from what is left, which is what the tree-walking
VM's `?` does. Only a search that runs out of handlers answers `false`, and that
is the failure leaving the program. Each turn pops a handler, so the search
cannot run for ever.

Two things the loop needed, neither of which was visible until the failure
reached an enclosing `try` for the first time:

- **A body that failed and was taken outside spends the original failure.** A
  `finally` that fails inside a `catch`-less `try` is handled by the enclosing
  `try` — through the nested drive the body is run by, which consults the handlers
  above it. The failure this region was handling is then not the one left over,
  and re-offering it would stop the program with a failure an enclosing `catch`
  had already dealt with. Both VMs drop the original in that case (the failing
  `finally` replaces it), so the bytecode VM now does too: it answers `true`
  where the handler stack has lost one of the handlers that enclosed this `try`.
- **`end_try_after` could not find the end of a region it had already entered.**
  It scanned forward from the failing instruction and returned the first marked
  `NOP` at nesting depth zero. For a handler reached by skipping an inner
  `try ... end`, the inner region's `TRY` is *behind* the instruction that
  failed, so the depth count never saw it and the inner `NOP` matched: the
  enclosing `try` ran its `catch` and then resumed *inside* the protected code it
  was supposed to have left.

  ```
  $ printf 'set reached to "no"\ntry\n    try\n        set bad to 1 + "one"\n    end\n    set reached to "yes"\ncatch error\n    set reached to "caught"\nend\nsay reached\n' > target/tmp/skip_handler.rb
  $ cargo run --bin rb -- run target/tmp/skip_handler.rb           # tree:     caught
  $ cargo run --bin rb -- vm target/tmp/skip_handler.rbc           # before:   yes
  ```

  Fixed by scanning from the `TRY` that installed the handler — which is where
  the region begins, so the instructions before the failure are part of it — and
  passing `handler.start` down. For a handler that *is* the innermost enclosing
  one it finds the same `NOP` it did before.

Pinned by three corpus programs the differential test compares on both VMs
(`try/a-failure-with-nothing-around-it-stops-the-program-after-the-finally`,
`try/a-finally-runs-on-the-way-out-of-a-failure-with-no-catch`,
`try/a-try-with-no-catch-hands-the-failure-to-the-try-around-it` — the last of
which fails on the `end_try_after` half on its own), by three `test` blocks in
`tests/test_control_flow.rb`, and by two tests in `tests/bytecode_vm_test.rs`.
Taking each half of the fix back out turns **4 of 413** programs red.

### A `skip` in a `while` spent no iterations at all

```
$ printf 'set n to 0\nwhile n < 10000000\n    set n to n + 1\n    skip\nend\nsay "done"\n' > target/tmp/skip_while.rb
$ REDBLUE_MAX_ITERATIONS=5 cargo run --bin rb -- run target/tmp/skip_while.rb    # tree:      Maximum of 5 iterations reached in a 'while' loop
$ REDBLUE_MAX_ITERATIONS=5 cargo run --bin rb -- vm target/tmp/skip_while.rbc    # bytecode:  Step budget of 10000000 reached before the program finished
```

A sequence loop charges each turn at the `STORE` on its `top` that draws the next
value, and `skip` jumps to that `top`, so a skipped turn of a `for each` or a
`repeat` was already charged. A `while` has no such instruction: it charges at
the backward `JUMP` that ends a turn, and `SKIP` steps over that jump to reach
the condition. So a `while` that skipped every turn spent nothing, and only the
program-wide step budget stopped it. `turn_over_to` charges the turn it starts
when the loop is a `while`, and clears the exit flag when that charge fails — the
same thing `prepare_exit` does for a `finally` that could not run.

### A cap of N was N turns on one engine and N + 1 on the other

Not in the review, and found by the test above rather than by reading: the
bytecode VM charges a `while`'s turn at the *end* of the previous one, so it
allowed one turn more than the tree-walking VM's up-front charge. Its first turn
was free, because a `while` draws its entry when it turns over rather than before
it.

```
$ printf 'try\n    set n to 0\n    while n < 10000000\n        set n to n + 1\n    end\ncatch error\n    say "caught"\n    say n\nend\n' > target/tmp/one_past.rb
$ REDBLUE_MAX_ITERATIONS=3 cargo run --bin rb -- run target/tmp/one_past.rb    # tree:      caught / 3
$ REDBLUE_MAX_ITERATIONS=3 cargo run --bin rb -- vm target/tmp/one_past.rbc    # bytecode:  caught / 4
```

The tree-walking VM charges at the top of every turn, so a cap is a number of
turns; that is the language's answer and the published one, and it is what the
`skip` fix above had to match — a `skip` that charged nothing and a `while` that
charged one turn late are the same defect seen from two ends. Fixed by starting a
`while`'s entry at one turn: an entry is drawn when the loop turns over or an exit
leaves it, which is after the turn it is leaving has run, so that turn is what the
counter starts on. A sequence loop is untouched — `GET_ITER` and `GET_RANGE` draw
its entry *before* the turn.

Pinned by `edge_a_cap_counts_turns_on_both_vms_and_a_skipped_turn_is_one` in
`tests/bytecode_vm_test.rs`, which runs every loop form — plain, `skip`ped and
`break`ing — under caps of one, three and four and asserts the turn count each
one allows, and by `edge_a_skip_in_a_while_is_charged_as_one_iteration` in
`tests/loop_control_test.rs`. Both were written to fail on the pre-fix code: the
second reports the step budget where the iteration cap belongs, which is what the
review quoted.

The cap is invisible to the corpus, which runs at the published million and cannot
lower it from inside a program, so these tests added
`tree_walk_capped`/`bytecode_capped`/`assert_agrees_capped` — the same
comparison the corpus makes, at a cap a test can reach.
