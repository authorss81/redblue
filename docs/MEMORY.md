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
variant, and hold their contents inline. A function value shares for the same
reason, and it does so in two fields on two separate lines rather than one:
`body: FunctionBody` at `src/value.rs:85-91`, whose `Arc` is spelled out at
`src/value.rs:88-90`, and `captured: Arc<Captured>` at `src/value.rs:93`. Reading
a function value therefore hands out two handles — one for the body, one for the
scopes it closed over — rather than copying a body and a stack of scopes.

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
writes through when this value holds the last handle.

**There are exactly two property writes, and both are `set x.y to z`.** The
tree-walking VM is `src/interpreter.rs:1619-1630` — the guard at
`src/interpreter.rs:1625` matches `Value::Record` and nothing else, and the
write at `src/interpreter.rs:1626` is the `make_mut` above. The bytecode VM is
`src/bytecode/vm.rs:2652-2675`, the same guard at `src/bytecode/vm.rs:2664` and
the same write at `src/bytecode/vm.rs:2665`. Neither reaches a `Value::Object`,
and neither has to: an `object` declaration binds a `Record`
(`src/interpreter.rs:2523`), so no program can produce the `Object` variant and
there is no third shape to cover. A property write whose target is not a record
is **silently left alone** rather than reported — `set x to 5` then `set x.a to 1`
leaves `x` as `5` — which is a consequence of the guard being a pattern match
and not a check, and is called out here because it is the one place the rule
below is not visible.

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
no `Weak` anywhere in `src/`, no `Rc` in `src/value.rs`, no `Gc`, and no crate
in `Cargo.toml` that collects anything — so nothing in Redblue could break such
a cycle even if one existed: there is no back-reference to weaken and no
collector to notice the unreachable island.

### What leaks, and how much of it

A cycle of real records is never reclaimed, and the suite measures that in bytes
rather than asserting it in words. `tests/memory_model_test.rs` installs a
counting allocator, runs a Redblue program that grows a 400,000-element list
into two records, joins those two records into a cycle, drops every handle that
is not part of the cycle, and then asks the allocator how much is still held:

- the whole payload of the program's list is still allocated — tens of
  megabytes above the baseline — with **nothing outside the cycle** naming it;
- letting go of the last external handle afterwards releases **zero** bytes;
- and the record's own storage, which is `Shared<Fields>` like any other
  record's, is still there with a strong count of one.

That is what "never reclaimed" means, and it is the same measurement a `Weak`
back-reference or a tracing collector would have to change. If either landed, the
first number would fall.

### Why no Redblue program can build one today

No Redblue program reaches that cycle, so the limitation is latent rather than
active, and the reason is in the representation rather than in a missing
feature.

**A Redblue container is filled before the handle to it exists.**
`Value::list`, `Value::record` and `Value::object` (`src/value.rs:279-292`) each
build the payload first and hand out an `Arc` to it afterwards, and the payload
types — `Vec<Value>` and `Fields` — have no interior mutability, so there is no
slot that could later be filled with a handle to the container itself. `Arc` will
hand out a `&mut` to the payload only to the *last* handle it has, which is why
the missing capability is not a missing function but a missing type. Filling one
of these slots after the fact would need interior mutability the representation
does not have, which is exactly the one thing the test above supplies by hand
and exactly the thing `src/value.rs` does not.

Every write follows from that. A property write reads the value being stored
first (`src/interpreter.rs:1624`), so at the moment `make_mut` runs the storage
is held twice — by the binding and by the value about to be stored into it — and
`make_mut` copies. `Value::append` (`src/value.rs:303-311`) is the only write
that can go through, and it can only do so when the binding holds the last handle,
which is precisely the case in which nothing inside the payload can be pointing
back at it.

So the nearest a program gets is storing a record through its own field:

```redblue
set node to { name: "node", self: nothing }
set node.self to node
say node.self.self   // nothing
```

`node.self` holds the record *as it was before the write*: the value being stored
was itself a live handle, so `Arc::make_mut` copied before writing, and the new
storage never ended up holding a handle to itself. An alias or a cycle would make
that field the record itself, and the program could never finish printing it.

Run to completion, this program hands back every byte it allocated — and so does
the 400,000-element one above, which
`edge_a_redblue_program_hands_back_everything_it_allocated` checks under the same
allocator. The two are one measurement read in opposite directions, so the
megabytes the cycle keeps are attributable to the cycle and not to the program
that built the records.

The leak becomes reachable the moment any of these lands, and none of them is
hypothetical for a language with objects, closures and a standard library:

- a value that can contain itself (a self-referential record, a linked structure
  built by mutation rather than by replacement);
- interior mutability in a payload — a `RefCell` field, a `Cell`, an `UnsafeCell`
  — which is the one capability the test above supplies by hand and the one
  `src/value.rs` does not have;
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
illegal, which is never what an operator means. `run_isolated` resolves it for
every program it starts, so `rb run` and the test harness get the same limit.

`redblue::Vm::with_max_call_depth` sets the same limit in process instead,
without reading the environment at all, and ignores a zero for the same reason.

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

`tests/memory_model_test.rs` checks every claim above against the code, measures
the cycle leak and the absence of a program that leaks in the bytes they keep,
and checks that the `file:line` citations in this document still say what this
document says they say. If the representation changes, the documentation changes
with it or the suite goes red.
