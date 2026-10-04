# Phase 009 — Lexer robustness: malformed and non-ASCII input

## Reproduction

The finding reproduces on `main` (commit `e9bf84b`). Four of the six items in
the definition of done were already correct; two were real defects and two more
produced errors with unreadable messages.

```
$ printf 'say "hello\n' > target/tmp/unterm.rb
$ ./target/debug/rb run target/tmp/unterm.rb
hello
   exit=0                          <- DEFECT: no error at all

$ printf 'set x to 1\rsay "hi"\n' > target/tmp/lonecr.rb
$ ./target/debug/rb run target/tmp/lonecr.rb
hi
   exit=0                          <- DEFECT: the CR is dropped as whitespace,
                                      so no Newline token is emitted

$ printf '\xef\xbb\xbfsay "hi"\n' > target/tmp/bom.rb
$ ./target/debug/rb run target/tmp/bom.rb
Error: LexerError: Unexpected character '<invisible U+FEFF>'
  --> target/tmp/bom.rb:1:1       <- DEFECT: a BOM is a file artefact, not source

$ printf 'set x to 1\x00\n' > target/tmp/nul.rb
$ ./target/debug/rb run target/tmp/nul.rb
Error: LexerError: Unexpected character '<invisible NUL>'
  --> target/tmp/nul.rb:1:11      <- rejected with a column, but the message
                                      quotes a byte that prints as nothing

$ printf 'say "\xff\xfe hi"\n' > target/tmp/badutf8.rb
$ ./target/debug/rb run target/tmp/badutf8.rb
Error: IoError: stream did not contain valid UTF-8
                                     <- already correct (src/lib.rs:25)

$ printf 'say "\xf0\x9f\x98\x80 \xe4\xbd\xa0... e\xcc\x81"\n' | rb run -
<f0 9f 98 80 20 e4 bd a0 ... 65 cc 81>   <- already correct, byte-identical
```

After the change:

```
$ ./target/debug/rb run target/tmp/unterm.rb
Error: LexerError: Unterminated string
  --> target/tmp/unterm.rb:1:5
1 | say "hello
  |     ^

$ ./target/debug/rb run target/tmp/bom.rb
hi

$ ./target/debug/rb run target/tmp/nul.rb
Error: LexerError: Unexpected character '\0'
  --> target/tmp/nul.rb:1:11
```

## What changed

| File | Lines | What |
|---|---|---|
| src/lexer.rs | +29 −10 | `read_text` returns `Result` and reports `Unterminated string` at the opening quote (also for a dangling `\` at end of input) |
| src/lexer.rs | +8 −4 | `\r` ends a line; the `\n` of a CRLF pair is consumed with it, so CRLF is still one `Newline` token and the line counter does not inflate |
| src/lexer.rs | +4 −1 | `skip_whitespace` also skips `U+FEFF`, at the start of a file and mid-file |
| src/lexer.rs | +4 −1 | the unknown-character message uses `char::escape_debug`, so a NUL or BEL is readable in a terminal |

Nothing outside `src/lexer.rs` and the new test file was touched.

## Tests added

`tests/lexer_robustness_test.rs`, 16 new `#[test]` functions.

| Test | Edge class covered |
|---|---|
| `test_unterminated_string_is_a_spanned_lexer_error` | malformed input — asserts failure: `Error::Lexer` at 1:5 |
| `edge_unterminated_string_reports_the_line_the_quote_opened_on` | malformed input — the error names line 2, not line 4 |
| `edge_escape_at_end_of_input_inside_a_string_is_still_an_error` | malformed input — asserts failure, dangling `\` |
| `test_crlf_line_endings_produce_one_newline_per_line` | boundary — full token stream asserted, not just a count |
| `edge_lone_carriage_return_ends_a_line` | malformed input / boundary — lone CR emits a `Newline` |
| `edge_cr_does_not_inflate_the_line_counter` | boundary — error on line 2, the CR is not a line |
| `test_bom_is_stripped_and_not_lexed_as_an_identifier` | malformed input — BOM is invisible to the tokenizer |
| `edge_bom_before_each_line_is_stripped` | malformed input — BOM may not glue onto an identifier |
| `test_nul_byte_is_rejected_with_a_column` | malformed input — asserts failure at column 11 |
| `edge_control_character_message_is_readable` | malformed input — message contains `\0`, not a raw NUL byte |
| `edge_other_control_characters_are_rejected_with_a_column` | malformed input — BEL at 1:5, ESC at 1:11 |
| `edge_empty_source_lexes_to_only_eof` | empty — `""` is `[Eof]`; whitespace+comment is `[Newline, Newline, Eof]` |
| `test_unicode_round_trips_through_strings` | unicode — emoji, CJK, Hebrew RTL, combining acute, U+200F |
| `edge_column_after_multibyte_characters_counts_characters` | unicode / boundary — column 12 after four emoji, not a byte offset |
| `edge_invalid_utf8_file_is_an_io_error_not_a_panic` | malformed input — asserts failure: `Error::Io`, no panic, no span |
| `edge_bom_and_crlf_file_runs_from_disk` | malformed input — a Windows file with a BOM runs end to end from disk |

## Edge-case matrix

- empty — **covered**: `edge_empty_source_lexes_to_only_eof`; the empty string is
  one `Eof` token and an empty program still runs.
- singleton — **covered**: `test_crlf_line_endings_produce_one_newline_per_line`
  and `test_unicode_round_trips_through_strings` each lex exactly one statement /
  one string and assert the whole token stream.
- boundary — **covered**: CRLF vs lone CR vs LF (`test_crlf_...`,
  `edge_lone_carriage_return_ends_a_line`, `edge_cr_does_not_inflate_the_line_counter`);
  column after four 4-byte emoji (`edge_column_after_multibyte_characters...`).
- out_of_bounds — N/A + why: this phase adds no index, slice or collection
  access to the lexer; the lexer walks a `Vec<char>` behind `source.get()`, which
  returns `Option` and cannot index out of bounds. Index bounds are covered by
  `tests/index_bounds_test.rs` (phase 008).
- type_mismatch — N/A + why: the lexer produces no typed values and performs no
  operation that could receive a wrongly typed operand; token kinds are matched
  exhaustively. Interpreter-level type mismatches are covered by
  `tests/index_bounds_test.rs` and `tests/numeric_edge_test.rs`.
- numeric_boundary — N/A + why: no numeric arithmetic is added or changed. The
  number reader (`read_number`, already spanned) is untouched and is locked by
  `tests/numeric_edge_test.rs`.
- unicode — **covered**: `test_unicode_round_trips_through_strings` (emoji, CJK,
  RTL, combining mark, U+200F right-to-left mark) and
  `edge_column_after_multibyte_characters_counts_characters`.
- nesting_recursion — N/A + why: the lexer is iterative with no recursion and
  nests nothing; a string is read by one `loop`, and nothing in this change
  recurses. Recursion limits are phase 008's `MAX_CALL_DEPTH`.
- duplicate_missing_keys — N/A + why: the lexer has no records, keys or fields.
  Record key handling is `tests/record_order_test.rs`.
- malformed_input — **covered**: all six — unterminated string, dangling escape,
  lone CR, BOM, NUL/BEL/ESC, non-UTF-8 file bytes.
- resource_limit — **covered** for the phase's own input: the lexer is
  single-pass with no recursion and allocates one `Vec<char>` proportional to
  input (`Lexer::new`, src/lexer.rs:123), so a malformed input cannot make it
  loop or recurse. A whole-file upper bound and a maximum string length are
  **not** implemented — see follow-ups.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass (0 warnings) |
| `cargo test` | pass — 148 passed, 0 failed, 0 ignored (12 test binaries + doc-tests) |
| `./rbops/verify.sh phase-009` | **not run — `rbops/` is not present in this checkout** |

On the fourth gate: `rbops/` does not exist in the working directory
(`ls: cannot access 'rbops': No such file or directory`), and hard rule 1 forbids
creating or editing anything under `rbops/`. The three gates I can run are green
and the project CI (`.github/workflows/ci.yml`: `cargo build`, `cargo test`,
`cargo clippy -- -D warnings`) is the same set. The only additional thing that
gate is documented to do is run `examples/*.rb`, so I ran them myself:

```
$ for f in examples/*.rb; do ./target/debug/rb run "$f"; done
```

All six examples run clean (`hello`, `fizzbuzz`, `files`, `formats`, `time`,
`test_arithmetic`). `modules/MathUtils.rb` fails with
`ParserError: Expected function name` at `modules/MathUtils.rb:4:16` on
`constant PI to 3.14159` — I verified this failure is byte-identical before and
after this change by rebuilding at `git stash` (HEAD), so it is pre-existing and
unrelated. Recorded in `FINDINGS.md`.

## Invariants touched

- None of the language invariants in AGENTS.md section 2. `.rb`, `to … end`,
  `set x to <expr>`, `say`, the `Value` and `Error` variants, and the string
  syntax are all unchanged; no public type was renamed and `Error::Lexer(String,
  Span)` keeps its shape — it is now also *returned* where the lexer used to
  invent a value.
- One behaviour change beyond strict bug-fixing: a lone `\r` is now a line break
  instead of being dropped as whitespace, and `U+FEFF` is now insignificant
  instead of a lexer error. Both only affect byte sequences that previously
  produced a wrong token stream or a bogus error; every source file that lexed
  before still lexes to the same tokens (`cargo test` and all examples unchanged).

## Known gaps / follow-ups

- A string may still contain a raw newline and is only terminated by `"` or end
  of input. Making a raw newline terminate a string (and report a column on the
  line it opened) would be a better error, but it changes the string grammar and
  belongs in its own phase.
- No maximum source size and no maximum string length; the lexer is O(n) in
  memory and cannot recurse, so there is nothing to bound beyond input size.
- `Error::Lexer("Unterminated string", span)` carries no byte offset, so
  "unexpected end of input" cannot yet be pointed at precisely — the span is the
  opening quote by design.
- `modules/MathUtils.rb` does not parse (`constant` is not in the grammar).
- A combining mark inside an *identifier* ends the identifier, because
  `char::is_alphanumeric` is false for category `Mn`. Identifiers built from
  decomposed forms would need `is_alphanumeric() || is_mark()`. Not touched here.