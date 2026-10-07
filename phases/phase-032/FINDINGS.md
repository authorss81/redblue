# Phase 032 — FINDINGS

Work found while reconciling the stdlib module list in SPEC.md with
`src/stdlib.rs` that does **not** belong to this phase. Each entry is
file:line anchored so the auditor can promote it.

## 1. `MODULES` had no entry for `text`, `math`, `list` or `formats` — FIXED here

**Status:** fixed. `src/stdlib.rs:11` now lists all ten, and
`tests/stdlib_module_docs_test.rs` fails if either document and the list stop
agreeing in either direction.

Reproduced before the change:

```
$ printf 'say text.uppercase("hi")\n' > /tmp/a.rb && rb run /tmp/a.rb
Error: AnalyzerError: Unknown variable 'text'
$ printf 'say formats.parse_json("{}")\n' > /tmp/b.rb && rb run /tmp/b.rb
Error: AnalyzerError: Unknown variable 'formats'
```

The root cause was wider than the missing names: `src/stdlib.rs:22-58` registers
`uppercase`, `split`, `abs`, `sqrt` … as `Value::Builtin`, and **nothing
dispatched a `Value::Builtin`**. `Vm::call` only matched `Value::Function`
(`src/vm.rs:1163` before this phase), so `say uppercase("hi")` failed with
`Unknown function 'uppercase'` too — the flat names were dead as well as the
module ones.

## 2. `split` and `join` were registered but never implemented — FIXED here

**Status:** fixed in `src/stdlib.rs` `builtin_function`. `split`, `join`,
`contains`, `starts_with`, `ends_with`, `replace`, `push`, `pop`, `shift`,
`map`, `filter`, `reduce`, `pow`, `sin`, `cos`, `tan`, `log`, `exp`,
`is_number`, `is_text`, `is_list`, `is_record`, `to_text`, `to_number`,
`to_list` are all registered in `builtins()` and answered by **nothing**.

Only `split` and `join` were implemented here, because SPEC.md and README.md
both document `text.split` / `text.join`. The rest are listed below.

## 3. Bare builtin names still say `Unknown function` — NOT fixed, deliberately

**Status:** open. `say uppercase("hi")` and `say split("a,b", ",")` still fail
with `Unknown function`, while `text.uppercase("hi")` works.

`src/vm.rs:1163` now routes a `Value::Builtin` **only** when
`stdlib::call_module_function` recognises the name, so a bare builtin behaves
exactly as it did before this phase. Fixing it means dispatching every
registered `Value::Builtin` through `stdlib::builtin_function`, which would
turn `map`, `filter`, `reduce`, `push`, `pow` and 15 others from
`Unknown function` into *either* a working function or a wrong-argument error —
a much larger change to the language surface than reconciling module names
belongs to. It needs its own phase with its own tests.

## 4. SPEC.md documented calls the parser cannot read — corrected here, three more remain

**Corrected in this phase** (each named in `REPORT.md`):

- SPEC.md:849 `list.map([1, 2, 3], to (x) give back x * 2)` — the function
  literal of phase-034; `to (x) give back x * 2` is a `ParserError`.
- SPEC.md:822 `text.split("a,b,c", by ",")` — `by` is the range-loop step marker
  and is not a named argument; `AnalyzerError: Unknown variable 'by'`.
- SPEC.md:776 `set response to wait network.get(...)` — `wait` is a reserved
  token the parser does not accept in an expression; `ParserError: Unexpected
  token Wait`.
- SPEC.md:829 `set pi to math.PI` — a module has no members but functions, so
  `math.PI` is `AnalyzerError: Unknown variable 'math'` and bare `PI` is
  `Unknown variable 'PI'` (there is no stdlib constant binding in the analyzer;
  see FINDINGS 5).
- SPEC.md:864 `formats.parse_json('{"name": "Alice"}')` — single quotes are not
  a text literal in the lexer; `LexerError: Unexpected character '\''`.

**Still open, same class, not corrected here:**

- SPEC.md:310-311 in §Function Call shows `text.length("hello")` and
  `math.sqrt(2)` — both work now.
- SPEC.md:634 `give back math.PI * this.radius * this.radius` in the Properties
  example. Same problem as §Standard Library's `math.PI`, corrected there but
  **not** here, because this phase's test only reads §Standard Library. This is
  a leftover the auditor should route to the next spec-drift phase.
- SPEC.md:809-813 §console documents `ask "Your name?"`; `ask` is a reserved
  token (`src/lexer.rs`) that no statement parses, so it is a
  `ParserError: Unexpected token Ask`. `console.log` / `console.error` /
  `console.clear` all work.

## 5. There is no stdlib constant binding — NOT fixed

`src/stdlib.rs` inserts `PI` and `E` into the globals map, but the analyzer
never learns they are bound, so every read is `Unknown variable 'PI'`
(`tests/constant_test.rs:309` asserts exactly that, for a different reason).
`AGENTS.md` §Math Functions and `src/repl/completer.rs:70` both promise them.
Either the analyzer should know the stdlib globals, or the two constants should
be `constant` declarations SPEC.md teaches. Not this phase's concern.

## 6. `list.map` / `list.filter` / `list.reduce` do not exist

SPEC.md documented three higher-order list functions. They are registered in
`src/stdlib.rs` and implemented nowhere, and they take a function argument the
language cannot yet pass as a value. SPEC.md §Standard Library now documents
`list.length` only, and says why. The functions are phase-034's job.

## 7. `text` and `list` are not reserved words, which is load-bearing

`list` is a type name in the grammar (`take a list`), and `text` is a type name
(`set text to ""`). Neither is in the keyword table (`src/lexer.rs`), so
`list.length([1, 2])` parses as a member call and `set list to []` still works.
This phase relies on that and did not change it. Recorded so a future phase that
reserves those words knows what it breaks.