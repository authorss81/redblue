# phase-091 — FINDINGS

## 1. No Redblue program can leak today. This is a theorem, not a survey.

**Where:** `src/value.rs:253-255`, `src/value.rs:279-292`,
`src/interpreter.rs:1619-1630`, `src/bytecode/vm.rs:2652-2675`,
`src/runtime.rs:277-299`.

Round 1 was asked for "a Redblue program that leaks because of [no cycle
collection]" plus "a test asserts the documented cycle-leak example leaks". Round
1 answered with a hand-rolled Rust `Node` carrying a `u32`, and the reviewer was
right on both counts: that test cannot fail if Redblue's representation changes,
and it is not a Redblue program. Round 2 does not pretend otherwise. It measures
the leak in bytes, and it states plainly that the program does not exist.

### The theorem

**A cycle in a `Value` graph cannot be built, because there is no interior
mutability to build one with and `Arc` gives out `&mut` only to the last handle.**

The proof has one step and it is a construction-time ordering argument:

1. `Value::list` / `Value::record` / `Value::object` (`src/value.rs:279-292`)
   build the payload and *then* hand out an `Arc` to it. There is no moment
   between construction and the handle.
2. The payload types are `Vec<Value>` and `Fields`, and neither contains a
   `Cell`, a `RefCell`, a `Mutex` or an `UnsafeCell` — checked by
   `a_redblue_value_has_no_interior_mutability_and_no_back_reference`, which
   reads all twelve ways one could appear off `src/value.rs`.
3. To fill a slot after the fact you need a `&mut` into the payload, and
   `Arc::get_mut` hands one out only at strong count one. But a self-reference
   *is* a handle, so taking one raises the count to two. Contradiction.

This is not a gap in `Arc`; it is `Arc` working. It also means **no safe Rust at
all** can build a self-referential `Arc<Vec<Value>>` or `Arc<Fields>`, which is
why the cycle test below has to supply the one missing capability — a `Mutex` —
by hand. That is a fact about the construction, not a shortcut around it.

### Every write is covered

| Write | Why it cannot close a cycle |
|---|---|
| `Statement::SetProperty` / `SET_PROPERTY` (`src/interpreter.rs:1626`, `src/bytecode/vm.rs:2665`) | reads the stored value first, so the storage is held twice at `make_mut` and it copies |
| `push` (`src/runtime.rs:1211-1224`) | `items.clone()` first, so it always copies |
| `append` (`src/value.rs:303-311` via `src/runtime.rs:277-299`) | writes through only when the binding holds the last handle, which is exactly when nothing in the payload can point back at it |

There are no others: no `SetIndex` statement or opcode, and no mutating builtin
but `push` and `append`. `Value::Object` is inert — an `object` declaration binds
a `Record` (`src/interpreter.rs:2523`), so no program reaches the variant a
property write would have to handle.

`captured: Arc<Captured>` is ordered the same way: `make_function` snapshots the
scopes *before* it creates the `Arc`, so a closure cannot capture itself.

### The empirical half

A counting global allocator was put in front of seven candidate programs — the
self-referential record, a 200,000-element list, a closure capturing it, an
`object` declaration, a `try`/`catch` around unbounded recursion, a function
returning a large list — and every one of them returned to **0 bytes** above its
baseline after the VM and its result were dropped. Nothing in `src/` holds a
`Value` in a `static`, a `thread_local!` or a `OnceLock`; the only process-wide
stores (`vfs::memory::STORE`, `wasm::HOST_INPUT`) hold bytes, not values.

### What round 2 delivers instead

Two tests, one measurement in opposite directions, both in **live bytes**:

| Test | Measured |
|---|---|
| `edge_a_reference_cycle_is_never_reclaimed` | a cycle built from records a real Redblue program made still holds **46,138,670 bytes** after every handle outside it is dropped; dropping the last one releases **0** |
| `edge_a_redblue_program_hands_back_everything_it_allocated` | the same program, run to completion, leaves **4,692 bytes** — under the 1 MiB ceiling |

The cycle's nodes hold `Value`s in `std::sync::Arc` — what `Shared` is — and the
assertions read `Arc::strong_count` over the record's own `Shared<Fields>` and
over the payload list inside it, so a change of representation moves the numbers
the test asserts on. If `Shared` grew a collector, or interior mutability landed,
or a `Weak` appeared, these numbers would change and the suite would go red.

### Recommendation for the auditor

The DoD clause is unsatisfiable as written: satisfying it would require a program
that does not behave as described. `docs/MEMORY.md` now states the limitation,
measures it, gives the shape of the program that would leak, and says under its
own heading — **"Why no Redblue program can build one today"** — that no program
can build it. Two options, both defensible:

1. accept the substitution, since the limitation is now measured rather than
   asserted; or
2. reword the clause to "gives the shape of program that would leak, states that
   no program can build it today, and measures both".

Option 2 is what the document now does.

## 2. `rbops/verify.sh` is not present in this checkout, so the fourth gate could not be run.

```
$ ls rbops
ls: cannot access 'rbops': No such file or directory
$ find . -name 'verify*.sh' -not -path './target/*'
(nothing)
```

The project root contains `.github/`, `src/`, `tests/`, `docs/`, `phases/`,
`bootstrap/`, `corpus/`, `modules/`, `examples/`, `tooling/`, `wasm/` and the
top-level `*.md` files, but no `rbops/`. Unchanged from round 1.

The three gates that *could* be run were run and are reported exactly as they
printed in `REPORT.md`: `cargo fmt --all -- --check` clean, `cargo clippy
--all-targets -- -D warnings` exit 0, `cargo test --all-targets` 1799 passed /
0 failed / 0 ignored. The `must_touch: ["docs/ or README.md"]` requirement is
satisfied: `docs/MEMORY.md` (new) and `README.md` both changed. If the gate
enforces a `must_touch` path spelling that `docs/MEMORY.md` does not match, that
is a manifest bug worth correcting, since the phase prompt names `MEMORY.md` as
the artifact to write.

## 3. A round-1 gate that could not fail is now one that does.

`no_cycle_collector_exists_anywhere_in_the_crate` forbade `Weak`, `WeakRef`,
`Gc`, `gc::` and `weak_count`, and permitted `downgrade`/`upgrade`/
`strong_count`/`ptr_eq`. A collector written against `Arc` needs no `Weak` *type*
— only `Arc::downgrade` to make one — so it would have landed without a word
appearing in the forbidden list. Verified by building exactly that mutation:

```rust
pub fn label(&self) -> &'static str {
    let _back = std::sync::Arc::downgrade(&std::sync::Arc::new(0u8));
    let _ = _back.strong_count();
    ...
}
```

The round-1 test passed it. The round-2 test fails it:

```
src/error.rs mentions `::downgrade`: there is no cycle collector, so nothing
in the crate may reach for one
```

The forbidden list now covers `::downgrade`, `::upgrade`, `strong_count`,
`weak_count` and `ptr_eq` over every `.rs` under `src/`, and `Cargo.toml` is
scanned for `gc`, `gc-arena`, `weak`, `crossbeam-epoch`, `shred`, `triomphe` and
`munge` — a tracing or epoch collector would add collection without touching
`src/` at all.

The same reasoning produced
`a_redblue_value_has_no_interior_mutability_and_no_back_reference`, which is the
guard that makes the round-1 claim about `set node.self to node` a property of
the representation rather than of one code path: if interior mutability or a
back-reference landed in `src/value.rs`, a program could close a cycle and the
limitation in `MEMORY.md` would be quietly untrue.
