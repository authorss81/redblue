# phase-013 — FINDINGS

Defects found while bounding loops that are **not** this phase's concern. Each
has file:line evidence and is left for the auditor to promote.

## 1. `break` and `skip` are parsed and thrown away — `src/vm.rs:381`, `src/vm.rs:385`

Both statements are `Ok(Value::Nothing)` with a `// TODO: Implement proper
control flow`. So `break` is not a control-flow escape, and a `while` loop
cannot be left from inside Redblue at all. This phase made the runaway loop
*terminable from outside* (a `RuntimeError`) and *testable*, but the missing
`break` remains why a user-level loop escape does not exist.

Reproduced on `main` before this phase's edits — the loop runs to 5 because
`break` at `i is 2` does nothing:

```redblue
$ cat target/tmp/brk.rb
test "break is a no-op"
    set i to 0
    while i is not 5
        if i is 2 then
            break
        end
        set i to i + 1
    end
    say "i is now"
    say i
end
$ ./target/debug/rb test target/tmp/brk.rb
i is now
5                                        # expected 2: `break` was ignored
```

Worth noting for the phase that fixes it: once `break` works, `MAX_ITERATIONS`
becomes reachable as a *bug detector* rather than only as a safety net — a
well-written loop can never hit it.

## 2. `Statement::ForRange` is unreachable dead code — `src/parser.rs:134`

`ForRange` is declared, and handled by the analyzer (`src/analyzer.rs:108`), the
formatter (`src/formatter.rs:109`), the linter (`src/linter.rs:96`) and this
phase's VM arm — but **no parser arm constructs it**. `parse_for`
(`src/parser.rs:651`) only produces `ForEach`, and only for `for each x in <expr>`;
`for each i from 1 to 10` and `by` are documented in `SPEC.md:383` and
`SPEC.md:388` but not implemented.

```redblue
for each i from 1 to 5
    say i
end
```

This phase capped the `ForRange` VM arm anyway (`src/vm.rs` `Statement::ForRange`)
so the bound is in place if and when the parser starts building it. The missing
grammar belongs to a parser phase, not this one.

## 3. `modules/MathUtils.rb` and `modules/SuiteKit.rb` do not parse — pre-existing

```
$ ./target/debug/rb run modules/MathUtils.rb
Error: ParserError: Expected function name
  --> modules/MathUtils.rb:4:16
4 | to circle_area(radius)
  |                ^
```

A bare `to name(params) ... end` function declaration is not accepted at top
level, although `SPEC.md:375`-era module syntax and `src/vm.rs:159`'s own comment
("A module's functions are not bound to a name") both assume it. This is why
`shipped_examples_fit_inside_the_published_limits` walks `examples/` and not
`modules/`: the module files cannot be parsed on `main`, so asserting on them
would test a different phase's bug.

## 4. `rb format --check` is red on `main` for 22 of 24 `.rb` files — pre-existing

```
$ for f in examples/*.rb tests/*.rb modules/*.rb; do rb format --check "$f"; done
would reformat: examples/files.rb
would reformat: examples/fizzbuzz.rb
... 20 more
```

Confirmed pre-existing: this phase modified `src/vm.rs`, `src/lib.rs` and added
`tests/loop_bounds_test.rs` — no `.rb` file and no formatter code changed.

## 5. The test harness has no way to set a per-run budget — `src/testing/harness.rs:219`

`execute_test_program` calls `run_isolated`, which builds `Vm::new()`, so the
step budget for every `test` block comes from the process environment and is
identical for all of them. `Vm::with_max_steps` now exists, but nothing in
`src/testing/` uses it. A test file cannot declare "this test is allowed 200
steps"; a suite-wide budget has to come from `REDBLUE_MAX_STEPS` in the
environment. Deliberately not done here — giving the harness a budget syntax is
a language-surface change.
