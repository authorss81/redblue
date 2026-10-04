# Phase 005 — Source spans on every error

## Reproduction (before)

`src/error.rs` at the pre-phase commit (`4ec2dd9`) carried no positions at all:

```
$ git show 4ec2dd9:src/error.rs | sed -n '4,10p'
pub enum Error {
    Lexer(String),
    Parser(String),
    Analyzer(String),
    Runtime(String),
    Io(String),
}
```

Consequence, reproduced with `rb run` on a two-line program whose second line is
malformed:

```
$ printf 'set x to 1\nsay x +\n' > /tmp/bad.rb     # scratch copy, not in the repo
$ rb run bad.rb
Error: ParserError: Unexpected token Newline
```

No file, no line, no column, no caret — the message names a token kind and
nothing locates it. The lexer already carried `line`/`column` on `Token`
(`src/lexer.rs:97`), so the information existed and was thrown away at the
`Error` boundary.

## What changed

| File | Lines | What |
|---|---|---|
| src/error.rs | +127 −10 | New public `Span { line, column }` (1-based, character-counted). `Lexer`/`Parser`/`Analyzer`/`Runtime` now carry `(String, Span)`; `Io` stays spanless. Added `Error::span()`, `Error::render()` (message + `--> file:line:col` + echoed source line + caret). `Display` prints the location. The panic-shaped `expect` in `render` was replaced with a plain match. |
| src/parser.rs | +117 −39 | New `Stmt { span, statement }` wrapper; `parse_statement` records the position of the statement's first token (`src/parser.rs:288`, `src/parser.rs:407`). `Parser::span()` (`src/parser.rs:231`) falls back to the last token, then `Span::unknown()` only when the file has no tokens. All 18 parser error sites pass a span. |
| src/vm.rs | +186 −73 | `Vm` tracks `current_span` (`src/vm.rs:17`); `execute_statement` saves/restores it around each statement (`src/vm.rs:120`) so every runtime failure is reported against the statement that raised it. All 54 runtime error sites pass `self.span()`. `parse_json*` take a `Span`. No control-flow change. |
| src/analyzer.rs | +40 −33 | `errors` is `Vec<(String, Span)>`; `add_error` takes a span; the first error's span is reported (`src/analyzer.rs:33`). Multi-line analyzer messages are preserved verbatim, joined as before. |
| src/lexer.rs | +10 −5 | `Token::span()` (`src/lexer.rs:107`); the one lexer error site now carries a `Span` instead of embedding "at line {}, column {}" in the message string. No token was added or removed. |
| src/lib.rs | +18 −7 | `run_file_with_diagnostic` renders the caret diagnostic for `rb run` / bare `rb file.rb`; `Span` re-exported as `redblue::Span`. |
| src/formatter.rs | +3 −3 | Signature plumbing only (`Statement` → `Stmt`). No behaviour change. |
| src/linter.rs | +3 −3 | Signature plumbing only. No behaviour change. |
| tests/span_test.rs | +472 | 21 new tests (new file). |

`Error::Parser` and friends are two-argument tuple variants, so it is a
*compile error* to construct one without a span — the "no error path constructs
a variant without a span" box is enforced by the type, not by convention.

## Tests added

21 new `#[test]` functions in `tests/span_test.rs`; 14 of them are `edge_*`.
Every one asserts an exact `Span`, an exact rendered line, or an exact error
kind — none is a smoke test.

| Test | Edge class covered |
|---|---|
| `test_parser_error_reports_line_and_column` | parser, exact `2:8` |
| `edge_error_on_first_line_points_at_line_one` | error on line 1 |
| `edge_error_on_last_line_points_at_the_last_line` | error on last line |
| `edge_empty_source_is_a_valid_empty_program` | empty (empty file is a valid program; stray token still located at 1:1) |
| `edge_multibyte_column_offsets_are_character_based` | unicode (4-byte emoji ⇒ char column 10, byte offset 12) |
| `test_mixed_width_unicode_columns_are_counted_in_characters` | unicode (emoji + CJK + combining mark ⇒ char column 14, not byte 23) |
| `test_lexer_error_carries_position` | lexer error position |
| `edge_runtime_error_reports_the_statement_position` | runtime, blank line skipped |
| `edge_runtime_error_inside_a_loop_reports_the_inner_statement` | nesting |
| `edge_nested_block_error_points_at_the_innermost_statement` | nesting (`for`/`if` ⇒ innermost statement, 3:9) |
| `edge_nested_analyzer_error_points_at_the_offending_name` | nesting, analyzer |
| `test_analyzer_error_carries_position` | analyzer, name is quoted in the message |
| `test_render_draws_source_line_and_caret` | diagnostic shape (4 exact lines) |
| `edge_render_without_a_file_omits_the_file_name` | REPL / in-memory path |
| `edge_render_of_a_span_past_the_last_line_still_reports_the_location` | boundary: EOF span is `lines + 1`, location kept, no caret |
| `edge_crlf_source_renders_a_clean_caret_line` | malformed input (CRLF; no stray `\r`) |
| `edge_four_digit_line_number_widens_the_caret_gutter` | boundary: 4-digit gutter |
| `edge_column_past_the_end_of_the_line_does_not_panic` | boundary: column 40 on a 5-char line |
| `test_io_error_has_no_span` | failure asserted (`IoError`, `span() == None`) |
| `edge_unknown_span_renders_message_only` | boundary: `Span::unknown()` renders message only |
| `test_every_failed_program_reports_a_position` | 18 failing programs, one per error path; asserts a known span, `line >= 1`, `line <= lines + 1`, `column >= 1` |

**Test strength was verified by mutation, not asserted.** Two deliberate
breakages, each reverted immediately afterwards:

1. `src/error.rs`: `" ".repeat(span.column.saturating_sub(1))` → `" ".repeat(span.column)`
   ⇒ `test result: FAILED. 15 passed; 6 failed` (the six caret assertions).
2. `src/vm.rs`: `execute_statement` sets `Span::unknown()` instead of `stmt.span`
   ⇒ the runtime/nesting span tests fail (`got: None`).

## Edge-case matrix

| Row | Status |
|---|---|
| empty | covered — `edge_empty_source_is_a_valid_empty_program`; empty file parses and runs, and an `Io` failure has no span |
| singleton | covered — single-line programs in `edge_error_on_first_line_points_at_line_one`; a one-token file (`end`) is located at 1:1 |
| boundary | covered — `edge_render_of_a_span_past_the_last_line_still_reports_the_location`, `edge_four_digit_line_number_widens_the_caret_gutter`, `edge_column_past_the_end_of_the_line_does_not_panic`, `edge_unknown_span_renders_message_only` |
| out_of_bounds | **N/A** — the only out-of-bounds path (`xs[99]`, `src/vm.rs:411`) returns `Nothing` instead of raising, so there is no error to carry a span. Recorded in `FINDINGS.md` as a VM defect for a later phase. |
| type_mismatch | covered — `1 + "a"` ⇒ `RuntimeError: Cannot add non-numbers` at the statement span, in `test_every_failed_program_reports_a_position`; `{1: 2}` ⇒ `ParserError: Expected field name` at 1:6 |
| numeric_boundary | **N/A** — no numeric value is transformed by this phase; `Division by zero` (`1 / 0`) is covered as a runtime-span case, but `0/0`, `±Infinity`, `NaN` and `2^53±1` are all pre-existing arithmetic concerns with no span involvement |
| unicode | covered — `edge_multibyte_column_offsets_are_character_based`, `test_mixed_width_unicode_columns_are_counted_in_characters` (character columns, not byte columns) |
| nesting_recursion | covered — `edge_nested_block_error_points_at_the_innermost_statement`, `edge_runtime_error_inside_a_loop_reports_the_inner_statement`, `edge_nested_analyzer_error_points_at_the_offending_name`. Deep *call* nesting could not be exercised: see `FINDINGS.md` — user functions never execute their bodies, so no error can be raised inside one |
| duplicate_missing_keys | covered at the parser level — `say {1: 2}` ⇒ `Expected field name` at 1:6. Duplicate *runtime* keys collapse silently (`{a: 1, a: 2}` → `{a: 2}`) with no error to locate; see `FINDINGS.md` |
| malformed_input | covered — CRLF (`edge_crlf_source_renders_a_clean_caret_line`), unterminated `if`/`for`/`to` (EOF spans in `test_every_failed_program_reports_a_position`), stray `end`, unknown character, empty file |
| resource_limit | **N/A** — this phase adds no recursion, loop or allocation. Span tracking is one `Span` copy per statement with an O(1) save/restore (`src/vm.rs:120`). Deep-stack spans are blocked by the same defect as deep call nesting (see `FINDINGS.md`) |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — 0 warnings, 0 errors |
| `cargo test --all-targets` | pass — 52 passed, 0 failed, 0 ignored (`src/lib.rs` 5, `src/main.rs` 0, `expect_test` 21, `redblue_test` 5, `span_test` 21); `cargo test --doc` 0 passed, 0 failed |
| `./rbops/verify.sh phase-005` | **NOT RUN — the script does not exist in this checkout** (`bash: ./rbops/verify.sh: No such file or directory`, exit 127). There is no `rbops/` directory in the working tree at all. Substitutes I ran by hand: `examples/*.rb` (6/6 exit 0), `tests/*.rb` (3/3 exit 0), `modules/MathUtils.rb` (exit 1 — pre-existing, see below), `rb test` (21 passed, 0 failed, 1 pre-existing `.skip` in `tests/suite.rb`), `rb lint`, `rb format --check`. |

## Invariants touched

- None. `.rb` extension, `to … end` / `if … end` / `for … end`, `set x to <expr>`, `say`, the `Value` variant list and the `Error::{Lexer,Parser,Analyzer,Runtime,Io}` variant *names* are unchanged.
- `Error` is a **public API break in shape only**: the variants gained a second `Span` field. The variant names, the `Result` alias and `impl std::error::Error` are unchanged, and `redblue::Span` is exported alongside `redblue::Value`. Any downstream `match` that binds `Error::Parser(msg)` must become `Error::Parser(msg, _span)`. Nothing in this repository does.
- `Display` output changed: errors now carry a `--> line:column` line. This is the phase's purpose.
- No existing test was weakened, deleted or re-scoped. Zero new `#[ignore]`, `// skip` or `allow(clippy::…)`.

## Known gaps / follow-ups

- **Gate 4 was never executed.** `rbops/verify.sh` is absent from this checkout, so I cannot claim it passed. Everything it would normally cover I ran by hand: `examples/*.rb` (6/6 exit 0), `tests/*.rb` (3/3 exit 0), `modules/MathUtils.rb` (exit 1 — pre-existing, see below), `rb test` (21 passed, 0 failed, 1 pre-existing `.skip` in `tests/suite.rb`), `rb lint`, `rb format --check`. Note that exit-code-only checking is not sufficient: `examples/hello.rb` exits 0 while printing 2 lines instead of 3, because its `greet("World")` call at `examples/hello.rb:13` executes nothing (see `FINDINGS.md` F1).
- **`modules/MathUtils.rb` does not load.** `modules/MathUtils.rb:4` uses `constant PI to 3.14159`, which `docs/GRAMMAR.md:179` and `SPEC.md:191` both specify, but no `constant` token exists in `src/lexer.rs` — the file has always failed with `ParserError: Expected function name`. Pre-existing; the checkpoint's lexer diff only *added* `Token::span()` and removed nothing. → `FINDINGS.md`.
- **User-defined functions never execute their bodies.** `src/vm.rs:1006` returns `Value::Nothing` for any name that resolves to a `Value::Function`. `to add(a, b) … end` + `say add(2, 3)` prints `nothing`, not `5`. This already breaks `examples/hello.rb`, which exits 0 while printing 2 lines instead of 3. Pre-existing (the vm diff contains no control-flow change to `call`), and it is the most serious defect found — it also caps the nesting/recursion coverage above. → `FINDINGS.md` F1.
- **`xs[99]` returns `Nothing` instead of a clean runtime error**, so the out-of-bounds row of the edge matrix cannot be covered by a span. → `FINDINGS.md`.
- **Unterminated string literals are silently accepted.** `src/lexer.rs:319` calls `read_text()`, which runs past the newline, so `set x to "abc` swallows the rest of the file into one string value and reports nothing. → `FINDINGS.md`.
- The `linter` and `formatter` still report positions as plain prose; wiring `Span` into their diagnostics was left out to keep this diff on-topic.