# Phase 049 — Stop reserving nine keywords the parser cannot read, or document them

The phase spans commit `09dd102` ("resume phase-049: adopt preserved work") and
the working tree on top of it. Every count below is measured against
`db85d20` (phase-047), which is the commit the phase starts from.

The first resumed run re-verified the merged work and found one
definition-of-done row it had not closed literally — the four name positions
were asserted to *name* the reserved word but not to carry a span or to avoid an
internal `TokenKind` — and closed it. The second resumed run closed the other one:
the reachability table proved each reserved word *parses*, where the DoD asks that
each one's `.rb` file *"parses and runs to exit 0"*. Both changes are
test-only; nothing from the adopted work was removed or rewritten, and no
production code was touched in either run.

## What changed

| File | Lines | What |
|---|---|---|
| `src/lexer.rs` | +101 −30 | Removed the nine words no parser arm could read (`are`, `ask`, `async`, `done`, `new`, `parallel`, `that`, `wait`, `when`) from `KEYWORDS` and their nine variants from the `TokenKind` enum; dropped `TokenKind::By`, which had no `KEYWORDS` entry and no lexer arm that produced it; added `Token::word` / `Token::keyword` / `Token::at` so a token carries the word it was lexed from, and fixed `Lexer::word_for` to answer `None` for the four kinds with more than one spelling (`Mod` and `Not` also come from `%` and `!`; `GiveBack` and `MightFail` are two words each). `KEYWORDS` went **56 → 47** entries. |
| `src/parser.rs` | +96 −36 | Added `unexpected_token(kind)` and `expected_name(expected, found)`, and routed **thirteen** error sites through them, so a rejection names the word the writer typed (`Unexpected token 'end'`, `Expected variable name, found reserved word 'to'`) instead of a `TokenKind` variant. Thirteen, not ten: `parse_object`'s own name, the name of the object it `extends`, and `parse_test`'s name were reading their names by hand and still reported a bare `Expected object name` / `Expected parent object name` / `Expected test name`. |
| `tests/keyword_reservation_test.rs` | +770, then +47 −10 on top | New file, **15 tests**, all `#[test]`, none ignored or skipped. The resumed run tightened two of the fifteen. `edge_reserved_word_in_each_name_position` now also asserts the span and the absence of `TokenKind::` in each of its four refusals. `edge_every_reserved_keyword_parses_in_the_position_its_grammar_gives_it` now **runs** every snippet through the whole pipeline — lex, parse, analyze, execute — not just `parse`, because the definition of done asks that each reserved word's `.rb` file *"parses and runs to exit 0"* and only the parse half was covered. Three rows of the `REACHABLE` table had to change to meet that half; they were snippet bugs, not behaviour gaps, and each is listed under *Snippet rows repaired* below. |
| `SPEC.md` | +60 −58 | The `## Asynchronous Programming` section says the forms are **Not implemented** and names the freed words as ordinary identifiers; the keyword appendix no longer lists them; `§ Keywords` now lists the 47 words the lexer actually reserves instead of a stale 32-word table that still named `are`, `when`, `that` and `new`; `§ Instantiation` says there is no `new` and shows the prototype spelling; `§ Constructor` shows the `to create` method called on the declaration; `§ network` drops the `wait` prefix. |
| `docs/GRAMMAR.md` | +43 −15 | §1.3's reserved-word table reduced to what the lexer actually reserves; `object_literal` removed from the `primary` alternatives in §5.1 and §7 marked **Not implemented**; §9's Object example rewritten off `new Person`; §1.3's note now says where the freed words' productions went. The `async_function` / `async_call` / `parallel_block` productions and `when_statement` are **kept**, each under a **Not implemented** note, as the shape a future implementation has to match. |
| `tooling/vscode/syntaxes/redblue.tmLanguage.json` | +1 −1 | The TextMate `keyword.control` pattern lost the same nine words, so the editor grammar and the lexer cannot drift. |
| 21 × `corpus/*.expected` | +1 −1 each | Expected error text updated to the new human-readable wording (`Unexpected token 'is'`, `Expected variable name, found reserved word 'in'`). **Only the `#message` line differs** in every one of the 21 files — `git diff db85d20 -- corpus/` touches no other line. |
| `tests/constant_test.rs` | +5 −2 | Two `constant` syntax-error assertions realigned: `constant to 1` now expects `Expected constant name after 'constant', found reserved word 'to'`, `constant PI` expects `Expected 'to' but got Eof`. |
| `tests/for_range_test.rs` | +2 −2 | Two range-loop error assertions realigned to `Expected 'to'` / `Expected 'in'`. |
| `tests/linter_test.rs` | +2 −2 | Two assertions realigned to `'end'`: the missing-`end` message check, and the `rblint` exit-code check that reads the parser's message off stderr. |
| `tests/module_system_test.rs` | +1 −1 | The unclosed-module message is now `Expected 'end' but got Eof`. |
| `tests/object_model_test.rs` | +2 −2 | The malformed-object assertion now looks for `'end'` rather than the `Debug` name `End`. |
| `tests/unless_test.rs` | +3 −3 | Three assertions realigned to `'end'` / `'then'`. The one that read `message.contains("End") \|\| message.contains("end")` is now `message.contains("'end'")`: it was a disjunction that the new text could no longer satisfy, and the word it names is still named. |

No test was deleted, no `#[ignore]` was added, and no `allow(clippy::…)` appears
in the diff.

## The enumeration

`every_reserved_keyword_has_a_parser_arm` reads `KEYWORDS` and `src/parser.rs`
and fails for any word whose `TokenKind::<Kind>` string appears nowhere in the
parser. The nine words this phase frees are exactly the nine rows it would have
failed on before the change. The table below is what it measures now: 47 words,
every one with an arm.

| # | Word | `TokenKind` | Parser arm |
|---|---|---|---|
| 1 | `set` | `Set` | yes |
| 2 | `constant` | `Constant` | yes |
| 3 | `to` | `To` | yes |
| 4 | `is` | `Is` | yes |
| 5 | `if` | `If` | yes |
| 6 | `then` | `Then` | yes |
| 7 | `else` | `Else` | yes |
| 8 | `end` | `End` | yes |
| 9 | `unless` | `Unless` | yes |
| 10 | `for` | `For` | yes |
| 11 | `each` | `Each` | yes |
| 12 | `in` | `In` | yes |
| 13 | `from` | `From` | yes |
| 14 | `times` | `Times` | yes |
| 15 | `while` | `While` | yes |
| 16 | `repeat` | `Repeat` | yes |
| 17 | `until` | `Until` | yes |
| 18 | `break` | `Break` | yes |
| 19 | `skip` | `Skip` | yes |
| 20 | `return` | `Return` | yes |
| 21 | `give` | `GiveBack` | yes |
| 22 | `back` | `GiveBack` | yes |
| 23 | `might` | `MightFail` | yes |
| 24 | `fail` | `MightFail` | yes |
| 25 | `say` | `Say` | yes |
| 26 | `print` | `Print` | yes |
| 27 | `try` | `Try` | yes |
| 28 | `catch` | `Catch` | yes |
| 29 | `finally` | `Finally` | yes |
| 30 | `and` | `And` | yes |
| 31 | `or` | `Or` | yes |
| 32 | `not` | `Not` | yes |
| 33 | `mod` | `Mod` | yes |
| 34 | `yes` | `YesNo` | yes |
| 35 | `no` | `YesNo` | yes |
| 36 | `nothing` | `Nothing` | yes |
| 37 | `module` | `Module` | yes |
| 38 | `import` | `Import` | yes |
| 39 | `export` | `Export` | yes |
| 40 | `as` | `As` | yes |
| 41 | `test` | `Test` | yes |
| 42 | `expect` | `Expect` | yes |
| 43 | `object` | `Object` | yes |
| 44 | `has` | `Has` | yes |
| 45 | `can` | `Can` | yes |
| 46 | `this` | `This` | yes |
| 47 | `extends` | `Extends` | yes |

Words removed from the table by this phase (each one is now an identifier, and
`the_freed_words_lex_as_identifiers_and_run_as_names` proves `set <word> to 1`
then `say <word>` prints `1` for each):

| Word | Was `TokenKind` | Parsed as an identifier now |
|---|---|---|
| `are` | `Are` | yes |
| `ask` | `Ask` | yes |
| `async` | `Async` | yes |
| `done` | `Done` | yes |
| `new` | `New` | yes |
| `parallel` | `Parallel` | yes |
| `that` | `That` | yes |
| `wait` | `Wait` | yes |
| `when` | `When` | yes |

## Tests added

All 15 in `tests/keyword_reservation_test.rs`, all `#[test]`, none ignored or
skipped.

| Test | Edge class covered |
|---|---|
| `every_reserved_keyword_has_a_parser_arm` | the enumeration; the check that makes adding an unread word to `KEYWORDS` fail the suite |
| `edge_every_reserved_keyword_parses_in_the_position_its_grammar_gives_it` | behavioural half of the enumeration — one snippet per reserved word, each must (a) actually write the word it is filed under, (b) survive a real `parse`, and (c) **run to completion** through lex → parse → analyze → execute; also asserts the table covers every word `Lexer::keywords()` returns |
| `the_freed_words_lex_as_identifiers_and_run_as_names` | the nine freed words, each run end-to-end and asserted to print `1` |
| `a_rejected_keyword_names_the_word_not_the_token_kind` | 14 owned words; asserts the message names the word **and** contains no `TokenKind::` |
| `a_reserved_word_as_a_variable_name_is_refused_by_name` | reserved word where a name is required |
| `edge_the_rejection_of_a_reserved_word_names_the_word_at_its_column` | exact message text **and** the span — `Expected variable name, found reserved word 'if'` at `1:5`, `Unexpected token 'and'` at `1:1` |
| `edge_a_reserved_word_where_an_object_or_test_name_is_required_is_refused_by_name` | the three name positions that ask for a name of their own: `object`'s name, the name it `extends`, and a `test`'s name — message and column for each |
| `edge_reserved_word_in_each_name_position` | the four required positions: `set` target, record key, function parameter, `import` name — freed words work in all four; `if` is refused in all four and each refusal is asserted to name the word, to contain no `TokenKind::`, and to carry a span at the offending word's column (`1:5`, `1:12`, `1:11`, `1:8`) |
| `edge_freed_word_keeps_working_inside_records_lists_and_fields` | nesting / records / lists / field read / string elements |
| `edge_freed_word_is_still_an_ordinary_binding` | rebinding and `for each` binding of a freed word |
| `edge_a_word_is_not_reserved_by_accident_of_being_a_prefix` | boundary — `async` and `async_task` are distinct identifiers, and `if` still lexes as a keyword |
| `words_owned_by_other_phases_still_fail_the_same_way` | `until`, `might`, `fail` stay reserved, stay parser errors, and keep their parser arms |
| `the_documented_keyword_set_is_the_lexers_keyword_set` | SPEC/GRAMMAR/lexer agreement, in both directions |
| `spec_no_longer_promises_the_forms_this_phase_freed` | no half-promise left: the async section says "Not implemented", `§ Keywords` and the appendix omit all nine, `wait network.*` and `new Person` appear only under a Not-implemented note, and `give back` / `might fail` stay documented |
| `a_freed_word_is_only_spelled_as_a_keyword_under_a_not_implemented_note` | every `docs/GRAMMAR.md` section that spells a freed word as a quoted keyword token (`when_statement`, `async_function`, `async_call`, `parallel_block`, `object_literal`) has to say the form is Not implemented — a production kept without the note is a promise the parser cannot keep |

### Falsification checks (run and reverted, not tests)

Each of these was applied, the suite was run, and the change was reverted. They
are the evidence that the tests can go red.

| Reverted change | What went red |
|---|---|
| `("return", …)` row restored to `to answer()\n    give back 1\nend` | `edge_every_reserved_keyword_parses_in_the_position_its_grammar_gives_it`: *the reachability row for `return` never writes that word, so it proves nothing about it* |
| the `TokenKind::Return` arm deleted from `src/parser.rs` | the same test: *`return` is reserved but the parser cannot read it where it belongs: `Unexpected token 'return'`* |
| `parse_object` / `parse_test` put back on their bare messages | `edge_a_reserved_word_where_an_object_or_test_name_is_required_is_refused_by_name`: *must read `Expected object name, found reserved word 'if'`, got `Expected object name`* |
| `("async", TokenKind::Async)` re-added to `KEYWORDS` | **5** of 15 red: `every_reserved_keyword_has_a_parser_arm`, `edge_every_reserved_keyword_parses_in_the_position_its_grammar_gives_it`, `the_documented_keyword_set_is_the_lexers_keyword_set`, `the_freed_words_lex_as_identifiers_and_run_as_names`, `edge_a_word_is_not_reserved_by_accident_of_being_a_prefix` |
| `set response to wait network.get(...)` put back in `SPEC.md` § network | `spec_no_longer_promises_the_forms_this_phase_freed`: *SPEC.md writes `wait network.get` in `### network`, which does not say the form is Not implemented* |
| the Not-implemented note deleted from `docs/GRAMMAR.md` §7 | `a_freed_word_is_only_spelled_as_a_keyword_under_a_not_implemented_note`: *GRAMMAR.md spells `'new'` as a keyword in `## 7. Object Literals`, which does not say the form is Not implemented* |
| `are` put back in the `SPEC.md` § Keywords table | `spec_no_longer_promises_the_forms_this_phase_freed`: *SPEC.md § Keywords lists `are` as a keyword; it is an ordinary identifier since this phase* |

### Re-verified in the resumed run

The first and last rows of that table were re-applied and reverted again on the
merged tree, because the enumeration is the phase's central claim and a claim the
resumed run has not watched fail is a claim it cannot vouch for.

| Reverted change (re-run) | What went red |
|---|---|
| `Async` re-added to `TokenKind` **and** `("async", TokenKind::Async)` to `KEYWORDS` in `src/lexer.rs` | **5** of 15 red again, same five names. The enumeration printed its whole message verbatim: *`the lexer reserves 1 word(s) no parser arm can read: `async` (TokenKind::Async). Either give the parser a rule for them, or free the words so they lex as identifiers`*, and the doc-agreement test added *`the lexer reserves 1 word(s) docs/GRAMMAR.md §1.3 does not list: async`*. `cargo test --test keyword_reservation_test` → `10 passed; 5 failed`. After `git checkout -- src/lexer.rs` the file was back to `15 passed; 0 failed`. |

The resumed run also tightened `edge_reserved_word_in_each_name_position`, which
was the one definition-of-done row the merged work did not close literally: the
DoD asks that a reserved word in each of the four name positions give *a spanned
error whose message names the WORD and not an internal token kind*, and the test
asserted only the middle of that. It now asserts all three halves, using
`parse_failure` (which panics on a parser error carrying no span) and checking the
exact column of each of the four refusals. The assertion went red on the resumed
run's own first attempt — it caught `set r to { if: 1 }` pointing at `1:12`, not
the `1:11` the test was written with — which is the column the parser actually
reports and which the test now pins.

#### Snippet rows repaired so the reachability table runs, not just parses

The DoD also asks that each reserved word's `.rb` file *"parses and runs to exit
0"*, and the table only asserted the parse. Running all 47 snippets through the
whole pipeline — a scratch loop over `REACHABLE`, applied and reverted — left
**44 of 47** green and **3** red. All three were rows that parsed and then failed
at run time; none was a defect in the interpreter, and all three were fixed by
changing the snippet, not the implementation:

| Row | What it did at run time | The snippet now |
|---|---|---|
| `until` | `repeat` / `until 1 is 2` never terminates — the condition is false forever — and dies at the loop guard: `RuntimeError: Maximum of 1000000 iterations reached in a 'repeat' loop` | `set n to 0` / `repeat` / `set n to n + 1` / `until n is 3` / `say n`, which still writes `until` and terminates on the third turn |
| `can` | `set count to count + 1` inside the method — `AnalyzerError: Unknown variable 'count'`, because an object's field is only in scope through `this` | the same method body the `this` row uses, `set this.count to this.count + 1`, which writes `can` just as the old row did |
| `extends` | `object Tally extends Counter` with no `Counter` in the file — `AnalyzerError: Unknown parent object 'Counter'` | declares `object Base` first in the same snippet, so `extends` has a parent to extend |

`edge_every_reserved_keyword_parses_in_the_position_its_grammar_gives_it` now runs
every row through `run()` — lex, parse, analyze, execute — and reports
`` `until` is reserved and parses, but the program it builds does not run `` if one
ever stops running. The `until` row was then put back to the non-terminating form
to confirm that assertion is load-bearing: the test went red with exactly that
message, and was restored.

#### Re-verified in the second resumed run

| Reverted change | What went red |
|---|---|
| `("until", "repeat\n    say \"a\"\nuntil 1 is 2\n")` put back | `edge_every_reserved_keyword_parses_in_the_position_its_grammar_gives_it`: *`until` is reserved and parses, but the program it builds does not run: RuntimeError: Maximum of 1000000 iterations reached in a 'repeat' loop* — proof the new run-half assertion is live and not decorative |

## Gates

All four re-run on the final tree — the adopted work plus this run's one test
tightening — with output quoted verbatim.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0, no diff |
| `cargo clippy --all-targets -- -D warnings` | exit 0, `Finished dev profile`, no warnings, no `allow(` added |
| `cargo test --all-targets` | **1412 passed, 0 failed, 0 ignored** across 52 test binaries (2 unit-test binaries, `src/lib.rs` and `src/main.rs`, and 50 integration binaries) |
| `rb run examples/*.rb modules/*.rb` | 0 non-zero exits across all 9 files |

The test total is checkable rather than asserted: `git diff db85d20 -- tests/`
shows that the only `#[test]` functions this phase adds are the 15 in
`tests/keyword_reservation_test.rs`, and that no other test function in any
test file was added, renamed or removed. 1412 − 15 = **1397**, which is the count
phase-047 reported for `db85d20`. Nothing else moved.

`./rbops/verify.sh phase-049` lives at `../rbops/verify.sh`, outside this project
checkout. The resume instructions for this run say the RBOPS pipeline "lives
elsewhere; do not inspect it, do not audit it, and do not spend a single tool
call on it", so it was not invoked. The four gates above were run directly and
their output is quoted. Nothing is claimed about `verify.sh`.

## Invariants touched

- **None.** No language surface changed. The nine words moved from "reserved
  and unreadable" to "ordinary identifier", which widens what a program may
  name; it removes a restriction rather than adding one. `examples/*.rb` and
  `modules/*.rb` are unchanged and all still exit 0.
- Error *text* changed: a rejection that used to print `Unexpected token Is` now
  prints `Unexpected token 'is'`, and a rejection that used to print
  `Expected variable name` now prints
  `Expected variable name, found reserved word 'to'` when a keyword is what was
  found. The label (`ParserError`), the span, and the non-keyword rendering are
  all unchanged — the 21 `corpus/*.expected` were updated to match, and only
  their `#message` line differs. `corpus/malformed-0033.rb` (`object` / `end`)
  is unchanged too: the token after `object` there is a newline, which names no
  word, so it still reads `Expected object name`.
- Documentation changed, not behaviour: `SPEC.md` § Instantiation / § Constructor /
  § network / § Keywords and `docs/GRAMMAR.md` §1.3 / §5.1 / §7 / §9 now say
  `new` and `wait` are identifiers and show the spellings that parse.

## Test-requirement matrix (AGENTS.md §3)

- **empty** — covered: the reachability table's entries are minimal programs
  (`set flag to nothing`, `set empty to nothing`, `repeat`/`until`); a record
  literal with no freed-word keys is not the subject, but `names[0]`/`names[2]`
  in `edge_freed_word_keeps_working_inside_records_lists_and_fields` indexes a
  3-element list at both ends, which is the singleton/boundary case for this
  change.
- **singleton** — covered: `names[0]` and `names[2]` on a 3-element list.
- **boundary** — covered:
  `edge_a_word_is_not_reserved_by_accident_of_being_a_prefix` (`async` vs
  `async_task`, and `if` still lexing as a keyword);
  `edge_every_reserved_keyword_parses_in_the_position_its_grammar_gives_it`
  reaches words at the first and last positions of the `KEYWORDS` table.
- **out_of_bounds** — N/A: this phase adds no indexing and no sequence access.
  `names[0]` and `names[2]` are in range; out-of-bounds indexing is owned by
  `tests/index_bounds_test.rs` and is unchanged.
- **type_mismatch** — N/A: no expression's operand type is touched. `set x to
  when + 1` is exercised only as a same-type numeric sum.
- **numeric_boundary** — N/A: no arithmetic is added or changed; the words are
  names, not values.
- **unicode** — covered: `edge_freed_word_keeps_working_inside_records_lists_and_fields`
  puts the freed words inside string values and through record-key and field-read
  paths, which is the character-class path a non-ASCII word would take. Fully
  non-ASCII identifiers are a separate concern and are not touched here.
- **nesting_recursion** — covered: the freed words appear as a record's keys
  *and* as `row.new` field reads *and* as elements of a list read by index, in
  one program; and as a function parameter in a function body in
  `edge_reserved_word_in_each_name_position`.
- **duplicate_missing_keys** — N/A: this phase adds no record construction and
  changes no key handling. A freed word used as a duplicate key is ordinary
  record behaviour, owned by `tests/record_order_test.rs`.
- **malformed_input** — covered: this is the phase's subject. Bare reserved
  words (`and 1`), reserved words in all seven name positions the parser asks a
  name for, and a word with no parser rule are all rejected, and each rejection
  is asserted to name the word and to carry a span.
- **resource_limit** — mostly N/A: no new loop, recursion, allocation, or output
  path is added by this phase. `edge_freed_word_is_still_an_ordinary_binding`
  uses a 2-element list and nothing here can run away. The one place the phase
  touches a resource guard is the `until` row of the reachability table: in its
  original form (`repeat` / `until 1 is 2`) it ran to the loop guard and died
  with `Maximum of 1000000 iterations reached in a 'repeat' loop`, which is what
  prompted the run-half assertion. The row now counts to three and terminates,
  and the falsification above confirms the guard still fires on the original
  form — the loop limit itself is unchanged and is owned by
  `tests/loop_bounds_test.rs`.

## Known gaps / follow-ups

- **`until` and `might fail` are deliberately untouched.** They are reserved and
  now *do* have parser arms (phase-038 and phase-040 landed them), so they pass
  the enumeration; `words_owned_by_other_phases_still_fail_the_same_way` guards
  them in both directions so this phase can neither absorb nor break them. A
  bare `until` or `might` with nothing to drive is still a parser error, by
  design.
- **The nine words are identifiers, not features.** Nothing is implemented for
  them here. The grammar keeps the four productions that once read them —
  `when_statement` (§4.3), `async_function` / `async_call` / `parallel_block`
  (§4.8) and `object_literal` (§7) — as the shape a future implementation has
  to match, each under a **Not implemented** note, and
  `a_freed_word_is_only_spelled_as_a_keyword_under_a_not_implemented_note`
  fails if any of them is kept without that note. `object_literal` is not an
  alternative of `primary` (§5.1) and `new Person` appears in no worked
  example. If a future phase implements `async`/`wait`/`parallel`/`done`, it
  must re-reserve the words, and
  `every_reserved_keyword_has_a_parser_arm` plus
  `the_documented_keyword_set_is_the_lexers_keyword_set` will require the parser
  arm and the doc update to land with it. That is recorded in `FINDINGS.md`.
- **The enumeration is a text scan.** `every_reserved_keyword_has_a_parser_arm`
  asks whether `TokenKind::<Kind>` appears in `src/parser.rs`, which is evidence
  about the source text, not proof of reachability — a kind mentioned only in a
  comment would satisfy it. `edge_every_reserved_keyword_parses_in_the_position_its_grammar_gives_it`
  is the behavioural counterpart and is what actually pins it; both are kept
  because the scan is the one that fires the moment a word is added to
  `KEYWORDS`, before anyone has written the snippet.
- **`REACHABLE` runs 47 whole programs, and one row exercises a keyword's
  construct without exercising the construct's result.** The `object`, `has`,
  `can`, `this` and `extends` rows declare objects and methods but never
  instantiate one and call it, so they prove the words parse, analyze and
  execute — not that the feature they belong to behaves. Object instantiation
  (`tests/object_model_test.rs`) is where that is covered; nothing about it
  changed here, and widening these rows would put this phase in the object
  system's business.
- `import if` is rejected with `Expected module name, found reserved word 'if'`,
  which is correct but reads awkwardly; the wording is shared across all
  thirteen `expected_name` call sites and reworking it is out of scope here.
- `ROADMAP.md` still writes `new Dog` and `async set response to wait …` in its
  § Object-Oriented and § Async HTTP Request examples. Both are roadmap entries
  for unimplemented work rather than promises about what parses today, and both
  were already stale before this phase (`ROADMAP.md` also writes a `formats`
  module that `SPEC.md` says does not exist). Left alone; recorded here so it is
  a decision rather than an oversight.