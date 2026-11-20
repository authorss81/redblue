# Phase 120 — Let the REPL pipe to the shell and bind keys, without loading code

## What changed
| File | Lines | What |
|---|---|---|
| src/repl/commands.rs | +31 −4 | adds `ReplCommand::Bind(Option<String>)` and its command-table row (`:bind` / `.bind`, optional argument), carries the bind remainder through the argument accessor, makes `:pipe`'s help row and variant documentation say it is for **read-only inspection** (with `!<command>` remaining the general host-shell escape), and pins the new parse rows in the existing parse test |
| src/repl/mod.rs | +488 −22 | factors the shared `:alias`/`:bind` body into `define_shortcut`/`store_shortcut`, gives each command its own plural so refusals read "aliases"/"bindings", adds `define_binding` (refuses a missing/self/unknown/self-rebinding target at bind time), `validate_binding_target` (resolves shortcut targets before asking the command table and returns the command text that is stored), `bind_command`/`print_bindings` (deterministic name-ordered listing under the shared "Shortcuts" heading), dispatch for `ReplCommand::Bind`, and drops the duplicate manual `:pipe` help sentence |
| tests/repl_test.rs | +154 −1 | end-to-end session tests for bound-key dispatch, deterministic listing order, bind-time refusal, colour path, `:pipe` output/exit/usage, the help text's read-only scope, and both the success and missing-last-result paths of a pipe to a command that reads stdin |
| tests/unreviewed_work_ack_test.rs | +1 −0 | pins the preserved phase-119 delivery commit by SHA, the handoff the resumed run requires |

## Tests added
| Test | Edge class covered |
|---|---|
| bind_maps_a_key_to_an_existing_command | happy path — a key reaches its command |
| edge_bind_refuses_an_unknown_command_at_bind_time | failure / missing target — named at bind time, not stored |
| edge_bind_refuses_a_key_bound_to_itself | recursion — a key cannot run itself |
| edge_bind_refuses_a_key_bound_to_a_command_that_rebinds_it | recursion — a target that is `:bind`/`:alias` naming the same key is refused |
| edge_alias_cycle_refusal_uses_the_alias_wording | wording — an alias cycle is named in the plural, not as "aliass" |
| edge_alias_table_stops_at_its_capacity | wording — the alias capacity refusal names "aliases", not "aliass" |
| bind_replaces_a_rebound_key_rather_than_stacking | duplicate keys — rebinding replaces |
| bind_accepts_a_target_that_is_another_shortcut | nesting — a key may follow another shortcut |
| edge_bind_refuses_a_key_that_shadows_a_builtin_command | duplicate names — a key may not take a command word |
| edge_bind_with_no_command_is_a_named_usage_error | empty / malformed — a bare key is a usage error |
| edge_pipe_command_with_no_output_is_empty_and_succeeds | empty — no output is a success, not a failure |
| edge_pipe_to_a_missing_command_reports_the_shells_failure | missing command — non-zero status and a shell message, without pinning `/bin/sh`'s 127 or wording |
| edge_pipe_to_a_missing_command_reports_its_nonzero_status_and_message | missing command end-to-end — same non-zero/non-empty contract |
| edge_bind_keeps_the_command_it_resolved_when_the_target_alias_changes | alias target redefined — the key keeps the command it resolved at bind time |
| edge_bind_refuses_a_key_that_is_not_usable | malformed key — a sigil inside the key is refused by name |
| edge_bind_with_a_key_but_no_command_names_the_missing_command | empty / malformed — a key with no command names the missing half |
| edge_bind_table_stops_at_its_capacity | resource limit — the shared table cap refuses a new binding with the "bindings" wording |
| edge_bind_refuses_an_indirect_cycle | recursion — an indirect loop through aliases is refused with the "bindings" wording |
| bind_runs_a_key_and_lists_bindings_in_order | deterministic order — the listing is sorted, not insertion order |
| edge_a_bound_key_paints_its_command_output_like_any_other_command | colour path — a bound command's output is painted |
| edge_bind_refuses_an_unknown_command_before_the_key_is_used | failure — refused on the binding line, session continues |
| edge_pipe_runs_a_command_and_reports_output_and_exit | non-zero exit — 0 and 3 are reported exactly, not swallowed; the missing-command case is covered by the non-zero/non-empty contract above |
| edge_pipe_with_no_argument_is_a_usage_error_not_an_empty_command | empty — a bare `:pipe` never starts an empty command |
| help_scopes_pipe_to_read_only_inspection | boundary — the help text states `:pipe`'s read-only scope, so the documented contract cannot drift from the goal |
| edge_pipe_to_a_command_that_reads_stdin_returns_what_it_wrote | stdin — a real `:pipe tr a-z A-Z` transforms the last result, so only a child that received stdin can print the uppercase marker |
| edge_pipe_to_a_stdin_command_without_a_last_result_names_it | failure — a stdin-reading command with no last result is a named error, and the session continues |

The `commands.rs` parse test also gains the `:bind`/`.bind` rows (bare and with a
remainder), so the new variant is exercised through the same parse path as every
other command.

## Test requirement matrix
| Row | Status |
|---|---|
| empty | covered — empty command output, bare `:pipe`, bare `:bind` key |
| singleton | covered — a single binding listed, a single key rebound |
| boundary | covered — exit 0 vs 3 vs a missing command's non-zero status; empty vs non-empty output; the help text's read-only scope |
| out_of_bounds | N/A — no indexing or positional lookup is added; bindings are looked up by name |
| type_mismatch | N/A — command arguments are text; no typed value API is added, and the existing pipe coercion of the last value is unchanged |
| numeric_boundary | N/A — only the host's exit code is compared; no arithmetic is added |
| unicode | N/A — keys are command shortcut words and shell bytes are decoded by the existing `shell` module, which this phase does not change |
| nesting_recursion | covered — a key may follow another shortcut, a self-binding and an indirect self-rebinding are refused, and `edge_bind_refuses_an_indirect_cycle` refuses a loop through an alias chain |
| duplicate_missing_keys | covered — rebinding replaces; a missing/unknown target is refused |
| malformed_input | covered — bare `:pipe`, bare `:bind`, a key with no command, an unusable key, and an unknown target are clean named errors |
| resource_limit | covered — `edge_bind_table_stops_at_its_capacity` fills the shared table and asserts `:bind` refuses a new key with the "bindings" wording; no new resource surface is added |

## Gates
| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 3271 passed, 0 failed, 0 ignored (87 binaries) |
| ../rbops/verify.sh "phase-120" | PASS |

## Invariants touched
- None. `:pipe` keeps its existing "last result on stdin" contract and `:bind`
  writes the same shortcut table `:alias` already writes; no language surface,
  `Value` variant, error type or grammar is changed.

## Known gaps / follow-ups
- `:plugin` was considered and declined (untrusted runtime code loading would
  weaken the gate); recorded in `FINDINGS.md` so it is not re-proposed blindly.
- `:bind` and `:alias` share one table, so `:bind` lists aliases too and an alias
  may name a key. That is deliberate (both are a name for a command); both
  listings therefore use the heading "Shortcuts" and the command help says so.
- `:bind` stores the command its target resolved to, not the alias name. An
  alias redefined later cannot silently change what a key runs.
- `:pipe`'s read-only scope is a usage contract stated in its help, not a
  sandbox: the shell command is run as typed, so a user can still type a
  mutating command. `!<command>` remains the general host-shell escape and the
  name for that use; enforcing a sandbox around an arbitrary host shell is out
  of scope for this phase.
