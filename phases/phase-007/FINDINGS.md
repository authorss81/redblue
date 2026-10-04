# Phase 007 — FINDINGS

Out of scope for this phase, each with the command that shows it. Nothing here
was fixed, because none of it belongs to numeric edge semantics.

## 1. `stdlib::builtins()` registers builtins the VM never dispatches

`src/stdlib.rs:16` inserts `sqrt`, and `src/stdlib.rs:203-207` implements them,
but `Vm::call` (`src/vm.rs:666`) has no arm for any of them: the match ends at
`src/vm.rs:1100` and falls through to the user-function lookup at
`src/vm.rs:1094`, which fails.

```
$ printf 'say sqrt(4)\n' > b.rb && ./target/debug/rb run b.rb
Error: RuntimeError: Unknown function 'sqrt'
$ printf 'say abs(-1)\n' > b.rb && ./target/debug/rb run b.rb
Error: RuntimeError: Unknown function 'abs'
$ printf 'say uppercase("a")\n' > b.rb && ./target/debug/rb run b.rb
Error: RuntimeError: Unknown function 'uppercase'
```

AGENTS.md's "Standard Library (Implemented)" lists all of these as working, and
`src/repl/completer.rs:75` even completes `sqrt`. Phase-007 fixes the *numeric
semantics* of `sqrt` at `src/stdlib.rs:270` (it no longer returns NaN), but a
Redblue program still cannot call it. Needs a phase: wire
`stdlib::builtin_function` into `call_builtin`, or move the arms into the VM.

## 2. Comparison and modulo syntax that SPEC.md documents is not in the lexer

`SPEC.md` §Comparison documents `>`, `>=`, `<`, `<=`, `==`, `!=`,
`is greater than`, `is less than or equal to`; §Arithmetic documents
`10 mod 3`. Only `is` and `is not` lex.

```
$ printf 'say 1 < 2\n' > b.rb && ./target/debug/rb run b.rb
Error: LexerError: Unexpected character '<'
$ printf 'say 1 == 1\n' > b.rb && ./target/debug/rb run b.rb
Error: LexerError: Unexpected character '='
$ printf 'say 5 mod 3\n' > b.rb && ./target/debug/rb run b.rb
Error: RuntimeError: Unknown variable 'mod'
```

`BinaryOp::{Less,LessEqual,Greater,GreaterEqual}` exist in `src/vm.rs:595-630`
but nothing can produce them. SPEC.md is ahead of the implementation. Phase-007
therefore could only test numeric equality (`0 * -1 is 0`), never an ordering.
Needs a phase: decide whether the symbolic forms are added to the lexer or
removed from SPEC.md.

## 3. `repeat` and `while` have no loop guard

`src/vm.rs:313` (`Statement::Repeat`) and `src/vm.rs:326` (`Statement::While`) loop on a number with
no ceiling. Phase-006 bounds call depth only.

```
$ printf 'set n to 0\nrepeat 1e18 times\n  set n to n + 1\nend\nsay n\n' > b.rb
$ timeout 5 ./target/debug/rb run b.rb; echo $?
124        # still running when killed
```

A Redblue program that overflows `n` used to print `infinity`; after phase-007
it fails with `infinity is not a finite number` — still unbounded, but no longer
printing a number that does not exist. A loop-count ceiling is still missing.

## 4. `Statement::ForRange` is unreachable and bypasses the numeric check

`src/vm.rs:282` handles `Statement::ForRange`, but `Parser::parse_for`
(`src/parser.rs:518`) only ever builds `ForEach`, so the range arm is dead code.

```
$ printf 'for each x from 1 to 10\n  say x\nend\n' > b.rb && ./target/debug/rb run b.rb
Error: ParserError: Expected In but got From
```

Its `i += step` (`src/vm.rs:308`) still builds `Value::Number` directly, so if
the arm is ever wired up, a range whose step overflows would put an infinity in
a number. Left alone in phase-007 as dead code; must be fixed by whichever phase
implements `for x from 1 to 10`.

## 5. `json.parse` renders its inner error twice

`src/vm.rs:910` wraps a failure with `e.to_string()`, which already includes the
`RuntimeError:` prefix and the source caret, so the reported message is

```
RuntimeError: infinity is not a finite number
  --> 1:1
```

nested inside a `Runtime` message. Phase-007 asserts on the substring
(`tests/numeric_edge_test.rs:187`) rather than the exact message because of it.

## 6. `Value::Number` is still publicly constructible with a non-finite double

The invariant fixes the variant list, so `Value::Number(f64::NAN)` remains
reachable from Rust (`Value::number` is the door the VM uses, not a private
type). Phase-007 specifies its display (`not a number`, `infinity`,
`negative infinity`) and asserts it, but no Redblue program can produce one. If
total type safety matters more than the variant list, that is a language-design
phase, not a bug fix.