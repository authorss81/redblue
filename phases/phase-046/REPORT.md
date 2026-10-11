# Phase 046 — Rich errors: suggestions and rendered diagnostics

## Finding reproduced

Verified on `main` before any change, with the checked-in binary:

```
$ printf 'set count to 10\nsay coun\n' > a.rb && ./target/debug/rb run a.rb
10
$ printf 'sya "hi"\n' > d.rb && ./target/debug/rb run d.rb
Error: AnalyzerError: Unknown variable 'sya'
  --> d.rb:1:1
1 | sya "hi"
  | ^
$ printf 'say "one\ntwo\nthree\n' > ml.rb && ./target/debug/rb run ml.rb
Error: LexerError: Unterminated string
  --> ml.rb:1:5
1 | say "one
  |     ^
$ grep -rn "did you mean\|suggest" src/ --include=*.rs | head
(no output)
```

Two things are wrong, and both are in the phase's evidence:

1. **No suggestion anywhere.** `sya` is `say` with two letters transposed — the
   single most common way to mistype a keyword — and the diagnostic names the
   unknown *variable* and nothing else. `say coun` after `set count to 10` gets
   the same.
2. **The rendered caret covers the whole failure.** The unterminated string runs
   to end of input, three lines of source, and the diagnostic prints line 1 and a
   caret on it. The reader is sent to the opening quote and told nothing about
   where the string was supposed to close.

The finding stands.

## What changed

| File | Lines | What |
|---|---|---|
| `src/suggest.rs` | +201 (new) | Optimal-string-alignment `distance`, `closest`, `did_you_mean`. A transposition costs one edit; a name is never suggested unless the distance is shorter than the name itself |
| `src/analyzer.rs` | +30 −2 | `add_unknown_variable`: the candidates are every scope's names plus every keyword, deduplicated and sorted, offered as one list so the nearest of them wins |
| `src/error.rs` | +96 −26 | `Span` carries an optional end (`end_line`/`end_column`, `extended_to`, `is_multiline`); `render` splits into `render_single` (byte-for-byte the old form) and `render_range` |
| `src/lexer.rs` | +14 −2 | `unterminated(span)` — the failure spans from the opening quote to wherever reading stopped |
| `src/lib.rs` | +1 | `pub mod suggest;` |
| `corpus/objects-0022.expected` | 1 −1 | `Unknown variable 'at'` → `Unknown variable 'at'; did you mean 'as'?` — the one recorded corpus expectation the new message changes |
| `tests/lexer_robustness_test.rs` | +8 −4 | two span assertions re-pinned with the end (see below) |
| `tests/tooling_grammar_test.rs` | +4 −2 | one span assertion re-pinned with the end |
| `tests/diagnostics_test.rs` | +509 (new) | 24 integration tests |
| `src/suggest.rs` (tests) | +70 | 6 unit tests over `distance` and `closest` |

### The suggestion rule

Two rules keep a suggestion worth reading:

- **A transposition is one mistake, not two.** `sya` for `say` is the most common
  way to mistype a keyword; a plain Levenshtein distance charges a substitution
  and an insertion for it, which puts the pair outside the budget and refuses the
  fix. The distance here is optimal string alignment — insertion, deletion,
  substitution, or swapping two adjacent characters.
- **A suggestion must be shorter than the name it is about.** A distance budget
  alone admits a one-character name, where every other name is one edit away and
  the "nearest" of them is an accident of the alphabet. `z` is never near
  anything, and `for each i in [1] … end / say i` still reads exactly
  `Unknown variable 'i'`, which is what `for_range_test.rs:218` pins.

The budget is `max(len/3, 1)` over the two names, so `coun`/`count` (1 of 5) and
`tehn`/`then` (1 of 4) are offered and `zzzzzzzzz` is not.

Candidates come out of a `HashSet`, whose iteration order is not promised, so they
are sorted and deduplicated and the winner is chosen by `(distance, name)` —
smallest distance, then first by sort order. The answer cannot depend on run to
run, and `edge_candidates_are_offered_the_same_way_however_often_the_program_is_run`
runs the same program seventeen times to say so.

Only the analyzer suggests. The two VMs' `Unknown variable` at runtime are not
touched: they are reached only where the analyzer did not see the read, and
changing one without the other would make the differential corpus test report a
divergence that does not exist. Recorded in `FINDINGS.md`.

### The range

`Span` records an end **only when the span covers more than one line**.
`extended_to` drops an end that is not past the start, so a one-line span is
still `Span::new(2, 8)` and compares equal to what it always compared equal to —
`PartialEq` stays derived over all four fields, and a range is honestly not a
point.

`render_single` is the old `render` body, moved out unchanged. `render_range`
numbers every line the span covers, pads the gutter to the width of the largest
line number, and draws `^~~~~` under the column the range opens at, running to
the end of the line it opens on. The end position is clamped to the lines the
source actually has: a span that runs to end of input ends one line past the last
line of the file, and those lines are not there to print.

### Two existing tests were re-pinned, not loosened

`Span` gaining an end made three value comparisons stale. Each was updated to the
*longer* assertion — the same start position, plus the end:

- `tests/lexer_robustness_test.rs::test_unterminated_string_is_a_spanned_lexer_error`
  — `Span::new(1, 5)` → `Span::new(1, 5).extended_to(2, 1)`
- `tests/lexer_robustness_test.rs::edge_unterminated_string_reports_the_line_the_quote_opened_on`
  — `Span::new(2, 5)` → `Span::new(2, 5).extended_to(4, 1)`, plus a new
  `is_multiline()` assertion
- `tests/tooling_grammar_test.rs::diagnostics_surface_a_spanned_lexer_error`
  — same, its own message invites exactly this ("update the pinned column")

`edge_escape_at_end_of_input_inside_a_string_is_still_an_error` needed no change:
`say "abc\` ends on its own line, so no end is recorded and the assertion still
holds.

`corpus/objects-0022.expected` records `object After extends Nothing at all`, where
`at` is a typo for `as`. The corpus is generated; the regenerated expectation is
what the generator now produces, and
`edge_regenerating_the_corpus_reproduces_the_checked_in_one_byte_for_byte` proves
the checked-in file is the generated one. Nothing else in the 200-program corpus
changed.

## What the reader gets

```
$ rb run typo.rb
Error: AnalyzerError: Unknown variable 'sya'; did you mean 'say'?
  --> typo.rb:1:1
1 | sya "hi"
  | ^

$ rb run stray.rb
Error: LexerError: Unterminated string
  --> stray.rb:1:5
1 | say "one
2 | two
3 | three
  |     ^~~~~
```

## Tests added

`tests/diagnostics_test.rs` — 24 new `#[test]` functions. `src/suggest.rs` — 6 new
`#[test]` functions in-module. 21 are named `edge_*`.

| Test | Edge class covered |
|---|---|
| `test_mistyped_keyword_suggests_the_intended_keyword` | the phase's first box: `sya` → `'say'` |
| `test_near_miss_identifier_suggests_the_declared_name` | the phase's second box: `coun` → `'count'` |
| `edge_transposed_and_missing_letters_both_count_as_one_mistake` | boundary — transposition (`adn`, `tehn`) and a dropped letter (`mport`), each one edit |
| `edge_a_name_with_no_near_match_is_reported_with_no_suggestion` | empty / far — `zzzzzzzzz` reported byte-for-byte as before |
| `edge_a_one_character_name_is_never_guessed_at` | boundary — `z` (len 1) never suggested; the rule that keeps `Unknown variable 'i'` exact |
| `edge_a_suggestion_does_not_replace_the_name_it_is_about` | — both names present, exactly two pairs of quotes |
| `edge_a_suggested_name_is_still_rejected_when_written` | **asserts a failure** — the suggested program runs, the written one is still refused, span unmoved |
| `test_a_suggestion_survives_into_the_rendered_diagnostic` | the message reaches what the user reads |
| `test_multiline_span_renders_every_offending_line` | golden — three lines, underlined on the first |
| `edge_multiline_span_past_the_end_of_the_file_is_clamped` | golden + out_of_bounds — one-line file, range ends past EOF |
| `edge_a_single_line_span_is_still_rendered_on_one_line` | golden — the one-line form is unchanged, caret included |
| `edge_the_error_variants_are_unchanged` | the phase's third box — all five labels still what a caller matches on |
| `test_a_two_line_range_draws_two_lines` | golden — the minimal range |
| `test_a_range_wide_enough_to_widen_the_gutter_is_aligned` | golden + boundary — a two-digit gutter (lines 10–12) aligns the underline |
| `edge_a_range_past_the_last_line_is_clamped_to_the_lines_there_are` | out_of_bounds — `extended_to(99, 1)` on a two-line source |
| `edge_an_end_on_the_start_line_is_not_a_range` | boundary — `extended_to(2, 4)` on line 2 is not multi-line and gains no underline |
| `edge_a_range_opening_past_the_end_of_its_line_underlines_one_character` | out_of_bounds — a column one past the last character; the underline must not run backwards |
| `edge_a_multibyte_line_is_underlined_by_character_and_not_by_byte` | unicode — two emoji; the caret is at character 5, not byte 34 |
| `edge_a_crlf_file_breaks_the_same_way_as_a_lf_file` | malformed_input — a CRLF file renders the same three lines |
| `test_a_name_from_an_outer_scope_is_suggested_inside_a_nested_body` | nesting_recursion — `total` bound at depth 0, suggested at depth 3 |
| `edge_a_name_declared_after_the_read_is_not_suggested_as_if_it_were_in_scope` | duplicate_missing_keys — `later` is not a scope; `count` bound below the read is not offered |
| `edge_a_unicode_name_typo_is_suggested_in_its_own_script` | unicode — `cafè` → `café`, measured in characters |
| `edge_candidates_are_offered_the_same_way_however_often_the_program_is_run` | determinism — 17 runs of a program whose candidates come out of a `HashSet` |
| `edge_a_duplicate_name_in_two_scopes_suggests_the_same_thing` | duplicate_missing_keys / nesting — one name shadowed three frames deep is one candidate |

`src/suggest.rs`: `test_a_transposition_costs_one` (distance, including empty
inputs), `edge_the_nearest_of_several_is_chosen`, `edge_a_tie_is_broken_the_same_way_every_time`
(forward and backward orderings name the same thing), `edge_no_candidate_is_too_close`
(empty list, empty target, one-character target, a name suggested back to itself),
`edge_a_single_candidate_is_accepted_or_rejected_on_its_merits`,
`test_the_message_half_reads_as_a_sentence`.

### Edge-case matrix

- **empty** — covered. `edge_no_candidate_is_too_close` (`closest("x", [])` and
  `closest("", …)` are both `None`); `edge_a_name_with_no_near_match_…` for a
  message with nothing to add; `edge_an_end_on_the_start_line_is_not_a_range` for a
  source where the range has nothing to cover.
- **singleton** — covered. `edge_a_single_candidate_is_accepted_or_rejected_on_its_merits`
  is a one-element candidate list, accepted on its merits and rejected on its
  merits; `edge_multiline_span_past_the_end_of_the_file_is_clamped` is a one-line
  file.
- **boundary** — covered. `edge_a_one_character_name_is_never_guessed_at` (the
  shortest name there is), `edge_an_end_on_the_start_line_is_not_a_range` (an end
  exactly at the start is not a range), `test_a_range_wide_enough_to_widen_the_gutter_is_aligned`
  (the gutter crossing from one digit to two), `edge_transposed_…` (the budget's
  own boundary).
- **out_of_bounds** — covered. `edge_a_range_past_the_last_line_is_clamped_to_the_lines_there_are`
  (an end 97 lines past the file), `edge_a_range_opening_past_the_end_of_its_line_underlines_one_character`
  (a column one past the last character). Both are rendered, not clamped away, and
  neither panics.
- **type_mismatch** — N/A. This phase changes no value, no operation and no type
  rule. It reads a name and renders a position; it never inspects what a name
  holds, so there is no point at which a wrong-typed operand could change its
  behaviour. `edge_a_suggested_name_is_still_rejected_when_written` asserts the
  property this phase *could* have broken — that suggesting a fix does not make
  the program legal.
- **numeric_boundary** — N/A for the same reason: no number is read, compared or
  converted. The one numeric boundary that exists here is the distance budget,
  pinned by `edge_transposed_and_missing_letters_both_count_as_one_mistake` (1 of
  3 and 1 of 5 accepted) and `edge_a_name_with_no_near_match_…` (0 of 9 refused).
- **unicode** — covered. `edge_a_unicode_name_typo_is_suggested_in_its_own_script`
  (a name in a non-ASCII script), `edge_a_multibyte_line_is_underlined_by_character_and_not_by_byte`
  (two emoji; columns counted in characters, underline counted in characters). The
  distance itself is over `char`, not bytes, for the same reason.
- **nesting_recursion** — covered. `test_a_name_from_an_outer_scope_is_suggested_inside_a_nested_body`
  reads at `for` → `if` → statement depth; `edge_a_duplicate_name_in_two_scopes_…`
  binds the same name at three depths. The candidate list is every scope, not the
  innermost one, and neither test recurses.
- **duplicate_missing_keys** — covered as the nearest thing this phase has. There
  are no record keys involved, but there are duplicate *candidates* and a missing
  one: `edge_a_duplicate_name_in_two_scopes_…` (a name in several scopes is
  deduplicated into one candidate) and
  `edge_a_name_declared_after_the_read_is_not_suggested_…` (a name the program
  binds below the read is *missing* from the candidates, because `later` is not a
  scope).
- **malformed_input** — covered. `edge_a_crlf_file_breaks_the_same_way_as_a_lf_file`
  (CRLF), `edge_multiline_span_past_the_end_of_the_file_is_clamped` and
  `edge_a_range_past_the_last_line_…` (a span reaching past end of file),
  `test_multiline_span_renders_every_offending_line` (the unterminated string
  itself). An empty file, a BOM and a lone stray token are already covered by
  `tests/span_test.rs::edge_empty_source_is_a_valid_empty_program` and
  `tests/lexer_robustness_test.rs`, neither of which this phase changed.
- **resource_limit** — N/A, with the arithmetic stated. The only work added is
  `distance`, `O(n·m)` over the two names' characters, run once per unbound read
  against a candidate list bounded by the program's own names plus the 57
  keywords. Nothing is unbounded, nothing is allocated per statement, nothing
  recurses, and no new cap is needed: the inputs are a program's identifiers, not
  its runtime. No existing step, call-depth or iteration limit was touched.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, no warnings |
| `cargo test --all-targets` | 1361 passed, 0 failed, 0 ignored (50 test binaries) |
| `cargo test --doc` | 2 passed, 0 failed |
| `rb run examples/*.rb` and `modules/*.rb` | every one exits 0 |
| `./rbops/verify.sh phase-046` | **not run — `rbops/` is not in this checkout** (see below) |

### The fourth gate, honestly

The brief names `./rbops/verify.sh phase-046` as the fourth gate. This working
directory is the project checkout: `ls rbops` returns `No such file or directory`,
and the brief instructs me not to read or write outside it. I could not run that
script and am not claiming a result for it. Everything it is described as
checking that I *could* run, I ran: the two cargo gates, the full test suite (which
includes the 200-program corpus, the two-VM differential test, the bootstrap
fixed-point test and the LSP/REPL/wasm suites), the doc tests, and every example
and module. No test was deleted, skipped, `#[ignore]`d or given an
`allow(clippy::…)`; no assertion was loosened; no threshold was lowered. The three
span assertions that had to move are stronger than they were, and the one corpus
expectation that moved is the one the generator now produces.

`must_touch: ["src/"]` is satisfied: `src/suggest.rs` (new), `src/analyzer.rs`,
`src/error.rs`, `src/lexer.rs`, `src/lib.rs`.

## Invariants touched

- **None of the language.** `.rb` is still the extension; `to … end` / `if … end`
  still close with `end`; `set x to <expr>` is still assignment; `say` still
  prints; the `{interp}` and trailing-comma syntaxes are untouched; `Value`'s
  variants are untouched.
- **`Error::{Lexer,Parser,Analyzer,Runtime,Limit,Io}` is unchanged.** No variant
  added, removed or renamed, and `label()` still spells all five of them exactly
  as before. `edge_the_error_variants_are_unchanged` pins the labels. (`Limit`
  already existed; this phase did not add it.)
- **`Span` gained two public fields** and three methods (`extended_to`,
  `is_multiline`, and `new` now fills the new fields). This is the one additive
  API change. It is additive and backward-compatible for every construction in the
  tree — `Span::new` keeps its signature and its meaning, and no struct literal
  builds a `Span` outside `src/error.rs` — but a caller that destructures
  `Span { line, column }` exhaustively would need the two new names. `PartialEq`
  is still derived over all four fields.
- **One recorded message changed**: `Unknown variable 'x'` may now be
  `Unknown variable 'x'; did you mean 'y'?`. Any caller matching the whole string
  rather than a prefix sees the longer one. This is the phase's point, and
  `tests/for_range_test.rs:218`, which pins `Unknown variable 'i'` exactly, still
  passes — because the rule that makes it pass is a tested rule.
- **Additive public module** `redblue::suggest`, exporting `distance`, `closest`
  and `did_you_mean`.

## Known gaps / follow-ups

- Only the analyzer suggests. The two VMs' *runtime* `Unknown variable`
  (`src/interpreter.rs:1832`, `src/bytecode/vm.rs:1717`) still has no hint. It is
  reached where the analyzer did not see the read, and changing one VM without
  the other would make `differential_test::every_corpus_program_agrees_between_the_two_vms`
  report a divergence. Doing it needs both, in one change. → `FINDINGS.md`.
- Builtin names are not candidates. `say uppercas("hi")` is not an unknown-variable
  failure (a call's existence is a runtime check) so it is out of scope here, but
  a *variable* named after a builtin typo — `set coun to 1` with `count` never
  declared — has nothing to be suggested, because the candidate list is the
  program's own scopes plus keywords. `stdlib::is_builtin` names could be added;
  it is deliberately not added here because it would change messages for programs
  outside this phase's evidence. → `FINDINGS.md`.
- The unclosed-block failure (`if x then` with no `end`) is still reported at end
  of input on one line, even though the block spans many. Making it a range means
  threading the opening keyword's position into the ten
  `self.expect(&TokenKind::End)?` call sites in `src/parser.rs` — a parser change,
  not a diagnostic change, and too wide for this phase. → `FINDINGS.md`.
- A mistyped *keyword in keyword position* is still a bare parser error:
  `iff 1 is 1 then` reads `Expected Then but got Identifier("iff")` with no
  suggestion, because the parser does not know what word would have fit. The
  analyzer case (`sya "hi"`, where the word lands where a name is expected) is
  covered and pinned. → `FINDINGS.md`.
- `Span::extended_to` is called from exactly one production site (the lexer's
  unterminated string). The renderer's every branch is golden-tested through the
  constructor directly, but a second production range would be the honest test
  that the rendering matches what the language actually needs.
