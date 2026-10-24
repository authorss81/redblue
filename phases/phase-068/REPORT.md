# Phase 068 — Talk to the shell without leaving: `!cmd`, `.pipe`, `.cd`, `.pwd`, `.ls`

## What changed

| File | Lines | What |
|---|---|---|
| src/repl/shell.rs | new | host-shell module: deadline-bounded `run`, streaming `pipe`, capped output that surfaces a real read failure, `listing`, `ShellOutcome`, the clean no-shell refusal, and process-group cleanup with a range-checked group id that joins every reader and writer thread |
| src/repl/mod.rs | +574 −18 | `cwd` session state, tilde-aware path resolution for every file argument, symlink-preserving `.cd`, `.cd`/`.pwd`/`.ls` dispatch and listing, `!` and `.pipe` dispatch, shell output/exit/truncation rendering, help line, cwd in checkpoints/reset, `path_completion`, and refreshing the editor's directory each line |
| src/repl/commands.rs | +134 −0 | `Shell`, `Pipe`, `ChangeDir`, `PrintWorkingDirectory`, `ListDirectory` variants and table rows; `!` special parse; `.cd` path completion and `<directory>` usage; argument-text coverage |
| src/repl/completer.rs | +1 −1 | `DIRECTORY_SEPARATOR` shared with `.ls` so both use the same directory marking; path completion now takes the directory to read |
| src/repl/editor.rs | +62 −8 | `SessionDir` shared handle, `with_completion_dir`/`line_source_with_dir`, and TAB reading the session directory instead of the process directory |
| src/repl/debug_repl.rs | +44 −12 | the sub-REPL carries the session directory and resolves through the session's own path resolver, so `.debug <relative file>` and `.run <relative file>` read where `:load` does, including `~` |
| Cargo.toml / Cargo.lock | +4 −0 | `libc` on Unix, for stopping a timed-out command's whole process group |
| tests/repl_test.rs | +41 | end-to-end `!` output/exit and `.pipe`/`.cd`/`.pwd`/`.ls`/`:load` session test, run on every host |
| tests/unreviewed_work_ack_test.rs | +1 −0 | phase-068's delivery commits are acknowledged by the subject rule, not by a stale SHA |

`!cmd` runs through `sh -c` (or `cmd /C` on Windows), prints the command's stdout and stderr, and always reports its exit status. A failing command's status is printed as `exit code: N`; it is the command's result, not a REPL error. An empty `!` is refused with a usage line before any process is spawned.

`.pipe <cmd>` writes the session's `_` value to the command's stdin. The writer runs on its own thread while the main thread drains stdout and stderr, so a 1 MB payload through `cat` cannot deadlock on a full output pipe. No last result is a named refusal.

Both shell paths now have a deadline (30 seconds) and a per-stream output cap (8 MiB): a command that would hold the session for ever is killed, and one that would fill memory has its read end closed and is reported as truncated. On Unix the shell leads its own process group, so a timeout stops the descendants that inherited its pipes too; the output readers and the input writer are always joined, never detached, so a timed-out command cannot leave threads behind. A genuine read failure on either output stream is reported rather than being mistaken for a clean end of file. The pipe writer distinguishes a broken pipe — the command stopped reading, which is its own answer — from any other write failure, which is reported instead of silently discarded. `.cd` validates through `metadata` and stores the lexical path, so entering a symlink keeps the name the user typed rather than rewriting it to the target; `~` at the start of a path expands to the host home directory.

The session now owns a working directory (`cwd`), distinct from the process directory. `.cd` changes only the session and leaves it unchanged on failure; `.pwd` prints it. `.ls` lists it with a trailing `/` on directories, using the same constant as phase-060 completion. `:load`, `:save`, `:restore`, `:run` and `.export` resolve relative paths against it. Checkpoints carry `cwd`, and `:reset` returns it to the process directory.

TAB completion reads that same directory through a shared `SessionDir` handle refreshed beside the session's names at every prompt, so `.cd` moves what `<TAB>` offers as well as what `:load` reads. The handle is shared with the editor rather than copied, and a directory that has gone missing completes nothing rather than falling back to the process directory.

Host-only status is stated in the help text and in the `.pipe` description; a target without `sh`/`cmd` gets `NO_SHELL` rather than an obscure spawn failure.

## Tests added

| Test | Edge class covered |
|---|---|
| shell_runs_a_host_command_and_reports_output_and_exit_code | happy path — output and zero exit asserted separately |
| edge_shell_reports_a_failing_exit_code_instead_of_a_repl_failure | failure — non-zero status is surfaced, not translated into `Error:` |
| edge_shell_with_an_empty_command_is_a_usage_error_and_spawns_nothing | empty / malformed input — `!` with no command |
| pipe_sends_the_last_value_to_the_commands_stdin | direction — `_` reaches `cat`'s stdin, not its stdout |
| edge_pipe_with_no_last_result_names_what_is_missing | empty — no `_` yet |
| edge_pipe_to_a_failing_command_surfaces_its_exit_and_keeps_the_session | failure + state — exit 9, bindings survive |
| edge_pipe_streams_a_megabyte_payload | resource limit — 1 MB survives unchanged without deadlock |
| edge_change_directory_to_a_missing_path_names_it_and_leaves_the_directory_alone | missing / failure — path named, cwd unchanged |
| edge_change_directory_makes_load_resolve_there_and_pwd_is_distinct | boundary — session directory is not the process directory; bare `:load` resolves there; `.ls` agrees |
| edge_change_directory_accepts_a_relative_path_from_the_session_directory | relative path — `.cd nested` and `.cd ..` resolve under the session directory |
| edge_restore_brings_back_the_session_directory | checkpoint state — restoring a checkpoint restores the directory relative paths resolve against |
| edge_reset_returns_the_session_directory_to_the_process_directory | reset state — `:reset` moves the session back to the process directory |
| edge_shell_carries_unicode_through_unchanged | unicode — emoji and accents in command output |
| edge_listing_marks_directories_with_the_completion_separator | boundary — directories end in `/`, files do not |
| edge_a_command_that_overruns_its_deadline_is_stopped (shell.rs) | resource limit — a command that never exits is killed and named |
| edge_a_pipe_command_that_overruns_its_deadline_is_stopped (shell.rs) | resource limit — the same deadline covers a command that owns stdin |
| edge_output_over_the_limit_is_closed_and_marked_truncated (shell.rs) | resource limit — an endless writer is capped and the outcome says so |
| edge_pipe_to_a_command_that_stops_reading_is_not_a_repl_failure (shell.rs) | failure — the command's early close is its own answer, not a second error |
| edge_a_leading_tilde_expands_to_the_home_directory | path edge — `~` expands only as the first component and only when a home exists |
| edge_change_directory_keeps_the_symlink_path_the_user_entered | boundary — `.cd link` keeps `link`, rather than rewriting it to the target |
| empty_shell_command_is_refused_without_starting_a_process (shell.rs) | empty — the refusal is the constant, before spawn |
| listing_of_a_missing_directory_names_the_path (shell.rs) | missing resource — listing failure names the path |
| edge_shell_and_directory_commands_parse (commands.rs) | malformed input — `!` keeps the whole command, empty `!`, missing `.pipe`/`.cd` arguments, `.pwd`/`.ls` |
| shell_escape_shows_command_output_and_exit_code (repl_test.rs) | end-to-end dispatch — `hi`, zero exit and failing exit in one session |
| edge_pipe_cd_and_ls_use_the_session_directory (repl_test.rs) | end-to-end state on every host — pipe output through `sort`, `.cd`, `.pwd`, `.ls` and `:load` share one directory |
| edge_path_completion_follows_the_session_directory_after_cd (repl/mod.rs) | boundary — TAB after `.cd` offers the file `:load` would read, not the process tree |
| edge_path_completion_in_a_missing_directory_offers_nothing (repl/mod.rs) | missing resource — a vanished session directory completes nothing rather than panicking |
| edge_change_directory_refuses_a_regular_file_and_names_it (repl/mod.rs) | failure — a file is not a directory, the path is named, and the session does not move |
| edge_path_completion_follows_the_shared_session_directory (editor.rs) | boundary — the editor reads the live shared directory, so moving it moves TAB |
| edge_a_reader_past_the_drain_deadline_is_joined_not_detached (shell.rs) | resource limit — a reader that outlives the drain deadline is joined and its output kept |
| edge_a_writer_past_the_drain_deadline_is_joined_not_detached (shell.rs) | resource limit — a writer that outlives the drain deadline is joined rather than detached |
| edge_debug_resolves_a_relative_file_against_the_session_directory (repl/mod.rs) | boundary — `.debug <relative file>` reads where `:load` would after `.cd` |

### Mandatory edge-case matrix

- **empty** — covered: empty `!`, empty `.pipe` argument, no last `_`, and an empty command refused before spawning.
- **singleton** — covered: exactly one last value piped, one directory changed, one file loaded by bare name.
- **boundary** — covered: session directory versus process directory, in both commands and TAB; directory separator in `.ls`; a 1 MB payload boundary.
- **out_of_bounds** — N/A: no indexing or positional access was added; a missing path (including one that disappears after `.cd`) is the nearest analogue and is covered as a clean error.
- **type_mismatch** — N/A: `.pipe` accepts any `Value` and renders it as text; no command takes a typed value and no coercion was added.
- **numeric_boundary** — N/A: no arithmetic or numeric formatting was added; exit codes are the host's own `i32`.
- **unicode** — covered: `edge_shell_carries_unicode_through_unchanged`.
- **nesting_recursion** — N/A: no recursive structure is walked; values are rendered by the existing `Display`.
- **duplicate_missing_keys** — N/A: no keyed collection is added or read by these commands; missing paths are covered.
- **malformed_input** — covered: empty `!`, missing `.pipe`/`.cd` arguments, missing directory, a regular file where a directory is required, missing listing path, and a vanished completion directory.
- **resource_limit** — covered: the 1 MB pipe test, the 30-second command deadline, the 8 MiB output cap, and an endless writer whose read end is closed; listing is a directory read, bounded by the filesystem listing already used by completion.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2141 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh phase-068 | pass — `VERIFY PASS — phase-068` |

## Invariants touched

- None in the language: no grammar, keyword, `Value`/`Error` variant or `.rb` extension changed.
- REPL-only: `!` is a new command prefix, and `.pipe`/`.cd`/`.pwd`/`.ls` are new command-table entries. `:load`, `:save`, `:restore`, `:run` and `.export` now resolve relative paths against the session directory; absolute paths are unchanged.
- Checkpoints now include the session directory, so restoring a checkpoint restores where relative paths point as well as the bindings.

## Known gaps / follow-ups

- None.
