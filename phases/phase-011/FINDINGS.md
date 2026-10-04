# Phase 011 — FINDINGS

Work that was found while doing phase 011 but is **not** this phase's concern.
Per AGENTS.md hard rule 7 these are recorded here for the auditor to promote
into real phases.

## 1. `give back` does not leave a function

**Where:** `src/vm.rs` — `Statement::GiveBack` evaluates its expression and
returns it from `execute_statement`, but no flag is set and nothing unwinds. A
`give back` inside an `if`, a `for` body or a `try` body therefore yields a
value that is then discarded, and the statements *after* it still run.

**Evidence.** The base case of a recursive function does not stop the
recursion, so the program runs to the call-depth limit instead of returning:

```
$ cat target/tmp/give_back_in_if.rb
to parity(n)
    set answer to yes
    if n is 0 then
        give back answer
    end
    give back is_odd(n - 1)
end

to is_odd(n)
    set answer to no
    if n is 0 then
        give back answer
    end
    give back parity(n - 1)
end

say parity(10)
$ ./target/debug/rb run target/tmp/give_back_in_if.rb
Error: RuntimeError: Maximum call depth of 1000 reached while calling 'parity'
  --> target/tmp/give_back_in_if.rb:14:10
14 |     give back parity(n - 1)
   |          ^
```

`parity(10)` should be `yes`. The `give back answer` in the base case is
discarded and `give back is_parity_odd(9)` runs as well.

This is pre-existing on `main` and phase 011 does not touch it, but it shapes
every recursive program in the language: the closure tests in
`tests/closure_capture_test.rs` and `tests/test_closures.rb` all have to compute
a conditional result into a variable and return it *after* the block, which is
not what a reader of SPEC.md would write. It is arguably the largest single
gap between SPEC.md and the interpreter.

**Suggested phase:** "`give back` returns from the enclosing function".

## 2. No anonymous function form, so SPEC.md's closure example does not parse

**Where:** `SPEC.md` § Closures, and `src/parser.rs::parse_function`, which
requires an identifier after `to`.

**Evidence.**

```
$ cat target/tmp/spec_closure.rb
to make_counter(start)
    set count to start
    give back to
        give back count
    end
end
$ ./target/debug/rb run target/tmp/spec_closure.rb
Error: ParserError: Expected function name
  --> target/tmp/spec_closure.rb:3:17
3 |     give back to
  |                 ^
```

`add 1 to count` from the same example is also not in the grammar
(`Expected End but got Eof`). Phase 011 makes *named* nested declarations real
closures; the anonymous form SPEC.md shows is a separate language addition, and
`add … to …` is a separate grammar addition. Both were deliberately left alone
— adding them would change the language surface, which hard rule 8 forbids
without a phase that says so.

**Suggested phase:** "Anonymous function values (`to (x) … end`) and
`add n to x`", with a decision on whether SPEC.md or the grammar moves.

## 3. A loop body that fails leaks its scope

**Where:** `src/vm.rs` — the `for` / `while` / `repeat` handlers do
`self.push_scope()`, then `for stmt in body { self.execute_statement(stmt)?; }`,
then `self.pop_scope()`. The `?` returns before the pop.

**Evidence.** One empty `IndexMap` leaks per iteration in which an error is
raised inside the loop body and caught outside it:

```
repeat 1000000 times
    try
        for each v in [1]
            set x to no_such_function()
        end
    catch error
    end
end
```

`self.locals` grows by one entry per outer iteration and is never truncated
except at a function-call boundary, which phase 011 added
(`Vm::call_user_function` truncates to the depth it started at). A loop long
enough to matter therefore grows the scope stack without bound while running.

Phase 011 does not fix it: the leak is in the loop handlers, not in the call
path, and a fix would touch three more handlers than this phase's concern.
The scope-stack truncation in `call_user_function` was needed here regardless,
because a caught failure inside a closure must leave the stack balanced
(`edge_a_caught_failure_inside_a_closure_leaves_the_stack_balanced`).

**Suggested phase:** "Balance the scope stack on an error out of a loop body".

## 4. Module members are still unreachable

**Where:** `src/vm.rs::load_module`.

An import now contributes only the module's `set` statements, which is what it
effectively contributed before: module functions were recorded in a flat map
keyed by name but never bound to a name, so `MathUtils.circle_area(5)` could
never resolve. Phase 011 removed that dead map (the body now lives in the
function value), so `load_module` says so in a comment rather than pretending
to record something.

```
$ ./target/debug/rb run modules/MathUtils.rb
Error: ParserError: Expected function name
  --> modules/MathUtils.rb:4:16
4 | constant PI to 3.14159
```

Note that `modules/MathUtils.rb` does not parse at all on `main` either —
`constant X to Y` is not in the grammar. `tests/test_modules.rb` and
`modules/SuiteKit.rb` are the shipped, passing surface.

**Suggested phase:** "Bind imported module members to a namespace".

## 5. A closure cannot be named from inside its own body when it is declared
   before anything that could bind the name

Not a defect, but a documented limit of capture-by-value: a nested declaration
reaches itself through the scope it was declared in, which is what
`a_closure_recurses_through_its_own_declaration_scope` (500 deep) and
`nested_declarations_reach_each_other` rely on. A program that wants two nested
declarations to call each other must declare the *second* one inside the scope
the first closes over, which is the natural order anyway. A forward reference —
`to outer() to even() … odd() … end end` with `odd` declared after `even` — is
reachable today only because `odd` is looked up in the live enclosing scope, not
in the capture.