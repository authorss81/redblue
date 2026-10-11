# phase-043 — FINDINGS

Work this phase did not do, with the evidence that prompted it.

## 1. A `return` inside an `if` does not return from the function — pre-existing, not from this phase

`tests/wasm_test.rs::edge_recursion_beyond_the_call_depth_is_a_clean_error_not_an_abort`
was written to pin the playground's call-depth behaviour, and the native run
turned up something it was not looking for.

A function that calls *itself* reports the depth limit on every recursion, not
only on deep ones:

```
$ cat target/tmp/f.rb
to f(n)
    if n is 0 then
        return 0
    end
    return f(n - 1)
end
say f(3)

$ ./target/debug/rb run target/tmp/f.rb
Error: RuntimeError: Maximum call depth of 1000 reached while calling 'f'
  --> target/tmp/f.rb:5:5
5 |     return f(n - 1)
  |     ^
```

Depth 3, limit 1000. The same program under the WebAssembly module gives the
same error, so the two builds agree — which is what this phase had to guarantee,
and it does.

**This is pre-existing.** Verified by stashing every `src/` change in this phase
and rebuilding from a pristine tree:

```
$ git stash push -- src/runtime.rs src/interpreter.rs src/lib.rs src/bytecode/vm.rs
$ cargo build --bin rb && ./target/debug/rb run target/tmp/f.rb
Error: RuntimeError: Maximum call depth of 1000 reached while calling 'f'
```

Identical. It is not caused by this phase, and fixing it would change what
programs the language accepts — a language-semantics change that phase-043 does
not open.

### The cause, found in review round 1

Round 1 recorded the symptom as a suspected `call_depth` leak. It is not one —
the increments and decrements in `call_user_function` are balanced. The counter
is climbing because the recursion is *unbounded*, not because a decrement is
missing:

```
$ cat target/tmp/i.rb
to h(n)
    if n is 0 then
        return "zero"
    end
    return "nonzero"
end
say h(0)
say h(5)

$ ./target/debug/rb run target/tmp/i.rb
nonzero
nonzero
```

`return` inside an `if` does not return from the function; execution falls
through to the next statement, so `h(0)` prints "nonzero" and `f` recurses with
`n` counting down forever until the depth counter stops it. Which means the
round-1 test `f(100000)` passed for *any* limit and pinned nothing.

The behaviour is already known and worked around elsewhere:
`tests/call_depth_test.rs` says so on its `countdown` helper —

> `reached` is declared at the top level because the analyzer only accepts a
> variable it has already seen, and `give back` inside an `if` branch does not
> return from the function — so the recursive call is assigned, not returned.

Round 1 fixed the test to use that shape and to assert from both sides of the
boundary, so it pins `MAX_CALL_DEPTH` rather than passing vacuously. The
semantics themselves are untouched: a `return` that only returns from its
`if` is a language change, and this phase does not open one.

### What to look at, if it is picked up

`src/interpreter.rs:2026` runs a function body:

```rust
let result = self.execute_statements(body);
```

and `Statement::Return` at `src/interpreter.rs:1325` is

```rust
Statement::Return(expr) | Statement::GiveBack(expr) => match expr {
    Some(e) => self.evaluate(e),
    None => Ok(Value::Nothing),
},
```

— an expression, not a signal. Nothing distinguishes it from the value of an
ordinary statement, so an enclosing `if` treats it as one. Giving it a signal
means deciding what `execute_statements` does with it across `if`, `repeat`,
`for`, loops and `try`, and what a `return` inside a `finally` means — which is
a phase of its own, and one that has to decide about `map` and the bytecode VM
too.

## 2. `rbops/` is not in the checkout

`ls rbops/` → `No such file or directory`, and a filesystem search for
`verify.sh` under any `rbops` path returns nothing. The pipeline lives outside
the working directory, which is this run's only writable and readable scope, so
`./rbops/verify.sh phase-043` could not be run. It is recorded as NOT RUN in
`REPORT.md` rather than claimed. The other gates — `cargo fmt --check`,
`cargo clippy -- -D warnings`, `cargo test --all-targets` — were run and are
reported with what they printed.

## 3. A build script's `#[cfg]` is the host, not the target

Not a Redblue defect and not this phase's, but it cost real time here and will
cost it again in any project that has a `build.rs`:

```rust
#[cfg(target_arch = "wasm32")]          // wrong
{ println!("cargo::rustc-link-arg=-zstack-size=16777216"); }
```

A build script is compiled for the **host**, so on a Linux machine
cross-compiling to wasm this branch is dead and the flag is silently not
emitted. The working form, used in `build.rs`:

```rust
if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") { … }
```

The pre-existing Windows arm directly above it has the same latent issue — it is
correct today only because that build is always native, never cross-compiled.

## 4. `RUSTFLAGS` in the environment silently discards `.cargo/config.toml` target flags

CI here sets `RUSTFLAGS=-D warnings`. An environment `RUSTFLAGS` **replaces**
`[target.<triple>] rustflags` from `.cargo/config.toml` rather than merging with
it, so a wasm-only linker flag written there compiles to nothing, with no
warning. Verified directly: with the config in place and `RUSTFLAGS` set, the
flag never reached the linker.

`build.rs` is immune, which is why the stack argument is emitted there.