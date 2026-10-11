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

## 5. Re-dispatch of phase-043: the finding is stale — the phase is already done

A later run was dispatched against the same phase. It re-verified the finding
before changing anything, as the phase prompt requires, and the finding **no
longer reproduces on `main`**. Commit `50c2792` ("rbops: phase-043") already
implemented all three items of the definition of done. Nothing was changed in
this run; per the phase prompt, a stale phase is recorded here rather than
"fixed" by inventing a change.

### The evidence as quoted, and why it is no longer true

The finding read:

> Cargo.toml defines `[[bin]] rb` and `[lib] redblue` with no wasm target,
> profile, bindgen or bindings anywhere in the tree.

Every clause is now false. `Cargo.toml:28` declares
`crate-type = ["rlib", "cdylib"]`; `src/wasm.rs` defines the `rb_*` C ABI;
`wasm/redblue.js`, `wasm/index.html` and `wasm/check-examples.js` are the
bindings and the runner page.

### Re-verification actually run on this checkout

The documented build, from a clean `wasm32-unknown-unknown` target install,
succeeds and produces a runnable module:

```
$ rustup target add wasm32-unknown-unknown
$ cargo build --release --target wasm32-unknown-unknown --lib
    Finished `release` profile [optimized] target(s) in 32.06s
$ ls -la target/wasm32-unknown-unknown/release/redblue.wasm
-rwxr-xr-x 2 runner runner 1090899 … redblue.wasm
```

The byte-identical-output item of the definition of done, re-run here rather
than taken on trust from the earlier report:

```
$ cargo build --release --bin rb
$ node wasm/check-examples.js
ok    hello.rb  (43 bytes identical)  — say, variables and a function call
ok    fizzbuzz.rb  (68 bytes identical)  — repeat, nested if and for-each
ok    formats.rb  (151 bytes identical)  — json.parse / json.stringify / records
ok    test_arithmetic.rb  (45 bytes identical)  — arithmetic and string formatting
ok    random.rb  (182 bytes identical)  — the seeded random builtins
ok    files.rb  (160 bytes identical)  — the files module, through the interpreter

6 examples, byte-identical under node and native rb.
```

Gates re-run on this checkout: `cargo fmt --all -- --check` clean;
`cargo clippy --all-targets -- -D warnings` clean, zero warnings;
`cargo test --all-targets` green across all 43 test binaries, **1258 passed,
0 failed, 0 ignored** — the same total the earlier `REPORT.md` claimed, so that
report was accurate rather than aspirational.

### DoD item 3 — "no `#[cfg]` branches alter interpreter semantics"

Checked by reading every `#[cfg]` in `src/` (11 sites: `runtime.rs` ×8,
`vfs.rs:228`, `interpreter.rs:213`, `wasm.rs:870`). All are I/O-boundary only,
as the phase claims, and none touches lexing, parsing, analysis or instruction
execution:

| Site | Boundary | Nature |
|---|---|---|
| `runtime.rs:356,371` | clock | `HOST_CLOCK` slot; native reads the system clock |
| `runtime.rs:500` | clock | `now()` returns the host-supplied clock |
| `runtime.rs:743` | sleep | playground refuses rather than blocking |
| `runtime.rs:1381,1406,1425` | stdin / network | `read_line` and `network_get`/`post` refuse cleanly |
| `vfs.rs:228` | filesystem | re-exports the in-memory VFS over `std::fs` |
| `interpreter.rs:213` | stack strategy | wasm has no threads, so the run is inline instead of on a sized thread |
| `wasm.rs:870` | clock | `rb_set_clock` exists in both builds; natively a no-op |

The stack-strategy arm is the only one that changes *how* the VM runs, and it is
scoped to the target rather than to "spawn failed" precisely so a native
`spawn` failure still refuses instead of running unprotected. The depth limit
that both builds enforce is the same product in both, which is what
`edge_recursion_beyond_the_call_depth_is_a_clean_error_not_an_abort` pins.

### Net result for the dispatcher

Phase-043 satisfies its definition of done as committed. The correct outcome is
that this re-dispatch is a **no-op**, and the phase can be closed. The
`must_touch: ["src/"]` check will fail for this run by construction, because a
run that correctly declines to invent a change has no `src/` diff to show —
this is a manifest-bookkeeping artifact, not missing work.

The one genuinely new fact worth recording: `wasm/check-examples.js` is a
**Node** harness and passes here, but the phase also names wasmtime, which is
not installed in this environment and was therefore not exercised. The
underlying guarantee it rests on — that the module is a plain
`wasm32-unknown-unknown` `cdylib` with no host-specific imports — is what makes
it portable to wasmtime, but that claim is untested here and should not be
reported as verified.