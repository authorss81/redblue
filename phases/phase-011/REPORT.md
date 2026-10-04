# Phase 011 — Closure capture and lexical scoping

## Decision

**Real closures**, not an "unsupported" error. A function value now carries the
local scopes that were live where it was declared, plus its own body, so a
nested declaration reads its enclosing bindings and two declarations of one
name keep their own bodies.

The policy, decided and documented (AGENTS.md "shadowing, capture-by-value vs
by-reference"):

| Question | Decision | Why |
|---|---|---|
| Capture by value or by reference? | **By value**, copied at declaration time | `get_var` returns owned `Value`s and `set_var` has no cell to write back through; a reference capture would need shared mutable cells threaded through every binding. Copying at declaration is also the semantics a reader of `to make_adder(x)` expects: it closes over `x`. |
| Globals captured? | **No** — read live at call time | A function value that copied all ~100 builtins plus every global would be enormous and would freeze them. A closure that counts in a global counts for every call, which is the idiom `a_global_is_read_at_call_time_not_captured` pins. |
| Shadowing | Parameters above captures; captures above the caller's frames | See below. |
| Where do captured scopes sit? | **Above** the caller's frames | An escaped closure must read the environment it was written in, not a same-named binding of whoever called it. |
| Does a body assign to the enclosing scope? | **No** — it updates its own copy | Follows from by-value. `capture_is_by_value_so_an_assignment_does_not_escape`. |

A read of a name searches, innermost first: the current scope, the parameters,
the captured scopes, the caller's frames, then a global. A write now searches
the same stack (see "Invariants touched").

## Reproduction

The finding reproduces on `main` (`bd59fde`). `Value::Function` held a name and
a parameter list; the body lived in a flat `HashMap<String, Vec<Stmt>>` keyed by
that name. Three distinct failures, all reproduced before any edit:

```
$ cat target/tmp/nested2.rb
to make_counter(start)
    set count to start
    to inner()
        give back count
    end
    give back inner
end
set c to make_counter(5)
say c()
$ ./target/debug/rb run target/tmp/nested2.rb
nothing                                <- DEFECT: `count` is not in scope; the
                                         correct answer is 5
```

```
$ cat target/tmp/nested3.rb
to make_a(start)
    set count to start
    to inner()
        give back count + 1
    end
    give back inner
end
to make_b(start)
    set count to start
    to inner()                          <- the same nested name, twice
        give back count + 100
    end
    give back inner
end
set a to make_a(1)
set b to make_b(1)
say a()
say b()
$ ./target/debug/rb run target/tmp/nested3.rb
nothing                                <- DEFECT: one body map, keyed by name;
nothing                                   so the second `inner` overwrote the
                                         first. Correct answers: 2 and 101
```

```
$ cat target/tmp/alias2.rb
to add(a, b)
    give back a + b
end
set f to add
give back f(2, 3)
$ ./target/debug/rb run target/tmp/alias2.rb
                                     <- DEFECT: `nothing`. The call looked the
                                        body up by the *called* name, so an
                                        alias found no body. Correct: 5
```

The Redblue-language suite for this phase was run against the pre-fix build and
**11 of its 16 blocks failed** — they are not vacuous:

```
$ git stash -q && cargo build --quiet
$ ./target/debug/rb test tests/test_closures.rb
Test Results: 16 total, 5 passed, 11 failed, 0 skipped (31.2% success)
```

## What changed

| File | Lines | What |
|---|---|---|
| src/value.rs | +57 −3 | `CapturedScope`, `Captured`, `FunctionValue` (name, params, `Arc<Vec<Stmt>>` body, `Arc<Captured>` environment), a hand-written `PartialEq`, and `Value::Function(String, Vec<String>)` → `Value::Function(FunctionValue)` |
| src/vm.rs | +63 −37 | `locals` is `Vec<CapturedScope>`; the `functions` map is gone (the body lives in the value); new `Vm::make_function` captures the live scope stack; `call_user_function` pushes captures then parameters and truncates the stack on the way out; `set_var` resolves a name the way `get_var` does |
| src/lib.rs | +4 −1 | `pub use value::{FunctionValue, Value}` — the payload of a public variant has to be nameable |
| src/testing/runner.rs | +1 −1 | match arity for `Value::Function` |
| tests/closure_capture_test.rs | +703 (new) | 26 `#[test]` functions |
| tests/test_closures.rb | +267 (new) | 16 Redblue `test` blocks |

Total production diff: **+125 −42** across four files.

### Why the body moved into the value

`call_user_function` looked the body up by the *name being called*. Keeping
`functions: HashMap<String, Vec<Stmt>>` and adding an environment beside it would
still leave two nested declarations of one name sharing one body. The body is
part of what a function *is*, so it goes in the value and the map is deleted —
which also removes the "bound to a function value but with no retained body"
fallback that used to answer `nothing` for an aliased call.

`Arc`, not `Rc`: `run_isolated` moves the VM across a thread boundary, so
`Value` must stay `Send`. The `Arc` also keeps cloning a function value cheap,
which matters because *every* variable read clones the value bound to it and a
recursive call reads the name it was reached through.

### Why the stack is truncated rather than popped

```rust
let caller_depth = self.locals.len();
for scope in function.captured.iter() { self.locals.push(scope.clone()); }
self.push_scope();
...
self.locals.truncate(caller_depth);
```

A body that fails part way through can leave a scope behind (see FINDINGS.md §3),
so popping a fixed number of times would not balance the stack. Truncating to
the depth the call started at balances it on every exit path, including an
error a `try` catches — which is what
`edge_a_caught_failure_inside_a_closure_leaves_the_stack_balanced` asserts.

## Tests added

`tests/closure_capture_test.rs`, 26 `#[test]` functions (floor: 3). 15 are
named `edge_*` (floor: 1). 4 assert that a failure is produced — two through
`eval_err`, two through a caught error whose catch they assert.

| Test | Edge class covered |
|---|---|
| `nested_declaration_sees_the_enclosing_binding` | the finding itself; red before the fix |
| `edge_escaped_closure_reads_its_own_environment_not_the_callers` | scope-order: a caller's `x` must not be read |
| `same_nested_name_declared_twice_keeps_both_bodies` | duplicate declaration — the old body-by-name collision |
| `a_parameter_shadows_what_the_declaration_captured` | shadowing, both directions |
| `capture_is_by_value_so_an_assignment_does_not_escape` | capture-by-value; fails without the `set_var` fix |
| `a_global_is_read_at_call_time_not_captured` | globals not in the capture |
| `three_levels_of_nesting_each_capture_the_level_outside` | nesting depth 3 (DoD) |
| `nested_declarations_reach_each_other` | mutual recursion between nested declarations (DoD) |
| `a_closure_recurses_through_its_own_declaration_scope` | recursion 500 deep through a closure's own name |
| `top_level_mutual_recursion_is_unchanged` | mutual recursion, top level (regression guard) |
| `a_function_value_called_through_an_alias_runs_its_own_body` | the alias defect |
| `edge_two_closures_in_one_list_keep_separate_captures` | two closures in one aggregate |
| `edge_a_closure_in_a_record_with_duplicate_and_missing_keys` | duplicate_missing_keys — repeated key, missing field, function in a record |
| `edge_empty_captured_values_survive` | empty — `""` captured, `[]` returned |
| `edge_a_declaration_with_no_captured_bindings_is_a_function` | empty — a closure with nothing to capture |
| `edge_numeric_boundaries_inside_a_closure_are_unchanged` | numeric_boundary — `2^53`, and **asserts failure**: `x / 0` stays a `Runtime` error |
| `edge_unicode_capture_survives` | unicode — CJK + emoji captured and returned |
| `edge_a_function_value_displays_as_its_declaration_name` | boundary — display and `type_of` of a function value |
| `edge_a_closure_calling_an_undefined_name_is_a_caught_error` | **asserts failure** — exact message, `Unknown function 'no_such_helper'` |
| `edge_a_caught_failure_inside_a_closure_leaves_the_stack_balanced` | resource_limit + **asserts failure** — a caught division by zero, then the program continues |
| `edge_unbounded_closure_recursion_is_a_caught_error` | resource_limit + **asserts failure** — depth limit reached twice, stack still usable |
| `edge_a_closure_declared_in_a_loop_captures_that_iteration` | nesting — each iteration captures its own `n` |
| `edge_the_same_nested_name_in_two_sibling_scopes_stays_separate` | duplicate declaration across sibling scopes |
| `edge_assigning_to_an_enclosing_binding_reaches_that_binding` | the `set_var` resolution change: a parameter assigned from a loop body, and that a plain assignment still lands in a global |
| `edge_two_closures_over_one_name_do_not_share_an_assignment` | by-value isolation between two closures over one name |
| `a_hand_built_function_value_is_usable_and_displayed` | the public `Value::Function` payload is nameable and displayable from Rust |

`tests/test_closures.rb`, 16 Redblue `test` blocks (floor: 2), 8 named
`edge_closures_*`, 25 `expect` assertions across the 16 blocks, 4 of which use
`try … catch error`. They cover the same matrix in the language itself and are
run by `rb test` and by `tests/redblue_suite_test.rs`, which fails a `test` block
that asserts nothing.

## Edge-case matrix

- **empty** — covered: `edge_empty_captured_values_survive` (captured `""`,
  captured `[]` returned), `edge_a_declaration_with_no_captured_bindings_is_a_function`
  (nothing to capture).
- **singleton** — covered: `a_parameter_shadows_what_the_declaration_captured`
  is a one-parameter closure over one captured binding;
  `edge_two_closures_in_one_list_keep_separate_captures` is a two-element list
  indexed at `0` and `1`.
- **boundary** — covered: `a_parameter_shadows_what_the_declaration_captured`
  (parameter over capture, the exact shadowing boundary);
  `edge_a_function_value_displays_as_its_declaration_name` (display and
  `type_of` of a function value, which must not leak the capture);
  `edge_a_closure_declared_in_a_loop_captures_that_iteration` (a declaration
  inside a scope that is pushed and popped per iteration).
- **out_of_bounds** — N/A + why: this phase adds no indexing, slicing or
  collection access. `both[0]` / `both[1]` in these tests go through
  `Expr::Index`, whose bounds behaviour is unchanged and is covered by
  `tests/index_bounds_test.rs` (phase 008). A closure cannot be indexed.
- **type_mismatch** — covered at the level this phase can reach:
  `edge_numeric_boundaries_inside_a_closure_are_unchanged` asserts that
  `x / 0` inside a closure body is a `Runtime` error rather than a value or a
  panic; `edge_a_closure_calling_an_undefined_name_is_a_caught_error` asserts
  that a name which is not a function is a clean `Runtime` error naming it.
  Value-level type coercion is untouched and stays in
  `tests/numeric_edge_test.rs` (phase 007).
- **numeric_boundary** — covered: `edge_numeric_boundaries_inside_a_closure_are_unchanged`
  divides a number one past `2^53` inside a closure and asserts the exact
  result, plus the division-by-zero failure.
- **unicode** — covered: `edge_unicode_capture_survives` captures CJK text and
  returns it concatenated with an emoji, byte-for-byte through the capture.
  Lexer-level Unicode behaviour is covered by `tests/lexer_robustness_test.rs`
  (phase 009) and is untouched.
- **nesting_recursion** — covered: three levels of nested declarations each
  capturing the level outside (`three_levels_of_nesting_each_capture_the_level_outside`),
  a closure recursing 500 deep through its own name, and mutual recursion both
  between nested declarations and at top level.
- **duplicate_missing_keys** — covered:
  `edge_a_closure_in_a_record_with_duplicate_and_missing_keys` — a record with a
  repeated key (last wins), a field that is not there (`nothing`), and a
  closure held in a field; `same_nested_name_declared_twice_keeps_both_bodies`
  and `edge_the_same_nested_name_in_two_sibling_scopes_stays_separate` cover
  the duplicate-*declaration* form of the same question, which is the one this
  phase actually changed.
- **malformed_input** — covered at the only level this phase can reach:
  `edge_a_closure_calling_an_undefined_name_is_a_caught_error` and
  `edge_a_caught_failure_inside_a_closure_leaves_the_stack_balanced` drive
  runtime failures through a closure and assert the exact message and the
  continuing program. Malformed *source* is the parser's concern and is
  unchanged and covered by `tests/parser_hardening_test.rs` (phase 010).
- **resource_limit** — covered: `edge_unbounded_closure_recursion_is_a_caught_error`
  (the call-depth limit, twice, with the stack still usable afterwards) and
  `edge_a_caught_failure_inside_a_closure_leaves_the_stack_balanced` (the
  stack is truncated on the error path). The one unbounded path left — a loop
  body that fails leaks its scope — is FINDINGS.md §3 and is not reachable
  through this phase's own change.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass (0 warnings) |
| `cargo test --all-targets` | pass — **197 passed, 0 failed, 0 ignored** (26 of them new) |
| `./rbops/verify.sh phase-011` | **not run — `rbops/` is not present in this checkout** |

Per binary: 6 (lib), 0 (main), 11 (call_depth), **26 (new
closure_capture)**, 13 (discovery), 21 (expect), 8 (index_bounds), 16 (lexer
robustness), 20 (numeric edge), 23 (parser hardening), 25 (record order), 8
(redblue suite), 5 (redblue), 15 (span). `cargo test --doc` is 0 doc-tests;
`cargo test` (with doc-tests) is green with the same 197.

On the fourth gate: `rbops/` does not exist in the working directory
(`ls: cannot access 'rbops': No such file or directory`), and hard rule 1
forbids creating anything under `rbops/`. What the gate is documented to add on
top of the three above — the examples and the Redblue suite — was run directly:

```
$ for f in examples/*.rb modules/SuiteKit.rb; do ./target/debug/rb run "$f" || echo "FAIL $f"; done
$ ./target/debug/rb test
Tests run: 219
Passed: 219
Failed: 0
```

All six examples and `modules/SuiteKit.rb` exit 0. The Redblue suite went from
203 to 219 passing blocks (16 added, 0 lost). `modules/MathUtils.rb` fails to
parse on `main` as well (`constant X to Y` is not in the grammar) — verified by
stashing this phase's changes and re-running it, so it is not a regression;
FINDINGS.md §4. `rb lint` exits 0 on all 12 `tests/*.rb`.

## Invariants touched

- None of the language invariants in AGENTS.md §2. `.rb`, `to … end`,
  `set x to <expr>`, `say`, the `Value` and `Error` variant *names*, and the
  trailing-comma / `{interp}` string syntax are all unchanged. No public type
  was renamed. The only public addition is `redblue::FunctionValue`, which had
  to become nameable because it is now the payload of `Value::Function`.
- **The payload of `Value::Function` changed**, from
  `Function(String, Vec<String>)` to `Function(FunctionValue)`. The variant
  name is unchanged; the two fields are now four fields of a named struct, and
  `FunctionValue` additionally carries the body and the captured scopes. A Rust
  caller that matched or constructed `Value::Function` must be updated — the
  variant list in §2 is preserved, the payload is not.
- **`set x to v` now resolves a name the way reading it does.** It binds in the
  innermost live scope that already has the name, and in a global when no local
  scope does. It used to bind only in the innermost scope and fall straight
  through to a global otherwise, so assigning to a variable of an *enclosing*
  scope — a parameter from inside a loop body, or a captured name from inside a
  function that closed over it — silently created a global of that name. This
  is required by capture-by-value: without it a body assigning to a captured
  name writes a global that every later call reads.
  `edge_assigning_to_an_enclosing_binding_reaches_that_binding` pins both halves
  (`add_to(10)` is now `13`, was `10`; a plain assignment still lands in a
  global and is readable afterwards).
- **Three cases that answered `nothing` now answer the right value**: a call to
  a closure returned from another scope, a call to an aliased function, and a
  call to one of two same-named nested declarations. All three were the finding.
- The scope stack is now truncated to the depth a call started at on every exit
  path. Previously a call popped exactly one scope and a body that failed could
  leave more behind than it found.

## Known gaps / follow-ups

- **`give back` still does not leave a function** (FINDINGS.md §1). Every
  recursive program in this language, including the closure tests here, has to
  compute a conditional result into a variable and return it after the block.
  This is the largest SPEC.md/behaviour gap found and is not this phase's
  concern, but it shapes how these tests are written.
- **The anonymous closure form in SPEC.md does not parse** (FINDINGS.md §2).
  Named nested declarations are now real closures; `give back to … end` and
  `add 1 to count` are still not in the grammar and were deliberately left
  alone (hard rule 8). SPEC.md § Closures therefore still describes something
  the interpreter cannot parse, and README/ROADMAP were not updated to claim
  otherwise.
- **A loop body that fails leaks its scope** (FINDINGS.md §3) — pre-existing,
  in code this phase does not otherwise touch.
- **Capturing is a copy, so it is O(size of the visible scopes) per
  declaration.** A declaration inside a long loop copies the scopes per
  iteration. It is linear in total work, and the alternative (shared cells) is
  the reference-capture design this phase rejected.
- **Two function values are equal when their name, parameters and captured
  bindings are equal.** `Stmt` has no `PartialEq`, so the body is not compared;
  every value built by one declaration shares that body anyway. Documented on
  the `impl` and in `a_hand_built_function_value_is_usable_and_displayed`.
- **Importing a module contributes only its `set` statements** (FINDINGS.md §4).
  Unchanged in effect; the dead body map it used to fill is gone, and
  `load_module` says so instead of pretending.