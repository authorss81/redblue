# Phase 003 — Write a real Redblue test suite

## Re-verification on resume

This report was written by a first run and then **independently re-verified by a
second run** on the recovered tree (`da2ffba`, whose parent is `ee528e1`, the
`main` this phase was measured against). Nothing was rewritten. The second run
re-executed every gate from scratch and re-ran both mutation checks; every number
below is what was measured, not what the first run claimed. The two probes were
created, measured, and deleted again — `git status` is clean of them.

| Claim in this report | Re-measured on `da2ffba` |
|---|---|
| `cargo fmt --all -- --check` passes | pass, 0 diff |
| `cargo clippy --all-targets -- -D warnings` passes | pass, 0 warnings |
| `cargo test --all-targets` = 67 passed | 67 passed across 7 binaries: 5 + 0 + 13 + 21 + 8 + 5 + 15 |
| `rb test` = 187 run, 187 passed, 0 failed, 0 skipped | 187 run, 187 passed, 0 failed, 0 skipped, exit 0 |
| 187 `test` blocks, 322 `expect`/`assert.` lines | 187 blocks, 322 assertion lines |
| 57 blocks named `edge_*` | 57 |
| 24 blocks use `try`/`catch error` | 24 (per-file: modules 7, arithmetic 5, suite 3, lists 2, records 2, text 2, integration 2, functions 1) |
| no tautological `expect x to be x`, no `expect nothing to be nothing` | 0 matches for either |
| `examples/*.rb` all exit 0 | all 6 exit 0 |
| only `src/testing/harness.rs` touched in `src/` | confirmed by `git diff ee528e1..HEAD --stat` |
| no live test removed | the only non-comment deletions in `tests/*.rb` are 3 `say` lines inside commented placeholders |

The mutation checks were repeated, not trusted:

- Dropping `tests/zz_probe.rb` containing `test "zz_mutation_probe"` with
  `expect x to be 2` (where `x` is 1) → `rb test` printed `Tests run: 188`,
  `Failed: 1` and exited **1**.
- Dropping the same file with no `expect` at all →
  `every_test_block_carries_an_assertion` **FAILED**, panicking at
  `tests/redblue_suite_test.rs:146`.
- Both probes were removed; `rb test` is back to 187/187 and `cargo test` to 67.

## Finding re-verified

The phase prompt's evidence still reproduces on `main` (`ee528e1`). Before this
change, `rb test` reported **22 tests, 21 passed, 0 failed, 1 skipped** while
every one of those tests asserted nothing:

```
$ cargo run --bin rb -- test
.....SKIP: // skip "Awaiting full test harness implementation" - // Reason: ...
................Tests run: 22
Passed: 21
Failed: 0
```

Two independent causes, both fixed:

1. `tests/suite.rb`, `tests/test_arithmetic.rb` and `tests/integration_test.rb`
   shipped with 21 `test` blocks entirely inside `//` comments — zero live
   Redblue test blocks in the repository.
2. `TestHarness::run_source` (`src/testing/harness.rs`) only recognised the
   comment-marker convention `// test "name"` / `// end`. It sliced the
   *commented* body lines out and handed them to the lexer, where they lex to an
   empty program — so a commented-out test reported as a **pass**. This is why
   the count was non-zero while the assertion count was zero.

The red test came first. `tests/redblue_suite_test.rs` was written and run
against the unmodified tree; 8 of its 8 tests failed:

```
test edge_suite_asserts_that_failures_are_produced ... FAILED
test edge_suite_contains_named_edge_tests ... FAILED
test edge_suite_covers_every_required_area ... FAILED
test edge_suite_declares_no_skip_markers ... FAILED
test edge_suite_files_are_not_all_commented_out ... FAILED
test edge_suite_reports_at_least_40_redblue_tests ... FAILED   # found 22
test edge_suite_runs_no_skipped_tests ... FAILED                # 1 skipped
test every_test_block_carries_an_assertion ... FAILED
test result: FAILED. 0 passed; 8 failed
```

## What changed

| File | Lines | What |
|---|---|---|
| `src/testing/harness.rs` | +93 −21 | `run_test_blocks`: discover real `test "..." ... end` blocks from the parser and run each body on its own `Vm`. `report_test` extracted so both discovery paths share one outcome path. `execute_test_statements` / `execute_test_program` split out of `execute_test_code`. `declares_test_block` keeps the marker convention working for `tests/discovery_test.rs` and `tests/expect_test.rs`. |
| `tests/redblue_suite_test.rs` | +243 (new) | 8 Rust gate tests over the shape of the Redblue suite (below). |
| `tests/test_arithmetic.rb` | +155 −21 | 22 blocks. Was 4 commented placeholders. |
| `tests/test_text.rb` | +149 (new) | 23 blocks. |
| `tests/test_lists.rb` | +167 (new) | 21 blocks. |
| `tests/test_records.rb` | +140 (new) | 21 blocks. |
| `tests/test_control_flow.rb` | +203 (new) | 22 blocks. |
| `tests/test_functions.rb` | +136 (new) | 14 blocks. |
| `tests/test_objects.rb` | +130 (new) | 15 blocks. |
| `tests/test_modules.rb` | +110 (new) | 10 blocks. |
| `tests/suite.rb` | +219 −70 | 19 cross-area blocks. Was 12 commented placeholders. |
| `tests/integration_test.rb` | +182 −29 | 20 whole-program blocks. Was 5 commented placeholders and the `// skip` marker. |
| `modules/SuiteKit.rb` | +15 (new) | A module file that parses, so `import` has a resolvable target. Without it the only module in the repo, `modules/MathUtils.rb`, cannot be imported at all (FINDINGS.md §6). |

Net Redblue suite: **187 test blocks, 0 skipped, 187 passing** (was 22
collected, 21 vacuous, 1 skipped). Every block carries at least one `expect`.

### The production change, in full

`run_source` now parses the file and, when the parse succeeds, runs each
`Statement::Test` the parser produced — as a one-statement program on a fresh
`Vm`, analysed on its own. Block boundaries come from the parser, so a nested
`if`/`for`/`try`/`object`/`to` inside a test cannot truncate the block. If the
file does not parse **and** it declares a real `test` block, the parse fault is
recorded as a suite failure; if it declares none, the file is a marker-style
file and the pre-existing `// test` scanner keeps owning it — which is what
`edge_malformed_rb_test_is_reported_as_a_failure` in
`tests/discovery_test.rs:250` requires.

Nothing else in `src/` was touched.

Known limitation of the discovery change: `run_test_blocks` collects
`Statement::Test` from the top level of a file only, so a `test` block nested
inside another block is not discovered. Every block in `tests/*.rb` is top
level, and `parse_test` is reachable only from `parse_statement`, so this is a
limitation of the change rather than a gap in the suite.

## Tests added

Rust gate over the suite (8 new `#[test]`, all `edge_*`-named except the
assertion scan):

| Test | Edge class covered |
|---|---|
| `every_test_block_carries_an_assertion` | smoke-test detection; fails if any block loses its `expect` |
| `edge_suite_reports_at_least_40_redblue_tests` | suite size floor, and `failed == 0` |
| `edge_suite_runs_no_skipped_tests` | muted-evidence regression |
| `edge_suite_declares_no_skip_markers` | no `// skip` / `# skip` in any `tests/*.rb` |
| `edge_suite_covers_every_required_area` | all eight areas named in the phase |
| `edge_suite_contains_named_edge_tests` | `edge_*` naming floor |
| `edge_suite_asserts_that_failures_are_produced` | floor of 5 blocks using `catch` |
| `edge_suite_files_are_not_all_commented_out` | the original finding, re-checked directly |

Redblue suite: 187 blocks, of which **57 are named `edge_*`** and **24 use
`try`/`catch` to assert that a failure is produced**. Selected rows:

| Test | Edge class covered |
|---|---|
| `edge_suite_reports_at_least_40_redblue_tests` | original finding |
| `edge_arithmetic_division_by_zero_is_a_caught_runtime_error` | numeric boundary + failure assertion |
| `edge_arithmetic_division_by_zero_is_not_a_silent_infinity` | numeric boundary |
| `edge_arithmetic_zero_over_zero_also_faults` | numeric boundary (`0/0`) |
| `edge_arithmetic_modulo_by_zero_yields_a_number_not_a_crash` | numeric boundary (`x % 0` → `NaN`, never a panic) |
| `edge_arithmetic_integers_past_2_53_lose_precision` | numeric boundary (`2^53+1`) |
| `edge_arithmetic_negative_zero_compares_equal_to_zero` | numeric boundary (`-0.0`) |
| `edge_arithmetic_decimal_addition_is_not_exact` | numeric boundary (`0.1 + 0.2`) |
| `edge_arithmetic_repeating_decimal_is_deterministic` | numeric boundary (`1/3`) |
| `edge_arithmetic_number_plus_text_is_a_caught_type_error` | type mismatch + failure assertion |
| `edge_arithmetic_list_plus_list_is_a_caught_type_error` | type mismatch + failure assertion |
| `edge_text_length_counts_bytes_not_characters` | unicode |
| `edge_text_length_counts_bytes_for_cjk` | unicode |
| `edge_text_length_counts_bytes_for_emoji_outside_the_bmp` | unicode (astral plane) |
| `edge_text_combining_mark_counts_as_its_own_byte` | unicode (combining mark) |
| `edge_text_right_to_left_text_round_trips` | unicode (RTL) |
| `edge_text_emoji_and_cjk_share_one_literal` | unicode |
| `edge_text_a_very_long_text_keeps_its_length` | unicode / long text |
| `text: braces are ordinary characters in a text literal` | malformed input (`{` with no `}` — FINDINGS.md §2) |
| `edge_lists_index_past_the_end_yields_nothing_not_an_error` | out of bounds (`index 999`) |
| `edge_lists_index_into_an_empty_list_yields_nothing` | empty + out of bounds |
| `edge_lists_index_negative_past_the_start_yields_nothing` | out of bounds (`index -99`) |
| `edge_lists_indexing_through_a_number_is_a_caught_error` | type mismatch + failure assertion |
| `edge_lists_a_record_is_not_a_list` | type mismatch |
| `edge_lists_length_of_nothing_is_a_caught_error` | type mismatch + failure assertion |
| `edge_lists_index_zero_of_a_singleton_is_its_only_element` | singleton + boundary (index 0 and -1) |
| `edge_records_missing_key_yields_nothing_not_an_error` | missing key |
| `edge_records_missing_key_on_an_empty_record_yields_nothing` | missing key + empty |
| `edge_records_walking_past_a_missing_key_is_a_caught_error` | missing key, nested + failure assertion |
| `edge_records_a_repeated_key_keeps_the_last_value` | duplicate key |
| `edge_records_a_repeated_key_with_the_same_value_is_stable` | duplicate key |
| `edge_records_a_record_is_not_a_text` | type mismatch |
| `edge_records_a_record_is_not_a_list` | type mismatch |
| `edge_records_indexing_a_record_is_a_caught_error` | type mismatch + failure assertion |
| `edge_records_depth_three_nesting_resolves` | nesting |
| `records: an empty record has no fields` | empty |
| `records: a single field record behaves like a singleton` | singleton |
| `edge_control_conditional_branch_does_not_leak_into_the_enclosing_scope` | scoping |
| `edge_control_nothing_is_falsy_in_an_if` | empty/`nothing` |
| `edge_functions_calling_an_undefined_function_is_a_caught_error` | failure assertion |
| `edge_functions_three_nested_declarations_parse` | nesting (3 scopes) |
| `edge_objects_a_declared_record_starts_empty_and_grows` | empty + singleton |
| `edge_objects_a_field_named_like_a_builtin_shadows_it` | name collision |
| `edge_objects_reassigning_a_declared_record_replaces_it` | type/state |
| `edge_modules_an_unknown_module_is_a_caught_runtime_error` | resource/state + failure assertion |
| `edge_modules_an_unknown_module_does_not_halt_the_program` | failure assertion |
| `edge_modules_an_unknown_module_with_an_alias_is_also_caught` | failure assertion |
| `edge_modules_importing_after_a_failed_import_still_works` | state after failure |
| `edge_modules_an_import_inside_a_loop_body_imports_once` | nesting + state |
| `edge_suite_a_runtime_error_mid_program_leaves_earlier_work_intact` | partial-failure state |
| `edge_suite_two_independent_failures_are_both_reported` | failure assertion ×2 |
| `edge_suite_an_empty_list_and_an_empty_text_stay_distinct` | empty + type mismatch |
| `edge_suite_deeply_nested_lists_resolve_at_every_level` | nesting (4 deep) |
| `edge_suite_deeply_nested_records_resolve_at_every_level` | nesting (4 deep) |
| `edge_integration_a_divisor_of_zero_stops_the_program_at_that_line` | resource/partial state |
| `edge_integration_a_single_element_collection_flows_through` | singleton |
| `edge_integration_a_program_that_only_fails_reports_the_failure` | failure assertion |

### Every test asserts something that can fail

- No `// smoke` marker anywhere: `every_test_block_carries_an_assertion` rejects
  any block without `expect` or `assert.`
- Mutation-checked, twice. The first run dropped two temporary files into
  `tests/`, both turned the gate red, and removed them; the resumed run repeated
  both probes and measured the same two failures:
  - a block with no `expect` → `every_test_block_carries_an_assertion` FAILED,
    naming the probe file (`tests/redblue_suite_test.rs:146`);
  - a block with `expect x to be 2` where `x` is 1 → `rb test` printed
    `Tests run: 188`, `Failed: 1` and exited 1, and
    `edge_suite_reports_at_least_40_redblue_tests` /
    `edge_suite_runs_no_skipped_tests` FAILED.
- The suite is deterministic: no wall clock, no network, no randomness. The only
  filesystem access is `modules/SuiteKit.rb`, which is committed to the repo, and
  the file contents are irrelevant to every assertion (only "the import resolved"
  is asserted). No `HashMap` iteration order reaches an output — the one test that
  touches a record compares *fields*, not a rendered map.

## Gates

All five rows below were re-executed by the resumed run, not inherited from the
first run's claim.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass (0 diff) |
| `cargo clippy --all-targets -- -D warnings` | pass (0 warnings) |
| `cargo test --all-targets` | **67 passed, 0 failed** across 7 binaries (5 + 0 + 13 + 21 + 8 + 5 + 15) |
| `rb test` | **187 run, 187 passed, 0 failed, 0 skipped**, exit 0 |
| `./rbops/verify.sh phase-003` | **NOT RUN — `./rbops/` does not exist in this checkout** |

On the fourth gate, exactly: `./rbops/verify.sh` cannot be executed from the
project root here (`ls: cannot access 'rbops': No such file or directory`, and
`find . -name verify.sh -not -path './target/*'` returns nothing). The RBOPS tree
is not part of this repository, so the phase's own gate script is not reachable.
I did not inspect, reconstruct or substitute for it. The three gates that *can*
run were all run and are green, and `.github/workflows/ci.yml` runs exactly
`cargo build`, `cargo test` and `cargo clippy -- -D warnings` — all three
verified locally green. **If `verify.sh` enforces anything beyond those three,
this phase is unverified against it.**

One candidate extra check was run and found **pre-existing across the whole
repository**, so it is not a regression of this phase and was deliberately not
"fixed": `rb format --check` reports `File would be reformatted` for all six
`examples/*.rb`, `modules/MathUtils.rb`, and every `.rb` file this phase
touched. No `.rb` file in the repository passes `rb format --check`, so that is
the formatter's debt, not this suite's — recorded rather than churned
(FINDINGS.md §11 and §14).

Also run, because AGENTS §2 makes the examples load-bearing:

| Command | Result |
|---|---|
| `rb run examples/*.rb` | all exit 0 |
| `rb run modules/MathUtils.rb` | exit 1 — **pre-existing**, reproduced on stashed `main` (FINDINGS.md §6) |
| `rb run modules/SuiteKit.rb` | exit 0 |
| `rb lint modules/SuiteKit.rb` | exit 0, one warning (`Unused variable: 'SUITE_KIT_NAME'`) |

## Invariants touched

None. No change to `.rb`, to `to … end` / `if … end` / `for … end`, to
`set x to <expr>`, to `say`, to the `Value` variants, to the `Error` variants, or
to the trailing-comma / `{interp}` string syntax. `src/testing/harness.rs` gained
a discovery path; the language surface is identical.

The phase's "unify the language behaviour" obligations are covered by findings,
not by edits — this phase changed tests and one test-harness file only.

## Edge-case matrix (AGENTS §3.2)

| Row | Covered? | Where |
|---|---|---|
| empty / zero / nothing | yes | empty text (`text: empty text has length zero`), empty list (`lists: an empty list has length zero`), empty record (`records: an empty record has no fields`), `repeat 0 times`, `[]` in `for each`, `nothing` in `edge_control_nothing_is_falsy_in_an_if`, `edge_lists_index_into_an_empty_list_yields_nothing` |
| singleton and boundary | yes | one-element list/record/text/loop in every list, record, text and control-flow file; index `0` and index `-1` in `lists: first and last element by index` and `edge_lists_index_zero_of_a_singleton_is_its_only_element` |
| out of bounds | yes | `edge_lists_index_past_the_end_yields_nothing_not_an_error` (index 999), `edge_lists_index_negative_past_the_start_yields_nothing` (index -99), `integration: deeply nested access stays in range` |
| type mismatch | yes | `edge_arithmetic_number_plus_text_is_a_caught_type_error`, `edge_arithmetic_list_plus_list_is_a_caught_type_error`, `edge_lists_a_record_is_not_a_list`, `edge_lists_indexing_through_a_number_is_a_caught_error`, `edge_lists_length_of_nothing_is_a_caught_error`, `edge_records_a_record_is_not_a_text`, `edge_records_a_record_is_not_a_list`, `edge_records_indexing_a_record_is_a_caught_error`, `edge_control_a_type_mismatch_in_a_condition_is_not_silently_true`, `suite: a number cannot be concatenated onto text` |
| type coercion boundaries | yes | `0/0`, `1/0` (×3), `x % 0` → `NaN`, `-0.0`, `0.1 + 0.2`, `2^53+1`, `1/3`. **`-2^31` and integer overflow past `i64`: N/A** — every Redblue number is an `f64` (`src/parser.rs:7`, `Expr::Number(f64)`), so there is no integer type to overflow. `9223372036854775807 + 1` (the largest `i64`) was probed and silently saturates at `9223372036854775807`; there is no saturating-versus-wrapping contract to assert, so it is recorded here rather than pinned as a test |
| unicode and escapes | yes | empty text, `\"`, `\\`, `\n`, `\t`, emoji (astral), CJK, RTL, combining mark, 40-char text — 8 `edge_text_*` tests plus `suite: unicode text survives every stage` and `integration: unicode/text escapes survive every stage` |
| nesting and recursion | partial | nesting: yes — 4-level lists, 4-level records, nested `for each`, nested `if`, 3 nested function declarations. **Recursion: N/A** — `Value::Function` stores no body, so a recursive call cannot run (FINDINGS.md §3). `tests/test_functions.rb` pins declaration behaviour instead |
| duplicate and missing keys | yes | `edge_records_a_repeated_key_keeps_the_last_value`, `edge_records_a_repeated_key_with_the_same_value_is_stable`, `edge_records_missing_key_yields_nothing_not_an_error`, `edge_records_missing_key_on_an_empty_record_yields_nothing`, `edge_records_walking_past_a_missing_key_is_a_caught_error` |
| malformed input | partial | malformed *program* input (unterminated string, unclosed `end`, stray token, empty file, BOM, CRLF, non-UTF-8) is rejected at parse/lex time, which aborts the file — it cannot be asserted from inside a Redblue `test` block, so it belongs to the Rust layer, where `tests/discovery_test.rs` already covers non-UTF-8 (`:227`), CRLF (`:293`) and a malformed body (`:250`). From Redblue, the one malformed *literal* that survives parsing is pinned: `text: braces are ordinary characters in a text literal`. **N/A in Redblue for the rest, and already covered in Rust** |
| resource and state | partial | file that does not exist: yes — `edge_modules_an_unknown_module_is_a_caught_runtime_error`, plus `edge_non_utf8_rb_file_is_reported_as_an_io_error` (pre-existing). State after failure: yes — `edge_modules_an_unknown_module_does_not_halt_the_program`, `edge_modules_importing_after_a_failed_import_still_works`, `edge_suite_a_runtime_error_mid_program_leaves_earlier_work_intact`, `edge_integration_a_divisor_of_zero_stops_the_program_at_that_line`. **Permission denied: N/A** — the suite writes no file. **Path with spaces: N/A** — the suite touches only `modules/SuiteKit.rb`. **Deeply nested call stack: N/A** — there is no recursion (finding 3). **Infinite-loop guard: N/A and unfixable in Redblue** — `while` has no iteration cap and `Statement::Break` is ignored (finding 4), so a Redblue test cannot safely express "loop forever and be stopped". This is the one matrix row with no coverage at all, and it is recorded as a finding rather than papered over |

## Known gaps / follow-ups

Fourteen findings are recorded in `phases/phase-003/FINDINGS.md`, each with a
`file:line` anchor, a reproduction, a severity and a suggested acceptance gate.
The five that block a real language surface:

- **§1** `stdlib::builtin_function` is never called; ~35 stdlib functions
  (`uppercase`, `split`, `abs`, `map`, `to_text`, …) all fail with
  `Unknown function`. Consequence for this phase: the text and math test areas
  can only cover what `length` and `type_of` do.
- **§2** `{interp}` string interpolation is never substituted, though
  `AGENTS.md` §2 lists it as an invariant. `tests/test_text.rb` pins the
  unsubstituted behaviour so the gap is visible; that test must be inverted when
  the fix lands.
- **§3** `Value::Function` stores no body, so every user function returns
  `nothing`. Consequence for this phase: the functions area tests declarations
  (`type_of(f)` is `"function"`), not return values. **No test in this phase
  asserts that a function returns a value**, so fixing §3 will not turn this
  suite red.
- **§13** of the three assertion forms AGENTS §3.1 permits, only
  `expect <expr> to be <expr>` exists: `expect … to contain …` dies with
  `Unknown variable 'contain'` and `assert.equal` with `Unknown function
  'assert_equal'`. Consequence: all 187 blocks use the one working form and
  express containment as whole-value equality or an explicit loop. **The
  testing contract in AGENTS §3.1 cannot be satisfied until this is fixed.**
- **§11** the lexer has no `<`, `>`, `<=`, `>=`, `==`, `!=`, while the formatter
  emits `is greater than`, which the parser cannot read — `rb format` can
  produce unparseable code. Consequence: every comparison in the suite is `is` /
  `is not`.
- **§14** `Formatter::write` (`src/formatter.rs:430-439`) inserts a space between
  any two adjacent writes, so `format_string_literal` emits `" ab "` for `"ab"` —
  `rb format` **silently changes every string literal's value**, and it is why
  `rb format --check` fails on every `.rb` file in the repository. Found by this
  phase's run of `rb format --check`; it is a formatter bug, not a test-suite
  bug, so it is a finding and not a fix here. It is the one thing this phase
  would most like a follow-up phase for, because `rb format` is the only
  mechanical way to keep the ten suite files readable and currently it would
  corrupt them.

No test in this suite asserts a *bug* as if it were a feature, except for the
three tests whose names say so explicitly and which point at the finding:
`text: braces are ordinary characters in a text literal` (FINDINGS.md §2),
`lists: a conditional inside a loop does not disturb the iteration` and
`lists: break inside a loop does not truncate it` (FINDINGS.md §4).

## Definition of done

| Requirement | Measured | Status |
|---|---|---|
| ≥ 40 Redblue test blocks, all with assertions | 187 blocks, 322 assertion lines, 0 blocks without an `expect` | met |
| ≥ 8 named `edge_*` | 57 | met |
| ≥ 5 tests that assert a failure is produced | 24 blocks use `try`/`catch error` and assert the catch fired | met |
| `cargo test` green, non-zero | 67 passed, 0 failed | met |
| `rb test` green, non-zero | 187 run, 187 passed, 0 failed, 0 skipped, exit 0 | met |
| ≥ 3 new `#[test]` / ≥ 2 new Redblue `test` blocks | 8 new `#[test]` + 165 new Redblue blocks | met |
| ≥ 1 test named `edge_*` | 57 Redblue + 8 Rust | met |
| ≥ 1 test asserting a failure is produced | 24 Redblue + `every_test_block_carries_an_assertion` | met |
| zero new `#[ignore]` / `// skip` / `allow(clippy::` | 0 added; `edge_suite_declares_no_skip_markers` and `edge_suite_runs_no_skipped_tests` enforce it | met |
| zero newly-failing pre-existing tests | all pre-existing test files untouched; 67 pass | met |

The fourth gate could not be run; see **Gates** above.