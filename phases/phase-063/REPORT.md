# Phase 063 — Print values three ways: `.pretty`, `.json`, `.table`

## What changed

Baseline is `b21821d` (the commit before this phase's work landed). The
adopted work is in `a31d067`; this round's changes are the last three rows.

| File | Lines | What |
|---|---|---|
| src/repl/render.rs | +634 −0 (new module) | `pretty` (one indent level per depth, `MAX_PRETTY_DEPTH = 64`, a value nested past the budget is refused rather than truncated), `json` (the language's own `runtime::json_stringify`, so the document is the one `json.stringify` writes), `table` (union-of-keys header row, empty cells for missing keys, a named refusal for a non-list / non-record item) |
| src/repl/commands.rs | +100 −0 | the three command rows (`pretty`, `json`, `table`, each also under `:`), their parse entries and the parse test |
| src/repl/mod.rs | +356 −5 | `print_pretty`/`print_json`/`print_table` dispatch, `pretty_answer`/`json_answer`/`table_answer`/`value_answer` (one-expression gate, effectful-call refusal, fork evaluation — the same three gates `:type` uses), the `:help` rows, and the `mod printing` tests |
| src/repl/introspect.rs | +16 −8 | `expression_of_for(program, command)`: the refusal names the command the user typed (`.pretty`) rather than `:type` |
| src/runtime.rs | +11 −2 | `json_escape_text` and `json_stringify` widened to `pub(crate)` so the REPL's `.json` prints the same document `json.stringify` writes |
| src/error.rs | +11 −0 (this round) | `pub(crate) fn text_cells(text)`: how many terminal cells a text is drawn in, so the table's columns are padded by what a terminal draws and not by character count |
| src/repl/render.rs | (this round, in the row above) | the table's header row is spelled the way the cells are: a key holding a line break no longer breaks the header row in two |
| tests/repl_printing_test.rs | +256 −0 | new integration suite: sessions piped into the real `rb` binary, asserted on stdout |
| tests/unreviewed_work_ack_test.rs | +109 −36 (this round) | the delivery-commit rule now covers phase-063's delivery subject as well as phase-084's (`DELIVERY_SUBJECT_PREFIXES`), the stale SHA entry for the previous round's delivery commit (`ba0a0db`) is gone, and a new test pins the rule — see "Known gaps" for why the rule, and not a SHA, is what acknowledges a delivery commit |

## Tests added

32 new `#[test]` functions (26 in `src/`, 6 in `tests/repl_printing_test.rs`),
zero ignored, zero skipped, zero `allow(clippy::` suppressions.

| Test | Edge class covered |
|---|---|
| `edge_pretty_of_a_ten_deep_nesting_terminates_with_bounded_lines` | nesting/recursion, resource limit |
| `edge_pretty_refuses_a_value_nested_past_its_budget` | resource limit — asserts the **failure** (depth > 64 is refused with an error, not truncated) |
| `pretty_output_reads_back_as_the_value_it_came_from` | acceptance: `.pretty` round-trips through the language's own `json.parse` to an equal value |
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
| `edge_a_key_holding_a_newline_stays_on_one_line` (this round) | malformed cell content: a line break in a *key* stays one header row |
| `edge_table_aligns_a_wide_character_column_by_the_cells_it_is_drawn_in` (this round) | unicode: a CJK column is padded by terminal cells, so the columns still line up |
| `the_three_printing_commands_parse` / `the_three_printing_commands_answer_about_an_expression` / `the_three_printing_commands_are_dispatched_and_not_run_as_source` | the commands reach the dispatcher and answer about an expression |
| `edge_a_printing_command_refuses_a_program_and_the_session_never_knows_it_happened` / `edge_a_printing_command_does_not_change_the_session` | malformed input: statements refused, no binding changed |
| `edge_a_printing_command_of_a_line_that_does_not_parse_is_refused` (unit + integration) | malformed input: `1 +`, `[1, 2`, `{ a: }` → `Error:` — asserts the **failure** |
| `edge_help_names_the_three_printing_commands` | acceptance: `:help` lists all three, and the completer offers them |
| `edge_a_table_of_one_record_names_its_keys_once` | singleton: one record is a header row and one row |
| `edge_the_three_printing_commands_print_three_shapes` / `edge_the_json_a_session_prints_is_valid_json` / `edge_table_of_a_value_that_is_not_a_list_of_records_is_refused` / `edge_a_table_of_an_empty_list_says_so_and_does_not_panic` / `edge_a_printed_text_keeps_its_own_script` / `edge_a_printing_command_of_a_line_that_does_not_parse_is_refused` (integration) | end-to-end through the real binary, incl. emoji/CJK/RTL text and a 20 s liveness bound (resource limit) |
| `edge_the_delivery_rule_covers_each_phase_in_flight_and_no_other` (this round, `tests/unreviewed_work_ack_test.rs`) | the history audit: every phase in flight is covered by the delivery rule, and a phase that is not in flight is still unreviewed work |

### Mandatory edge-case matrix

- **empty** — covered: `pretty_of_an_empty_thing…` (`[]`, `{}`, `""`, `nothing`, `yes`, `no`), `edge_table_of_an_empty_list…` (unit + integration)
- **singleton** — covered: `edge_a_table_of_one_record_names_its_keys_once`, `table_aligns_its_columns_under_one_header_row`
- **boundary** — covered: `edge_pretty_refuses_a_value_nested_past_its_budget` (depth 65 refused, 64 shown)
- **out_of_bounds** — N/A: these commands take no user-written index; `.table` addresses cells by record key, and a missing key is the empty cell covered by the ragged-row tests
- **type_mismatch** — covered: `edge_table_refuses_anything_that_is_not_a_list_of_records` (a number, and a list of numbers naming the item), and `edge_the_json_a_session_prints_is_valid_json` (a yes/no is a JSON boolean, not a number)
- **numeric_boundary** — covered: `edge_json_of_a_number_that_is_not_finite_is_null…` (NaN, ±∞ → `null`, per SPEC.md §"A number is always finite"); `json_is_valid_json_per_serde_json` (integers and floats). N/A for `2^53±1` and `-2^31`: these commands print a value the session already holds, and the value's own arithmetic is the language's, unchanged by this phase
- **unicode** — covered: `edge_json_keeps_unicode_text_as_it_is` (café, 日本語, مرحبا, 🎉, a combining mark), `edge_a_printed_text_keeps_its_own_script` (integration), `edge_table_aligns_a_wide_character_column_by_the_cells_it_is_drawn_in` (CJK padding)
- **nesting_recursion** — covered: `edge_pretty_of_a_ten_deep_nesting_terminates_with_bounded_lines`, `edge_pretty_refuses_a_value_nested_past_its_budget`
- **duplicate/missing_keys** — covered (missing): ragged rows + later-key column. Duplicate keys N/A: a record is an `IndexMap` — a key is held once by construction, and `keys_of` de-duplicates the union defensively
- **malformed_input** — covered: both parse-refusal tests (`.pretty 1 +`, `.json [1, 2`, `.table { a: }`), the statement/multi-statement refusal, and the two newline tests (a cell and a key)
- **resource_limit** — covered: the 64-level pretty budget (refusal, not truncation); the integration sessions have a 20 s liveness bound that fails a hang

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 71 test binaries, 1999 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh phase-063 | pass |

## Invariants touched

- None of the language surface: no grammar, keyword, `Value` variant or `Error` variant changed, and `examples/*.rb` and `modules/*.rb` are untouched.
- `src/runtime.rs`: `json_escape_text`/`json_stringify` visibility widened `pub(crate)` (not part of the public API); behaviour unchanged — `.json` deliberately prints the identical document `json.stringify` writes.
- `src/error.rs`: one new `pub(crate) fn text_cells`; the error renderer's own drawing is unchanged (it still uses `char_cells` and `display_width`).
- The `:type` refusal messages are unchanged (`expression_of` still passes `":type"`).
- Additive REPL command surface only: `.pretty`/`.json`/`.table` (and the `:` spellings), listed in `:help`.

## Known gaps / follow-ups

- **The delivery commit of this phase is acknowledged by subject, not by SHA.** The pipeline creates that commit *after* the run that wrote the work and re-creates it on every round (round 1's `ba0a0db` became `a31d067`), so a SHA written into the acknowledgement list is stale the moment the next round lands — that is exactly what the previous attempt's `ba0a0db` entry did, and the history audit (`tests/unreviewed_work_ack_test.rs`) reported it as stale while reporting the re-created commit as unreviewed. The audit's own rule exists for this case, so the rule now carries a prefix per phase in flight (`DELIVERY_SUBJECT_PREFIXES`) and no SHA is pinned. The cost: the rule is a list, and a phase that comes along later needs its prefix added.
- The hand-written terminal-width table in `src/error.rs` is incomplete for some emoji (⭐, ✨, the regional indicators), so a `.table` cell holding one is padded one cell short. → FINDINGS.md §1
- A `.table` cell holding a nested list or record is written on one line, with the single-line `Display`. → FINDINGS.md §2
- The session's own answer to a program is still the single-line `Display` (`src/repl/mod.rs:1370`); the three commands are opt-in. → FINDINGS.md §3
