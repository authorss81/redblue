# FINDINGS — phase-034

## 0. Re-dispatch on 2026-10-08 after `03bfa06`: the phase finding is stale

phase-034 was dispatched a second time with `HEAD = 03bfa06`, which is the
phase's own commit. Its finding does not reproduce on that `HEAD`. Per the
phase prompt's own instruction ("a stale phase must never be 'fixed' by
inventing a change"), this run changed **no production code**.

Both halves of the finding's evidence, re-run against `HEAD`:

```
$ printf 'set double to to (x) give back x * 2\nsay double(21)\n' > target/tmp/a.rb
$ cargo run --quiet --bin rb -- run target/tmp/a.rb; echo "exit=$?"
42
exit=0

$ printf 'say [1,2,3].map(to (x) give back x * 2)\n' > target/tmp/b.rb
$ cargo run --quiet --bin rb -- run target/tmp/b.rb; echo "exit=$?"
[2, 4, 6]
exit=0
```

`src/parser.rs:67` now carries `Expr::FunctionLiteral { params, body }`, which
is exactly what the finding said was absent from `Expr`. The two
`ParserError: Unexpected token To` outputs quoted in `REPORT.md` are what the
parser emitted on `ca6bd7d`; they are not emitted on `03bfa06`.

The audit trail says the dispatcher moved here while the phase was already in a
terminal-looking state — `03bfa06` is `rbops: phase-034`, the commit the
dispatcher itself writes on a successful run, and the working tree was clean at
`HEAD` with no `.running`, `.deferred`, `.blocked` or `.starved` marker in
`phases/phase-034/` (only `FINDINGS.md` and `REPORT.md`, which is a `.done`
shape). **The auditor should check why a completed phase was re-selected**;
if the phase is `.done`, re-dispatching it will keep producing this same
outcome, and if it is not `.done`, the marker that records which state it is in
is not being committed.

Also stale in the phase prompt, for whoever regenerates it: it states
`tests/loop_bounds_test.rs` has **22** tests. It has had **24** since at least
`ca6bd7d` (`git show ca6bd7d:tests/loop_bounds_test.rs | grep -c '#\[test\]'` →
24), and phase-034 did not touch that file. All 24 pass.

Defects found while adding the function literal that this phase did **not** fix,
because fixing them is not "make `to (x) ... end` an expression". Each is
anchored to a line read during this phase and is a candidate for the auditor to
promote to a phase.

## 1. 65 nested function literals overflow a 2 MiB stack — `src/parser.rs`

`MAX_BLOCK_DEPTH` is 64 (`src/parser.rs:345`) and the parser reports
`Blocks nest more than 64 levels deep`, which is what a file of 65 nested
literals gets from `rb`:

```
$ python3 -c "print('set f to to ()'*65 + 'give back 1' + ' end'*65)" > target/tmp/deep.rb
$ ./target/debug/rb run target/tmp/deep.rb; echo "exit=$?"
Error: ParserError: Blocks nest more than 64 levels deep
  --> target/tmp/deep.rb:1:906
exit=1
```

The same 65 levels on a **2 MiB** thread abort the process instead:

```
thread '<unknown>' (46143) has overflowed its stack
fatal runtime error: stack overflow, aborting
```

Measured on this branch with a probe that lexes and parses on a thread of a
given stack size: 65 nested `to f() ... end` blocks fit in 2 MiB, and 52 nested
literals fit but 56 do not. The cost per level is roughly 30 KB, dominated by
the frame of `parse_statement_inner`, and a literal level pays that frame *and*
the whole expression-precedence chain (`parse_expression` → … → `parse_primary`),
which a block level does not. `rb` survives only because it parses on the main
thread, which is 8 MiB.

This phase's own test for the budget
(`edge_nested_literals_past_the_block_budget_are_reported`) therefore runs in a
child process, and says why in a comment. The fix belongs to whoever tunes the
parser's stack budget: split the fat arms of `parse_statement_inner` into
`#[inline(never)]` helpers, or claim `MAX_NESTING_DEPTH` for a literal as well as
`MAX_BLOCK_DEPTH`. Raising `MAX_BLOCK_DEPTH` would make it worse, not better.

## 2. `stdlib::builtin_function` is unreachable — `src/stdlib.rs:210`

`uppercase`, `lowercase`, `trim`, `split`, `join`, `contains`, `starts_with`,
`replace`, `abs`, `floor`, `ceil`, `round`, `sqrt`, `pow`, `sin`, `cos`, `tan`,
`log` and `exp` are all registered in `stdlib::builtins()` as
`Value::Builtin(name)` and all implemented in `stdlib::builtin_function`, and
nothing calls that function:

```
$ git show HEAD:src/vm.rs | grep -c "stdlib::builtin_function"
0
$ printf 'say uppercase("a")\n' > target/tmp/u.rb && ./target/debug/rb run target/tmp/u.rb
Error: RuntimeError: Unknown function 'uppercase'
```

`Vm::call` (`src/vm.rs:1521`) resolves a name through `runtime::builtin` and then
through the variable table, and neither reaches `stdlib::builtin_function`. So
the entire table AGENTS.md § Standard Library documents as implemented is dead
code. `map` was in the same position — it is registered but had no
implementation at all — which is why this phase had to write one
(`Vm::map_builtin`).

## 3. `filter` and `reduce` are registered and unimplemented — `src/stdlib.rs:66`

`SPEC.md` § list writes `list.filter(xs, to (x) …)` and
`list.reduce(xs, 0, to (acc, x) …)`. Both names are registered as builtins and
neither exists, so both report `Unknown function`. This phase implemented `map`
because its own definition of done names it; `filter` and `reduce` belong with
it and were left alone to keep the diff on-topic.

## 4. `add x to y` and `x mod y is 0` do not parse — `src/parser.rs`

`SPEC.md` § Closures writes `add 1 to count`, and § list writes
`to (x) give back x mod 2 is 0`:

```
$ printf 'to f()\n    add 1 to total\nend\nf()\n' > target/tmp/a.rb && ./target/debug/rb run target/tmp/a.rb
Error: ParserError: Expected End but got Eof
```

So the `make_counter` example in `SPEC.md` § Closures still cannot run even
though `give back to … end` — the literal form that example needs — now parses
and returns a function. This phase enabled the literal, not the arithmetic.

## 5. An argument-count check would break a checked-in corpus fixture — `corpus/functions-0015.rb`

A wrong argument count binds the missing parameters to `nothing` and drops the
extra ones rather than being reported. A clean refusal is the better behaviour
and was implemented and measured in this phase:

```
'wrong' takes 1 argument, but 0 were given
```

It fails three existing tests, because `corpus/functions-0015.rb` is
`say wrong()` against `to wrong(n)` and its checked-in expectation
(`corpus/functions-0015.expected`) records `nothing`. Changing a corpus
expectation is a gate-weakening move, so the check was reverted and the failure
channel for a wrong argument count is what the language already does — the body
fails on the `nothing` its missing parameter was given
(`edge_calling_a_literal_with_too_few_arguments_fails`). A phase that wants the
refusal has to re-record the corpus, which is a deliberate act.

## 6. A list literal cannot begin on the next line — `src/parser.rs`

```
$ printf 'set xs to [\n    1,\n    2\n]\nsay xs\n' > target/tmp/l.rb && ./target/debug/rb run target/tmp/l.rb
Error: ParserError: Unexpected token Newline
```

`parse_primary`'s `LeftBracket` arm does not `skip_newlines()` after the `[`, so
a list that opens a line of its own is rejected. Not a literal bug — it is why
this phase's list-of-literals test writes the list on one line — but it is the
same shape of gap as the one this phase fixed, and the block forms all skip
their newlines.