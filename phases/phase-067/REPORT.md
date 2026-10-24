# Phase 067 — Give the session a memory: `.checkpoint`, `.restore`, `.undo`, `.redo`, `.sessions`

## What changed

| File | Lines | What |
|---|---|---|
| src/repl/commands.rs | +96 −4 | `Checkpoint`, `Sessions`, `Undo` and `Redo` variants and table rows; the `restore` help line states the file/memory split; `.restore` no longer advertises a filesystem path |
| src/repl/mod.rs | +502 −0 | session snapshots, named checkpoints, bounded undo/redo stacks, dispatch for the five commands, and the sigil split for `:restore` (file) versus `.restore` (checkpoint) |
| src/repl/history.rs | +13 −0 | `truncate`, so a checkpoint can put the history back at the position it saved |
| tests/repl_test.rs | +78 −0 | end-to-end sessions for checkpoint restore, empty undo/redo/sessions, redo invalidation, the restore-split, and the bound notice |

A snapshot holds the VM's globals and live scopes, the REPL's `_` last result,
the definitions map used by `.which`/`.type`, and the module context. `.checkpoint
<name>` stores one in creation order; `.restore <name>` puts it back and rewinds
the history to the saved position. `.undo`/`.redo` walk a separate bounded stack
of binding states, so a single `set` is a single step in each direction. The
stacks are bounded at 50 steps and checkpoints at 64; each boundary reports what
it dropped or refused rather than growing silently.

## Tests added

| Test | Edge class covered |
|---|---|
| checkpoint_restore_brings_back_bindings_last_result_and_history_position | singleton / whole state — value, `_` and history position all restored |
| edge_undo_after_one_set_removes_exactly_it_and_redo_brings_it_back | empty undo stack / exact step both directions |
| edge_duplicate_checkpoint_name_is_refused_and_the_old_one_is_kept | duplicate keys — refused, old checkpoint kept |
| edge_restore_of_an_unknown_checkpoint_is_an_error_and_changes_nothing | missing key / failure channel — `expect_err`, no mutation |
| edge_undo_history_is_bounded_and_drops_the_oldest_step | resource limit — 51 changes, oldest step dropped at 50 |
| edge_checkpoint_history_is_bounded_and_drops_the_oldest_checkpoint | resource limit — 65 saves, oldest checkpoint dropped at 64 |
| edge_restore_brings_back_the_whole_session_state | singleton / whole state — aliases, macros, recording, tour, file, format preference and transcript |
| edge_undo_after_restore_marks_the_checkpoint_whose_bindings_are_current | state — undo updates the `.sessions` marker instead of leaving a stale one |
| edge_sessions_lists_creation_order_and_marks_the_current_checkpoint | empty listing / order / current marker |
| edge_redo_after_a_new_change_is_cleared | boundary/state — new change invalidates redo |
| edge_help_states_the_file_and_memory_restore_split | help text states `:restore` file vs `.restore` checkpoint once |
| edge_session_memory_commands_parse (commands.rs) | malformed input — bare `.checkpoint`; `.restore` is not a path |
| checkpoint_restore_brings_back_bindings_and_last_result_end_to_end (tests/repl_test.rs) | dispatch — a real piped session |
| edge_undo_and_redo_and_sessions_with_nothing_to_show_say_so (tests/repl_test.rs) | empty — three named empties |
| edge_redo_after_a_new_change_reports_that_the_redo_history_was_cleared (tests/repl_test.rs) | state — the invalidation is stated in the message |
| edge_dot_restore_of_an_unknown_name_is_a_checkpoint_error_not_a_file_error (tests/repl_test.rs) | missing key / failure channel — dot and colon destinations stay separate |
| edge_undo_past_its_bound_drops_the_oldest_step_with_a_notice (tests/repl_test.rs) | resource limit — the bound and its notice in a real session |

### Mandatory edge-case matrix

- **empty** — covered: empty undo/redo stacks and an empty checkpoint listing all have named answers.
- **singleton** — covered: one `set`, one undo, one redo; one checkpoint restored.
- **boundary** — covered: undo at the 50-step bound and redo after a new change.
- **out_of_bounds** — covered as state, not indexing: `.undo` past the oldest step returns `Nothing to undo.`, never a panic.
- **type_mismatch** — N/A: the commands take names, not values; no coercion is performed.
- **numeric_boundary** — N/A: no arithmetic or number formatting is added.
- **unicode** — N/A: checkpoint names are compared as text; no new text parsing or rendering path exists. Existing REPL Unicode tests are untouched.
- **nesting_recursion** — N/A: snapshots clone existing values and do not recurse into them beyond the language's own clone.
- **duplicate_missing_keys** — covered: duplicate checkpoint name refused, unknown checkpoint named in the error.
- **malformed_input** — covered: `.checkpoint` with no name is `MissingArgument`; an unknown name is an error.
- **resource_limit** — covered: undo bounded at 50 with an explicit drop notice, checkpoints bounded at 64.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2107 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh phase-067 | pass — `VERIFY PASS — phase-067` |

## Invariants touched

- None in the language: no grammar, keyword, `Value`/`Error` variant or `.rb` extension changed.
- REPL-only: `.restore` is now the in-memory checkpoint spelling while `:restore` remains the saved-history-file command. Existing `:save`/`:restore` tests pass unmodified.
- Snapshots are session-local; nothing is written to disk.

## Known gaps / follow-ups

- Undo steps deliberately do not rewind the typed history (only `.checkpoint`/`.restore` restores the saved history position); undoing a line leaves its place in `:history` so the record of what was typed stays true.
