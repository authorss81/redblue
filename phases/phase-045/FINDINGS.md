# Phase 045 — FINDINGS

Work that is real, was not in this phase's scope, and is not done. Each entry is
anchored to a line that was read in this run.

## 1. `rb run` / `:run <file>` cannot be debugged

`src/repl/mod.rs:843` — `Repl::run_file` calls `crate::run_file(path)`, which goes
to `src/lib.rs:106` → `interpreter::run_isolated_in(&ast, root)`. That function
builds its `Vm` **inside** the spawned thread
(`src/interpreter.rs:195-203`: `builder.spawn(move || { let mut vm =
Vm::new_in(&root); vm.run(&threaded) })`), so a hook installed on the REPL's own
`self.vm` — the only VM the debugger attaches to, `src/repl/mod.rs:arm_debugger` —
cannot reach it.

Fixing it means giving `run_isolated_with_depth_in` an optional `DebugHook` and
threading it through the closure. `DebugHook` is already `+ Send`
(`src/debugger.rs:62`) precisely so this is possible. That changes the `rb run`
path, which is not what phase-045 was for.

## 2. No `Watch`, no `Backtrace`

The finding named six missing commands; this phase delivered four of the six plus
`:breaks` and `:delete`.

`Backtrace` is cheap and was left out only for want of room: `DebugEvent` already
carries `scopes` and `depth`, so the caller chain is exactly the scopes. It needs
`DebugEvent` to grow a `frames: Vec<&CapturedScope>` and `Frame::capture` to be
given more than the innermost view.

`Watch` cannot be built on the current design without giving it up. A watch
expression has to be *evaluated by the VM*, from inside the hook — and the hook is
called while `&mut self.debug_hook` and `&self.locals` are both borrowed from the
`Vm`, so it has no way to call `Vm::evaluate`. The alternative is a second,
narrower callback (`&mut dyn FnMut(&str) -> Result<Value>`) that the VM would call
with the event, which re-opens the borrow question the current design exists to
avoid. Whoever picks this up should decide deliberately whether a debugger is
allowed to evaluate expressions, and what that does to the "the hook only observes"
contract this phase established.

## 3. The REPL runs each balanced block as its own program

`src/repl/mod.rs:execute_line` runs a buffer the moment `open_block_depth` reaches
zero, so:

```
to add(a, b)
    set total to a + b
end
say add(2, 3)
```

runs as two programs. Line numbers for `:break` are therefore the *block's* lines,
and any debugging scenario needs one block (`repeat 1 times … end`). This is not a
debugger defect and was not changed here — but it is the first thing a user will
trip over, and a future phase could make the session hold a block until a blank
line or a second command.

## 4. `:debug` had no relationship to anything

Replaced with a real report of the session's breakpoints. Noted only because the
old text (`Debug mode - Not implemented yet`) is the sort of thing a reviewer may
want to grep for: it is gone from `src/repl/mod.rs`.
