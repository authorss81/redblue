# phase-114 FINDINGS

Work found while doing phase-114 that does **not** belong to this phase.
Recorded here for the auditor, per AGENTS.md § 1.7. Nothing below was fixed.

## 1. Dotted import is marked Not Implemented, not implemented

The dotted member import form `import a.b as c` is marked Not Implemented in
`docs/GRAMMAR.md`, and the parser rejects it with `ParserError: Dotted imports
are not implemented; use 'import name' or 'import name as alias'`. This phase
chose the "mark Not Implemented" arm of the definition of done rather than
implement a new module-member import form, which is a language-surface change
larger than a docs/parser mismatch. A future phase may implement it.

Reproduction:

```
import math.sqrt as s
say s(9)
```

⇒ `Error: ParserError: Dotted imports are not implemented; use 'import name' or 'import name as alias'`
