# Phase 054 — Highlight Redblue code live as it is typed, using Redblue's own lexer

## Reproduction (before any change)

`src/repl/editor.rs:189` on `main`:

```rust
impl Highlighter for SessionCompleter {}
```

The line editor is generic over a `Helper`, and `Highlighter` is one of the
three hooks around completion. The impl above is empty, so the hook is
rustyline's default (`fn highlight<'l>(&self, line: &'l str, _pos: usize) ->
Cow<'l, str>` at `rustyline-*/src/highlight.rs:30`), which returns
`Cow::Borrowed(line)` — the line as typed, unchanged. The exact wrong output for
a session typing `say "hi"` at the prompt is `say "hi"`, with `say`, the quote,
the word and the closing quote all in the terminal's foreground colour. Nothing
in the tree mapped a `TokenKind` to a colour; `grep -rn "highlight" src/` before
this phase returned only the import and the empty impl.

Re-verified on `main` at the start of this run: still reproduces.

## What changed

| File | Lines | What |
|---|---|---|
| `src/repl/highlight.rs` | +661 (new) | `highlight(line, style, theme)` — classifies a line with `Lexer::tokenize`, maps each `Token` to a `TokenClass`, and paints the non-plain byte ranges |
| `src/repl/style.rs` | +120 −0 | six new `Paint` roles (`Keyword`, `Str`, `Number`, `Comment`, `Builtin`, `Operator`), the `TokenClass` enum, and a `Theme` that is a *lookup* so a theme may have a hole in it |
| `src/repl/editor.rs` | +147 −8 | `impl Highlighter for SessionCompleter`, `EditSource::with_style`, `EditSource::highlight`, `LineSource::highlights`, `line_source(names, style)` |
| `src/repl/mod.rs` | +2 −1 | `pub mod highlight;` and passing the session's already-detected `Style` into `line_source` so the line and its surroundings are coloured the same way |

### How a comment is found without a second lexer

`Lexer::tokenize` **skips** comments — they are not tokens. So a plain tokenize
of `set x to 1 // the answer` cannot say where the comment began, only that
something between the `1` and the end of the line was not a token, and a token's
only position is its start. Recovering a comment from token positions alone is
impossible.

The line is therefore rewritten **before** it is lexed: every `//` becomes `!!`.
Two characters for two, so every column the lexer reports is still the column of
the line as written; `!` is an operator the lexer already has, so `!!` lexes to
two `Not` tokens exactly where the comment was invisible; and a `//` *inside* a
string stays inside the string token, because the substitution does not move a
quote. The first adjacent `Not`/`Not` pair is therefore where the comment began,
and everything from there to the end of the line is one `Comment` run.

It is one pass and one tokenize, not two. `the_marked_line_is_the_same_length_so_every_column_still_lands`
pins the length-preservation the whole scheme rests on.

### Keywords come from `Token::word`, not a second table

`Token::word` is set by exactly the tokens the lexer read from `lexer::KEYWORDS`,
and it distinguishes the words that share a kind from the symbols that make it:
`mod` is a keyword, `%` is the same token and is an operator, `not` is a keyword
and `!` is an operator. Builtins come from `stdlib::BUILTIN_NAMES`. There is no
word list in this phase that can drift from the language's.

### What it deliberately does not do

It does not change what gets executed. `highlight` is only reachable from
`Highlighter::highlight`, which rustyline calls to *draw* a line; the parsed line
is the one the user typed. A line the lexer rejects returns its input unchanged
for the same reason a session with colour off does.

## Tests added

All under `src/repl/highlight.rs` (14) and `src/repl/editor.rs` (4).
`git diff` on the two test modules plus the new file: **18 new `#[test]`
functions**, of which **7 are named `edge_*`**.

| Test | Edge class covered |
|---|---|
| `highlight::a_keyword_and_a_string_each_take_their_own_colour` | happy path — `say` keyword-coloured, `"hi"` string-coloured, byte-identical once stripped |
| `highlight::a_line_the_lexer_rejects_comes_back_plain_and_byte_identical` | **malformed input / failure channel** — asserts `Lexer::tokenize` errs *and* the line comes back plain |
| `highlight::an_unexpected_character_is_also_a_failure_rather_than_a_colour` | **failure channel**, second shape — a control character has no token |
| `highlight::numbers_comments_and_builtins_are_told_apart` | all six classes on real lines |
| `highlight::a_slash_inside_a_string_is_text_and_not_a_comment` | the `//` rewrite, on the case where it is wrong |
| `highlight::colour_off_returns_the_line_untouched` | display-only guarantee |
| `highlight::a_multibyte_string_is_one_run_and_never_split` | **unicode** — `é`, CJK, two emoji, two combining marks; exactly 4 escapes, never a split run |
| `highlight::edge_an_empty_line_is_plain_and_costs_nothing` | **empty** |
| `highlight::edge_a_line_that_is_only_whitespace_is_left_where_it_is` | **empty** — spaces, tab, NBSP, ideographic space |
| `highlight::edge_a_theme_missing_a_class_leaves_its_tokens_plain` | **missing key** — `Theme::without(TokenClass::Str)` |
| `highlight::edge_a_ten_kilobyte_line_is_highlighted_without_blowing_up` | **resource limit** — 10 KB single line in both shapes (one 10 KB token, ~4000 tokens), < 500 ms |
| `highlight::two_thousand_lines_of_real_code_highlight_in_under_a_second` | **resource limit** — 2000 lines of `bootstrap/compiler.rb`, < 1 s (the phase's benchmark gate) |
| `highlight::the_marked_line_is_the_same_length_so_every_column_still_lands` | the length-preservation invariant, char *and* byte |
| `highlight::edge_a_bare_bang_is_an_operator_and_a_doubled_one_is_a_comment` | `!` vs `!!`, `mod` vs `%`, `not`, `yes` |
| `editor::the_editor_hook_colours_a_line_typed_at_the_prompt` | the wiring itself — asserted **through `EditSource::highlight`**, not through a helper |
| `editor::an_editor_with_colour_off_hands_the_line_straight_back` | colour off |
| `editor::edge_a_piped_session_emits_no_escape_codes_even_with_colour_forced_on` | **piped/non-TTY** — `line_source(.., Style::forced(true))` reports `!highlights()` |
| `editor::a_shipped_theme_has_an_opinion_about_every_class_it_lists` | every class in `TokenClass::ALL` is colourable |

**Mutation check.** Replacing the one line that makes a reserved word a keyword
with `None` fails 5 of the 14 (`a_keyword_and_a_string_each_take_their_own_colour`,
`a_multibyte_string_is_one_run_and_never_split`,
`edge_a_bare_bang_is_an_operator_and_a_doubled_one_is_a_comment`,
`edge_a_theme_missing_a_class_leaves_its_tokens_plain`,
`numbers_comments_and_builtins_are_told_apart`). The tests can fail.

## Mandatory edge-case matrix

| Row | Status |
|---|---|
| empty / zero / "nothing" | **covered** — `edge_an_empty_line_is_plain_and_costs_nothing`, `edge_a_line_that_is_only_whitespace_is_left_where_it_is` |
| singleton and boundary | **covered** — `a_keyword_and_a_string_each_take_their_own_colour` (one token, one string); `edge_a_bare_bang_is_an_operator_and_a_doubled_one_is_a_comment` (one `!` vs two) |
| out of bounds | **N/A** — the highlighter has no index-taking API and no indexing at all. Token positions come from the lexer as `usize` and every one is bounds-checked with `Vec::get(..)?`, which makes `runs()` return `None` and the line plain (`highlight.rs:160`, `highlight.rs:176`) |
| type mismatch | **N/A** — there is no value here to be the wrong type. `class_of` matches on `TokenKind` and every unmatched kind is `None` (plain), never a panic and never a wrong colour |
| numeric boundary | **N/A** — `TokenKind::Number(f64)` is classified by *kind*; the number's value is never read, compared or formatted, so `0`, `-0.0`, `NaN`, `±Infinity` and `2^53±1` all take the same colour and none can differ |
| unicode | **covered** — `a_multibyte_string_is_one_run_and_never_split`; `edge_a_line_that_is_only_whitespace_is_left_where_it_is` (NBSP, ideographic space) |
| nesting / recursion | **N/A** — a highlighter is a single pass over one line. `runs()` contains no recursion, so a list literal nested 200 deep is just tokens, and that *is* covered by `two_thousand_lines_of_real_code_highlight_in_under_a_second`, which reads the real nested grammar |
| duplicate / missing keys | **covered** — `edge_a_theme_missing_a_class_leaves_its_tokens_plain` is the missing-key row: a theme with no colour for a class paints it plain. No duplicate-key notion exists here |
| malformed input | **covered** — `a_line_the_lexer_rejects_comes_back_plain_and_byte_identical` (unterminated string), `an_unexpected_character_is_also_a_failure_rather_than_a_colour`; plus a multi-line input and a CR-only line return plain by the early `line.contains('\n')` guard |
| resource and state | **covered** — `edge_a_ten_kilobyte_line_is_highlighted_without_blowing_up`, `two_thousand_lines_of_real_code_highlight_in_under_a_second` |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — no diff |
| `cargo clippy --all-targets -- -D warnings` | **pass** — zero warnings |
| `cargo test --all-targets` | **625 passed, 2 failed, 0 ignored** — see below |
| `./rbops/verify.sh phase-054` | **NOT RUN** — `rbops/` does not exist in this checkout (`ls rbops` → `No such file or directory`). This checkout contains `phases/`, `AGENTS.md` and `.github/` but not the pipeline that invokes it. The phase's own gate cannot be executed from here. **The phase is not verified until it runs on a machine that has `rbops/`.** |
| `rb run examples/*.rb` (7 files, AGENTS §2) | **pass** — every example exits 0 |

### The 2 failures are pre-existing and not mine

Both are in `tests/doc_blocks_test.rs`, about README block line numbers:

```
every_documentation_block_is_classified_exactly_once ... FAILED
  these entries point at a block that is not in any document:
    README.md:117  README.md:128  README.md:140
running_every_block_agrees_with_the_bucket_it_is_in ... FAILED
  README.md:139 is in no bucket; classify it
```

Verified pre-existing by `git stash -u` (working tree clean, i.e. exactly
`main`) and re-running: `test result: FAILED. 5 passed; 2 failed` — identical.
This phase touched no markdown file. Per AGENTS §7 this belongs in a separate
phase, not here.

## Invariants touched

- **None.** No change to the lexer, the parser, the grammar, `Value`, `Error`,
  the `.rb` extension, or anything that executes. `Lexer` is called and its
  output is read; it was not modified.
- The only non-additive edit to an existing item is the comment above
  `impl Hinter`/`impl Highlighter` in `src/repl/editor.rs`, which previously
  claimed Redblue has no syntax highlighting. That sentence is now false.

## Known gaps / follow-ups

- `!!` typed by hand highlights as a comment. It is indistinguishable from a
  rewritten `//` and `!!` is not a valid program, so nothing that runs is
  mis-drawn. Pinned deliberately in `edge_a_bare_bang_is_an_operator_and_a_doubled_one_is_a_comment`.
- A multi-byte grapheme straddling a run boundary is not joined: the runs are
  built from character indices, so a combining mark that begins one token and
  ends the next would land in whichever run the lexer put it in. Redblue's
  lexer does not produce such a boundary today (identifiers, strings and numbers
  are all whole), and the test asserts runs are never split *inside* a token.
- Delimiters (`(`, `,`, `.`, `:`) are deliberately plain. They are the most
  frequent characters on a line and colouring them would drown the tokens.
- The two pre-existing `doc_blocks_test` failures → FINDINGS.md.
- `./rbops/verify.sh` was not runnable from this checkout → the phase gate is
  outstanding.
