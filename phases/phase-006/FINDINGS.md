# Phase 006 — Findings

Out-of-scope items found while working this phase. None were fixed here.

## 1. Deeply nested source aborts in the parser, before the VM is reached

The phase evidence said "infinite or deep recursion aborts the process with a
Rust stack overflow". That is true of user-function recursion (now fixed by this
phase's counter) but there is a second, independent abort: the lexer/parser
recurse once per nesting level with no depth limit, and nothing catches it.

```
$ python3 -c "open('/tmp/n.rb','w').write('set x to ' + '('*20000 + '1' + ')'*20000)"
$ ./target/debug/rb run /tmp/n.rb
thread 'main' has overflowed its stack
fatal runtime error: stack overflow, aborting
$ echo $?
134
```

Also reachable with nested `[[[…]]]` and with nested `try`/`end` blocks. The
fix is a nesting-depth limit in `src/parser.rs` producing a `ParserError`; the
same `STACK_BYTES_PER_CALL` reasoning applies, because the parser runs on the
main thread, not on `run_isolated`'s sized thread.

**Severity: major** (an uncatchable abort on a 60 KB input file).
**Risk: low** — a counter and a `ParserError`, mirroring what this phase did in
the VM.

## 2. The REPL bypasses the sized interpreter thread

`src/lib.rs:run_source` and `src/testing/harness.rs:execute_test_program` now run
through `vm::run_isolated`, but `src/repl/mod.rs:240` still calls `self.vm.run(&program)`
on the main thread. A program with unbounded recursion typed at the prompt
therefore still aborts with exit 134 rather than reporting a depth error.

Wrapping it is not a one-liner: the REPL keeps one `Vm` alive across lines so
that variables persist, and moving that `Vm` in and out of a sized thread per
line would change what survives between lines. It needs a design decision, not
a patch.

**Severity: minor** (interactive surface only, no file or test can hit it).
**Risk: medium.**

## 3. Argument arity is not checked when a call binds its parameters

`Vm::call_user_function` binds `params[i]` to `args[i]`, uses `nothing` for a
missing argument and ignores a surplus one. `to f(a)` called as `f(1, 2)` runs
with `a = 1`; `to f(a, b)` called as `f(1)` runs with `b = nothing`, and a later
`b + 1` fails with a confusing `RuntimeError` far from the call.

```
$ printf 'to f(a, b)\n  give back a + b\nend\nsay f(1)\n' > a.rb
$ ./target/debug/rb run a.rb
Error: RuntimeError: Cannot add non-numbers
  --> a.rb:2:10
2 |     give back a + b
```

Deliberately not fixed here: this phase's concern is the depth bound, and an
arity error is new user-visible behaviour that needs its own tests and its own
decision about whether a missing argument is an error or `nothing`.

**Severity: minor.** **Risk: low.**

## 4. Loop iteration count is still unbounded

The depth counter bounds *calls*, not iterations. `while yes then end`, and
`for each x in [1,2,3]` nested inside itself by hand, still run until the
process is killed. Note that `break` and `skip` are parsed and ignored
(`phases/phase-003/FINDINGS.md` §4), so there is currently no way for a
Redblue program to stop an unbounded loop at all.

```
$ printf 'set n to 0\nwhile yes then\n  set n to n + 1\nend\n' > /tmp/w.rb
$ ./target/debug/rb run /tmp/w.rb    # never returns
```

A statement-count or iteration budget with a `RuntimeError` is the same shape of
fix as this phase's.

**Severity: major.** **Risk: medium** — it interacts with the missing
`break`/`skip` control flow.

## 5. `give back` inside `if` does not return from the function

A function body yields the value of its *last* statement, so `give back` in a
branch does not leave the function; execution falls through to whatever follows.
The naive recursion below therefore counts *down* through zero and keeps going,
and now ends at this phase's depth limit instead of aborting:
This is what makes the obvious way of writing recursion wrong:

```
to countdown(n)
    if n is 0 then
        give back 0        # does NOT return — countdown(-1) runs next
    end
    give back countdown(n - 1)
end
```

A program must assign in the branches and `give back` after them (the shape
`tests/test_functions.rb` now uses). Recursion depth accounting is correct
either way; this is a control-flow gap of the kind filed in
`phases/phase-003/FINDINGS.md` §4.

**Severity: major.** **Risk: medium** — needs the same control-flow signal
`break`/`skip` need.

## 6. `Vm::with_max_call_depth` ignores the environment but `run_isolated` does not

`with_max_call_depth` overrides the limit on a `Vm` the caller then runs itself;
`run_isolated` sizes its thread from `resolve_max_call_depth()`, which reads the
environment. A caller that builds a `Vm::with_max_call_depth(4)` and hands it to
`run_isolated` would get a 1000-deep stack, not a 4-deep one — safe, but the two
knobs are not interchangeable. Recorded because the pairing is easy to misuse.

**Severity: minor** (latent, no caller does this today).
**Risk: low** — either thread a limit through `run_isolated` or document the
pairing as forbidden.

## 7. `modules/MathUtils.rb` still fails to parse

Confirmed pre-existing and unchanged by this phase (verified with `git stash`):
`constant` is not a keyword, so the file dies in the parser. It is
`phases/phase-003/FINDINGS.md` §6, restated here only because AGENTS.md §2 says
`modules/*.rb` are specification-by-example and this phase touched the function
call path that modules feed into.

**Severity: major.** **Risk: low.**
