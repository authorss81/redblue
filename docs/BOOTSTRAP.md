# Bootstrapping Redblue: the ladder

**Redblue compiles Redblue.** This document is what that means, which rung the
project is on, and the one rule every rung is measured by.

The format a compiled file has is described in [BYTECODE.md](BYTECODE.md). This
is about who writes it.

## The ladder

| Stage | What exists | Definition of done |
|---|---|---|
| **S0** | The Rust `rb`: a tree-walking interpreter | `cargo test` is green and `rb run` runs every `examples/*.rb` |
| **S1** | `rb compile` — a bytecode compiler written in Rust | it emits `.rbc`, and `rb vm file.rbc` runs it |
| **S2** | `bootstrap/compiler.rb` — the same compiler written in Redblue | `rb run bootstrap/compiler.rb in.rb out.rbc` emits the same bytes as `rb compile` does |
| **S3** | **done** — the compiler, compiled by itself | `stage1.rbc` and `stage2.rbc` are byte-identical |
| **S4** | The shipped `rb` is built by `rb` | not started |

Two names are used throughout, and they are always the same two files:

- **stage 1** is `bootstrap/compiler.rb` compiled by the Rust frontend:
  `rb compile bootstrap/compiler.rb -o stage1.rbc`. The compiler, running on a
  machine it cannot itself produce.
- **stage 2** is what that file writes when it is given its own source:
  `rb vm stage1.rbc bootstrap/compiler.rb stage2.rbc`. The same compiler, its
  Redblue source compiled to bytecode, executed by `rb vm`.

## The one rule

> **`stage1.rbc` and `stage2.rbc` are the same file, byte for byte.**

There is no weaker version of this that counts. "The output is a valid `.rbc`"
is satisfied by a compiler that writes nonsense; "the output is plausible" is
satisfied by one that is *almost* right. The claim S3 makes is that compiling
`bootstrap/compiler.rb` with a compiler that was itself compiled from
`bootstrap/compiler.rb` produces exactly the bytes that compiling it with the
Rust frontend does — so the file contains nothing that only the Rust build knows,
and nothing that is a coincidence of this one input.

No shimming is permitted on the way to it. There is no Rust fast path that a
Redblue program silently falls back to: if `bootstrap/compiler.rb` does not do
something, then stage 2 does not do it either, and the comparison above says so.
And the language is not changed to make self-hosting easier — a grammar change
needed for S2 or S3 has to be justified on its own merits, in its own phase, and
is not part of this ladder.

## Why S3 is the hard one

S2 is a claim about the compiler agreeing with the Rust frontend on other
programs. `tests/bootstrap_selfhost_test.rs` checks that over the whole
`corpus/`, over `examples/` and `modules/`, and over a list of statement shapes
the corpus families do not all reach — hundreds of programs, byte for byte.

S3 is a claim about the compiler agreeing with the Rust frontend on **itself**.
That is one program instead of hundreds, and it is much harder for three reasons.

**It is the only input with the compiler's own shape.** Every corpus program is
small enough that a mistake in the code generator lands somewhere the tests
already look. `bootstrap/compiler.rb` is 2 800 lines of deeply nested
if/else chains and loops within loops, so it exercises code paths nothing else
does. Two of its bugs were found only here and are named below.

**It is the only input that pays the whole cost at once.** Lexing 91 kB,
parsing it, walking a parse tree with tens of thousands of nodes, and encoding
162 kB of output — every stage at full size, in one run, three times over for
the determinism check.

**It is bounded by the same step budget as any other program.** The guard in
`src/interpreter.rs` that stops a runaway program stops the compiler too, and
the default had to be large enough to let it finish. It is 32 000 000 steps
today; one self-compilation takes about 16 000 000 of them.

## What the fixed point has already caught

Two defects in `bootstrap/compiler.rb` were invisible to every S2 test and
were found by compiling the compiler:

1. **A `while` inside a `while`.** `compile_while` kept the slot of its own
   `JumpIfFalse` in a name across the call that compiled its body. A Redblue
   function's variable names are program-wide, so an inner `while` took the
   outer loop's slot and patched the wrong jump — the outer loop then pointed at
   the end of its block instead of at the instruction after itself.
   `lex_text` has exactly that shape. `edge_a_while_loop_nested_in_another_patches_its_own_jump`
   pins it now, with loops nested two and three deep.

2. **Encoding was quadratic in the size of the file.** The `.rbc` was built one
   `cat` at a time, and `cat` copies the list it is given, so a file of `n`
   bytes cost `n` copies of a list that grew to `n` elements. Compiling the
   compiler means encoding 162 139 bytes, and that alone took **most of six
   minutes**. The encoder now writes through `append`, which grows a named list
   in place. One self-compilation is about 14 seconds in a release build and
   about 2.5 minutes in the test build `cargo test` uses.

The second one is worth stating as a rule, because it is the shape more of the
compiler is likely to grow into: **`push` copies.** Anywhere a Redblue program
builds a list that grows to the size of its input, `set x to push(x, v)` is
quadratic and `append("x", v)` is not. `G_POOL`, `G_INTERN`, `G_CODE`, `G_JUMPS`,
`G_BLOCKS`, `G_TOKENS`, `G_PATCHED` and `G_BYTES` are all named globals for that
reason.

## What checks it

Everything below runs in `cargo test`, so it runs on every push:

| Test | What it holds |
|---|---|
| `edge_the_self_hosted_compiler_compiles_itself_byte_identically` | `stage1.rbc == stage2.rbc`, and stage 2's file decodes to the same chunk |
| `edge_three_consecutive_self_compilations_are_byte_identical` | three consecutive runs, one file |
| `edge_the_self_compilation_still_obeys_the_step_budget` | a run that outlasts its step budget fails, says so, and writes nothing |
| `edge_a_while_loop_nested_in_another_patches_its_own_jump` | the nested-`while` patch, on its own, without needing the self-compilation to find it again |
| `edge_stage3_is_byte_identical_on_every_corpus_program` | stage 2 == stage 1 for all 306 corpus programs the frontend accepts, through `rb vm` |
| `edge_stage3_is_byte_identical_on_the_awkward_shapes` | the statement shapes the corpus families do not all reach |
| `edge_stage3_refuses_every_malformed_corpus_program` | all 46 refusals, through `rb vm` |
| `stage2_is_byte_identical_on_its_corpus_families` | the same corpus, through the tree-walker |
| `stage2_is_byte_identical_on_the_examples_and_the_modules` | the language's specification by example |

## Running it by hand

```bash
# stage 1: the Rust frontend compiles the compiler
cargo run --release -- compile bootstrap/compiler.rb -o stage1.rbc

# stage 2: that file compiles itself
cargo run --release -- vm stage1.rbc bootstrap/compiler.rb stage2.rbc

# the fixed point
cmp stage1.rbc stage2.rbc && echo fixed point
```

Useful while working on the compiler:

```bash
# disassemble either file
rb dis stage1.rbc > /tmp/s1.txt
rb dis stage2.rbc > /tmp/s2.txt
diff /tmp/s1.txt /tmp/s2.txt
```

A difference in the disassembly names the instruction that disagrees, and the
`: line N:` annotation on every line says which line of `bootstrap/compiler.rb`
produced it — which is usually enough to find the construct that was compiled
wrong without a debugger.

If a run is refused with `Step budget of ... reached before the program
finished`, the compiler has outgrown the default in `MAX_STEPS`. Raise the
constant, and the measurement that justifies it, in the same commit. To watch a
run's cost without editing anything:

```bash
REDBLUE_MAX_STEPS=200000000 rb vm stage1.rbc bootstrap/compiler.rb stage2.rbc
```

## Determinism

`stage2.rbc` is byte-identical across runs because the compiler never depends on
an iteration order it does not control:

- the constant pool is indexed by position (`length(G_POOL)`), never walked to
  decide what comes next;
- the intern table is a linear scan that returns the **first** match, so two
  identical names cannot produce two entries in different orders;
- the block tree is written in the order the parser built it;
- no clock, no random source, and no `HashMap` order reaches the output.

That is a claim about the compiler, so it is tested rather than asserted:
`edge_three_consecutive_self_compilations_are_byte_identical` runs the whole
self-compilation three times and compares the three files.