# Phase 065 — Let users extend the REPL: `.alias` and `.macro`

## What changed

| File | Lines | What |
|---|---|---|
| src/repl/commands.rs | +53 −0 | `Alias(Option<String>)` and `Macro(Option<String>)` variants, their command-table rows (`:alias`/`.alias`, `:macro`/`.macro`), and their carried-argument handling |
| src/repl/completer.rs | +11 −1 | Command completion also offers user aliases (carried in the session vocabulary with the canonical sigil) while macro names complete as bare words |
| src/repl/mod.rs | +789 −10 | Session alias table, macro table and in-progress recording; alias definition, chain resolution and cycle refusal; macro start/stop/list/run; `:help` sections; `:save`/`:restore` state section; completion vocabulary |
| tests/repl_test.rs | +248 −0 | End-to-end sessions for aliases, macros, cycles, reset, help/completion, Unicode and save/restore |

The implementation reuses the existing command table and dispatch: an alias expands to a command line before `handle_command`, and a macro is replayed through the same per-line dispatch (`handle_command` or `execute_line`) the read loop uses. No language grammar, keyword, `Value` or `Error` variant changed.

## Tests added

Fourteen new `#[test]` functions in `tests/repl_test.rs` and four new `#[test]`
functions in `src/repl/mod.rs`; zero ignored, zero skipped, zero
`allow(clippy::` suppressions.

| Test | Edge class covered |
|---|---|
| `alias_shortcut_matches_the_command_it_names` | singleton — one alias output diffed against the command it names |
| `macro_replays_three_recorded_lines_with_identical_output` | singleton/order — three recorded lines replay with the same transcript |
| `edge_alias_cycle_is_refused_without_looping` | nesting_recursion / resource_limit — `a -> b`, `b -> a` is refused and does not loop |
| `edge_alias_with_no_arguments_lists_the_empty_table` | empty — bare `.alias` names the empty table, and lists a non-empty one |
| `edge_macro_recording_survives_reset_while_bindings_do_not` | state/reset — recording survives `:reset`; bindings do not |
| `edge_macro_without_a_recording_is_a_usage_error` | malformed_input — stopping nothing is a clean usage error |
| `edge_save_and_restore_round_trip_aliases_and_macros` | resource/state — one alias and one macro survive save/restore |
| `edge_alias_to_alias_resolves_to_the_command_at_the_end` | nesting_recursion — an alias of an alias reaches the final command |
| `edge_alias_colliding_with_a_builtin_is_refused` | malformed_input — a built-in name is refused, and the built-in still works |
| `edge_macro_run_of_an_unknown_name_is_an_error` | resource/state — an unknown macro is a clean error |
| `edge_unicode_alias_name_and_macro_line_round_trip` (module test) | unicode — a non-ASCII alias name and emoji macro line round-trip |
| `edge_module_completion_offers_session_aliases_and_macro_names` (module test) | boundary — TAB offers aliases under either sigil and macro names |
| `edge_restore_warns_about_malformed_saved_state_and_loads_the_rest` | malformed_input — malformed saved aliases and macros are named and skipped, and readable records still load |
| `edge_bare_alias_with_an_argument_is_source_not_an_alias_call` | boundary — `:name` takes an argument while a bare alias must stand alone |
| `edge_unknown_macro_subcommand_is_a_usage_error_not_a_macro_name` | malformed_input — a mistyped `.macro` sub-command is refused instead of run as a name |
| `edge_alias_table_stops_at_its_capacity` | resource_limit — a new alias past the cap is refused; redefining an existing one still works |
| `edge_macro_recording_stops_at_its_line_capacity` | resource_limit — a recording stores the cap and stops instead of growing without bound |
| `edge_macro_table_stops_at_its_capacity` | resource_limit — a new macro past the cap is refused; re-recording an existing one still works |

### Mandatory edge-case matrix

- **empty** — covered: `edge_alias_with_no_arguments_lists_the_empty_table` names the empty alias table; bare `.macro` with no recording is a usage error; `.macro list` names an empty macro table.
- **singleton** — covered: one alias and one three-line macro in the output-diff and round-trip tests.
- **boundary** — covered: an alias chain of two resolves to the command at the end; completion offers the alias under both sigils; `:name` takes an argument while a bare alias must stand alone.
- **out_of_bounds** — N/A: this change adds no index or slice a user supplies; macro records carry their own line count and a short file is reported and skipped.
- **type_mismatch** — N/A: the inputs are command text, not typed operands; a wrong macro sub-command or a missing name is reported as a usage error.
- **numeric_boundary** — covered: the macro line count in saved state is parsed without pre-allocating it, capped, and a malformed or truncated record is named and skipped.
- **unicode** — covered: `edge_unicode_alias_name_and_macro_line_round_trip` records an emoji-bearing line and replays it twice; alias names are stored as text without normalisation.
- **nesting_recursion** — covered: `edge_alias_to_alias_resolves_to_the_command_at_the_end` follows a chain; `edge_alias_cycle_is_refused_without_looping` refuses the loop before it can recurse.
- **duplicate_missing_keys** — N/A: no records are involved. Re-defining an alias replaces that alias; a missing macro is covered by `edge_macro_run_of_an_unknown_name_is_an_error`.
- **malformed_input** — covered: `edge_macro_without_a_recording_is_a_usage_error`, `edge_alias_colliding_with_a_builtin_is_refused`, `edge_unknown_macro_subcommand_is_a_usage_error_not_a_macro_name`, and `.macro run` with no name all report an `Error:`/`Usage:` and leave the session alive; `edge_restore_warns_about_malformed_saved_state_and_loads_the_rest` names every damaged saved record.
- **resource_limit** — covered: cycle detection bounds alias resolution; `MAX_ALIASES`, `MAX_MACROS` and `MAX_MACRO_LINES` bound the session tables and recordings; the existing block-size limits still apply while a macro replays a block.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2081 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh phase-065 | pass — `VERIFY PASS — phase-065` |

## Invariants touched

- None of the language surface: no grammar, keyword, `Value` variant or `Error` variant changed, and `examples/*.rb` and `modules/*.rb` are untouched.
- Aliases and macros are session state only. They are not language bindings: the analyzer is still given only real bindings, so a program saying a macro's name is still an unknown variable.
- A user alias may not shadow a built-in command word; the collision is refused by name.

## Known gaps / follow-ups

- Aliases, macros and a single macro's length are capped (256, 256 and 500 respectively) rather than unlimited. The cap is user-visible: the command that would cross it reports the limit instead of silently dropping data.
- A macro is replayed through the same dispatch as a typed line, including command lines. A macro that contains `.macro start` or `:quit` therefore does what those commands say when replayed; the phase only requires recording and replaying the session's lines, so this is not restricted.
- An in-progress recording is not persisted by `:save`; only completed macros and aliases are. The phase requires the named macros and aliases to survive, not a recording half-written at save time.
