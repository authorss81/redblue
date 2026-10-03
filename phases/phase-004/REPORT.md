# Phase 004 — Deterministic Record ordering

## What changed

| File | Lines | What |
|---|---|---|
| `src/value.rs` | +11 −2 | New `pub type Fields = IndexMap<String, Value>`; `Value::Record` and `Value::Object` payloads changed from `HashMap` to `Fields` |
| `src/vm.rs` | +5 −5 | Record construction sites (`Expr::Record`, `Statement::Object`, `time.now`, `parse_json_object`) build `Fields` instead of `HashMap` |
| `src/testing/runner.rs` | +1 −1 | `assert_type` record `TypeId` follows the payload type |
| `Cargo.toml` | +1 | `indexmap = "2"` dependency (already in `Cargo.lock` as a transitive dep of `reqwest`, so no new crate is fetched) |
| `tests/record_order_test.rs` | +438 | 23 new tests |

Production diff: **+19 −9**. No behaviour outside record field iteration is touched.

## Reproduction (before the change)

```
$ cat repro.rb
set r to {zebra: 1, alpha: 2, mango: 3, beta: 4, omega: 5}
say r
say json.stringify(r)

$ for i in $(seq 1 6); do ./target/debug/rb run repro.rb; done
{zebra: 1, beta: 4, mango: 3, omega: 5, alpha: 2}
{"zebra": 1, "beta": 4, "mango": 3, "omega": 5, "alpha": 2}
{alpha: 2, mango: 3, zebra: 1, beta: 4, omega: 5}
{"alpha": 2, "mango": 3, "zebra": 1, "beta": 4, "omega": 5}
{alpha: 2, mango: 3, beta: 4, zebra: 1, omega: 5}
{"alpha": 2, "mango": 3, "beta": 4, "zebra": 1, "omega": 5}
{alpha: 2, beta: 4, zebra: 1, omega: 5, mango: 3}
{"alpha": 2, "beta": 4, "zebra": 1, "omega": 5, "mango": 3}
{zebra: 1, omega: 5, alpha: 2, beta: 4, mango: 3}
{"zebra": 1, "omega": 5, "alpha": 2, "beta": 4, "mango": 3}
{mango: 3, alpha: 2, zebra: 1, beta: 4, omega: 5}
{"mango": 3, "alpha": 2, "zebra": 1, "beta": 4, "omega": 5}
```

Same program, same input, six different orderings. After the change all six runs print
`{zebra: 1, alpha: 2, mango: 3, beta: 4, omega: 5}`.

## The failing test came first

`test_record_display_preserves_insertion_order` was written and run before any production
edit. It failed for the right reason:

```
left:  "{delta: 5, epsilon: 10, kappa: 7, nu: 15, ...}"
right: "{zebra: 0, alpha: 1, mango: 2, beta: 3, ...}"
```

The whole new file was re-run against the pre-fix tree (`git stash` of the four source
files) to prove the tests can fail: **13 of 23 failed, 10 passed.** The 10 that pass on both
trees are the error-path and empty/singleton tests, which pin behaviour the fix must not
change.

## Tests added

`tests/record_order_test.rs` — 23 `#[test]` functions, 0 ignored, 0 skipped.

| Test | Edge class covered |
|---|---|
| `test_record_display_preserves_insertion_order` | boundary (20 keys), display order |
| `test_json_stringify_preserves_insertion_order` | boundary, JSON order |
| `edge_twenty_key_record_is_identical_across_fifty_runs` | resource/repeatability — 50 runs, byte-identical |
| `test_json_parse_preserves_source_order` | duplicate/missing keys — document key order survives parsing |
| `test_json_stringify_round_trips_through_parse` | round-trip, stability |
| `test_empty_record_displays_as_empty_braces` | empty — `{}`, `json.stringify`, `json.parse("{}")` |
| `test_singleton_record_displays_its_only_field` | singleton — one field, display and JSON |
| `test_first_and_last_field_are_reachable_in_order` | boundary — index 0 and index len−1 of the field order |
| `test_duplicate_key_takes_the_last_value_and_the_first_position` | duplicate keys — last-wins, first position |
| `test_duplicate_key_in_json_takes_the_last_value` | duplicate keys in JSON |
| `test_missing_key_reads_as_nothing_and_does_not_grow_the_record` | missing keys |
| `test_set_property_updates_in_place_without_reordering` | duplicate keys via `set r.k to v`; new field appends |
| `test_nested_records_preserve_order_at_every_level` | nesting, 2 levels |
| `test_deeply_nested_records_preserve_order` | nesting, 3 levels |
| `test_records_inside_lists_preserve_order` | nesting — record inside a list |
| `test_unicode_keys_and_values_preserve_order` | unicode — emoji, CJK, RTL, combining mark values |
| `test_unicode_keys_keep_insertion_order` | unicode — decomposed `café` vs `cafe` keys |
| `test_numeric_boundary_values_keep_field_order` | numeric boundary — `0`, `-0.0`, `2^53+1`, `0.1`, `−2^53−1` |
| `test_property_access_on_a_list_is_a_runtime_error` | type mismatch — **asserts a failure** |
| `test_malformed_json_object_is_an_error_not_a_reordered_record` | malformed input — **asserts a failure** |
| `test_record_literal_missing_colon_is_a_parse_error` | malformed input — **asserts a failure** |
| `test_record_equality_ignores_field_order` | ordering must not leak into equality |
| `test_expect_on_a_record_compares_field_values` | **asserts `expect` fails** on wrong field / wrong name |

### Mandatory edge-case matrix

- empty — covered (`test_empty_record_displays_as_empty_braces`)
- singleton — covered (`test_singleton_record_displays_its_only_field`)
- boundary — covered (`test_first_and_last_field_are_reachable_in_order`, 20-key tests)
- out_of_bounds — covered as missing-key access returning `nothing` without growing the
  record (`test_missing_key_reads_as_nothing_and_does_not_grow_the_record`). Records have no
  positional index in the language, so there is no `index == len` case to test.
- type_mismatch — covered (`test_property_access_on_a_list_is_a_runtime_error`)
- numeric_boundary — covered (`test_numeric_boundary_values_keep_field_order`)
- unicode — covered (`test_unicode_keys_and_values_preserve_order`,
  `test_unicode_keys_keep_insertion_order`)
- nesting_recursion — covered (`test_nested_records_preserve_order_at_every_level`,
  `test_deeply_nested_records_preserve_order`, `test_records_inside_lists_preserve_order`)
- duplicate_missing_keys — covered (4 tests)
- malformed_input — covered (2 tests: unterminated JSON object, record literal without a colon)
- resource_limit — covered (`edge_twenty_key_record_is_identical_across_fifty_runs`; the
  phase adds no unbounded work, and the 20-key × 50-run loop is the repeatability budget)

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass (no diff) |
| `cargo clippy --all-targets -- -D warnings` | pass (0 warnings) |
| `cargo test --all-targets` | 54 passed, 0 failed, 0 ignored (5 + 21 + 23 + 5; doc-tests 0) |
| `./rbops/verify.sh phase-004` | **could not run — `rbops/` does not exist in this checkout** |

`rbops/`, `rbops/phases.json`, `rbops/verify.sh` and `phases/` were all absent from the
working tree (`ls rbops` → `No such file or directory`; `find . -name verify.sh` → nothing
outside `target/`). This phase directory was created by hand to hold this report. In place
of that gate I ran what it stands for:

- `cargo build` then `rb run` on every `examples/*.rb`, `modules/*.rb`, `tests/*.rb`:
  all exit 0 except `modules/MathUtils.rb`, which fails with
  `Error: ParserError: Expected function name`. Verified pre-existing: the same error and
  exit code 1 occur on the stashed pre-fix tree. Recorded in `FINDINGS.md`.
- `rb test tests/suite.rb` → 12 total, 12 passed, 0 failed.
- `rb test tests/integration_test.rb` → 6 total, 5 passed, 0 failed, 1 skipped. The skip is
  the pre-existing `// skip` at `tests/integration_test.rb:26`, not added by this phase.
- `rb lint examples/formats.rb` → exit 0.
- `cargo clippy -- -D warnings` (the exact CI command in `.github/workflows/ci.yml`) → pass.

## Invariants touched

- `Value` variant **names** are unchanged (`Nothing/Number/Text/YesNo/List/Record/Object/
  Function/Builtin`). The *payload type* of `Record` and `Object` changed from
  `HashMap<String, Value>` to `IndexMap<String, Value>`, aliased as `redblue::value::Fields`.
  This is the change the phase exists to make; `Value` is still re-exported as
  `redblue::Value`.
- `IndexMap`'s `PartialEq` is order-independent, so record equality — and therefore
  `expect a to be b` — is unchanged. Pinned by
  `test_record_equality_ignores_field_order` and `test_expect_on_a_record_compares_field_values`.
- No grammar, `end`-terminator, `set x to`, `say`, string-interpolation or `.rb`-extension
  change. `Error::{Lexer,Parser,Analyzer,Runtime,Io}` unchanged.
- New dependency `indexmap`. `Cargo.lock` is gitignored in this repo, and `indexmap 2.14.2`
  was already resolved there as a transitive dependency of `reqwest`, so no new crate enters
  the build graph.

## Known gaps / follow-ups

- `Value::Object` is never constructed anywhere in `src/` — `Statement::Object` at
  `src/vm.rs:270-278` creates an empty `Value::Record`, not a `Value::Object`. The
  `Object` payload type was updated for consistency, but no object behaviour can be tested
  until objects are actually built. → `FINDINGS.md`
- `stdlib::builtins()` returns a `HashMap<String, Value>` (`src/stdlib.rs:4`). It is never
  iterated into user-visible output today, so it was left alone to keep the diff on-topic.
  → `FINDINGS.md`
- Field *updates* keep a field at its first-inserted position (`{a: 1, b: 2}` then
  `set r.a to 9` → `{a: 9, b: 2}`). This matches `HashMap`'s value semantics and is now
  pinned by a test; it is a choice, not an accident.
- `modules/MathUtils.rb` fails to parse (pre-existing). → `FINDINGS.md`
- `rbops/verify.sh` is missing from this checkout, so the project-specific gate is
  unverified. → `FINDINGS.md`
