# phase-019 — FINDINGS

Work that this phase found, verified, and deliberately did **not** do. Every item
below was reproduced on this tree before being written down; the command and the
output are given so the auditor can re-run it without taking this file's word.

None of these are caused by the bytecode VM. Each is a property of the language
or of `src/value.rs` that both VMs share, which is exactly why fixing them here
would have broken the one thing this phase is for: the two VMs agreeing.

---

## 1. `break` and `skip` are no-ops in both VMs

`src/vm.rs:529-536`:

```rust
Statement::Break => {
    // TODO: Implement proper control flow
    Ok(Value::Nothing)
}
Statement::Skip => {
    // TODO: Implement proper control flow
    Ok(Value::Nothing)
}
```

`src/bytecode/vm.rs:1430` (`break_loop`) and `src/bytecode/vm.rs:1436`
(`skip_loop`) deliberately do the same, with a comment saying the two VMs
disagreeing is worse than the feature being unfinished.

Reproduced:

```
$ printf 'set n to 0\nfor each v in [1, 2, 3]\n    if v is 2 then\n        break\n    end\n    set n to n + v\nend\nsay n\n' > /tmp/x.rb
$ rb run /tmp/x.rb
5
```

`break` does not stop the loop, so the answer is 5 (every element) and not 1.

`docs/GRAMMAR.md` and `SPEC.md` both document `break` and `skip`, so this is
spec drift in the *other* direction: the language promises what neither VM
delivers. **Both VMs agree**, which is why the differential corpus is green —
the corpus pins the agreement, not the feature.

**Why not fixed here:** implementing `break` means changing what a program
*means*. That is a language-design change and needs its own phase. Doing it here
would also have invalidated the differential corpus this phase exists to build.

## 2. Deeply nested data overflows the machine stack on both VMs

`src/value.rs:169` — `Value::List(Vec<Value>)`, `Value::Record`, and the derived
`Clone` at `src/value.rs:168`. Cloning or dropping an `n`-deep list recurses
`n` frames on the native stack. Neither VM recurses in its own loop — bytecode
frames live in `BytecodeVm::frames` on the heap — so this is `Value`'s recursion,
not either VM's.

Measured in one process, one thread, one 2 MiB test-harness stack:

| nesting depth | tree-walking VM | bytecode VM |
|---|---|---|
| 8000 | ok | ok *(given a 64 MiB stack)* |
| 14000 | ok | stack overflow |
| ~1900 | ok | stack overflow |

Both VMs survive 8000 when the thread stack is raised to 64 MiB, so the depth at
which it happens is a function of how much machine stack is available, not a
fixed language limit. The bytecode VM needs roughly 7x more stack per level than
the tree-walker, so it reaches the wall at a shallower value.

Pinned by `edge_both_vms_answer_the_same_at_a_nesting_depth_neither_overflows` in
`tests/bytecode_vm_test.rs`, which asserts both VMs answer identically at depth
400 — deep enough to be interesting, shallow enough that neither overflows.

**Why not fixed here:** making either VM survive arbitrary nesting means an
iterative (or refcounted) `Clone`/`Drop` for `Value`, which is a change to a
public API type the invariants table protects. One VM gaining it alone would make
the two VMs *disagree* about a program that today both run.

## 3. `>`, `<`, `>=`, `<=`, `is greater than` and `is less than` are documented but not lexable

`docs/GRAMMAR.md:95-98`:

```
lt_op        = 'is less than' | '<'
lte_op       = 'is less than or equal to' | '<='
gt_op        = 'is greater than' | '>'
gte_op       = 'is greater than or equal to' | '>='
```

```
$ printf 'say 1 > 2\n' > /tmp/a.rb && rb run /tmp/a.rb
Error: LexerError: Unexpected character '>'
$ printf 'say 1 is greater than 2\n' > /tmp/b.rb && rb run /tmp/b.rb
Error: AnalyzerError: Unknown variable 'greater'
```

The grammar lists them; the lexer has no token for any of the four symbols, and
the two word forms parse as two variables. Every comparison in `examples/` and
`modules/` is written `is` / `is not`, which is why nothing has noticed.

**Why not fixed here:** adding comparison operators changes the grammar, which
the phase constraints forbid without a phase that opens the design.

## 4. Top-level `constant X to N` does not parse, so a shipped module is unrunnable

`modules/MathUtils.rb:4`:

```
$ rb run modules/MathUtils.rb
Error: ParserError: Expected function name
  --> modules/MathUtils.rb:4:16
4 | constant PI to 3.14159
  |                ^
```

`constant` is only accepted inside a `module` body. A top-level `constant`
statement — which `SPEC.md:191-192` shows — does not parse. `modules/MathUtils.rb`
is therefore dead weight: it cannot be run, imported, or compiled.

Both VMs fail identically (the failure is in the frontend, before either VM), so
the differential corpus is green on it.

## 5. `stdlib::builtin_function` is never called

`src/stdlib.rs:210`. `src/stdlib.rs:14` registers the names, but nothing
dispatches to `builtin_function` — the only builtin entry point is
`src/runtime.rs:248`, which does not implement them.

```
$ printf 'say abs(-3)\n' > /tmp/c.rb && rb run /tmp/c.rb
Error: RuntimeError: Unknown function 'abs'
```

So `abs`, `floor`, `ceil`, `round`, `sqrt`, `pow`, `sin`, `cos`, `tan`, `log`,
`exp`, `uppercase`, `lowercase`, `trim`, `split`, `join`, `contains`,
`starts_with`, `ends_with`, `replace`, `push`, `pop`, `shift`, `map`, `filter`,
`reduce`, `is_number`, `is_text`, `is_list`, `is_record`, `to_text`,
`to_number`, `to_list`, `PI` and `E` are all registered as globals and all fail
with `Unknown function`. The working builtins are the ones `src/runtime.rs`
handles: `say`, `length`/`len`, `input`/`ask`, `random`, `files_*`, `time_*`,
`json_*`, `csv_*`, `network_*`, `expect`/`assert`, `console_*`, `random_*`,
`type_of`.

This restates finding 1 of `phases/phase-007/FINDINGS.md`, which is still open.
**Why not fixed here:** it would change what programs mean, on both VMs at once,
and belongs to the phase that owns the standard library.

## 6. `import MathUtils` does not bind the module name

```
$ printf 'import MathUtils\nsay MathUtils.circle_area(2)\n' > /tmp/d.rb && rb run /tmp/d.rb
Error: AnalyzerError: Unknown variable 'MathUtils'
```

`SPEC.md` and `AGENTS.md` both document `import MathUtils` followed by
`MathUtils.circle_area(5)`. The import runs but binds nothing the analyzer can
see, so the documented spelling does not work. The bytecode VM's `globals_only`
frame handling (`src/bytecode/vm.rs:267`) exists for exactly this case, so it will
work the day the frontend binds the name.

## 7. `rb vm` renders a different error position than `rb run` — a deliberate boundary

`tests/test_objects.rb` run both ways:

```
$ rb run tests/test_objects.rb
Error: RuntimeError: Object 'Base' is already declared
  --> tests/test_objects.rb:57:5
57 |     object Base
   |     ^

$ rb vm /tmp/test_objects.rbc
Error: RuntimeError: Object 'Base' is already declared
  --> 57:1
```

Same kind, same message, different rendering: a `.rbc` carries no source text to
render a caret under. `tests/bytecode_vm_test.rs` compares `error.label()` and
`error.message()` and documents why the position is excluded — so this is a
stated comparison boundary, not an unnoticed divergence. Recorded here so a
reviewer does not read it as one.

## 8. Two programs read from disk cannot be compared

`examples/time.rb` and `examples/random.rb` read the wall clock and a random
source. They are named in `NOT_COMPARABLE` in `tests/bytecode_vm_test.rs` with
the reason. They are still run by `tests/redblue_suite_test.rs`, so nothing stops
covering them — a differential *comparison* of a program whose output is a
timestamp is meaningless, and AGENTS.md §3.1.4 requires determinism.

Recorded because a corpus that quietly drops files is worse than one that names
them, and the names should be visible to whoever reads the phase.
