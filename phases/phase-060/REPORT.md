# Phase 060 — Export sessions and complete file paths: `.export` plus TAB for `:load`/`:save`

## What changed

This run closes the file loop: the three commands that turn a REPL session into
a file that can be edited, re-read and re-run. `.export` and TAB path completion
were already in the tree from commit `420f7b7` and were left alone.

`must_touch: [src/]` is satisfied: 3 of the 5 changed files are under `src/`.
This run's diff is **+926 −12** across 5 files.

| File | Lines | What |
|---|---|---|
| src/repl/commands.rs | +57 −2 | `ReplCommand::Reload(Option<String>)` and `ReplCommand::Format(Option<String>)`, two rows in `COMMAND_TABLE` declaring `FILE_USAGE` (which is what gives them TAB path completion for free), `argument_text` arms, and rows in the two exhaustive tests |
| src/repl/mod.rs | +346 −8 | `last_file` on the session; `path_or_last_file`, `reload_file`, `format_file`; `:reload`/`:format` dispatch; format-on-enter in `execute_code` with `format_on_enter`, the `same_tokens` guard, and a four-test `format_on_enter` unit module |
| src/repl/completer.rs | +3 −1 | One doc comment that said "the four commands that take a path today", which is now seven |
| tests/repl_test.rs | +519 −1 | 19 new end-to-end tests through the `rb` binary and three helpers; one existing assertion strengthened (see *Tests added*) |
| tests/unreviewed_work_ack_test.rs | +1 | Acknowledgement for `420f7b7`, the resume commit this phase arrived on. The audit was red without one; the file's own contract requires an entry with a reason for every unreviewed-work commit. |

## The finding, reproduced first

```
$ printf 'if 1  is  greater  than  0 then\nsay  "a"\nend\n' > m4.rb
$ rb format m4.rb
if 1 > 0 then
    say "a"
end
$ cat m4.rb
if 1  is  greater  than  0 then
say  "a"
end
```

`rb format <file>` is a filter: it prints the formatted source to stdout and
leaves the file byte-for-byte as it was — the file above still has its doubled
spaces and its unindented body, and the program runs either way, which is the
point: formatting is a change of *shape*, and nothing in the language ever
*wrote* that shape. The REPL had no `.reload` and no `.format` at all:

```
$ printf ':reload\n:format\n:quit\n' | ./target/debug/rb
>>> >>> >>> Goodbye!
```

Both were reported as unknown commands, so the loop was: edit a file in an
editor, re-type its path into `:load` to see the effect. There was no way to
re-run what you had just edited from the prompt that loaded it, and no way to
tidy the file from that prompt at all.

## What the change does

- **`.reload [file]`** — runs a file again *in this session*, so its bindings are
  the session's own (`:run` throws them away, `:load` keeps them but meant
  retyping a path). With no path it reloads the file the session last loaded.
- **`.format [file]`** (alias `:fmt`) — formats a file and **writes it back**,
  with no path acting on the last loaded file. Writes only when
  `formatter::needs_reformat` says the file changes; reports `'x' is already
  formatted.` otherwise. Every failure names the path it happened at — on both
  of `.format`'s two arms and on `.reload`'s, which reports through the same
  `load_file`.
- **Format-on-enter** — when a buffered block balances, the program is normalised
  by the formatter before the session **writes it down**, so what `.export`
  hands a reader is in the shape the formatter writes.

Format-on-enter normalises what a session *records*, and runs what was *typed*.
That split is the sharpest decision in the diff, and it is not cosmetic. Every
span a session reports is computed from the text handed to `execute_code_from`:
the diagnostic `render_error` draws, the line `record_definitions` files a name
under, and the source the debugger shows at a stop. `:history` prints the lines
that were *typed*, so a span computed from the normalised program names a line
of the formatter's output that is in no document the user has — the bug this
diff had before it was fixed, where `if 1 is 1 then say undefined_name end`, the
session's only line, was reported at `<session>:2` quoting a line the user never
wrote, and `.which` on the same kind of line answered `bound at <session>:2`
about a name bound on history line 1. Running what was typed keeps that
invariant; recording the normalised form is where the phase's feature lives.
`edge_a_one_line_block_reports_the_line_that_was_typed` and
`edge_which_names_the_line_a_one_line_block_was_typed_on` pin it.

The guard on the rewrite is the second sharpest decision. The formatter rewrites
**words**, not only layout: `import files, network as net` comes back as
`import files, network to net`. That is meaning-preserving and still not a
formatting concern, and a session that silently rewrote the words a user typed
would write down a program they never ran. `Repl::same_tokens` compares the two
token streams — `kind` *and* `word`, because the lexer gives `give` and `back`
one kind (`TokenKind::GiveBack`) and records the word beside it for exactly this
reason, with `TokenKind::Newline` dropped because it *is* the line break — and a
program whose tokens differ is recorded exactly as it was typed. A program either
side cannot lex is not rewritten at all. `repl::tests::format_on_enter` pins all
three halves: the unfold, the reject and the `word` comparison.

## Tests added

19 new `#[test]` functions in `tests/repl_test.rs`, 17 of them named `edge_*`,
plus a four-test `format_on_enter` unit module in `src/repl/mod.rs` that pins
the guard directly rather than through a session that happens to type the right
thing. One existing assertion was **strengthened, not weakened**:
`edge_load_of_a_missing_path_reports_the_failure` asserted the pathless
`Could not load file:` and now asserts `Could not load '<path>'`, because
`load_file` grew the path when `.reload` needed one too.

| Test | Edge class covered |
|---|---|
| `edge_format_rewrites_the_file_it_was_given` | the finding — the file on disk is the formatter's output, including `is greater than 0` → `>` |
| `edge_reload_reruns_the_file_the_session_last_loaded` | the loop, driven from inside the session with `files.write` so no timing is involved |
| `edge_format_of_an_already_formatted_file_says_it_changed_nothing` | idempotence — a file that is not rewritten must say so |
| `edge_format_of_a_path_that_cannot_be_read_names_the_path_and_the_session_survives` | **failure** — a directory fails at `read_to_string`, so the message names the path and says it was a *read* |
| `edge_format_of_a_file_that_cannot_be_written_names_the_path_and_leaves_it_alone` | **failure** — the `std::fs::write` arm, reached by a mode-`0444` file rather than by a directory, which never gets that far |
| `edge_reload_of_a_path_it_cannot_read_names_the_path_it_failed_at` | **failure** — both of `.reload`'s ways of naming a file it cannot run; the path is in the message |
| `edge_reload_with_nothing_loaded_reports_rather_than_inventing_a_path` | **failure** — an empty session has no file and is told so instead of being handed an invented path |
| `edge_format_with_nothing_loaded_reports_rather_than_inventing_a_path` | **failure** — and does not claim a file was "already formatted" |
| `edge_format_then_reload_agree_about_which_file_the_session_is_working_on` | **boundary** — the file `.reload` acts on is the one `.format` named *last*, whichever arm each format took |
| `edge_format_of_an_already_formatted_file_is_enough_for_a_bare_reload` | **boundary** — the other direction: naming a file is enough, in a session that has loaded nothing |
| `edge_a_one_line_block_reports_the_line_that_was_typed` | **singleton** — a one-line typed block's diagnostic names line 1 and quotes the line that was typed |
| `edge_which_names_the_line_a_one_line_block_was_typed_on` | **singleton** — `.which` names the typed line, which is a line `:history` prints |
| `edge_format_of_a_file_that_does_not_exist_names_the_path` | **failure** — a missing file names the path |
| `edge_format_of_a_file_that_does_not_parse_leaves_it_alone` | **failure** — malformed source; the file is byte-for-byte unchanged |
| `edge_format_of_an_empty_file_says_it_changed_nothing` | **empty** |
| `edge_format_of_a_filename_holding_a_space_is_quoted_not_split` | **unicode-ish / quoting** — the quotes are part of the command's spelling and are opened, not left in the path |
| `edge_tab_after_the_file_loop_commands_offers_paths_and_nowhere_else` | the completer gate: TAB offers a path in `:reload`/`:format`, offers nothing for `:format` with no argument started and nothing for `say "` |
| `reload_of_a_named_path_keeps_the_bindings_it_made` | **singleton** — one file, one binding, and the session still holds it |
| `format_on_enter_records_the_normalised_program_in_the_transcript` | the format-on-enter contract, observed through `.export` |
| `format_on_enter::{edge_a_one_line_block_is_unfolded, edge_an_import_the_formatter_would_rewrite_is_returned_byte_identical, edge_a_program_that_cannot_lex_is_returned_unchanged, edge_same_tokens_reads_the_word_and_not_only_the_kind}` | the **reject** paths, which nothing else reaches: an inverted guard — `code` returned always, `TokenKind::Newline` dropped, `word` dropped, one side unlexable — is a no-op feature with the suite green |

Every failure assertion checks the *path is named* and that the session
survives, not merely that something was printed. `.format`'s write arm is
reached by a file the process may read and not write, and the test decides
which branch it is in by trying that same write itself — uid 0 bypasses the
permission bits, so the assertion there would be meaningless rather than flaky,
which is the pattern `edge_files_unreadable_file_is_an_error_when_unprivileged`
already uses.

**A test was watched failing for the right reason.** Before any production
change, 7 of the 9 then-written tests failed with `Unknown command 'reload'` /
`'format'`; `edge_reload_with_nothing_loaded_reports_rather_than_inventing_a_path`
passed *vacuously* — it only asserted that stdout contained the word "reload",
which the unknown-command message also contains — and was tightened to assert
`this session has loaded no file`, which the unknown-command path cannot produce.

Every test added in this round was then watched failing against the defect it
covers, by putting the defect back: normalising the program *before* it ran
turned `edge_a_one_line_block_reports_the_line_that_was_typed` and
`edge_which_names_the_line_a_one_line_block_was_typed_on` red; moving the
`last_file` write back below the `already formatted` return turned
`edge_format_then_reload_agree_about_which_file_the_session_is_working_on` and
`edge_format_of_an_already_formatted_file_is_enough_for_a_bare_reload` red;
`format_on_enter` returning `code` unconditionally, `significant` keeping the
newlines, and `same_tokens` reading only `kind` each turned one of the four
unit tests red. A test nobody has seen fail is not yet known to be a gate.

No test was weakened, skipped, ignored or suppressed. This run's diff adds no
`#[ignore]`, no `// skip` and no `allow(clippy::`. The only pre-existing test file
touched is the acknowledgement list, which gained one data row, and the one
pre-existing assertion changed is the `:load` failure message described above.

Two tests that did fail after the change were *my* test bugs, not the code's, and
were fixed rather than worked around: `scratch_path` creates the file, so
`create_dir_all` on it raised `AlreadyExists`, and `:say reloaded` is not a
command (`say` is Redblue source). One expectation was wrong about the formatter
— `if 1 is greater than 0 then` becomes `if 1 > 0 then` — and was corrected to
the formatter's real output rather than the output I had assumed.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — no output |
| `cargo clippy --all-targets -- -D warnings` | **pass** — `Finished dev profile`, no warning or error |
| `cargo test --all-targets` | **pass** — 1859 passed, 0 failed, 0 ignored |
| `./rbops/verify.sh phase-060` | **NOT RUN — `rbops/` does not exist in this checkout** |

```
$ ls rbops
ls: cannot access 'rbops': No such file or directory
$ find . -maxdepth 3 -name "verify*.sh" -not -path "./target/*"
(no output)
```

The fourth gate could not be executed: there is no `rbops/verify.sh` in the tree
this phase was given, and `find` finds no verify script anywhere. This report
does not claim it passed. `.github/workflows/ci.yml` runs
`cargo fmt --all -- --check`, `cargo clippy -- -D warnings` and the test suite —
the first three gates above — and those are green. See F1 in `FINDINGS.md`,
recorded identically by phases 057, 058 and 059.

`cargo test --doc` was run as well (2 passed), and backwards compatibility was
checked by hand: every file in `examples/` run through `rb run` with no error
output.

## Invariants touched

- **A span a session reports is a line of `:history`.** `current_line_number` is
  the history count and `record_definitions` / `render_error` / the debugger's
  `set_source` are all read off the text `execute_code_from` was given, so a
  diagnostic's `<session>:line:column`, the line `.which` files a name under,
  and the source shown at a stop are all checkable against `:history`. This
  diff **nearly broke it**: format-on-enter substituted the reformatted program
  for the typed one before `execute_code_from` ran, so a one-line block was
  reported at the line of the *next* line the user typed, quoting a line of the
  formatter's output that is in no document at all. What was typed is what runs
  now, and the normalised form is only what is written down.
- **`last_file` means one thing.** It is the file the session last *named or
  loaded*, and both `.reload` and `.format` act on it when given no path. This
  diff added it, and its two write sites disagreed: `format_file` set it on the
  write but not on the `already formatted` return, so `:format A.rb`,
  `:format B.rb`, `:reload` re-ran `A.rb`. It is set once now, before the
  `needs_reformat` return.
- No `Value` variant, no `Error` variant, no grammar change, no change
  to `to … end` / `set x to` / `say`, no change to the `.rb` extension.
- The REPL gained two commands. `:help` is built from `COMMAND_TABLE`, so both
  appear there without a second list.
- `edge_only_the_commands_that_take_a_path_take_a_path` was **extended**, not
  weakened: its exact list grew from 8 entries to 11 (`fmt`, `format`, `reload`
  and nothing else), and it still fails if a command that does not take a path
  starts taking one.
- `rb format <file>` is unchanged and still prints to stdout. `:format` is a new
  command beside it, not a change to it — see F8.

## Edge-case matrix (AGENTS.md §3.2)

| Row | Status | Where |
|---|---|---|
| empty | covered | `edge_format_of_an_empty_file_says_it_changed_nothing`; `.format`/`.reload` with no argument and no file loaded (`edge_format_with_nothing_loaded_…`, `edge_reload_with_nothing_loaded_…`) |
| singleton | covered | `reload_of_a_named_path_keeps_the_bindings_it_made` — one file, one binding, still held after the run; `edge_a_one_line_block_reports_the_line_that_was_typed` — one typed line, one program, one line number |
| boundary | covered | `edge_format_of_an_already_formatted_file_says_it_changed_nothing` and `edge_format_then_reload_agree_about_which_file_the_session_is_working_on` — the `needs_reformat` boundary, on both arms, and which side of it `.reload` lands on; a path with one space in it |
| out_of_bounds | covered | `edge_format_of_a_file_that_does_not_exist_names_the_path` and `edge_reload_of_a_path_it_cannot_read_names_the_path_it_failed_at` — a path past the end of the filesystem is a reported error naming the path, not a panic; `edge_tab_after_the_file_loop_commands_offers_paths_and_nowhere_else` offers nothing rather than failing when there is no argument |
| type_mismatch | covered | `edge_format_of_a_path_that_cannot_be_read_…` — a path that names a directory is not a file; `edge_format_of_a_file_that_cannot_be_written_…` — a path that names a readable file the session may not overwrite; `edge_format_of_a_file_that_does_not_parse_leaves_it_alone` — text that is not a program |
| numeric_boundary | **N/A** | `.format`, `.reload` and format-on-enter act on text and paths; none parses a number. The formatter's own arithmetic is over indentation levels and line counts, exercised by `edge_format_rewrites_the_file_it_was_given` (a nested block) and unchanged by this diff. The language's numeric boundaries are covered by `tests/numeric_edge_test.rs`. |
| unicode | covered | `edge_format_of_a_filename_holding_a_space_is_quoted_not_split` — a path with characters that would split a naive tokeniser. `Repl::same_tokens` compares `word` and `kind` but never byte offsets, so a multi-byte character cannot make two equal streams compare unequal. The pre-existing `edge_a_session_of_unicode_exports_and_reads_back_unchanged` still passes. |
| nesting_recursion | **N/A** | None of the three commands recurses. `edge_format_rewrites_the_file_it_was_given` formats a nested `if`/`end` block, and format-on-enter runs *after* a block balances (`execute_line` returns early while `needs_continuation` holds), so a nested block is unfolded once and never re-entered. The recursion the session itself has — a `repeat` block buffered line by line — is covered by the pre-existing `a_block_is_one_exchange_holding_every_line_it_was_typed_on`. |
| duplicate_missing_keys | **N/A** | Records are a runtime value type; a formatter rewrites source text and a completer answers a prefix. Neither reads a record's keys. The nearest property — a file that does not exist, and a file with no content — is covered above. |
| malformed_input | covered | `edge_format_of_a_file_that_does_not_parse_leaves_it_alone` — a formatter that half-wrote a file it could not read would destroy the source it was asked to tidy; `edge_a_program_that_cannot_lex_is_returned_unchanged` — the same class one level down, where format-on-enter refuses to rewrite a program it cannot compare |
| resource_limit | covered | `.format` reads one file and writes one file: the work is bounded by the file's own size, which is the bound the file imposes. Format-on-enter runs on a buffer that has already passed `MAX_BLOCK_LINES`/`MAX_BLOCK_BYTES` (`exceeds_block_limits`), so it cannot grow a buffer past the guard; `Repl::same_tokens` lexes twice more on top, over a buffer already bounded. |

## Known gaps / follow-ups

- **`rbops/verify.sh` could not be run** — see the Gates table and F1 in
  `FINDINGS.md`. Nothing in this report claims that gate's result.
- The formatter rewrites **words** as well as layout (`as` → `to` on an import),
  and `docs/GRAMMAR.md` does not say which spelling it emits. F7.
- `rb format <file>` still prints and does not write; `:format` is the command
  that writes. Whether the CLI needs a `-w` is a decision, not a bug. F8.
- `:reload` re-runs a file but does not reset the session first, so a name the
  previous run bound and this one does not survives. `:load` behaves the same
  way and this phase did not change it.
- The pre-existing F2 (a filename inside a Redblue program is typed blind), F5
  (`:run` output is not in the transcript) and F6 (TAB does not complete inside
  an opening quote) are unchanged.

## Re-verification of the phase's original finding

The prompt's finding — no `.export` in `commands.rs`, and the completer handling
`:`/`.` sigils with no path handling — no longer reproduces: `.export` writes
Markdown to stdout or a file, and TAB offers filesystem paths, both verified on
this tree and both green under `cargo test`. It was not "fixed" by inventing a
change. What did reproduce, in the same code and reached from the same
definition of done (a file you can write, read back and re-run), is that the loop
had no last half: nothing wrote a formatted file and nothing re-ran one. That is
what this run fixed.