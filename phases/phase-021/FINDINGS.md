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

## 3. The bytecode VM cannot run the compiler it compiles (blocks S3)

**Severity:** major. **Where:** `src/bytecode/vm.rs:963` (`pop_n`).

`rb vm` over `bootstrap/compiler.rb`'s own stage-1 output fails partway through,
on a program stage 1 compiles and the tree-walker runs:

```redblue
$ rb compile bootstrap/compiler.rb -o target/tmp/rb021/stage1.rbc
$ rb vm target/tmp/rb021/stage1.rbc target/tmp/rb021/s3/in.rb out.rbc
Error: RuntimeError: bytecode asked for 2 values its frame never pushed
  --> 2176:1
$ echo $?
1
```

`bootstrap/compiler.rb:2176` is a `push(…, {o: …, a: …, x: …, l: …})` — a
4-field record literal, so `BUILD_RECORD 4`. The disassembly of the failing
block shows the record built on the operand stack and `pop_n(8)` reached for
more than the frame had.

**This is pre-existing and unrelated to the argument fix in this phase.** It
reproduces with `src/lib.rs` reverted to `HEAD` (verified by stashing), so it is
not a regression from the `rb vm` arm.

**Not reduced to a minimal program.** Ten candidate reductions were tried —
`BUILD_RECORD 4` in a `while` in a function, a 4-field record built from a
`CALL` result, a record passed through two nested calls, a 4-field record as an
argument to a 1-arg function, `push(list, {4 fields})` in a loop, and the
literal line-2176 shape with the same names. Every one of them ran correctly.
The bug therefore needs something in the 2 748-line program beyond the
constructs above, and the reducer that would find it does not exist yet — it is
`tests/common/shrink.rs`, which shrinks a failing *program* for the tree-walker
differential and is not wired to a bytecode-VM failure.

**Why it matters for the ladder.** S3 is "the S2 compiler compiled by itself,
byte-identical output". It cannot start: stage 1's output of `compiler.rb` does
not run, so there is no stage-2 compiler to compare against. The `rb vm`
argument channel this phase added is what makes the invocation expressible at
all — before it, `rb vm a.rbc b c d` printed the help text and exited **0**.
The next phase on this rung has to fix this first.

**Suggested acceptance gate.** A `#[test]` that compiles `bootstrap/compiler.rb`
with stage 1, runs the `.rbc` under `rb vm` with a small input, and asserts it
either produces the byte-identical `.rbc` or fails on a *named* construct. Once
`shrink.rs` can reduce it, pin the reduction.

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
