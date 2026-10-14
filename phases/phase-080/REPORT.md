# Phase 080 — Make `items at N` and `length of X` real, or strike them from SPEC.md

## Reproduction (before any change)

```
$ printf 'set items to [1,2,3]\nset first to items at 0\nsay first\n' > t1.rb
$ ./target/debug/rb run t1.rb
Error: AnalyzerError: Unknown variable 'at'; did you mean 'as'?
  --> t1.rb:2:20

$ printf 'set items to [1,2,3]\nset count to length of items\nsay count\n' > t2.rb
$ ./target/debug/rb run t2.rb
Error: AnalyzerError: Unknown variable 'of'; did you mean 'if'?
  --> t2.rb:2:21

$ ./target/debug/rb format t1.rb
set items to [1, 2, 3]
set first to items        <- three statements where SPEC.md documents one
at
0
say first

$ ./target/debug/rb lint t1.rb
(no output, exit 0)        <- a file rb cannot run, reported as clean
```

The finding reproduced exactly on `main`. `set first to items at 0` parsed as
**three** statements (`set first to items`, `at`, `0`), not as a parse error —
which is why `rb format` printed them back as three lines and why `rb lint`, which
reads the same tree, had nothing to complain about.

## Decision on the three documented lines, one at a time

| SPEC.md line | As written | Decision |
|---|---|---|
| § List Operations, `set first to items at 0` | `Unknown variable 'at'` | **Implemented.** Prints `1` for `[1,2,3]`. |
| § List Operations, `set last to items at -1` | `Unknown variable 'at'` | **Implemented.** Prints `3` — the last element, which is what the line says and what `items[-1]` has always answered. |
| § List Operations, `set count to length of items` | `Unknown variable 'of'` | **Implemented.** Prints `3`. |

None was struck. All three now run and print the documented value, so no SPEC.md
correction was needed for them; § List Operations was extended to say precisely
what each spelling means and how it is told apart from the variable `at`.

## What changed

Round 2 changed these files on top of the round-1 table below. Both tables are
kept: the first is what phase-080 did, the second is what the round-2 review
sent back for.

### Round 2

| File | What |
|---|---|
| `src/parser.rs` | `Parser::parse_worded_operand` reads the operand of both worded forms at the additive level; `Parser::kind_starts_operand` / `is_operand_start` are the table that decides it; `at_index_ahead` and the `length of` primary ask that table instead of the wider one |
| `src/linter.rs` | the analyzer append moved from `lint()` into `lint_program()`, so both entry points share one step |
| `src/lsp.rs` | `diagnostics` no longer runs the analyzer and returns early on its refusal, so an editor gets the same findings `rb lint` does |
| `bootstrap/compiler.rb` | the worded operand is read at level 3; `is_operand_start` is stage 2's copy of the operand table; `parse_primary` refuses `might fail` deliberately, beside the refusal it already had for a function literal |
| `docs/GRAMMAR.md` | § 5.1/§ 5.2 name the operand `additive`, state the line rule, and precedence rows 9 and 10 split `[...]` (Left) from `at` (Right) |
| `SPEC.md` | § List Operations: the index is the arithmetic after `at`, a comparison is not part of it, and neither form takes an operand from the next line |
| `tests/index_words_test.rs` | three tests: the comparison behind `at`, the comparison behind `length of`, and the line rule |
| `tests/linter_test.rs` | one fixture corrected (`show(2)`), one new test for the two entry points |
| `tests/bootstrap_selfhost_test.rs` | two new tests and the corpus-size guards 321 → 323, 312 → 314 |
| `tests/common/generator.rs`, `corpus/lists-0027..0028.{rb,expected}` | two corpus programs, generated not hand-listed |
| `README.md` | the corpus count CI pins: 367 → 369 |
| `phases/phase-080/{REPORT,FINDINGS}.md` | this section, and § 3 / § 4 rewritten for the narrowed operand and the deliberate stage-2 refusal |

### Round 1

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +102 | `items at N` as a postfix arm on `Expr::Index`; `length of X` as a primary producing the `length(X)` call; `Parser::at_index_ahead` and `Parser::length_of_ahead` decide the disambiguation |
| `src/linter.rs` | +35 −11 | `lint()` asks the analyzer whether `rb` could run the file and reports its refusal when the linter had no finding of its own; `syntax_error` → `stage_error(prefix, …)` so the stage is named |
| `src/formatter.rs` | +5 −3 | the comment claiming `object at index` is not syntax the parser accepts was false after the change; corrected, and the canonical bracketed spelling documented |
| `bootstrap/compiler.rb` | +79 | the **stage 2 compiler written in Redblue** learned both forms, so the fixed-point tests still hold — no shim, no Rust fast path. `is_expression_start` also gained the two entries (`nothing`, `mightfail`) it was missing; see "Round 1 fixes" below |
| `SPEC.md` | +29 | § List Operations: what each spelling means, what it is equal to, how it is distinguished from the name `at`, and how to step twice |
| `docs/GRAMMAR.md` | +12 −5 | § 5.1 `length 'of' expression` as a primary, § 5.2 `postfix 'at' expression`, precedence table rows 8 and 9; the stale `postfix 'of' expression // generic type` line is gone |
| `tests/index_words_test.rs` | +719 | 19 new tests (new file) |
| `tests/common/generator.rs` | +15 | six new corpus programs in the `lists` family, generated not hand-listed |
| `corpus/lists-0021..0026.{rb,expected}` | +12 files | six corpus programs, each diffed on both engines |
| `tests/bootstrap_selfhost_test.rs` | +86 −10 | the corpus-size guards: 315 → 321, 306 → 312 (counts only; the guards still fail at 322 and 313), plus `edge_nothing_heads_an_expression_on_both_engines` |
| `tests/linter_test.rs` | +69 −1 | one fixture corrected, plus `edge_a_warning_does_not_hide_the_analyzer_s_refusal` |
| `tests/interpolation_test.rs` | +9 −1 | **one fixture corrected** — see "Existing tests touched" below |
| `README.md` | +1 −1 | the corpus count CI pins: 361 → 367 |

### Design decisions, stated

- **Both forms reuse existing expression kinds.** `items at N` is
  `Expr::Index`, exactly what `items[N]` is; `length of X` is
  `Expr::Call { name: "length", args: [X] }`, exactly what `length(X)` is. There
  is no new node, so there is one set of bounds, one set of error messages, one
  bytecode instruction, and no second implementation to drift. Both engines and
  the bootstrap compiler therefore agree by construction, and
  `tests/index_words_test.rs` asserts the agreement rather than assuming it
  (`the_word_form_and_the_bracket_form_agree_on_every_position`,
  `the_word_form_and_the_call_form_agree`).
- **`at` and `length` are not reserved and did not become reserved.** `at` is an
  index only when an operand follows it **on the same line**; `length of` is the
  worded form only as the two words together on the same line. `set at to 0`,
  `say at`, `items at at`, `length([1,2,3])` and a program that binds its own
  `length` all keep working — asserted, not assumed.
- **The index operand is the arithmetic after `at`.** `items at n + 1` indexes by
  `n + 1`, which is the reading an English-like language needs, and a comparison
  is not part of the index — `items at 0 is 10` is `items at 0` compared with
  `10`. The consequence — a second `at` belongs to the index, so `xs at 1 at 0` is
  `xs at (1 at 0)` and fails `Cannot index non-list` — is documented in SPEC.md
  and GRAMMAR.md and pinned by a test, rather than left to be discovered. Round 1
  got this wrong in the other direction: the operand was the *whole* expression,
  so `is`, `and` and `or` were swallowed. See "Round 2 fixes" § 1.
- **Neither worded form reaches across a newline.** `xs at` at the end of a line
  is the variable `at` and the analyzer refuses it; `length of` at the end of a
  line is a syntax error where it is written. The two answers differ because `at`
  is an ordinary name and `length of` is not, and both are pinned.
- **`rb lint` merges the analyzer's refusal into its report; it never
  substitutes it.** The analyzer always runs and its refusal is **appended** to
  the linter's own findings, so a file `rb run` refuses exits 1 whether or not
  the tree walk had something of its own to say. Both of the obvious alternatives
  are wrong and were rejected here:
  - *Consult the analyzer only when the report is otherwise empty* — what the
    first cut did. That made the pairing conditional on the very thing that
    makes a file dirty: `set u to unknown_name` with `u` unread produced one
    warning and no error, so `rb lint` exited **0** on a file `rb run` refuses.
    The diagnostic vanished the moment the file had a second, unrelated
    complaint. Pinned by `edge_a_warning_does_not_hide_the_analyzer_s_refusal`.
  - *Return the analyzer's refusal in place of the report* — which deletes the
    linter's own findings on every file the analyzer refuses, including the one
    `edge_extends_of_an_undeclared_parent_is_still_a_plain_read` exists to pin.
  - *Put the append in `lint()` only* — what round 1 did, and what the round-2
    review found. `lint_program` is what `crate::lsp::diagnostics` calls, so the
    append has to live there to reach every entry point. See "Round 2 fixes" § 3.
- **`rb format` canonicalises `items at 0` to `items[0]`.** Both spell the same
  `Expr::Index`, and one canonical spelling is the formatter's job. The property
  that matters — statement count never changes — is asserted on the *reparsed*
  formatted text, plus idempotence.

### Existing tests touched

`tests/linter_test.rs::edge_a_parameter_that_hides_an_outer_variable_is_reported`
used the source `show with 2`. There is no `with` syntax — the line parses as
three expressions and the analyzer refuses it with `Unknown variable 'with'` —
so the fixture was a program `rb` cannot run, and it only produced the one
warning the test asserts because `rb lint` stopped at the syntax tree. With this
phase's change it reports the refusal too, so the test would have been asserting
a rule it could no longer reach in isolation. Round 2 changed the fixture to
`show(2)`, the same correction made below to `greet("world")`. **No assertion was
removed**: the test still asserts the same warning, and now also asserts the
fixture produces no lint *error*, which is what keeps it a runnable program.

`tests/linter_test.rs::function_parameter_is_not_a_false_positive` used the
source `greet with "world"`. There is no `with` syntax: the fixture is refused by
the analyzer with `Unknown variable 'with'`, so it is a program `rb` cannot run —
it only produced no lint error because `rb lint` stopped at the syntax tree. With
this phase's change it reports the refusal and never reaches the rule the test
exists for. The fixture is now `greet("world")`. **No assertion was changed**:
the test still asserts `every_message(source).is_empty()` for a function whose
parameter is never read, which is exactly what it asserted before.

`tests/interpolation_test.rs::edge_the_linter_sees_a_name_used_only_inside_braces`
used the source `set name to "x"` / `say "hi {name2}"`, with `name2` bound
nowhere. The fixture is refused by the analyzer with `Unknown variable 'name2'`,
so it is a program `rb` cannot run, and its own `assert!(errors.is_empty(), "…
must be valid")` passed only because `rb lint` asked the analyzer whether the
file ran *only* when the tree walk had found nothing — and this file had a
warning. Under the merge described above that pairing hole is closed, so the
fixture has to be a program that runs. It is now
`set name to "x"` / `set name2 to "y"` / `say "hi {name2}"`. **No assertion was
changed**: the test still asserts the same two warning lists, and `name` is still
the name nothing reads.

`tests/bootstrap_selfhost_test.rs` carries exact corpus-size guards whose own
failure message is "a family has been added or removed and the counts below are
not the whole corpus any more". Six corpus programs were added, so `315 → 321` and
`306 → 312`. The guards still fail at the next number.

## Round 1 fixes

Two defects the round-1 review found in this phase's own diff, both fixed here
and both pinned by tests that fail without the fix.

1. **`bootstrap/compiler.rb::is_expression_start` was missing two entries.**
   That function is stage 2's copy of `Parser::kind_starts_expression`
   (`src/parser.rs:1778`), and the two tables decide the same question — may this
   token head an expression — for `give back`, for the `at` of a worded index,
   and for the operand of `length of`. `Nothing` was in the Rust table and not in
   the Redblue one, so `say xs at nothing` was one `say` of one `index` under
   stage 1 — **213 bytes** — and a `say xs` followed by two stray statements
   under stage 2 — **246 bytes**. Both runs succeeded and reported nothing; the
   disagreement was only ever visible in the bytes, and no corpus program wrote
   `at nothing` in it. `give back nothing` and `length of nothing` were wrong the
   same way, the second one refused outright. `MightFail` was missing too;
   `might fail` is *not* yet a stage-2 primary (`parse_primary` refuses it), so
   the entry changed nothing for the better — it gave stage 2's worded index a
   token its primary chain could not read. Fixed in `bootstrap/compiler.rb`,
   pinned by
   `tests/bootstrap_selfhost_test.rs::edge_nothing_heads_an_expression_on_both_engines`
   (five programs: the three that failed, plus two controls that never consult
   the table and must keep agreeing). **Superseded in part by round 2**, which
   gives the worded forms their own table — see "Round 2 fixes" § 1 and § 4.

2. **`lint()` consulted the analyzer only when it had nothing else to say.** The
   pairing "`rb run` refuses ⇒ `rb lint` reports" therefore held only for files
   the linter found nothing wrong with, which is the one condition under which a
   reader is most likely to act on the verdict. `set u to unknown_name` with `u`
   unread produced one warning, no error, and **exit 0**. Fixed by always running
   the analyzer and **appending** its refusal, never substituting it — see "Design
   decisions, stated". Pinned by
   `tests/linter_test.rs::edge_a_warning_does_not_hide_the_analyzer_s_refusal`,
   which asserts the warning still comes out *and* the refusal does, through the
   binary as well as the API. **Superseded in part by round 2**, which moved the
   append out of `lint()` entirely — see "Round 2 fixes" § 3.

## Round 2 fixes

Six defects the round-2 review found, all in this phase's own diff. One was a
BLOCKER, four MAJOR, one MINOR; all six are fixed and pinned by tests that fail
without the fix. No gate was weakened, no test deleted, no `#[ignore]` and no
`allow(` added.

### 1. BLOCKER — `at` and `length of` swallowed the comparison (`src/parser.rs`)

`xs at 0 is 10` parsed as `xs at (0 is 10)` and `length of xs is 3` as
`length(xs is 3)`, because the operand of both worded forms was read with
`parse_expression()`, the whole precedence chain. Every comparison, conjunction
and disjunction after `at` or `of` therefore became part of the **operand**: an
index by the answer of a comparison, which is a boolean, so the program failed at
run time with

```
$ ./target/debug/rb run t.rb          # t.rb: `say xs at 0 is 10`
Error: RuntimeError: Index must be a number
$ ./target/debug/rb run t2.rb         # t2.rb: `say length of xs is 3`
Error: RuntimeError: length requires a list or text
```

while the identical lines written `xs[0] is 10` and `length(xs) is 3` answered
`yes`. Two spellings of one operation, disagreeing about which expression they
are part of, decided by a precedence rule no reader would guess — and turning
working programs into runtime failures rather than refusals, which is the worst
failure shape available.

**The fix, and the level it uses.** The operand of both forms is now read at the
**additive** level — `Parser::parse_worded_operand` → `parse_addition()`, and
level 3 in the stage-2 compiler. That is the level below every comparison
(`docs/GRAMMAR.md` § 5.5), so `at n + 1` and `of "ab" + "cd"` still read the whole
arithmetic the writer meant, `is`/`and`/`or` stay outside the operand, and the
nesting budget is spent exactly as before (200 levels is still a spanned
diagnostic, not a stack overflow).

`at` is still read as an index only when an operand follows it on the same line,
so `set at to 0`, `say at` and `items at at` are untouched, and the second `at`
of `xs at 1 at 0` still belongs to the index — the right-nesting documented in
SPEC.md and now given its own row in the precedence table.

**Both engines.** `bootstrap/compiler.rb` learned the same level and its own
`is_operand_start` table, arm for arm with `Parser::kind_starts_operand`. Stage 1
and stage 2 are byte-identical on every program in this section, which is the
only evidence that says so — a class of defect where both engines make the *same*
mistake is invisible to a byte-identity test, so the tests assert the worded and
bracketed spellings agree **within one program** as well.

### 2. MAJOR — an linter fixture that `rb` cannot run (`tests/linter_test.rs`)

`edge_a_parameter_that_hides_an_outer_variable_is_reported` used the source
`show with 2`. There is no `with` syntax: the line parses as three expressions and
the analyzer refuses it with `Unknown variable 'with'`, so the fixture was a
program `rb` cannot run — and now that `rb lint` reports the refusal, the test's
own warning assertion was one edit away from being about something else entirely.
It is now `show(2)`, the same correction the round-1 fix made to `greet with
"world"`. **No assertion was weakened**: the test still asserts the same warning,
and now also asserts the fixture produces no lint *error*, so a future
unrunnable fixture fails here rather than silently changing what the test covers.

### 3. MAJOR — `rb lint` and `rb diagnostics` disagreed (`src/linter.rs`, `src/lsp.rs`)

`lint()` appended the analyzer's refusal; `lint_program()` — the one
`crate::lsp::diagnostics` calls, because an editor has the tree already — walked
the tree and stopped. So on every file the analyzer refuses, `rb lint` reported
the linter's findings **and** the refusal, while `rb diagnostics` reported the
refusal **alone** and discarded the findings. The REPORT's claim that "`rb lint`
reports the refusal" was true of one entry point and not the other.

The append now lives in `lint_program`, which is the single place the analyzer is
asked, and `lint` delegates to it. `diagnostics` no longer runs the analyzer and
returns early on its refusal, so an editor shows the blocking error *and* the
rest. Nothing is dropped and nothing is duplicated; the linter's tree walk
produces warnings only, so the blocking error is still first among the errors.
Pinned by `tests/linter_test.rs::edge_the_two_lint_entry_points_report_the_same_file_the_same_way`,
which asserts the two APIs agree message for message *and* states what they must
contain — "they match" can be satisfied by two empty answers.

### 4. MAJOR — stage 2's table named a primary stage 2 does not have (`bootstrap/compiler.rb`)

`is_expression_start` returned yes for `mightfail` while `parse_primary` refused
it, so `xs at might fail f()` triggered the word index in both engines: stage 1
built `index(might fail f())` and stage 2 refused. Code this phase added was
deciding, from a table with no arm behind it, whether a program was well formed.

Fixed by the operand table (the worded operand is level 3, and `might fail` is
not a primary at that level, so `at` is the name it has always been and both
engines agree), by a deliberate `mightfail` arm in `parse_primary` that refuses in
a sentence naming which runner *does* compile it — exactly as the `to` arm already
refuses a function literal — and by a test that pins the disagreement as a
**refusal** rather than a mistranslation. `FINDINGS.md` § 4 records what is still
missing: stage 2 cannot compile the guard, and implementing it is a later phase.

### 5. MAJOR — the precedence table claimed an associativity the parser does not have (`docs/GRAMMAR.md`)

Row 9 read `property access, call, index ([...] and at) | Left`. `xs[1][0]` is
left, `xs at 1 at 0` is **not**: the worded index is right-nested, because the
operand is everything the index reads. An implementer following the table gets
`((xs at 1) at 0)` and a different runtime error. Row 9 is split: row 9 is
`xs[...]`, row 10 is `xs at ...` with the associativity, the reason and a pointer
to § 5.2. The grammar in § 5.1/§ 5.2 was corrected the same way — both worded
forms take an *additive*, not an expression, which is the change finding 1
requires and which the grammar did not state.

### 6. MINOR — the line rule was unpinned, and the two forms answered differently

`length of` with its operand on the next line is a parser error (the newline is
not an operand); `xs at` split across lines is two statements, because `at` is an
ordinary name and the analyzer refuses the second one. The asymmetry is forced —
`at` **cannot** be an error without reserving the word — and untested either way.

**Decided: neither worded form reaches across a newline.** `xs at` ends the line
and `length of` is refused where it is written, both pinned by
`tests/index_words_test.rs::edge_neither_worded_form_takes_an_operand_from_the_next_line`,
which asserts both answers rather than one, and states in SPEC.md and
`docs/GRAMMAR.md` that they differ and why.

### Round-2 tests

| Test | File | Edge class |
|---|---|---|
| `edge_a_comparison_after_a_worded_index_compares_the_element` | `tests/index_words_test.rs` | precedence — the BLOCKER: every case asserted twice, worded against the spelling it stands for |
| `edge_a_comparison_after_a_worded_length_measures_first` | `tests/index_words_test.rs` | precedence — the same for `length of`, including arithmetic still inside the operand |
| `edge_neither_worded_form_takes_an_operand_from_the_next_line` | `tests/index_words_test.rs` | malformed_input / boundary — the line rule, both answers |
| `edge_the_two_lint_entry_points_report_the_same_file_the_same_way` | `tests/linter_test.rs` | consistency — `rb lint` and `rb diagnostics` on one file, plus what each must contain |
| `edge_a_comparison_after_a_worded_index_is_the_same_program_on_both_engines` | `tests/bootstrap_selfhost_test.rs` | stage1/stage2 — byte-identity for the fixed level, plus the arithmetic controls that were identical before |
| `edge_a_construct_stage_two_cannot_compile_is_refused_rather_than_mistranslated` | `tests/bootstrap_selfhost_test.rs` | stage1/stage2 — the `might fail` gap is a refusal, not a mistranslation |

Corpus: `lists-0027` (a comparison, a conjunction and a disjunction after `at`,
each written in both spellings in one program) and `lists-0028` (`length of xs is 3`
against `length(xs) is 3`, `xs at n + 1` against `xs[n + 1]`). Both run on both
engines and are diffed by `tests/differential_test.rs`; the count guards in
`tests/bootstrap_selfhost_test.rs` move 321 → 323 and 312 → 314, and the README
pin 367 → 369.

## Tests added

`tests/index_words_test.rs`, 22 `#[test]` functions, 11 of them named `edge_*` —
19 from round 1 and the three round-2 tests listed above.

| Test | Edge class covered |
|---|---|
| `set_first_to_items_at_zero_prints_the_first_element` | SPEC.md:319 — the happy path, the thing that was broken |
| `set_last_to_items_at_negative_one_prints_the_last_element` | SPEC.md:320 — the happy path |
| `set_count_to_length_of_items_prints_the_element_count` | SPEC.md:327 — the happy path |
| `the_word_form_and_the_bracket_form_agree_on_every_position` | boundary / singleton — `0`, `len-1`, `-1`, `-3`, and a one-element list where `0` and `-1` are the same position |
| `the_word_form_and_the_call_form_agree` | empty, unicode — `[]`, `[1,2,3]`, `["héllo","日本語"]`, `"abc"` |
| `edge_each_documented_form_is_exactly_one_statement` | **the defect** — statement count on the parsed `Program`, never by matching an error message |
| `the_word_forms_bind_the_value_the_documentation_promises` | the stray statements were silent, not cosmetic |
| `edge_an_out_of_bounds_index_is_a_spanned_runtime_error_in_both_spellings` | out_of_bounds — `len`, `999`, `-4` on `[1,2,3]`; `0`, `-1`, `999` on `[]`; each spelled twice and the two must name the same failure |
| `a_word_form_applied_to_the_wrong_type_is_a_clean_runtime_error` | type_mismatch / malformed_input — `length of 5`, `at "0"`, `at nothing`, `length of` with no operand |
| `at_is_still_a_usable_variable_name` | the disambiguation: `set at to 7`, `items at at`, bare `say at` |
| `length_alone_is_still_the_builtin_and_can_be_shadowed` | the other disambiguation, including shadowing |
| `edge_formatting_never_changes_the_statement_count` | the second tool — statement count of the *reparsed* formatted text, plus idempotence |
| `formatting_a_word_form_keeps_what_the_program_prints` | format must not change the values either |
| `edge_each_documented_form_pairs_a_run_result_with_a_lint_result` | each form × {runs, lints clean} |
| `a_file_rb_cannot_lex_parse_or_analyze_has_a_lint_diagnostic` | asserts a **failure is produced**: lexer, parser and analyzer refusals each yield a diagnostic, and the diagnostic names the stage |
| `edge_the_worded_index_operand_is_read_as_a_whole_expression` | precedence + the chaining limit, pinned so it is a decision |
| `edge_length_of_reads_one_operand_and_refuses_a_missing_one` | malformed_input — `length of` with nothing to measure; `items at` with nothing to index |
| `edge_length_of_measures_elements_and_bytes_under_the_right_spelling` | unicode — elements vs bytes, empty text |
| `edge_a_worded_index_operand_that_nests_past_the_limit_is_refused` | resource_limit — 200 levels of nesting is a spanned `Parser` error, not a stack overflow |

Corpus: `lists-0021` (the documented lines, both spellings, on one program),
`lists-0022` (`length of` over a list, the empty list and multi-byte text),
`lists-0023` (`at 3` — past the end), `lists-0024` (`at -1` on the empty list),
`lists-0025` (`xs at at` — the disambiguation), `lists-0026` (`at 0.5` — not a
whole number). Each runs on **both** engines and the answers are diffed by
`tests/differential_test.rs`; the `.expected` files are read, never rewritten, by
that runner.

Two more `#[test]` functions were added outside that file, for the round-1 fixes:

| Test | Edge class covered |
|---|---|
| `bootstrap_selfhost_test::edge_nothing_heads_an_expression_on_both_engines` | the stage-1/stage-2 disagreement on an operand the table omitted — `at nothing`, `give back nothing`, `length of nothing`, plus the two controls that never consult the table. Each program was verified to fail (different bytes, or a refusal) against the pre-fix stage 2 |
| `linter_test::edge_a_warning_does_not_hide_the_analyzer_s_refusal` | the lint/run pairing under a file that triggers a warning **and** an analyzer refusal at once, through the API and through the `rb lint` binary's exit code |

### Edge-case matrix (AGENTS.md § 3.2)

| Row | Status |
|---|---|
| empty | covered — `length of []`, `[] at 0`, `[] at -1`, `[] at 999` |
| singleton | covered — `[42] at 0` and `[42] at -1` are the one position, asserted against the bracket form |
| boundary | covered — `at 0`, `at len-1`, `at -1`, `at -len`, `at n + 1`, `at 3 - 1` |
| precedence | covered (round 2) — the operand is an additive, so `at n + 1` and `length of "ab" + "cd"` stay inside it while `is`, `and` and `or` stay outside; every case asserted twice, worded against the spelling it stands for (`edge_a_comparison_after_a_worded_index_compares_the_element`, `edge_a_comparison_after_a_worded_length_measures_first`), and on both engines |
| out_of_bounds | covered — `len`, `999`, `-4`, and the three empty-list positions, each in both spellings |
| type_mismatch | covered — `length of 5`, `at "0"`, `at nothing`, `at yes`, `at` a record |
| numeric_boundary | covered — `at 0.5` (fractional index refused, not truncated) and corpus `lists-0026`; `-0.0`/`±1e308` are unchanged `[]` behaviour already pinned by `tests/index_bounds_test.rs` and re-used by the word form because it is the same node |
| unicode | covered — `length of ["héllo","日本語","x"]` is 3, `length of "héllo"` is 6 bytes, `length of ""` is 0 |
| nesting_recursion | covered — the operand parses through the same nesting budget as every other expression (`edge_a_worded_index_operand_that_nests_past_the_limit_is_refused`); two-deep reads are corpus `lists-0021`/`0022` via the `set … to … at …` bindings, and the single-expression limit is the documented chaining rule |
| duplicate_missing_keys | N/A — this phase adds no record operation and touches no record path. `set first to items at 0` reads a **list**; `src/runtime.rs:index` refuses a record with `Cannot index non-list`, which is the existing answer and is unchanged |
| malformed_input | covered — `length of` with no operand, `items at` with no operand, 200-deep nesting, and the **line rule** in both directions: `length of` with its operand on the next line is refused where it is written, `xs at` with its index on the next line is the name `at` and is refused by the analyzer (`edge_neither_worded_form_takes_an_operand_from_the_next_line`); a BOM/CRLF/non-UTF8 source never reaches the new arms because it fails earlier, in the lexer, which `a_file_rb_cannot_lex_parse_or_analyze_has_a_lint_diagnostic` covers |
| resource_limit | covered — nesting budget for the operand; the operand is evaluated, not compiled per use, so there is no new loop or recursion to bound |

## Gates

Re-run after the round-2 fixes, by hand:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** |
| `cargo clippy --all-targets -- -D warnings` | **pass** — no `allow(` added, no warning |
| `cargo test --all-targets` | **pass** — 1537 passed, 0 failed, 0 ignored, across 57 targets |
| `./rbops/verify.sh phase-080` | **not run — the file does not exist in this checkout.** `rbops/` is absent from the working tree (`ls rbops` → `No such file or directory`), and the pipeline that dispatches this phase lives outside the project. Not claimed as passing. |

Run by hand, because they are the checks CI performs and `verify.sh` was not
available:

| Check | Result |
|---|---|
| `cargo test --test differential_test` | pass — 82 passed, 0 failed; both engines agree on all 369 programs |
| `cargo test --test bootstrap_selfhost_test` | pass — 23 passed, 0 failed; **stage 2 and stage 3 are byte-identical to stage 1 on every one of the new corpus programs** |
| `cargo test --test bootstrap_ship_test` | pass — 16 passed, 0 failed |
| `examples/*.rb` and `modules/*.rb` | every file exits 0 |
| README corpus-count pin | pass — 369 programs, README says 369 |
| `rb run` / `rb format` / `rb lint` on the original repro | `set first to items at 0` prints `1`; `set count to length of items` prints `3`; `rb format` emits `set first to items[0]` — one statement; `rb lint` on a file `rb` cannot run now prints `Analysis error: …` and exits 1 — including when the file also has a warning, which was the round-1 defect |
| the round-2 repros | `say items at 0 is 1` prints `yes` under **both** engines and their `.rbc` files are byte-identical; `rb diagnostics` on a file the analyzer refuses now returns the same two findings `rb lint` prints, in the same order |

## Invariants touched

- None of the listed language invariants changed. `.rb` is still the extension;
  `to … end` / `if … end` / `for … end` still close with `end`; `set x to <expr>`
  is still assignment; `say` still prints; `Value` and `Error` variants are
  untouched.
- **New: `at` is a postfix index operator, `length of` is a primary.** Both are
  grammar additions, not re-openings of the design: neither is a reserved word,
  neither adds a statement form, and the bracketed and called spellings still work
  and are asserted to be the same operation.
- **New: the operand of both worded forms is an additive, not an expression.**
  Round 1 shipped the whole expression, which meant `xs at 0 is 10` indexed by a
  boolean. `docs/GRAMMAR.md` § 5.1/§ 5.2 and SPEC.md § List Operations now state
  the level, and the precedence table gives `xs[...]` and `xs at ...` separate
  rows because they do not share an associativity.
- **Changed: `rb lint` always runs the analyzer and merges its refusal into the
  report.** It could already report `Syntax error:`. A file the analyzer refuses
  used to exit 0 with no output; it now exits 1 with a diagnostic, *whatever else
  the linter found* — the analyzer's verdict is appended, never substituted, so no
  file that previously produced a finding produces fewer.
- **Changed: `rb diagnostics` reports what `rb lint` reports.** The two entry
  points disagreed about every file the analyzer refuses — one gave the linter's
  findings plus the refusal, the other gave the refusal alone. The analyzer is
  now asked in one place (`lint_program`), so there is no way to reach the linter
  without its verdict.
- **Changed: `rb format` canonicalises `items at 0` to `items[0]`.** The
  statement count is unchanged and the program's output is unchanged; only the
  spelling of one index is normalised.
- **Changed: `rb compile`'s stage-2 compiler understands both forms.** Without
  this the fixed-point tests fail, and the rule in AGENTS.md § 7 forbids solving
  that with a Rust-only fast path.

## Known gaps / follow-ups

- `rb lint` cannot report a failure that only occurs when the program runs —
  calling a function that does not exist, indexing past the end. A linter reads
  the tree and does not run the program. → FINDINGS.md § 1
- `xs at 1 at 0` does not step twice; the second `at` belongs to the index. This
  is documented in SPEC.md and GRAMMAR.md and pinned by a test, but it is a
  limitation a reader may trip over. → FINDINGS.md § 3
- `might fail <call>` is still not a stage-2 construct: `bootstrap/compiler.rb`'s
  `parse_primary` refuses it, so a program that uses one compiles under `rb` and
  is refused by `rb run bootstrap/compiler.rb`. Round 2 made that refusal
  deliberate and named (`'might fail <call>' is not compiled yet`), moved the
  worded operand to a level that cannot read the guard at all, and pinned the
  disagreement with a test — so the gap is a refusal a reader is told about, not a
  silent mistranslation. Teaching stage 2 the guard is still a later phase.
  → FINDINGS.md § 4
- `./rbops/verify.sh phase-080` was not run: `rbops/` is not in this checkout.
  The four `cargo` gates and every CI-specific check listed above were run by
  hand instead, and none of them is a substitute for `verify.sh`.