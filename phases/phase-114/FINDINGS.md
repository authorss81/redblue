# Phase 114 — FINDINGS

Work that belongs to a phase other than this one. Nothing here was implemented;
each item is anchored to a file:line and a reproduction, per AGENTS.md § 6.4
("a phase with no file:line evidence is not created").

## 1. `docs/GRAMMAR.md` § 4.1 writes a typed declaration the parser refuses

The same defect class as this phase's finding, in the same section: a production
with no Not Implemented note and no parser arm.

- `docs/GRAMMAR.md:203` —
  `declaration = 'set' identifier [ ':' type_expression ] 'to' expression`
- `docs/GRAMMAR.md:209` — the same `[ ':' type_expression ]` on `constant`.
- `src/parser.rs:779` (`parse_set`) — the name arm accepts an `Identifier` or
  `This` and nothing else, so a `Colon` after the name is never consumed.

Reproduction (before and after this phase — unchanged by it):

```
$ printf 'set x: number to 1\nsay x\n' > typed.rb && rb run typed.rb
Error: ParserError: Expected 'to' but got Colon
  --> typed.rb:1:6
```

The grammar also documents `type_expression` throughout § 2
(`list_type`, `record_type`, `object_type`, `function_type`, `generic_type`,
`union_type` at `docs/GRAMMAR.md:140-157`) and `annotated_statement` /
`parameter` forms. Whether the whole type-annotation grammar is implemented is a
larger audit than this phase could answer; what is verified here is that the
*optional* annotation on `set` and `constant` is refused with a bare
`Expected 'to' but got Colon`, which is the wording a reader cannot act on.

Suggested phase: "Make `set x: T to …` and `constant x: T to …` parse, or strike
the annotation from GRAMMAR.md § 4.1" — and, as part of the same audit, count
how many of § 2's `type_expression` productions any program can contain.

## 2. A builtin value is callable now but not usable

This phase had to make one narrow change outside its own feature: a name bound to
a `Value::Builtin` is that builtin when it is **called**, because a dotted import
binds the member's own value and `import math.sqrt as s` then `s(9)` is required
to print 3 by this phase's own definition of done.

- `src/interpreter.rs:2302` — `Vm::call`, the `Value::Builtin` arm, guarded by
  `builtin != name` so a name bound to a builtin of its own name cannot recurse.
- `src/bytecode/vm.rs:2779` — the same arm in the bytecode VM.
- `src/value.rs:257` — `Value::Builtin(String)` carries the name, which is what
  makes the delegation possible at all.

What is still refused, unchanged by this phase:

```
$ printf 'set g to 1\nsay g.map\n' > bv.rb && rb run bv.rb
Error: RuntimeError: Cannot access property on non-object
```

and a builtin passed as an argument to another builtin (`map(sqrt, 2)`) has no
path, because `stdlib::map_function` requires a `Value::Function` and the
registry holds builtins as names rather than as callable values.

So "builtins are values" is now **half** implemented by accident of this phase's
need, and the half that landed was landed for a different reason. That is worth
its own phase with its own decision, because the alternative is to keep a builtin
value that can be called but not read, which is the kind of half-feature a reader
cannot predict. Suggested phase: "Decide what a builtin value is — callable, or a
name — and make both engines agree on the whole of it."

## 3. A name repeated inside one multiple assignment is not an error

`set x, x to 1, 2` binds `x` twice and is accepted, because the interpreter binds
each position left to right and the second binding wins. That is what two
consecutive `set x to …` statements do, and it is what
`src/analyzer.rs:330` (`SetMultiple`, `declare_assigned` per name) allows — the
second `declare` is the same call the single `set` makes twice.

No test pins it either way and no production says what should happen:
`docs/GRAMMAR.md:206` writes `{ ',' identifier }`, which does not say the
identifiers must be distinct. Recorded rather than changed, because a duplicate
name is a question about the language's declaration rules and not about this
phase's two productions. If the answer is "a clean parse error", it belongs with
whatever phase settles `constant` redeclaration, which has the same shape.

## 4. The REPL's `:example` list and `docs/GRAMMAR.md` have no dotted-import example

`src/repl/mod.rs:343` (`QUICK_REFERENCE`) and the `:example` list below it cover
`set`, `say`, `if` and `for each` only. Neither the dotted import nor the
multiple assignment is in either list, so `rb repl`'s `:help` still shows nothing
about the two forms this phase added. This phase deliberately did not add them:
`src/repl/mod.rs` is REPL surface, the two lists are examples a user reads, and
which examples belong there is the maintainers' call rather than a parser phase's.
Cheap for whoever owns the REPL examples.

## 5. `rbops/` is absent from the checkout, so `verify.sh` could not be run

`ls rbops/` → `No such file or directory`. The phase's fourth gate is
`./rbops/verify.sh phase-114` and it could not be executed. AGENTS.md § 1 rule 1
forbids touching `rbops/`, so it was neither created nor reconstructed. The three
gates that were run are in REPORT.md; the fourth is reported as not run rather
than as passing. If a phase is expected to be verifiable in a checkout without
`rbops/`, the manifest should say so, because a reviewer reading only REPORT.md
cannot tell "not run because absent" from "not run".