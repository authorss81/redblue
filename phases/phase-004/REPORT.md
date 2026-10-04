# Phase 004 — Deterministic Record ordering

## Finding status

The finding reproduced on the pre-phase tree (`4ec2dd9`). A 20-key record literal
printed through the real binary gave **12 distinct outputs across 12 separate
processes**:

```bash
$ cat order.rb                      # 20-key record literal, keys deliberately not alphabetical
set rec to {zebra: 0, alpha: 1, mango: 2, ... , rho: 19}
say rec
say json.stringify(rec)

$ for i in $(seq 1 12); do ./target-base/debug/rb run order.rb | head -1 | cut -c1-60; done \
    | sort -u | wc -l
12
```

Root cause was exactly as filed: `Value::Record`/`Value::Object` held
`HashMap<String, Value>` (`src/value.rs:11` before the fix), and both
`Value::Display` (`src/value.rs:34-40`) and `json_stringify` (`src/vm.rs:1067`)
iterated that map. `std::collections::HashMap` seeds its hasher per map, so the
iteration order differs in every process.

After the fix, the same command over **50** processes yields **1** distinct
output (identical md5 for all 50 runs).

## What changed

| File | Lines | What |
|---|---|---|
| Cargo.toml | +1 | added `indexmap = "2"` |
| src/value.rs | +15 −4 | new `pub type Fields = IndexMap<String, Value>`; `Value::Record`/`Value::Object` now store `Fields`. `Display` for `Record` (src/value.rs:43) needed no change — it iterates whatever the map is, and now gets insertion order |
| src/vm.rs | +10 −10 | record construction sites switched from `HashMap::new()` / `HashMap::from([…])` to `Fields::new()` / `Fields::from([…])`: record literal (src/vm.rs:415), object creation (src/vm.rs:276), `time.now` (src/vm.rs:691), `parse_json_object` (src/vm.rs:939) |
| src/testing/runner.rs | +1 −1 | `assert_type` maps `Value::Record` to `TypeId::of::<Fields>()` |
| tests/record_order_test.rs | +453 | 23 new tests (see below) |

No behaviour outside record field order was touched. `IndexMap`'s `PartialEq` is
order-insensitive, so `Value` equality — and therefore `expect a to be b` — is
unchanged (`test_record_equality_ignores_field_order`).

## Tests added

All in `tests/record_order_test.rs` (23 new `#[test]` functions). Every one has a
real assertion with a named failure message; none is a smoke test.

| Test | Edge class covered |
|---|---|
| `test_record_display_preserves_insertion_order` | core ordering (`say`) |
| `test_json_stringify_preserves_insertion_order` | core ordering (`json.stringify`) |
| `edge_twenty_key_record_is_identical_across_fifty_runs` | definition of done: 20 keys, 50 runs, byte-identical |
| `test_json_parse_preserves_source_order` | ordering through `json.parse`, not just literals |
| `test_json_stringify_round_trips_through_parse` | round-trip stability |
| `test_empty_record_displays_as_empty_braces` | empty |
| `test_singleton_record_displays_its_only_field` | singleton |
| `test_first_and_last_field_are_reachable_in_order` | boundary (index 0 and index len−1), read path |
| `test_duplicate_key_takes_the_last_value_and_the_first_position` | duplicate keys: last-wins, first position retained |
| `test_duplicate_key_in_json_takes_the_last_value` | duplicate keys in `json.parse` |
| `test_missing_key_reads_as_nothing_and_does_not_grow_the_record` | missing key |
| `test_set_property_updates_in_place_without_reordering` | mutation must not reorder existing fields; new fields append |
| `test_nested_records_preserve_order_at_every_level` | nesting |
| `test_deeply_nested_records_preserve_order` | nesting, 3 levels |
| `test_records_inside_lists_preserve_order` | nesting inside lists |
| `test_unicode_keys_and_values_preserve_order` | unicode (emoji, CJK, RTL, combining mark) |
| `test_unicode_keys_keep_insertion_order` | unicode keys, decomposed vs ASCII |
| `test_numeric_boundary_values_keep_field_order` | numeric boundary (`0`, `-0.0`, `2^53±1`, `0.1`) |
| `test_property_access_on_a_list_is_a_runtime_error` | **asserts a failure**: type mismatch → `Error::Runtime` |
| `test_malformed_json_object_is_an_error_not_a_reordered_record` | **asserts a failure**: malformed input → `Error::Runtime` |
| `test_record_literal_missing_colon_is_a_parse_error` | **asserts a failure**: malformed input → `Error::Parser` |
| `test_record_equality_ignores_field_order` | regression guard: order must not leak into `expect` |
| `test_expect_on_a_record_compares_field_values` | **asserts a failure**: records differing in key or value must not compare equal |

Quota check: 23 ≥ 6 new tests; `edge_*` present; 4 tests assert failures; 0 new
`#[ignore]`/`skip`/`allow(clippy::)`; 0 pre-existing tests modified or re-scoped.

## Edge-case matrix

- **empty** — covered (`test_empty_record_displays_as_empty_braces`: `{}`,
  `json.stringify`, `json.parse("{}")`).
- **singleton** — covered (`test_singleton_record_displays_its_only_field`).
- **boundary** — covered (`test_first_and_last_field_are_reachable_in_order`:
  first and last field of a 20-field record, both display and read paths).
- **out_of_bounds** — covered. Records are not indexable, so the equivalent is
  the missing-key read, asserted not to create a field
  (`test_missing_key_reads_as_nothing_and_does_not_grow_the_record`).
- **type_mismatch** — covered
  (`test_property_access_on_a_list_is_a_runtime_error`).
- **numeric_boundary** — covered (`test_numeric_boundary_values_keep_field_order`).
- **unicode** — covered (`test_unicode_keys_and_values_preserve_order`,
  `test_unicode_keys_keep_insertion_order`).
- **nesting_recursion** — covered for nesting (3 levels, records inside lists).
  N/A for *recursion*: recursion depth is a call-stack concern, orthogonal to
  field ordering and untouched by this phase.
- **duplicate_missing_keys** — covered (four tests).
- **malformed_input** — covered (unterminated JSON object, record literal with
  no colon).
- **resource_limit** — N/A. This phase adds no allocation, recursion or loop; it
  swaps one map for another. `IndexMap` has the same O(1) bounds as `HashMap`.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test --all-targets` | 54 passed, 0 failed (5 lib + 0 bin + 21 `expect_test` + 23 `record_order_test` + 5 `redblue_test`), 0 ignored |
| `rbops/verify.sh phase-004` | **not run — `rbops/verify.sh` does not exist in this checkout.** See "Known gaps". |

Backwards compatibility was checked by hand: all six `examples/*.rb` run clean
under the new binary (`hello`, `fizzbuzz`, `files`, `formats`, `time`,
`test_arithmetic`). `modules/MathUtils.rb` is a module definition file, not a
runnable script — invoking it directly fails with `ParserError: Expected function
name`, which is pre-existing and unrelated.

## Invariants touched

- None. `Value`'s variant names and arity are unchanged (`Record(Fields)`,
  `Object(String, Fields)`); only the field storage type behind the `Fields`
  alias changed. `Error::{Lexer,Parser,Analyzer,Runtime,Io}` unchanged.
  `.rb` extension, `end` blocks, `set x to <expr>`, `say`, `{interp}` strings all
  untouched. No existing test weakened.

## Known gaps / follow-ups

- **`rbops/verify.sh` is absent from this checkout.** No `rbops/` directory, no
  `phases/` directory, no `.opencode/agent/`. I could not run the fourth gate
  and did not create or modify it (hard rule 1). The three gates that exist are
  green. Recorded in `FINDINGS.md`.
- Scope left deliberately alone, recorded in `FINDINGS.md`: VM scope maps
  (`globals`, `locals` in `src/vm.rs:11-12`) are still `HashMap`. Nothing
  iterates them into output today, so they are not observable, but they are the
  same latent hazard if a future feature prints a scope.
- Pre-existing, unrelated: `set person to record { name: "Alice" }`, the form
  documented at `SPEC.md:107`, does not parse (`ParserError: Expected field
  name`); only the bare `{…}` literal form works. Not touched here.