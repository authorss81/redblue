# Phase 060 — Export sessions and complete file paths: `.export` plus TAB for `:load`/`:save`

## Reproduce (before the change)

The finding's first half, on this tree, before anything was touched:

```
$ printf 'set x to 6 * 7\nsay "hi"\n.export session.md\n:quit\n' | ./target/debug/rb
...
>>> hi
>>> Unknown command 'export'. Type :help for available commands.
>>> Goodbye!

$ ls session.md
ls: cannot access 'session.md': No such file or directory
```

The finding's second half, as a red test against the completer that already
existed (`src/repl/editor.rs`, `EditSource::complete`, no terminal needed):

```
$ cargo test --lib repl::editor::tests::edge_tab_after_a_command_that_takes_a_path_offers_paths
---- edge_tab_after_a_command_that_takes_a_path_offers_paths stdout ----
a directory is offered with the separator that means it is one:
    ["say", "set", "sin", "skip", "split", "sqrt", "sys"]
```

That is `:load s` answered from the keyword and builtin tables, with the
filesystem nowhere in it: `SessionCompleter::word_at` split the line on
whitespace, `complete_with("s")` matched the vocabulary, and no code anywhere in
`src/repl/` had heard of a directory. Both halves reproduced, so the finding was
not stale.

After (every transcript below re-run on this tree as it now stands):

```
$ printf 'say "one"\nrepeat 2 times\n    say "tick"\nend\nsay nosuchvar\nsay 6 * 7 + 1\n.export\n:quit\n' \
    | REDBLUE_COLOR=always ./target/debug/rb | sed -n '/# Redblue session/,$p' | cat -v
# Redblue session

```redblue
say "one"
```
one

```redblue
repeat 2 times
say "tick"
end
```
tick
tick

```redblue
say nosuchvar
```
Error: AnalyzerError: Unknown variable 'nosuchvar'
  --> <session>:1:5
1 | say nosuchvar
  |     ^

```redblue
say 6 * 7 + 1
```
43
```

Run with `REDBLUE_COLOR=always` and piped through `cat -v`: the only escape
sequences in the whole session are the prompt and `Goodbye!`, both of which are
outside the document. `:help` gained one line from the table:
`:export <filename>  Write the session as Markdown; no filename writes it here`.

## What changed

`must_touch: ["src/"]` is satisfied: five files under `src/` changed and one was
added — **+1450 −10** under `src/`, out of **+1701 −10** over the whole phase (7
files), from `git diff --shortstat` on this tree.

| File | Lines | What |
|---|---|---|
| `src/repl/transcript.rs` | +560 (new) | The document. `Exchange` (what was typed, what it said) and `Transcript` (the session's exchanges, bounded at `LIMIT` = 500, oldest dropped). `markdown()` writes the `redblue` fence and text blocks and strips escapes on the way out; `read_markdown()` is its other half and is what the round trip is tested through; `fence_width`/`fence_language` widen a fence to survive an input that prints one. 13 tests. |
| `src/repl/completer.rs` | +447 −1 | `path_argument` (where the cursor is, when it is in a path argument), `split_fragment`, `complete_path_in` (a named directory rather than the process's, so a test can use a scratch one), `sort_key`, `quote_if_spaced`, `is_directory` (follows a symlink), `current_dir`, and `ReplCompleter::complete_path`. `MAX_PATH_CANDIDATES` = 200. 8 tests. |
| `src/repl/commands.rs` | +181 −6 | `ReplCommand::Export(Option<String>)` and its row in `COMMAND_TABLE`; `FILE_USAGE`, the `<filename>` placeholder that says which commands take a path; `ReplCommand::takes_a_file_path`; `ReplCommand::unquote_path`. 2 tests, plus 4 rows in `parse_reads_the_whole_alias_table`. |
| `src/repl/mod.rs` | +82 −3 | `Repl::transcript` and `Repl::last_output`; `execute_code_from` records one exchange per line run; `run_program` keeps what the VM said before it is printed; `export_session`; the `ReplCommand::Export` arm; `:reset` clears the transcript with the bindings. |
| `src/repl/style.rs` | +106 | `strip_escapes` — `ESC [ … @…~` and a bare escape byte, which is what a document is made of. 3 tests. |
| `src/repl/editor.rs` | +74 | `SessionCompleter::complete` asks for a path first, and falls back to the vocabulary when the cursor is not in one. 2 tests. |
| `tests/repl_test.rs` | +251 | End to end, through the `rb` binary: 5 tests, with the extraction helpers (`plain`, `exported_document`, `fenced_blocks`). |

Three decisions in that diff are worth their reasoning here, because each of them
is a choice a reviewer should be able to check.

**Where the output text comes from.** The VM prints `say` output itself
(`Vm::run`), and the session printed the `= value` line and the rendered failure
itself, so there was no one place holding a line's output as text. `run_program`
now takes the VM's buffer into `last_output` before printing it on the error path
— the same lines, read once more — and `execute_code_from` assembles the
exchange's output from that plus the `=` line plus the rendered failure. Both are
stored already stripped (`strip_escapes`), and `markdown()` strips again, so a
future painted string recorded here cannot leak into a document either.

**Why `FILE_USAGE` rather than a list.** `takes_a_file_path` asks the command
table whether the entry's usage is the `<filename>` placeholder. That is one
answer to "which commands take a path" rather than two, so a sixth such command
says so by declaring the same thing, and it is what
`edge_only_the_commands_that_take_a_path_take_a_path` pins — including the half
that is easy to forget, that a sigil is required, because `load x` with no sigil
is Redblue source calling a function called `load`, not a command.

**Why `.export` is in the path commands.** The DoD names four commands;
`.export`'s argument is a filename by the same declaration as theirs, and a
completer that completes `:load` but not `.export` would be arbitrary. It does
not weaken the gate the DoD asks for: `say "a` still offers nothing, and the
predicate — not a list of four — is what decides.

## Tests added

33 new `#[test]` functions, 21 of them named `edge_*`, plus 4 rows added to the
existing alias table.

| Test | Edge class covered |
|---|---|
| `transcript::an_exported_session_is_markdown_with_one_redblue_block_per_line` | the shape the phase is about: one `redblue` fence per line run, each holding what was typed, with what it said after it |
| `transcript::a_session_read_back_from_its_own_markdown_is_the_session` | round trip, unit half — write then read, no terminal |
| `transcript::edge_no_escape_sequence_can_reach_the_document` | escapes — a coloured result and a coloured failure both reach the document with the text intact and no `ESC` |
| `transcript::edge_a_line_that_prints_a_fence_is_written_inside_a_longer_one` | escapes / malformed input — `say "```"` closes the block it is written inside unless the fence is widened; the widened block still reads back |
| `transcript::edge_a_session_that_has_run_nothing_says_so` | empty — the document is a heading and a sentence, no fence, nothing to read back |
| `transcript::edge_a_long_session_keeps_the_newest_exchanges_and_no_more` | resource — 505 exchanges in, 500 out, the oldest gone, the document bounded with it |
| `transcript::edge_a_cleared_transcript_exports_an_empty_session` | state — `:reset` clears the record, so an export after it is a session that ran nothing |
| `transcript::a_block_is_one_exchange_holding_every_line_it_was_typed_on` | nesting — a block is one exchange, indentation and all, not one per line |
| `transcript::an_exchange_that_said_nothing_writes_no_output_block` | boundary — the whole document, line for line, for a `set` followed by a `say`: no phantom paragraph |
| `transcript::edge_the_fence_is_one_backtick_longer_than_the_longest_run_inside_it` | boundary — `fence_width` at 0, 1, 2, 3 backticks and two separate runs |
| `transcript::a_document_written_by_hand_is_read_the_same_way` | malformed input — a `rust` block is not read, a widened block is, text outside a block is not, and a fence inside a widened block is content |
| `transcript::edge_an_unclosed_fence_at_the_end_of_a_document_reads_as_nothing` | malformed input — a truncated export does not read back as half a line of Redblue |
| `transcript::an_exchange_keeps_its_input_apart_from_its_output` | the `Exchange` pair the renderer reads apart |
| `completer::a_file_command_completes_the_paths_it_could_load` | the finding — `sr` → `src/`, `no` → `notes.txt`, `zzz` → nothing, in a scratch directory |
| `completer::an_empty_fragment_offers_the_directory_and_replaces_only_the_argument` | empty / boundary — `:load ` at 6, `:load src/li` at 12, `.save out.txt` at 13: the offset the replacement starts at, not just the candidates |
| `completer::edge_a_filename_with_a_space_in_it_is_offered_quoted` | the DoD's "never split" — `my file.rb` is offered `"my file.rb"`, `plain.rb` unquoted |
| `completer::edge_nothing_to_offer_is_no_candidates_rather_than_a_failure` | empty / out of bounds / malformed — an empty directory, a prefix nothing matches, a directory that does not exist, a path that is a file, `pos` past the end of the line, `pos` inside a character |
| `completer::edge_a_dot_entry_is_not_offered` | state — `.hidden` is not offered, and the file is still there |
| `completer::edge_a_long_listing_is_capped_and_the_order_does_not_depend_on_the_disk` | resource / determinism — 210 entries, 200 offered, the same list twice, the cut is the first of the sorted list rather than readdir order |
| `completer::a_symlink_to_a_directory_is_offered_as_a_directory` | resource — the link is followed, so the next TAB walks into it (`#[cfg(unix)]`, with the other branch asserted) |
| `completer::a_path_is_offered_only_in_the_argument_of_a_command_that_takes_one` | the gate — twelve lines at given positions, including `:loadx s`, `::load s`, bare `load s` and `say "s` |
| `editor::edge_tab_after_a_command_that_takes_a_path_offers_paths` | end to end through `EditSource`: `:load `, `:load s`, `:load sr` all offer `src/`; `:load ` offers a file; `sr` narrows; and `say`/`:save` are *not* offered in the argument |
| `editor::edge_tab_outside_a_file_argument_offers_no_path` | the other side of the gate — `say "a`, `set x to "s`, `sr`, `say "src/` offer nothing |
| `commands::edge_export_is_a_command_whose_path_may_be_left_out` | `.export p` → `Export(Some(..))`, `.export` and `.export␣␣␣` → `Export(None)`, and it is offered by the completer |
| `commands::edge_only_the_commands_that_take_a_path_take_a_path` | the predicate — the five commands and their eight words, and eleven lines that are or are not path arguments |
| `style::stripping_a_painted_string_gives_the_text_back` | all fourteen paint roles round-trip |
| `style::edge_stripping_a_plain_string_changes_nothing` | empty — the empty string, indentation, a tab, and a disabled style painting nothing |
| `style::edge_an_escape_that_is_not_a_paint_sequence_leaves_only_text` | escapes — a bare escape byte, `[2J[H`, and the backticks of a fence |
| `repl_test::edge_export_writes_a_two_line_session_as_two_fenced_markdown_blocks` | the DoD's first bullet, through the binary: exactly two ` ```redblue ` fences, the blocks line for line, `one` and `two` as text blocks, no `ESC`, no prompt |
| `repl_test::edge_a_session_exported_to_a_file_runs_again_from_the_fences_read_back` | the DoD's round trip: export → file → fences read back → a fresh session says `one` and `two` once each |
| `repl_test::edge_export_to_a_path_that_cannot_be_written_names_the_path` | **failure**: `/no-such-directory-for-export/session.md` → `Could not export … <path>`, and nothing is left at the path |
| `repl_test::edge_export_of_a_session_that_has_run_nothing_says_so` | empty, through the binary |
| `repl_test::edge_a_quoted_path_with_a_space_in_it_is_the_file_it_names` | the form TAB offers is a form `:load` opens |
| `commands::tests::parse_reads_the_whole_alias_table` (4 rows added) | `:export`, `:export␣␣␣`, `:export session.md`, and `:load "my file.rb"` → `my file.rb` while `:prompt "rb> "` keeps its quotes |

Failure asserted: `edge_export_to_a_path_that_cannot_be_written_names_the_path`
(a clean error naming the path, and no file created), plus
`edge_nothing_to_offer_is_no_candidates_rather_than_a_failure` (`None` for a
cursor past the end and inside a character) and
`edge_an_escape_that_is_not_a_paint_sequence_leaves_only_text`.

No test was weakened, skipped, ignored or suppressed: `git diff` over this phase
contains no `#[ignore]`, no `// skip` and no `allow(clippy::`. No pre-existing
test changed except by gaining rows.

Every colour assertion strips escapes before asserting
(`repl_test::plain`) or compares against `Style::forced(…)`, so changing the
palette cannot fail this suite. Every path-completion unit test runs in a scratch
directory under `target/tmp/` created and emptied by the test itself, so nothing
depends on what the checkout holds — the two editor tests are the exception and
they read the checkout deliberately, asserting `contains("src/")` rather than an
exact list.

## Edge-case matrix

| Row | Covered by |
|---|---|
| empty / zero / nothing | `transcript::edge_a_session_that_has_run_nothing_says_so` and `repl_test::edge_export_of_a_session_that_has_run_nothing_says_so` (nothing run, nothing fenced), `completer::edge_nothing_to_offer_…` (an empty directory), `completer::an_empty_fragment_…` (`:load ` offers the whole directory), `style::edge_stripping_a_plain_string_changes_nothing` (the empty string), `transcript::an_exchange_that_said_nothing_writes_no_output_block` (a `set` says nothing, so no output block) |
| singleton and boundary | `transcript::an_exported_session_is_markdown_with_one_redblue_block_per_line` (a two-line session, each block one line), `completer::an_empty_fragment_…` (offsets 6, 12, 13 — the boundary is the argument start, not the line), `transcript::edge_the_fence_is_…` (0–3 backticks), `repl_test::edge_a_quoted_path_…` (one spaced name) |
| out of bounds | `completer::edge_nothing_to_offer_…`: `path_argument(":load ", 1000)` is `None` (the cursor is past the end of the line) and `path_argument(":loa\u{1F600}", 4)` is `None` (inside a character) — neither panics nor slices mid-character. `style::strip_escapes` walks chars, not bytes, and the transcript has no indexing at all. The argument index out of bounds — `read_markdown` over an input with no fences — is `a_document_written_by_hand_…` and `edge_an_unclosed_fence_…` |
| type mismatch | The path logic is reached by a *command word*, not by a guess: `completer::a_path_is_offered_only_…` covers the mismatches — `:loadx s` (a command that does not exist), `::load s` (two sigils), bare `load s` (source, not a command) and `say "s` (a string) all get no path. In the transcript, an input that is a block is not a line and an output that is a multi-line failure is not a line either: `a_block_is_one_exchange_…` and the `nosuchvar` case in the transcript above |
| numeric boundary | N/A — no number is computed. The one number a user reads is `Exported N exchange(s)`, which is `Transcript::len()`, pinned by `repl_test::edge_a_session_exported_to_a_file_…` at 2 and by `edge_a_long_session_keeps_the_newest_…` at `LIMIT`. `0/0`, `±Infinity`, `2^53±1` raise no branch here: a value reaches a document as the text `format!("= {}", value)` produced, the same text the terminal printed |
| unicode and escapes | **the row this phase is mostly about**: `style::stripping_a_painted_string_…` (every paint role), `style::edge_an_escape_that_is_not_a_paint_sequence_leaves_only_text` (a bare `ESC`, `[2J[H`, and backticks), `transcript::edge_no_escape_sequence_can_reach_the_document`, `repl_test::edge_export_writes_…` (asserts no `ESC` in the document with `REDBLUE_COLOR` on by construction, and strips escapes before asserting anyway), `completer::a_symlink_…` (`#[cfg(unix)]`). Non-UTF-8 filenames are skipped rather than offered — `complete_path_in` asks for `to_str()` and a name that is not text cannot be typed; non-UTF-8 *source* is N/A, unchanged from before |
| nesting and recursion | `transcript::a_block_is_one_exchange_holding_every_line_it_was_typed_on` (a block nested three lines is one exchange and reads back whole), `completer::an_empty_fragment_…` (a path walked into, `src/li`), `repl_test::edge_export_writes_…` (two exchanges, two blocks, two text blocks, in order). N/A beyond that: `markdown` and `read_markdown` walk a flat list and never recurse, and the completer reads one directory level at a time, so neither can overflow a stack |
| duplicate and missing keys | N/A — no record is read or written. The nearest shape is two lines with the same text: `repl_test::edge_export_writes_…` fences each of them separately, and `Transcript::read_markdown` reads each block separately, so a session that ran `say "one"` twice exports two blocks and reads back two lines. A repeated name in a *source* line is copied verbatim (`an_exchange_keeps_its_input_apart_from_its_output`) |
| malformed input | `transcript::a_document_written_by_hand_is_read_the_same_way` (a block of another language, a widened fence, text outside a block, a fence inside a block), `transcript::edge_an_unclosed_fence_…` (a truncated export), `transcript::edge_a_line_that_prints_a_fence_…`, `completer::edge_nothing_to_offer_…` (`pos` past the line, `pos` mid-character, a path that is a file, a directory that is not there), `completer::a_path_is_offered_only_…` (`::load`, `:loadx`, bare `load`) |
| resource and state | Two limits, both asserted: `Transcript::LIMIT` = 500 by `edge_a_long_session_keeps_the_newest_exchanges_and_no_more` (505 in, 500 out, the oldest dropped, the document bounded with it) and `MAX_PATH_CANDIDATES` = 200 by `edge_a_long_listing_is_capped_…`. The transcript is cleared by `:reset` (`edge_a_cleared_transcript_exports_an_empty_session`). A directory that cannot be read costs one `read_dir` and offers nothing; a path whose type cannot be read is offered as a file (`is_directory`), which is the reading that does least. The export writes one file and, on failure, none (`edge_export_to_a_path_that_cannot_be_written_…` asserts the path is absent afterwards). The VM's own `MAX_OUTPUT_BYTES` still bounds what one line can say |

## Gates

Run in this checkout, in this order, on the tree as it now stands:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | **1781 passed, 0 failed, 62 test binaries** (log: `target/tmp/phase060-test.log`) |
| `cargo test --doc` | 2 passed, 0 failed |
| `./rbops/verify.sh phase-060` | **could not run — `rbops/` is not in this checkout** |

`ls rbops` → *No such file or directory*, exactly as phases 057, 058 and 059
recorded. I did not create, inspect or work around it; the four cargo gates above
were run here and `./rbops/verify.sh` will have to run where it exists. See
FINDINGS.md F1.

Re-verified by hand on this tree, not taken from the test output: `examples/*.rb`
(7 files) and `modules/*.rb` (2 files) each through `rb run`, all exit 0; the
export above, under `REDBLUE_COLOR=always`, with `cat -v`.

## Invariants touched

- None of the language. No `Value` variant, no `Error` variant, no keyword, no
  `to … end` / `set x to <expr>` / `say` form, no `.rb` extension. The parser,
  analyzer, interpreter, bytecode VM and stdlib are untouched apart from the
  comment on `Vm::run`'s output buffer that the new code relies on.
- **One new REPL command, in the one table.** `.export <filename>` is a new word
  in `COMMAND_TABLE`, which is the same place `:quit` and `:load` live, so it is
  parsed, completed and documented by the three things that are already derived
  from it. `:help` gained a line because the help is built from the table; nothing
  about any existing command's behaviour changed.
- **`export` was already a language keyword.** A bare `export` is the statement
  that publishes a module member, so `.export` (with a sigil) is the command and
  `export x` is still source — `printf 'export\n' | rb` reports
  `ParserError: Expected a name to export after 'export'`, as it did before this
  phase. Both sigils reach the command, as for every other.
- **A quoted path now parses to the path inside the quotes**, for the five
  commands whose argument is a filename and for no other: `:load "my file.rb"`
  is `Load("my file.rb")` where it was `Load("\"my file.rb\"")`. `:prompt "rb> "`
  keeps its quotes (display text), and every unquoted path is untouched — the
  existing `edge_a_command_argument_with_a_space_in_it_is_not_truncated` and the
  `save`/`restore` round trip both pass unmodified. The reason it is here: TAB
  offers a spaced name *quoted*, and a completer that offers a name the command
  cannot open is a completer that lies.
- REPL output for a line is unchanged: `last_output` holds the same lines
  `run_program` already printed, and is drained afterwards.

## Known gaps / follow-ups

- **A filename whose name really starts and ends with `"` is unreachable through a
  command**, because `unquote_path` removes that pair. A file whose name starts or
  ends with a quote *without* the other one is fine, and typing it unquoted is
  fine; only the matched pair is affected. This is the trade every shell makes and
  it is documented on the function, but it is a trade.
- **A name with a space is offered quoted, and the quotes are typed for you.** The
  alternative — offering it unquoted — would hand `:load` two arguments, so this is
  the honest choice; but it means the user sees `"my file.rb"` in the line where a
  Unix user might expect `my\ file.rb`. Both are accepted today; only the first is
  offered, because only the first round-trips through this parser.
- **`.export` records what the session ran, not what it was told.** REPL commands
  are not exchanges: `:vars`, `:help` and `:save` do not appear in a transcript,
  for the same reason `restore_session` does not re-run them (they are about the
  session rather than its bindings). A reader wanting the typed lines has `:save`.
- **The transcript holds 500 exchanges and the history 1000 lines.** A session
  longer than 500 exports its last 500 lines and says nothing about what was left
  out — the alternative is a document nobody can read. The count is printed by
  `Exported N exchange(s)` when a file is written, and is not printed when the
  document goes to stdout, where it would be prose inside the document.
- **Dot-entries are not completed.** `.env` and `.git` must be typed whole. A
  `show hidden` switch for the completer is a follow-up if anyone asks for one.
- **`complete_path_in` reads a whole directory and sorts it** before truncating
  the list. A directory of a million entries costs a million `readdir` calls, which
  is what every shell completer does and is the price of an order that does not
  depend on the filesystem. It happens on a TAB keystroke, in the read loop, and no
  completion so far touches the disk at all.
- **`rustdoc` warnings in `src/repl/commands.rs:458` and `:492`** (`bare_names`,
  `COMMAND_TABLE`) predate this phase — they are links from public docs to private
  items and are not new. `cargo doc` is not one of the four gates.
- `rbops/verify.sh` still cannot run here — FINDINGS.md F1.
