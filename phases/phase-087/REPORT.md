# Phase 087 — Count characters in `length`, not UTF-8 bytes

## What changed

`length` over text answered `s.len()` — the `String`'s UTF-8 **byte** count —
so every text outside ASCII was two, three or four times its real length, and
said so with a plausible-looking number. The correct character count sat one
file away in `src/stdlib.rs` behind an arm no caller reached.

The fix is two lines of behaviour and one deletion. The rest of this diff is
the tests, the corpus expectations that recorded the byte count, and the prose
that documented it.

| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +20 −14 | `"length" \| "len"` on `Value::Text` now answers `s.chars().count()`; comment rewritten to say *why* a character is a scalar value and not a glyph |
| `src/stdlib.rs` | +10 −14 | the unreachable `"length" =>` arm is **deleted**, not repaired, with a comment saying where the one implementation lives |
| `tests/text_length_characters_test.rs` | +420 −0 | **new** — 18 tests, every row through both engines |
| `tests/test_text.rb` | +41 −17 | six tests that pinned the byte count now pin characters *and* still assert the byte count through `length(bytes.from_text(s))` |
| `tests/stdlib_module_names_test.rs` | +34 −20 | `edge_length_counts_bytes_in_both_arms` → `edge_length_counts_characters_in_one_arm`; the dotted `text.length` row moved `6` → `5` |
| `tests/index_words_test.rs` | +5 −5 | `length of "héllo"` moved `6` → `5` |
| `tests/integration_test.rb` | +3 −1 | the all-stages unicode test moved `18` → `10`, with the byte count named |
| `corpus/unicode-0001..0008.expected`, `corpus/text-ops-0011/0019.expected`, `corpus/lists-0022.expected` | +11 −11 | recorded byte counts → character counts |
| `tests/doc_blocks_{fragments,known_fail,runs}.txt` | +9 −9 | `SPEC.md` line numbers shifted by the two lines the prose edit added |
| `README.md`, `SPEC.md` | +15 −13 | `length` documented as counting characters; SPEC.md states the character/byte/grapheme distinction |
| `tests/unreviewed_work_ack_test.rs` | +1 −0 | acknowledgement entry for the `resume phase-087` WIP checkpoint commit, with its re-derivable numstat |
| `phases/phase-087/REPORT.md` | new | this file |

Nothing was removed from the language: the byte count is still reachable, and
`length(bytes.from_text(s))` answers it (`length(bytes.from_text("héllo"))` is
`6`). What moved is which number the word `length` names.

## Reproduction, before the fix

Run against the binary built from `HEAD~1` (the WIP checkpoint carried the tests
but not yet a rebuilt binary, so the binary was still the byte-counting one):

```
$ ./target/debug/rb run repro.rb
6          # say text.length("日本")   -- two characters, six bytes
2          # say length("é")          -- one character, two bytes
3          # say text.length("abc")   -- the ASCII row, correct by accident
```

After the fix, same file, rebuilt:

```
$ ./target/debug/rb run repro.rb
2
1
3
```

The `é` rows are the source of a discrepancy in the phase's own wording, and it
is worth being exact about it. The evidence line in the prompt says
`length("é")` printed `2` and calls that wrong; the definition-of-done line says
`length("é")` should print `2`. Both are right about different spellings, and
both are now pinned:

| spelling | code points | UTF-8 bytes | before | after |
|---|---|---|---|---|
| `"é"` precomposed U+00E9 | 1 | 2 | 2 | **1** |
| `"e"` + U+0301 combining acute | 2 | 3 | 3 | **2** |

`tests/text_length_characters_test.rs` (`text_length_counts_latin1_and_emoji_as_one_character_each`)
asserts the precomposed `1`;
`combining_marks_are_counted_as_two_code_points` asserts the decomposed `2`. So
whichever `é` the definition-of-done was looking at, that exact program prints
`2`.

## The two-engine point

`length` is dispatched from two places — the tree-walking VM and the bytecode VM
— and both reach `runtime::builtin`. Every row in the new test file runs through
both, and `both_engines_answer_every_row_of_the_table_identically` compares the
two engines *to each other* rather than restating each expectation twice, so a
divergence between the dispatchers is a diff in one assertion rather than two
rows that were meant to match.

## Tests added

`tests/text_length_characters_test.rs` — 18 `#[test]` functions, all new:

| Test | Edge class covered |
|---|---|
| `text_length_counts_cjk_characters_not_bytes` | unicode / 3-byte CJK |
| `text_length_counts_latin1_and_emoji_as_one_character_each` | unicode / 2-byte Latin-1 + 4-byte astral |
| `list_length_is_unchanged` | boundary — the list arm did not move (`3`, `0`, `1`) |
| `edge_text_length_of_empty_text_is_zero` | **empty** |
| `edge_text_length_of_a_singleton_is_one` | **singleton** |
| `edge_text_length_ascii_is_unchanged` | **boundary / back-compat** — ASCII cannot tell the two counts apart |
| `combining_marks_are_counted_as_two_code_points` | **unicode / combining** — the deliberate choice, recorded |
| `edge_text_length_right_to_left_counts_characters` | unicode / RTL |
| `edge_text_length_of_a_list_of_zero_elements_is_zero` | empty, for the list argument kind |
| `edge_text_containing_nul_counts_the_characters_around_it` | unicode / escapes / NUL, built three ways |
| `edge_text_length_at_the_last_character_has_no_off_by_one` | **boundary** — last character, 3-byte and 4-byte |
| `edge_text_length_mixed_script_text_is_the_sum_of_its_characters` | unicode / mixed widths |
| `edge_text_length_of_a_long_mixed_string_is_exact` | **resource** — 300 characters / 800 bytes, grown by a loop, plus the 297 case |
| `both_engines_answer_every_row_of_the_table_identically` | differential, tree-walker vs bytecode VM |
| `length_of_a_record_is_still_a_clean_runtime_error` | **type mismatch + asserts a failure** |
| `length_of_a_number_is_still_a_clean_runtime_error` | **type mismatch + asserts a failure** |
| `edge_length_over_text_has_exactly_one_implementation` | the dead arm cannot come back |
| `edge_no_other_builtin_moved_when_length_changed` | 25-row registry walk, both engines |

Red-green evidence, both directions, done in the tree and then reverted:

* Restoring `s.len()` in `src/runtime.rs` turns **9 of the 18** red — exactly the
  unicode rows (`日本` 6≠2, `e`+U+0301 3≠2, `a日b` 5≠3, `שלום` 8≠4, the
  mixed-script string 14≠7, the 300/800 loop 800≠300, `text.length("日")` 3≠1,
  `length("é")` 2≠1, `🎉` 4≠1). The empty, ASCII, singleton, list and both
  error rows stay green, because they were never byte/character-dependent.
* Re-adding the `"length" =>` arm to `src/stdlib.rs` turns
  `edge_length_over_text_has_exactly_one_implementation` red with
  `stdlib must not implement 'length' over text a second time`.

## AGENTS.md §3.2 edge-case matrix

| Row | Status |
|---|---|
| empty / zero / nothing | **covered** — `edge_text_length_of_empty_text_is_zero` (`""` → 0, both engines, both spellings); `edge_text_length_of_a_list_of_zero_elements_is_zero` for the list arm |
| singleton and boundary | **covered** — `edge_text_length_of_a_singleton_is_one`; `edge_text_length_at_the_last_character_has_no_off_by_one` pins index `len-1` with a 3-byte and a 4-byte final character; `list_length_is_unchanged` covers `length([7])` and `length([])` |
| out of bounds | **N/A** — `length` does not index. It returns a count and never reads at an index, so there is no index that can go out of bounds; there is no branch changed here that could take one. The list *indexing* rows are untouched code and are pinned by the pre-existing suite |
| type mismatch | **covered** — `length_of_a_record_is_still_a_clean_runtime_error` and `length_of_a_number_is_still_a_clean_runtime_error`: neither panics, neither coerces, both produce `length requires a list or text`, and both engines produce the same error |
| numeric boundary | **N/A** — `length` performs no arithmetic. It returns a `usize` widened to `f64`, and a count above 2^53 would need ~9 petabytes of text in memory, so no representable boundary is reachable. `edge_no_other_builtin_moved_when_length_changed` pins the arithmetic builtins (`abs`, `sqrt`, `pow`, `floor`, `ceil`) as unmoved |
| unicode and escapes | **covered** — 2-byte Latin-1 (`é`, `ü`), 3-byte CJK (`日本`, `世界`, `a日b`), 4-byte astral (`🎉`, `👩‍💻`), combining marks (`e`+U+0301), RTL (`שלום`), NUL (`\u0000a\u0000`), newline (`"a\nb"`), a 300-character mixed string, and the byte spellings (`bytes.from_text` / `bytes.text`) alongside |
| nesting and recursion | **N/A** — `length` is `O(n)` over a flat string's code points and holds no state, so there is no recursive call to bound and no scope to nest. The rows that exist in the pre-existing suite are untouched |
| duplicate and missing keys | **N/A** — no record is accepted at all; a record is one of the two type-mismatch rows above and is refused before any key is read |
| malformed input | **N/A** — a malformed program (unterminated text, unclosed `end`, stray token) is refused by the lexer or parser before any value reaches `length`, so no malformed input can reach the branch that changed. `length_of_a_record_is_still_a_clean_runtime_error` is the nearest reachable refusal and is covered |
| resource and state | **covered (the part that applies)** — `edge_text_length_of_a_long_mixed_string_is_exact` builds a 300-character string in a loop and asserts 300 and 800 exactly, plus the 297 case, so a size-dependent counting bug cannot hide. N/A: filesystem, permissions, call depth and the infinite-loop guard — `length` touches no file, no environment and no recursion |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test --all-targets` | **1766 passed, 0 failed, 0 ignored** |
| `cargo test --doc` | 2 passed, 0 failed |
| `./rbops/verify.sh phase-087` | **not run — `rbops/` is not in this checkout** |

On the fourth gate, stated plainly rather than claimed: `rbops/verify.sh` does
not exist in this working tree (`ls rbops` → `No such file or directory`, and
`git ls-files | grep rbops` is empty). The RBOPS pipeline lives outside the
project checkout. I ran the three gates that exist here and did not fabricate a
fourth result. The acknowledgement-list test
(`unreviewed_work_in_head_history_is_acknowledged`) that the pipeline would
otherwise catch is the one process gate that did bite during this phase, and it
is green: the `resume phase-087` WIP checkpoint commit is now on
`tests/unreviewed_work_ack_test.rs`'s `ACKNOWLEDGED` list with its re-derivable
numstat (`+546/-104`, 23 files, `+30/-28` under `src/`).

## Invariants touched

None. Checked against `phases/INVARIANTS.md` and AGENTS.md §2:

* `.rb` extension, `to … end`, `set x to <expr>`, `say` — untouched.
* The `Value` enum and the `Error` enum — untouched; the fix is inside an
  existing arm, producing the `Value::Number` that arm already produced.
* Trailing commas, `{interp}` — untouched.
* Every test in `tests/` still passes. Six pre-existing tests changed their
  expected number because they named the byte count as the contract
  (`edge_text_length_counts_bytes_not_characters` and five neighbours in
  `tests/test_text.rb`, plus rows in `index_words_test.rs`,
  `stdlib_module_names_test.rs` and `integration_test.rb`). This is the change
  the phase asked for, and it is not a weakening: each of those tests now
  asserts **both** numbers — `length(accented)` is 5 and
  `length(bytes.from_text(accented))` is still 6 — so the byte count is more
  tightly pinned after this phase than before it.
* `examples/*.rb` and `modules/*.rb` — all run. `hello.rb` → `Hello, World!`,
  `fizzbuzz.rb` → `5`, `test_arithmetic.rb` → `All arithmetic tests completed`,
  `files.rb` → `Done!`, `formats.rb` → `Demo complete!`, `random.rb` →
  `[green, red, blue]`, `time.rb` → `Time module demo complete!`, and both
  modules run clean. That is the condition under which the README and SPEC
  edits were made.

One behavioural change a reviewer should look at on purpose, recorded rather
than buried: **a grapheme is not a character.** `é` rendered from `e` + U+0301
is one glyph and answers 2, where a byte count said 3. Grapheme segmentation is
a different measurement needing UAX #29 tables plus join and stack handling, and
it would be a silent second answer to the same word. The choice is pinned by
`combining_marks_are_counted_as_two_code_points`, which says so in its comment.

## Known gaps / follow-ups

- `length` counts Unicode **scalar values**, so an unpaired surrogate cannot be
  represented at all (Rust `char` cannot hold one) and a string containing one
  in its source form is a lexer error rather than a length. Nothing to do.
- There is no `grapheme_count` builtin. If the language ever wants one it must
  be named differently from `length`; adding it under this name would be the
  exact defect this phase removes. → worth an auditor phase only if the
  roadmap asks for it.
- `unicode-segmentation` is already a dependency of the crate, so a future
  grapheme builtin costs nothing in build weight.
- `verify.sh` was not run. If it has a check beyond the four gates that this
  checkout can express, it has not been satisfied here, and that is stated
  rather than papered over.