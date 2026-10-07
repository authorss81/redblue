# Phase 015 — FINDINGS

Found while verifying phase-015. Each is anchored to a line I actually read.

Two of the findings recorded by the parked attempt (F1 and F5 below) no longer
reproduce on the current tree: later phases gave the language the two features
they were about. They are rewritten as RESOLVED with the evidence, because a
finding that no longer holds must not send the next phase looking for a bug that
is not there.

## F1 — RESOLVED: `modules/MathUtils.rb` parses and runs

The parked attempt recorded this as a major parser defect (`constant PI to
3.14159` was not grammar). It is grammar now: `Statement::Constant` is declared
at `src/parser.rs:115` and the parser builds it. Verified against the built
binary:

```
$ rb run modules/MathUtils.rb ; echo rc=$?
rc=0
```

`Statement::Constant` is handled in `src/formatter.rs:291`, and this phase's
corpus now includes `modules/`, so the file is covered by both the idempotence
and the losslessness properties over every corpus file.

## F2 — FIXED HERE: `formatter_test.rs` corpus omitted `modules/`

`tests/formatter_test.rs:13` iterated `["examples", "tests"]` only, while
`AGENTS.md` §2 designates `modules/*.rb` as specification-by-example. Now
`["examples", "tests", "modules"]`. The directory read still tolerates a missing
directory, so this is safe if `modules/` is ever absent.

Both property tests — `format_is_idempotent_over_the_whole_corpus` and
`format_preserves_the_meaning_of_every_corpus_file` — now cover every `.rb`
file under all three directories, and `edge_a_module_and_its_exports_round_trip`
covers the module statement form directly.

## F3 — `rb format` does not rewrite the file in place

**Severity: minor.** `src/lib.rs:179`:

```rust
Ok(formatted) => print!("{}", formatted),
```

`rb format foo.rb` prints to stdout. `rb format foo.rb > foo.rb` therefore
truncates the file before the shell ever reads it. `--check` is the only other
mode and it does not write either.

This bit me during verification of the parked attempt and cost a cycle: an
idempotence sweep that expected in-place rewrite concluded "check fails after
format" for 20 files, and the harness was what was wrong. Options: add
`rb format --write`, or make `rb format` write in place and require `--stdout`.
Either is a CLI change, not a phase-015 change.

## F4 — no `rbops/` in the checkout

`ls rbops/` → `No such file or directory`; `.opencode/` is likewise absent. So
`./rbops/verify.sh phase-015` — the gate `AGENTS.md` §1 rule 2 makes the only
authority — could not be run from this tree. Stated in `REPORT.md` rather than
claimed as passing. The other three gates were run directly.

## F5 — RESOLVED: the specification's range loop and relational operators are built

The parked attempt recorded these as unreachable spec drift: `Statement::ForRange`
was declared but never constructed, and the lexer had no `<` token. Both are
grammar on the current tree. Verified against the built binary:

```
$ printf 'for each i from 0 to 10 by 5\n    say i\nend\n' > t.rb
$ rb run t.rb
0
5
10
$ printf 'say 1 is less than 2\n' > t.rb ; rb run t.rb
yes
$ printf 'say 1 < 2\n' > t.rb ; rb run t.rb
yes
```

`TokenKind::From` is at `src/lexer.rs:100`, `BinaryOp::{Less, LessEqual, Greater,
GreaterEqual}` are declared in `src/parser.rs` and formatted at
`src/formatter.rs`, and no corpus file uses any of them — so the formatter's
rendering of them would otherwise have gone unasserted by anything. This phase
adds the coverage: `edge_a_range_loop_is_formatted_and_keeps_its_bounds_and_step`,
`edge_a_range_loop_step_is_not_lost_or_invented`,
`edge_every_relational_operator_round_trips_in_both_spellings`,
`edge_a_relational_operator_is_not_reordered_around_its_operands`,
`edge_a_spec_only_form_is_never_reported_as_an_error`.

Whoever adds an operator or a loop form next must extend
`format_covers_every_statement_form` and
`format_keeps_grouping_for_unary_and_every_operator_level` in
`tests/formatter_test.rs`; nothing mechanically enforces the link.