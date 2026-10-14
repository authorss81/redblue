# Phase 085 — Decode `\uXXXX` in a string literal instead of silently dropping the backslash

## Reproduction, before any change

```
$ printf 'say "\\u00e9"\n' > target/tmp_u.rb && ./target/debug/rb run target/tmp_u.rb | od -c
0000000   u   0   0   e   9  \n
$ printf 'say "A\\u0042C"\n' > target/tmp_b.rb && ./target/debug/rb run target/tmp_b.rb | od -c
0000000   A   u   0   0   4   2   C  \n
```

Exit 0 both times. The finding in the phase prompt reproduces verbatim on
`main`: a `\u` escape silently rewrote itself into the letter `u` followed by
the four hex digits as ordinary text.

## What changed

| File | Lines | What |
|---|---|---|
| `src/lexer.rs` | +45 −1 | `read_text` records the backslash position and routes `\u` to a new `read_unicode_escape`; the `Some(c) => text.push(c)` catch-all is replaced by a refusal. New `read_unicode_escape` reads exactly four hex digits and refuses anything else, including a surrogate half. |
| `tests/unicode_escape_test.rs` | +254 | new — 23 tests, all on both engines |
| `SPEC.md` | +40 | new § "Escapes in a text literal": the six escapes, the exact-four-digits rule, and the five refusals |
| `docs/GRAMMAR.md` | +6 | after line 81: the escape set is closed — a short run is not padded, a fifth digit is text, a surrogate half is refused |
| `tests/doc_blocks_runs.txt` | shifted | +`SPEC.md:132` (the new runnable example) and every later `SPEC.md:`/`docs/GRAMMAR.md:` key shifted to its new opening line |
| `tests/doc_blocks_fragments.txt` | shifted | +`SPEC.md:146` (the refusal catalogue, `excerpt`) and the same key shifts |
| `tests/doc_blocks_known_fail.txt` | shifted | key shifts only, no entry added, removed or re-worded |

### The fix

`src/lexer.rs:317-333` — the `Some(c) => text.push(c)` arm was the defect: it
pushed the one character after the backslash and dropped the backslash. It is
now two arms:

- `Some('u') => text.push(self.read_unicode_escape(escape)?)`
- `Some(other) => Err(Error::Lexer(format!("Unknown escape '\\{other}' in text"), escape))`

`read_unicode_escape` (`src/lexer.rs:342-374`) accumulates exactly four hex
digits with `char::to_digit(16)` and finishes with `char::from_u32`, so:

- fewer than four digits, or a non-hex digit → `Unicode escape needs four hex digits`
- four digits naming a surrogate half → `Unicode escape \uD83D names no character of its own`, because `char::from_u32` returns `None` and a lone surrogate has no UTF-8 encoding. Substitution of U+FFFD would be the same silent rewrite, one level down.
- a fifth digit is not part of the escape and falls through as ordinary text

Every refusal carries `Span::new(self.line, self.column)` taken **before**
`self.advance()` consumes the backslash, so the caret sits under the `\` — which
is what a writer has to find in a file with a thousand strings in it.

`\n`, `\t`, `\r`, `\"` and `\\` are untouched; `None => unterminated(span)` is
untouched, so a bare trailing `\` and an unterminated string keep the message
they always had.

## Tests added

All in `tests/unicode_escape_test.rs`. `printed_text` runs a program on the
tree-walking VM **and** the bytecode VM, compares the two against each other and
against a Rust `&str` literal — the silent `u00e9` survived a 1,414-test suite
because both engines read the same token, so agreeing with each other proves
nothing on its own.

| Test | Edge class covered |
|---|---|
| `unicode_escape_decodes_to_the_character_it_names` | the phase's headline: `say "\u0041"` is `A` |
| `unicode_escape_decodes_adjacent_to_ordinary_text` | `say "A\u0042C"` is `ABC` — the exact command in the finding |
| `unicode_escape_survives_interpolation` | `\u0041{name}` — the lexer and the `{…}` scanner must not eat each other |
| `existing_escapes_keep_their_behaviour` | `\n` `\t` `\r` `\"` `\\`, one assertion each |
| `edge_an_empty_literal_is_still_empty` | empty |
| `edge_the_highest_four_digit_escape_is_the_last_basic_plane_character` | boundary: U+FFFF, the largest four hex digits name |
| `edge_five_hex_digits_are_an_escape_then_ordinary_text` | boundary: `\u1f600` is `\u1f60` then `0`; `\u{1f600}` is refused |
| `edge_two_byte_escape_is_one_code_point_not_two_bytes` | unicode: U+00E9 asserted as `as u32 == 0xE9`, `chars().count() == 1`, `len() == 2` |
| `edge_upper_and_lowercase_hex_digits_decode_identically` | unicode: `\u00E9` == `\u00e9`, `\uABCD` == `\uabcd` |
| `edge_escape_immediately_before_the_closing_quote_is_decoded` | boundary: `"\u0041"` — the digit run and the quote must not merge |
| `edge_a_run_of_escapes_decodes_left_to_right` | `\u0041\u0042\u0043` is `ABC`, not `A` `B` `C` out of order |
| `edge_a_literal_may_mix_escapes_and_raw_unicode` | unicode: `"\u0041é中"` is `Aé中` — the decode must not consume raw text |
| `edge_an_escaped_quote_after_an_escape_does_not_end_the_literal` | `"\u0041\"b"` is `A"b` — a decoded escape must not leave the lexer thinking a quote is open |
| `edge_a_bare_backslash_u_is_refused` | malformed: `"\u"` |
| `edge_non_hex_digits_after_backslash_u_are_refused` | malformed: `"\uZZZZ"` |
| `edge_a_short_escape_is_refused_not_padded` | malformed: `"\u0"`, `"\u00"`, `"\u00e"` — explicitly *not* U+000E and not U+00E0 |
| `edge_a_surrogate_half_is_refused` | malformed: `"\ud83d"` |
| `edge_an_unknown_escape_is_refused` | malformed: `"\q"` — the same silent rewrite the `\u` case was |
| `edge_an_escape_at_the_end_of_input_is_refused` | malformed: `say "\u00e9` with no closing quote |
| `edge_a_bare_trailing_backslash_is_refused` | malformed: `say "abc\` at end of input |
| `edge_an_unterminated_string_is_refused` | malformed |
| `edge_a_refused_escape_is_refused_on_the_bytecode_path_too` | asserts a **failure** from `compile_source`, so `rb compile` cannot skip the lexer's refusal |
| `edge_the_refusal_names_the_line_it_is_on` | asserts a **failure** at `--> 3:` for an escape on line 3 |

Counts: 23 `#[test]` functions (floor is 3), 20 of them named `edge_*` (floor is
1), 13 of them asserting a refusal is produced (floor is 1). Zero `#[ignore]`,
zero skips, zero `allow(clippy::` suppressions, zero pre-existing tests touched.

`assert_refused_at` is the shared failure assertion: it requires an
`Error::Lexer(..)` (not a `Runtime` error), requires the message to name the
problem, and requires the rendering to carry a line and column.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, no warnings |
| `cargo test --all-targets` | 1580 passed, 0 failed |
| `cargo test` (adds doc-tests) | 1582 passed, 0 failed |
| `./rbops/verify.sh phase-085` | **not run — see below** |

`rbops/` is not present in this checkout; the pipeline that dispatched the phase
lives in another repository and this run was told not to inspect it. The other
three gates of AGENTS.md § 3.4 were run here and are green. What `verify.sh`
would add beyond them, and what I ran by hand instead:

- all 7 `examples/*.rb` exit 0
- all `modules/*.rb` exit 0
- the 99 `corpus/*.rb` programs that exit non-zero are the corpus's deliberate
  faults (`faults-*`, `malformed-*`, out-of-bounds, call-depth); each still
  produces the message it is recorded as producing, which
  `tests/common/corpus.rs` and `tests/differential_test.rs` check and which pass
- `tests/doc_blocks_test.rs` (7 passed) executes the new `SPEC.md:132` example
  through `rb` and it exits 0

End-to-end through the shipped binary, which is the phase's first definition-of-
done line:

```
$ printf 'say "\\u0041"\nsay "caf\\u00e9"\n' > target/tmp_esc.rb
$ ./target/debug/rb run target/tmp_esc.rb
A
café
$ ./target/debug/rb compile target/tmp_esc.rb && ./target/debug/rb vm target/tmp_esc.rbc | od -c
0000000   A  \n   c   a   f 303 251  \n
```

`303 251` is `0xC3 0xA9` — the UTF-8 of U+00E9, the character `é`.

## Invariants touched

- None. No `Value` variant, `Error` variant, file extension, keyword, or
  statement form changed. `say`, `set x to`, `to … end`, trailing commas and
  `{interp}` are all as they were. The only new `Error` variant is none — a
  bad escape is an `Error::Lexer`, the variant that already existed for it.

## Edge-case matrix (phase checklist)

- **empty** — covered: `edge_an_empty_literal_is_still_empty`; the decode loop
  is never entered for `""`, and an empty literal is still the empty string.
- **singleton** — covered: `edge_two_byte_escape_is_one_code_point_not_two_bytes`
  (exactly one character out of a four-digit escape) and
  `edge_escape_immediately_before_the_closing_quote_is_decoded` (exactly one
  escape, nothing after it).
- **boundary** — covered: `edge_the_highest_four_digit_escape_is_the_last_basic_plane_character`
  (U+FFFF, the largest four digits name), `edge_a_short_escape_is_refused_not_padded`
  (one digit short), `edge_five_hex_digits_are_an_escape_then_ordinary_text`
  (one digit long).
- **out_of_bounds** — N/A: an escape has no index. The nearest analogue is
  "past the end of the run", which is `edge_five_hex_digits_are_an_escape_then_ordinary_text`.
- **type_mismatch** — N/A: this change is entirely in the lexer, below the point
  where Redblue has a type system. There is no expression to mistype; the
  equivalents are a non-hex digit (`\uZZZZ`) and a half surrogate (`\ud83d`),
  both covered.
- **numeric_boundary** — covered for the one numeric thing here: `0/0`, `±Inf`
  and `2^53` cannot be spelled by this escape at all (it is not arithmetic, it
  is four hex digits into a `u32`), so they are N/A; what is covered is the
  decode arithmetic's own boundary — U+FFFF is the largest value four digits
  name, and overflow of the `value` accumulator is impossible because exactly
  four digits are accumulated into a `u32` that cannot exceed 0xFFFF.
- **unicode** — covered, and it is the substance of the phase: `é` by code
  point and byte length, U+FFFF, case-insensitive hex, escapes mixed with raw
  `é`/`中`, and a lone surrogate refused.
- **nesting_recursion** — N/A: `read_text` is a flat loop with no recursion and
  no nesting; an escape is decoded in one pass over four characters. The nearest
  thing is an escape adjacent to `{interp}`, which is
  `unicode_escape_survives_interpolation`.
- **duplicate_missing_keys** — N/A: a text literal has no keys. No record is
  built or read here.
- **malformed_input** — covered, six tests: bare `\u`, non-hex digits, one and
  two and three digits, surrogate half, unknown escape `\q`, escape at end of
  input, unterminated string after an escape, bare trailing `\`.
- **resource_limit** — N/A: the decode consumes at most four characters per
  escape and allocates at most 4 bytes (`char`); it cannot recurse, loop without
  bound, or grow a string. A program with a million escapes is bounded by its
  own source length, as every other literal is.

## Known gaps / follow-ups

- A code point above U+FFFF cannot be written with this escape: four hex digits
  stop at U+FFFF, so `😀` has to be written as the character itself.
  `edge_five_hex_digits_are_an_escape_then_ordinary_text` pins the behaviour
  rather than leaving it unspecified, and SPEC.md and GRAMMAR.md now say so. A
  `\u{1F600}`-style braced form would be the way to widen this, but that is a
  grammar change and belongs in its own phase — recorded in `FINDINGS.md`.
- `rb format` does not *produce* `\u` escapes: `src/formatter.rs:984`
  `escape_text` writes `\\`, `\"`, `\n`, `\r`, `\t` and every other character
  verbatim. A non-ASCII character therefore round-trips through the formatter
  as itself, which the lexer reads literally — so there is no drift to fix
  here, and none was introduced.
- `src/formatter.rs:60` `collect_comments` tracks strings with a `\` + next
  character rule. That is still correct for `\u`: the `\` escapes the `u`, the
  four digits are ordinary characters inside the literal, and none of them is a
  quote or a `/`. Left alone.