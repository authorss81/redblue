# Phase 063 — Print values three ways: `.pretty`, `.json`, `.table`

## What changed

Baseline is `ba0a0db^` (the state before this phase's adopted work landed).

| File | Lines | What |
|---|---|---|
| src/repl/render.rs | +562 −0 | new module: `pretty` (one indent level per depth, `MAX_PRETTY_DEPTH = 64` refusal past the budget), `json` (delegates to the language's own `runtime::json_stringify`), `table` (union-of-keys header row, empty cells for missing keys, named refusal for a non-list / non-record item) |
| src/repl/commands.rs | +100 −0 | the three command rows (`pretty`, `json`, `table`, each also under `:`), their parse entries and parse tests |
| src/repl/mod.rs | +277 −5 (commit) / +361 −6 (incl. this phase's 3 new unit tests) | `print_pretty`/`print_json`/`print_table` dispatch, `pretty_answer`/`json_answer`/`table_answer`/`value_answer` (one-expression gate, effectful-call refusal, fork evaluation — the same three gates `:type` uses), `:help` entries, and the `mod printing` tests |
| src/repl/introspect.rs | +16 −8 | `expression_of_for(program, command)`: the refusal names the command the user typed (`.pretty`) rather than `:type` |
| src/runtime.rs | +11 −2 | `json_escape_text` and `json_stringify` widened to `pub(crate)` so the REPL's `.json` prints the same document `json.stringify` writes |
| tests/repl_printing_test.rs | +256 −0 | new integration suite: sessions piped into the real `rb` binary, asserted on stdout |
| tests/unreviewed_work_ack_test.rs | +5 −0 | the pipeline's `resume phase-063: adopt preserved work` commit (`ba0a0db`) added to `ACKNOWLEDGED`/`MEASURED`, as that test's own failure message instructs |

## Tests added

29 new `#[test]` functions (23 in `src/`, 6 in `tests/repl_printing_test.rs`), zero ignored, zero skipped.

| Test | Edge class covered |
|---|---|
| `edge_pretty_of_a_ten_deep_nesting_terminates_with_bounded_lines` | nesting/recursion, resource limit |
| `edge_pretty_refuses_a_value_nested_past_its_budget` | resource limit — asserts the **failure** (depth > 64 is refused with an error, not truncated) |
| `pretty_output_reads_back_as_the_value_it_came_from` | acceptance: `.pretty` round-trips through `json.parse` to an equal value |
| `pretty_indents_one_level_per_depth` / `pretty_of_an_empty_thing_is_closed_on_the_same_line` | nesting, empty (`[]`/`{}`/`nothing`/`""`/`yes`/`no`) |
| `json_is_valid_json_per_serde_json` | numbers, texts, yes/no, `nothing` → `null`, lists, records — parsed by `serde_json` |
| `edge_json_of_a_number_that_is_not_finite_is_null_and_never_a_nan_literal` | numeric boundary: NaN/±Infinity → `null`, asserts no `NaN` literal is ever written |
| `json_escapes_the_characters_a_text_may_hold` | unicode/escapes: quote, backslash, newline in a text |
| `edge_json_keeps_unicode_text_as_it_is` | unicode: CJK kept as-is, valid JSON |
| `table_aligns_its_columns_under_one_header_row` | singleton/shared key set, keys named once |
| `edge_table_of_ragged_rows_fills_the_missing_cells_with_nothing` | duplicate/missing keys: missing key → empty cell, not an error |
| `edge_table_gives_a_later_key_its_own_column` | duplicate/missing keys: a key only a later row holds still gets a column |
| `edge_table_of_an_empty_list_says_so_and_does_not_panic` | empty: named message, never a panic |
| `edge_table_refuses_anything_that_is_not_a_list_of_records` | type mismatch — asserts the **failure**, and names the offending item |
| `edge_a_cell_holding_a_newline_stays_on_one_line` | malformed cell content: newline stays one row |
| `the_three_printing_commands_parse` / `the_three_printing_commands_answer_about_an_expression` / `the_three_printing_commands_are_dispatched_and_not_run_as_source` | the commands reach the dispatcher and answer about an expression |
| `edge_a_printing_command_refuses_a_program_and_the_session_never_knows_it_happened` / `edge_a_printing_command_does_not_change_the_session` | malformed input: statements refused, no binding changed |
| `edge_a_printing_command_of_a_line_that_does_not_parse_is_refused` (unit + integration) | malformed input: `1 +`, `[1, 2`, `{ a: }` → `Error:` — asserts the **failure** |
| `edge_help_names_the_three_printing_commands` | acceptance: `:help` lists all three |
| `edge_the_three_printing_commands_print_three_shapes` / `edge_the_json_a_session_prints_is_valid_json` / `edge_table_of_a_value_that_is_not_a_list_of_records_is_refused` / `edge_a_table_of_an_empty_list_says_so_and_does_not_panic` / `edge_a_printed_text_keeps_its_own_script` / `edge_a_printing_command_of_a_line_that_does_not_parse_is_refused` (integration) | end-to-end through the real binary, incl. emoji/CJK/RTL text and a 20 s liveness bound (resource limit) |

### Mandatory edge-case matrix

- **empty** — covered: `pretty_of_an_empty_thing…` (`[]`, `{}`, `""`, `nothing`), `edge_table_of_an_empty_list…`
- **singleton** — covered: `edge_a_table_of_one_record_names_its_keys_once`, `table_aligns_its_columns…`
- **boundary** — covered: `edge_pretty_refuses_a_value_nested_past_its_budget` (depth 65 refused, 64 shown)
- **out_of_bounds** — N/A: these commands take no user-written index; `.table` addresses cells by record key, and a missing key is the empty cell covered by the ragged-row tests
- **type_mismatch** — covered: `edge_table_refuses_anything_that_is_not_a_list_of_records` (number, and a list of numbers naming the item)
- **numeric_boundary** — covered: `edge_json_of_a_number_that_is_not_finite_is_null…` (NaN, ±∞); `json_is_valid_json_per_serde_json` (integers/floats)
- **unicode** — covered: `edge_json_keeps_unicode_text_as_it_is`, `edge_a_printed_text_keeps_its_own_script` (café, 日本語, مرحبا, 🎉)
- **nesting_recursion** — covered: `edge_pretty_of_a_ten_deep_nesting_terminates…`, `edge_pretty_refuses_a_value_nested_past_its_budget`
- **duplicate/missing_keys** — covered (missing): ragged rows + later-key column. Duplicate keys N/A: a record is a map — a key is held once by construction, and `keys_of` de-duplicates the union defensively
- **malformed_input** — covered: both parse-refusal tests (`.pretty 1 +`, `.json [1, 2`, `.table { a: }`), plus statement/multi-statement refusal
- **resource_limit** — covered: 64-level pretty budget (refusal, not truncation); integration sessions have a 20 s liveness bound that fails a hang

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 69 test binaries, 1925 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh phase-063 | pass |

## Invariants touched

- None of the language surface: no grammar, keyword, `Value` variant or `Error` variant changed.
- `src/runtime.rs`: `json_escape_text`/`json_stringify` visibility widened `pub(crate)` (not part of the public API); behaviour unchanged — `.json` deliberately prints the identical document `json.stringify` writes.
- The `:type` refusal messages are unchanged (`expression_of` still passes `":type"`).
- Additive REPL command surface only: `.pretty`/`.json`/`.table` (and `:` spellings), listed in `:help`.

## Known gaps / follow-ups

- `.table` pads by character count (`cell.chars().count()` in `layout`, src/repl/render.rs:214), not terminal display width: a cell holding CJK/emoji misaligns by column in a real terminal. The text is intact (tested); only the padding is off. → FINDINGS.md candidate.
- No FINDINGS.md was written this phase; the gap above is recorded here.
