# Phase 052 — Wire the REPL to a real line editor: TAB completion, persistent history, sane interrupts

## Finding, reproduced first

```
$ printf 'set foo to 1\nfo\t\nset bar to to_upper\n\nquit\n' | cargo run --quiet --bin rb
Welcome to Redblue v0.1.0 - Interactive REPL
Type 'help' for commands, 'quit' to exit

>>> >>> Error: AnalyzerError: Unknown variable 'fo'; did you mean 'foo'?
  --> 1:1
>>> Error: AnalyzerError: Unknown variable 'to_upper'
  --> 1:1
>>> >>> Error: AnalyzerError: Unknown variable 'quit'
  --> 1:1
>>> Goodbye!
```

The session has `foo` bound. TAB did not offer it: `io::stdin().read_line`
(`src/repl/mod.rs:274` before this change) is cooked-mode stdin, so no keystroke
reaches the program at all — the tab arrives as whitespace inside a finished
line. `Repl::complete()` was a live, tested engine nothing called, and
`~/.redblue/history` was never loaded or written.

## What changed

| File | Lines | What |
|---|---|---|
| `Cargo.toml` | +1 | `rustyline = "18"`, the one line-editing dependency |
| `Cargo.lock` | +107 | pins `rustyline 18.0.1` and its 12 transitive packages from crates.io |
| `src/repl/editor.rs` | +635 (new) | `LineSource`, `Line`, `PlainSource`, `EditSource`, the `ReplCompleter` adapter, `~/.redblue/history` |
| `src/repl/mod.rs` | +239 −16 | the `read_line` loop becomes `Repl::session_loop`, which reads a `Line` from any source; `run` picks the source from the terminal and prints `farewell(SessionEnd)` |
| `src/repl/completer.rs` | +1 | `#[derive(Clone)]` so the editor and the test hook can hold the same completer |

The seam is one trait, `LineSource`. `PlainSource` is the reader a session with
no terminal gets and is the old code verbatim; `EditSource` is a `rustyline`
editor for a terminal. Nothing below the seam changed: a `Line::Text` is still
trimmed, recorded in the history and dispatched through the unchanged
`handle_command` / `execute_line` path.

`run` chooses with `io::stdin().is_terminal()`, so `printf … | rb`, CI and every
other non-terminal session take the `PlainSource` branch and see byte-identical
output (verified by re-running the command above after the change: unchanged).

## Tests added

19 new `#[test]` functions, 8 of them `edge_*`.

| Test | Edge class covered |
|---|---|
| `tab_completes_a_builtin_and_only_the_names_the_session_has_bound` | singleton / boundary — `upper` → `uppercase`; `to_up` → only a name the session has bound |
| `completion_looks_at_the_word_under_the_cursor_and_not_the_whole_line` | boundary — `say coun` starts at byte 4, not 0 |
| `edge_tab_on_an_empty_prefix_offers_the_whole_vocabulary` | empty — the empty prefix offers keywords *and* the `:commands` |
| `edge_tab_with_no_match_offers_nothing_and_does_not_panic` | empty / out of bounds — no match is no candidates, not a panic |
| `edge_completion_inside_a_multi_byte_character_offers_nothing` | unicode — cursor at byte 5 of `say 😀x` returns nothing instead of slicing mid-character |
| `a_word_after_a_unicode_space_is_completed_from_its_own_start` | unicode — `say coun` starts at byte 5, not `index + 1` |
| `history_survives_a_restart_of_the_session` | resource/state — write a line, drop the session, build a new one on the same file, assert the line is offered |
| `the_history_directory_is_created_by_the_session_that_writes_to_it` | first run — `~/.redblue` does not exist until the first save |
| `edge_a_history_file_that_does_not_exist_yet_is_not_an_error` | resource/state — a missing history file is the ordinary first run |
| `edge_a_history_file_that_is_not_writable_is_reported_rather_than_lost` | **asserts a failure** — `save` returns `Err(AlreadyExists)`, naming why |
| `a_plain_source_has_no_history_file_and_never_fails_to_flush` | piped input still has nothing to flush |
| `the_default_history_path_is_under_the_home_directory` | the path is `<home>/.redblue/history` |
| `the_editor_maps_the_terminals_interrupts_onto_what_the_session_means` | **asserts a failure** — `Interrupted` is neither a line nor the end; an I/O error is an `Err` |
| `interrupt_abandons_the_line_and_keeps_the_session_alive` | **Ctrl-C** — a half-typed line is dropped, the session runs the next line |
| `edge_control_d_ends_the_session_with_the_farewell_it_always_printed` | **Ctrl-D** — `SessionEnd::Eof`, and the farewell is `Goodbye!` for all three endings |
| `edge_a_read_that_fails_ends_the_session_and_is_reported_as_a_failure` | **asserts a failure** — `Err` from the source ends the session as `InputError`, records nothing |
| `edge_an_unclosed_block_is_still_reported_at_end_of_input` | malformed input — the old EOF behaviour survives the new loop |
| `the_read_loop_trims_and_skips_a_blank_line_exactly_as_the_pipe_did` | empty + singleton — a blank line runs nothing; `"  set padded to 5  "` runs trimmed |
| `a_session_restores_what_a_previous_one_typed_and_not_the_history_files_header` | malformed input — the editor's history file carries a `#V2` version header; `:history` must not list it as a line someone typed |

The session-level tests drive the loop through a `ScriptedSource` that returns
`Line::Text` / `Line::Interrupted` / `Line::Eof` / `Err` on demand, so Ctrl-C,
Ctrl-D and a failed read are pinned without a terminal.

### Also verified by hand, on a real pty

`target/debug/rb` driven through `pty.fork()`:

```
  set foo to 1           -> 'set foo to 1'
  say fo + TAB           -> '>>> say foo'          # completed, then ran: printed 1
  half typed line        -> 'say "unfi'
  CTRL-C                 -> new prompt, session alive
  after ctrl-c           -> 'alive'                # the next line ran
  CTRL-D                 -> 'Goodbye!'
history file: ~/.redblue/history = ['#V2', 'set foo to 1', 'say foo', 'say "alive"']
=== second session ===
  up-arrow               -> '>>> say "alive"'      # history survived the restart
```

The abandoned `say "unfi` is not in the history file. A second session's
`:history` lists `set foo to 1`, `say foo`, `say "alive"` and no `#V2`.

Two things the pty run caught that the unit tests could not, both fixed here:
`Editor::with_config` starts with **no helper**, so TAB did nothing at all until
`set_helper` was called (`src/repl/editor.rs:206`), and the first `save` failed
with `No such file or directory` because `~/.redblue` did not exist until a
session had something to write (`EditSource::save` now creates it, pinned by
`the_history_directory_is_created_by_the_session_that_writes_to_it`).

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test --all-targets` | 1486 passed, 0 failed, 0 ignored |
| `cargo test --doc` | 2 passed, 0 failed |
| `./rbops/verify.sh phase-052` | **not run** — `rbops/` is not present in this checkout (the pipeline that invoked this run keeps it elsewhere, and the phase instructions say not to go looking for it). The four commands above are the three gates I could run, plus the documented doc-test run. This phase must be re-gated by `verify.sh` before it is `.done`. |

## Test requirements — rows ticked

- **empty** — covered: `edge_tab_on_an_empty_prefix_offers_the_whole_vocabulary`,
  `the_read_loop_trims_and_skips_a_blank_line_exactly_as_the_pipe_did`.
- **singleton** — covered: `tab_completes_a_builtin_and_only_the_names_the_session_has_bound`
  (exactly one candidate), `history_survives_a_restart_of_the_session` (one entry).
- **boundary** — covered: `completion_looks_at_the_word_under_the_cursor_and_not_the_whole_line`
  (start offset 4 at the last word), `the_default_history_path_is_under_the_home_directory`.
- **out_of_bounds** — covered: `edge_completion_inside_a_multi_byte_character_offers_nothing`
  (cursor past the end of the word / inside a character), `edge_tab_with_no_match_offers_nothing_and_does_not_panic`.
  There is no indexing to go out of bounds on: the completer slices with `str::get`,
  which yields `None` rather than panicking, and that `None` path is the asserted one.
- **type_mismatch** — N/A. This change moves lines between an `io::Result` and a
  `Line`, and reads a path from `HOME`. There is no runtime value type in the new
  code to mismatch; `line_from` returning `Err` for a non-interrupt, non-Eof
  reader failure is the closest thing and is covered by
  `the_editor_maps_the_terminals_interrupts_onto_what_the_session_means`.
- **numeric_boundary** — N/A. No arithmetic was added. The only number in the new
  code is a byte offset, covered under out_of_bounds and unicode.
- **unicode** — covered: `edge_completion_inside_a_multi_byte_character_offers_nothing`,
  `a_word_after_a_unicode_space_is_completed_from_its_own_start` (a `U+00A0` before
  the word, which `index + 1` would have split), and the history file round-trip is
  asserted on text.
- **nesting_recursion** — N/A. No new recursion. The one thing that recurses here is
  the pre-existing `Repl::session_names` → `Vm::user_names` walk, unchanged.
- **duplicate_missing_keys** — N/A. No map or record is introduced; the one shared
  cell (`SessionNames`) is overwritten wholesale each line rather than keyed, so it
  cannot hold a duplicate or miss one. `tab_completes_a_builtin_and_only_the_names_the_session_has_bound`
  pins the overlap case that does exist: a live name that is also a builtin is
  offered once.
- **malformed_input** — covered: `edge_an_unclosed_block_is_still_reported_at_end_of_input`
  (an unclosed block at EOF), `edge_a_read_that_fails_ends_the_session_and_is_reported_as_a_failure`
  (a reader that is not producing input), and the history file whose
  *directory* is a regular file.
- **resource_limit** — covered: `the_history_directory_is_created_by_the_session_that_writes_to_it`
  and `edge_a_history_file_that_is_not_writable_is_reported_rather_than_lost`
  (a history file that cannot be written is reported, not silently dropped).
  The pre-existing `HISTORY_LIMIT` / `MAX_BLOCK_LINES` / `MAX_BLOCK_BYTES` guards
  are untouched and still covered by their own tests.

## Invariants touched

None. No change to the grammar, the `Value` variants, the `Error` variants, the
`.rb` extension or `set x to <expr>`. `execute_line`, `handle_command` and
everything under them are byte-identical; the only difference is where the line
text came from.

## Known gaps / follow-ups

- **`rbops/verify.sh phase-052` was not run** — see the Gates table. It has to pass
  before this phase is `.done`.
- The `:save` / `:restore` commands (`src/repl/mod.rs`) still write and read the
  in-session `ReplHistory`; they are unchanged. A terminal session now also has the
  editor's file-backed history, and `Repl::restore_history` seeds `Repl::history`
  from it at startup so `:history` answers about the same lines Up-arrow offers.
  They are two histories that agree at startup, not one — `:restore` still replaces
  only the session's copy. Recorded in `FINDINGS.md`.
- The debugger's `debug_stop` (`src/repl/mod.rs`) still reads with
  `io::stdin().read_line`, so a breakpoint prompt gets no completion, no history and
  no Ctrl-C. It was outside this phase's scope — recorded in `FINDINGS.md`.
- `default_history_path()` reads `HOME` rather than a platform home lookup, so a
  Windows session with no `HOME` keeps its history in memory. Documented at the
  function; `the_default_history_path_is_under_the_home_directory` holds either way.
- TAB on an empty prefix completes to `:?` in the editor, because the candidate set
  is the whole vocabulary and they share no common prefix. That is rustyline's
  "ambiguous" rendering and is what any readline does there.