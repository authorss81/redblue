# Redblue's memory model

Every value in a running program is one of nine shapes, and everything about
memory follows from how those shapes are stored.

```rust
// src/value.rs:248-257
pub enum Value {
    Nothing,
    Number(f64),
    Text(String),
    YesNo(bool),
    List(Shared<Vec<Value>>),      // Shared<T> = std::sync::Arc<T>
    Record(Shared<Fields>),
    Object(String, Shared<Fields>),
    Function(FunctionValue),
    Builtin(&'static str),
}
```

## There is no garbage collector, and none is planned

`src/value.rs:276` defines the whole ownership strategy in one line:

```rust
pub type Shared<T> = std::sync::Arc<T>;
```

An `Arc` is freed when its **last reference** goes away. That is the whole
mechanism. There is no tracing collector, no mark-and-sweep pass, and nothing
that runs in the background looking for values nothing points at.

The practical consequence: memory is reclaimed **predictably**, at the moment a
value becomes unreachable, without waiting for a collection cycle. There is no
pause, and no memory graph to walk.

## Reading a variable copies the value

This is the part most likely to surprise you, so it is worth being precise about.

```rust
// src/value.rs:88-90, on FunctionValue::body
/// Behind an `Arc` because reading any variable clones the value bound to
/// it, and a recursive call reads the name it was reached through.
```

**Assignment copies; it does not alias.** `set a to b` does not make `a` and `b`
the same list. Mutating one cannot change the other. There is no reference type
in the language, so there is no way to accidentally build an alias.

The cost is copying, not zero-cost. For a long list that means real work, and
`Value::List` wrapping its contents in an `Arc` means the *outer* container is
shared cheaply while anything still costs a clone to read.

**What this buys:** a language where nothing can be accidentally aliased, and no
borrow checker to learn. **What it costs:** copying rather than passing by
handle. For a language whose first goal is readable English, that is a
deliberate trade, not an oversight.

## A reference cycle leaks — this is a real limitation

There is no `Weak` anywhere in the source tree. A zero-count check is the honest
description:

```bash
grep -rc "Weak<" src/    # 0
```

Because an `Arc` frees only when the **last** reference drops, two values that
reference each other keep each other alive forever:

```
list a contains list b
list b contains list a
      ↓
neither ever reaches zero references
      ↓
neither is ever freed
```

**This is a genuine limitation of the current design and is documented as one.**
It is not a bug to be fixed in a later phase — it is a property of reference
counting without weak references. The fix, if it is ever wanted, is a tracing
collector; the cost is a background traversal and the loss of deterministic
reclamation.

Most programs never build a cycle, because building one requires *deliberately*
making two structures reference each other. When one does leak, it leaks
quietly and slowly, which is the worst way for it to present itself.

## Numbers are `f64`, and integers stop being exact at 2^53

```rust
// src/value.rs:250
Number(f64),
// src/value.rs:109
/// `2^53`, the largest magnitude at which every whole number is still held
/// exactly.
```

Every number — including one written as an integer literal — is an IEEE-754
double. Values below `2^53` are exact. `9007199254740993` is not `9007199254740992`.

This matters more than it looks:

- **Money cannot be represented exactly.** Adding `0.1` ten times does not give
  `1.0`. This is the single most common way Redblue surprises someone.
- Loop counters are fine at any realistic size.
- Division can produce values with no exact representation, as can `sqrt` and
  `log`.

## Deep recursion is bounded, and the bound is catchable

Redblue does not overflow the native stack, because it does not use it the way a
naive interpreter does. Recursion is counted against a limit:

```
MAX_CALL_DEPTH = 1000        (build.rs:19)
MAX_ITERATIONS = 1_000_000   (interpreter.rs)
MAX_STEPS      = 32_000_000  (interpreter.rs)
```

The call-depth limit and the per-frame stack budget are deliberately the same
number, declared once in `build.rs` and shared between the interpreter and the
WebAssembly linker — otherwise the two would drift and one of them would be
wrong.

Exceeding the limit raises a **catchable** `Error::Limit`:

```redblue
try
    set deep to recurse(99999)
catch error
    say "caught: " + error
end
```

So runaway recursion is a recoverable error in a Redblue program, not a crash
that takes the process down. `src/bytecode/vm.rs:18-21` states it as a property
worth having: there is no path from a `.rbc` to a native stack overflow.

Each limit is overridable by environment variable when you need different
bounds:

| Limit | Variable |
|---|---|
| Call depth | `REDBLUE_MAX_CALL_DEPTH` |
| Loop iterations | `REDBLUE_MAX_ITERATIONS` |
| Total steps | `REDBLUE_MAX_STEPS` |

## Summary

| Question | Answer |
|---|---|
| Is there a garbage collector? | No. `Arc` frees on last reference. |
| Is there cycle collection? | No `Weak` exists, so **a cycle leaks**. |
| Does assignment alias? | No. Reading a variable **copies**. |
| Are there references or pointers? | No. Nothing can alias. |
| Are integers exact? | Up to `2^53`. Past that, floating point. |
| Can I run out of stack? | No. Bounded by `MAX_CALL_DEPTH`, catchable. |

## See also

- `SPEC.md` — the language specification
- `docs/GRAMMAR.md` — grammar reference
- `src/value.rs` — where every claim above is defined