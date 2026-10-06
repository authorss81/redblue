# Phase 031 — FINDINGS

Work discovered while implementing `module ... end`, `export`, and the
`import X as Y` alias that does **not** belong to this phase. Each entry is
file:line anchored so the auditor can promote it.

## 1. The bytecode VM has no module system — `import … as …` and `module … end` do not work under `rb compile` / `rb vm`

**Severity:** major (the two VMs disagree on every module program)
**Evidence:**

- `src/bytecode/codegen.rs:443` — `Statement::Module` compiles its body
  inline; there is no `Opcode::Module`, so a module declaration is not a
  declaration on this VM: the duplicate-declaration refusal, the
  `export`-validation refusal, the module's own scope (`Vm::module_depth`,
  `src/vm.rs:195`) and the circular-import refusal all do not happen.
- `src/bytecode/codegen.rs:449` — `Statement::Export` compiles to nothing.
- `src/bytecode/vm.rs:2017` — `fn compile_module` compiles only the `set` and
  `constant` statements of a module file, so a module file's `to` functions
  are never bound under the `module_function` name the tree-walker binds them
  to (`Vm::load_module`, `src/vm.rs:443`). `import MathUtils as M` then
  `M.circle_area(5)` is `Unknown function 'M_circle_area'` on this VM and
  `12.56636` on the tree-walker.
- `src/bytecode/vm.rs:1974` — `fn import` records the module name and
  then pushes `nothing` for the following `STORE`; it has no place to record
  *which name* the import bound, which is the whole of an alias. Verified:

  ```
  $ printf 'import json as J\nsay J.stringify(2)\n' | rb compile / rb vm
  RuntimeError: Unknown function 'J_stringify'      # tree-walker prints 2
  ```

**Why not fixed here:** the honest encoding is a new `Opcode::Module`, a
module frame, and an alias in the chunk header (which is a change to the
`.rbc` format). That is bootstrap-stage work — it belongs with the phases
that own the bytecode VM, not with the phase that gave the parser an arm for
`module`.

**What it constrained in this phase, stated plainly:** no Redblue `test`
block exercising `module ... end` or an import alias was added to `tests/`,
because every `.rb` file under `tests/` is corpus input to
`a_corpus_of_programs_runs_identically_on_both_vms`
(`tests/bytecode_vm_test.rs:133`) and the two VMs must agree on it. The
phase's coverage is therefore entirely in `tests/module_system_test.rs`,
which runs the tree-walking pipeline. Adding `tests/test_modules.rb` to
`NOT_COMPARABLE` (`tests/bytecode_vm_test.rs:124`) would have made the gate
green while hiding a divergence, which AGENTS.md rule 2 forbids.

## 2. `rb format` still writes the old `to` spelling of an import alias

**Severity:** minor (cosmetic drift from `SPEC.md` / `docs/GRAMMAR.md`)
**Evidence:** `src/formatter.rs:439` writes `" to "` where the alias goes, so
`rb format` rewrites `import json as J` into `import json to J`.

**Why not fixed here:** `tests/formatter_test.rs:579` and
`tests/formatter_test.rs:762` pin the round trip of
`import files, network to net` — source in, same text out. Changing the
formatter to emit `as` means editing that fixture, and AGENTS.md rule 2 forbids
re-scoping an existing test in a phase that did not break it. Both spellings
parse to the same tree (`src/parser.rs:560`), so the formatter's output is
always a valid program; it is a canonicalisation question, not a defect.

## 3. A module name is callable but not indexable

**Severity:** minor (an asymmetry, not a fault)
**Evidence:** `import json as J` then `say J.PI` is
`RuntimeError: Cannot access property on non-object` — `call_method`
(`src/vm.rs:1194`) resolves the receiver, but `Expr::Property` does not go
through that path.

**Why not fixed here:** `SPEC.md:662` writes the member access as a call,
`MathUtils.circle_area(5)`, and property access on a module namespace is not
in the documented surface. Making a module a record of its members is a
design decision about whether a module *is* a value.

## 4. `parser::module_body` drops sibling statements beside a `module` declaration

**Severity:** minor
**Evidence:** `src/parser.rs:267` — a file that holds a `module` declaration
*and* top-level statements beside it has those statements ignored when it is
read as a module.

**Why not fixed here:** `docs/GRAMMAR.md:145` — the `module` production — gives a module file a body and
nothing else, so the file this phase changed (`modules/MathUtils.rb`) is
inside the rule. Loosening it is a grammar question.