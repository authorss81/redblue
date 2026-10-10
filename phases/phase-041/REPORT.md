# Phase 041 — Implement `text.split` and `text.join`, and pin which stdlib names are reachable dotted

## Reproduction (before any change)

The phase finding claims `split` and `join` do not exist in any spelling. **That
part is stale** — phase-014 (`27d4aeb`) added both, and they are reachable
today:

```
$ printf 'import text\nsay text.split("a,b", ",")\n' > /tmp/p.rb && rb run /tmp/p.rb
[a, b]
$ printf 'set p to split("a,b", ",")\nsay p\n' > /tmp/p.rb && rb run /tmp/p.rb
[a, b]
```

**The gap that does reproduce is the one the finding quotes SPEC.md for.** SPEC.md
`§ text` (`SPEC.md:1017-1018`) spells both calls with a `by` argument label, and
that program is refused before it ever runs:

```
$ printf 'set p to text.split("a,b", by ",")\nsay p\n' > /tmp/p.rb && rb run /tmp/p.rb
Error: AnalyzerError: Unknown variable 'by'
  --> /tmp/p.rb:1:1
1 | set p to text.split("a,b", by ",")
  | ^
EXIT=1
```

`by` is a positional marker in a range loop's step (`for each i from 0 to 6 by 2`,
`src/parser.rs:919-941`) and was never given a production in a call's argument
list, so `by` parsed as an ordinary variable read that nothing binds.

A second, smaller inconsistency the finding points at and that is **not** a defect:
`text.split(..)` works with **no `import text`**. `src/analyzer.rs:459-471`
accepts a method-call receiver that names a module. The import is optional, not
required — `text_split_reaches_dotted_without_an_import_and_that_is_the_rule`
pins that so it cannot drift silently.

## What changed

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +47 −0 | `Parser::at_argument_label` — recognises `by` as a call-argument label; one `if self.at_argument_label() { self.advance() }` in the argument loop of `parse_postfix` |
| `tests/text_split_join_test.rs` | +880 | 17 new tests (new file) |

Round 1 (review findings) amended the same two files — see **Round 1** below.

Nothing else changed. No production file other than the parser was touched; the
answer each case gives was already correct and is now pinned rather than changed.

### Why `by` is read positionally, not made a keyword

`by` must not join `KEYWORDS` (`src/lexer.rs:14-69`): that would refuse
`to can grow(by)` / `say by`, a parameter name the language has always allowed
and which `tests/bytecode_test.rs:614` asserts. So `by` is matched as a word, the
same way `at_range_step_marker` already does it, and the label is distinguished
from a variable by **what follows it**: a label is only a label when the next
token can begin an expression. `split(x, by ",")` labels the separator;
`grow(by)` passes a variable, because `)` begins no expression. This is the entire
disambiguation and it is two lines.

## Round 1 — review findings, all fixed

Four findings; the BLOCKER is a program that stopped parsing, and the two MAJORs
were untested rules rather than wrong values. All four are fixed in
`src/parser.rs` and `tests/text_split_join_test.rs`, and each fix has a test that
fails without it (proof by mutation, at the end of this section).

| # | Severity | Finding | Fix |
|---|---|---|---|
| 1 | BLOCKER | `at_argument_label` called every token but `)`/`,` an expression start, contradicting its own doc and `is_expression_start` | `Parser::kind_starts_expression` extracted from `is_expression_start`; the lookahead now asks that table instead of a two-token blacklist |
| 2 | MAJOR | the label arm sat in the generic `parse_postfix` loop, so *every* call accepted `by` although SPEC.md:1017-1018 defines it for `split`/`join` only | `Parser::callee_takes_argument_label` restricts the label to those two builtins; every other call reads `by` as the variable it has always been able to be |
| 3 | MAJOR | the lookahead skipped a `Newline` to decide "label", but the parse after `advance()` did not | `skip_newlines()` after the `advance()`, so the decision and the parse agree; a label and its value may be written on different lines |
| 4 | MINOR | `tests/text_split_join_test.rs` header claimed dotted `text.split` needs `import text`, contradicting the pinned rule | header corrected — the import is optional, and a bare `say text` is what is refused |

### Why finding 1 broke working programs

`split("a,b", by + ",")` is one expression that *uses* the variable `by`. The
blacklist read the `+` after `by` as "not `)`, not `,`, therefore a label",
dropped the word, and handed `+ ","` to `parse_expression` — which has no unary
`+`. A program that ran yesterday is a parse error today. The fix asks the same
table `is_expression_start` asks, so the two can no longer disagree about what
"can begin an expression" means, and the blacklist can never reappear here.

### Why the label is restricted to two names rather than left generic

A label any call accepts is a spelling that works everywhere and means nothing
anywhere but `split`/`join`: `pow(2, by 10)` would read as "2 to the power of the
variable `by`, with an extra `10`" instead of being what it looks like. Restricting
it is the direction that keeps SPEC.md and the parser telling the same story, and
it changes nothing that worked before — no spelling accepted a label until this
phase added one. Both sides are pinned, so the rule is a test rather than a claim:

* `the_by_label_is_read_for_split_and_join_and_for_no_other_call` — the four
  accepted spellings, `pow(2, by 10)` answering `8` (`1024` if the `by` were
  dropped), a user function refusing `scale(by 2)` with `Unknown variable 'by'`,
  and `by` as a whole argument still being the variable.
* `a_by_in_front_of_an_operator_is_a_variable_and_never_a_label` — `by + by`,
  `by + 1` and `by is "x"`, each pinned on a **value**, because a parser that
  dropped the `by` has nothing left to produce one.

### What finding 3 does and does not buy

A label and the value it labels may be written on different lines:
`text.split("a,b", by` ⏎ `",")` now runs on both engines. What is still refused —
and pinned as refused, so the rule cannot widen into "argument lists may wrap" —
is a newline anywhere *else* in the list, including before the `by` itself.

### Proof each fix is load-bearing

`src/parser.rs` was mutated three ways and `cargo test --test text_split_join_test`
re-run after each, the mutation reverted between runs:

| Mutation | Test that failed |
|---|---|
| restore the two-token blacklist in `at_argument_label` | `a_by_in_front_of_an_operator_is_a_variable_and_never_a_label` |
| make `callee_takes_argument_label` always true | `the_by_label_is_read_for_split_and_join_and_for_no_other_call` |
| drop `skip_newlines()` after the label `advance()` | `a_label_and_the_value_it_labels_may_be_written_on_different_lines` |

Three tests, three mutations, one failure each: no test passes for two reasons.

## Tests added

20 `#[test]` functions in `tests/text_split_join_test.rs`, 5 of them `edge_*`.
Every test goes through the full pipeline (lexer → parser → **analyzer** → VM),
because two of the findings are analyzer refusals and a helper that skipped the
analyzer could not fail on either.

| Test | Edge class covered |
|---|---|
| `table_every_split_case_answers_the_same_on_both_engines` | table: empty subject, subject == separator, separator absent, adjacent separators, 3-char separator, empty separator |
| `table_every_join_case_answers_the_same_on_both_engines` | table: empty list, one element, adjacent empty elements, 3-char separator, empty separator |
| `spec_md_text_section_runs_verbatim` | the four SPEC.md `§ text` lines, byte-for-byte |
| `reachability_table_holds_for_every_row` | dotted **and** flat for 14 stdlib names, each asserted on its own value |
| `dotted_and_flat_spellings_agree_for_every_reachable_builtin` | one builtin, two spellings, one function |
| `text_split_reaches_dotted_without_an_import_and_that_is_the_rule` | the dotted/import rule, and that a bare `say text` is still refused by name |
| `split_and_join_refuse_a_wrongly_typed_argument_and_name_the_builtin` | **type_mismatch**: 8 programs × both spellings, all caught Runtime errors naming the builtin, all with a span |
| `a_wrongly_typed_split_argument_is_caught_by_try_catch_in_a_redblue_program` | **failure channel**: `try`/`catch error` in a Redblue program, not a process abort |
| `edge_empty_subject_splits_to_one_empty_part` | **empty**: `""` → one empty part, not zero |
| `edge_empty_list_joins_to_empty_text` | **empty**: `[]` → `""`, and the one-element boundary |
| `edge_empty_separator_is_a_caught_error_and_join_with_one_still_concatenates` | **empty separator**, both directions, on both engines |
| `edge_subject_longer_than_the_separator_keeps_every_part` | **boundary**: separator at both ends, longer than subject, exactly one match |
| `edge_unicode_subject_splits_on_character_boundaries` | **unicode**: emoji, CJK, a multi-byte separator, a combining mark, and joining |
| `edge_a_number_is_where_a_text_is_wanted_is_refused_cleanly_on_both_engines` | **type_mismatch** through the CLI, dotted and flat, same message |
| `edge_a_record_where_a_list_is_wanted_is_refused_not_iterated` | record and number where text/list is wanted |
| `edge_nested_and_deep_subjects_do_not_recurse` | **resource**: 6000-char subject, 1000-element list |
| `the_bytecode_vm_answers_split_and_join_the_same_as_the_interpreter` | every row of both tables, compared value-for-value (error-for-error on the one row with no value) |
| `the_by_label_is_read_for_split_and_join_and_for_no_other_call` | **round 1, finding 2**: the four spellings that accept a label, `pow` and a user function refusing one, `by` as a whole argument |
| `a_by_in_front_of_an_operator_is_a_variable_and_never_a_label` | **round 1, finding 1**: `by + by`, `by + 1`, `by is "x"` — pinned on values, not on errors |
| `a_label_and_the_value_it_labels_may_be_written_on_different_lines` | **round 1, finding 3**: the label across two lines on both engines, and the newline placements that stay refused |

**The tables are the definition of done's list, one row each:** empty subject,
subject equal to the separator, separator absent, adjacent separators,
3-character separator, plus `text.split("", by ",")` and `text.join([], by ",")`.

**Byte-identical across engines:** every row of both tables runs through
`run_on_both_engines` / `refuse_on_both_engines`, which compare the raw stdout
bytes of `rb run` against `rb vm` over `rb compile`'s output, and assert both
against the expected text. For the rows that must be refused, both engines must
exit non-zero with the **same first line** of stderr and print nothing.

**Proof the tests bite:** with `src/parser.rs` stashed, 15 of the 17 fail; the 2
that pass are the ones that do not use `by`.

## Gates

### Round 1 (after the review findings)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test --all-targets` | **1201 passed, 0 failed, 0 ignored** (44 suites) |
| `./rbops/verify.sh phase-041` | **NOT RUN — the script does not exist in this checkout** |

`rbops/` is still absent from the working tree (`ls rbops/verify.sh` → `No such
file or directory`), and the brief forbids inspecting the pipeline that invoked
this phase. The three gates above were all re-run after the fixes and all three
are green; the fourth is reported as unrun rather than claimed. No gate was
weakened, no `#[ignore]`, `// skip` or `allow(clippy::)` was added, and no test
was deleted — the suite grew by three.

### Round 0 (as first written)

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test --all-targets` | 1198 passed, 0 failed, 0 ignored (includes the 39 in `tests/stdlib_modules_test.rs`) |
| `./rbops/verify.sh phase-041` | **NOT RUN — the script does not exist in this checkout** |

`examples/` and `modules/` — 8 files — were each run through `rb run`; all exit 0.

## The reachability table

Every name in `src/stdlib.rs: BUILTIN_NAMES` (66 names). **Dotted works for all
of them, so SPEC.md needs no amendment.** The deciding line for every dotted row
is the same: `src/stdlib.rs:524` `module_member_name`, which splits a qualified
name at the first `_` and accepts it when the prefix is in `MODULES` and the
remainder is in `BUILTIN_NAMES`; `src/stdlib.rs:541` `resolve` then maps it to
the bare builtin, which is what makes the two spellings *one* function rather
than two that look alike. `src/analyzer.rs:459-471` is what lets the module name
be a receiver with no `import`.

### The 25 module-qualified names — dotted `module.member` and flat `module_member`

Both spellings are live for all 25: `files_read` is registered as a global
(`src/stdlib.rs:113`) *and* resolves as `files.read`; the flat qualified form
works because `module_member_name` returns `None` for a name `BUILTIN_NAMES`
already holds (`src/stdlib.rs:515-521`).

| Registered (flat) | Dotted | Module |
|---|---|---|
| `list_map` | `list.map` | list |
| `files_read` | `files.read` | files |
| `files_write` | `files.write` | files |
| `files_append` | `files.append` | files |
| `files_exists` | `files.exists` | files |
| `files_lines` | `files.lines` | files |
| `files_delete` | `files.delete` | files |
| `files_copy` | `files.copy` | files |
| `files_rename` | `files.rename` | files |
| `time_now` | `time.now` | time |
| `time_sleep` | `time.sleep` | time |
| `time_format` | `time.format` | time |
| `time_unix` | `time.unix` | time |
| `json_parse` | `json.parse` | json |
| `json_stringify` | `json.stringify` | json |
| `csv_parse` | `csv.parse` | csv |
| `network_get` | `network.get` | network |
| `network_post` | `network.post` | network |
| `console_log` | `console.log` | console |
| `console_error` | `console.error` | console |
| `console_clear` | `console.clear` | console |
| `bytes_from_text` | `bytes.from_text` | bytes |
| `bytes_write` | `bytes.write` | bytes |
| `bytes_text` | `bytes.text` | bytes |
| `sys_argv` | `sys.argv` | sys |

### The 41 bare names — flat, and dotted through **every** module

`abs`, `floor`, `ceil`, `round`, `sqrt`, `pow`, `sin`, `cos`, `tan`, `log`,
`exp`, `uppercase`, `lowercase`, `trim`, **`split`**, **`join`**, `contains`,
`starts_with`, `ends_with`, `replace`, `length`, `push`, `append`, `pop`,
`shift`, `map`, `filter`, `reduce`, `is_number`, `is_text`, `is_list`,
`is_record`, `to_text`, `to_number`, `to_list`, `type_of`, `expect`, `assert`,
`random_number`, `random_choice`, `random_shuffle`.

Deciding line: `src/stdlib.rs:529` — the remainder of the split is looked up in
`BUILTIN_NAMES` with **no** check that it belongs to that module, so
`text.split` and `math.split` are the same `split`. A dotted spelling through the
*wrong* module therefore succeeds rather than being refused. That is the
accident the phase asked to make a documented property: it is now stated here and
pinned for the 14 names in `REACH`. Making it a refusal instead would be a
breaking change to a working spelling and is out of this phase's scope —
recorded in `FINDINGS.md`.

## Invariants touched

None. `src/parser.rs` gains one method and one guarded `advance()`; no existing
spelling changes meaning. `to can grow(by)` / `say by` still parse as a variable
(`tests/bytecode_test.rs:614` passes), `for each i from 0 to 6 by 2` still reads
its step, and `KEYWORDS` is untouched so `rb keywords` output is unchanged.

Round 1 does not touch an invariant either, and the direction of both fixes is
toward the language that already existed: a variable `by` in front of an operator
is a variable again (it was read as a label for one round), and a `by` label is
refused for calls other than `split`/`join`, which no program could have relied on
because no other call accepted one. The one shape that *is* new is the label
written across two lines, which previously did not parse.

## Test matrix rows

- **empty** — covered. Empty subject, empty list, empty separator, and the
  zero-match/`""` case for `join`.
- **singleton** — covered. `join(["only"], ",")`; a subject with no separator in
  it is one part.
- **boundary** — covered. Separator at index 0 and at the end
  (`split(",a,", ",")` → `["", "a", ""]`), separator longer than the subject,
  separator exactly matching, 3- and 4-character separators.
- **out_of_bounds** — N/A. `split` and `join` take no index. The nearest
  equivalent, indexing a split result, is pinned indirectly: every table row
  prints `length(parts)` alongside its parts, so a result that dropped or added
  an element fails.
- **type_mismatch** — covered. Number where text is wanted, record where text is
  wanted, text where a list is wanted, number where a list is wanted, a missing
  argument, and no arguments at all — each × dotted and flat, each a caught
  Runtime error naming the builtin.
- **numeric_boundary** — N/A. Neither function takes or returns a number. The
  numeric edge of the *language* (`0/0`, `1/0`, `±Inf`) is untouched by this
  change and is held by `tests/numeric_edge_test.rs` (23 tests, green).
- **unicode** — covered. Emoji (4-byte), CJK (3-byte), a multi-byte separator
  (U+2026) whose byte boundary falls inside a character, a combining mark
  (U+0301), and the join direction for each.
- **nesting_recursion** — N/A for nesting. Neither function recurses, so there is
  no recursion depth to vary; `edge_nested_and_deep_subjects_do_not_recurse`
  covers the resource row below instead. A subject that *is* a list of lists is
  refused rather than flattened, pinned in
  `edge_a_record_where_a_list_is_wanted_is_refused_not_iterated`.
- **duplicate_missing_keys** — N/A. Neither function takes a record. The record
  refusal is covered in the type_mismatch row instead.
- **malformed_input** — covered by construction: an unterminated string or an
  unclosed `)` never reaches the builtin at all, and the parser refuses them
  first (held by `tests/lexer_robustness_test.rs` and
  `tests/parser_hardening_test.rs`, both green). The `by` change touches only
  the argument list, so it cannot accept a stray token that was refused before.
- **resource_limit** — covered. 6000-character subject (3 parts) and a
  1000-element list both complete on both engines; neither function recurses, so
  neither is bounded by `MAX_CALL_DEPTH`.

## Known gaps / follow-ups

- `verify.sh` could not be run — `rbops/` is not in this checkout. Recorded in
  `FINDINGS.md` rather than claimed as a pass.
- A dotted spelling through the *wrong* module succeeds (`math.split` is
  `split`). Documented above, pinned for 14 names, and left as-is because
  refusing it would break a spelling that works today.
- Redblue's lexer has no `\u` escape, so `"😀"` in a Redblue source is the nine
  ASCII characters `u{1F600}`. Found while writing the unicode test; a phase of
  its own. → `FINDINGS.md`.
- `say` renders `[""]` and `[]` identically, so a list containing one empty
  string is indistinguishable from an empty list when printed. Pre-existing, not
  touched here; the tests bracket each part to route around it. → `FINDINGS.md`.