# The Redblue memory model

This document states the memory model exactly as it is implemented. It is a
description, not a proposal: nothing here is planned, and nothing here is a
defect waiting to be filed except where it says **known limitation**.

## The model in one paragraph

A Redblue value is immutable once it is constructed. Reading a value out of a
variable — passing it, storing it, returning it — copies the *binding*, not the
contents, and a write builds a new value rather than changing the old one. There
is no garbage collector, no tracing, no ownership rule and no borrow checker,
because none of them are needed to keep that promise: a value is never changed
in place, so two bindings can never be caught out by a change made through
either. What sharing buys is that a copy is cheap — a list, a record and an
object are reference-counted handles, so handing one around costs an atomic
increment rather than a walk of its contents.

## Values are immutable once constructed, and shared

`src/value.rs:253-255` is the whole of it:

```rust
List(Shared<Vec<Value>>),
Record(Shared<Fields>),
Object(String, Shared<Fields>),
```

and `src/value.rs:276` is what `Shared` is:

```rust
pub type Shared<T> = std::sync::Arc<T>;
```

So a list, a record and an object's fields all live behind one `Arc`. Cloning a
`Value` clones the handle; the payload is not walked and not copied. This is an
optimisation only — the correctness argument does not depend on it, because
nothing writes through the shared payload without copying first.

`number`, `text`, `yes/no` and `nothing` are `f64`, `String`, `bool` and a unit
variant, and hold their contents inline. A function value holds its body and
captured scopes behind `Arc`s for the same reason (`src/value.rs:88-90`).

## Reading a variable copies rather than aliases

`src/value.rs:88-90` states the rule:

> Behind an `Arc` because reading any variable clones the value bound to it,
> and a recursive call reads the name it was reached through.

So `set second to first` gives `second` its own binding of the same shared
payload, and writing through `second` is invisible through `first`:

```redblue
set first to [1, 2]
set second to first
set second to push(second, 3)
say length(first)   // 2
say length(second)  // 3
```

The two names share one allocation until the write. The write goes through
`Arc::make_mut`, which copies the payload whenever the handle is shared and only
writes through when this value holds the last handle — `set node.self to node`
at `src/interpreter.rs:1626` is the shape of every property write.

The rule this keeps a program's meaning unchanged is therefore: **read by
cloning the handle, write through `Arc::make_mut` and never through `&mut`**.
A `&mut Vec` reaching an element of a shared list is the one way to make a change
a program cannot see through the name it made it with.

The cost is copying rather than zero-cost reference passing. For a language whose
goal is plain-English readability, a programmer never has to think about
ownership — which is the trade the model makes, deliberately.

## Known limitation: no cycle collection, so a cycle leaks

**There is no tracing garbage collector, and there is no cycle collector. Every
value is reclaimed by reference count alone, and a reference cycle is never
reclaimed at all.**

`Arc` frees its payload when the strong count reaches zero. Two objects that
hold each other never do, because each keeps the other's count at one. There is
no `Weak` anywhere in `src/`, no `Rc`, and no `Gc`, so nothing in Redblue could
break such a cycle even if one existed — there is no back-reference to weaken and
no collector to notice the unreachable island.

**Today no Redblue program can build a cycle**, and it is worth being precise
about why, because the limitation is latent rather than active. A Redblue
container is filled *before* the handle to it exists, so there is no slot that
could later be filled with a handle to the container itself. The nearest a
program gets is storing a record through its own field:

```redblue
set node to { name: "node", self: nothing }
set node.self to node
say node.self.self   // nothing
```

`node.self` holds the record *as it was before the write*: the value being stored
was itself a live handle, so `Arc::make_mut` copied before writing, and the new
storage never ended up holding a handle to itself. An alias or a cycle would make
that field the record itself, and the program could never finish printing it.

The leak becomes reachable the moment any of these lands, and none of them is
hypothetical for a language with objects, closures and a standard library:

- a value that can contain itself (a self-referential record, a linked structure
  built by mutation rather than by replacement);
- a native resource whose handle is closed by a finaliser that the finaliser's
  own registration keeps alive;
- any runtime structure that keeps values in a registry keyed by identity.

Each of those is a leak of unbounded growth in a long-running program, and none
of them has a workaround at the Redblue level today. The honest statement is that
it is a limitation of the design and not a bug to be fixed later by patching a
line: adding cycle collection means either a `Weak` back-reference in the value
representation or a real tracing collector, and both are a language-level change
that needs its own phase.

## Deep recursion is bounded, and the bound is catchable

Recursion is bounded by `MAX_CALL_DEPTH` (1000, generated by `build.rs:19` and
re-exported as `redblue::MAX_CALL_DEPTH`). Exceeding it raises
`Error::Limit(String, Span)` — a clean failure naming the function and the
limit, exit code 1, not an abort.

`src/bytecode/vm.rs:18-21` states why the bytecode VM can promise this:

> That is what makes deep recursion safe here: [`BytecodeVm::call_depth`] counts
> frames in a `Vec`, so a call chain as deep as
> [`DEFAULT_MAX_CALL_DEPTH`](crate::interpreter::MAX_CALL_DEPTH) costs heap
> rather than machine stack. There is no path from a `.rbc` to a native stack
> overflow.

The tree-walking VM counts the same way, and a program that recurses without end
is catchable like any other error:

```redblue
to boom(n)
    boom(n + 1)
end

set caught to no
try
    boom(0)
catch error
    set caught to yes
end
say caught   // yes
```

### `REDBLUE_MAX_CALL_DEPTH` overrides it

`MAX_CALL_DEPTH_ENV` is `REDBLUE_MAX_CALL_DEPTH`
(`src/interpreter.rs:42`). `resolve_max_call_depth()` (`src/interpreter.rs:139`)
reads it and falls back to `MAX_CALL_DEPTH` when the value is absent,
non-numeric, zero or negative — a limit of zero would make every function call
illegal, which is never what an operator means. `redblue::Vm::with_max_call_depth`
sets the same limit in process, without the environment.

```bash
REDBLUE_MAX_CALL_DEPTH=5 rb run deep.rb
```

Loop iteration (`REDBLUE_MAX_ITERATIONS`) and whole-program steps
(`REDBLUE_MAX_STEPS`) are bounded the same way and raise the same `Error::Limit`.

## What is not here

No ownership, no borrowing, no lifetimes, no destructuring that moves, no
`null` — the absence of all four is a feature of a language that reads as plain
English, and the price paid for it is copying rather than zero-cost reference
passing.

## Tests

`tests/memory_model_test.rs` checks every claim above against the code, and
checks that the `file:line` citations in this document still say what this
document says they say. If the representation changes, the documentation
changes with it or the suite goes red.