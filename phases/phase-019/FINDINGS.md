# phase-019 — FINDINGS

## 1. The checkpoint merge did not compile, and the collision was in the format

`e97983a` merged phase-024 (`0ff5fe7`) with the phase-019 checkpoint
(`7fdea15`). Both branches had added an opcode at the end of the table, and the
merge kept only one of them:

```
$ git show e97983a:src/bytecode/opcode.rs | grep -c DeclareConst
1
$ cargo check --all-targets
error[E0599]: no variant, associated function, or constant named `DeclareConst`
             found for enum `Opcode`
  --> src/bytecode/codegen.rs:196:36
error[E0061]: this method takes 1 argument but 2 arguments were supplied
  --> src/vm.rs:731:26
```

`src/vm.rs` was left worse than a missing symbol: `load_module` had the new
three-line body *and* the last line of the old one (`self.modules.insert(name.to_string(), ast)`),
so it referenced a `name` and an `ast` that no longer existed.

**Resolved in this phase, not here.** Both opcodes are needed and both are now
present: `DeclareConst` was restored to the table and implemented, and the
end-of-try marker was moved off a new byte onto the reserved `Nop`. The
format-level consequence is recorded below.

## 2. `END_TRY` cannot have a byte value of its own

`docs/BYTECODE.md:218` pins `DECLARE_CONST` to byte 46, and
`tests/bytecode_test.rs::edge_the_constant_instruction_has_a_byte_of_its_own`
pins both that value *and* that the table ends there:

```rust
assert_eq!(
    Opcode::ALL.len(),
    Opcode::DeclareConst.to_byte() as usize + 1,
    "the table is indexed by byte value, so it ends at the last instruction"
);
```

The checkpoint added `EndTry` *after* `DeclareConst`, which renumbers
`DeclareConst` to 47 and fails that pre-existing test. AGENTS.md rule 2 forbids
weakening it, and renumbering byte 46 would break every `.rbc` a version-3
compiler has written.

**What this phase did instead.** `Nop` (byte 0) was already reserved as "a
filler, never emitted by this compiler", so the end-of-try marker rides on it —
but on its *operand*, not on the byte alone. `END_TRY_MARKER` (`0xFFFFFFFF`, the
format's existing reserved value, which no pool or block list can reach) is the
one `arg` of a `Nop` that ends a region; every other `arg` — including the `0` a
filler carries — is the filler. The opcode table is unchanged and no version bump
is needed. If a future phase wants a mnemonic for the marker, byte 47 is free
*once* the `ALL.len()` assertion is generalised from "`DeclareConst` is last" to
"the table has no gaps and no byte is reused" — which is the property it was
reaching for. Recorded so the auditor can decide whether that is worth a
format-version phase.

The round-1 review found that the byte-0 marker as first written was not enough:
`Nop` ended a region on *any* `NOP`, and the failure path resumed *at* the
marker. See §7 and §8.

## 3. A module's `constant` was dropped by the bytecode loader (fixed here)

Found by this phase, not by the checkpoint. `compile_module` filtered the
module's top level to `Statement::Set` only, so `modules/MathUtils.rb`'s
`constant PI to 3.14159` bound nothing and the read fell through to the
builtin's `PI`:

```
$ printf 'import MathUtils\nsay PI\n' > /tmp/x.rb
$ rb run /tmp/x.rb
3.14159
$ rb compile /tmp/x.rb -o /tmp/x.rbc && rb vm /tmp/x.rbc
3.141592653589793
```

The corpus differential test did not catch this, because no corpus program
*reads* a module's constant. Fixed here and pinned by
`edge_a_modules_constant_binds_rather_than_falling_through_to_the_builtin`.

## 4. A module's `set` was inserted into globals rather than refused onto a constant

Not a defect in itself, but worth recording because the two VMs disagreed about
it before this phase and the difference is now load-bearing. `Vm::load_module`
on the tree-walking side used `bind_module_name`, which refuses to write onto a
constant the program has already bound; the bytecode side wrote into
`self.globals` directly. The bytecode side now goes through
`declare_const`/`refuse_constant_rebind`, pinned by
`edge_a_modules_constant_cannot_be_rebound_by_the_importing_program`.

## 5. `modules/*.rb` functions are still unreachable — carried, not fixed

A module's `to ... end` functions are not bound to a name, so
`MathUtils.circle_area(5)` cannot be called. This is pre-existing on the
tree-walking VM and unchanged here: both VMs agree that it fails. It belongs to
a module phase, not to the bytecode VM.

## 6. `Value`'s nesting depth is still a native-stack limit

`edge_both_vms_answer_the_same_at_a_nesting_depth_neither_overflows` pins that
both VMs agree at 400 levels of nesting. Past that, the recursive drop of
`Value` overflows the machine stack in `src/value.rs` — a property of `Value`,
not of the bytecode VM's loop, and out of scope here. The bytecode VM's *own*
recursion (call depth) is enforced: `edge_both_vms_stop_unbounded_recursion_at_the_same_depth`.

## 7. Round-1 review: the end-of-try marker was a byte, and byte 0 was not enough

Four defects, all found by the round-1 review of this phase's own diff, all
reproduced before the fix and pinned after it.

### 7a. BLOCKER — a handled inner `try` ran the enclosing `finally` early

`handle_failure` resumed *at* the region's closing `NOP`, and `Nop` pops
whatever handler is on top. After an inner failure has been handled the handler
on top is the **enclosing** one, so the inner marker popped it and ran its
`finally` before the rest of its protected code — and everything after the inner
`try` then ran with no protection at all.

```
$ cat nested.rb
set log to ""
try
    set log to log + "a"
    try
        say 1 / 0
    catch
        set log to log + "c"
    finally
        set log to log + "f"
    end
    set log to log + "b"
finally
    set log to log + "F"
end
say log
$ rb run nested.rb            # the tree-walking VM, which is the specification
acfbF
$ rb vm nested.rbc            # before the fix
acfFb
```

**Fixed** by resuming *past* the marker (`target + 1`) on the failure path: both
handlers have already run there, so the instruction has nothing left to do, and
skipping it is what keeps the enclosing region installed. Pinned by
`edge_a_handled_inner_try_leaves_the_enclosing_region_protected`, whose second
program fails outright without the fix.

### 7b. BLOCKER — every `NOP` closed a region, on both paths

The marker was byte 0 with no operand, so a `.rbc` holding a filler `NOP` inside
protected code ended the region there: the success path popped the handler and
ran the `finally` early, and the failure path's scan stopped at the filler and
resumed *inside* the region it had just caught a failure in. The comment claiming
"the nesting count is what tells the two apart" was false — the count separates a
nested region's closer from this one's, not a filler from a closer.

**Fixed** by `END_TRY_MARKER` on the operand (§2). The filler's arg is `0`, the
marker's is `0xFFFFFFFF`, and both paths compare. Pinned by
`edge_a_filler_nop_does_not_close_a_protected_region` (hand-built chunks, since
the compiler emits no filler of its own) and by
`a_trys_region_ends_at_one_marked_nop_and_writes_no_other_filler` on the compile
side.

### 7c. BLOCKER — a second `import` re-ran the module

The bytecode VM had no already-loaded set, so it recompiled and re-ran a module
on every import. A module's `constant` cannot be declared twice, so the second
import of `MathUtils` failed on the module's own first declaration while the
tree-walker — which has kept an already-loaded table all along — answered with a
no-op:

```
$ printf 'import MathUtils\nimport MathUtils\nsay PI\n' > x.rb
$ rb run x.rb
3.14159
$ rb vm x.rbc
Error: RuntimeError: Constant 'PI' is already declared
```

**Fixed** with `BytecodeVm::modules`, a set keyed by the name the program wrote,
inserted before the module runs. Pinned by
`edge_importing_the_same_module_twice_loads_it_once`.

### 7d. MAJOR — a loop variable was refused for shadowing a constant

`store` called `refuse_constant_rebind` before the loop-variable check, which
contradicted its own comment and diverged from `Vm::set_var`, where a live local
is written without a refusal. `constant X to 1` plus `for each X in [1, 2]` reads
1, 2, 1 on the tree-walker and failed on the bytecode VM.

**Fixed** by moving the refusal into the plain-binding path. Pinned by
`edge_a_loop_variable_shadows_a_constant_instead_of_being_refused`.

## 8. Round-1 review: two smaller ones

- **MAJOR — a test could not fail for the reason it claimed.**
  `edge_a_nop_outside_a_try_does_not_run_an_enclosing_finally` said the program's
  own two `set`s compiled to a filler `NOP` between them; `codegen.rs` wrote no
  filler anywhere, so the test asserted nothing about fillers. It is replaced by
  `edge_a_filler_nop_does_not_close_a_protected_region`, which hand-builds the
  chunk and puts a filler inside the protected region — where the claim is
  checkable, and where it fails without the fix.
- **MINOR — a doc contradicted its code.** `import` said "only the `set`
  statements are kept" while `compile_module` had been keeping `Constant` too
  since this phase. The doc now says both.
- **MINOR — the tree-walker read and parsed a module twice.** `load_module` called
  `module_bindings`, which parsed the file, and then called `module_program` on
  the same path to remember the module — so a second read that failed left the
  program holding half a module. `module_bindings` now takes the parsed
  `Program`, and `load_module` reads the file once.