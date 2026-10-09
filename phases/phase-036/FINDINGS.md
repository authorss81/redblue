# FINDINGS — phase-036

## The phase's cited finding is STALE. It was already fixed before this phase ran.

The evidence block names four sites as broken. None of them is broken on the tree
this phase started from (`cc50661`):

| Cited evidence | State at phase start |
|---|---|
| `src/interpreter.rs:697-704` — `Statement::Break`/`Statement::Skip` are each a `// TODO: Implement proper control flow` evaluating to `Ok(Value::Nothing)` | **No such text exists.** `grep -rn 'TODO: Implement proper control flow' src/` printed nothing before this phase made any edit. The statements are at `src/interpreter.rs:1268` and `:1273` and they call `raise_loop_control`, which is the real implementation. |
| `src/bytecode/vm.rs:1496-1505` — `break_loop`/`skip_loop` are each just `self.advance(frame)` | **No.** They are at `src/bytecode/vm.rs:2034` and `:2051`. Both find the owning loop by instruction offset and then call `leave_owned_loop` / `turn_over_to`, which run the `finally` of every `try` crossed and pop the frames between the instruction and the loop. |
| `for each i in [1,2,3]` + `break` prints 1, 2, 3 | **It printed `1`.** Verified on both engines before any edit — see below. |
| `repeat 5 times` + `break` prints `never` five times | **It printed nothing.** |
| `tests/test_lists.rb:104` `expect seen to be 4`, `:106` `expect total to be 10` | **Already corrected.** `:104` is `expect seen to be 3`; the skip case asserts `total` is 8; the break case at `:119` asserts `total` is 3. |
| comment at `tests/test_lists.rb:92` admitting the words are `parsed but no loop honours them yet` | **Gone.** The comment at `:93` reads the opposite: "a `skip` ends the turn it is in". |

### Who fixed it

`git log` on `tests/loop_control_test.rs` and `tests/test_lists.rb`:

- `861080d resume phase-025: adopt preserved work`
- `054e0d6 rbops: phase-022`
- `bba8b60 rename tree-walking vm.rs to interpreter.rs`

phase-025's preserved work was adopted **under phase-022's id**, which is the id
`dispatch.sh` actually picked up. `tests/loop_control_test.rs` is 2839 lines and
81 test functions, and its module doc at `:1-12` describes the no-op defect in
exactly the words the phase prompt uses — so phase-022 knew about this finding,
fixed it, and left the description of the defect behind as the rationale for its
own tests.

**So the phase prompt's premise — "phase-025 is `.blocked` with 0 commits and will
never run again, this phase re-owns the finding" — is right about the id and wrong
about the work.** The work exists, on `main`, green, with its own test file.

## What this phase did anyway

Per the phase's own instruction: *"If the finding no longer reproduces on `main`,
stop and write that to `FINDINGS.md` — a stale phase must never be 'fixed' by
inventing a change."* So **no production behaviour was changed.** Inventing a
second implementation of `break` would have been exactly the failure mode that
warning names.

One item in the phase's Definition of Done was, however, **genuinely unmet**, and
it is a verification gap rather than a behaviour gap:

> "a test that counts executed BREAK and SKIP instructions proves the handlers are
> reached"

Nothing counted them. Every existing check reads printed output, and printed
output **cannot distinguish a working `break` from a dropped one** wherever the
loop would have ended anyway — which is precisely the shape the original defect
took. So the class the phase names could regress with the entire suite green.

phase-036 closed that gap: `BytecodeVm::loop_control_counts` and
`Vm::loop_control_counts` count the dispatched instructions on both engines, and
`tests/loop_instruction_test.rs` asserts them. Both counters were verified
load-bearing by neutering each handler in turn and watching tests go red (§
"Mutation check" in REPORT.md).

## For the auditor

1. **phase-025 should be marked `.done`, not `.blocked`.** Its work is on `main`
   under phase-022's commit. Leaving it blocked will keep re-generating phases
   against a fixed defect. This is the second time a stale-cited phase has been
   dispatched (phase-035 found the same shape, see `phases/phase-035/REPORT.md`).
2. **The stale-citation check belongs in the dispatcher.** Both stale phases
   cited a finding that had been fixed by adopted work from a *different* id.
   A cheap gate — "does the cited `file:line` still contain the cited text?" —
   before dispatch would have caught both.
3. **Unrunnable gate: `./rbops/verify.sh` does not exist in this checkout.** The
   phase prompt lists it as the fourth of four gates. `ls rbops/` →
   `No such file or directory`; the only shell script in the project root is
   `export.sh`. The gate could not be run, so `phases/phase-036/REPORT.md` records
   it as **not run** rather than claiming a pass. The other three gates were run
   and are recorded with their real output.