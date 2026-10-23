# Phase 088 — Make the reachability-only tests assert the value they reach for

## What changed

The finding reproduces on the base commit: tests asked the two engines whether a
program *ran* and counted `Ok` as proof it was right. The earlier rounds of this
phase (already committed at `HEAD`, `45e3f05`) changed every named test to read
the value the run answered — its printed lines, the value it ended on, or the
refusal it returned — and closed the twenty reachability-only sites. This
continuation does not rewrite that work.

**This run** is judged against `HEAD`, as the resumed-work instructions require.
The preserved work is already committed, so the gate sees only the new diff. It
adds three tests to `tests/phase_088_value_contract_test.rs`
that re-point the remaining edges at the run that produces them:

1. **A refusal is named on each engine.** `pow("two", 3)` is refused by the
   library and by both engines with the same message, and neither prints
   anything before refusing. The test passes only when the run comes back `Err`.
2. **A loop body that must not run prints nothing on each engine.** Both
   zero-turn shapes — `repeat 0 times` and `for each x in []` — carry
   `say "never"` in the body; a body that ran one turn would print it.
3. **Empty and non-empty output both carry their value.** A quiet program and a
   talking program both end on `Number(3.0)` from `sqrt(9)`; the talking one
   prints exactly `3`. Neither output shape can hide a wrong value.

No production code changed. The defect was in the assertions, so the fix is in
the assertions.

| File | Lines (base → tree) | What |
|---|---|---|
| `tests/phase_088_value_contract_test.rs` | +145 −0 | **this run**: three `edge_*` tests — a named failure on both engines, a loop body that must not run, and empty vs. non-empty output held to the value the run answered |
| `tests/stdlib_module_docs_test.rs` | +252 −18 | earlier round (committed): the six builtins are asserted by literal output and value on both engines; wrong-type refusals name the builtin |
| `tests/loop_bounds_test.rs` | +202 −17 | earlier round (committed): `say "never"` plus `take_output()` on the unentered loops; exact counts on executed and nested loops |
| `tests/loop_control_test.rs` | +12 −1 | earlier round (committed): `break` asserts the number of turns taken |
| `tests/time_builtin_test.rs` | +12 −2 | earlier round (committed): `time.sleep(0)` asserts the value |
| `tests/redblue_test.rs` | +195 −15 | earlier round (committed): all five tests assert exact output, with four added value/output tests |
| `tests/text_split_join_test.rs` | +28 −4 | earlier round (committed): the `try … catch` fixture asserts the flag it caught |
| `tests/unless_test.rs` | +16 −4 | earlier round (committed): a trailing `unless` asserts the line it prints |
| `tests/failure_channel_test.rs` | +176 −0 | earlier round (committed): four failures — wrong type, out of bounds, unknown function on both engines, parser refusal |
| `tests/phase_088_value_contract_test.rs` | +513 −0 | earlier round (committed): values behind renderings, the failure channel from both sides, lexer/parser/runtime refusals |
| `tests/phase_088_scanner_test.rs` | +468 −0 | earlier round (committed): the phase's scanner, tested over fixtures and over `tests/` |
| `tests/unreviewed_work_ack_test.rs` | +24 −13 | earlier round (committed): the delivery rule is re-pointed at phase-088 |

The twenty reachability-only tests the finding named are each given a reachable
assertion or replaced by one, and none was deleted: the base scan named
`loop_bounds_test.rs` (9), `loop_control_test.rs` (1), `redblue_test.rs` (5),
`stdlib_module_docs_test.rs` (1), `time_builtin_test.rs` (1) and
`unless_test.rs` (1); in this tree the same scanner reports
`vacuous tests: 0`. The suite total rose, it did not fall: the base commit
`c6e9837` holds 1967 `#[test]` functions and this tree holds 2009 — the 39 tests
the earlier rounds added plus this run's three.

## The finding, reproduced

On the base commit, before any change:

```
$ printf 'say abs(-3)\nsay uppercase("hi")\nsay trim("  x  ")\nsay sqrt(9)\nsay text.uppercase("hi")\nsay text.split("a,b", ",")\n' > repro.rb
$ ./target/debug/rb run repro.rb
3
HI
x
3
HI
[a, b]
```

`tests/stdlib_module_docs_test.rs:242` ran those six programs and matched
`Ok(_) => {}`, so `sqrt` answering `1` or `split` answering one element would
have passed. In this tree the same six builtins are held to the literal lines
above and to their values on both engines.

The loop sites had the same shape: `tests/loop_bounds_test.rs:586-601` ran
`repeat "five" times` and `for each x in 5` with `say "never"` in each body but
never called `take_output()`. They now assert the output is empty, which the
literal `never` makes meaningful.

## Tests added in this run

| Test | File | Edge class covered |
|---|---|---|
| `edge_a_builtin_failure_is_named_by_each_engine` | `phase_088_value_contract_test.rs` | **failure is produced**; type mismatch (`pow("two", 3)`); the library and both engines refuse with the same message and no output |
| `edge_a_loop_body_that_must_not_run_prints_nothing_on_each_engine` | `phase_088_value_contract_test.rs` | empty/zero; a loop body that must not run (`say "never"`) on both engines |
| `edge_empty_and_non_empty_output_are_told_apart_by_their_values` | `phase_088_value_contract_test.rs` | empty output vs. one printed line; the value `Number(3.0)` behind both |

Every earlier test from the committed rounds remains in the tree, including the
failure assertions in `failure_channel_test.rs`,
`phase_088_value_contract_test.rs` and `stdlib_module_docs_test.rs`. Nothing was
skipped, ignored, or deleted.

## The edge-case matrix

| Row | Covered | Where |
|---|---|---|
| empty | covered | `edge_empty_and_non_empty_output_are_told_apart_by_their_values`; `edge_a_loop_body_that_must_not_run_prints_nothing_on_each_engine` |
| singleton | covered | `edge_a_builtin_that_answers_the_right_characters_and_the_wrong_value_is_caught` — `text.split("a", ",")` answers one element |
| boundary | covered | the same test — `text.split("a,b", ",")` answers two, not one |
| out_of_bounds | covered | `edge_an_index_past_the_end_of_a_list_is_refused` — index `3`, `5`, and `0` on an empty list, all named refusals |
| type_mismatch | covered | `edge_a_builtin_failure_is_named_by_each_engine` (`pow("two", 3)`); `edge_a_builtin_given_the_wrong_type_is_refused_by_the_library`; `edge_type_mismatch_count_is_not_a_loop` |
| numeric_boundary | covered | `1 / 0` and `0 / 0` refusals in `phase_088_value_contract_test.rs`; the full numeric matrix is pinned by `tests/numeric_edge_test.rs` |
| unicode | covered | `tests/unicode_escape_test.rs`; the builtin value tests hold CJK text unchanged |
| nesting_recursion | N/A | this phase changes what tests assert, not nesting or recursion; covered by `tests/loop_bounds_test.rs` and `tests/call_depth_test.rs` |
| duplicate_missing_keys | N/A | no record is read or written by any changed assertion; covered by `tests/record_order_test.rs` and `tests/index_bounds_test.rs` |
| malformed_input | covered | `edge_a_builtin_failure_is_named_by_each_engine` (bad argument type); lexer/parser refusals in `phase_088_value_contract_test.rs` |
| resource_limit | covered | the loop cap and step budget are pinned by value in `tests/loop_bounds_test.rs` |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — no warnings |
| `cargo test --all-targets` | **2009 passed, 0 failed, 0 ignored** across 74 test binaries |
| `../rbops/verify.sh phase-088` | **VERIFY PASS** |

```
$ grep "^test result:" target/tmp/full_test_after.log | awk '{p+=$4; f+=$6; i+=$8; n++} END {print p, f, i, n}'
2009 0 0 74
```

## Invariants touched

- **None.** No production code changed: `git diff --numstat HEAD -- src/` is
  empty. No language surface, no `Value` variant, no error kind, no grammar, and
  no `.rb` file changed.

## Known gaps / follow-ups

- The scanner reads `tests/*.rs` and follows the `tests/common/` helpers. The
  Redblue-level blocks in `tests/*.rb` are covered by a different gate
  (`tests/redblue_suite_test.rs:134` and `:207`), not by this scanner.
