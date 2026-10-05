# Phase 015 — Formatter must be idempotent and lossless

## What changed

Three runs contributed. The first (commit `329e83d`) fixed the finding; the
second (`fb4c0ae`) closed the coverage gap it left behind; the third (this
round) fixed everything review round 1 raised against the first two — one
BLOCKER and four MAJORs, all of them real. A verification pass between the
second and third re-ran every gate and corrected the test counts, which the
earlier runs understated — see "Count correction" below.

| File | Lines | What |
|---|---|---|
| `src/formatter.rs` | +384 −147 | **run 1** — comment attachment (`//` comments no longer dropped), exact `needs_reformat` comparison helper, exact trailing-newline emission, text-literal escaping that round-trips, operator-grouping parentheses preserved, no state carried between `format` calls |
| `src/lib.rs` | +1 −1 | **run 1** — `rb format --check` now calls `formatter::needs_reformat(&source, &formatted)` instead of `source.trim() != formatted.trim()` |
| `tests/formatter_test.rs` | +978 −0 | **run 1** — 39 `#[test]` functions, 21 `edge_*` |
| `src/formatter.rs` | +98 −1 | **run 2** — `give back` pair rendering and block-tail comment attachment |
| `tests/formatter_test.rs` | +370 −0 | **run 2** — 12 `#[test]` functions, all 12 `edge_*`: 7 pinning comment attachment at block depth (else, try/catch/finally, nested block tail, trailing comment) and 5 covering the spec-documented forms no corpus file and no prior test reached (see F5) |
| `src/formatter.rs` | +40 −10 | **run 3 (this round)** — the `catch` BLOCKER and block-opening-line comment loss, see "Review round 1" |
| `tests/formatter_test.rs` | +455 −38 | **run 3 (this round)** — 10 `#[test]` functions, all 10 `edge_*`, and a structural replacement for the debug-string tree oracle |

Phase total, `27d4aeb`→`HEAD`: `src/formatter.rs` +521 −157 (476 → 840
lines), `tests/formatter_test.rs` +1841 −38 (new file), `src/lib.rs` +1 −1.
`src/lib.rs` shows +33 −1 over that range because phase-017 also edited it;
phase-015's share is the single `needs_reformat` line.

### The finding, reproduced

The finding quoted `src/formatter.rs` as "431 lines"; at the pre-phase commit
`27d4aeb` it was **476** (`git show 27d4aeb:src/formatter.rs | wc -l`). The
claim of "zero tests" is the load-bearing part and was correct:
`tests/formatter_test.rs` did not exist before this phase.

Before the change, `src/lib.rs:173` compared:

```rust
if source.trim() != formatted.trim() {
```

`trim()` erases trailing whitespace and the trailing newline, so a file
differing from its formatted form *only* in those respects was reported as
already formatted. Re-verified against the current tree in the verification
pass — all now exit 1, which the old comparison would have reported as exit 0:

```
printf 'say "hi"\n   \n'  > f.rb ; rb format --check f.rb   # trailing whitespace  -> exit 1
printf 'say "hi"'        > f.rb ; rb format --check f.rb   # no trailing newline   -> exit 1
printf 'say "hi"\r\n'    > f.rb ; rb format --check f.rb   # CRLF line ending      -> exit 1
printf 'say "hi"\n\n'    > f.rb ; rb format --check f.rb   # trailing blank line   -> exit 1
printf 'say "hi"\n'      > f.rb ; rb format --check f.rb   # already formatted     -> exit 0
```

## Tests added

**61** `#[test]` functions in `tests/formatter_test.rs`, **43** named
`edge_*`. 39 from run 1, 12 from run 2, 10 from run 3. Zero `#[ignore]`,
`// skip`, or `allow(clippy::`. No test was deleted in run 3; the one function
that *was* removed is `strip_spans`, the debug-string helper replaced by the
structural walk in the BLOCKER-4 fix below.

> **Count correction (verification pass).** The previous revision of this report
> stated "44 tests, 26 `edge_*`, 5 from this resume". That was wrong on all
> three counts. Measured directly:
> `grep -A1 '#\[test\]' tests/formatter_test.rs | grep -c '^\s*fn '` → **61**,
> `grep -o 'fn edge_[a-z0-9_]*' | wc -l` → **43**, and diffing the test-name
> sets between `329e83d` and `fb4c0ae` shows **12** names added in run 2, not 5.
> Run 2 also changed `src/formatter.rs` (+98 −1), which the old "What changed"
> table did not list at all. The numbers are now measured rather than asserted.
> The report understated the phase; the code and the tests are unchanged by
> this correction.

### Definition of done

| Requirement | Test | Result |
|---|---|---|
| idempotence property over `examples/` + `tests/` | `format_is_idempotent`, `format_is_idempotent_over_the_whole_corpus` | covered |
| format then parse gives an equivalent AST for every example | `format_preserves_the_meaning_of_every_corpus_file`, `same_program` compares every `Program`/`Stmt`/`Statement`/`Expr` field, `span` excepted; `edge_the_tree_comparison_is_not_fooled_by_text_that_looks_like_a_span` pins that the comparison is structural | covered |
| `--check` compares exactly, including trailing newline | `check_compares_exactly_including_the_trailing_newline` (helper), `formatted_output_ends_with_exactly_one_newline`, `check_rejects_a_file_that_differs_only_in_its_last_byte` (helper), and — **through the binary** — `edge_the_check_flag_reports_a_difference_through_the_exit_code`, `edge_the_check_flag_rejects_a_file_it_cannot_format`, `edge_the_check_flag_accepts_what_the_format_flag_prints` | covered |
| formatter preserves comments | `format_preserves_comments`, `format_preserves_every_comment_in_every_corpus_file`, `format_preserves_comments_inside_blocks_and_at_the_end`, `edge_a_comment_trailing_a_catch_line_stays_after_the_name_it_annotates`, `edge_a_comment_trailing_a_block_opening_line_stays_on_that_line`, `edge_a_comment_trailing_the_only_line_of_an_empty_block_stays_there` | covered |

## Review round 1 — five findings, all fixed

Review raised one BLOCKER and four MAJORs against runs 1 and 2. Every one was
real; each is fixed here with a test that fails against the pre-fix code.

### 1. BLOCKER — `catch` + a trailing comment produced unparseable output

`src/formatter.rs` routed the `catch` line through `write_keyword_line`, which
writes the keyword and then *ends the line* with the trailing comment; the
error's name was appended after that. Reproduced on the pre-fix code:

```
$ rb format catch.rb          # try / say 1 / catch err // note / say err / end
try
    say 1
catch  // noteerr
    say err
end
```

The name landed inside the comment. The result does not read back as `catch` at
all, so this broke both idempotence and losslessness — the two properties the
phase exists to guarantee — and nothing covered it, because no test had a
comment on a `catch` line.

Fix: the `catch` line is now built as indent + `catch ` + name + **one**
trailing comment. Pinned by
`edge_a_comment_trailing_a_catch_line_stays_after_the_name_it_annotates`, which
asserts the exact bytes, asserts `same_program` still holds, and asserts
`catch //` never appears in the output.

### 2. MAJOR — a comment trailing a block-opening line moved off that line

`if … then`, `for each …`, `repeat … times`, `while …`, `to …`, `to can …`,
`object …`, `try` and `test …` never claimed the comment written at the end of
their own line; `format_statements` looked for it only after the whole
statement had been written. Three observed wrong placements, all on the
pre-fix code:

```
if 1 is 1 then // note        try // t          repeat 3 times // rep
    say 1                     say 1             end
end                    →    catch e       →    say 1
                         end
```

- as a leading comment on the block's *first statement* (`note` read as a note
  about `say 1`);
- trailing the `end` that closed the block;
- for a block with an empty body, trailing that same `end` — so `repeat 3 times
  // rep` came out as `repeat 3 times` / `end // rep`.

Fix: a new `Formatter::open_line` claims the statement's own trailing comment
before the block is written. Pinned by
`edge_a_comment_trailing_a_block_opening_line_stays_on_that_line` (every
block-opening form at once, byte-exact, plus a `!contains("end //")` guard) and
`edge_a_comment_trailing_the_only_line_of_an_empty_block_stays_there` (the
empty-body case).

Both fixes were confirmed non-vacuous: reverting them one at a time fails
**3** of the new tests, and reverting both fails the same 3.

### 3. MAJOR — the `rb format --check` CLI branch was never exercised

`needs_reformat` had a unit test, but the branch that calls it — `src/lib.rs`,
the four-argument `format` + `--check` case — was untested, and `run_rb` took a
single mode so no test in the file could express `rb format --check <file>`.
Reintroducing `trim()` there would have kept the suite green.

Fix: `run_rb` now delegates to a new `run_rb_args`, and `run_format_check`
drives the real binary. Three new tests:

- `edge_the_check_flag_reports_a_difference_through_the_exit_code` — exit 0 for
  a formatted file with empty stdout; exit 1 plus
  `File would be reformatted` for seven byte-level differences (no trailing
  newline, trailing blank line, trailing whitespace, whitespace-only last line,
  CRLF, doubled space, leading blank line). Every one of those is a difference a
  `trim()` on both sides would erase.
- `edge_the_check_flag_rejects_a_file_it_cannot_format` — exit 1,
  `Format error` on stderr, nothing on stdout.
- `edge_the_check_flag_accepts_what_the_format_flag_prints` — for every corpus
  file, `rb format`'s own stdout passes `rb format --check`, and one trailing
  space on it does not.

Mutation check: putting `source.trim() != formatted.trim()` back — the original
reported bug — fails **2** of these three
(`edge_the_check_flag_reports_a_difference_through_the_exit_code`,
`edge_the_check_flag_accepts_what_the_format_flag_prints`). Before this round,
that mutation left the suite green.

### 4. MAJOR — the losslessness oracle was string surgery on `{:?}`

`same_program` rendered both trees with `format!("{:?}")` and cut every
`Span { … }` out of the string. A debug rendering contains the program's own
text, so `say "Span { line: 1 }"` had *the literal* cut out of it too — and two
programs differing only inside that text compared **equal**. The oracle the file
trusts most could false-pass and false-fail.

Fix: `same_program` is now a structural walk — `same_stmts` → `same_stmt` →
`same_statement` → `same_expr`, every variant matched explicitly, `Stmt::span`
the only field excluded, `Expr::Number` compared by `to_bits()` so `0.0`/`-0.0`
stay distinguishable and a `NaN` no source can spell does not make a tree
unequal with itself. An unnamed pair of variants returns `false`, so a variant
added to the AST later fails here loudly instead of comparing equal by default.

Pinned by `edge_the_tree_comparison_is_not_fooled_by_text_that_looks_like_a_span`
(which asserts the old behaviour was a false pass) and
`edge_the_tree_comparison_detects_a_change_behind_any_shape_of_text`. Mutation
check: putting `strip_spans` back fails the first of those two.

### 5. MAJOR — the resource-limit test was inside the budget

`edge_a_very_deeply_nested_program_is_handled_or_reported` used depth 60, below
`MAX_NESTING_DEPTH`/`MAX_BLOCK_DEPTH` of 64 (`src/parser.rs:243`, `:252`), so its
`Err` arm could never be taken. The depth-60 idempotence case is kept
unchanged; two tests were added that are past the budget:

- `edge_a_program_deeper_than_the_parser_budget_is_reported_not_truncated` —
  `MAX_NESTING_DEPTH + 6`, asserted to be a `Parser error` naming
  `parser::MAX_NESTING_DEPTH` (not a hard-coded 64), and to exit 1 with
  `Format error` and empty stdout through the binary.
- `edge_blocks_deeper_than_the_parser_budget_are_reported_not_truncated` —
  `MAX_BLOCK_DEPTH + 6` nested `if`s, for the block half of the budget.

### Re-measured after the fixes

Both properties re-verified from the binary, not from the tree:

- 2436-comment sweep: a marker comment injected at **every line position of
  every corpus file** — 2436 cases — 0 markers lost, 0 non-idempotent, 0
  semantic drift.
- 10251-mutation sweep: line deletion, duplication, blank lines, stray tabs,
  trailing whitespace, CRLF across the corpus — 0 non-idempotent, 0 drift.
  (1872 variants are rejected by the parser and were counted, not skipped.)
- Idempotence + `rb format --check` over the tree's `.rb` files: **20 pass,
  0 problems, 1 parse-skip** (`modules/MathUtils.rb`, F1).
- `PARITY-OK` on all 5 deterministic examples; `examples/time.rb` differs only
  in its wall-clock output, as before.
- `rb format` 5× on the same file: byte-identical.

### Added in run 2 — comment attachment at block depth

Run 2 added block-tail comment handling in `emit_comments`
(`src/formatter.rs:673`) plus 7 tests that pin where a comment lands when a
block's only tail *is* a comment:

| Test | Edge class covered |
|---|---|
| `edge_a_comment_trailing_a_statement_stays_in_that_statement_block` | nesting_recursion |
| `edge_a_comment_at_the_end_of_a_block_stays_in_that_block` | nesting_recursion |
| `edge_a_comment_at_the_end_of_a_nested_block_stays_at_its_own_depth` | nesting_recursion |
| `edge_a_block_whose_only_tail_is_a_comment_keeps_it_inside` | boundary |
| `edge_a_comment_before_else_stays_in_the_then_branch` | nesting_recursion |
| `edge_a_comment_in_a_try_catch_finally_stays_with_its_branch` | nesting_recursion |
| `edge_a_block_comment_is_still_a_parser_error_when_the_block_is_unclosed` | malformed_input — **asserts failure** |

### Added in run 2 — the coverage gap F5 exposed

Auditing which `Stmt`/`BinaryOp` variants the parser can actually produce
showed four spec-documented forms reach no corpus file and no prior test:

| Test | Edge class covered |
|---|---|
| `edge_a_for_from_to_loop_is_reported_not_silently_rewritten` | malformed_input — **asserts failure** (`for … from … to … by …`, `SPEC.md:388`) |
| `edge_a_symbolic_relational_operator_is_reported_not_guessed_at` | malformed_input — **asserts failure** (`<`, `docs/GRAMMAR.md:438`) |
| `edge_a_comparison_containing_or_is_reported_not_silently_rewritten` | malformed_input — **asserts failure** (`is greater than or equal to`; also asserts the CLI exits 1 and prints nothing it cannot stand behind) |
| `edge_a_word_comparison_fails_the_same_way_before_and_after_formatting` | malformed_input — **asserts failure**; exit code, stdout and error *identity* unchanged by formatting |
| `edge_every_spec_only_form_is_either_idempotent_or_a_clean_reported_error` | malformed_input / boundary — umbrella: each form is idempotent or a clean lexer/parser error, never a silent rewrite |

Two of these caught wrong assumptions while being written, which is the evidence
they are not vacuous:

- `edge_a_word_comparison_fails_the_same_way_before_and_after_formatting`
  initially compared whole `stderr` and failed: a diagnostic quotes the source
  line it read, and formatting moved that text. Only the `Error: …` head is
  compared now, via the `error_line` helper. The quoted line is a picture of
  the formatted file, not part of the error.
- `edge_a_comparison_containing_or_is_reported_not_silently_rewritten`
  initially treated both word comparisons alike and failed: `is less than`
  parses into three statements, but `is greater than **or** equal to` does not
  parse at all, because `or` is a real keyword. The two cases are now separate
  tests.

### Added in run 3 — the review-round-1 fixes

| Test | Edge class covered |
|---|---|
| `edge_a_comment_trailing_a_catch_line_stays_after_the_name_it_annotates` | malformed_input / nesting_recursion — **the BLOCKER**: `catch err // note` came out as `catch  // noteerr` |
| `edge_a_comment_trailing_a_block_opening_line_stays_on_that_line` | nesting_recursion — every block-opening form with a trailing comment, byte-exact |
| `edge_a_comment_trailing_the_only_line_of_an_empty_block_stays_there` | boundary — the empty-body case, where the note used to land on `end` |
| `edge_the_check_flag_reports_a_difference_through_the_exit_code` | boundary — **asserts failure** (exit 1) through the real binary |
| `edge_the_check_flag_rejects_a_file_it_cannot_format` | malformed_input — **asserts failure** (exit 1, `Format error`) |
| `edge_the_check_flag_accepts_what_the_format_flag_prints` | boundary / semantics — `rb format` output passes `rb format --check`, and a trailing space does not |
| `edge_the_tree_comparison_is_not_fooled_by_text_that_looks_like_a_span` | coverage — the losslessness oracle itself, on a program holding span-shaped text |
| `edge_the_tree_comparison_detects_a_change_behind_any_shape_of_text` | coverage — the oracle still sees a difference behind braces, quotes and backslashes |
| `edge_a_program_deeper_than_the_parser_budget_is_reported_not_truncated` | resource_limit — **asserts failure** (`MAX_NESTING_DEPTH + 6`) |
| `edge_blocks_deeper_than_the_parser_budget_are_reported_not_truncated` | resource_limit — **asserts failure** (`MAX_BLOCK_DEPTH + 6`) |

### Edge class covered

| Test | Edge class covered |
|---|---|
| `edge_empty_program_formats_to_nothing` | empty |
| `edge_empty_text_literal` | empty |
| `edge_singleton_inputs` | singleton / boundary |
| `edge_numeric_boundaries_round_trip` | numeric_boundary |
| `edge_unicode_text_round_trips` | unicode |
| `edge_text_holding_a_quote_and_a_backslash_round_trips` | unicode / escapes |
| `edge_a_literal_spanning_lines_keeps_its_slashes_and_later_comments` | unicode / escapes / malformed-adjacent |
| `edge_a_very_long_text_literal_round_trips` | resource_limit |
| `edge_a_very_deeply_nested_program_is_handled_or_reported` | resource_limit / nesting_recursion |
| `edge_nested_lists_and_records_round_trip` | nesting_recursion |
| `edge_duplicate_record_keys_keep_their_order` | duplicate_missing_keys |
| `edge_missing_record_field_is_not_invented_by_formatting` | duplicate_missing_keys |
| `edge_a_duplicate_key_is_still_overwritten_after_formatting` | duplicate_missing_keys |
| `edge_crlf_source_formats_to_lf_and_is_idempotent` | malformed_input |
| `edge_a_bom_does_not_become_part_of_the_first_statement` | malformed_input |
| `edge_unclosed_block_is_a_parser_error` | malformed_input |
| `formatter_rejects_malformed_input_instead_of_guessing` | malformed_input — **asserts failure** |
| `edge_a_program_that_fails_still_fails_the_same_way_after_formatting` | malformed_input — **asserts failure**, failure parity |
| `edge_grouping_survives_in_what_the_program_computes` | nesting_recursion — executes both forms, compares output |
| `format_keeps_the_grouping_the_parentheses_forced` | nesting_recursion |
| `format_keeps_right_hand_grouping_of_equal_precedence` | nesting_recursion |
| `format_keeps_grouping_for_unary_and_every_operator_level` | nesting_recursion |
| `edge_equality_against_a_negation_keeps_its_parentheses` | nesting_recursion |
| `check_rejects_a_file_that_differs_only_in_its_last_byte` | boundary — **asserts failure** (exit code 1) |
| `edge_slashes_inside_a_text_literal_are_not_comments` | malformed_input / unicode |
| `edge_a_comment_that_is_only_slashes_is_kept` | boundary |
| `format_writes_index_with_brackets_not_at` | singleton / boundary |
| `formatter_instance_does_not_carry_state_between_calls` | determinism |
| `formatted_programs_behave_exactly_as_the_originals` | semantics — runs the real `rb` binary on original and formatted source and compares stdout |
| `format_covers_every_statement_form` | coverage |
| the 5 tests in the table above | malformed_input / boundary |

### Edge-case matrix

- **empty** — covered (`edge_empty_program_formats_to_nothing`, `edge_empty_text_literal`)
- **singleton** — covered (`edge_singleton_inputs`, `format_writes_index_with_brackets_not_at`)
- **boundary** — covered (`check_rejects_a_file_that_differs_only_in_its_last_byte`, `edge_a_comment_that_is_only_slashes_is_kept`, `formatted_output_ends_with_exactly_one_newline`, `edge_every_spec_only_form_is_either_idempotent_or_a_clean_reported_error`)
- **out_of_bounds** — N/A. The formatter indexes no runtime value; it walks the AST it just parsed. `grep` for `unwrap()`/`panic!`/`unreachable!`/`.expect(` in `src/formatter.rs` returns nothing. There are exactly two index expressions, `src/formatter.rs:209` (`&body[i]`) and `:225` (`&body[i + 1]`); the first is guarded by `while i < body.len()` and the second by the `next_give_back` flag, which is only true when `body.get(i + 1)` returned `Some` (`src/formatter.rs:212`). Both loop exits advance `i` (`:231` `i += 2`, `:247` `i += 1`), so neither can panic or spin. Index bounds proper are a VM concern, already covered by `tests/index_bounds_test.rs`.
- **type_mismatch** — N/A. The formatter never evaluates or converts a value; it only re-renders tokens. There is no expression at which a type could be wrong. The nearest thing is the spec forms in F5, now covered by 5 tests.
- **numeric_boundary** — covered (`edge_numeric_boundaries_round_trip`) — numeric literals re-render byte-identically.
- **unicode** — covered (`edge_unicode_text_round_trips`, `edge_text_holding_a_quote_and_a_backslash_round_trips`, `edge_a_bom_does_not_become_part_of_the_first_statement`)
- **nesting_recursion** — covered (13 tests: the 6 listed above plus the 7 block-depth comment tests added in run 2)
- **duplicate_missing_keys** — covered (3 tests, listed above)
- **malformed_input** — covered (13 tests: the 5 F5 tests, the 5 listed above, and the 3 added in run 2)
- **resource_limit** — covered (`edge_a_very_deeply_nested_program_is_handled_or_reported`, `edge_a_very_long_text_literal_round_trips`)

### Independent verification beyond the suite

Driven against the built binary, not inferred from the tree. Run 1 recorded the
first three bullets; the second resume added the rest, the verification pass
re-ran all of them plus the mutation checks below, and run 3 re-ran them all
again after the fixes (the numbers are in "Review round 1 → Re-measured after
the fixes").

- Idempotence + `format --check` accepted its own output over all 21 `.rb`
  files in `examples/`, `modules/`, `tests/` — 0 non-idempotent, 0 rejected
  by `--check` after formatting. (`modules/MathUtils.rb` is the one file that
  does not parse at all; see F1.) Re-measured in the verification pass:
  **20 idempotent, 0 problems, 1 parse-skip**.
- All 6 `examples/*.rb` run; `modules/SuiteKit.rb` runs.
- All 6 examples produce byte-identical stdout before and after formatting
  (`examples/time.rb` excluded from the byte comparison — it prints
  `time.now()` nanoseconds, which differ between any two runs; its parity is
  covered instead by `formatted_programs_behave_exactly_as_the_originals`,
  which uses deterministic fixtures). Re-measured: `PARITY-OK` on all 5
  deterministic examples.
- **840-mutation fuzz**: each corpus file mutated 40 ways — line
  deletion, duplication, blank lines, stray tabs/indent, injected comments,
  trailing whitespace, CRLF. 0 non-idempotent, 0 reformat errors, 0 semantic
  drift outside `examples/time.rb`'s wall-clock output. Widened in run 3 to
  **10251 cases** with the same 0/0 result.
- **Comment-position sweep**: a `// MARKXYZ` comment injected at
  every line position of 5 block-structured programs — **115 cases**, marker
  lost **0** times. (An earlier run of this sweep in the previous revision
  reported 23 cases; it named files that do not exist in `examples/`, so the
  `for` loop body never executed and produced a silently empty result. The
  115 above is the real count.) Run 3 widened it to **every line position of
  every corpus file — 2436 cases**, still 0 lost, 0 non-idempotent, 0 drift.
- **Determinism**: `rb format` run 5× on each of 20 files, all
  byte-identical; a 36-key record formats to a stable key order.
- **Escape round-trip**: `\n \t \r \\ \"` in a text literal each
  decode and re-encode to the same literal.

### Mutation testing — evidence the tests can actually fail

The reviewer's standing hunt is "tests that cannot fail". Each headline
behaviour — and, in run 3, each of the five review findings — was mutated back
to its broken form and the suite re-run. Every mutation was caught, by the tests
that claim to cover it, and the source was restored (`git status src/` clean
afterwards each time).

| Mutation | Result |
|---|---|
| `needs_reformat` reverted to `source.trim() != formatted.trim()` — i.e. the original reported bug | **4 failed**, 57 passed: `check_compares_exactly_including_the_trailing_newline`, `check_rejects_a_file_that_differs_only_in_its_last_byte`, `edge_the_check_flag_reports_a_difference_through_the_exit_code`, `edge_the_check_flag_accepts_what_the_format_flag_prints` |
| `emit_comments` line-guard removed (`src/formatter.rs:676`), so block-tail comments attach at the wrong depth | **10 failed**, 51 passed: the 6 block-depth comment tests plus the 3 added in run 3 plus `edge_a_literal_spanning_lines_keeps_its_slashes_and_later_comments` |
| `catch` routed back through `write_keyword_line` (finding 1) | **2 failed**, 59 passed: `edge_a_comment_trailing_a_catch_line_stays_after_the_name_it_annotates`, `edge_a_comment_trailing_a_block_opening_line_stays_on_that_line` |
| `open_line` reduced to a bare `newline()` (finding 2) | **2 failed**, 59 passed: `edge_a_comment_trailing_a_block_opening_line_stays_on_that_line`, `edge_a_comment_trailing_the_only_line_of_an_empty_block_stays_there` |
| Both of the above at once | **3 failed**, 58 passed: all 3 run-3 comment tests |
| `same_program` reverted to `strip_spans` on `{:?}` (finding 4) | **1 failed**, 60 passed: `edge_the_tree_comparison_is_not_fooled_by_text_that_looks_like_a_span` |
| `MAX_NESTING_DEPTH` / `MAX_BLOCK_DEPTH` raised from 64 to 10000 (finding 5) | **2 failed**, 59 passed: both depth tests, on their re-anchor assertion — with the budget raised, the 70-block case reaches `format` and **overflows the stack**, aborting the process. That abort is the guard working: before the fix, no test reached that depth. |

The findings-1/2/4/5 rows are the evidence that the five review findings are
fixed by the code rather than by prose, and the `--check` row is the evidence
that the exact comparison is now pinned **through the binary**, not only
through the helper. The last row is also the only one where a mutation is
caught by a *stack overflow* rather than an assertion: the tests report a
raised budget before they would parse the pathological source, which is why
that check is present.

## Gates

All four were re-run after the fixes, in the order `AGENTS.md` §3.4 requires.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff (exit 0) |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings (`Finished dev profile`) |
| `cargo test --all-targets` | **363 passed, 0 failed, 0 ignored** |
| `cargo test` (adds doc-tests) | pass, 363 passed, 0 failed; Doc-tests 0 |
| `./rbops/verify.sh phase-015` | **NOT RUN — `rbops/` does not exist in this checkout** |

The 363 reconciles: 353 before this round, +10 new `edge_*` tests in
`tests/formatter_test.rs`, +0 elsewhere. `tests/formatter_test.rs` itself is
**61 passed, 0 failed, 0 ignored**.

The previous revision's "(323 after run 1, +5 this resume)" was wrong on the
`+5`; the run-2 delta is 12 and the run-3 delta is 10.

### Honest note on the fourth gate

`rbops/` is absent from the working tree (`ls rbops/` →
`No such file or directory`); `.opencode/` is likewise absent. The prompt
states the invoking pipeline lives elsewhere and must not be inspected, so
`./rbops/verify.sh` was never executed and **no claim is made about its
output**. Its content is **unknown to this run**, not assumed-passing. If it
enforces a rule the other three gates do not, only that rule is unverified.

The three runnable gates were each executed directly in this tree, in the order
`AGENTS.md` §3.4 requires, and passed as tabulated above.

## Invariants touched

- None of the language-surface invariants in `phases/INVARIANTS.md` are
  touched. No `TokenKind`, `Value` variant, `Error` variant, keyword, or
  grammar production is added, removed, or renamed. `.rb` remains the source
  extension. Both code commits touched `src/formatter.rs` (run 2 added
  `give back` pair rendering and block-tail comment handling); the
  verification pass that corrected this report changed **no source file at
  all** — only this document.
- `redblue::formatter::needs_reformat` is a new `pub` function (run 1). This
  is an addition, not a change: `rb format --check` behaviour is the CLI
  surface, and the comparison semantics it encodes are unchanged in intent —
  only the `trim()` that defeated them is gone.

## Known gaps / follow-ups

- `modules/MathUtils.rb` cannot be parsed, so it cannot be formatted. This is
  pre-existing and unrelated to the formatter: the file uses
  `constant PI to 3.14159`, which is not grammar. Confirmed pre-existing by
  this resume — the file is untouched by phase-015, and the version from
  `27d4aeb` (pre-phase-015) fails with the identical error. The formatter
  correctly *rejects* it rather than guessing. → F1 in `FINDINGS.md`.
- The corpus used by `corpus()` covers `examples/` and `tests/` only. It does
  not include `modules/`, because `modules/MathUtils.rb` does not parse and
  would fail the corpus-wide property tests for a reason that is not the
  formatter's. Deliberately **not** worked around by skipping unparseable
  files in `corpus()` — that would hide exactly the failure worth surfacing.
  The change is one string once F1 is fixed. → F2.
- `SPEC.md` and `docs/GRAMMAR.md` document `for … from … to … [by …]` and the
  relational operators, none of which the parser builds. Six branches of
  `src/formatter.rs` are consequently unreachable and are now pinned by 5
  tests that record the current behaviour instead of asserting dead arms.
  → F5.
- Grouping-parentheses preservation is covered for the operator levels the
  formatter special-cases. A future operator added to the grammar must be
  added to `format_keeps_grouping_for_unary_and_every_operator_level`, or
  that test's table needs extending — nothing enforces the link.
- `rb format` prints to stdout rather than rewriting in place, so
  `rb format foo.rb > foo.rb` truncates the file first. → F3.
