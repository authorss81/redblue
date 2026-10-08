# phase-022 — FINDINGS

**This phase is BLOCKED.** S3 is not reachable without a language change that
phase-022's PROMPT does not authorise. Everything below was measured in this run
on this machine. Nothing here is estimated except where it says so.

---

## 1. The finding reproduces: stage 1 cannot compile the compiler in usable time

The evidence line is `S2 exists but nothing proves it can build itself.` It
reproduces, and the reason it does not exist is not that nobody wrote the check —
it is that the check cannot run.

The self-compilation, under the unmodified `main`:

| Input to `rb run bootstrap/compiler.rb <in> <out>` | Wall clock |
|---|---|
| 100 statements | 2.76 s |
| 200 statements | 9.08 s |
| 400 statements | **34.78 s** |

Ratios 3.29× and 3.83× per doubling of the input: **n^1.94**. `bootstrap/compiler.rb`
is 2 748 lines, which is 2.78 doublings past the 400-statement point;
extrapolating the measured 3.83× gives **≈ 24 minutes for one self-compilation**.

That is the number that blocks the ladder, and it is not a red test — it is a
test that cannot return. `edge_the_self_hosted_compiler_compiles_itself_byte_identically`
was written and run first, as the phase requires, under
`cargo test --test bootstrap_selfhost_test`. It was killed by a **900 s** wall
clock with no output: 900 s is 38× the 400-statement measurement and the run was
still going. A test that never returns is worse than a red gate, because a suite
that never finishes cannot be read at all — which is the same reason
`phases/phase-021/FINDINGS.md` §3c gave for removing its own copy of this test.

---

## 2. What I implemented, measured, and then reverted

I did not leave this at "it is slow". The hypothesis was the one in
`phases/phase-021/FINDINGS.md` §3b: the cost is `push`'s deep copy, so make the
accumulator cheap. I implemented the smallest version of that which is *provably
semantics-preserving*, and measured it.

**The change** (all four files reverted; the tree is clean):

- `src/bytecode/opcode.rs` — a new opcode `AppendLocal` (byte 49, `FORMAT_VERSION`
  5 → 6) that pops a value and appends it **in place** to the list held by the
  variable named by its operand.
- `src/bytecode/codegen.rs` — `set NAME to push(NAME, <expr>)` compiles to
  `APPEND_LOCAL` instead of `LOAD` / `PUSH_CONST` / `CALL` / `STORE`.
- `src/bytecode/vm.rs` — `append_local`, resolving the name exactly as `get_var`
  does and refusing with `push`'s own two messages, word for word.
- `tests/bootstrap_selfhost_test.rs` — the fixed-point test.

**Why it is sound, which is why it was worth trying.** A Redblue list is never
shared: every copy of one is a deep copy, so a variable holds the only handle to
its list and appending through that handle is unobservable from anywhere else.
`push` itself is untouched. Nothing about the language's meaning changes; only
the cost of one shape of statement changes.

**What it bought.** `bootstrap/compiler.rb` has 30 `push` call sites; 17 of them
are the `set NAME to push(NAME, x)` shape and became `APPEND_LOCAL`. That includes
the single largest accumulator in the compiler — the lexer's token list
(`bootstrap/compiler.rb:394`), roughly 30 000 records for a 2 748-line input —
which went from quadratic to linear. The self-compilation still **did not finish
in 900 s**.

**Why it was reverted anyway.** It bumps the bytecode format version and adds an
instruction, and it does not deliver any of the four boxes in the definition of
done. Shipping a format change in a phase that does not meet its DoD is the kind
of half-measure §5 calls fake completion. It is a real optimisation and belongs
in the phase that makes the accumulator cheap — after that phase exists it is
about twenty lines. Reverted, tree clean.

---

## 3. The blocker, measured: it is not `push`. It is that Redblue has no shared mutable value

`APPEND_LOCAL` made every accumulator that a loop appends to *linear*. The
remaining quadratic is a different copy, in a different place, and no change to
`push` can reach it.

Redblue's codegen threads one immutable record through every call —
`bootstrap/compiler.rb:2213`:

```redblue
to emit_const(ctx, tag, raw)
    set emit_const_index to length(ctx.p)
    set emit_const_pool to push(ctx.p, {t: tag, b: raw})      // the pool grows here
    give back {p: emit_const_pool, i: ctx.i, c: ctx.c, j: ctx.j, b: ctx.b}
end
```

`ctx` is a **parameter**, so every call copies it: the constant pool `p`, the
intern table `i`, the code list `c` (one record per emitted instruction) and the
jump list `j`. For `bootstrap/compiler.rb` itself that is a pool of ~15 000 and a
code list of ~30 000 records, **copied once per constant and once per instruction**.

Measured on this machine, with `APPEND_LOCAL` in place, timing `emit_const`
called *n* times and nothing else (`target/tmp/s3/ctxcost.rb`):

| calls | wall clock | ratio |
|---|---|---|
| 400 | 0.76 s | — |
| 800 | 3.05 s | 4.01× |
| 1600 | 11.96 s | 3.92× |

**n^1.97.** Extrapolating the measured 3.92× from 1 600 to the ~15 000 entries of
`bootstrap/compiler.rb`'s own pool: 11.96 s × ~122 = **≈ 24 minutes for the
constant pool alone**, before the code list is counted, and the code list is
larger.

The same shape appears in the parser, where accumulators are threaded through
tail recursion (`bootstrap/compiler.rb:812`, `:1181`, `:1448`, `:1907`, `:2025`,
`:2056`) — there the copy is the argument, plus the copy on the way back.

**The root cause is one fact.** `Value::List(Vec<Value>)` and
`Value::Record(Fields)` own their contents, and every read of a variable clones
it (`src/interpreter.rs:927`, `src/bytecode/vm.rs:979`). A self-hosted compiler
needs to *share* a growing buffer across function calls. Redblue cannot express
that. `phases/phase-021/FINDINGS.md` §3b reached this conclusion and proposed two
routes; §3d then showed that the obvious `Rc` route (`Rc::make_mut` in `push`)
does not work, because the argument is cloned out of the environment before the
call and the refcount is already 2.

---

## 4. What is required, precisely

One of these. All three are language changes.

**Option A — a shared mutable buffer (smallest).** A value the language can hand
to a callee without copying and that callee can append to:

```redblue
set b to buffer()
append(b, 1)          // O(1), and the callee sees it
set xs to to_list(b)  // one copy, at the end
```

Cost: interior mutability in a `Value` payload. AGENTS.md §2 lists the `Value`
variants as an invariant and `redblue::Value` as exported, so **this phase
cannot do it** — the PROMPT does not re-open the language design. It needs its
own phase with its own conformance work: `push`/`append` interaction, aliasing,
what `say`, `==` and `type_of` report for a buffer, and what happens when a
buffer is captured by a closure.

**Option B — records and/or lists become copy-on-write (`Rc` with `make_mut`).**
Cheaper to add and it does not add a value kind, but it changes what `push` costs
*and* turns every record into shared state, so `set r.field to x` (`SET_PROPERTY`,
`src/interpreter.rs`) stops meaning "a new record". `phases/phase-021/FINDINGS.md`
§3d explains why `Rc` alone leaves the `set xs to push(xs, v)` loop quadratic:
`Rc::make_mut` sees the environment's own copy and takes the copying branch. It
needs the argument to reach the callee *moved*, not cloned — which means the
variable read has to stop cloning, which is §4's real shape underneath both
options.

**Option C — do not make Redblue fast; make the compiler not need sharing.**
Rewrite `bootstrap/compiler.rb`'s codegen so the pool, the code list and the jump
list live in one frame that the emit loop mutates, and the parser's eight
tail-recursive accumulators into `while` loops over a local. This needs Option A
or B to exist first (an in-place append to a *named* target is not expressible
today), and it is a rewrite of ~1 500 lines of a compiler whose byte output is
the phase-021 test suite.

**Suggested acceptance gate for whichever phase takes it** — not a timing test,
which no machine can hold:

1. A `#[test]` that appends 50 000 times through the new form and asserts the
   result's length, with an **iteration counter** in the test that fails if the
   append path ever becomes a copy again. The count, not the clock: this is the
   gate that `phases/phase-021/FINDINGS.md` §3d asked for and that a type change
   alone would pass while staying quadratic.
2. `edge_append_does_not_alias` — a buffer bound to two names and appended
   through one: the two must share, or not, and whichever it is, it must be
   pinned.
3. The existing `edge_push_does_not_alias_its_argument` family, unchanged, so
   `push`'s copy semantics are proven not to have moved.
4. Only then the fixed point: `edge_the_self_hosted_compiler_compiles_itself_byte_identically`
   plus the three-consecutive-runs determinism check.

---

## 5. What this phase did not verify, and why

- **Not** that stage 1 and stage 2 ever agree about `bootstrap/compiler.rb`. The
  comparison cannot be made, so it is not claimed in either direction. The
  S2 agreement over the corpus is unaffected and still holds — it is
  `phases/phase-021`'s `edge_stage3_is_byte_identical_on_every_corpus_program`
  and this run did not touch it.
- **Not** determinism of a fixed point. Determinism of the *smaller* runs is
  unaffected: three runs of `rb run bootstrap/compiler.rb` over the 400-statement
  input produce identical bytes, because the nondeterminism AGENTS.md §5 warns
  about is `HashMap` iteration order reaching output, and the compiler indexes
  `ctx.p` and `ctx.i` by position, never by iterating them.
- **Not** anything about `docs/BOOTSTRAP.md`. It does not exist
  (`ls docs/` → `BYTECODE.md`, `GRAMMAR.md`). Writing it to describe a fixed
  point that does not exist would be the fabrication AGENTS.md §3 warns about;
  it belongs in the phase that has one.
- **Not** the `rb vm` argument channel or anything else phase-021 fixed. Untouched.

---

## 6. Smaller things found on the way, not fixed (AGENTS.md §1 rule 7)

- `docs/BYTECODE.md:3` says **"Format version: 4"**. `FORMAT_VERSION` is `5`
  (`src/bytecode/format.rs:75`) and the version table at `docs/BYTECODE.md:73`
  already documents version 5. The header line is stale from phase-020 and was
  never updated. One-line fix; it is documentation drift in another phase's area,
  so it is recorded rather than smuggled in here.
- The default `corpus/` walk in `tests/bootstrap_selfhost_test.rs` compares 306
  files where a reader counting the 15 non-`malformed` families gets 315; nine
  are frontend-refused. Already recorded as `phases/phase-021/FINDINGS.md` §2.
  Unchanged.