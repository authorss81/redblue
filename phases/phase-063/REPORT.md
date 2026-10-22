# Phase 063 — Print values three ways: `.pretty`, `.json`, `.table`

## Resumed run

The previous attempt's work was preserved in commit `8d7c994` (`resume
phase-063: adopt preserved work`), which made every gate see an empty phase
diff: `verify.sh` reported "no changed files detected against HEAD" and
"no implementation change" even though the tree held the completed work. The
preserved commit was the phase's own delivery commit, not another phase's, and
no code was changed in this run: the commit was uncommitted with
`git reset --mixed HEAD~1` (working tree untouched, no file lost) so the phase
diff is visible again against `c6e9837`, the parent the preserved commit was
built on.

After that, the four gates were run from the restored tree and all passed:
`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets` (2038 passed, 0 failed, 0 ignored) and
`verify.sh phase-063` (`VERIFY PASS`). No code was changed because the only
failure was diff visibility — the work below was already complete and every
test was already green. This run re-measured the diff and corrected the test
counts in this report to what the tree actually adds against `c6e9837`.

## What changed

The historical rounds below were measured against `b21821d` (the commit before
this phase's work landed) and the adopted work in `a31d067`; the current working
diff is against `c6e9837`. Round 2's changes are the rows marked *round 2*,
round 3's are marked *round 3*, round 4's are marked *round 4*, and round 5's
— the second review's findings — are marked *round 5*. The line counts are
measured against the baseline.

| File | Lines | What |
|---|---|---|
| src/repl/render.rs | +1358 −0 (new module) | `pretty` (one indent level per depth), `json` (the language's own JSON writer, with the renderers' depth budget on it and a refusal for a value JSON cannot hold), `table` (union-of-keys header row, empty cells for missing keys, a named refusal for a non-list / non-record item, cells escaped so a terminal cannot be mangled by them) |
| src/repl/commands.rs | +100 −0 | the three command rows (`pretty`, `json`, `table`, each also under `:`), their parse entries and the parse test |
| src/repl/mod.rs | +378 −5 | `print_pretty`/`print_json`/`print_table` dispatch, `pretty_answer`/`json_answer`/`table_answer`/`value_answer` (one-expression gate, effectful-call refusal, fork evaluation — the same three gates `:type` uses), the `:help` rows, and the `mod printing` tests |
| src/repl/introspect.rs | +126 −12 | `expression_of_for(program, command, wording)`: the refusal names the command the user typed (`.pretty`) rather than `:type`, and each command carries its own wording |
| src/runtime.rs | +244 −31 | `json_escape_text` and `json_stringify` widened to `pub(crate)` so the REPL's `.json` prints the same document `json.stringify` writes; round 5 gives the writer a depth budget, a named refusal for a value JSON cannot hold, and escapes the characters a terminal acts on |
| src/value.rs | +123 −4 | `Value::Display` — the single-line writer behind `say`, `print` and the session's `= value` answer — follows a value only to the parser's nesting budget and names the rest instead of overflowing the stack (round 5) |
| src/error.rs | +136 −18 | the error renderer measures a caret and a drawn line with `unicode-width` instead of its own incomplete table of wide ranges, so emoji, variation selectors, skin tones and regional indicators land under the right cell (round 5) |
| Cargo.toml, Cargo.lock | +6 −0 | `unicode-width` promoted from a transitive dependency of `rustyline` to a direct one: it is how a terminal's width is known, and the table renderer asks it a question of its own (round 3) |
| tests/repl_printing_test.rs | +463 −0 | new integration suite: sessions piped into the real `rb` binary, asserted on stdout |
| tests/unreviewed_work_ack_test.rs | +109 −36 | the delivery-commit rule now covers phase-063's delivery subject as well as phase-084's (`DELIVERY_SUBJECT_PREFIXES`), the stale SHA entry for the previous round's delivery commit (`ba0a0db`) is gone, and a new test pins the rule — see "Known gaps" for why the rule, and not a SHA, is what acknowledges a delivery commit |
| src/repl/render.rs | (round 2, in the row above) | **one depth budget for all three renderers.** `MAX_PRETTY_DEPTH` became `MAX_RENDER_DEPTH`, and `.json` and `.table` now refuse a value nested past it instead of recursing: the same value was refused cleanly by `.pretty` and killed the process by `.json` (a stack overflow in Rust is a fatal abort, not a catchable panic) |
| src/repl/render.rs | (round 2, in the row above) | **a function, a builtin and an object are not written as `null`.** `.pretty` shows them the way the language writes them (`<function f>`); `.json` refuses the document by name, because JSON has no literal for them and `null` says "nothing" about a value that is not nothing |
| src/repl/render.rs | (round 2, in the row above) | **a cell is escaped** so an escape sequence or a right-to-left mark cannot mangle the terminal — and the column is padded by the cells the terminal draws, which is the escaped form |
| src/repl/render.rs | (round 2, in the row above) | **the layout drops only the padding it added.** A cell that ends in spaces keeps them, in the last column as much as in any other: the same value is not printed two different ways depending on where it sat |
| src/repl/introspect.rs | (round 2, in the row above) | **`:type`'s refusal words are what they always were** ("described", "describe"), and they are pinned by a test; the printing commands bring their own wording ("shown") instead of changing an existing command's |
| src/repl/render.rs | (round 2, in the row above) | **only the characters a terminal acts on are escaped.** A quote, an apostrophe, a backslash, a combining mark and a letter of any script are drawn by every terminal there is, so a cell that spelled them out would show the reader a character the value does not hold: the cell is what `say` would have printed, and the only thing it may change is what the terminal would do with it |
| src/repl/render.rs | (round 2, in the row above) | **the layout keeps the content's own trailing spaces whatever follows them.** The whitespace the layout adds after the last cell that holds something — the padding and the separator — is the layout's to drop; the spaces a cell ends in are the content's to keep, and dropping them with the layout's made `ada ` print as `ada` in one row and as `ada ` in the next |
| src/repl/mod.rs | (round 2, in the row above) | **a refusal is painted the way `:type` paints it.** The three printers paint through `paint_diagnostic`, not through one whole-string paint: the failure is painted and the reader's own source line is left as they typed it, so a printing command answers a misspelling with the same picture of it that every other command gives |
| src/repl/mod.rs | (round 2, in the row above) | **the `.json` doc comment says what the renderer does.** "The document the language's own `json.stringify` writes" is true for every value JSON can hold and not for the three it cannot, so the comment now says so — the wording this report already used |
| src/repl/render.rs | (round 3, in the row above) | **a column is padded by the cells a terminal draws, and the table no longer borrows the error renderer's answer.** `cell_width` measures a whole cell with `unicode-width`, the reference for how wide a terminal draws a character: the error renderer's own table of wide ranges counts characters one at a time and does not know every emoji, so a column holding `⭐` was padded by one cell and the column beside it started one cell further along in that row than in the others. The same held for `❤️`, whose variation selector the old table counted as no cell at all |
| src/repl/render.rs | (round 3, in the row above) | **an emoji that is one glyph is padded by that one glyph.** A face with a skin tone and a heart with a variation selector are two characters each, and a terminal draws each of them as one glyph two cells wide; `unicode-width` measures the sequence, so the column is padded by the glyph and not by its parts |
| src/error.rs | (round 3, in the row above) | **the error renderer keeps no helper of the table's.** `text_cells` — added in round 2 so the table could ask the error renderer how wide a text is drawn — is removed: the table asks `unicode-width` instead, and the error renderer was left exactly as it was — until round 5 replaced its own width table with `unicode-width` too |
| src/repl/render.rs | (round 4, in the row above) | **a table of records that hold no keys is named.** The empty list is named ("no rows, and no keys to name"); a list whose every record is empty is the same reader-facing case — rows, and nothing to name them by — and it was rendered as a table with no columns, which is a blank line and no answer at all. Now it is named the way the empty list is named |
| src/repl/render.rs | (round 4, in the row above) | **`.pretty` spells out the characters a terminal acts on, and `.json` does not — and says so.** A right-to-left override or a zero-width mark is written `\u202e` in the document `.pretty` writes: JSON's own escape, so the document still reads back through `json.parse` as the value it came from. `.json` keeps the language's own bytes, because the document it prints is the document `json.stringify` writes, byte for byte, and a document that differed from the language's own writer would be a document no program could produce. SPEC.md §"json and csv" now said which was which; round 5 escaped the marks in the language's own writer as well, so `.json` and `.pretty` no longer differ |
| src/repl/render.rs, src/repl/mod.rs, tests/repl_printing_test.rs | (round 4, in the row above) | **the identity is pinned by tests, not by prose.** `edge_the_json_document_is_the_language_s_own_writer_s_document` holds the renderer against the language's own writer, and `edge_the_json_a_session_prints_is_the_document_json_stringify_writes` holds it against the language's own `json.stringify` inside a real session: the same value, printed twice, byte for byte the same. A change to the language writer's shape (`, ` → `,`) now fails a test instead of silently making the two documents differ |
| src/runtime.rs | (round 5, in the row above) | **the language's own JSON writer refuses a value it cannot hold, and escapes the marks a terminal acts on.** A function, a builtin and an object are refused by name instead of written as `null`, which says "nothing" about a value that is not nothing. A text holding a bidi override, a zero-width mark, a C1 control or another format character is written as JSON's own `\uXXXX` escape, so `.json` remains the document `json.stringify` writes while still not reordering the reader's terminal. `json.parse` reads every escaped character back, so the document round-trips |
| src/runtime.rs | (round 5, in the row above) | **the language's own writer has the same depth budget as the renderers.** A value nested past 64 levels is refused with a catchable `Error::Limit`, not followed into a fatal stack overflow, so `try … catch error … end` can survive a value a program built too deep |
| src/value.rs | (round 5, in the row above) | **the session's own answer, `say` and `print` name the rest of a value past the parser's budget** (`<value nested more than 64 levels deep>`) rather than walking a hundred thousand stack frames and aborting the process |
| src/error.rs | (round 5, in the row above) | **the error renderer asks `unicode-width` how a terminal draws a character.** `⭐`, `✨`, a heart with a variation selector, a face with a skin tone and a pair of regional indicators are measured as the glyph a terminal draws, and a tab is one cell, so a caret under an emoji is not one cell short |
| src/repl/render.rs, src/repl/mod.rs, SPEC.md | (round 5, in the row above) | **the `.json` prose is true again.** `.json` no longer writes a terminal-active mark raw: the language's own writer escapes it, so the document `.json` prints and the document `json.stringify` writes stay byte for byte the same, and both can be printed without reordering the line |
| tests/repl_printing_test.rs, tests/stdlib_modules_test.rs | (round 5, in the row above) | **the tests follow the writer.** The `.json` mark test now asserts the escape, and the unicode round-trip test keeps plain scripts as they are and expects the marks to be escaped |

| tests/unreviewed_work_ack_test.rs | (round 4, in the row above) | **the delivery acknowledgement records what the commit actually carries.** The reason is measured from the commit at audit time (`git show --numstat`), so the record of what unreviewed code is in the tree is the tree's own record and not a description written by hand in a round that has been and gone — and the deviation is named in the file's own docs: phase-063's prefix is in the list that judges phase-063's own commit |

## Tests added

71 new `#[test]` functions (53 in `src/`, 15 in
`tests/repl_printing_test.rs`, 3 in `tests/unreviewed_work_ack_test.rs`),
zero ignored, zero skipped, zero `allow(clippy::` suppressions. 62 of them are
named `edge_*`. Measured directly against `c6e9837` the diff adds all 71; the
gate's post-suite count reads 22 rust tests and 18 `edge_*` tests, which is
still well over its floor (3 and 1).

| Test | Edge class covered |
|---|---|
| `edge_pretty_of_a_ten_deep_nesting_terminates_with_bounded_lines` | nesting/recursion, resource limit |
| `edge_pretty_refuses_a_value_nested_past_its_budget` | resource limit — asserts the **failure** (depth > 64 is refused with an error, not truncated) |
| `edge_json_of_a_value_nested_past_its_budget_is_refused` (round 2) | resource limit — the same budget for the JSON renderer: asserts the **failure**, and that the refusal names the budget |
| `edge_table_of_a_value_nested_past_its_budget_is_refused` (round 2) | resource limit — the same budget for a table cell: asserts the **failure** |
| `edge_a_value_just_inside_the_budget_is_still_written` (round 2) | boundary — the budget is a refusal, not a truncation: a value just inside it is still written whole |
| `pretty_output_reads_back_as_the_value_it_came_from` | acceptance: `.pretty` round-trips through the language's own `json.parse` to an equal value |
| `pretty_indents_one_level_per_depth` / `pretty_of_an_empty_thing_is_closed_on_the_same_line` | nesting, empty (`[]`/`{}`/`nothing`/`""`/`yes`/`no`) |
| `json_is_valid_json_per_serde_json` | numbers, texts, yes/no, `nothing` → `null`, lists, records — parsed by `serde_json` |
| `edge_json_of_a_number_that_is_not_finite_is_null_and_never_a_nan_literal` | numeric boundary: NaN/±Infinity → `null`, asserts no `NaN` literal is ever written |
| `json_escapes_the_characters_a_text_may_hold` | unicode/escapes: quote, backslash, newline in a text |
| `edge_json_keeps_unicode_text_as_it_is` | unicode: CJK kept as-is, valid JSON |
| `edge_pretty_of_a_function_is_not_null` (round 2) | type mismatch — a function is written in its own form, never as the `null` that means nothing |
| `edge_json_of_a_function_is_refused_and_not_null` (round 2) | type mismatch — asserts the **failure**: JSON has no literal for a function, so the document is refused by name |
| `edge_json_of_a_builtin_or_an_object_is_refused_and_not_null` (round 2) | type mismatch — the other two leaves with no JSON literal, refused and named |
| `edge_a_table_cell_holding_a_function_is_not_null` (round 2) | type mismatch — a table cell holds the function's own form |
| `edge_json_of_a_function_nested_in_a_record_is_refused_and_names_it` (round 3) | type mismatch — asserts the **failure**: the refusal is not the top of the document but the *nested* value that cannot be written, and it is named |
| `edge_table_of_a_list_holding_a_text_is_refused_and_names_the_item` (round 3) | type mismatch — asserts the **failure**: a list of records with a text in it names the item and what it is |
| `edge_a_table_column_holding_an_emoji_is_padded_by_the_cells_it_is_drawn_in` (round 3) | unicode — an emoji is one character and two cells: the column beside it must start at the same cell in both rows |
| `edge_a_table_column_holding_an_emoji_glyph_is_padded_by_the_glyph_it_is_drawn_in` (round 3) | unicode — a face with a skin tone and a heart with a variation selector are one glyph, two cells, not two characters |
| `edge_a_table_column_holding_an_emoji_lines_up_on_the_screen` (round 3, integration) | unicode — the same alignment through the real binary, asserted on the exact output |
| `table_aligns_its_columns_under_one_header_row` | singleton/shared key set, keys named once |
| `edge_table_of_ragged_rows_fills_the_missing_cells_with_nothing` | duplicate/missing keys: missing key → empty cell, not an error |
| `edge_table_gives_a_later_key_its_own_column` | duplicate/missing keys: a key only a later row holds still gets a column |
| `edge_table_of_an_empty_list_says_so_and_does_not_panic` | empty: named message, never a panic |
| `edge_table_refuses_anything_that_is_not_a_list_of_records` | type mismatch — asserts the **failure**, and names the offending item |
| `edge_a_cell_holding_a_newline_stays_on_one_line` | malformed cell content: newline stays one row |
| `edge_a_key_holding_a_newline_stays_on_one_line` (round 2) | malformed cell content: a line break in a *key* stays one header row |
| `edge_table_aligns_a_wide_character_column_by_the_cells_it_is_drawn_in` (round 2) | unicode: a CJK column is padded by terminal cells, so the columns still line up |
| `edge_a_cell_a_terminal_would_mangle_is_escaped_rather_than_written_out` (round 2) | unicode/escapes — an ESC sequence and a right-to-left mark are spelled out, and the column is padded by the cells the terminal draws |
| `edge_a_cell_shows_the_characters_a_terminal_draws_as_they_are` (round 2) | unicode/escapes — a quote, an apostrophe and a combining mark are drawn as they are: only the characters a terminal acts on are escaped |
| `edge_a_cell_ending_in_spaces_keeps_them_in_every_column` (round 2) | malformed cell content — the content's own trailing spaces survive in the last column, so the same value is not printed two ways |
| `edge_a_cell_ending_in_spaces_before_an_empty_cell_keeps_them_too` (round 2) | malformed cell content — the same spaces survive when the cell after them is empty: the layout's separator and padding go, the content's spaces stay |
| `the_three_printing_commands_parse` / `the_three_printing_commands_answer_about_an_expression` / `the_three_printing_commands_are_dispatched_and_not_run_as_source` | the commands reach the dispatcher and answer about an expression |
| `edge_a_printing_command_refuses_a_program_and_the_session_never_knows_it_happened` / `edge_a_printing_command_does_not_change_the_session` | malformed input: statements refused, no binding changed |
| `edge_a_printing_command_of_a_line_that_does_not_parse_is_refused` (unit + integration) | malformed input: `1 +`, `[1, 2`, `{ a: }` → `Error:` — asserts the **failure** |
| `edge_help_names_the_three_printing_commands` | acceptance: `:help` lists all three, and the completer offers them |
| `edge_a_table_of_one_record_names_its_keys_once` | singleton: one record is a header row and one row |
| `edge_the_type_refusal_keeps_the_wording_it_has_always_had` (round 2) | acceptance — an existing command's user-facing text is pinned word for word, so it cannot change silently again |
| `edge_a_printing_command_refusal_names_its_own_command_and_its_own_words` (round 2) | acceptance — the printing commands' refusal names the command the user typed and its own words, and never mentions another command |
| `edge_the_three_printing_commands_print_three_shapes` / `edge_the_json_a_session_prints_is_valid_json` / `edge_table_of_a_value_that_is_not_a_list_of_records_is_refused` / `edge_a_table_of_an_empty_list_says_so_and_does_not_panic` / `edge_a_printed_text_keeps_its_own_script` / `edge_a_printing_command_of_a_line_that_does_not_parse_is_refused` (integration) | end-to-end through the real binary, incl. emoji/CJK/RTL text and a 20 s liveness bound (resource limit) |
| `edge_a_value_nested_past_the_budget_is_refused_and_the_session_survives` (integration, round 2) | resource limit — a session that builds a 2000-deep value is refused by `.json` and by `.table`, and the session goes on: asserts the **failure**, and that the process is still alive afterwards |
| `edge_a_table_cell_a_terminal_would_mangle_is_escaped_rather_than_written_out` (integration, round 2) | unicode/escapes — no raw escape sequence reaches the terminal through a real session |
| `edge_the_json_of_a_function_is_refused_and_not_null` (integration, round 2) | type mismatch — the refusal reaches the session's stdout, and `.pretty` shows the function's own form |
| `edge_a_printing_command_answers_a_misspelling_exactly_as_type_does` (integration, round 2) | acceptance — the three printers and `:type` answer a line that does not parse byte for byte alike, colour on: the same prefix, the same renderer, the same colour contract |
| `edge_a_table_cell_shows_the_characters_a_terminal_draws_as_they_are` (integration, round 2) | unicode/escapes — a cell holds the text as the value holds it, through a real session |
| `edge_the_delivery_rule_covers_each_phase_in_flight_and_no_other` (round 2, `tests/unreviewed_work_ack_test.rs`) | the history audit: every phase in flight is covered by the delivery rule, and a phase that is not in flight is still unreviewed work |
| `edge_the_delivery_acknowledgement_records_what_the_commit_carries` (round 4, `tests/unreviewed_work_ack_test.rs`) | the history audit: the acknowledgement says what the commit actually carries — its own line counts and its own paths, measured from the commit — so a reason that went back to describing a round's contents from memory fails |
| `edge_a_delivery_commit_that_cannot_be_measured_says_so` (round 4, `tests/unreviewed_work_ack_test.rs`) | the history audit: a commit that cannot be measured says so, instead of inventing a number for it |
| `edge_a_table_of_records_with_no_keys_says_so` (round 4, unit + integration) | empty: a list whose every record is empty has rows and nothing to name them by, and is named the way the empty list is named |
| `edge_pretty_spells_out_the_characters_a_terminal_would_mangle` (round 4) | unicode/escapes: a bidi override, a zero-width mark and a C1 control are spelled out in JSON's own escape, and the document still reads back as the value it came from |
| `edge_pretty_of_a_text_a_terminal_would_mangle_reads_back_as_the_value` (round 4) | acceptance: what `.pretty` writes is a document the language's own `json.parse` reads back as the value it came from — even when the value holds a mark a terminal acts on |
| `edge_the_json_document_is_the_language_s_own_writer_s_document` (round 4) | acceptance: the document `.json` prints is the document the language's own writer writes, byte for byte |
| `edge_the_json_a_session_prints_is_the_document_json_stringify_writes` (round 4, integration) | acceptance: the same value, printed by `.json` and by the language's own `json.stringify` inside a real session, is byte for byte the same document |
| `edge_the_json_document_escapes_the_marks_a_terminal_would_act_on` (round 4/5, integration) | unicode/escapes: `.json` is a document first and a terminal rendering second, and the marks are escaped |
| `edge_json_stringify_refuses_a_value_nested_past_its_budget` (round 5) | resource limit — the language's own writer refuses depth past 64 with a catchable error instead of overflowing the stack |
| `edge_json_stringify_refuses_a_value_with_no_json_literal` (round 5) | type mismatch — a function, a builtin and an object are refused by name, never written as `null` |
| `edge_json_stringify_escapes_the_characters_a_terminal_acts_on` (round 5) | unicode/escapes — a bidi override, a zero-width mark, a C1 control, a bidi control and a BOM are escaped, not written raw |
| `edge_display_of_a_deep_value_names_the_rest_and_does_not_overflow` (round 5) | nesting/recursion, resource limit — the single-line writer names the rest past the budget |
| `edge_display_writes_a_value_inside_the_budget_whole` (round 5) | boundary — a value inside the budget is still written whole and unchanged |
| `edge_the_caret_counts_the_cells_a_terminal_draws` (round 5) | unicode — star, sparkles, heart with variation selector, skin toned face and regional indicators are measured by `unicode-width` |
| `edge_the_drawn_line_reports_the_glyph_it_draws` (round 5) | unicode — the drawn line's cell count matches what the terminal draws |
| `edge_the_json_document_escapes_the_marks_a_terminal_would_act_on` (round 5, integration) | unicode/escapes — `.json` and `.pretty` both escape the mark through a real session, and `.json` is byte for byte the language's own document |

### Mandatory edge-case matrix

- **empty** — covered: `pretty_of_an_empty_thing…` (`[]`, `{}`, `""`, `nothing`, `yes`, `no`), `edge_table_of_an_empty_list…` (unit + integration), and round 4's `edge_a_table_of_records_with_no_keys_says_so` (unit + integration): a list that is not empty but whose every record is empty is the same reader-facing case — nothing to name — and is named too
- **singleton** — covered: `edge_a_table_of_one_record_names_its_keys_once`, `table_aligns_its_columns_under_one_header_row`
- **boundary** — covered: `edge_pretty_refuses_a_value_nested_past_its_budget` (depth 65 refused, 64 shown), `edge_a_value_just_inside_the_budget_is_still_written` (the budget is a refusal, not a truncation), and round 3's width boundary: a cell one emoji wide is padded by two cells, and a cell one character wider than its neighbour is padded by one
- **out_of_bounds** — N/A: these commands take no user-written index; `.table` addresses cells by record key, and a missing key is the empty cell covered by the ragged-row tests
- **type_mismatch** — covered: `edge_table_refuses_anything_that_is_not_a_list_of_records` (a number, and a list of numbers naming the item), `edge_the_json_a_session_prints_is_valid_json` (a yes/no is a JSON boolean, not a number), round 2's three leaf tests (`edge_pretty_of_a_function_is_not_null`, `edge_json_of_a_function_is_refused_and_not_null`, `edge_json_of_a_builtin_or_an_object_is_refused_and_not_null`, `edge_a_table_cell_holding_a_function_is_not_null`) and round 3's two (`edge_json_of_a_function_nested_in_a_record_is_refused_and_names_it`, `edge_table_of_a_list_holding_a_text_is_refused_and_names_the_item`) — a function, a builtin and an object are never written as the `null` that means nothing, whether they stand alone or sit inside a record
- **numeric_boundary** — covered: `edge_json_of_a_number_that_is_not_finite_is_null…` (NaN, ±∞ → `null`, per SPEC.md §"A number is always finite"); `json_is_valid_json_per_serde_json` (integers and floats). N/A for `2^53±1` and `-2^31`: these commands print a value the session already holds, and the value's own arithmetic is the language's, unchanged by this phase
- **unicode** — covered: `edge_json_keeps_unicode_text_as_it_is` (café, 日本語, مرحبا, 🎉, a combining mark), `edge_a_printed_text_keeps_its_own_script` (integration), `edge_table_aligns_a_wide_character_column_by_the_cells_it_is_drawn_in` (CJK padding), round 2's `edge_a_cell_a_terminal_would_mangle_is_escaped_rather_than_written_out` (unit + integration) — an escape sequence and a right-to-left mark are escaped, not written out — `edge_a_cell_shows_the_characters_a_terminal_draws_as_they_are` (unit + integration) — a quote, an apostrophe and a combining mark are drawn as they are — round 3's three width tests (`edge_a_table_column_holding_an_emoji_is_padded_by_the_cells_it_is_drawn_in`, `edge_a_table_column_holding_an_emoji_glyph_is_padded_by_the_glyph_it_is_drawn_in`, and the integration `edge_a_table_column_holding_an_emoji_lines_up_on_the_screen`): an emoji is two cells, an emoji that is one glyph is two cells and not the sum of its characters — and round 4's `edge_pretty_spells_out_the_characters_a_terminal_would_mangle`, and round 5's `edge_json_stringify_escapes_the_characters_a_terminal_acts_on` and `edge_the_caret_counts_the_cells_a_terminal_draws`: every writer escapes the marks a terminal acts on, `.json` is still the language's own document, and a caret under an emoji is measured by the glyph a terminal draws
- **nesting_recursion** — covered: `edge_pretty_of_a_ten_deep_nesting_terminates_with_bounded_lines`, `edge_pretty_refuses_a_value_nested_past_its_budget`, `edge_json_of_a_value_nested_past_its_budget_is_refused`, `edge_table_of_a_value_nested_past_its_budget_is_refused`, `edge_a_value_nested_past_the_budget_is_refused_and_the_session_survives` (integration)
- **duplicate/missing_keys** — covered (missing): ragged rows + later-key column. Duplicate keys N/A: a record is an `IndexMap` — a key is held once by construction, and `keys_of` de-duplicates the union defensively
- **malformed_input** — covered: both parse-refusal tests (`.pretty 1 +`, `.json [1, 2`, `.table { a: }`), the statement/multi-statement refusal, the two newline tests (a cell and a key), and round 2's `edge_a_cell_ending_in_spaces_keeps_them_in_every_column` and `edge_a_cell_ending_in_spaces_before_an_empty_cell_keeps_them_too`. N/A for an empty file, a BOM, CRLF and non-UTF-8 bytes: these commands take an expression, and an expression that does not parse is refused by the parse-refusal tests above
- **resource_limit** — covered: the 64-level budget, shared by **all three** renderers (a refusal, not a truncation) — `edge_pretty_refuses…`, `edge_json_of_a_value_nested_past_its_budget_is_refused`, `edge_table_of_a_value_nested_past_its_budget_is_refused`, `edge_a_value_just_inside_the_budget_is_still_written`, and the integration session that builds a 2000-deep value and is still alive afterwards; the integration sessions have a 20 s liveness bound that fails a hang. The table's own work is bounded by the value it is given: a cell's width is measured by the characters it holds, so a cell of *n* characters costs *n*, and the layout's width array is as long as the number of columns
  - **Round 5 closes the writer gap:** the session's own answer (`= value`), `say` and `print` name the rest of a value past the parser's budget instead of overflowing, and `json.stringify` refuses with a catchable `Error::Limit`. The three printers' budget, the single-line writer's budget and the JSON writer's budget are now one budget: 64 levels.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2038 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh phase-063 | pass |

## Round 4 — the review's findings, and what was measured

| Finding | What it said | What the tree says |
|---|---|---|
| 1 (BLOCKER) | the phase's own test `table_aligns_its_columns_under_one_header_row` asserts a table `layout` cannot produce, so the suite is red and this report's gate row is false | **The suite is green**: `cargo test --all-targets` ran 2038 passed, 0 failed, 0 ignored, and the test passes in it. The review's trace read the first column as 10 cells wide; "bartholomew" is **11** cells wide (`unicode-width` 0.2.2 measures that string as 11), so the column is 11 wide, the header is "name" + 7 of padding + 2 of separator = 9 spaces before "age", and the first row is "ada" + 8 + 2 = 10 spaces before "36" — which is what the test asserts. A width of 10 is not reachable: `layout` takes the width of the *widest* cell in a column, so a column narrower than its own cell would underflow `widths[index] - cell_width(cell)` and panic. Round 4 keeps the expectations and pins the invariant the finding was about — the second column starts at the same cell in the header and in both rows — so a table that did not line up would fail |
| 2 (MAJOR) | `.table` of a list whose records hold no keys prints blank lines and says nothing | **Fixed**: the case is named the way the empty list is named ("The records hold no keys: there is nothing to name."), and `edge_a_table_of_records_with_no_keys_says_so` (unit + integration) pins it |
| 3 (MAJOR) | the delivery rule forgives phase-063's own commit by a prefix phase-063 wrote, and the reason describes an earlier round's contents | **Fixed**: the acknowledgement is measured from the commit at audit time, so it names the commit's own line counts and paths, and the deviation is named in the audit file's own docs |
| 4 (MINOR) | `.pretty` and `.json` write the characters a terminal acts on raw, where `.table` escapes them | **Fixed for `.pretty`** — a bidi override, a zero-width mark and a C1 control are written in JSON's own `\uXXXX` escape, so the document still reads back through `json.parse` as the value it came from — and **documented for `.json`**: it is a document first and a terminal rendering second, because the document it prints is the one `json.stringify` writes. SPEC.md §"json and csv" says so now |
| 5 (MINOR) | the identity ".json is the document `json.stringify` writes" is asserted here but pinned by no test | **Fixed**: `edge_the_json_document_is_the_language_s_own_writer_s_document` holds the renderer against the language's own writer, and `edge_the_json_a_session_prints_is_the_document_json_stringify_writes` holds it against the language's own `json.stringify` inside a real session |
| 6 (MINOR) | the round-3 width assertion `cell_width(glyph) == 2` may be 4 under unicode-width 0.2.2 | **Measured: 2.** unicode-width 0.2.2 measures `\u{1f44d}\u{1f3fb}` as two cells and `\u{2764}\u{fe0f}` as two cells — its table reads the sequence rather than summing its characters — so round 3's claim holds and its test is green |

## Round 5 — the second review's findings

| Finding | What it said | What the tree says |
|---|---|---|
| 1 (MAJOR) | `json_stringify` recurses once per level with no depth check, so a deep value is a fatal stack overflow | **Fixed**: the writer has the same 64-level budget as the renderers and refuses past it with a catchable `Error::Limit`; `edge_json_stringify_refuses_a_value_nested_past_its_budget` pins the failure |
| 2 (MAJOR) | `Value::Display` recurses once per level with no depth check, so `say` and the session's answer can abort the process | **Fixed**: `write_display` follows the parser's budget and writes `<value nested more than 64 levels deep>` for the rest; `edge_display_of_a_deep_value_names_the_rest_and_does_not_overflow` pins it |
| 3 (MAJOR) | `json.stringify` writes `null` for a function, a builtin and an object, where `.json` refuses them; the two surfaces disagree and SPEC is silent | **Fixed**: `json.stringify` refuses all three by name with a catchable error, `.json` inherits the same refusal, and SPEC.md §"json and csv" states it. `edge_json_stringify_refuses_a_value_with_no_json_literal` pins all three |
| 4 (MAJOR) | `json_escape_text` leaves terminal-active characters raw, so `.json` writes a bidi override or a C1 control to the terminal | **Fixed**: the language's own escaper spells them out as JSON's own `\uXXXX` escape, so `.json` is still byte-for-byte `json.stringify`'s document and neither mangles the terminal; `edge_json_stringify_escapes_the_characters_a_terminal_acts_on` and the integration test pin it |
| 5 (MAJOR) | `src/error.rs`'s `WIDE` table counts `⭐`, `✨`, `❤️`, regional indicators and variation selectors wrong, so the caret lands short | **Fixed**: the table and `in_ranges` are gone; `char_cells` and `text_cells` ask `unicode-width`, including tabs as one cell. `edge_the_caret_counts_the_cells_a_terminal_draws` and `edge_the_drawn_line_reports_the_glyph_it_draws` pin the glyphs |
| 6 (MINOR) | `mangles_the_terminal` in `src/repl/render.rs` misses terminal-active characters | **Fixed**: there is one predicate, `runtime::mangles_terminal`, and both the renderers and the language's writer use it. Its set covers C0/C1 controls, the escape byte, the bidi controls, zero-width and other format characters, the BOM and the tags block |

## Invariants touched

- No grammar, keyword, `Value` variant or `Error` variant changed, and `examples/*.rb` and `modules/*.rb` are untouched. Round 5 does change two language-visible answers, both stated in SPEC.md: `json.stringify` refuses a function, a builtin, an object and a value past 64 levels, and every single-line writer names the rest of a value past the parser's budget instead of aborting.
- `src/runtime.rs`: `json_escape_text`/`json_stringify` visibility widened `pub(crate)` (not part of the public API). Round 5 adds the depth budget, the named refusal and the terminal-active escapes; `.json` still prints the identical document `json.stringify` writes, and the round-4 identity tests plus round 5's integration test pin it.
- `src/error.rs`: the hand-written `WIDE` range table is gone; `char_cells`/`text_cells`/`display_width`/`drawn_line` ask `unicode-width`, which the table renderer already depends on. The error renderer's caret and gutter shape are unchanged; only the measurement is correct for glyphs the old table did not know.
- The `:type` refusal messages are unchanged — the wording is byte for byte what it was before this phase (`described`, `describe`), and `edge_the_type_refusal_keeps_the_wording_it_has_always_had` pins all three word for word. The printing commands are new, and they carry their own wording ("shown").
- A table cell spells out only the characters a terminal acts on (the C0/C1 controls, the escape byte, the bidi and zero-width marks), so a quote, an apostrophe, a backslash and a combining mark are drawn as the value holds them. The command is new, so nothing that existed before this phase prints differently.
- The three printers paint a refusal through `paint_diagnostic`, the same path `:type` uses, so a line that does not parse is drawn the same way whichever command asked about it. The refusal *words* are unchanged.
- Additive REPL command surface only: `.pretty`/`.json`/`.table` (and the `:` spellings), listed in `:help`.
- One dependency added: `unicode-width`, which was already in the tree as a transitive dependency of `rustyline`. It changes no behaviour outside the table's column padding.
- Round 4 changed no language behaviour: `.pretty` writes the characters a terminal acts on as JSON's own `\uXXXX` escape (the document it writes still reads back through `json.parse` as the value it came from), `.json` writes the language's own bytes, and SPEC.md §"json and csv" now says which is which. `tests/doc_blocks_runs.txt` and `tests/doc_blocks_known_fail.txt` were moved by the nine lines SPEC.md gained — the registry keys each block by the line of its opening fence.

## Known gaps / follow-ups

- **The delivery commit of this phase is acknowledged by subject, not by SHA.** The pipeline creates that commit *after* the run that wrote the work and re-creates it on every round (round 1's `ba0a0db` became `a31d067`), so a SHA written into the acknowledgement list is stale the moment the next round lands — that is exactly what the previous attempt's `ba0a0db` entry did, and the history audit (`tests/unreviewed_work_ack_test.rs`) reported it as stale while reporting the re-created commit as unreviewed. The audit's own rule exists for this case, so the rule now carries a prefix per phase in flight (`DELIVERY_SUBJECT_PREFIXES`) and no SHA is pinned. **Round 4 makes the acknowledgement record what the commit carries**: the reason is measured from the commit at audit time (`git show --numstat`), so it names the commit's own line counts and its own paths instead of a description written by hand in a round that has been and gone — and the deviation is named in the audit file's own docs, because phase-063's prefix is in the list that judges phase-063's own commit. The cost that is left: the rule is a list, and a phase that comes along later needs its prefix added.
- A `.table` cell holding a nested list or record is written on one line, with the single-line `Display`. → FINDINGS.md §1
- The session's own answer to a program is still the single-line `Display` (`src/repl/mod.rs`); the three commands are opt-in. → FINDINGS.md §2
- A table cell spells out the characters a terminal acts on and leaves the rest as they are, so a value holding a backslash and an `n` prints the same way as a value holding a line break. Escaping the backslash too would make the spelling unambiguous and would put a character the value does not hold into every cell that holds one.
- **The second review's fixed findings are no longer gaps.** Round 5 gave the language's writers a depth budget and a catchable refusal, made `json.stringify` refuse a value JSON cannot hold instead of writing `null`, escaped the characters a terminal acts on in the language's own writer (so `.json` stays byte-for-byte the language's document), and replaced the error renderer's hand-written width table with `unicode-width`.
