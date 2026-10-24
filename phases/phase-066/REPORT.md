# Phase 066 — Settle engine arguments with evidence: `.compare` runs both engines

## What changed

| File | Lines | What |
|---|---|---|
| src/repl/commands.rs | +18 −0 | `ReplCommand::Compare(String)`, its command-table row (`:compare`/`.compare`), and the carried-argument arm |
| src/repl/mod.rs | +357 −0 | Dispatch and `compare_answer`: one frontend pass against the session's names, compilation to bytecode, a tree-walker fork, a fresh BC VM seeded with readable bindings, side-by-side rendering, AGREE/DIFFER, and eight tests |
| src/bytecode/vm.rs | +10 −0 | `seed_binding`, so the bytecode side starts from a copy of the session's names without borrowing the tree-walker |
| tests/repl_test.rs | +17 −0 | End-to-end `.compare 1 + 2` session asserting the verdict word |

`.compare` parses and analyzes once, compiles the same tree, then runs a
`Vm::fork` and a fresh `BytecodeVm`. The session's VM is never run and never
mutated. Both engines' `say` output is captured, so an effect appears once per
engine and never in the session's output or transcript. A parse, analyzer or
bytecode-codegen failure is reported once instead of comparing two absences.

## Tests added

| Test | Edge class covered |
|---|---|
| compare_prints_both_values_and_agree | singleton — one expression, two values, verdict word |
| edge_compare_reports_differ_when_one_engine_places_a_failure_elsewhere | type_mismatch / failure channel — `sqrt("x")` inside a call; tree-walker places it at 2:10, bytecode at 2:1, so DIFFER is printed |
| edge_compare_with_no_argument_is_a_usage_error | empty / malformed_input — a bare `.compare` and an all-whitespace expression are refused |
| edge_compare_leaves_the_session_vm_and_its_bindings_untouched | state/resource — bindings diffed before and after a comparison that reads one |
| edge_compare_runs_a_side_effect_once_on_each_engine | singleton/order — `say "tick"` appears exactly twice, never in the session |
| edge_compare_reports_a_compile_failure_once | malformed_input / failure channel — `1 +` reports one ParserError and no verdict |
| edge_compare_agrees_on_unicode_nesting_and_a_numeric_boundary | unicode + nesting — emoji/CJK/combining mark in nested list+record; `1 / 0` compared as a failure |
| edge_compare_runs_a_ten_kilobyte_program_on_both_engines | boundary/resource — ≥10KB source finishes on both engines and prints AGREE |
| compare_runs_both_engines_and_reports_agreement (tests/repl_test.rs) | dispatch — a real piped session answers `.compare`, not "Unknown command" |

### Mandatory edge-case matrix

- **empty** — covered: `edge_compare_with_no_argument_is_a_usage_error`; an empty/whitespace expression is refused, not compared.
- **singleton** — covered: `compare_prints_both_values_and_agree` asks about one expression and checks both values plus the verdict.
- **boundary** — covered: the same test checks each engine's value; `edge_compare_runs_a_ten_kilobyte_program_on_both_engines` is the source-size boundary.
- **out_of_bounds** — N/A: the command adds no index or slice of its own; a program's out-of-bounds access is compared, and both engines already refuse it.
- **type_mismatch** — covered: `sqrt("x")` is the DIFFER fixture; both engines refuse it and the comparison names both refusals.
- **numeric_boundary** — covered: `1 / 0` is compared as a failure on both engines in `edge_compare_agrees_on_unicode_nesting_and_a_numeric_boundary`.
- **unicode** — covered: `edge_compare_agrees_on_unicode_nesting_and_a_numeric_boundary` uses an emoji, CJK text and a combining mark through nested containers.
- **nesting_recursion** — covered for values by the nested list/record above. No recursion was added to the command; each engine's existing call-depth limit still applies.
- **duplicate_missing_keys** — N/A: the command does not read or build records; a record is a value passed to both engines unchanged.
- **malformed_input** — covered: `edge_compare_reports_a_compile_failure_once` and `edge_compare_with_no_argument_is_a_usage_error`.
- **resource_limit** — covered: the 10KB boundary test and each interpreter's existing step/call/output limits. `.compare` adds no loop or allocation proportional to input beyond one parsed tree and one chunk.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2090 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh phase-066 | pass — `VERIFY PASS — phase-066` |

## Invariants touched

- None. No grammar, keyword, `Value` variant, `Error` variant or `.rb` extension changed.
- `.compare` is REPL session state only, and it is read-only: the tree-walker runs on `Vm::fork`, the bytecode VM on a fresh VM seeded with copies.
- Existing tests, examples and modules are untouched.

## Known gaps / follow-ups

- The bytecode side is seeded with the session's readable bindings, not with its declared functions, objects or modules. Comparing an expression that calls a function the session declared reports DIFFER, because the bytecode VM only runs compiled bodies (`'f' has no compiled body, which only rb vm can run`). Compiling session declarations into the comparison is a separate capability, not a bug in the comparison.
- A failure's position is part of the comparison. A bytecode instruction carries a line and no column, so a runtime failure placed at column >1 by the tree-walker prints DIFFER even when the message is identical. This is the genuine difference the phase asks the command to be able to show.
