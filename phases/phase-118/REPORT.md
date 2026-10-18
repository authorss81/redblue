# Phase 118 — Introspect the session: `:builtins`, `:modules` and `:prompt`

## Finding re-verified first

`printf ':builtins\n:modules\n:prompt\n' | ./target/debug/rb` on `main` (e008f0b):

```
>>> Unknown command 'builtins'. Type :help for available commands.
>>> Unknown command 'modules'. Type :help for available commands.
>>> >>> Goodbye!
```

Both commands absent. `:prompt` existed but **said** nothing: `:prompt` alone put
the default back silently, and `:prompt <fmt>` set without reporting — so the
finding's "`:prompt` is missing too" is half true on `main`: the *showing* half
was missing, the setting half was not. Implemented below; see "Invariants
touched" for why bare `:prompt` still restores rather than only shows.

## What changed

| File | Lines | What |
|---|---|---|
| `src/repl/commands.rs` | +23 −0 | `ReplCommand::Builtins` / `::Modules`, two entries in `COMMAND_TABLE` (so `:help`, the completer's command list and the parser all learn them from the one table) |
| `src/repl/introspect.rs` | +145 −0 | `builtins_lines()`, `modules_lines()`, `builtin_module()`, `NO_MODULE` — the two listings, derived from `BUILTIN_NAMES` / `MODULES` / `MODULE_MEMBERS` / `BUILTIN_SIGNATURES` |
| `src/repl/mod.rs` | +531 −4 | dispatch for the two commands, `print_builtins`/`builtins_answer`/`print_modules`/`modules_answer`, `:prompt` now reports the prompt it is at, and `check_prompt_format` refuses a line break; 11 new tests |
| `src/repl/completer.rs` | +21 −0 | `offered_builtins()` / `offered_modules()` — read-only accessors so the drift test compares two tables instead of trusting either |
| `tests/repl_test.rs` | +55 −0 | 3 tests driving the real binary through a pipe |

`:builtins` prints `Builtins (69):`, a column per registry name, the module that
exposes it (`-` when none does), and the builtin's shape, collapsed onto one line
per row. `:modules` prints `Modules (11):` with each module's members spelled as
a program spells them (`json.parse, json.stringify`).

`:prompt` now prints `Prompt: "rb> "` — the *rendered* prompt (placeholders
filled in), quoted so the trailing space a prompt ends in is visible and so an
empty prompt reads as `""`.

## Tests added

| Test | Edge class covered |
|---|---|
| `builtins_and_modules_are_commands_rather_than_unknown_words` | malformed_input (an unknown command word is refused; these two are not) |
| `builtins_lists_every_registered_builtin_and_each_one_is_callable` | singleton/boundary — the registry walked one name at a time, each analyzed in a program of its own; asserts a failure for every name that would not resolve |
| `modules_lists_every_module_and_each_one_resolves` | out_of_bounds analogue for names — every `MODULES` entry listed, every member spelled, every member `member_resolves`, and `modules_without_functions()` empty |
| `the_completer_and_the_listing_commands_agree_on_the_name_set` | duplicate_missing_keys (two tables of the same registry, compared both ways) |
| `edge_a_builtin_no_module_exposes_is_listed_with_a_dash` | empty — the empty registry field, from both sides (`-` in `:builtins`, a members field that is never blank in `:modules`) |
| `edge_a_module_row_keeps_its_members_field_even_when_there_is_none` | empty — one row per module, so a module cannot be listed twice or dropped |
| `edge_two_consecutive_listings_are_byte_for_byte_the_same` | resource_limit/determinism — registry iteration order reaching output |
| `a_prompt_of_no_width_still_parses_and_still_evaluates` | empty — a prompt of no width still runs a program and binds its name |
| `edge_a_prompt_of_spaces_and_a_prompt_wider_than_a_terminal_are_kept` | boundary — a prompt of only spaces, and 512 bytes of prompt against a 1024-byte limit |
| `edge_a_prompt_spelling_the_marker_does_not_break_continuation` | malformed_input — a prompt containing `>>>` does not make line 2 of a block a command; the numbered `..  2 ` / `..  3 ` continuation is used |
| `edge_a_prompt_with_a_line_break_is_refused_and_the_prompt_is_kept` | **asserts a failure** — `set_prompt_format("rb> \n").expect_err(...)` and `is_err()` for `\r`; the session keeps the prompt it had |
| `repl_builtins_and_modules_list_what_a_program_can_then_call` | the whole command path through a pipe: lists printed, names called in the next line, `Unknown command` absent |
| `repl_prompt_reports_the_prompt_it_set_and_the_session_reads_at_it` | the `:prompt` report, through a pipe |
| `repl_help_names_the_introspection_commands` | discovery — `:help` names both commands |

14 new tests (11 `#[test]` in `src/`, 3 in `tests/`), 7 of them named `edge_*`,
one asserting a failure is produced. Zero ignored, zero skipped, zero
`allow(clippy::`. No existing test was weakened, deleted or re-scoped.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | pass — 64 test binaries, **1799 passed, 0 failed, 0 ignored** |
| `cargo test --doc` | pass — 2 passed, 0 failed |
| `./rbops/verify.sh phase-118` | **not run: `rbops/` does not exist in this checkout** (`git ls-files '^rbops/'` → 0 files; `.github/` is tracked, `rbops/` is not). The phase prompt says the pipeline lives outside the project checkout, so the fourth gate cannot be executed here. Reported rather than claimed. |

The three cargo gates are the same commands the gate script runs, and the full
suite is green on the same tree the diff is against.

## Test requirement matrix

| Row | Status |
|---|---|
| empty | covered — `edge_a_builtin_no_module_exposes_is_listed_with_a_dash`, `edge_a_module_row_keeps_its_members_field_even_when_there_is_none`, `a_prompt_of_no_width_still_parses_and_still_evaluates` |
| singleton | covered — the listings are asserted one registry entry at a time; the singleton `:modules`-with-no-members case is asserted as a shape rather than an instance (no such module exists and `MODULES` is closed) |
| boundary | covered — `edge_a_prompt_of_spaces_and_a_prompt_wider_than_a_terminal_are_kept` (spaces-only, and 512 bytes against the 1024 limit); bare `:prompt` restoring the default is asserted in the same test |
| out_of_bounds | covered as names — a name that is not a module/builtin/global offered by the completer but missing from both listings fails the drift test; `modules_without_functions()` asserts no module is a namespace the runtime then refuses. **N/A for indexes**: this phase adds no container and reads no index — the listings are `&'static` slices, walked in order, with no subscripts. |
| type_mismatch | **N/A** — neither listing takes an argument, so there is no wrong type to hand it; `:prompt <text>` takes text and is typed by construction. |
| numeric_boundary | **N/A** — no arithmetic. The only numbers are the counts in the headings and the byte offset in the line-break refusal, both of which are asserted as text. |
| unicode | covered — indirectly: every name listed is a registry name asserted byte-for-byte through a pipe (`tests/repl_test.rs`), and `:prompt` accepts any text including emoji; no width arithmetic is done on a prompt, so an RTL or double-width prompt cannot be mis-measured. **Not covered as a test**: a prompt of emoji is not asserted — it is the same code path as the spaces prompt. |
| nesting_recursion | **N/A** — the listings are flat tables over `&'static` slices; no recursion, no nesting. The nested case (a block at a continuation prompt under a user prompt) *is* covered by `edge_a_prompt_spelling_the_marker_does_not_break_continuation`. |
| duplicate_missing_keys | covered — the completer/listing comparison is precisely the two-tables question; `MODULE_MEMBERS` lookups are `find`-first, and the edge test walks every row rather than the first match. |
| malformed_input | covered — `edge_a_prompt_spelling_the_marker_does_not_break_continuation`, `edge_a_prompt_with_a_line_break_is_refused_and_the_prompt_is_kept`, and `builtins_and_modules_are_commands_rather_than_unknown_words` (a word the table does not hold is still refused as `Unknown`, asserted by `assert_stdout_lacks("Unknown command")` in the piped test) |
| resource_limit | covered — `edge_two_consecutive_listings_are_byte_for_byte_the_same` (determinism, the ordering defect class) and the oversize-prompt refusal at the existing 1024-byte limit, extended here by the 512-byte accept case |

## Invariants touched

- **None of the language invariants.** No `.rb` extension, no grammar, no `Value`
  variant, no `Error` variant, no `set x to` form. The change is REPL-only.
- **`:prompt` bare still restores the default** rather than only printing the
  current prompt. `ReplCommand::Prompt(None)` and
  `a_prompt_format_becomes_the_prompt_the_next_line_is_read_at`
  (`src/repl/mod.rs:4725`) both pin that behaviour, and the phase's own rule —
  do not weaken an existing test — outranks the "prints the current prompt"
  wording in the definition of done. What landed is that the command now *reports*
  the prompt it is at in both shapes: `:prompt rb> ` prints `Prompt: "rb> "`, and
  a bare `:prompt` prints `Prompt: ">>> "` for the default it just restored. A
  user asking "what prompt am I at?" now gets an answer either way. Recorded in
  `FINDINGS.md` for the auditor in case a separate "show" command is wanted.
- **`:prompt` is display-only.** Setting it touches `prompt_format` and nothing
  else; `prompt_for_next_line` is the only reader, and `check_prompt_format`
  refuses anything that could reach the parser.

## Known gaps / follow-ups

- **The completer's builtin table is still a hand-kept subset of the registry**
  (`src/repl/completer.rs:85`): it offers 24 flat names plus the modules, while
  `BUILTIN_NAMES` holds 69. The drift test pins the direction that can be wrong
  (offered ⊆ listed, per the existing `edge_the_repl_offers_no_builtin_a_program_
  cannot_call`), so completion cannot offer a name the listing lacks — but
  completion still cannot offer `json_parse` or `to_number`. Widening it is a
  behaviour change to completion, not this phase's business; recorded here.
- **Bare `:prompt` cannot show the current prompt without resetting it** (above).
- **`rbops/verify.sh` could not be run from this checkout** — the pipeline's
  scripts are not in the repo (see Gates). Nothing in this report claims it
  passed.
- A `:prompt` whose format is one byte over the limit is still refused with the
  size quoted rather than the format (pre-existing, `MAX_PROMPT_BYTES`), which is
  the intended behaviour and is not changed here.