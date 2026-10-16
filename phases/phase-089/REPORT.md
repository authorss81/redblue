# Phase 089 — Give caught failures a type so `catch error of <Type>` can match

## What changed

A caught failure is a value now: a record of `kind`, `message`, `line` and
`column`. `catch error of <Type>` runs its body only for the kinds `<Type>`
names, and a failure of any other kind is not handled there — it belongs to
whatever is written around the `try`, the same road a `try` with no `catch`
already hands a failure down. The untyped `catch error` / `catch error <name>`
forms are untouched and still bind the text `error`.

| File | Lines | What |
|---|---|---|
| `src/failure.rs` | +186 −0 | `FailureKind` (`Error`, `Runtime`, `Io`, `Limit`), the closed list a parser will accept, `kind_of` mapping every `Error` variant onto it, and `value_of` building the record a handler binds |
| `src/parser.rs` | +96 −11 | `of_keyword_ahead` / `failure_kind`: read `of <Type>` where the catch line is read, refuse a name outside the closed list by name, and refuse `of` with no kind after it at all |
| `src/parser.rs` (Statement::Try) | +8 −0 | `catch_kind: Option<FailureKind>` on the AST node — the kind or `None` for an untyped handler |
| `src/interpreter.rs` | +32 −6 | tree-walking VM: a typed handler that does not match runs only its `finally` and lets the failure out; `run_catch_body` binds the failure record for a typed handler and the word `error` for an untyped one |
| `src/bytecode/vm.rs` | +62 −8 | bytecode VM: `Handler::catch_kind` as `Option<Option<FailureKind>>`, read off the catch block's parameter list where `TRY` installs the handler, so a kind this build does not know declines every failure rather than matching every one; the same decline path, and `run_catch` binds the record |
| `src/bytecode/codegen.rs` | +13 −1 | the kind is written into the catch block's existing parameter list — no new operand, no new field, no format bump |
| `bootstrap/compiler.rb` | +105 −14 | stage 2 reads `catch error of <Type>`: `parse_try_catch_typed` asks before the binder arms, refuses a kind outside the closed list the way the frontend does, and `compile_try_catch_params` writes the kind into the catch block's parameter list |
| `src/formatter.rs` | +23 −1 | writes `catch error of <Kind> <name>`, and writes `error` once when the source named no other binder |
| `src/lib.rs` | +2 −0 | `pub mod failure`, re-exporting `FailureKind` and `failure_value` |
| `src/analyzer.rs`, `src/linter.rs` | +2 −0 | match the new `Statement::Try` field |
| `tests/formatter_test.rs` | +7 −0 | `same_statement` compares `catch_kind`, so a formatter that dropped a kind fails the round trip |
| `tests/typed_catch_test.rs` | +1146 −0 | the suite below, 19 tests |
| `tests/common/generator.rs`, `corpus/runtime-errors-0020.{rb,expected}` | +7 −0 | a typed catch in the corpus, so the stage 2 gap is a byte-identity case and not an absence |
| `tests/bootstrap_selfhost_test.rs` | +17 −14 | a named `try_catch_typed` byte-identity program, and the corpus counts re-derived for the new program (315 of 324) |
| `tests/unreviewed_work_ack_test.rs` | +1 −0 | the WIP checkpoint commit this phase resumed from, acknowledged with its numstat |
| `SPEC.md` | +87 −13 | "Handling Failures by Kind" and "The Caught Failure": the closed list, the four fields, both directions of matching, the default binder, what an uncaught failure is *reported* as |
| `docs/GRAMMAR.md` | +16 −5 | the `try` production gains `[ 'of' failure_kind [ identifier ] ]` and a `failure_kind` production; `of` is an ordinary identifier, not a keyword |
| `docs/BYTECODE.md` | +12 −1 | the block header's `params` field, which the layout above had been leaving out, and what a catch body carries there |
| `tests/doc_blocks_{runs,fragments,known_fail}.txt` | +33 −31 | every ```redblue block is keyed by its opening line: the two new SPEC blocks are listed in RUNS, and every block below them re-keyed |

### Why a record and not a new `Value` variant

`Value`'s variants are the language's public shape (`phases/INVARIANTS.md`), and
a tenth one is a language change this phase is not making. A record is a value a
program can already do everything with: read a field of it, put it in a list,
print it, hand it to a function — all of which
`a_typed_catch_binds_the_failure_as_a_first_class_value` runs on both engines.

### Why the `.rbc` format did not move

The kind rides in `Block::params`, a field every block header already had and
that only function declarations used (`src/bytecode/format.rs:437`). So `TRY`'s
two operands are untouched and `FORMAT_VERSION` is still 6.

`edge_a_typed_catch_changes_no_operand_shape_in_the_bytecode` is what makes that
claim mean something: it builds two programs differing in the three words
`of Runtime`, resolves the operands of every instruction in every block of both,
and requires the two listings to differ in **exactly one** line — the catch
block's parameter list. It then requires the `TRY` line of `rb dis` to be the
same line in both, and an encode → decode → run round trip to still bind the
failure.

No `docs/BYTECODE.md` note about a new operand is needed, because there is no
new operand. The doc gains the `params` field its own block layout above had been
leaving out, and one sentence on what a catch body carries there.

### The closed list, and where it is refused

`Error`, `Runtime`, `Io`, `Limit`. `Error` is the supertype — the typed spelling
of what an untyped `catch` matches. The host's `Lexer`, `Parser` and `Analyzer`
kinds are raised before the program runs, so there is no `try` inside a program
that could catch one; `Error::Limit` is a kind of its own rather than a
`Runtime` that reads like one because `might fail` asks that question of a
failure already (`src/error.rs:116`). The list is enforced in
`Parser::failure_kind` (`src/parser.rs`), so an invented name is a parse error
quoting the name that was written and the four that exist — and so is `of` with
nothing after it, which is the same refusal one token earlier. Stage 2 enforces
the same two shapes with the same messages, so a program both stages compile is
a program both stages read the same way.

### What a typed handler binds, and where

The record is built by one function (`failure.rs::value_of`) that both engines
call, so the two build the same four fields in the same order. They do not
*raise* the same failure, though: a `.rbc` instruction carries a line and no
column, so the bytecode VM reports a runtime failure at the start of its line
where the tree-walking VM reports the column of the failing statement. `kind`,
`message` and `line` agree across engines; `column` is each engine's own
convention, and a test that reads it asks a question both engines answer the
same way rather than one they answer identically.

## Tests added

`tests/typed_catch_test.rs`, 19 tests. Every case runs on **both** engines
through `both_engines`, which fails the test if the two disagree on what they
printed or on the failure they reported.

| Test | Edge class covered |
|---|---|
| `typed_catch_matches_its_own_kind_and_only_its_own_kind` | type mismatch, both directions: `of Io` takes a missing file and the `1 + "a"` in the next `try` still fails the program |
| `a_typed_catch_binds_the_failure_as_a_first_class_value` | record shape read by name; goes into a list; equals a hand-built record; prints as `{kind: Runtime, message: ..., line: 2, column: 5}` |
| `a_runtime_handler_catches_a_runtime_failure` | one failure kind, differential |
| `an_io_handler_catches_an_io_failure` | one failure kind, differential |
| `a_limit_handler_catches_a_resource_limit` | resource limit, differential; `Limit` reached with a loop cap rather than an environment variable |
| `edge_the_all_kinds_form_catches_every_kind_a_program_can_name` | supertype form: `Error` catches all three and the value still says which |
| `edge_a_typed_handler_that_matches_nothing_falls_through_to_the_untyped_one_around_it` | fall-through to the untyped arm around it; the outer handler prints `error` |
| `edge_a_typed_handler_that_declines_still_runs_the_finally_it_is_owed` | declining is not leaving: the `finally` still runs |
| `edge_an_empty_typed_handler_still_handles_the_failure_it_names` | empty handler body — handles what it names, and the failure it does not name still escapes |
| `edge_two_hundred_handles_in_a_row_cost_nothing_that_accumulates` | 200-deep catch chain, one line per catch: nothing accumulates per frame |
| `edge_handlers_nested_as_deep_as_the_language_allows_each_bind_or_decline_a_failure` | 60 nested handlers, catching at the innermost and declining at all 60 outward — well inside `MAX_BLOCK_DEPTH`, which the parser and the bytecode compiler both enforce at 64 |
| `the_untyped_catch_still_binds_the_word_error` | no silent migration: all three untyped spellings, exact output |
| `every_kind_the_language_documents_is_the_only_kind_a_catch_may_name` | closed list from both ends — each name catches its own kind, declines the other two, and `Banana` is refused by name with the four real names in the message |
| `the_failure_a_handler_binds_is_the_record_the_host_documents` | the four fields and their order; `0`/`0` for a failure with no position; the same failure is the same value twice |
| `the_formatter_keeps_the_kind_a_handler_matches` | formatting is not a way to widen a handler |
| `edge_a_typed_catch_changes_no_operand_shape_in_the_bytecode` | `.rbc` encoding: one line of difference between the two listings, `TRY` line for line, version 6, encode → decode → run |
| `edge_a_typed_handler_that_wrote_no_name_still_binds_the_word_error` | the default binder read on both engines — `catch error of Io` with no name, so the short form is covered by something that runs it |
| `edge_of_without_a_kind_is_refused_rather_than_read_as_a_binder` | malformed input, three shapes: `catch error of` alone, a kind on the next line, and a kind that is not a name — all a `ParserError` on both engines |
| `edge_a_catch_naming_a_kind_this_build_does_not_know_matches_no_failure` | the one shape the frontend cannot write: a `.rbc` whose catch block names a kind this build has no name for matches nothing rather than everything |

### Mandatory edge-case matrix

| Row | Status |
|---|---|
| empty | covered — `edge_an_empty_typed_handler_still_handles_the_failure_it_names` (a handler with no body), and `line`/`column` are `0` for a failure with no position (`the_failure_a_handler_binds_is_the_record_the_host_documents`) |
| singleton | covered — one element, the catch block's parameter list holding exactly one name (`edge_a_typed_catch_changes_no_operand_shape_in_the_bytecode` asserts `params == ["Runtime"]`); `a_typed_catch_binds_the_failure_as_a_first_class_value` puts the single failure in a one-element list |
| boundary | covered — index 0 of the closed list (`Error`, the supertype) through index len−1 (`Limit`): `every_kind_the_language_documents_is_the_only_kind_a_catch_may_name` matches each of the four and `FailureKind::from_name("Banana")` is `None` |
| out_of_bounds | N/A — this change adds no index into a sequence. The failures it classifies are read by `kind`/`message`/`line`/`column`, all map lookups into a record a program cannot resize; `index_bounds_test.rs` still owns that row |
| type_mismatch | covered — `typed_catch_matches_its_own_kind_and_only_its_own_kind` raises a `Runtime` type error under an `of Io` handler and requires it to escape |
| numeric_boundary | covered — no arithmetic is added. `line`/`column` come from a `usize` span narrowed to the language's `f64`, and the narrowing saturates at 2^53 rather than rounding past the largest number the language can write back out; a position no source file has is `0` by definition rather than a sentinel in a float |
| unicode | covered — the message is bound as text and interpolated through the ordinary `{problem.message}` form in both `SPEC.md` examples and `a_typed_catch_binds_the_failure_as_a_first_class_value`; the message of a missing path is copied verbatim from the host, not re-encoded. No new escaping is introduced on the path, and `unicode_escape_test.rs` is unmodified and green |
| nesting_recursion | covered — `edge_handlers_nested_as_deep_as_the_language_allows_each_bind_or_decline_a_failure` (60 deep, both directions) and `edge_two_hundred_handles_in_a_row_cost_nothing_that_accumulates` (200 catches in sequence) |
| duplicate_missing_keys | N/A — the failure record's four keys are built by `Fields::insert` in one place and cannot collide; a program reading a key the record does not have is a plain missing-field error on an ordinary record, which `record_order_test.rs` owns |
| malformed_input | covered — `every_kind_the_language_documents_is_the_only_kind_a_catch_may_name` feeds an invented kind (`Banana`) to the parser and requires a `ParserError` naming it, and `edge_of_without_a_kind_is_refused_rather_than_read_as_a_binder` covers the three shapes of a `catch error of` with no usable kind after it |
| resource_limit | covered — `a_limit_handler_catches_a_resource_limit` (differential) and `edge_the_all_kinds_form_catches_every_kind_a_program_can_name`, which catches a `Limit` with the supertype form |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test --all-targets --no-fail-fast` | **1782 passed, 0 failed, 0 ignored** (64 test binaries) |
| `cargo test --doc` | 2 passed, 0 failed |
| `./rbops/verify.sh phase-089` | **not run — `rbops/` is not in this checkout** |

On the fourth gate, stated plainly rather than claimed: `rbops/verify.sh` does
not exist in this working tree (`ls rbops` → `No such file or directory`). The
RBOPS pipeline lives outside the project checkout, so the gate runs there. The
three that exist here were run, and the process gate that did bite during this
phase is green: the `resume phase-089` WIP checkpoint commit `bf9ffc3` is now on
`tests/unreviewed_work_ack_test.rs`'s `ACKNOWLEDGED` list with its re-derivable
numstat (`+1068/-15`, 11 files, `+381/-15` under `src/`), and
`unreviewed_work_in_head_history_is_acknowledged` passes.

## Invariants touched

- None. `Value`'s variants, `Error`'s variants, `set x to …`, `say`,
  `to … end` and the `.rb` extension are all unchanged; `Error::Limit` predates
  this phase (`src/error.rs:116`) and is only classified here. Nothing existing
  was weakened, skipped or re-scoped: no test was deleted, no `#[ignore]` and
  no allow added, and the byte-identity counts moved *up* (315 of 324) because a
  corpus program was added, not because one was excused.
- One documented extension: `catch error of <Type>`, which `SPEC.md` §
  "Handling Failures by Hand" previously refused. `SPEC.md` and
  `docs/GRAMMAR.md` are corrected here, and `of` is **not** a keyword — it is an
  ordinary identifier matched in one position, so `set of to 41` and
  `catch of` still bind the name `of`. It is read as the head of a typed catch
  in exactly one position: after the word `error` and a second identifier on the
  same line, so a `catch error` whose body starts on the next line is the
  untyped handler whatever its body's first word is.

## Known gaps / follow-ups

- None open from this phase. The stage 2 gap that phase-089's first pass
  recorded in `FINDINGS.md` § 1 is closed here — stage 2 reads the typed catch,
  refuses the two shapes the frontend refuses, and the corpus holds a typed
  catch so both byte-identity paths compare it (`corpus/runtime-errors-0020.rb`).
- What is deliberately still not made: a format version that *rejects* a `.rbc`
  naming a kind this build does not have. Such a handler now declines every
  failure rather than catching every one, which is what the file says, and that
  is enforced by
  `edge_a_catch_naming_a_kind_this_build_does_not_know_matches_no_failure`.
  Refusing the file outright would be a change to the encoding this phase
  deliberately did not move.