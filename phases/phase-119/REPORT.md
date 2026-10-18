# Phase 119 — Expose the last value as `$_` and name sessions

## What changed

| File | Lines | What |
|---|---|---|
| `src/lexer.rs` | +26 −0 | `LAST_VALUE_NAME` and the one lexeme that uses it: `$_` is an identifier, matched exactly. `$` followed by anything else is still `Unexpected character '$'`. |
| `src/interpreter.rs` | +12 −0 | `Vm::seed_binding` — binds a name in the globals a program reads, for a caller that owns the session rather than the program. |
| `src/repl/mod.rs` | +495 −1 | `$_` seeded from the same value `_` is bound from, `:last`/`:session`/`:sessions` and their dispatch, and the tests. |
| `src/repl/commands.rs` | +72 −0 | `Last`, `Session(Option<String>)`, `Sessions` in the enum, the table and `argument_text`, plus the parse test. |
| `src/repl/labels.rs` | +197 (new) | The record file `:session` appends to and `:sessions` reads: append-only `<unix-seconds>\t<name>`, bounded in bytes, listed for the last week in name order. |
| `tests/repl_last_value_test.rs` | +125 (new) | The same behaviour typed at the real `rb` binary through a pipe. |

`_` is not touched. It is still inserted at the same place, from the same value,
on the same lines — `$_` is seeded from a clone of it in the same breath, and
`last_answer` reads `_` rather than a second copy, so `:last` and `$_` cannot
disagree without one of them failing.

## Reproduction (before the change)

```
$ printf '2 + 3\nsay $_\n' | ./target/debug/rb
>>> = 5
>>> Error: LexerError: Unexpected character '$'
  --> <session>:1:1
1 | $_
  | ^

$ printf ':session mine\n:sessions\n:last\n' | ./target/debug/rb
>>> Unknown command 'session'. Type :help for available commands.
>>> Unknown command 'sessions'. Type :help for available commands.
>>> Unknown command 'last'. Type :help for available commands.
```

After:

```
$ printf '40 + 2\nsay $_ + 3\n:last\n:session chasing a null\n:sessions\n:session\n' | ./target/debug/rb
>>> = 42
>>> 45
>>> = 42
>>> this session is now: chasing a null
>>>   chasing a null
>>> this session: chasing a null
```

`:vars` lists `$_ = 42` beside `_ = 42`, and `:type $_` / `:inspect $_` answer
about it, because the binding is an ordinary global rather than a side table.

## Tests added

Rust `#[test]`s (11 in `src/`, 1 in `src/repl/commands.rs`) and 4 in
`tests/repl_last_value_test.rs`.

| Test | Edge class covered |
|---|---|
| `repl::tests::last_value_name_resolves_to_the_previous_lines_value` | happy path, asserted against the literal 42 |
| `repl::tests::last_value::last_and_the_last_value_name_are_the_same_value` | singleton; `:last`, `$_` and `_` read together; a list value, not only a number |
| `repl::tests::last_value::edge_last_value_read_before_any_line_has_returned_one` | empty — first line; asserts a failure twice over (`Unknown variable '$_'`, `:last` refusal) |
| `repl::tests::last_value::edge_a_failed_line_leaves_the_last_successful_value_in_place` | failure line — `1 / 0` between two successes |
| `repl::tests::last_value::the_underscore_binding_is_unchanged_by_this` | the `_` binding this phase must not change |
| `repl::tests::session_names::a_session_name_is_written_and_listed_in_sorted_order` | duplicate keys (`alpha` recorded twice), deterministic order |
| `repl::tests::session_names::naming_a_session_changes_no_programs_behaviour` | labelling is not observable in a program's answer |
| `repl::tests::session_names::edge_session_names_with_spaces_sigils_and_junk` | spaces, a command sigil (`:quit` as a *name*), empty name, tab, oversize name |
| `repl::tests::session_names::edge_a_unicode_session_name_is_listed_as_it_was_written` | unicode — emoji, CJK, RTL, round-tripped |
| `repl::tests::session_names::edge_a_label_file_past_its_cap_is_refused_rather_than_read` | resource limit — the record file's byte cap |
| `repl::tests::session_names::edge_listing_covers_this_week_and_nothing_older` | boundary — `WEEK_SECONDS ± 1`, and a record dated in the future |
| `repl::tests::session_names::edge_a_malformed_label_file_lists_nothing_rather_than_failing` | malformed input — no tab, empty name, missing file |
| `repl::commands::tests::parse_reads_a_session_name_whole_and_tells_it_from_a_listing` | `:sessions` is not parsed as `:session`; a name carrying a sigil arrives whole |
| `repl_last_value_test::the_last_value_is_readable_from_the_next_line_at_the_prompt` | end-to-end through the binary |
| `repl_last_value_test::a_failed_line_leaves_the_last_successful_value_readable` | end-to-end failure line |
| `repl_last_value_test::the_last_value_on_the_first_line_is_refused_by_name` | end-to-end, asserts a failure |
| `repl_last_value_test::the_last_command_prints_what_the_name_reads_and_refuses_when_there_is_none` | end-to-end `:last`, both answers |

`_` is asserted by three tests that predate this phase and pass unmodified:
`repl::tests::edge_reset_drops_a_block_the_session_had_not_finished_typing`
(`_` is `None` after a line that returned nothing),
`repl::tests::edge_a_block_that_outgrows_the_buffer_is_dropped_not_kept` (`_` is
`5` after `2 + 3`), and
`repl::tests::introspection::edge_type_refuses_a_program_and_the_session_never_knows_it_happened`
(a question about the session does not change the last result).

## Test requirement matrix

| Row | Status |
|---|---|
| empty | covered — `edge_last_value_read_before_any_line_has_returned_one` (a session with no last value at all); an empty session name is refused |
| singleton | covered — one successful line, and a listing of one recorded name; `alpha` recorded twice is listed once |
| boundary | covered — `edge_listing_covers_this_week_and_nothing_older` at `WEEK_SECONDS − 1` and `+ 1`, plus a stamp from the future |
| out_of_bounds | N/A — nothing in this change is indexed. The nearest equivalent, an over-long read of a file that only grows, is `edge_a_label_file_past_its_cap_is_refused_rather_than_read` |
| type_mismatch | covered — `$_` is not number-shaped: `last_and_the_last_value_name_are_the_same_value` reads a list value through it, and a name holding any kind is what a copy of the last value is |
| numeric_boundary | N/A — no arithmetic is added. The nearest relevant case, a failed numeric line, is `edge_a_failed_line_leaves_the_last_successful_value_in_place` (`1 / 0`) |
| unicode | covered — `edge_a_unicode_session_name_is_listed_as_it_was_written`; the lexer's non-ASCII identifier path is untouched |
| nesting_recursion | N/A — no new scope, call or loop. `$_` is resolved by the one global lookup a program already uses, and the record file is read line by line |
| duplicate_missing_keys | covered — a repeated name is listed once; an empty or unparseable record line is skipped rather than listed |
| malformed_input | covered — `edge_a_malformed_label_file_lists_nothing_rather_than_failing`, and `$foo` is still the lexer error it always was (`edge` case in the parse of the lexeme: only `$_` lexes) |
| resource_limit | covered — the record file is capped at 64 KiB in both directions (read refused, append refused), and a session name is capped at 120 bytes |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass (no diff) |
| `cargo clippy --all-targets -- -D warnings` | pass (no warnings) |
| `cargo test --all-targets` | **1802 passed, 0 failed**, 65 test targets |
| `./rbops/verify.sh phase-119` | **not run — the script is not in this checkout** |

The fourth gate could not be run and is not claimed. `rbops/` does not exist
here: `ls rbops` → `No such file or directory`, and
`find . -maxdepth 2 -name verify.sh -not -path ./target/*` returns nothing.
`rbops/phases.json` is likewise absent, so this phase's `must_touch` could not
be read; the changed files are `src/` and `tests/`, which is the union the
manifest entries from 011 onward declare.

As a substitute for the examples the gate runs, every `examples/*.rb` was run
with the rebuilt binary (`for f in examples/*.rb; do ./target/debug/rb run "$f"; done`)
with no failure.

## Invariants touched

None of the table in `AGENTS.md` §2. Specifically:

- `.rb` is still the extension; nothing about file handling changed.
- `$` is lexed as an identifier **only** as the two characters `$_`. `$name`,
  `$1` and a lone `$` are the errors they were before this phase
  (`the_last_value_on_the_first_line_is_refused_by_name` and the manual run
  above both show `$foo` still refused).
- `set x to <expr>`, `say`, `to … end` and the `Value` / `Error` types are
  untouched.
- The existing `_` binding is unchanged, as the three pre-existing tests above
  show.

New surface, stated plainly: `$_` is a name a program may read, and a `set $_`
inside one line is visible for the rest of that line only — the next successful
line re-seeds it from that line's value, so `$_` can never drift away from what
`:last` prints. `:last`, `:session` and `:sessions` are three new REPL
commands.

## Known gaps / follow-ups

- **A session name is not a restore target.** `:session <name>` names the
  session and records it; restoring still goes through `:save`/`:restore` under
  a file name. Making `:restore <name>` find a named session would change
  `:restore`'s argument meaning, which this phase was not asked to do.
  → FINDINGS.md
- `$_` is not offered by the editor's completer and not highlighted: it is a
  global, so it *is* completed once it is bound (it appears in `session_names`
  through `Vm::user_names`), but a session that has not yet produced a value
  will not see it. Left alone rather than special-cased in the highlighter.
- The listing is filtered by the local clock, so a machine whose clock jumps
  backwards can see a name disappear from `:sessions` for a week.
  → FINDINGS.md