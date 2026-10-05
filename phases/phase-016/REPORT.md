# Phase 016 — Linter must not produce false positives

Continuation run. The previous attempt's work is committed and kept; this run
did not redo it. It found that **`cargo test` was red when it started** — two
pre-existing tests in `tests/tooling_grammar_test.rs` pinned the linter's
*old* behaviour and had been left failing by phase-016's own fix — repaired
them, and added one test for a linter guarantee nothing pinned.

## What changed

| File | Lines | What |
|---|---|---|
| `tests/tooling_grammar_test.rs` | +20 −5 | two stale expectations corrected to the linter's current contract, plus a new position-ordering assertion |
| `tests/linter_test.rs` | +30 −0 | 1 new test: `edge_a_name_assigned_on_several_lines_is_reported_once_at_the_first_set` |

No production code changed this run. `src/linter.rs` is byte-identical to `HEAD`
(`git diff src/linter.rs` → empty). The phase's `src/` work is already merged and
is described under *Phase total* below.

## The finding, re-verified

**It was still reproducing as a red suite**, not as a false positive:

```
$ cargo test --all-targets
---- diagnostics_report_linter_findings_as_warnings stdout ----
thread 'diagnostics_report_linter_findings_as_warnings' panicked at tests/tooling_grammar_test.rs:377:5:
assertion `left == right` failed: the linter reports a whole-program finding with no position
  left: (1, 1)
 right: (0, 0)

---- unused_variable_warnings_come_out_in_a_stable_name_order stdout ----
thread 'unused_variable_warnings_come_out_in_a_stable_name_order' panicked at tests/tooling_grammar_test.rs:457:9:
assertion `left == right` failed: attempt 0: unused-variable findings must be sorted by name so the diagnostics JSON is reproducible
  left: ["Unused variable: 'zeta'", "Unused variable: 'alpha'", "Unused variable: 'mid'", "Unused variable: 'beta'"]
 right: ["Unused variable: 'alpha'", "Unused variable: 'beta'", "Unused variable: 'mid'", "Unused variable: 'zeta'"]

test result: FAILED. 16 passed; 2 failed; 0 ignored
```

Both were written against the pre-position linter. Each asserted something that
phase-016 deliberately improved, and both assertions were *false as written*:

### 1. `diagnostics_report_linter_findings_as_warnings`

It asserted the linter returns `(0, 0)` — "the linter reports a whole-program
finding with no position". It does not. Phase-016 gave every finding the line
that defined the variable (`definition_lines`, `src/linter.rs:154-156`), so for
`set x to 1` the finding is `(1, 1)`. The old expectation now describes a linter
that no longer exists.

The assertion is `(1, 1)` now, and its message says what the number *means*: a
finding must be placeable so an editor can underline it. This is **stronger**,
not weaker — `(0, 0)` was a statement that the linter knew nothing; `(1, 1)` is a
statement that it knows exactly where. The rest of the test is untouched,
including the downstream check at `tests/tooling_grammar_test.rs:404-408` that
`diagnostics()` still yields `(1, 1)`, and the JSON checks.

### 2. `unused_variable_warnings_come_out_in_a_stable_name_order`

This one could not be repaired by editing the linter, because the two orders are
mutually exclusive and each is pinned by a test:

| Test | Pins |
|---|---|
| `tests/linter_test.rs:346` `edge_warning_order_is_stable_and_follows_the_source` | `zeta, alpha, mu, beta` — source order |
| `tests/tooling_grammar_test.rs:432` `unused_variable_warnings_come_out_in_a_stable_name_order` | `alpha, beta, mid, zeta` — name order |

for the same fixture shape (four unused names declared in the order
`zeta, alpha, mid, beta`).

**Source order is the correct contract**, and this is not a preference:

- Every finding now carries a line number. Under name order, `rb lint` prints
  `Warning: line 1: 'zeta'`, then `line 3`, then `line 2` — the line numbers
  jump around for no reason the reader can reconstruct. Under source order they
  ascend, which is how a reader scans a file.
- The name was only ever a tiebreaker. Before phase-016 every finding sat at the
  same fabricated position, so name order was the only ordering available and
  the test pinned it honestly. Phase-016 gave findings real positions; the
  tiebreaker became the weaker half of the key, not the whole key.

So the phase-017 test's `expected` vector is now the source order, its comment
explains *why* the stable order is positional, and its assertion message no
longer claims the linter sorts by name. The 64-attempt loop, the
linter-entry-point agreement check, and the JSON-equality check are all unchanged.

To stop it degenerating into "whatever the linter emits is fine", a **new**
assertion was added inside the same loop — findings must be strictly increasing
in `(line, column)`:

```rust
assert!(
    found.windows(2).all(|pair| {
        (pair[0].line, pair[0].column) < (pair[1].line, pair[1].column)
    }),
    "attempt {}: findings must be ordered by position, which is what \
     makes the order total: {:?}",
    attempt,
    found
);
```

That pins the contract the other test pins, from the diagnostic side.

## The new test

`edge_a_name_assigned_on_several_lines_is_reported_once_at_the_first_set`

`set` is assignment, so `set x to …` on three lines is one binding assigned three
times. The linter keeps the *first* definition line (`or_insert`,
`src/linter.rs:155-156`) and reports one finding. That is why
`tests/test_numeric_edges.rb` produces a single `x` warning rather than one per
failing division, and nothing tested it.

**Watched it fail.** Replacing `or_insert` with an `and_modify` that overwrites
(so the *last* definition wins) turns it red for the right reason:

```
thread 'edge_a_name_assigned_on_several_lines_is_reported_once_at_the_first_set'
panicked at tests/linter_test.rs:69:5:
assertion `left == right` failed: the finding must point at the first `set x`, the line that introduced it
  left: 12
 right: 2
test result: FAILED. 0 passed; 1 failed
```

The experiment was reverted; `src/linter.rs` is unmodified.

## Phase total (merged earlier, re-verified this run)

| File | Change | What |
|---|---|---|
| `src/linter.rs` | 257 → 447 lines | the four rules, sorted findings, spanned syntax errors, `extends` reads its parent |
| `src/lib.rs:156-165` | +6 −2 | `rb lint` prints the line of every warning and every error |
| `tests/linter_test.rs` | 48 → 49 tests | the rules' positive and negative cases and the edge matrix |

The four false positives fixed are enumerated in `FINDINGS.md` items 1–5.

## Tests added

1 test this run; 49 in `tests/linter_test.rs`, 18 in `tests/tooling_grammar_test.rs`.

| Test | Edge class covered |
|---|---|
| `edge_a_name_assigned_on_several_lines_is_reported_once_at_the_first_set` | singleton / duplicate: one name assigned three times is one finding at line 2 |
| `diagnostics_report_linter_findings_as_warnings` (corrected) | placement: a finding carries the line that defined the variable, not `0:0` |
| `unused_variable_warnings_come_out_in_a_stable_name_order` (corrected, assertion added) | determinism over 64 calls, and position is a total order |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, no warnings |
| `cargo test --all-targets` | **390 passed, 0 failed, 0 ignored** |
| `./rbops/verify.sh phase-016` | **not run — the script does not exist in this checkout** |

Gate 4 honestly: `./rbops/verify.sh phase-016` → `No such file or directory`,
exit 127. `ls rbops` → `No such file or directory`, and `find . -name verify.sh
-not -path ./target/*` returns nothing. The whole `rbops/` tree is absent from
this checkout, so the gate cannot be executed here and no claim is made about
its verdict. The three cargo gates are the commands the phase prompt lists, run
as written.

Independently checked on the built binary:

```
$ for f in examples/*.rb; do ./target/debug/rb lint "$f"; done
(no output — examples/ is silent)

$ for f in examples/*.rb; do ./target/debug/rb run "$f"; done
(all 6 examples exit 0)

$ ./target/debug/rb lint tests/test_lists.rb
Warning: line 124: Unused variable: 'item'
Warning: line 197: Unused variable: 'bad'

$ ./target/debug/rb lint modules/MathUtils.rb
Error: line 4: Syntax error: Expected function name     exit=1
```

## Invariants touched

- None. No grammar, no `Value` variant, no `Error` variant, no `.rb` extension,
  no `end`-delimited block. `redblue::linter::lint` keeps its signature and its
  `(errors, warnings)` shape. `Value`, `Error` and the statement and expression
  enums are untouched — `git diff` this run is `tests/` only.
- No test was weakened, skipped, removed or re-scoped. Two were *corrected* from
  an expectation the implementation had deliberately outgrown to the current
  one, and the corrected one is stricter. `grep -rn "#\[ignore\]\|allow(clippy::"
  src tests` returns nothing; `cargo test` reports `0 ignored`.

## Definition of done

| Item | Status |
|---|---|
| zero false positives across `examples/` and `tests/` | **yes** — `examples/` is silent; the 17 unused variables and 1 shadowing warning across `tests/*.rb` are all true positives, re-checked by hand this run (`tests/test_numeric_edges.rb:11`, `tests/integration_test.rb:145` and `:153` read as never-read assignment targets inside `try`, which is the group described in FINDINGS F3) |
| every rule has a positive and a negative test | unused variable, unused import, shadowing, unparsable source — each has both in `tests/linter_test.rs` |
| rule for shadowing, unused variable, unused import, missing end | all four; `missing end` surfaces as a `LintError` (`src/linter.rs:432-446`), so `rb lint` exits 1 |
| lint never panics on malformed source | `edge_unterminated_string_…`, `edge_stray_token_…`, `edge_unclosed_list_…`, `edge_bom_and_crlf_…`, `edge_deeply_nested_…`; non-UTF-8 bytes are rejected by `run_cli` before the linter sees them |

## Known gaps / follow-ups

- **Linting a module file as a program reports its export surface as unused.**
  `rb lint modules/SuiteKit.rb` prints `Unused variable: 'SUITE_KIT_NAME'`, but
  `import` copies exactly those `set` names into globals (`src/vm.rs:285-290`).
  → FINDINGS **F4**, not fixed here: it needs a module-aware entry point and a
  decision about which files are modules, which is its own phase.
- **`modules/MathUtils.rb` cannot be linted.** It uses `constant PI to 3.14`,
  which `parse_callable` (`src/parser.rs:815`) rejects. Adding `constant` is a
  grammar change → FINDINGS **F1**.
- **A defined and never called function is not reported.**
  `defined_functions` (`src/linter.rs:21`) is collected and never read; Redblue
  functions are first class, so the rule cannot be written from the syntax tree
  without tracking values → FINDINGS **F2**.
- **`rb lint` does not recurse into a directory.** It takes one path, as it
  always has. The corpus walk is `tests/linter_test.rs:436`.
- **`modules/` is outside the corpus walk** because of F1 and F4.
- **An import inside a block is never reported as unused**, even when the
  surrounding file never uses the module. Chosen on purpose:
  `tests/test_modules.rb` imports modules to prove they resolve, and that is the
  statement under test.
- **A shadowed `catch` binding is reported as `Parameter 'x' shadows an outer
  variable`.** It is a binding, not a parameter; the wording came from
  `bind_signature` and was not worth a separate message for one case.

## Edge-case matrix

| Row | Covered | By |
|---|---|---|
| empty | yes | `edge_empty_and_whitespace_only_source_lints_clean`, `edge_empty_and_singleton_collections_are_linted`, `edge_empty_source_produces_no_diagnostics` |
| singleton | yes | `edge_empty_and_singleton_collections_are_linted`, `edge_an_unread_loop_variable_is_reported`, and this run's `edge_a_name_assigned_on_several_lines_…` (one name, one finding) |
| boundary | yes | the two above, plus `edge_every_parent_in_a_chain_of_extends_is_read` and `edge_unicode_source_reports_the_right_line` |
| out_of_bounds | yes | `edge_numeric_and_index_edges_are_not_lint_errors` (`items[999]`, `items[-1]`) — the linter stays out of the runtime's business |
| type_mismatch | yes | `edge_numeric_and_index_edges_are_not_lint_errors` (`1 + "one"`) |
| numeric_boundary | yes | `edge_numeric_and_index_edges_are_not_lint_errors` (`2^53+1`, `i64::MIN`, `1e308`, `1 / 0`), and this run's new test over three failing divisions of `x` |
| unicode | yes | `edge_unicode_source_reports_the_right_line` (emoji, accented, RTL), `edge_unicode_variable_names_are_linted` |
| nesting_recursion | yes | `nested_scopes_do_not_leak_names_into_each_other`, `edge_a_name_bound_in_a_nested_scope_only_is_still_reported`, `edge_deeply_nested_blocks_…`, `edge_every_parent_in_a_chain_of_extends_is_read` |
| duplicate_missing_keys | yes | `edge_a_record_with_a_repeated_key_is_not_a_lint_error`, `edge_a_missing_field_is_not_a_lint_error` |
| malformed_input | yes | `edge_unterminated_string_…`, `edge_stray_token_…`, `edge_unclosed_list_…`, `a_missing_end_is_reported_as_an_error`, `edge_bom_and_crlf_source_lints_clean`, `edge_extends_of_an_undeclared_parent_…`, non-UTF-8 bytes via the binary |
| resource_limit | yes | `edge_deeply_nested_blocks_are_a_diagnostic_not_a_stack_overflow` (500), `edge_deeply_nested_expressions_…` (500), `edge_a_long_source_is_linted_without_truncation` (2000 lines) |
