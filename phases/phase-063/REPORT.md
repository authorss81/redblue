# Phase 063 — Print values three ways: `.pretty`, `.json`, `.table`

## What changed

Baseline is `b21821d` (the commit before this phase's work landed). The
adopted work is in `a31d067`; this round's changes are the last rows. The line
counts are measured against the baseline.

| File | Lines | What |
|---|---|---|
| src/repl/render.rs | +1176 −0 (new module) | `pretty` (one indent level per depth), `json` (the language's own JSON writer, with the renderers' depth budget on it and a refusal for a value JSON cannot hold), `table` (union-of-keys header row, empty cells for missing keys, a named refusal for a non-list / non-record item, cells escaped so a terminal cannot be mangled by them) |
| src/repl/commands.rs | +100 −0 | the three command rows (`pretty`, `json`, `table`, each also under `:`), their parse entries and the parse test |
| src/repl/mod.rs | +378 −5 | `print_pretty`/`print_json`/`print_table` dispatch, `pretty_answer`/`json_answer`/`table_answer`/`value_answer` (one-expression gate, effectful-call refusal, fork evaluation — the same three gates `:type` uses), the `:help` rows, and the `mod printing` tests |
| src/repl/introspect.rs | +126 −12 | `expression_of_for(program, command, wording)`: the refusal names the command the user typed (`.pretty`) rather than `:type`, and each command carries its own wording |
| src/runtime.rs | +11 −2 | `json_escape_text` and `json_stringify` widened to `pub(crate)` so the REPL's `.json` prints the same document `json.stringify` writes |
| src/error.rs | +11 −0 | `pub(crate) fn text_cells(text)`: how many terminal cells a text is drawn in, so the table's columns are padded by what a terminal draws and not by character count |
| tests/repl_printing_test.rs | +441 −0 | new integration suite: sessions piped into the real `rb` binary, asserted on stdout |
| tests/unreviewed_work_ack_test.rs | +109 −36 | the delivery-commit rule now covers phase-063's delivery subject as well as phase-084's (`DELIVERY_SUBJECT_PREFIXES`), the stale SHA entry for the previous round's delivery commit (`ba0a0db`) is gone, and a new test pins the rule — see "Known gaps" for why the rule, and not a SHA, is what acknowledges a delivery commit |
| src/repl/render.rs | (this round, in the row above) | **one depth budget for all three renderers.** `MAX_PRETTY_DEPTH` became `MAX_RENDER_DEPTH`, and `.json` and `.table` now refuse a value nested past it instead of recursing: the same value was refused cleanly by `.pretty` and killed the process by `.json` (a stack overflow in Rust is a fatal abort, not a catchable panic) |
| src/repl/render.rs | (this round, in the row above) | **a function, a builtin and an object are not written as `null`.** `.pretty` shows them the way the language writes them (`<function f>`); `.json` refuses the document by name, because JSON has no literal for them and `null` says "nothing" about a value that is not nothing |
| src/repl/render.rs | (this round, in the row above) | **a cell is escaped** so an escape sequence or a right-to-left mark cannot mangle the terminal — and the column is padded by the cells the terminal draws, which is the escaped form |
| src/repl/render.rs | (this round, in the row above) | **the layout drops only the padding it added.** A cell that ends in spaces keeps them, in the last column as much as in any other: the same value is not printed two different ways depending on where it sat |
| src/repl/introspect.rs | (this round, in the row above) | **`:type`'s refusal words are what they always were** ("described", "describe"), and they are pinned by a test; the printing commands bring their own wording ("shown") instead of changing an existing command's |
| src/repl/render.rs | (this round, in the row above) | **only the characters a terminal acts on are escaped.** A quote, an apostrophe, a backslash, a combining mark and a letter of any script are drawn by every terminal there is, so a cell that spelled them out would show the reader a character the value does not hold: the cell is what `say` would have printed, and the only thing it may change is what the terminal would do with it |
| src/repl/render.rs | (this round, in the row above) | **the layout keeps the content's own trailing spaces whatever follows them.** The whitespace the layout adds after the last cell that holds something — the padding and the separator — is the layout's to drop; the spaces a cell ends in are the content's to keep, and dropping them with the layout's made `ada ` print as `ada` in one row and as `ada ` in the next |
| src/repl/mod.rs | (this round, in the row above) | **a refusal is painted the way `:type` paints it.** The three printers paint through `paint_diagnostic`, not through one whole-string paint: the failure is painted and the reader's own source line is left as they typed it, so a printing command answers a misspelling with the same picture of it that every other command gives |
| src/repl/mod.rs | (this round, in the row above) | **the `.json` doc comment says what the renderer does.** "The document the language's own `json.stringify` writes" is true for every value JSON can hold and not for the three it cannot, so the comment now says so — the wording this report already used |

## Tests added

49 new `#[test]` functions (38 in `src/`, 11 in `tests/repl_printing_test.rs`),
zero ignored, zero skipped, zero `allow(clippy::` suppressions. 18 of them are
this round's.

| Test | Edge class covered |
|---|---|
| `edge_pretty_of_a_ten_deep_nesting_terminates_with_bounded_lines` | nesting/recursion, resource limit |
| `edge_pretty_refuses_a_value_nested_past_its_budget` | resource limit — asserts the **failure** (depth > 64 is refused with an error, not truncated) |
| `edge_json_of_a_value_nested_past_its_budget_is_refused` (this round) | resource limit — the same budget for the JSON renderer: asserts the **failure**, and that the refusal names the budget |
| `edge_table_of_a_value_nested_past_its_budget_is_refused` (this round) | resource limit — the same budget for a table cell: asserts the **failure** |
| `edge_a_value_just_inside_the_budget_is_still_written` (this round) | boundary — the budget is a refusal, not a truncation: a value just inside it is still written whole |
| `pretty_output_reads_back_as_the_value_it_came_from` | acceptance: `.pretty` round-trips through the language's own `json.parse` to an equal value |
| `pretty_indents_one_level_per_depth` / `pretty_of_an_empty_thing_is_closed_on_the_same_line` | nesting, empty (`[]`/`{}`/`nothing`/`""`/`yes`/`no`) |
| `json_is_valid_json_per_serde_json` | numbers, texts, yes/no, `nothing` → `null`, lists, records — parsed by `serde_json` |
| `edge_json_of_a_number_that_is_not_finite_is_null_and_never_a_nan_literal` | numeric boundary: NaN/±Infinity → `null`, asserts no `NaN` literal is ever written |
| `json_escapes_the_characters_a_text_may_hold` | unicode/escapes: quote, backslash, newline in a text |
| `edge_json_keeps_unicode_text_as_it_is` | unicode: CJK kept as-is, valid JSON |
| `edge_pretty_of_a_function_is_not_null` (this round) | type mismatch — a function is written in its own form, never as the `null` that means nothing |
| `edge_json_of_a_function_is_refused_and_not_null` (this round) | type mismatch — asserts the **failure**: JSON has no literal for a function, so the document is refused by name |
| `edge_json_of_a_builtin_or_an_object_is_refused_and_not_null` (this round) | type mismatch — the other two leaves with no JSON literal, refused and named |
| `edge_a_table_cell_holding_a_function_is_not_null` (this round) | type mismatch — a table cell holds the function's own form |
| `table_aligns_its_columns_under_one_header_row` | singleton/shared key set, keys named once |
| `edge_table_of_ragged_rows_fills_the_missing_cells_with_nothing` | duplicate/missing keys: missing key → empty cell, not an error |
| `edge_table_gives_a_later_key_its_own_column` | duplicate/missing keys: a key only a later row holds still gets a column |
| `edge_table_of_an_empty_list_says_so_and_does_not_panic` | empty: named message, never a panic |
| `edge_table_refuses_anything_that_is_not_a_list_of_records` | type mismatch — asserts the **failure**, and names the offending item |
| `edge_a_cell_holding_a_newline_stays_on_one_line` | malformed cell content: newline stays one row |
| `edge_a_key_holding_a_newline_stays_on_one_line` (this round) | malformed cell content: a line break in a *key* stays one header row |
| `edge_table_aligns_a_wide_character_column_by_the_cells_it_is_drawn_in` (this round) | unicode: a CJK column is padded by terminal cells, so the columns still line up |
| `edge_a_cell_a_terminal_would_mangle_is_escaped_rather_than_written_out` (this round) | unicode/escapes — an ESC sequence and a right-to-left mark are spelled out, and the column is padded by the cells the terminal draws |
| `edge_a_cell_shows_the_characters_a_terminal_draws_as_they_are` (this round) | unicode/escapes — a quote, an apostrophe and a combining mark are drawn as they are: only the characters a terminal acts on are escaped |
| `edge_a_cell_ending_in_spaces_keeps_them_in_every_column` (this round) | malformed cell content — the content's own trailing spaces survive in the last column, so the same value is not printed two ways |
| `edge_a_cell_ending_in_spaces_before_an_empty_cell_keeps_them_too` (this round) | malformed cell content — the same spaces survive when the cell after them is empty: the layout's separator and padding go, the content's spaces stay |
| `the_three_printing_commands_parse` / `the_three_printing_commands_answer_about_an_expression` / `the_three_printing_commands_are_dispatched_and_not_run_as_source` | the commands reach the dispatcher and answer about an expression |
| `edge_a_printing_command_refuses_a_program_and_the_session_never_knows_it_happened` / `edge_a_printing_command_does_not_change_the_session` | malformed input: statements refused, no binding changed |
| `edge_a_printing_command_of_a_line_that_does_not_parse_is_refused` (unit + integration) | malformed input: `1 +`, `[1, 2`, `{ a: }` → `Error:` — asserts the **failure** |
| `edge_help_names_the_three_printing_commands` | acceptance: `:help` lists all three, and the completer offers them |
| `edge_a_table_of_one_record_names_its_keys_once` | singleton: one record is a header row and one row |
| `edge_the_type_refusal_keeps_the_wording_it_has_always_had` (this round) | acceptance — an existing command's user-facing text is pinned word for word, so it cannot change silently again |
| `edge_a_printing_command_refusal_names_its_own_command_and_its_own_words` (this round) | acceptance — the printing commands' refusal names the command the user typed and its own words, and never mentions another command |
| `edge_the_three_printing_commands_print_three_shapes` / `edge_the_json_a_session_prints_is_valid_json` / `edge_table_of_a_value_that_is_not_a_list_of_records_is_refused` / `edge_a_table_of_an_empty_list_says_so_and_does_not_panic` / `edge_a_printed_text_keeps_its_own_script` / `edge_a_printing_command_of_a_line_that_does_not_parse_is_refused` (integration) | end-to-end through the real binary, incl. emoji/CJK/RTL text and a 20 s liveness bound (resource limit) |
| `edge_a_value_nested_past_the_budget_is_refused_and_the_session_survives` (integration, this round) | resource limit — a session that builds a 2000-deep value is refused by `.json` and by `.table`, and the session goes on: asserts the **failure**, and that the process is still alive afterwards |
| `edge_a_table_cell_a_terminal_would_mangle_is_escaped_rather_than_written_out` (integration, this round) | unicode/escapes — no raw escape sequence reaches the terminal through a real session |
| `edge_the_json_of_a_function_is_refused_and_not_null` (integration, this round) | type mismatch — the refusal reaches the session's stdout, and `.pretty` shows the function's own form |
| `edge_a_printing_command_answers_a_misspelling_exactly_as_type_does` (integration, this round) | acceptance — the three printers and `:type` answer a line that does not parse byte for byte alike, colour on: the same prefix, the same renderer, the same colour contract |
| `edge_a_table_cell_shows_the_characters_a_terminal_draws_as_they_are` (integration, this round) | unicode/escapes — a cell holds the text as the value holds it, through a real session |
| `edge_the_delivery_rule_covers_each_phase_in_flight_and_no_other` (this round, `tests/unreviewed_work_ack_test.rs`) | the history audit: every phase in flight is covered by the delivery rule, and a phase that is not in flight is still unreviewed work |

### Mandatory edge-case matrix

- **empty** — covered: `pretty_of_an_empty_thing…` (`[]`, `{}`, `""`, `nothing`, `yes`, `no`), `edge_table_of_an_empty_list…` (unit + integration)
- **singleton** — covered: `edge_a_table_of_one_record_names_its_keys_once`, `table_aligns_its_columns_under_one_header_row`
- **boundary** — covered: `edge_pretty_refuses_a_value_nested_past_its_budget` (depth 65 refused, 64 shown), `edge_a_value_just_inside_the_budget_is_still_written` (the budget is a refusal, not a truncation)
- **out_of_bounds** — N/A: these commands take no user-written index; `.table` addresses cells by record key, and a missing key is the empty cell covered by the ragged-row tests
- **type_mismatch** — covered: `edge_table_refuses_anything_that_is_not_a_list_of_records` (a number, and a list of numbers naming the item), `edge_the_json_a_session_prints_is_valid_json` (a yes/no is a JSON boolean, not a number), and this round's three leaf tests (`edge_pretty_of_a_function_is_not_null`, `edge_json_of_a_function_is_refused_and_not_null`, `edge_json_of_a_builtin_or_an_object_is_refused_and_not_null`, `edge_a_table_cell_holding_a_function_is_not_null`) — a function, a builtin and an object are never written as the `null` that means nothing
- **numeric_boundary** — covered: `edge_json_of_a_number_that_is_not_finite_is_null…` (NaN, ±∞ → `null`, per SPEC.md §"A number is always finite"); `json_is_valid_json_per_serde_json` (integers and floats). N/A for `2^53±1` and `-2^31`: these commands print a value the session already holds, and the value's own arithmetic is the language's, unchanged by this phase
- **unicode** — covered: `edge_json_keeps_unicode_text_as_it_is` (café, 日本語, مرحبا, 🎉, a combining mark), `edge_a_printed_text_keeps_its_own_script` (integration), `edge_table_aligns_a_wide_character_column_by_the_cells_it_is_drawn_in` (CJK padding), this round's `edge_a_cell_a_terminal_would_mangle_is_escaped_rather_than_written_out` (unit + integration) — an escape sequence and a right-to-left mark are escaped, not written out — and this round's `edge_a_cell_shows_the_characters_a_terminal_draws_as_they_are` (unit + integration) — a quote, an apostrophe and a combining mark are drawn as they are
- **nesting_recursion** — covered: `edge_pretty_of_a_ten_deep_nesting_terminates_with_bounded_lines`, `edge_pretty_refuses_a_value_nested_past_its_budget`, `edge_json_of_a_value_nested_past_its_budget_is_refused`, `edge_table_of_a_value_nested_past_its_budget_is_refused`, `edge_a_value_nested_past_the_budget_is_refused_and_the_session_survives` (integration)
- **duplicate/missing_keys** — covered (missing): ragged rows + later-key column. Duplicate keys N/A: a record is an `IndexMap` — a key is held once by construction, and `keys_of` de-duplicates the union defensively
- **malformed_input** — covered: both parse-refusal tests (`.pretty 1 +`, `.json [1, 2`, `.table { a: }`), the statement/multi-statement refusal, the two newline tests (a cell and a key), and this round's `edge_a_cell_ending_in_spaces_keeps_them_in_every_column` and `edge_a_cell_ending_in_spaces_before_an_empty_cell_keeps_them_too`
- **resource_limit** — covered: the 64-level budget, now shared by **all three** renderers (a refusal, not a truncation) — `edge_pretty_refuses…`, `edge_json_of_a_value_nested_past_its_budget_is_refused`, `edge_table_of_a_value_nested_past_its_budget_is_refused`, `edge_a_value_just_inside_the_budget_is_still_written`, and the integration session that builds a 2000-deep value and is still alive afterwards; the integration sessions have a 20 s liveness bound that fails a hang.
  - **The gap this does not cover:** the session's *own* answer (`= value`, `src/repl/mod.rs:1370`) and the language's own `say` and `json.stringify` still follow a value as deep as it goes, and a value that deep is a fatal stack overflow rather than a catchable error. That is pre-existing and is not this phase's surface → FINDINGS.md §4 and §5.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2017 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh phase-063 | pass |

## Invariants touched

- None of the language surface: no grammar, keyword, `Value` variant or `Error` variant changed, and `examples/*.rb` and `modules/*.rb` are untouched.
- `src/runtime.rs`: `json_escape_text`/`json_stringify` visibility widened `pub(crate)` (not part of the public API); behaviour unchanged — `.json` deliberately prints the identical document `json.stringify` writes, for every value JSON can hold.
- `src/error.rs`: one new `pub(crate) fn text_cells`; the error renderer's own drawing is unchanged (it still uses `char_cells` and `display_width`).
- The `:type` refusal messages are unchanged — the wording is byte for byte what it was before this phase (`described`, `describe`), and `edge_the_type_refusal_keeps_the_wording_it_has_always_had` pins all three word for word. The printing commands are new, and they carry their own wording ("shown").
- A table cell spells out only the characters a terminal acts on (the C0/C1 controls, the escape byte, the bidi and zero-width marks), so a quote, an apostrophe, a backslash and a combining mark are drawn as the value holds them. The command is new, so nothing that existed before this phase prints differently.
- The three printers paint a refusal through `paint_diagnostic`, the same path `:type` uses, so a line that does not parse is drawn the same way whichever command asked about it. The refusal *words* are unchanged.
- Additive REPL command surface only: `.pretty`/`.json`/`.table` (and the `:` spellings), listed in `:help`.

## Known gaps / follow-ups

- **The delivery commit of this phase is acknowledged by subject, not by SHA.** The pipeline creates that commit *after* the run that wrote the work and re-creates it on every round (round 1's `ba0a0db` became `a31d067`), so a SHA written into the acknowledgement list is stale the moment the next round lands — that is exactly what the previous attempt's `ba0a0db` entry did, and the history audit (`tests/unreviewed_work_ack_test.rs`) reported it as stale while reporting the re-created commit as unreviewed. The audit's own rule exists for this case, so the rule now carries a prefix per phase in flight (`DELIVERY_SUBJECT_PREFIXES`) and no SHA is pinned. The cost: the rule is a list, and a phase that comes along later needs its prefix added.
- The hand-written terminal-width table in `src/error.rs` is incomplete for some emoji (⭐, ✨, the regional indicators), so a `.table` cell holding one is padded one cell short. → FINDINGS.md §1
- A `.table` cell holding a nested list or record is written on one line, with the single-line `Display`. → FINDINGS.md §2
- The session's own answer to a program is still the single-line `Display` (`src/repl/mod.rs:1370`); the three commands are opt-in. → FINDINGS.md §3
- The session's own answer, the language's `say` and the language's `json.stringify` follow a value as deep as it goes, and a value that deep is a fatal stack overflow rather than a catchable error. The three commands have a budget; these do not. → FINDINGS.md §4
- The language's `json.stringify` still writes `null` for a function, a builtin and an object, where the REPL's `.json` refuses them by name. → FINDINGS.md §5
- A table cell spells out the characters a terminal acts on and leaves the rest as they are, so a value holding a backslash and an `n` prints the same way as a value holding a line break. Escaping the backslash too would make the spelling unambiguous and would put a character the value does not hold into every cell that holds one.
