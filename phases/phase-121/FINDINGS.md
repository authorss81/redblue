# Phase 121 — findings from re-verification

## The audit's reproduction no longer holds on `main`

The evidence says `filter([1,2,3], to (x) give back x > 1)` answers
`RuntimeError: filter was called with arguments it cannot use`. It does not:

```bash
$ ./target/debug/rb run target/tmp_filter.rb
[2, 3]
$ ./target/debug/rb compile target/tmp_filter.rb -o target/tmp_filter.rbc
$ ./target/debug/rb vm target/tmp_filter.rbc
[2, 3]
```

`src/interpreter.rs:3092` and `src/bytecode/vm.rs:3405` ask
`stdlib::list_operation` before `stdlib::builtin`, so both engines dispatch
`map`/`filter`/`reduce` through the higher-order driver. Phase-077 added this
(`phases/phase-077/REPORT.md`); the audit read the registration in
`src/stdlib.rs` and the fallback message in `src/stdlib.rs:1246` but not the
dispatch that runs first. `tests/stdlib_modules_test.rs` already pinned
`filter` and `reduce` on both engines before this phase.

## The remaining gap this phase closes

The phase's own definition of done is `reduce([1,2,3], 0, to (a,b) give back a + b end)`,
i.e. the start value before the function. `stdlib::reduce_parts` accepted only
`reduce(xs, f)` and `reduce(xs, f, start)`, so the definition-of-done program
still failed:

```
RuntimeError: reduce requires a function and an optional start value, but was given 2 arguments
```

That is the change made here: an equivalent `[start, Function]` arm in
`reduce_parts`, so both orders reach the same driver. No name was removed and no
name was added.

## Template inconsistency

The definition of done asks for "the nine functions written this phase or the
nine names removed". The verified finding is explicitly "two names, not a
class": `filter` and `reduce`. This phase adds no new builtin name and removes
none; it accepts one more argument order for `reduce`. The report names those two
names rather than fabricating seven others.
