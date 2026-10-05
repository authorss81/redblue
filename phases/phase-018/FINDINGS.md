# phase-018 — Findings

Work that is out of scope for this phase, recorded here so the auditor can
promote it to a real phase. Nothing below was fixed here.

## 1. `modules/MathUtils.rb` does not parse, so `rb compile` cannot compile it

`rb compile modules/MathUtils.rb` fails:

```
ParserError: Expected function name
  --> modules/MathUtils.rb:4:16
4 | constant PI to 3.14159
```

`constant` is not in the lexer's `KEYWORDS` table (`src/lexer.rs:14-68`), so
`constant PI to 3.14159` reaches the parser as an identifier expression and the
declaration syntax has no rule. `modules/SuiteKit.rb` — which uses only `set`
and `to` — compiles.

This is a frontend gap, not a bytecode one: `rb run modules/MathUtils.rb` fails
the same way today, so it predates this phase. Phase-018's definition of done
is `examples/*.rb`, and all six compile. A phase that adds `constant … to …` to
the lexer and parser would unblock compiling every `modules/*.rb`.

## 2. `Expr::InterpolatedText` is unreachable — `{interp}` is not implemented

`Expr::InterpolatedText(Vec<Expr>)` is declared at `src/parser.rs:56` and
nothing ever constructs it. The lexer keeps the braces in the literal, so:

```
$ rb run target/tmp/interp.rb      # set n to 2 / say "n is {n}"
n is {n}
```

`BUILD_TEXT` and `JOIN`-style interpolation are specified in `SPEC.md` and in
`AGENTS.md`'s invariant table ("Trailing-comma and `{interp}` string syntax"),
so this is a specified feature the frontend does not implement. The compiler
lowers the node correctly, and
`edge_an_interpolated_text_becomes_build_text_over_its_parts` pins that with a
constructed AST — the only way to reach it until the parser emits the node.

## 3. `AGENTS.md` documents an import syntax the parser does not accept

`AGENTS.md` shows `import files, network as net`. The parser accepts
`import files to net` (`src/parser.rs:477-497` expects `To`, then an
identifier); `as` is not a keyword at all, and `import MathUtils, files as net`
is an analyzer error — `Unknown variable 'as'`. The compiler supports both
forms of `ImportItem` (`name`, optional `alias`) and is not the thing to
change; either the parser gains `as` or `AGENTS.md` is corrected to `to`.

## 4. `rbops/verify.sh` is not in this checkout

```
$ ls rbops/verify.sh
ls: cannot access 'rbops/verify.sh': No such file or directory
```

The fourth gate could not be run here, the same as phase-017. The dispatcher
runs it. Recorded in `REPORT.md` as **not run**, not as passed.

## 5. Stage S1b: what the format deliberately leaves open

Recorded so the next phase does not have to re-derive it. None of it is
required for S1a and none of it is claimed to work.

- **Nothing runs a `.rbc`.** `rb vm file.rbc` does not exist; `rb` still only
  tree-walks source. The fixed-point ladder in `AGENTS.md` §7 starts at S1b.
- **`BREAK` / `SKIP` are opcodes, not jumps.** They were made opcodes so that a
  loop's shape does not have to be rewritten to find them, including inside a
  `try`. S1b owns what `BREAK` inside a `try` inside a loop means.
- **A loop holds its iterator or range on the stack across its body.** No
  instruction pops it; S1b decides where it is released.
- **`TRY` names handler blocks instead of jumping to them.** The failed value
  being the value a `catch` block is entered with is a requirement written into
  `docs/BYTECODE.md`, not an implemented behaviour.
- **Names are stored, not resolved to slots.** `LOAD`/`STORE` carry a name; a
  Redblue closure resolves it against the captured scope. Slot allocation is a
  later stage and a version bump, since it changes what a version-N file means.
- **`GET_RANGE` operand counts (1, 2, 3) are a convention this compiler
  established.** A VM written later reads them; if a different split proves
  better it is a new opcode byte and a version bump. (The other convention in
  this list, `DEF_OBJECT`'s "extends" flag, is gone: the parent is a name in
  the constant pool, which review round 1 fixed. See `REPORT.md`.)
- **Nothing decides what `NO_CONST` in `DEF_OBJECT` should do about a parent
  that is not declared.** The tree-walking VM walks the chain by name and
  reports a missing or cyclic parent at run time (`src/vm.rs`,
  `declare_object`). A `.rbc` VM has to do the same, and the file gives it
  everything it needs to: the parent's name, interned like any other.