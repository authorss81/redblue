# Phase 086 — findings

## F1 — `rb compile` refused a function literal — **FIXED**

**Severity:** blocker (as reviewed). Fixed in the round-1 review pass; the gap is
closed rather than carried forward.

`SPEC.md:647`, the headline of "### First-Class Functions", is:

```redblue
set double to to (x) give back x * 2

set numbers to [1, 2, 3]
set doubled to numbers.map(double)
```

`rb run` printed `[2, 4, 6]`. `rb compile` on the same file stopped before
compiling anything:

```
ParserError: `to (x) ... end` as an expression is not compiled yet; run programs that use one with `rb run`
```

The block form of the same declaration compiled and ran identically, and
`tests/method_map_test.rs` used **that** form for its compiled half — so the gate
was checking a different program from the one SPEC.md prints, and the gate in
`tests/doc_blocks_test.rs` ran only `rb run`, so nothing in the suite could see
the refusal. `SPEC.md:647` sat in `tests/doc_blocks_runs.txt` while `rb compile`
could not build it.

**What changed.** `src/bytecode/codegen.rs` lowers `Expr::FunctionLiteral`
instead of refusing it. A literal is a declaration written where a value goes:
it compiles to a nested `BlockKind::Function` block — named
`interpreter::ANONYMOUS_FUNCTION`, with the literal's parameter names and body —
and emits `DEF_FUNCTION`, which pushes the closure. The `STORE` that follows
`DEF_FUNCTION` in the *statement* path is the only thing that popped it; a
literal has no name to bind, so nothing follows and the closure is left on the
stack as the value of the enclosing expression. `Compiler::expr` now takes the
enclosing block's `blocks` and `depth`, which is the whole of the change: every
other expression in the grammar is a value already and needs neither.

Both spellings SPEC.md gives a literal — the one-line `to (x) give back x * 2`
and the block closed by its own `end` — arrive as this one node, so both compile.

**Verified on the built binary**, SPEC.md:647 verbatim:

```
$ rb run spec647.rb
[2, 4, 6]
$ rb compile spec647.rb -o spec647.rbc && rb vm spec647.rbc
Compiled spec647.rb -> spec647.rbc
[2, 4, 6]
```

**The gate that could not see it now can.** `tests/doc_blocks_test.rs` gained
`compile_block`, and the RUNS bucket is checked with it: a RUNS block has to
exit 0 under `rb run` **and** exit 0 under `rb compile`. All 49 RUNS blocks
compile today, so nothing moved between buckets; what changed is that the next
block that cannot be compiled fails this suite instead of being listed as
working documentation.

**One thing this exposed, and did not fix.** Making a literal reachable on the
bytecode VM made a new program reachable, and it disagreed with the other engine
on one message: a call-depth failure inside a *recursive* literal. The
tree-walking VM reports the call-site name (`while calling 'loop'`, from
`Vm::call_user_function`, which is handed the name the source spelled); the
bytecode VM reported `function.name`, which for a literal is the anonymous name.
`BytecodeVm::call_function` now takes `called_as` and reports that, so the two
engines say the same thing. For every named declaration the two names are the
same string, so no existing message changed.

## F2 — the `many` arm of `map_function` had no test on either engine — **FIXED**

**Severity:** major (as reviewed).

`pub fn map_function(args: &[Value])` has four arms. `tests/method_map_test.rs`
asserted two of them — `xs.map(5)` for `[one]`, and `xs.map()` for `[]` — and
nothing anywhere called `xs.map(f, g)`. The `many` arm was a branch no program
in the suite could reach, on either engine.

`edge_a_map_given_a_second_function_is_refused_the_same_way_by_every_spelling`
now asks all three spellings (`xs.map(f, g)`, `map(xs, f, g)`,
`list.map(xs, f, g)`) on both engines, plus three arguments so the count is read
rather than matched against a fixed `2`, plus a non-list receiver to show the
count belongs to the function side and not to the receiver.

## F3 — one operation, two spellings, two errors — **FIXED**

**Severity:** minor (as reviewed).

The flat spelling (`map(xs, f, extra)`, and `list.map(xs, f, extra)`) refused
as `map requires a list and a function` — a message that names neither the
extra argument nor its count, and that is *false* when the list was supplied.
The method spelling (`xs.map(f, extra)`) said
`map requires a function, but was given 2 arguments`. Same operation, same
mistake, two different sentences.

`Vm::map_builtin` and `BytecodeVm::call_named` now destructure the list and hand
the **rest** to `stdlib::map_function` — the one function both engines already
shared — so all four spellings agree:

| call | both engines |
|---|---|
| `map(xs)` / `xs.map()` / `list.map(xs)` | `map requires a function, but no argument was given` |
| `map(xs, 5)` / `xs.map(5)` | `map requires a function, but was given 5` |
| `map(xs, f, g)` / `xs.map(f, g)` / `list.map(xs, f, g)` | `map requires a function, but was given 2 arguments` |
| `map(5, f)` / `xs.map(f, g)` with `xs` bound to a number | `map requires a list and a function` |

Only a **first** argument that is not a list still says
`map requires a list and a function`, because that is the receiver and no
argument check could describe it.

One existing assertion changed with it:
`tests/function_literal_test.rs::edge_calling_map_without_a_function_fails`
recorded `map([1, 2, 3])` as `map requires a list and a function`. That call
*does* supply a list, so the old message was the misleading half; it now asserts
the unified wording, `map requires a function, but no argument was given`. The
test is not deleted or weakened — it still asserts the exact message on both
engines, and the message is now the one the method spelling gives for the same
mistake.
