# Phase 031 — FINDINGS

Work discovered while implementing `module ... end`, `export`, and the
`import X as Y` alias that does **not** belong to this phase. Each entry is
file:line anchored so the auditor can promote it.

## 1. FIXED — the bytecode VM had no module system

**Status:** a module file, an `import X as Y` alias and a `module ... end`
*declaration* now all work on both VMs, and the corpus holds programs that
declare a module, so the differential is no longer vacuous.

Fixed across the two runs, each with a test that was red before the change:

- `src/bytecode/vm.rs` `import` — a builtin namespace is importable under any
  alias. It has no file behind it, so the loader looked for one, did not find it,
  and refused `import json` outright: `rb run` printed the value and `rb vm` said
  `Cannot find module 'json'`.
- `src/bytecode/vm.rs` `import` — the alias is recorded, in a `module_aliases`
  table that `call_method` resolves through, exactly as the tree-walking VM does.
- `src/bytecode/vm.rs` `compile_module` — a module file's `to` functions are
  compiled and published onto their `module_member` names, so `import MathUtils
  as M` then `M.circle_area(5)` runs on this VM too. It previously said
  `Unknown function 'M_circle_area'`.
- `src/bytecode/vm.rs` `import` — a circular import is refused with
  `Circular import of module '<name>'`. The loader recorded a module as loaded
  *before* running it, so a self-import found it loaded and passed **silently**,
  while the tree-walking one refused it.
- `src/bytecode/vm.rs` `import` — an import does not take over a name the program
  already holds. The `STORE` after an `IMPORT` is stepped over rather than run
  when the alias is already bound, which is the tree-walker's guard. Writing it
  back worked for a `set` but not for a `constant`: `constant M to 5` then
  `import MathUtils as M` raised `Cannot assign to constant 'M'` here and
  succeeded there.
- `src/bytecode/vm.rs` `import` — a module is recorded in the loaded set only
  once it has compiled *and* run. The set was written before `compile_module`,
  whose `?` returned without removing the name, so a module that failed to
  compile left the name behind: a retry of the same name skipped loading and
  bound the alias to nothing instead of reporting the failure again.
- `src/bytecode/vm.rs` `import` — the module's *own* name is bound when unbound,
  which is what makes `say MathUtils` a read rather than an unknown variable.
- `src/bytecode/codegen.rs` — `Opcode::Module` (byte 47) and `Opcode::Export`
  (byte 48), and a `module body` block kind. A module declaration was compiled
  inline with `export` compiled to nothing, so this VM had no record of the
  module: the duplicate-declaration refusal, the `export`-validation refusal,
  the module's own scope and the published members all did not happen here.
- `src/bytecode/vm.rs` — `module`, `finish_module`, `module_exports` and
  `declare_member`, plus `module_depth` on `set_var` and `declared_modules` on
  `is_module_name`. The refusals are raised where the tree-walking VM raises
  them — before the body runs — and in the same words.
- `src/bytecode/format.rs` — `FORMAT_VERSION` 3 → 4. Version 3 wrote a filler
  `0` in `IMPORT`'s second operand, so read as version 4 a version-3 file would
  bind an import to `constants[0]`: whichever name happened to be first in the
  pool. A version-3 file is refused rather than reinterpreted.
- `src/parser.rs` — `module_exports` is now `pub` and lives with
  `module_declared_names`, so the compiler and the tree-walking VM read which
  `export` counts and what a module defines through one rule instead of two.

**Why this is not hiding a divergence:** `module ... end` is now exercised on both
VMs by ten generated programs in `tests/bytecode_vm_test.rs`'s corpus, by eight
`edge_*` tests in the same file, and by five Redblue `test` blocks in
`tests/test_modules.rb` that the corpus differential runs. Nothing was added to
`NOT_COMPARABLE`.

## 2. `rb format` still writes the old `to` spelling of an import alias

**Severity:** minor (cosmetic drift from `SPEC.md` / `docs/GRAMMAR.md`)
**Evidence:** `src/formatter.rs:440` writes `" to "` where the alias goes, so
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
(`src/vm.rs:1185`) resolves the receiver, but `Expr::Property` does not go
through that path.

**Why not fixed here:** `SPEC.md:662` writes the member access as a call,
`MathUtils.circle_area(5)`, and property access on a module namespace is not
in the documented surface. Making a module a record of its members is a
design decision about whether a module *is* a value. Both VMs answer it the
same way, so it is not a divergence.

## 4. `parser::module_body` drops sibling statements beside a `module` declaration

**Severity:** minor
**Evidence:** `src/parser.rs:267` (`module_body`) — a file that holds a `module` declaration
*and* top-level statements beside it has those statements ignored when it is
read as a module.

**Why not fixed here:** `docs/GRAMMAR.md:143` § 3.1 — the `module` production — gives a module file a body and
nothing else, so the file this phase changed (`modules/MathUtils.rb`) is
inside the rule. Loosening it is a grammar question.

## 5. The analyser still does not see into a module file

**Severity:** minor
**Evidence:** `src/runtime.rs:48` (`module_bindings`) returns only `set` and
`constant` names, so `import CircleMod` then `say X` where `CircleMod` declares
`constant X to 1` is `AnalyzerError: Unknown variable 'X'` on **both** VMs. It
is consistent, so it is not a divergence, but it means an imported name is
usable only once it is written as a plain read at run time.

**Why not fixed here:** the analyser reads a module's names from the file before
the VM runs, so a module whose declarations depend on a function the analyser
cannot execute cannot contribute names. Not a defect this phase introduced.

## 6. `rbops/baseline.json` cannot be updated from this checkout

**Severity:** informational
**Evidence:** `rbops/` is not in this checkout — `ls rbops` → `No such file or
directory`; AGENTS.md's contract is injected into the phase context rather than
checked out, and the phase's own constraints forbid touching `rbops/` anyway.

**What it means:** the baseline's `unparseable` entry for
`modules/MathUtils.rb` was recorded against the parser that refused
`module MathUtils` with `ParserError: Unexpected token Module`. That file parses
and runs today (`./target/debug/rb run modules/MathUtils.rb` exits 0), so the
entry is stale and should be dropped by whoever can edit `rbops/`. This phase
did fix it; it could not record that fact.

## 7. The bytecode VM resolves a call against a name, not a value

**Severity:** informational (a limit of the design, not a defect)
**Evidence:** `src/bytecode/vm.rs` `call_method`, and `src/bytecode/opcode.rs:105`
— `CALL_METHOD` carries
`receiver.method`, so a member call is resolved against the receiver's *name*.
That is what makes `files.read` the builtin `files_read` and `Counter.bump` a
method on a declared type, and it is the format's version-3 change.

**What it means:** a module a program declares is reached as `Name.member`, and
`Name` has to be a name. It is bound when unbound — `import Counter` then
`say Counter` reads `nothing`, and both VMs agree — but a receiver that is an
expression rather than a name has no spelling at all. Neither VM has one, so
nothing disagrees; it is recorded so a later phase does not read the design as
accidental.