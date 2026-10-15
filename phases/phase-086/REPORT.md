# Phase 086 — Make `xs.map(f)` work when the receiver is a variable

Two rounds: the phase's own fix, then the round-1 review pass, which closed one
blocker and the two smaller findings. Both are in this tree.

## Part 1 — the phase's fix

The receiver path is `object → module → list`. The list step is a **fallback**,
asked only after the module step has answered nothing, and only for `map` and
only for a name holding a `Value::List`.

### Reproduction, before the fix

```
$ rb run spec647.rb          # set doubled to numbers.map(double)  -- SPEC.md:647
Error: RuntimeError: Module 'numbers' has no function 'map'
```

Both engines refused; `[1, 2, 3].map(f)` printed `[2, 4, 6]` throughout, which is why
the gap survived.

| File | What |
|---|---|
| `src/stdlib.rs` | `pub fn map_function(args: &[Value]) -> Result<&FunctionValue, String>` — the single place both engines get the callee for a method-spelled `map`, and the single place the bad-argument message is worded |
| `src/interpreter.rs` | `Vm::call_method` asks the variable receiver's **value** for `map` after the module path has answered nothing; the object-receiver path shares `stdlib::map_function` |
| `src/bytecode/vm.rs` | the same fallback in `BytecodeVm::call_method`, on the same side of the module check; the object-receiver path shares the same helper |
| `tests/method_map_test.rs` | new file, every case asked on **both** engines |
| `tests/differential_test.rs` | `edge_a_higher_order_call_answers_the_same_way_on_both_engines` flipped its name-receiver case from "still refused" to asserting the answer, and gained a module-collision case and a number-receiver case |
| `tests/doc_blocks_known_fail.txt`, `tests/doc_blocks_runs.txt` | `SPEC.md:647` moved out of known-fail (`RuntimeError: Module 'numbers' has no function 'map'`) into runs |

### Verification, on the built binary

```
$ rb run spellings.rb
[2, 4, 6]      # xs.map(double)
[2, 4, 6]      # list.map(xs, double)
[2, 4, 6]      # [1, 2, 3].map(double)
[2, 4, 6]      # map(xs, double)
$ rb compile spellings.rb -o spellings.rbc && rb vm spellings.rbc
[2, 4, 6] / [2, 4, 6] / [2, 4, 6] / [2, 4, 6]
```

### Invariants the phase's fix touches

- None. No grammar change, no `Value` variant added or removed, no change to
  `Error::{Lexer,Parser,Analyzer,Runtime,Io}`, no change to `.rb`, `to…end`, `set … to`,
  `say`, trailing commas or `{interp}`.
- `map` gains a fourth spelling. `map(xs, f)`, `list.map(xs, f)` and `[1, 2, 3].map(f)`
  are untouched and are asserted against the same literal in
  `edge_every_spelling_of_map_answers_the_same_list`.
- The fallback runs **after** the module path, so no `module.member` call changed.

## Part 2 — round 1 review: what the reviewer found and what was done

The reviewer's three findings are written up in full in `FINDINGS.md`. In short:

### BLOCKER — `rb compile` refused a function literal, so SPEC.md:647 was not compilable

`src/bytecode/codegen.rs` lowered `Expr::FunctionLiteral` to a
`ParserError` — `` `to (x) ... end` as an expression is not compiled yet `` — so
SPEC.md's headline example ran under `rb run` and could not be compiled at all.
The test that claimed to cover it ran the **block form** on the compiled engine,
so it was checking a different program from the one SPEC.md prints, and
`tests/doc_blocks_test.rs` ran only `rb run`, so nothing in the suite could see
the refusal. `SPEC.md:647` sat in `tests/doc_blocks_runs.txt` while `rb compile`
could not build it.

**Fixed, not carried.** A literal is a declaration written where a value goes,
so `Compiler::expr` now compiles it to a nested `BlockKind::Function` block —
named `interpreter::ANONYMOUS_FUNCTION`, carrying the literal's parameter names
and body — and emits `DEF_FUNCTION`, which pushes the closure. The `STORE` that
follows `DEF_FUNCTION` in the *statement* path is the only thing that popped it;
a literal has no name to bind, so nothing follows and the closure is left on the
stack as the value of the enclosing expression. That required threading the
enclosing block's `blocks` and `depth` into `expr` — the whole of the refactor,
since every other expression in the grammar is a value already and needs neither.

```
$ rb run spec647.rb                      # SPEC.md:647, verbatim
[2, 4, 6]
$ rb compile spec647.rb -o spec647.rbc && rb vm spec647.rbc
Compiled spec647.rb -> spec647.rbc
[2, 4, 6]
```

**And the gate that could not see it now can.** `tests/doc_blocks_test.rs` gained
`compile_block`, and the RUNS bucket is checked with it: a RUNS block has to exit
0 under `rb run` **and** exit 0 under `rb compile`. All 50 RUNS blocks compile
today, so nothing moved between buckets; what changed is that the next block that
cannot be compiled fails this suite instead of being listed as working
documentation. The assertion was checked with teeth — inverted, it fails on the
first block with the compile error and the block's body in the message.

**One thing this exposed.** A new program shape became reachable, and it
disagreed with the other engine on one message: a call-depth failure inside a
*recursive* literal. `Vm::call_user_function` is handed the name the call site
spelled, so the tree-walker says `while calling 'loop'`; `BytecodeVm::call_function`
reported `function.name`, which for a literal is the anonymous name.
`call_function` now takes `called_as` and reports that. For every named
declaration the two names are the same string, so no existing message changed.

### MAJOR — the `many` arm of `map_function` had no test on either engine

`tests/method_map_test.rs` asserted `xs.map(5)` and `xs.map()` and nothing
anywhere called `xs.map(f, g)`. `edge_a_map_given_a_second_function_is_refused_the_same_way_by_every_spelling`
now asks all three spellings on both engines, plus three arguments so the count is
read rather than matched against a fixed `2`, plus a non-list receiver to show the
count belongs to the function side and not to the receiver.

### MINOR — one operation, two spellings, two errors

The flat `map(xs, f, extra)` said `map requires a list and a function` — naming
neither the extra argument nor its count, and false when the list was supplied —
while `xs.map(f, extra)` said `map requires a function, but was given 2 arguments`.
`Vm::map_builtin` and `BytecodeVm::call_named` now destructure the list and hand the
**rest** to `stdlib::map_function`, which both engines already shared:

| call | both engines say |
|---|---|
| `map(xs)` · `xs.map()` · `list.map(xs)` | `map requires a function, but no argument was given` |
| `map(xs, 5)` · `xs.map(5)` | `map requires a function, but was given 5` |
| `map(xs, f, g)` · `xs.map(f, g)` · `list.map(xs, f, g)` | `map requires a function, but was given 2 arguments` |
| `map(5, f)` · a non-list receiver | `map requires a list and a function` |

Only a **first** argument that is not a list still says
`map requires a list and a function`, because that is the receiver and no argument
check could describe it.

### The one existing assertion that changed

`tests/function_literal_test.rs::edge_calling_map_without_a_function_fails`
recorded `map([1, 2, 3])` as `map requires a list and a function`. That call *does*
supply a list, so the old message was the misleading half. It now asserts the
unified wording, `map requires a function, but no argument was given` — the same
message `xs.map()` gives for the same mistake. The test is not deleted, skipped or
weakened: it still runs on both engines and still asserts the exact string.

### Files changed in round 1

| File | ± | What |
|---|---|---|
| `src/bytecode/codegen.rs` | +74 −45 | `expr` takes `blocks` and `depth`; `Expr::FunctionLiteral` lowers to a nested function block plus `DEF_FUNCTION` instead of returning a `ParserError` |
| `src/bytecode/vm.rs` | +26 −7 | `call_function` reports the call-site name in a call-depth failure; the flat `map`/`list_map` path shares `stdlib::map_function` for the argument side |
| `src/interpreter.rs` | +12 −1 | `map_builtin` matches the list and hands the rest to `stdlib::map_function`, so all four spellings word the refusal the same way |
| `tests/doc_blocks_test.rs` | +77 −1 | `compile_block`, and RUNS means "runs **and** compiles" |
| `tests/function_literal_test.rs` | +261 −1 | `compiled`/`both`/`both_fail` helpers and 4 `edge_*` tests pinning the literal lowering on both engines |
| `tests/method_map_test.rs` | +76 −27 | SPEC.md:647 compiled **verbatim** instead of in block form; 1 new `edge_*` test for the `many` arm |
| `tests/doc_blocks_runs.txt`, `tests/doc_blocks_fragments.txt`, `tests/doc_blocks_known_fail.txt` | ±comment | the header says what RUNS now means; three stale counts corrected to the real 50 / 39 / 14 of 103 |
| `tests/random_builtin_test.rs`, `tests/stdlib_module_names_test.rs` | ±comment | two comments that cited the compiler refusal as the reason for a choice |

### Tests added in round 1

| Test | What it pins |
|---|---|
| `method_map_test::edge_the_specification_example_of_a_first_class_function_runs` | SPEC.md:647 **verbatim** — literal and all — compiles and prints `[2, 4, 6]` on both engines |
| `method_map_test::edge_a_map_given_a_second_function_is_refused_the_same_way_by_every_spelling` | the `many` arm, on three spellings × both engines; three arguments so the count is read; a non-list receiver so the count is not the receiver's |
| `function_literal_test::edge_a_literal_is_lowered_on_both_engines` | 11 shapes — one-line and block spelling, inline as an argument, global read at call time, local captured by value, a parameter shadowing, a literal nested in a literal, one read out of a list and out of a record, no parameters, empty body — each asserted *against the tree-walker* |
| `function_literal_test::edge_a_literal_maps_in_every_spelling_on_both_engines` | all four `map` spellings with a literal callee on both engines |
| `function_literal_test::edge_a_literal_refused_by_map_is_the_same_diagnostic_on_both_engines` | a bad argument and a body that fails on an element are the same diagnostic on both |
| `function_literal_test::edge_a_recursive_literal_is_named_by_its_call_site_on_both_engines` | a recursive literal is reported under `loop`, not `anonymous function`, and stopped by `Error::Limit` on both — a compiler that inlined the body would overflow the host stack instead |

### Test-requirement matrix (AGENTS.md §3), after round 1

| Row | Status |
|---|---|
| empty | covered — `edge_an_empty_and_a_singleton_receiver_answer`, `edge_a_receiver_whose_value_is_nothing_is_refused` |
| singleton | covered — same test; also the singleton element of `edge_a_list_of_records_maps_…` |
| boundary | covered — the four `map` spellings, the module-vs-list boundary (`list` has a `map` member, `text` does not), and the argument *counts* 0 / 1 / 2 / 3 |
| out_of_bounds | **N/A** — `map` never indexes a position; the receiver is a whole value and the callee decides what to do with each element. Unchanged by this phase and covered by `tests/index_bounds_test.rs` and `tests/loop_bounds_test.rs`. |
| type_mismatch | covered — `edge_a_bad_argument_to_a_variable_receiver_map_is_a_named_runtime_error`, `edge_a_map_given_a_second_function_…`, `edge_a_receiver_that_is_not_a_list_is_still_refused`, `edge_a_list_of_records_maps_…` |
| numeric_boundary | **N/A** — no arithmetic is added; the only numeric boundary this phase could affect is the argument count in the refusal, asserted for 0, 1, 2 and 3. |
| unicode | covered — `edge_a_variable_receiver_holding_unicode_text_maps_element_by_element` |
| nesting_recursion | covered — `edge_a_variable_receiver_map_nests_and_recurses_within_the_call_limit`, `edge_a_list_in_a_record_field_and_one_from_a_call_both_answer`, `edge_a_recursive_literal_is_named_by_its_call_site_on_both_engines` |
| duplicate_missing_keys | covered — `edge_a_list_of_records_maps_and_a_missing_field_is_a_runtime_error` |
| malformed_input | **N/A** — no lexer or reader code is touched. A literal that does not parse is still a `ParserError`, and `edge_an_unterminated_literal_names_the_missing_end` pins it on the tree-walker; the compiler refuses nothing a parser accepted, so a block that runs is a block that compiles (`tests/doc_blocks_test.rs`). |
| resource_limit | covered — `edge_a_variable_receiver_map_nests_and_recurses_within_the_call_limit` and `edge_a_recursive_literal_…`, both on both engines; `MAX_CALL_DEPTH` is reached as an error, not a thread overflow |

Quota in round 1: 5 new `#[test]`s, 5 named `edge_*`, 3 assert a failure is produced
on both engines, zero new `#[ignore]` / `// skip` / `allow(clippy::`, zero tests deleted.

## Gates

Run from the project root on this tree, after round 1.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** (no diff) |
| `cargo clippy --all-targets -- -D warnings` | **pass** (exit 0, no warnings) |
| `cargo test --all-targets --no-fail-fast` | **1598 passed, 0 failed, 0 ignored** across 61 test binaries |
| `./rbops/verify.sh phase-086` | **not run — `rbops/` does not exist in this checkout** |

`cargo test` was run with `--no-fail-fast` so the count is the whole suite rather than the
run stopping at the first failure. 1593 before round 1, +5 new tests.

The fourth gate could not be run: this working directory has no `rbops/` directory at all
(`ls rbops/` → `No such file or directory`), so the script the contract names is not present
here. The three gates that could be run are green. If `verify.sh` reports anything further,
this report does not claim otherwise.

## Known gaps / follow-ups

- **`SPEC.md:710` runs but does not answer the same compiled.** The block is
  `set Person.name to "Alice"` with no `object Person` declaration above it. `rb run`
  exits 0; `rb compile` + `rb vm` exits non-zero with `Unknown variable 'Person'`. That is
  a pre-existing divergence between the two engines, not one this phase introduced — it is
  reproducible from `a721434` and none of this round's diff touches property assignment — and
  the RUNS bucket deliberately checks only that a block *compiles*, not that the compiled
  program agrees, because that is `tests/differential_test.rs`'s job. Recorded here rather
  than fixed: it is a receiver-dispatch bug on `set`, not a `map` one.
- The fallback is written for `map` only. `filter`, `reduce` and friends are not builtin
  in Redblue yet, so there is nothing for them to reach; if they arrive, the same shape
  belongs on a member that is also a list method.
- A name holding a list called as `list.map(f)` (where `list` is also a module with a
  `map` member) is still refused, as `list_map` with one argument. That is the pre-existing
  behaviour and is pinned by `edge_a_receiver_named_after_a_module_maps_the_list_it_holds`
  rather than changed.
