# Phase 005 — FINDINGS

Out-of-scope defects found while working this phase. Per AGENTS.md rule 7 they
are recorded here, not fixed. Each has a file:line anchor and a reproduction.

---

## F1 — User-defined functions never execute their bodies (severity: critical)

**Anchor:** `src/vm.rs:1005-1007`

```rust
// User-defined function (simplified)
if let Some(Value::Function(_, _)) = self.get_var(name) {
    Ok(Value::Nothing)
```

`Vm::call` recognises that a name is a user function and then returns
`Nothing` without ever evaluating the function body. The `Value::Function`
variant carries `(name, params)` only (`src/vm.rs:270`), so the body is not
reachable from `call` at all.

**Reproduction:**

```
$ printf 'to add(a, b)\n    give back a + b\nend\nsay "SUM-IS"\nsay add(2, 3)\n' > f.rb
$ rb run f.rb
SUM-IS
nothing
```

Expected `5`. A function with no `give back` is equally silent:

```
$ printf 'to greet(name)\n    say "HELLO-{name}"\nend\ngreet("World")\nsay "AFTER"\n' > g.rb
$ rb run g.rb
AFTER
```

Expected `HELLO-World` before `AFTER`.

**This already breaks `examples/hello.rb`, the file AGENTS.md §2 calls
"specification-by-example".** `examples/hello.rb:9-13` defines `greet(name)` and
calls it:

```
$ rb run examples/hello.rb | cat -n
     1  Hello, World!     <- line 2, `say "Hello, World!"`
     2  Hello, World!     <- line 6, `say greeting`
                          <- line 13, `greet("World")`, prints nothing
```

Three `say` statements, two lines of output, **exit code 0**. Any gate that
checks only the exit status of `examples/*.rb` passes this.

**Why no test caught it:** `tests/redblue_test.rs:55` (`test_function`) asserts
only `result.is_ok()`, which is a smoke test under AGENTS.md §3.1. No
`#[test]` anywhere in the repository defines a Redblue `to` function and asserts
what it returned — `grep -n "^\s*to " tests/expect_test.rs` finds none.

**Suggested acceptance gate:** a user function executes its body, `give back`
yields the value, `say add(2, 3)` prints `5`, and `examples/hello.rb` prints
three lines. `Value::Function` will need to carry a body (`Rc<[Stmt]>` or an
index into the program) — a public-API change to a variant AGENTS.md §2 lists as
frozen, so this needs its own phase.

---

## F2 — `constant` declarations are specified but not implemented (severity: major)

**Anchor:** `SPEC.md:191`, `docs/GRAMMAR.md:179` (spec) vs `src/lexer.rs:223`
(keyword table — `constant` is absent).

```
SPEC.md:191:constant PI to 3.14159
```

No `TokenKind::Constant` exists anywhere in `src/`.

**Reproduction:**

```
$ rb run modules/MathUtils.rb
Error: ParserError: Expected function name
  --> modules/MathUtils.rb:4:16
4 | constant PI to 3.14159
  |                ^
```

`modules/MathUtils.rb:4` and `:5` both use it, so the whole module fails to load
and no program can `import MathUtils`. This is a backwards-compatibility break on
a file AGENTS.md §2 names as "specification-by-example". It is not caused by this
phase: the phase-005 lexer diff only added `Token::span()` and removed nothing.

---

## F3 — Out-of-bounds list indexing returns `Nothing` instead of erroring (severity: major)

**Anchor:** `src/vm.rs:411`

```rust
Ok(items.get(i as usize).cloned().unwrap_or(Value::Nothing))
```

```
$ printf 'set xs to [1, 2, 3]\nsay xs[99]\nsay "AFTER"\n' > i.rb
$ rb run i.rb
nothing
AFTER
```

AGENTS.md §3.2 requires index `-1`, `len` and `999` to be "a clean runtime
error, never a panic". They are silent. A negative index that is more negative
than the length wraps: `src/vm.rs:406-410` computes `len + n` as `i64`, and
`xs[0 - 1]` returns `3` rather than failing.

This is why the `out_of_bounds` row of phase-005's edge matrix is N/A: with no
error raised there is no span to attach.

---

## F4 — Unterminated string literals silently swallow the rest of the file (severity: major)

**Anchor:** `src/lexer.rs:319`

```rust
let text = lexer.read_text();
```

**Reproduction:**

```
$ printf 'set x to "abc\nsay 1\n' > s.rb
$ rb run s.rb
abc
say 1
```

`read_text` runs past the newline instead of stopping at it, so `x` becomes the
multi-line string `abc\nsay 1` and the program succeeds. AGENTS.md §3.2 lists
"unterminated string" under malformed input. It should be a `LexerError` with a
`Span` — `Error::Lexer` now has the field to carry it, which this phase added.

---

## F5 — `tests/redblue_test.rs` is four smoke tests (severity: minor, process)

**Anchor:** `tests/redblue_test.rs:6`, `:18`, `:31`, `:42`, `:55`

All five tests assert `result.is_ok()`. None can fail for a wrong answer, only
for a crash — which is exactly what let F1 through CI. AGENTS.md §3.1 requires
`expect <expr> to be <value>` or an equivalent. Converting them to assert output
would have caught the function defect on the day it was written.