# Phase 033 — Test the REPL and route it through the ReplCommand it already has

## Reproducing the finding

The finding as filed: all four REPL modules have no tests, and 333 of 731 lines
are never constructed. Both reproduce on the phase's starting commit (`3479d17`):

```
$ grep -c '#\[test\]' src/repl/*.rs
src/repl/commands.rs:0
src/repl/completer.rs:0
src/repl/history.rs:0
src/repl/mod.rs:0

$ grep -rn "ReplCommand::parse\|ReplHistory::\|ReplCompleter::" src/    # outside their own files
(no output)
```

Reading the code to check the claim turned up something the finding did not
report, and it is a hang rather than dead weight:

```
$ printf 'say 1\n' | timeout 3 ./target/debug/rb; echo "exit=$?"
exit=124
```

`exit=124` is `timeout` killing the process: at EOF `read_line` returns
`Ok(0)`, `input.trim()` is empty, and `run` did `continue`
(`src/repl/mod.rs:57-64` before the fix) — so the REPL looped forever, printing
`>>> ` as fast as it could. 9.9 MB of prompt in three seconds, and Ctrl-D could
never leave it. Piping anything into `rb` without a trailing `:quit` never
terminated. That is the defect this phase fixes; the untested dead weight is the
second half.

## What changed

| File | Lines | What |
|---|---|---|
| src/repl/mod.rs | +196 −65 | `handle_command` now `match`es on `ReplCommand::parse` instead of re-parsing the command word against its own copy of the table — the second table is deleted, not relocated. `history: Vec<String>` → `ReplHistory` (with `HISTORY_LIMIT`, `src/repl/mod.rs:22`); `save_session` writes through `ReplHistory::save_to_file`, which is the same format `load_from_file` reads, instead of `join("\n")`+`write`; the read loop breaks on `Ok(0)` so EOF terminates; `Repl::complete` + `session_names` wire the completer into the session |
| src/repl/commands.rs | +193 −7 | `ReplCommand::NotACommand` and `ReplCommand::MissingArgument { command, usage }` added, `#[derive(Debug, PartialEq, Eq)]`, and 6 unit tests |
| src/repl/completer.rs | +172 | `complete_with(word, names)` merges a session's own names into the static vocabulary; `complete` delegates to it; 11 unit tests |
| src/repl/history.rs | +252 | a `max_size == 0` guard in `push`, and 15 unit tests |
| src/vm.rs | +19 | `Vm::user_names()` — a VM's own bindings minus the stdlib's, sorted, so completion can offer what a session defined without offering every builtin |
| tests/repl_test.rs | +238 | 10 end-to-end tests that drive the real `rb` binary over a pipe |

### Why two new variants on `ReplCommand`

Routing through the existing table failed three of the ten end-to-end tests on
the first attempt, and both failures were the shared table's fault, not the
dispatcher's:

- `ReplCommand::parse("2 + 3")` returned `Unknown("2 + 3")`, so the REPL answered
  every expression with `Unknown command '2 + 3'. Type :help...` and evaluated
  nothing. The old private table had a `starts_with(':')` guard that returned
  "not a command" before it ever matched a word; `parse` had that guard too but
  reported the result as `Unknown`, and the guard and the unknown-command arm
  had different meanings. Hence `NotACommand`.
- `parse(":load")` returned `Unknown("load requires a file path")`, so the user
  was told they had mistyped a command they had typed correctly. Hence
  `MissingArgument`, which the REPL renders as the `Usage: :load <filename>` it
  printed before.

Both variants exist because the table is now the single authority on "is this a
command", and an authority has to answer that question correctly.

## Tests added

51 new tests: 41 `#[test]` functions in `src/repl/`, 10 in `tests/repl_test.rs`.
Quota: ≥3 new `#[test]` ✔ (41), ≥1 named `edge_*` ✔ (17), ≥1 asserting a failure
✔ (`edge_load_of_a_missing_file_is_reported_as_an_error`,
`edge_load_of_a_missing_path_reports_the_failure`,
`edge_command_missing_its_argument_is_reported_not_guessed`,
`repl_reports_an_uncaught_error_and_stays_alive`).

| Test | Edge class covered |
|---|---|
| `edge_unterminated_block_then_eof_terminates` | **the finding's bug**: an open `if` block and EOF; the session must terminate, not spin. This test was red for 20 s before the fix |
| `edge_first_line_syntax_error_keeps_the_repl_alive` | malformed input — an unterminated string on line 1; the REPL must print the error and prompt again |
| `edge_load_of_a_missing_path_reports_the_failure` | file that does not exist |
| `edge_command_missing_its_argument_is_reported_not_guessed` | malformed input — a command with nothing to act on |
| `edge_unknown_command_is_named_and_the_session_survives` | malformed input — a command word that does not exist |
| `repl_reports_an_uncaught_error_and_stays_alive` | numeric boundary — `1 / 0` is a clean error, and the session survives it |
| `repl_runs_a_program_that_catches_its_own_error` | out_of_bounds inside a `try` — `[1, 2, 3][999]` caught, asserted on the value the program then prints |
| `edge_zero_size_history_stores_nothing` | boundary — a `max_size` of 0 held one entry |
| `edge_load_of_a_missing_file_is_reported_as_an_error` | a history file that does not exist is an `Err`, not a panic and not a silent empty load |
| `edge_unicode_entries_survive_the_history_file` | unicode — `héllo — 世界 🌍` through the one-entry-per-line file format |
| `edge_blank_lines_in_the_history_file_are_skipped` | empty — blank lines do not become empty entries |
| `edge_empty_history_answers_every_question_without_panicking` | empty — `get(0)`, `search`, `search_all("")` on nothing |
| `edge_empty_and_whitespace_input_is_not_a_command` | empty — `""`, `"   "`, `"\t"` |
| `edge_bare_sigil_is_reported_not_executed` | boundary — a lone `:` names no command and does not index past the word |
| `edge_completion_ignores_the_case_of_the_prefix` | type/case boundary — `UP` and `up` complete alike, and the name's own spelling is what is offered |
| `edge_empty_prefix_offers_the_whole_vocabulary_sorted` | empty prefix — the whole vocabulary, sorted and deduplicated, and no session lines in it |
| `edge_a_session_name_that_duplicates_a_builtin_appears_once` | duplicate keys — `PI` in both places completes to one entry |
| `edge_completion_after_reset_forgets_the_sessions_names` | state — `:reset` drops bindings, so they cannot still be completed |
| `push_suppresses_a_consecutive_duplicate` / `push_keeps_a_repeated_line_that_is_not_consecutive` | duplicate entries — only *consecutive* duplicates collapse |
| `push_evicts_the_oldest_entry_at_max_size` / `push_evicts_before_it_adds_...` | boundary — eviction order, and the limit held after the push |
| `search_returns_the_most_recent_match` / `search_of_a_prefix_that_matches_nothing_returns_none` | empty — a hit and a miss |
| `parse_reads_the_whole_alias_table` | the regression this phase exists to prevent: 30 aliases, one table |
| `repl_runs_commands_and_quits`, `repl_shows_the_result_of_an_expression_under_vars`, `repl_runs_a_multiline_block` | the documented surface, asserted on stdout *and* exit code |

### Determinism

No wall-clock, no ports, no network. `SESSION_LIMIT` (20 s) is a liveness bound,
not a performance assertion — it converts the pre-fix infinite loop into a
failing test instead of an unkillable one; nothing asserts a duration. Files are
written only under `target/tmp/`, each with a process-id + counter name, and
removed afterwards. The one map in the REPL's output path is `variables`, and
`Repl::variable_lines` sorts it, so `:vars` prints in name order however the map
was filled — asserted over nine names rather than one, because a hash map
handing over a few keys in name order by chance is a chance the test would
otherwise pass on most runs and fail on the rest.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings, no `allow(` added |
| `cargo test --all-targets` | **848 passed, 0 failed, 0 ignored** (813 after the first review round) |
| `cargo test --doc` | pass, 1 passed |
| `./rbops/verify.sh phase-033` | **NOT RUN — the script is not in this checkout** |

The second review round's gates, re-run after the fixes below, are the same four:
**848 passed / 0 failed / 0 ignored**, fmt clean, clippy clean with no `allow(`
added, doc tests 1 passed.

`edge_a_decoded_chunk_runs_identically_to_the_compiled_one` in
`tests/bytecode_vm_test.rs` is flaky and is **not** fixed by either review round:
it read the corpus out of `examples/`, `modules/` and `tests/` and round-tripped
every `.rb` it found, and it failed on two of the runs of this round and on none
of the nine that followed. Nothing in this phase writes into those three
directories (every scratch file goes under `target/tmp/`), and running that test
alone passed nine times out of nine. It is a pre-existing race in a test that
shares a filesystem with the rest of the suite, and it is left alone rather than
narrowed: `#[ignore]`ing it or special-casing it would hide the next failure too.

**On the fourth gate, stated plainly:** `rbops/` does not exist in the working
directory (`ls rbops` → `No such file or directory`; `./rbops/verify.sh
phase-033` → exit 127). The dispatcher that would run it lives outside this
checkout and I was instructed not to inspect it. I have not run that gate and am
not claiming it passes. The three gates I could run are green, and
`./target/debug/rb run` over all 6 `examples/*.rb` and both `modules/*.rb`
succeeds, plus `rb test` → `Tests run: 274 / Passed: 274 / Failed: 0`.

## What the review round fixed

The seven findings, and what each fix is:

1. **`:load` never ran anything** (BLOCKER). `load_file` read the file and
   printed its size while `:help` promised "Load and run a file". It now runs the
   source through `execute_code`, so the file's declarations become the session's
   own — the difference from `:run`, which runs a file in a VM of its own.
2. **`load_from_file` truncated silently.** `reader.lines().map_while(Result::ok)`
   stopped at the first unreadable line and returned `Ok(())`, so a corrupt
   history file loaded as a good short history. It now propagates with
   `let line = line?`.
3. **`:vars` printed a `HashMap` in iteration order.** Names are sorted before
   printing, in `variable_lines` so the order is readable back by a test.
4. **EOF dropped an unclosed block silently.** `report_unclosed_block` prints the
   buffered lines and says they were never closed, rather than printing
   `Goodbye!` and losing them.
5. **Command arguments kept only their first word.** `:ast 1 + 1` parsed to
   `Ast("1")` and `:load /tmp/a b.rb` to `Load("/tmp/a")` — a path truncated to a
   directory. All six argument-taking commands now read the whole remainder, and
   the alias table asserts the argument as well as the variant, which it could not
   do before.
6. **`user_names` scanned only `globals`.** `to` binds with `declare` +
   `set_var`, which lands in `locals`, so a function the session had just defined
   was never offered. The local scopes are unioned in.
7. **The multiline detector matched the last word of a line.** `repeat 3 times`
   ends in `times`, `to greet(name)` in `)` and `for each i from 1 to 10` in `10`,
   so real blocks ran half-written, while a stray `end` matched and swallowed
   every line after it. Replaced with `parser::open_block_depth(&tokens)`, counted
   from the token stream.

## Invariants touched

- None. No change to `Value`, `Error`, the grammar, the `.rb` extension, `say`,
  `set x to …`, or `… end`. `ReplCommand` gains two variants and two derives —
  additive, on a type that was previously constructed by nobody. `parser` gains
  one `pub fn` and one private predicate; nothing that already existed changed
  behaviour.
- One behaviour change beyond the hang, and it is a fix: **the `.` sigil now
  works everywhere.** `handle_command` stripped only `:` while `parse` strips
  `:` or `.`, so `.quit` was previously reported as `Unknown command '.quit'`.
- One deliberate narrowing: `:history` now records at most `HISTORY_LIMIT` (1000)
  lines and drops consecutive duplicates, which `Repl::history`'s old `Vec`
  neither did. Nothing observable at 1000 lines.
- Two behaviour changes from the review round, both narrowing what a mistake is
  allowed to hide: an unreadable history file is an error rather than a short
  history, and a command given several words is given all of them. A path with a
  space in it now reaches the file it names instead of the directory before it.

## What the second review round fixed

All ten findings from the second round. The first one was a BLOCKER.

1. **The REPL could not use a variable it had just set** (BLOCKER). `run_code`
   analyzed every line against an empty scope, so `set x to 1` then `say x` was
   `AnalyzerError: Unknown variable 'x'` — the value was in the VM and the
   analyzer had never heard of the name. `Analyzer::with_bound_names` /
   `analyzer::analyze_with_bound_names` seed the program scope from the
   session's own bindings (`Vm::user_names`, which already knows what a `set`, a
   `to` and an `import` bound), and `run_code` seeds every line with them. A name
   the session never bound is still an unknown variable: the check is carried
   across lines, not turned off.
2. **`:funcs` was a stub.** It printed "User-defined functions are stored in the
   VM" and named none, and `tests/repl_test.rs` asserted that sentence, so the
   missing listing could not fail anything. `Vm::user_functions` +
   `Repl::function_lines` list what the session declared, in name order.
3. **`:inspect` looked only in the REPL's own map**, which holds `_` and nothing
   else, so `:inspect <session-name>` answered "not found" for every name the
   session had actually bound. It asks the VM through `Vm::user_value` now.
4. **A command's argument was rebuilt from words joined by one space**, so
   `:ast say "a   b"` asked about `say "a b"` and a path with two spaces in it
   named a file that does not exist. `ReplCommand::argument` slices the remainder
   out of the input instead.
5. **`:save` had no other half.** Nothing in the REPL ever called
   `load_from_file`, so a saved session could not be loaded. `:restore` reads the
   file back, and the round trip is asserted through the commands that write and
   read it, in a unit test and end to end.
6. **The completer shipped its own command list** — sixteen canonical names beside
   a parser that took thirty aliases, so `:q`, `:h`, `:hist`, `:v`, `:f`, `:l`,
   `:s`, `:i` and `:examples` all worked and none was completable. The parser's
   table is now the only one: `ReplCommand::names()` feeds the completer and
   `ReplCommand::help_lines()` feeds `:help`, which lists every alias too.
7. **`:example` promised a string interpolation `say` does not perform**:
   `say "Hello, {{name}}!"` printed `Hello, {{name}}!`. The examples and the help's
   quick reference are now `QUICK_REFERENCE` / `EXAMPLES` — source, not prose — and
   a test runs every entry, so neither list can show something the session refuses.
8. **The multiline buffer had no limit.** A piped block whose `end` never arrived
   buffered until the process was killed. `MAX_BLOCK_LINES` and `MAX_BLOCK_BYTES`
   drop the block and say so, and the unclosed-block report echoes at most
   `BLOCK_REPORT_LINES` of them and counts the rest.
9. **`save_to_file` / `load_from_file` took `&str`**, so a path that is not UTF-8
   could not be named at all. Both take `impl AsRef<Path>`.
10. **`to` was in the keyword list twice**, hidden by the downstream `dedup`.

Two more defects surfaced while fixing those, both pre-existing and both on the
same path:

- `Vm::run` prints the VM's whole output buffer at the end of a program and leaves
  it there, so every line re-printed every line before it: `say "A"` on the second
  line came back on every line after it. `Repl::run_program` drains it, and prints
  what is left on the error path — a program that fails half way through used to
  lose the `say` it had already run.
- `:vars` read the same near-empty map, so it listed `_` for a session that had
  bound a dozen names. It reads the VM's bindings as well, which closes the second
  half of `FINDINGS.md` #2.

## Invariants touched

- None of the language changed. No change to `Value`, `Error`, the grammar, the
  `.rb` extension, `say`, `set x to …`, or `… end`. `analyzer` and `vm` gain
  entry points (`analyze_with_bound_names`, `Vm::user_value`, `Vm::user_functions`);
  `get_var` now delegates to a new private borrowing `get_var_ref` and behaves
  identically. `ReplCommand` gains a `Restore` variant and the table that
  replaced its `match`.
- Three behaviour changes beyond the ten findings, all fixes: the REPL carries
  state across lines, `:vars` / `:funcs` / `:inspect` read the session rather than
  the REPL's own map, and a `say` is printed once.
- Two narrowing changes: a block past the buffer limit is dropped with a report
  rather than grown without bound, and an unclosed-block report echoes at most 20
  lines.

## Known gaps / follow-ups

- `rbops/verify.sh` could not be run (above). This phase is **not** gated on the
  project's own gate from inside this checkout.
- `ReplCompleter` is constructed and reachable via `Repl::complete`, but `rb`
  never calls it: there is no readline binding, so the read loop still uses
  `std::io::stdin().read_line`. Completion is API-only until a phase adds the
  interactive path. → `FINDINGS.md` #6