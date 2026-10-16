# Phase 059 — Diagnostics point at the code: source context at the prompt, carets on the name

## Reproduce (before the change)

At `d514651` (the parent of this work):

```
$ printf 'say nosuchvar\n' | ./target/debug/rb
>>> Error: AnalyzerError: Unknown variable 'nosuchvar'
  --> 1:1

$ printf 'say nosuchvar\n' > probe.rb && ./target/debug/rb run probe.rb
Error: AnalyzerError: Unknown variable 'nosuchvar'
  --> probe.rb:1:1
1 | say nosuchvar
  | ^
```

Two lines at the prompt, four from a file, for the same program. `execute_code_from`
printed `format!("Error: {}", e)` — `Display`, which is label + message +
`--> line:column` and stops — while `Error::render()` (`src/error.rs:200`) also draws
the offending source line and a caret under the column. And the caret it drew was
under the wrong thing: `nosuchvar` starts at column 5 and the caret was at column 1,
because the analyzer reported the statement that read the name rather than the name.
Confirmed on `main`, so the finding was not stale.

After (every transcript below re-run on this tree):

```
$ printf 'say nosuchvar\n' | ./target/debug/rb
>>> Error: AnalyzerError: Unknown variable 'nosuchvar'
  --> <session>:1:5
1 | say nosuchvar
  |     ^

$ ./target/debug/rb run target/tmp/probe.rb
Error: AnalyzerError: Unknown variable 'nosuchvar'
  --> target/tmp/probe.rb:1:5
1 | say nosuchvar
  |     ^
```

With `REDBLUE_COLOR=always`, the label line, the `-->` line and the caret line are
painted `Paint::Error`; `1 | say nosuchvar` is left exactly as it was typed.

## What changed

`must_touch: ["src/"]` is satisfied: nine files under `src/`, **+897 −61**, out of
+1317 −85 over the whole phase (22 files).

| File | Lines | What |
|---|---|---|
| `src/repl/mod.rs` | +98 −13 (production) | `render_error` replaces the `Display` print: `Error::render(code, Some(file))` over the session's buffer, coloured by `paint_diagnostic`. `paint_diagnostic` paints the label line, the `-->` line and the caret line in `Paint::Error` and leaves the source line plain; `is_caret_line` (`src/repl/mod.rs:455`) tells a caret gutter (blank, and a `^`/`~` past it) from a source gutter (a line number). `failure_line` (:470) gives `.type`/`.time`/`.bench` the same `Error: ` prefix, renderer and colour contract, so a command does not answer a misspelling with a worse picture of it than typing the line would. |
| `src/repl/mod.rs` | +551 (tests) | `mod error_rendering` — 12 tests. |
| `src/error.rs` | +178 −19 | The renderer draws in **terminal cells**, not characters. `char_cells` (:296) with `ZERO_WIDTH`/`WIDE` tables, `display_width` (:323) and `drawn_line` (:332); `render_single` (:354) and `render_range` (:378) measure the caret and the underline in cells, clamped to what is drawn. Two limits on the drawing: `MAX_DRAWN_CELLS` = 1024 (:10) and `MAX_DRAWN_LINES` = 10 (:17), each marked with `ELLIPSIS` (:20), so a megabyte-long line or a runaway string cannot make a diagnostic megabytes long. |
| `src/parser.rs` | +37 −15 | `Expr::Variable(String, Span)` (:20) — a name carries where it was written. Interpolation keeps its own offsets: `read_interpolated` is handed the position of the first character inside the braces (:2742) and re-spans each token onto `span.column + token.column - 1` (:2807), so `{a + b}` puts `a` and `b` under the characters the reader sees. |
| `src/analyzer.rs` | +24 −5 | `name_position` (:243) — `Unknown variable 'x'` is reported where `x` was written, falling back to the statement's span when the parser recorded none, so a failure is never left without a position. Two call sites: a bare name read (:575) and a method receiver that is not a module (:609). |
| `src/bytecode/codegen.rs`, `src/formatter.rs`, `src/interpreter.rs`, `src/linter.rs`, `src/repl/introspect.rs` | +9 −9 | The new arity of `Expr::Variable`, and nothing else. |
| `corpus/malformed-0031.expected`, `corpus/objects-0003.expected`, `corpus/objects-0009.expected`, `corpus/objects-0014.expected` | +4 −4 | The four golden positions of unknown-variable diagnostics, which moved to the name. |
| `tests/span_test.rs` | +113 −5 | 3 new tests. |
| `tests/repl_test.rs` | +16 | `a_failure_at_the_prompt_shows_the_line_that_failed` — the same thing end to end, against the `rb` binary. |
| `tests/diagnostics_test.rs` | +12 −7 | `edge_a_multibyte_line_is_underlined_by_display_cell_and_not_by_byte` (renamed): the underline of an emoji is two cells per emoji, so `^~~~~~~~` became `^~~~~~~~~~`. |
| `tests/formatter_test.rs`, `tests/interpolation_test.rs` | +6 −6 | The new arity, plus the interpolation column assertion moving from the statement's 5 to the name's 16. |
| `tests/tooling_grammar_test.rs` | +3 −2 | The LSP diagnostic's pinned column for `say missing`: 2:1 → 2:5. This test was **failing** on the tree before this round — see below. |
| `tests/unreviewed_work_ack_test.rs` | +1 | Acknowledgement entry for the resume commit `8389c5a`, so `unreviewed_work_in_head_history_is_acknowledged` has nothing unacknowledged to report. Data, not prose: the entry names the SHA, what it carries and why it was deliberately not rewritten. |

The REPL part is small: two lines at the print site, three functions to render and
paint a diagnostic, and one more so a `.type`/`.time`/`.bench` failure gets the same
treatment — the REPL calls the renderer the file runner already calls, so there is no
second format to drift. The rest of the change set is what it took for that rendering
to be worth printing: the caret lands on the name, on the right cell, and the drawing
is bounded.

`file` is whatever the caller already had: `<session>` for a line typed at the prompt
(so a failure reads `<session>:1:5`, matching what `.which`/`.type` already print for
the session) and the path for `:load` (so a failure in a loaded file names the file
you can open). The location line therefore gains a file prefix it did not have before;
`1:1` was not a place a reader could act on.

## Tests added

| Test | Edge class covered |
|---|---|
| `error_rendering::a_failure_a_line_produces_is_drawn_with_that_line_under_it` | happy path + boundary — `say nosuchvar` draws `1 | say nosuchvar` and a caret under the name (1:5); `set x to 1 +` proves the caret is not pinned to column 1 (1:13); a runtime type failure is labelled `RuntimeError` and quoted the same way |
| `error_rendering::a_failure_covering_several_lines_is_drawn_across_them` | malformed input / nesting — an unterminated string spanning two buffer lines draws both and the `~~~` underline from the column it opened at |
| `error_rendering::a_session_failure_says_the_same_place_run_does` | agreement — runs the program both ways (`Repl::run_code` and `crate::run_file_with_diagnostic`, the exact path `rb run` takes), diffs the `line:column`, and asserts the two renderings are byte-identical once `<session>` is replaced by the path |
| `error_rendering::the_bytecode_engine_reports_the_same_failure_in_the_same_place` | agreement across engines — the same program on the tree-walker and on `BytecodeVm`; the whole rendering is equal, not just the position |
| `error_rendering::edge_a_failure_inside_a_block_points_into_the_block` | multiline block — the caret points at line 2 of the buffer, under the name on it, not at line 1 and not at a history line |
| `error_rendering::edge_a_failure_with_no_position_renders_the_message_alone` | empty / failure — `Error::Io` has no span: message and label only, no `-->` and no gutter, no panic |
| `error_rendering::edge_a_position_beyond_the_buffer_renders_the_location_and_stops` | out of bounds — a span at 99:3 in a 2-line buffer draws the location and stops at 2 lines instead of indexing past the end |
| `error_rendering::edge_a_failure_on_an_empty_line_is_drawn_as_an_empty_line` | empty — an empty buffer line is drawn as `1 | ` with the caret still on its column |
| `error_rendering::edge_a_wide_line_is_drawn_whole_and_the_caret_stays_under_its_column` | boundary — a 500-character line is drawn whole (asserted character for character) with the caret where the failure says |
| `error_rendering::edge_a_line_with_wide_characters_is_drawn_intact` | unicode — emoji, accented and CJK text in the line survives the round trip and the caret does not drift |
| `error_rendering::colour_paints_the_diagnostic_and_leaves_the_source_line_plain` | colour contract — with colour off the REPL rendering **is** `Error::render` byte for byte; the painted and plain renderings agree once escapes are stripped; label, location and caret are painted and the source line is not |
| `error_rendering::colour_leaves_the_renderers_own_truncation_notice_unpainted` | colour contract, negative — a range longer than `MAX_DRAWN_LINES` leaves `… N more line(s) not drawn`, which has a caret gutter but is the renderer's own note: it is not painted, while the underline of the range it does draw still is |
| `span_test::edge_a_tab_before_the_column_is_a_cell_and_not_a_gap` | unicode and escapes — a tab is a control character and `char_cells` answered 0 for it, stepping the caret over every tab on the line; both shapes are pinned, the caret of a single-line diagnostic and the opening of a range underline |
| `span_test::edge_an_unknown_variable_is_reported_under_the_name_it_names` | position, four shapes — at the head of a line (`5`), mid-expression (`9`), inside an interpolation (`12`) and in a condition (`4`), each asserting both the span and where the drawn caret lands |
| `span_test::edge_a_line_longer_than_the_drawing_limit_is_cut_and_the_caret_lands_on_the_cut` | resource — a failure at column 1108 in a line past `MAX_DRAWN_CELLS` draws 1024 cells and `…`, and the caret is clamped to the cut |
| `tests/repl_test.rs::a_failure_at_the_prompt_shows_the_line_that_failed` | end-to-end — `rb` itself prints the line and the caret for a typed `say nosuchvar` |
| `tests/tooling_grammar_test.rs::diagnostics_surface_a_spanned_analyzer_error` | agreement with the editor — `rb diagnostics` reports the unknown variable at the name, not at the statement |

16 new `#[test]` functions, 10 of them named `edge_*`, plus
`edge_a_multibyte_line_is_underlined_by_character_and_not_by_byte` renamed to say what
it now pins. Nine of the twelve `error_rendering` tests need a program that fails and
get it from `failure_of`, which `expect_err`s; the three that build an `Error` by hand
are pinning the *drawing* of a span (no position, past the end of the buffer, an empty
line) rather than a program that produces one. No test was weakened,
skipped, ignored or suppressed: `git diff d514651` over this phase contains no
`#[ignore]`, `// skip` or `allow(clippy::`.

Every colour assertion strips escapes or compares against `Style::forced(...)`, so
changing the palette cannot fail this suite. The `caret` helper asks the production
`is_caret_line` rather than repeating its predicate, so a test cannot pass on a
reading of the drawing that production does not have.

## Edge-case matrix

| Row | Covered by |
|---|---|
| empty / zero / nothing | `edge_a_failure_with_no_position_renders_the_message_alone` (`Io` failure, empty buffer, nothing invented), `edge_a_failure_on_an_empty_line_is_drawn_as_an_empty_line` (`1 | ` is drawn as an empty line, not dropped) |
| singleton and boundary | `a_failure_a_line_produces_is_drawn_with_that_line_under_it` — a buffer of exactly one line is the base case; its second half is the other end, a caret at column 13 |
| out of bounds | `edge_a_position_beyond_the_buffer_renders_the_location_and_stops` — span line 99 in a 2-line buffer; the renderer stops at the location and draws no line that is not there. The other direction — a column past the end of the *line* — is `span_test::edge_a_line_longer_than_the_drawing_limit_is_cut_and_the_caret_lands_on_the_cut` |
| type mismatch | `a_failure_a_line_produces_is_drawn_with_that_line_under_it`, third program: `say 1 + "a"` renders as `RuntimeError: Cannot add non-numbers` with its line quoted. The renderer is kind-agnostic — it draws whatever label and message the failure carries — and `the_bytecode_engine_…` asserts the `RuntimeError` label and message of an out-of-bounds index |
| numeric boundary | N/A for arithmetic — nothing numeric was added; a number reaches the renderer only as text inside a message or a source line, and is passed through byte for byte (`the_bytecode_engine_…` asserts `Index 9 is out of bounds` survives; `edge_a_wide_line_…` asserts the whole line is copied unchanged). `0/0`, `1/0`, `-0.0`, `NaN`, `±Infinity` and `2^53±1` raise no new branch here: their rendering is the rendering of any other `RuntimeError`. `edge_a_line_longer_than_the_drawing_limit_…` does pin a numeric: 1108 columns, and the caret clamped to the 1024 cells drawn |
| unicode and escapes | `edge_a_line_with_wide_characters_is_drawn_intact` — emoji, `ünïcode`, CJK survive, caret unmoved; `edge_multibyte_column_offsets_are_character_based` and `diagnostics_test::edge_a_multibyte_line_is_underlined_by_display_cell_and_not_by_byte` pin the other half, that a cell and a character are counted differently and the drawing follows the cell; `edge_a_tab_before_the_column_is_a_cell_and_not_a_gap` pins a tab at one cell. Non-UTF-8 bytes are N/A: a REPL line arrives as `String` from the line reader and a file is read with `read_to_string`, so a source that is not valid UTF-8 is rejected before it can be rendered |
| nesting and recursion | `edge_a_failure_inside_a_block_points_into_the_block` — a block is the only nesting a REPL buffer has, and its line numbering is asserted rather than assumed. Deeper recursion is N/A: the renderer walks a flat list of source lines and never recurses, so it cannot blow a stack at any depth. A name inside `{}` is the other kind of nesting, and `edge_an_unknown_variable_is_reported_under_the_name_it_names` pins its column at 12 |
| duplicate and missing keys | N/A — no record is inspected; the source line is copied verbatim, which `edge_a_wide_line_…` asserts character for character, so a repeated key in the drawn line is drawn exactly as written. A missing field is an analyzer message like any other and takes the same path as `say nosuchvar` |
| malformed input | `a_failure_covering_several_lines_is_drawn_across_them` (unterminated string over two lines), the `set x to 1 +` case (parser error mid-line), and the empty-buffer case above |
| resource and state | Two limits, both asserted: `MAX_DRAWN_CELLS` by `span_test::edge_a_line_longer_than_the_drawing_limit_…` (1024 cells and `…`, caret clamped to the cut) and `MAX_DRAWN_LINES` by `colour_leaves_the_renderers_own_truncation_notice_unpainted` (10 lines and `… N more line(s) not drawn`). Beyond those, `render` allocates one `Vec<&str>` of borrowed lines over the buffer, which is already bounded by `MAX_BLOCK_LINES` / `MAX_BLOCK_BYTES` (`exceeds_block_limits`), and a drawn line is one `String` built to the cell limit. No file is touched, no VM state changes, and a failed line leaves the session running (`repl_reports_an_uncaught_error_and_stays_alive` covers the survival; `a_failure_at_the_prompt_shows_the_line_that_failed` runs a session to `:quit`) |

## Gates

Run in this checkout, in this order, on the tree as it now stands:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 1748 passed, 0 failed, 62 test binaries (log: `target/tmp/phase059-test.log`) |
| `cargo test --doc` | 2 passed, 0 failed |
| `./rbops/verify.sh phase-059` | **could not run — `rbops/` is not in this checkout** |

`rbops/` and `rbops/verify.sh` are not present in this working tree
(`ls rbops/verify.sh` → *No such file or directory*), exactly as phases 057 and 058
recorded. I did not create, inspect or work around them. The four cargo gates above
were run here; `./rbops/verify.sh` will have to run where it exists.
`examples/*.rb` (7 files) were each run through `rb run` by hand and all exit 0.

## What the round-two review changed

The first version of this report described the REPL change alone and said of the rest
of the tree: *"`render` itself is untouched"*, *"analyzer untouched"*, and — in the
section headed "One place the definition of done does not match the language" — that
the caret under `nosuchvar` is at **column 1** because the analyzer reports the
statement, with the change to report the name deferred to `FINDINGS.md` F1. None of
that was true of the tree the report shipped with: `src/error.rs`, `src/parser.rs` and
`src/analyzer.rs` were all changed, the diagnostic said `1:5`, and F1 was done here.
So this report now describes the whole change set, and F1 is gone from `FINDINGS.md`
because there is no longer a follow-up to record.

Four defects were fixed with it:

1. **`diagnostics_surface_a_spanned_analyzer_error` was failing** — `tests/tooling_grammar_test.rs`
   pinned the LSP diagnostic for `say missing` at 2:1, which the analyzer no longer
   produces. The first report claimed `cargo test --all-targets` was 1744 passed, 0
   failed; on this tree it was not. The pin is updated to 2:5, which is the position
   the editor should underline.
2. **A tab was measured as zero cells** (`src/error.rs:296`) — `char_cells` returns 0
   for anything `is_control()`, and a tab is one, so the caret stepped left over
   every tab before the column it points at while the function's own comment promised
   one. Fixed by answering 1 for `'\t'` ahead of the control check, and pinned by
   `span_test::edge_a_tab_before_the_column_is_a_cell_and_not_a_gap`.
3. **The truncation notice was painted as a failure** (`src/repl/mod.rs:455`) —
   `is_caret_line` called any blank gutter a caret, and `… N more line(s) not drawn`
   has a blank gutter, so the renderer cutting a diagnostic short was reported in
   `Paint::Error` as if it were part of the failure. Fixed by requiring a `^` or `~`
   past the gutter, and pinned by `colour_leaves_the_renderers_own_truncation_notice_unpainted`.
   The test helper `caret` had repeated the same predicate, so the suite could not
   have caught it; it now calls the production function.
4. **A new limit had no test** — `MAX_DRAWN_CELLS` was asserted by nothing. Pinned by
   `span_test::edge_a_line_longer_than_the_drawing_limit_is_cut_and_the_caret_lands_on_the_cut`.

Findings 1 and 4 of the review are the same defect read twice: the report did not
describe the tree. Findings 2 and 3 were MAJOR rather than BLOCKER, and are fixed
anyway because they are defects in this phase's own code.

## Invariants touched

- `Expr::Variable` gained a `Span` (`src/parser.rs:20`). That is an AST variant the
  LSP, `wasm.rs`, the formatter and the linter all pattern-match, and every match in
  the tree was updated — but no *meaning* of a program changed: no `Value` variant, no
  `Error` variant, no keyword, no `set x to <expr>` / `say` / `to … end` form, and a
  program that ran before runs the same and says the same. What changed is what an
  analyzer *reports*, which is the point of the phase.
- **Positions moved, deliberately.** An unknown variable is now reported at the name
  rather than at the statement that read it, so `1:5` replaces `1:1` in `rb run`, in
  the REPL and in `rb diagnostics`, and the four corpus goldens moved with it
  (`say Tagged.tag` 4:1 → 4:5, `give back value` 4:14 → 4:19). That is a visible change
  to `rb run`'s output for one class of failure; every other failure position is
  untouched — `set x to 1 +` is still 1:13, and `say 1 + nosuchvar` moves from 1:1 to
  1:9 because the statement span was never the name's column.
- `Error::render` **is** changed: carets and underlines are measured in terminal
  cells rather than characters, and the drawing is bounded. Its callers — `rb run`
  (`src/lib.rs:84`), `compile_source` (`src/lib.rs:507`), `wasm.rs:208` and
  `bootstrap.rs:274` — print through the same function, so all four get the cell
  measurement and the two new limits at once. With ASCII and no long lines the bytes
  are identical, which is why the corpus goldens moved only where a name moved.
- The REPL's error text changes shape (source line + caret) but the *message* is
  unchanged: `colour_paints_the_diagnostic_and_leaves_the_source_line_plain` asserts
  the rendering equals `Error::render` byte for byte when colour is off.
- The `Error: ` prefix and the destination (stdout) are unchanged.

## Known gaps / follow-ups

- The REPL's location line now names `<session>`, where it previously named nothing
  (`--> 1:1`). A transcript saved before this phase reads slightly differently, and
  the session history number is *not* what the line shows — the buffer line is. That
  is the same choice `.which`/`.type` already make, and it is asserted
  (`edge_a_failure_inside_a_block_points_into_the_block`).
- `WIDE` and `ZERO_WIDTH` are hand-written range tables rather than a dependency's
  Unicode data, so a character this table gets wrong is drawn at the wrong width.
  Nothing in the tree needed a range that is not in the tables; the alternative was a
  new dependency for two small functions, and a table that can be read in the source
  is the smaller thing to be wrong about.
- `MAX_DRAWN_LINES` and `MAX_DRAWN_CELLS` are not configurable and are not reported to
  the reader as numbers. The `…` marker says something was left out; how much is only
  spelled out for lines.
- The `.bytecode`/`.asm` listing (phase 058) renders compile errors through
  `Error::render` already, so it inherits the cell measurement and the limits without
  a change of its own.
- `rbops/verify.sh` still cannot run here — `FINDINGS.md` F2.