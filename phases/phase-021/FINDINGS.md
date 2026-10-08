# phase-021 — FINDINGS

Out-of-scope defects found while doing this phase's work. **None of these were
fixed**, per AGENTS.md §1 rule 7 ("one phase, one concern"). Each is anchored to
a line I read.

---

## 1. `give back` inside an `if` does not return from the function (tree-walker)

**Severity:** major. **Where:** `src/vm.rs:1213`.

**Reproduction** — save as `gb.rb`:

```redblue
to pick(n)
    if n is greater than 10 then
        give back "big"
    else
        give back "small"
    end
end
say pick(1)
```

```
$ ./target/debug/rb run gb.rb
nothing
$ ./target/debug/rb compile gb.rb -o gb.rbc && ./target/debug/rb vm gb.rbc
small
```

The tree-walker prints `nothing`; the bytecode VM prints `small`. **The two
engines disagree on the same source**, which is worse than either answer being
wrong on its own.

**Cause.** `src/vm.rs:1213`:

```rust
Statement::Return(expr) | Statement::GiveBack(expr) => match expr {
    Some(e) => self.evaluate(e),
    None => Ok(Value::Nothing),
},
```

This evaluates the operand and returns it as the **value of the statement**. It
does not unwind the function's frame. So when `give back` is not the last
statement of the body — which is every time it sits inside an `if`, a loop, a
`try` or a nested block — its value is discarded and the function returns
whatever the enclosing block statement evaluated instead. Here that is the
`if`, which is `nothing`.

**The bytecode VM is right** (`src/bytecode/vm.rs` compiles `give back` to
`RETURN`), so `SPEC.md`/`docs/GRAMMAR.md` almost certainly already say a
function returns from its `give back` — meaning this is a tree-walker regression
against the spec, not a spec question. It should be checked against both before
being fixed.

**Suggested acceptance gate.** A `corpus/functions-*` case plus a `#[test]`
asserting `run` and `compile && vm` produce identical output for a function
whose `give back` is inside an `if`. The "both engines agree" assertion is the
strong form and would have caught this.

**Note for the bootstrap ladder:** `bootstrap/compiler.rb` compiles `give back`
to `RETURN` (`Opcode::Return`), matching the bytecode VM, **not** the
tree-walker. So the compiler in `bootstrap/` is right and `src/vm.rs` is wrong.
That is the correct way round, but it means the compiler cannot be validated by
differential testing against `rb run` — only against `rb compile`, which is what
`tests/bootstrap_selfhost_test.rs` does.

---

## 2. Nine corpus programs in non-`malformed` families are frontend-refused

**Severity:** minor (filing, not behaviour). **Where:** `corpus/`.

`corpus/malformed-*.rb` (46 files) is the family the frontend refuses, and
`UNSUPPORTED` in `tests/bootstrap_selfhost_test.rs:397` says so. But nine more
programs are refused too, and they sit in families that otherwise compile:

| File | Refusal |
|---|---|
| `corpus/text-ops-0006.rb` | `LexerError: Unterminated string` |
| `corpus/control-flow-0003.rb` | `ParserError: Expected End but got Eof` |
| `corpus/control-flow-0004.rb` | `ParserError: Expected End but got Eof` |
| `corpus/control-flow-0005.rb` | `ParserError: Expected End but got Eof` |
| `corpus/nesting-0015.rb` | `ParserError: Expected To but got Equal` |
| `corpus/objects-0003.rb` | `AnalyzerError: Unknown variable 'Tagged'` |
| `corpus/objects-0009.rb` | `AnalyzerError: Unknown variable 'value'` |
| `corpus/objects-0014.rb` | `AnalyzerError: Unknown variable 'Inner'` |
| `corpus/objects-0022.rb` | `AnalyzerError: Unknown parent object 'Nothing'` |

They are the same *kind* of program as `malformed` and are arguably misfiled,
which is why the corpus walk compares 306 rather than the 315 files in its 15
families. The walk skips them with a documented `continue`, so nothing is
silently passing — but a reader counting corpus files will get 315 and a test
message that says 306. I corrected that message in this phase and left the
filing alone.

**Suggested fix.** Either move the nine into `malformed-*.rb`, or state in
`corpus/README.md` that a family may contain frontend-refused members. The
second is smaller and keeps the refusal cases near the constructs they exercise.
---

## 3. S3 is blocked by a quadratic in `push`, not by the bytecode VM

**Severity:** major. **Where:** `src/runtime.rs:695` (`push`), and the absence of
any mutable data structure in the language.

### 3a. The defect the earlier run of this phase recorded here is FIXED

The first version of this section said the bytecode VM could not run the compiler
it compiles: `rb vm stage1.rbc in.rb out.rbc` died with `bytecode asked for 2
values its frame never pushed` at `bootstrap/compiler.rb:2176`. That is fixed —
`push_loop` recorded a sequence loop's `stack_base` one below the height its body
ran at (`src/bytecode/vm.rs:1721`), so leaving the loop ate a value the enclosing
frame had pushed. The loop's sequence lives in the `Loop` entry, not on the
operand stack, so the height is what `GET_ITER`/`GET_RANGE` left behind.

Re-verified from scratch in this run, with the compiler running **as bytecode**:

| Check | Result |
|---|---|
| Whole corpus outside `malformed` | **306 programs, 306 byte-identical, 0 differ** |
| 13 hand-written awkward shapes (loops, calls, closures, records, `try`, escapes, `f64` boundaries) | **13 of 13 byte-identical** |
| `corpus/malformed-*.rb` | 43 lexer/parser refusals, all refused with no file; 1 analyzer-only refusal; 2 programs the frontend accepts, both byte-identical |

All three are `cargo test` cases now (`edge_stage3_is_byte_identical_on_every_corpus_program`,
`edge_stage3_is_byte_identical_on_the_awkward_shapes`,
`edge_stage3_refuses_every_malformed_corpus_program`), so the next rung does not
have to re-derive them.

### 3b. What still blocks S3: the compiler cannot compile itself in any useful time

```redblue
$ time rb run  bootstrap/compiler.rb target/tmp/s3/in.rb out.rbc   # 100 lines
  30.17 s
$ time rb run  bootstrap/compiler.rb target/tmp/s3/in.rb out.rbc   # 200 lines
  110.69 s
$ time rb run  bootstrap/compiler.rb target/tmp/s3/in.rb out.rbc   # 400 lines
  430.40 s
```

Growth is ×3.6–3.9 per doubling of the input, i.e. **n^1.9**. `bootstrap/compiler.rb`
is 2 748 lines, which is 6.9 doublings past 400: extrapolating, one self-compilation
is of the order of **10^6 seconds — about 48 days**. `rb vm stage1.rbc` on the same
inputs costs the same to within a few percent, so it is not an engine difference.

**Cause, measured.** `push` clones the list it is given:

```rust
// src/runtime.rs:705
let mut pushed = items.clone();
pushed.push(value.clone());
```

A loop of *n* appends is *n* deep clones of a growing list. Redblue has no mutable
data structure — a `set r.field to x` evaluates `r` by value first — so every
list-building program is quadratic, and the compiler's two hot loops
(`lex` at `bootstrap/compiler.rb:394`, `parse_statements` at
`bootstrap/compiler.rb:812`) are exactly that. Isolated on one machine:

| Loop body | 400 | 800 | 1600 |
|---|---|---|---|
| `push(l, i)` — a number | 0.00 s | 0.02 s | 0.08 s |
| `push(l, {k: "abcdefgh", l: 12345, x: 1.5})` — a record | 0.09 s | 0.36 s | 1.51 s |

Numbers are nearly free; records are quadratic, because cloning a record clones its
map and its text. The compiler's tokens are records (`{k, l}`) and so are its AST
nodes, which is why it is the slow case and not merely the awkward one.

**Why it matters for the ladder.** S3 is "the S2 compiler compiled by itself,
byte-identical output". Its only proof is the fixed point, and the fixed point
needs one self-compilation. So S3 is not reachable until this is fixed, and the fix
is in the language, not in `bootstrap/`.

### 3c. The test that asserted it was removed, and why that is stated here

The previous attempt of this phase added
`edge_stage3_recompiles_the_compiler_byte_identically`, which compiles
`bootstrap/compiler.rb` with stage 2 and compares the bytes. It was **removed** in
this run. It did not fail — it cannot finish: by the numbers above it would run for
weeks inside `cargo test`, which is worse than a red gate because a suite that
never returns cannot be read at all. It was a test this phase added in its own
first attempt, asserting a claim this phase cannot make; it is recorded here rather
than deleted quietly.

What replaced it is the coverage that *is* reachable and *is* in the gate: the whole
corpus through the bytecode path, the awkward shapes, and the whole `malformed`
family. The one thing still unasserted is the self-compilation, and §3b says why.

### 3d. Suggested acceptance gates, either order

1. **Give the callee exclusive ownership of the list it appends to.** Gate: a
   `#[test]` that times nothing but *counts* — `push` a record 10 000 times into a
   list built in a loop, and assert that the loop's own iteration cap is not what
   stops it — plus `edge_push_does_not_alias_its_argument` (a list bound to two
   names and appended through one must not change through the other) and
   `edge_push_of_a_shared_list_copies_rather_than_writing_through`.

   **The obvious spelling of this does not work, and the reason matters enough to
   write down.** The first version of this section proposed
   `Value::List(Vec<Value>)` → `Value::List(Rc<Vec<Value>>)` with `push` as
   `Rc::make_mut`, on the grounds that it "is O(1) while the list has one owner —
   the loop case". The loop case does not have one owner. At the moment `push`
   runs in `set xs to push(xs, v)`, the list is held twice whatever its type:

   - `Expr::Call` builds the argument list by calling `evaluate` on each argument
     (`src/interpreter.rs:1455`), so
   - `args[0]` is evaluated as an `Expr::Identifier`, which resolves through
     `get_var` — `get_var_ref(name).cloned()` (`src/interpreter.rs:926`) — and
     therefore clones the value out of the environment while the environment
     keeps its own copy.

   So `Rc::make_mut` sees a refcount of 2, takes the copying branch, and clones
   the `Vec<Value>` — the same deep clone, element for element, that
   `items.clone()` does today. The quadratic in §3b's table survives that change
   unchanged; only the type changes. `Rc` fixes the case where the list really has
   one owner, which is not the shape a compiler writes. The gate has to be the
   count above, run before and after the change, because the type change alone
   will leave it quadratic and pass any test that only checks behaviour.
2. **A mutable accumulator for the language.** A `buffer` value with `append`,
   `length` and `to_list`, or a list with an in-place append. Gate: the S3 fixed
   point itself, once it can run. Cost: a new stdlib type, i.e. a language feature
   and its own phase.

Option 1 as corrected — moving the value out of the environment for the duration
of the call, which the interpreter can only do if it also knows the assignment
target the call's result is bound to — and option 2 are both language changes.
Neither is this phase's diff, and neither is a `Value` payload change on its own.

## 4. The phase's stated finding no longer reproduces

The evidence line in `phases/phase-021/PROMPT` is `The self-hosted path does not
exist yet.` It does not reproduce. `bootstrap/compiler.rb` is 2 748 lines, runs
under the Rust `rb`, and `cargo test --test bootstrap_selfhost_test` was green
on arrival at 8 passed / 0 failed.

Stage 2 was re-verified independently of that suite before anything was
changed:

| Check | Result |
|---|---|
| Whole corpus, every family | 308 programs stage 1 accepts → **308 byte-identical, 0 differ** |
| `examples/*.rb` + `modules/*.rb` | 8 of 8 byte-identical |
| `tests/*.rb` (large real programs) | 5 of 5 byte-identical |
| 179 `f64` values (random bit patterns + subnormal boundaries) | **179 byte-identical, 0 differ** |
| 19 text/unicode/escape cases | **19 byte-identical, 0 differ** |
| Block nesting at 63 / 64 / 65 levels | 63 and 64 identical; 65 refused by both |
| `rb lint bootstrap/compiler.rb` | exit 0, no output |

**The five corpus programs stage 2 accepts and stage 1 refuses are all
analyzer-stage refusals** (`Unknown variable 'x'`, `extends 'Nothing'`), and
`bootstrap/compiler.rb` implements no analyzer — it is a lexer, parser and code
generator. So there are no bytes to disagree about. Recorded here rather than
claimed as a fix.
