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