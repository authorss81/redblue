# Phase 004 — Deterministic Record ordering

## Finding status

The finding **reproduced on the pre-phase tree** (`4ec2dd9`). A 20-key record
literal printed through the real binary gave **12 distinct outputs across 12
separate processes**:

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
`Value::Display` and `json_stringify` iterated that map. `HashMap` seeds its
hasher per map, so the iteration order differs in every process.

After the fix, 50 processes produce **one** distinct output:

```bash
$ for i in $(seq 1 50); do ./target/debug/rb run target/tmp/order.rb; done | md5sum
724cef72af2c91d666039bac7dc44b31  -
```

## What changed

| File | Lines | What |
|---|---|---|
| `Cargo.toml` | +1 | added `indexmap = "2"` |
| `src/value.rs` | +15 −4 | new `pub type Fields = IndexMap<String, Value>`; `Value::Record`/`Value::Object` now store `Fields`. `Display` for `Record` needed no change — it iterates whatever map it is given, and now gets insertion order |
| `src/vm.rs` | +10 −10 | record construction sites switched from `HashMap::new()` / `HashMap::from([…])` to `Fields::new()` / `Fields::from([…])`: record literal, object creation, `time.now`, `parse_json_object` |
| `src/testing/runner.rs` | +1 −1 | `assert_type` maps `Value::Record` to `TypeId::of::<Fields>()` |
| `tests/record_order_test.rs` | +508 | 25 tests (see below) |

No behaviour outside record field order was touched. `IndexMap`'s `PartialEq`
is order-insensitive, so `Value` equality — and therefore `expect a to be b` —
is unchanged (`test_record_equality_ignores_field_order`).

## Tests added

All in `tests/record_order_test.rs`. Every one has a real assertion with a named
failure message; none is a smoke test.

| Test | Edge class covered |
|---|---|
| `test_record_display_preserves_insertion_order` | core ordering (`say`) |
| `test_json_stringify_preserves_insertion_order` | core ordering (`json.stringify`) |
| `edge_twenty_key_record_is_identical_across_fifty_runs` | definition of done: 20 keys, 50 in-process runs |
| `edge_twenty_key_record_is_identical_across_fifty_processes` | **the actual defect**: 20 keys, 50 *separate processes*, byte-identical stdout |
| `test_json_parse_preserves_source_order` | ordering through `json.parse`, not just literals |
| `test_json_stringify_round_trips_through_parse` | round-trip stability |
| `test_empty_record_displays_as_empty_braces` | empty |
| `test_singleton_record_displays_its_only_field` | singleton |
| `test_first_and_last_field_are_reachable_in_order` | boundary (index 0 and index len−1), read path |
| `test_duplicate_key_takes_the_last_value_and_the_first_position` | duplicate keys: last-wins, first position retained |
| `test_duplicate_key_in_json_takes_the_last_value` | duplicate keys in `json.parse` |
| `test_missing_key_reads_as_nothing_and_does_not_grow_the_record` | missing key |
| `test_set_property_updates_in_place_without_reordering` | mutation must not reorder; new fields append |
| `test_nested_records_preserve_order_at_every_level` | nesting |
| `test_deeply_nested_records_preserve_order` | nesting, 3 levels |
| `test_records_inside_lists_preserve_order` | nesting inside lists |
| `test_unicode_keys_and_values_preserve_order` | unicode (emoji, CJK, RTL, combining mark) |
| `test_unicode_keys_keep_insertion_order` | unicode keys, decomposed vs ASCII |
| `test_numeric_boundary_values_keep_field_order` | numeric boundary (`0`, `-0.0`, `2^53±1`, `0.1`) |
| `test_property_access_on_a_list_is_a_runtime_error` | **asserts a failure**: type mismatch → `Error::Runtime` |
| `test_record_is_not_indexable_at_any_position` | **asserts a failure**: out-of-bounds — index `0`, `1`, `2`, `19`, `-1`, `999` on a record are all clean `Error::Runtime`, never a panic |
| `test_malformed_json_object_is_an_error_not_a_reordered_record` | **asserts a failure**: malformed input → `Error::Runtime` |
| `test_record_literal_missing_colon_is_a_parse_error` | **asserts a failure**: malformed input → `Error::Parser` |
| `test_record_equality_ignores_field_order` | regression guard: order must not leak into `expect` |
| `test_expect_on_a_record_compares_field_values` | **asserts a failure**: records differing in key or value must not compare equal |

Quota: 25 new `#[test]` functions (floor 3); 2 named `edge_*`; 5 tests assert a
failure is produced; 0 new `#[ignore]` / `skip` / `allow(clippy::)`; 0
pre-existing tests modified or re-scoped.

### The ordering tests were verified to fail without the fix

An in-process loop cannot prove a *per-process* defect is gone, so the cross-process
test spawns the real `rb` 50 times via `CARGO_BIN_EXE_rb` and compares full
stdout. To prove it can actually fail, `Fields` was temporarily pointed back at
`HashMap` and the suite re-run:

```
test edge_twenty_key_record_is_identical_across_fifty_processes ... FAILED
assertion `left == right` failed: process 2 of 50 printed a different field order than process 1
  left:  "{lambda: 13, zebra: 0, mango: 2, gamma: 8, alpha: 1, ... }"
  right: "{omicron: 17, eta: 11, gamma: 8, lambda: 13, pi: 18, ... }"
```

`cargo test --test record_order_test` then reported `14 failed; 11 passed` — and
that count is itself nondeterministic, since it depends on which random orders
come up (single-field and empty records cannot fail). `src/value.rs` was then
reverted from backup and the suite returned to `25 passed; 0 failed`.

## Definition of done

| Requirement | Where |
|---|---|
| record display order is stable across processes | `edge_twenty_key_record_is_identical_across_fifty_processes` |
| `json.stringify` round-trips and is stable | `test_json_stringify_round_trips_through_parse`, `test_json_stringify_preserves_insertion_order`, and the stringify line asserted by the cross-process test |
| edge test: 20-key record prints in insertion order, run 50x, identical | `edge_twenty_key_record_is_identical_across_fifty_processes` (50 processes) and `edge_twenty_key_record_is_identical_across_fifty_runs` (50 in-process) |
| duplicate-key last-wins semantics tested | `test_duplicate_key_takes_the_last_value_and_the_first_position`, `test_duplicate_key_in_json_takes_the_last_value` |

## Edge-case matrix

- **empty** — covered: `test_empty_record_displays_as_empty_braces` (`{}` display,
  `json.stringify`, `json.parse("{}")`).
- **singleton** — covered: `test_singleton_record_displays_its_only_field`.
- **boundary** — covered: `test_first_and_last_field_are_reachable_in_order`
  (first and last field of a 20-field record, both display and read paths).
- **out_of_bounds** — covered: `test_record_is_not_indexable_at_any_position`
  (indices `0`, `1`, `2`, `19`, `-1`, `999` all rejected as `Error::Runtime`, no
  panic) and `test_missing_key_reads_as_nothing_and_does_not_grow_the_record`.
- **type_mismatch** — covered:
  `test_property_access_on_a_list_is_a_runtime_error`.
- **numeric_boundary** — covered: `test_numeric_boundary_values_keep_field_order`
  (`0`, `-0.0`, `2^53±1`, `0.1`; note `2^53+1` and `-0.0` print as their f64
  neighbours, which is pre-existing `Value::Display` behaviour, not ordering).
- **unicode** — covered: `test_unicode_keys_and_values_preserve_order`,
  `test_unicode_keys_keep_insertion_order` (emoji, CJK, RTL, combining mark).
- **nesting_recursion** — nesting covered (3 levels, records inside lists).
  *Recursion* is N/A: call-stack depth is orthogonal to field ordering and is
  untouched by this phase.
- **duplicate_missing_keys** — covered by four tests (listed above).
- **malformed_input** — covered: unterminated JSON object (`Error::Runtime`),
  record literal with no colon (`Error::Parser`).
- **resource_limit** — N/A: this phase adds no allocation, recursion or loop; it
  swaps one map for another. `IndexMap` has the same O(1) lookup bounds as
  `HashMap`, and the 50-process test completes in ~0.1 s.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff, exit 0 |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings, exit 0 |
| `cargo test --all-targets` | **92 passed, 0 failed, 0 ignored** (5 lib + 0 bin + 13 `discovery_test` + 21 `expect_test` + 25 `record_order_test` + 8 `redblue_suite_test` + 5 `redblue_test` + 15 `span_test`) |
| `./rbops/verify.sh phase-004` | **could not run — `rbops/verify.sh` does not exist in this checkout** (`exit 127`). See FINDINGS.md §1 |

Plain `cargo test` (the command CI runs) was also executed: same 92 passed,
0 failed, 0 ignored.

Backwards compatibility was re-checked by hand against the freshly built
binary: all six `examples/*.rb` run clean — `hello`, `fizzbuzz`, `files`,
`formats`, `time`, `test_arithmetic`.

## Invariants touched

- None. `Value`'s variant names and arity are unchanged (`Record(Fields)`,
  `Object(String, Fields)`); only the storage type behind the `Fields` alias
  changed. `Error::{Lexer,Parser,Analyzer,Runtime,Io}` unchanged. `.rb`
  extension, `end` blocks, `set x to <expr>`, `say`, `{interp}` strings all
  untouched. No existing test weakened, skipped or re-scoped.

## Known gaps / follow-ups

- **`rbops/verify.sh` is absent from this checkout.** There is no `rbops/`
  directory, no `.opencode/agent/`. The fourth gate could not be executed and, per
  hard rule 1, nothing under `rbops/` was authored or edited to work around it.
  The three gates that do exist are green. Recorded in `FINDINGS.md` §1.
- Scope left deliberately alone, `FINDINGS.md` §3, §5 and §6: VM scope maps
  (`globals`, `locals` in `src/vm.rs`) are still `HashMap`; `l[3]` reads as
  `nothing` and `l[-1]` counts from the end; two assertion helpers in
  `src/testing/assertions.rs` are dead, one of them inverted. None of these
  reaches record field order.
- Pre-existing and unrelated, `FINDINGS.md` §2: `set person to record { … }`, the
  form documented at `SPEC.md:107`, does not parse. Not touched here.