# phase-029 — FINDINGS

Work that belongs to a later phase, found while implementing phase-029.

## 1. `catch <name>` binds the literal text `"error"`, not the error message

`src/vm.rs:740`:

```rust
self.set_var(var, Value::Text("error".to_string()))?;
```

A `try … catch failure … end` clause that does `say failure` prints `error`,
not `RuntimeError: Modulo by zero` and not even the bare message. A Redblue
program therefore cannot observe *which* error it caught, only *that* it caught
one. `SPEC.md` documents `try/catch` but never says what the bound name holds,
so this is a documentation-shaped defect as much as an implementation one.

Impact: any phase that must prove, from inside the language, that a specific
failure was produced cannot do it — which is exactly the "asserts a failure is
produced" row in AGENTS.md section 3.1. phase-029 pins the message from Rust
(`assert_both_fail`) and pins only catchability from Redblue.

Suggested phase: bind the error's `message()` to the catch name, and document it
in `SPEC.md`.

## 2. `length of <expr>` does not parse, but `SPEC.md` documents it

`SPEC.md:322` (the List Operations block this phase's `is in` example sits in):

```redblue
set count to length of items
```

`rb run` on that line gives:

```
Error: AnalyzerError: Unknown variable 'of'
  --> 1:8
```

`docs/GRAMMAR.md:448` lists `of` at precedence 8, so the grammar documents a
form the parser has never implemented. The same family of gap as this phase's
finding, in a different operator.

Suggested phase: implement `length of <expr>` (and audit the other precedence-8
`of` forms) or correct `SPEC.md`.

## 3. `rb format` emitted an unparseable `in` until this phase fixed it

Not a follow-up — fixed here — but recorded because the shape is worth the
auditor's attention: `BinaryOp::In` was fully implemented in the runtime, the
analyzer, the bytecode backend and the formatter, and completely unreachable
from the parser. Any `BinaryOp` variant with no parse site is invisible to the
`cargo test` suite, because no existing test can construct one.

Suggested phase: a gate that fails when a `BinaryOp`/`UnaryOp`/`Statement`
variant is never constructed in `src/`, mirroring
`tooling_grammar_test::grammar_covers_the_lexer_keyword_set`. phase-029's
`mod_is_a_keyword_and_not_an_identifier` and
`the_word_forms_run_on_the_bytecode_vm_too` are the ad-hoc version of that check
for these two variants.