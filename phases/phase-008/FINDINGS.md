# phase-008 — FINDINGS

Defects observed while proving total indexing and field access. **None of these
were fixed in this phase**: each is outside phase-008's scope (which is the
`Expr::Index` and `Expr::Property` paths in `src/vm.rs`), and the contract says
to record them here rather than smuggle them into the diff. Each is anchored to a
command and a line so the auditor can promote it to a real phase.

---

## F1 — `{interp}` string interpolation is not implemented, but `examples/` and SPEC use it

**Severity: blocker.** This is the largest single gap found in the phase.

`Expr::InterpolatedText` exists in the AST (`src/parser.rs:47`) and is evaluated
by the VM (`src/vm.rs:506`), but nothing ever *produces* it: the lexer emits
`TokenKind::Text(s)` for the whole string including the braces, and the parser
turns that into a literal `Expr::Text` (`src/parser.rs:1246-1248`). The
interpolation arm of the VM is therefore dead code.

```
$ printf 'set name to "World"\nsay "Hello, {name}!"\n' > i.rb && ./target/debug/rb run i.rb
Hello, {name}!
```

The braces are not even treated as markup: the text is passed through verbatim,
so the string prints the *name* of the variable rather than its value, and no
error is raised.

Consequences:
- `examples/hello.rb:10` prints `Hello, {name}!` instead of `Hello, World!`.
- `SPEC.md:623` documents `say "Error: {error message}"`, which cannot work.
- AGENTS.md's invariant table lists "Trailing-comma and `{interp}` string syntax"
  as protected, so the invariant is currently *violated*, not merely missing.

This phase could not use `say "x is " + y` style concatenation for message
assertions in the Redblue tests without noticing this; see F3 for the workaround
that resulted.

## F2 — `catch error` does not bind the error message

`catch error` binds the literal text `"error"`, so the message is unreachable
from Redblue source (`src/vm.rs:388`):

```rust
self.set_var(var, Value::Text("error".to_string()));
```

```
$ printf 'try\n    set x to [1][5]\ncatch error\n    say error\nend\n' > c.rb
$ ./target/debug/rb run c.rb
error
```

`SPEC.md:623-637` promises `say "Error: {error message}"` and typed catch
(`catch error of FileError`), neither of which exists. This is why the Redblue
tests in this phase can only assert *that* a failure was caught, and why every
assertion on the error *text* lives in `tests/index_bounds_test.rs` instead.

## F3 — ~15 stdlib functions are registered as builtins but not implemented

`src/stdlib.rs` inserts a global for each of these, but `call_builtin` in
`src/vm.rs` has no arm for them, so each raises
`RuntimeError: Unknown function '<name>'`:

`push`, `pop`, `shift`, `map`, `filter`, `reduce`, `contains`, `starts_with`,
`ends_with`, `replace`, `is_number`, `is_text`, `to_text`, `to_number`,
`to_list` (plus the `*_contains`-style module forms).

```
$ printf 'say contains("hello", "ell")\n' > s.rb && ./target/debug/rb run s.rb
Error: RuntimeError: Unknown function 'contains'
```

AGENTS.md advertises `contains("text", sub)`, `split`, `join`, `replace` and the
`push`/`pop`/`map`/`filter`/`reduce` family as part of the standard library. The
registration without an implementation means a program that reads as valid
Redblue fails at the last statement. This phase had to assert on exact strings
with `expect x to be "..."` instead of `contains(...)`.

## F4 — `set x.field to v` on a non-record silently does nothing

Field *read* is a `Runtime` error on a non-record (`src/vm.rs:471-481`), but field
*write* is not (`src/vm.rs:234-245`): the `if let Some(Value::Record(..))` arm has
no `else`, so the statement evaluates to `nothing` and the program continues with
exit code 0.

```
$ printf 'set n to 5\nset n.x to 1\nsay n\n' > f.rb && ./target/debug/rb run f.rb
5
exit=0
```

A typo in a field name is therefore indistinguishable from a successful write —
silent data loss, which is the same defect class as the out-of-bounds index this
phase fixed. Left alone here because it is `Statement::SetProperty`, not
`Expr::Index`/`Expr::Property`. A follow-up should make it symmetric with the
read path and name the shape it refused.

## F5 — `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` in the time module

Four sites: `src/vm.rs:742`, `src/vm.rs:907`, `src/vm.rs:1066`, `src/vm.rs:1078`,
`src/vm.rs:1091`. `duration_since` returns `Err` when the instant is *before* the
epoch, so a machine with a clock set before 1970-01-01 panics the interpreter
instead of producing a `Runtime` error. Not reachable from indexing, so out of
scope; it is the only remaining `unwrap()` on a user-reachable path that this
phase's audit of `src/vm.rs` surfaced outside the JSON parser (whose `parts[0]` at
`src/vm.rs:1221` is guarded by the `parts.len() != 2` check on the line above).

## F6 — SPEC.md documents indexing syntax the parser does not accept

`SPEC.md:317-319` shows `set first to items at 0` and `set last to items at -1`.
The `at` keyword does not exist: `items at 0` is an analyzer error
(`Unknown variable 'at'`), and `docs/GRAMMAR.md:333` gives the real form,
`postfix '[' expression ']'`. The *semantics* SPEC documents (0 is first, -1 is
last) are correct and are what this phase enforces; only the surface syntax in
SPEC is wrong. This phase deliberately did not add `at` — hard rule 8 forbids a
grammar change without a phase that says so — but SPEC.md should be corrected, or
`at` should be implemented, by whoever owns the grammar.