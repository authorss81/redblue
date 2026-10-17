# Phase 114 — Make `set x, y to 1, 2` and `import a.b as c` parse, or strike them from GRAMMAR.md

**Decision: implement both.** Neither production is struck. `docs/GRAMMAR.md` § 3.1
and § 4.1 now describe what the two forms *mean* and what they refuse, and both
spelling work through `rb run` (tree-walking VM) and `rb compile` + `rb vm`
(bytecode VM).

## Finding re-verified first

On `main` (commit `e008f0b`), before any change:

```
$ printf 'set x, y to 1, 2\nsay x\nsay y\n' > target/tmp/p114/multi.rb
$ ./target/debug/rb run target/tmp/p114/multi.rb
Error: ParserError: Expected 'to' but got Comma
  --> target/tmp/p114/multi.rb:1:6
1 | set x, y to 1, 2
  |      ^
exit 1

$ ./target/debug/rb run target/tmp/p114/dotted.rb      # import math.sqrt as s
Error: ParserError: Unexpected token Dot
  --> target/tmp/p114/dotted.rb:1:12
1 | import math.sqrt as s
  |            ^
exit 1

$ ./target/debug/rb run target/tmp/p114/plain.rb       # import math / import time as t
ok
exit 0
```

The finding reproduces exactly as the phase recorded it. After this change the
same three programs print `1`, `2`, `3` and `ok`.

## What changed

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +175 −1 | `parse_set_multiple` — the comma-list name and value lists, equal-count check, property-target refusal; `Statement::SetMultiple`; the dotted-import arm of `parse_statement` (`.member` + required `as`); `ImportItem.member` and `ImportItem::bound_name` |
| `src/interpreter.rs` | +68 −0 | `SetMultiple` executes as one evaluate-then-bind per position, left to right; `imported_member` resolves `module.member`; the `Import` arm binds the alias to the member and not to a namespace; a name bound to a `Value::Builtin` is that builtin (needed so `s(9)` is a call) |
| `src/bytecode/vm.rs` | +88 −18 | the `IMPORT` module operand may be the dotted spelling `a.b`; `imported_member`; the same alias-binding rule; the same builtin-value call rule |
| `src/bytecode/codegen.rs` | +21 −2 | `SetMultiple` lowers to one value + `STORE` per position; `Import` emits the dotted spelling and `item.bound_name()` |
| `src/analyzer.rs` | +22 −1 | declares every name of a `SetMultiple`; a dotted import declares the alias alone, not the module name |
| `src/formatter.rs` | +24 −0 | writes `set a, b to 1, 2` and keeps the `.member` of a dotted import (it was writing `import math to s`, a different program that still parses) |
| `src/linter.rs` | +10 −0 | a `SetMultiple` is as many bindings as it names, and every value is read |
| `src/runtime.rs` | +15 −5 | a `SetMultiple` in a module body contributes all of its names to the importer |
| `src/repl/introspect.rs` | +8 −1 | `.keywords` and the call walk see a `SetMultiple` |
| `docs/GRAMMAR.md` | +24 −0 | § 3.1 and § 4.1 say what each form binds, requires and refuses, plus one new ```redblue block (`set width, height to 4, 3`) classified in `tests/doc_blocks_runs.txt` |
| `SPEC.md` | +13 −0 | "Import with Alias" gains the third import form and the multiple-assignment rule |
| `docs/BYTECODE.md` | +7 −1 | `IMPORT`'s primary operand may be `module.member`; no new opcode and no version bump |
| `tests/doc_blocks_*.txt` | +38 −37 | block keys moved down with the text above them; one block added to the running bucket |
| `tests/multi_assign_import_dotted_test.rs` | +457 (new) | 22 tests |
| `tests/unreviewed_work_ack_test.rs` | +1 −1 | housekeeping for the resume commit itself — the `ACKNOWLEDGED` data list, one entry swapped (see *This resume run* below) |
| `docs/GRAMMAR.md` | +4 −3 | § 3.1: "an alias is required in this form (`as`, or the older `to`)", which is what the parser has always accepted and what the formatter emits |
| `tests/doc_blocks_runs.txt`, `tests/doc_blocks_fragments.txt` | +10 −10 | the ten `docs/GRAMMAR.md:<line>` keys below § 3.1, moved down by one because the sentence above them is one line longer. Data only — no bucket gained or lost a block. |

No opcode was added, no instruction byte changed, and `Chunk::encode`'s output
for every program that does not use the new spellings is byte-identical — which
is why `tests/bootstrap_selfhost_test.rs` (stage-2 byte-identity on the corpus,
the examples and the modules) is unaffected.

### The semantics chosen, and why

- **Multiple assignment is left to right.** Each value is evaluated and bound
  before the next is evaluated, so `set x, y to 1, 2` is the same run of bindings
  two consecutive `set` statements would have made. `set y, x to 3, 4` after
  `set x, y to 1, 2` therefore leaves `x` at 4, not the tuple reading. The
  grammar's `{ ',' expression }` is one expression per value, not a list, and
  there is no destructuring anywhere else in the language to be consistent with.
- **A value cannot read a name its own statement binds.** The analyzer walks the
  values before it declares the names, exactly as it does for a single `set`, so
  `set x to x + 1` and `set x, y to 1, x + 1` are refused the same way
  (`Unknown variable 'x'`). This is the analyzer's pre-existing order, not a
  choice this phase made.
- **A dotted import binds one member and nothing else.** `import math.sqrt as s`
  binds `s` to `sqrt`'s own value; the module's own name is *not* bound, in
  either VM and in the analyzer. A program that asked for one member did not ask
  for the namespace.
- **An alias is required** for the dotted form, because the grammar writes it and
  because binding a member under its own name would shadow a program name the
  program never wrote. `import math.sqrt` is a parse error naming the missing
  alias. The alias keyword is `as` (what § 3.1 writes) or `to` (the older
  spelling, already accepted for the plain form and kept because programs under
  `tests/` use it) — which is also why the formatter writes
  `import math.sqrt to s`, and why `edge_the_formatter_keeps_both_new_spellings`
  runs what the formatter wrote rather than string-matching `as`.

## Tests added

All in `tests/multi_assign_import_dotted_test.rs`; 22 `#[test]` functions, of
which 17 are named `edge_*`.

| Test | Edge class covered |
|---|---|
| `multiple_assignment_binds_every_name` | happy path — the phase's headline DoD |
| `multiple_assignment_takes_any_expressions` | values are arbitrary expressions, indexed too |
| `edge_multiple_assignment_of_two_and_three_names` | two names, three names, order (sequential, not tuple), and `Unknown variable 'a'` for a value reading its own statement's name |
| `edge_multiple_assignment_with_a_trailing_comma_is_refused` | malformed input — `set x, y, to 1, 2` |
| `edge_multiple_assignment_arity_mismatch_is_refused` | missing right-hand side `set x, y to 1` **and** one value too many `set x, y to 1, 2, 3`; both name both counts |
| `edge_multiple_assignment_with_no_names_is_refused` | empty — `set to 1`, `set x, y to` |
| `edge_multiple_assignment_refuses_a_property_target` | `set a.b, c to 1, 2` names the property rule rather than `Expected 'to' but got Comma` |
| `edge_multiple_assignment_of_many_names` | boundary / resource — 64 names bind; 64 names with 63 values is refused naming "64 names" and "63 values" |
| `edge_multiple_assignment_of_unicode_names` | unicode — `α, β` bind; the count still decides |
| `edge_multiple_assignment_binds_mixed_types` | type boundary — a number, a text and a list side by side, and `nothing` as a value |
| `dotted_import_binds_the_named_member` | happy path — `import math.sqrt as s` then `s(9)` |
| `dotted_import_reaches_a_module_files_member` | the other member source — a `modules/*.rb` member published by the loader |
| `plain_imports_still_work` | back-compat — `import Mod`, `import Mod as m`, and `M.degrees_to_radians(180)` |
| `edge_dotted_import_does_not_take_over_a_bound_name` | shadowing rule — `set f to 5` after `import math.floor as f` leaves the program's 5 |
| `edge_dotted_import_of_a_missing_member_is_refused` | **failure** — `Module 'math' has no member 'no_such_function'`, and the same for a module file's missing member |
| `edge_dotted_import_of_a_missing_module_is_refused` | **failure** — the ordinary `Cannot find module` |
| `edge_dotted_import_without_as_is_refused` | malformed input — `import math.sqrt` names the required `as` |
| `edge_dotted_import_joins_a_comma_list` | a dotted import is one item of an import list |
| `edge_both_engines_agree_on_the_new_spellings` | the two engines on one program using both new forms |
| `edge_the_formatter_keeps_both_new_spellings` | round-trip: format, check the member survived, check it is a fixed point, then run the formatted text on both engines |
| `edge_the_linter_sees_every_name_of_a_multiple_assignment` | one unused variable reported by name, the read one not |
| `edge_crlf_and_empty_input_are_unchanged` | malformed input — CRLF after the new form, the empty program, `import math. as s`, `import math.sqrt as` with no alias, `import as s` |

Eleven of these assert a **failure** (`Error::Parser`, `Error::Analyzer` or
`Error::Runtime`), and every one of them checks the message names the thing that
was wrong, not merely that something failed. Zero `#[ignore]`, zero `// skip`,
zero `allow(clippy::`.

## Test-requirements matrix

- **empty** — covered: `set to 1` and `set x, y to` (`edge_multiple_assignment_with_no_names_is_refused`), `set n, b to nothing, nothing`, the empty program (`edge_crlf_and_empty_input_are_unchanged`).
- **singleton** — covered: `multiple_assignment_takes_any_expressions` uses one-indexed and one-value reads; the smallest multiple assignment that is not a single `set` is two names, and two names is the phase's own headline, pinned by `multiple_assignment_binds_every_name`.
- **boundary** — covered: two names / three names / 64 names (`edge_multiple_assignment_of_two_and_three_names`, `edge_multiple_assignment_of_many_names`), and the one-off arity mismatch on both sides.
- **out_of_bounds** — N/A: neither form indexes. The nearest analogue is the equal-count rule, which is the boundary row above and is covered both ways.
- **type_mismatch** — N/A as a *rejection*: the language has one `Value` enum, a `set` performs no conversion, and a multiple assignment adds none. What the row is really asking — that the values are not quietly coerced into one another — is covered by `edge_multiple_assignment_binds_mixed_types` (number, text and list in one statement; `nothing` as a value).
- **numeric_boundary** — N/A: the values of a multiple assignment are ordinary expressions evaluated by the code that already existed. `0/0`, `1/0`, `-0.0`, `NaN`, `2^53±1` and `i64` overflow are all properties of the arithmetic operators, untouched here; pinning them again would be a copy of `tests/numeric_edge_test.rs`.
- **unicode** — covered: `edge_multiple_assignment_of_unicode_names`.
- **nesting_recursion** — N/A: neither form nests (the grammar writes no nested list of names), and neither adds recursion. The interpreter's call-depth and block-depth limits are unchanged and already pinned by `tests/call_depth_test.rs`.
- **duplicate_missing_keys** — N/A: no record is built. The nearest analogue is a name repeated inside one statement — `set x, x to 1, 2` — which is *not* an error here, matching `set x to 1` then `set x to 2`, so there is nothing to assert. Recorded in FINDINGS.md as the one semantic this phase had to pick.
- **malformed_input** — covered: trailing comma, arity mismatch both ways, no names, empty right-hand side, property target, `import math.sqrt` with no alias keyword, `import math. as s`, `import math.sqrt as` with no alias name, `import as s`, CRLF, empty program.
- **resource_limit** — covered: 64 names and 64 values in one statement (`edge_multiple_assignment_of_many_names`). The 64 is not a limit the parser imposes; it is the largest list this phase's tests drive, and both engines answer it identically.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — no diff |
| `cargo clippy --all-targets -- -D warnings` | **pass** — zero warnings |
| `cargo test --all-targets` | **1807 passed, 0 failed** (sum of every binary's `test result` line) |
| `./rbops/verify.sh phase-114` | **not run — the file is not in this checkout** |

All three runnable rows are from a clean re-run **at the end of this resume**,
after every edit. Two of them were red before then, and both reds are recorded
above rather than smoothed over: the first `cargo test` here was red on
`unreviewed_work_in_head_history_is_acknowledged`, and `tests/doc_blocks_test.rs`
went red twice in this run while § 3.1's sentence was being made precise and its
block keys re-shifted. The 1807/0 is the final figure, not the first one.

`rbops/` does not exist here: `ls rbops/` returns `No such file or directory`, and
so does `./rbops/verify.sh`. That gate is the pipeline's, not the agent's, and I
did not create or reconstruct it (rule 1: do not touch `rbops/`). What I could run
in its place, I ran:

- every `examples/*.rb` runs with exit 0;
- every `modules/*.rb` and every `corpus/*.rb` exits with the same status as on
  `e008f0b` — 98 non-zero exits both before and after, all of them the
  deliberate runtime-error and malformed-input programs the corpus records in
  its `.expected` files;
- `tests/doc_blocks_test.rs` passes, which executes every ```redblue block in
  `SPEC.md`, `docs/GRAMMAR.md`, `README.md`, `PHILOSOPHY.md` and `HANDOUT.md`
  through `rb run` **and** `rb compile`.

### The resume run: what was red, and what was changed

The first attempt's `Gates` table claimed clippy and `cargo test` were green.
They were not. Both were run before any edit in this run, and both were red:

```
error: needlessly taken reference of left operand   --> src/bytecode/vm.rs:2779:46
error: needlessly taken reference of left operand   --> src/interpreter.rs:2302:46
error: casting to the same type is unnecessary (`usize` -> `usize`)
                                                  --> tests/multi_assign_import_dotted_test.rs:399:18
error: test failed, to rerun pass `--test unreviewed_work_ack_test`
      unreviewed_work_in_head_history_is_acknowledged
      UNACKNOWLEDGED: 0e95f21... resume phase-114: adopt preserved work
```

| File | Lines | What |
|---|---|---|
| src/interpreter.rs | +2 −1 | `builtin.as_str() != name` instead of `&builtin != name` — the same comparison, written so `clippy::op_ref` is satisfied |
| src/bytecode/vm.rs | +1 −1 | the same one-line fix on the bytecode engine's identical guard |
| tests/multi_assign_import_dotted_test.rs | +1 −1 | `values[..count - 1]` — `count` is already `usize` |
| tests/unreviewed_work_ack_test.rs | +1 −1 | the resume commit acknowledged in `ACKNOWLEDGED`, with its measured `git show --numstat` and the reason it was not rewritten |
| phases/phase-114/REPORT.md | — | this section |

### This resume run: the same audit, one commit further on

The gates were run again before any edit in this run. `cargo fmt` and
`cargo clippy` were green; `cargo test --all-targets` was **red on exactly one
test**, and it was the same housekeeping audit, one commit further on:

```
test unreviewed_work_in_head_history_is_acknowledged ... FAILED
unreviewed-work commits selected: 30 | acknowledged: 30 | unacknowledged: 1
  | stale: 1
  | UNACKNOWLEDGED: 70029f7a66e1dda09bc3f1814fff57082b66bbd7  resume phase-114: adopt preserved work
  | STALE: 0e95f21cb545f4dfc7394e7f63fe1b4c712f417f
```

That audit selects commits whose subject marks unreviewed work and requires two
things of the list in `tests/unreviewed_work_ack_test.rs:66`: every selected
commit is acknowledged, and **no entry names a commit `HEAD` does not contain**
(the `scan.stale` assertion at `tests/unreviewed_work_ack_test.rs:492`). The
previous run's list named `0e95f21` — correct then, because that commit *was*
`HEAD`. The pipeline then committed this work as `70029f7a`, which made
`0e95f21` a commit `HEAD` does not contain (stale) and left `70029f7a`
unacknowledged. The list was swapped from one to the other and the reason's
`numstat` figures were re-derived from `git show --shortstat` (19 files,
+1304/−66; +433/−28 under `src/`). `cargo test --test unreviewed_work_ack_test`
is 13/13 after it.

Nothing else was changed in this run: no production code, no test assertion, no
selector, no threshold. `ACKNOWLEDGED` is data, the entries before and after are
both full lowercase object ids with non-empty reasons, and the edit is the
`+1/−1` of one line.

One further change was forced by that one: making § 3.1's sentence precise
(`docs/GRAMMAR.md`, +4 −3) made it one line longer, and
`tests/doc_blocks_test.rs` classifies every ```redblue block by the line it
starts on, so the ten `docs/GRAMMAR.md:<line>` keys after it moved down by one.
Those keys are shifted in `tests/doc_blocks_runs.txt` and
`tests/doc_blocks_fragments.txt` and nowhere else — no bucket gained or lost a
block, and `tests/doc_blocks_test.rs` is 7/7 after it.

Nothing else was changed. No test was weakened, skipped, ignored, re-scoped or
deleted; no threshold, assertion or assertion message was loosened. The three
clippy findings were lint violations in code that was otherwise correct, and the
fourth was the repository's own unreviewed-work audit asking for the resume
commit to be listed, which is what every prior resume commit in this repository's
history has done.

What this run re-verified itself, rather than taking on report, with
`target/debug/rb` built from this tree:

| Check | Result |
|---|---|
| `set x, y to 1, 2` → `say x`, `say y` | prints `1`, `2`, exit 0 |
| `set a, b, c to 1, 2, 3` → three says | prints `1`, `2`, `3`, exit 0 |
| `import math.sqrt as s` → `say s(9)` | prints `3`, exit 0 |
| `import math`, `import time as t` → `say "ok"` | prints `ok`, exit 0 |
| `set x, y, to 1, 2` | `ParserError: Expected variable name, found reserved word 'to'` |
| `set x, y to 1` | `ParserError: A multiple assignment to 2 names has 1 value` |
| `set x, y to 1, 2, 3` | `ParserError: A multiple assignment to 2 names has 3 values` |
| `import math.sqrt` | `ParserError: Expected 'as' after the dotted import 'import math.sqrt'` |
| `import math.no_such_function as s` | `RuntimeError: Module 'math' has no member 'no_such_function'` |
| `import nope.sqrt as s` | `RuntimeError: Cannot find module 'nope'` |
| every `examples/*.rb` | 7/7 exit 0 |
| `modules/*.rb` + `corpus/*.rb` | 372 files, 98 non-zero exits (unchanged from the count the first attempt recorded; `cargo test` pins each against its `.expected`) |

## Invariants touched

- **None.** `phases/INVARIANTS.md` and `AGENTS.md` § 2 are unchanged by this diff:
  `.rb` is still the extension, `to … end` still uses `end`, `set x to <expr>` is
  still assignment, `say` still prints, the `Value` variants are unchanged, the
  `Error` variants are unchanged, and trailing-comma and `{interp}` string syntax
  are untouched. What changed is which *statements the parser accepts*, which is
  what this phase exists to change.
- One public API addition, both additive: `parser::Statement::SetMultiple` and
  `parser::ImportItem::member`. `redblue::Value` and `redblue::Error` — the two
  the contract names — are not touched. Every existing test keeps passing, and no
  test was weakened, skipped, ignored or re-scoped.

## Known gaps / follow-ups

- `./rbops/verify.sh` was not run because it is not in the checkout. The gate
  should be run by the pipeline; nothing in this report claims it passed.
- A third grammar production is in exactly the state this phase found the other
  two in — `docs/GRAMMAR.md` § 4.1 writes
  `declaration = 'set' identifier [ ':' type_expression ] 'to' expression`, and
  `set x: number to 1` is `ParserError: Expected 'to' but got Colon`. Same class
  of defect, next phase. → FINDINGS.md
- "First-class builtins" is now half-true and should be finished deliberately
  rather than by accident: a name bound to a builtin can be *called*
  (`set f to sqrt` then `f(9)`), which is what the dotted import needs, but a
  builtin value is still refused everywhere else — `g.map` on a builtin is
  `Cannot access property on non-object`. → FINDINGS.md
- `set x, x to 1, 2` binds the same name twice in one statement and is allowed,
  matching two consecutive `set` statements. Recorded rather than changed.