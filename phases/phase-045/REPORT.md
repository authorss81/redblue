# Phase 045 — Debugger: breakpoints, stepping and frame inspection in the REPL

## Finding reproduced

Verified on `main` before any change:

```
$ printf ':break 3\n:step\n:continue\n:frame\n' | cargo run --quiet --bin rb
Welcome to Redblue v0.1.0 - Interactive REPL
Type 'help' for commands, 'quit' to exit

>>> Unknown command 'break'. Type :help for available commands.
>>> Unknown command 'step'. Type :help for available commands.
>>> Unknown command 'continue'. Type :help for available commands.
>>> Unknown command 'frame'. Type :help for available commands.
>>> Goodbye!
```

```
$ grep -n "Break\|Step\|Continue\|Watch\|Backtrace\|Frame" src/repl/commands.rs | head
2:pub enum ReplCommand {
```

`ReplCommand` held `Quit, Help, Clear, History, Vars, Functions, Load, Save, Restore,
Reset, Run, Debug, Inspect, Ast, Tokens, Example` and nothing else.
`Repl::debug_mode` — the whole of `:debug` — printed `Debug mode - Not implemented yet`.
The finding stands.

## What changed

| File | Lines | What |
|---|---|---|
| `src/debugger.rs` | +348 | new module: `StepMode`, `DebugAction`, `DebugEvent`, `DebugHook`, `Frame`, `DebugSession`. Breakpoints (a `BTreeSet` of lines), the pending step, and `poll` — the one rule that decides whether the VM stops |
| `src/lib.rs` | +1 | `pub mod debugger;` |
| `src/interpreter.rs` | +71 −0 | `Vm::set_debug_hook`, `Vm::debug_before` called first in `execute_statement`, a `statement_depth` counter, and `builtin_names` so a frame can tell a program's names from the VM's own |
| `src/repl/commands.rs` | +73 −1 | `Break`, `Breaks`, `Delete`, `Step`, `Next`, `Continue`, `Frame` in the command table, with aliases, and 15 new rows in the table's own parse test |
| `src/repl/mod.rs` | +265 −4 | the session's `DebugSession`, the eight command handlers, `arm_debugger`, `line_number`, `debug_stop`, and `:debug` replaced with a real report |

The interpreter changed by addition only: `execute_statement` calls `debug_before`,
which returns immediately when no hook is installed. The hook is a
`Box<dyn FnMut(DebugEvent) -> DebugAction + Send>` that may only answer *what to do
next*. It cannot raise an error, cannot change a value, and is asked before
`charge_step`, so a stop does not consume the program's step budget. Every
diagnostic a program can raise still comes from the statement itself.

Nothing was added to the lexer, the parser, the AST or the grammar. A Redblue
program cannot name a breakpoint, a step or a frame — the tests prove it by typing
every debugger word without a sigil and asserting the *language* answers.

### How a breakpoint decides

`poll` is the only place that says stop. A breakpoint on line `L` is suppressed
while execution is inside the statement that stopped, and a stop records three
numbers: the line, the **call** depth and the **statement** depth. All three are
needed to tell the three ways one line comes round again from one another:

| Reaching the line again | call depth | statement depth | stops? |
|---|---|---|---|
| a nested statement of the same block (`if x then say "a" end` on one line) | same | deeper | no |
| the next iteration of a loop | same | same | yes |
| a recursive call | deeper | deeper | yes |

Rule: suppress only when line, call depth *and* statement depth all place the
statement strictly inside the one that stopped. The recursion case is why
statement depth alone is not enough: a recursive call produces a *deeper*
statement instance of the very same node.

### What the user gets

```
>>> :step
>>> ..  ..  ..  ..  ..  ..  Stopped at line 1: repeat 1 times
line 1 (depth 0)
(rb-debug) Stopped at line 2: to add(a, b)
line 2 (depth 0)
(rb-debug) Stopped at line 6: say add(2, 3)
line 6 (depth 0)
    add = <function add>
(rb-debug) Stopped at line 3: set total to a + b
line 3 (depth 1)
    a = 2
    add = <function add>
    b = 3
(rb-debug) 5
```

`:break <line>` (toggles), `:breaks`, `:delete <line>`, `:step` (into), `:next`
(over), `:continue`, `:frame`, and `:debug` for the list. At a stop the prompt is
`(rb-debug) ` and the answers are `c` / `s` / `n` / `f`, each also reachable with
a sigil. End of input resumes, so a piped session with a breakpoint in it still
ends when its input does.

## Tests added

`tests/debugger_test.rs` — 16 new `#[test]` functions, every one driving the real
`rb` binary over a pipe and asserting on what it printed.

| Test | Edge class covered |
|---|---|
| `breakpoint_in_a_loop_fires_on_every_iteration` | loop / `continue` resumes to the next iteration |
| `step_over_runs_the_call_and_step_into_descends_into_it` | step-over, step-into, frame listing inside a call |
| `a_recursive_call_reaches_the_same_breakpoint_at_every_depth` | nesting_recursion — the same line at depth 1 and depth 2 |
| `a_program_that_fails_still_reports_its_own_error_under_the_debugger` | **asserts a failure**: `1 + "a"` still reaches the user as `Cannot add non-numbers` while stopped on that line. type_mismatch |
| `a_program_that_will_not_parse_is_reported_without_ever_stopping` | malformed_input — a program that never ran never stops |
| `the_debugger_is_not_reachable_from_redblue_source` | the grammar is unchanged: every debugger word without a sigil reaches the language |
| `frame_at_the_prompt_reports_the_last_stop_and_nothing_before_one` | `Nothing has stopped yet.` before the first stop; `total = 3` after it |
| `unicode_in_a_stopped_frame_is_shown_intact` | unicode — `héllo 🌍 世界` unescaped and untruncated |
| `edge_break_on_a_line_the_program_never_reaches_is_simply_never_taken` | out_of_bounds / boundary — line 999 on a 3-line program |
| `edge_frame_of_a_stop_with_nothing_bound_lists_no_names` | empty — and no `<builtin>` from the VM's own table |
| `edge_end_of_input_at_a_stop_resumes_rather_than_hanging` | resource_limit / state — EOF at a stop ends the session |
| `edge_a_line_number_that_is_not_a_line_is_refused` | **asserts a failure** ×4 — no argument, `abc`, `0`, `-1`, `1 2`, an overflowing literal. numeric_boundary, malformed_input |
| `edge_break_twice_clears_the_breakpoint_and_delete_says_what_it_did` | empty / duplicate — set, toggle off, delete, delete again |
| `edge_a_loop_over_the_iteration_cap_still_stops_and_still_ends` | resource_limit — `REDBLUE_MAX_ITERATIONS=3`: three stops, then the interpreter's own cap still fires |
| `edge_a_breakpoint_on_a_one_line_program_fires_once` | singleton, at both ends of the range |
| `edge_a_frame_lists_the_same_names_in_the_same_order_twice` | determinism — the same session printed twice is byte-identical, and the listing is ordered by name rather than by map iteration order |

`src/repl/commands.rs` gained 15 rows in `parse_reads_the_whole_alias_table`
(`:break`/`:b`, `:breaks`/`:bps`, `:delete`/`:d`, `:step`/`:st`, `:next`/`:n`,
`:continue`/`:c`, `:frame`/`:fr`) and their arguments in the `carried` match. The
existing test was strengthened, not changed: it still runs its original 31 rows.

### Edge-case matrix

- **empty** — covered (`edge_frame_of_a_stop_with_nothing_bound_lists_no_names`,
  `edge_break_twice_…` / `No breakpoints set.`)
- **singleton** — covered (`edge_a_breakpoint_on_a_one_line_program_fires_once`)
- **boundary** — covered (`:break 999` past the end; `:break 0` at the start)
- **out_of_bounds** — covered (`edge_break_on_a_line_the_program_never_reaches…`;
  a breakpoint on a line no statement is written on never fires and never panics)
- **type_mismatch** — covered (`a_program_that_fails_still_reports_its_own_error…`)
- **numeric_boundary** — covered (`edge_a_line_number_that_is_not_a_line_is_refused`,
  which includes a literal too large for `usize`)
- **unicode** — covered (`unicode_in_a_stopped_frame_is_shown_intact`)
- **nesting_recursion** — covered (`a_recursive_call_reaches_the_same_breakpoint_at_every_depth`)
- **duplicate_missing_keys** — N/A. The debugger reports *bindings*, not record
  keys; it never reads, writes or merges a record. A missing field is a runtime
  error the program raises, and that the debugger does not swallow is exactly what
  `a_program_that_fails_still_reports_its_own_error_under_the_debugger` proves.
- **malformed_input** — covered (`a_program_that_will_not_parse_is_reported_without_ever_stopping`,
  and the refused-argument rows of `edge_a_line_number_that_is_not_a_line_is_refused`)
- **resource_limit** — covered (`edge_end_of_input_at_a_stop_resumes_rather_than_hanging`,
  `edge_a_loop_over_the_iteration_cap_still_stops_and_still_ends`)

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 1331 passed, 0 failed, 0 ignored (49 test binaries) |
| `cargo test --doc` | 2 passed, 0 failed |
| `rb run examples/*.rb` | every example still runs, exit 0 |
| `./rbops/verify.sh phase-045` | **not run — `rbops/` is not in this checkout** (see below) |

### The fourth gate, honestly

The brief names `./rbops/verify.sh phase-045` as the fourth gate. This working
directory is the project checkout only: `ls rbops` returns `No such file or
directory`, and the brief instructs me not to reach outside it. I could not run
that script and am not claiming a result for it. Everything it is described as
checking that I *could* run, I ran: the two cargo gates, the full test suite, and
the examples. No test was deleted, skipped, `#[ignore]`d, or given an
`allow(clippy::…)`, and no existing assertion was loosened.

## Invariants touched

- None. `.rb` is still the extension; `to … end` / `if … end` still close with
  `end`; `set x to <expr>` is still assignment; `say` still prints; `Value` and
  `Error` are untouched; no string syntax changed; `tests/` all pass.
- `ReplCommand` gained seven variants. It is `pub`, so this is an additive API
  change — the one public-surface change the phase asks for.
- `Vm` gained one public method, `set_debug_hook`. No existing signature moved.

## Known gaps / follow-ups

- `:run <file>` and `rb run` are **not** debugged. They build a fresh VM inside
  `run_isolated_with_depth_in`, so the hook never reaches them; only a program
  typed or buffered at the REPL prompt does. Threading a hook through
  `run_isolated_*` is a separate change to the `rb run` path and is recorded in
  `FINDINGS.md` rather than smuggled in here.
- There is no `Watch` and no `Backtrace`. `DebugEvent` carries one frame's worth
  of bindings; a watch expression would have to be evaluated by the VM from
  inside the hook, which is the one thing this design deliberately does not allow
  (the hook observes, it does not run the program). Both are in `FINDINGS.md`.
- Line numbers are the *program's*, which at the REPL is the buffered block.
  `:break 2` before a four-line block means line 2 of that block, not line 2 of
  the session. That is stated in `:help`'s description column and is what a REPL
  debugger can honestly offer; a file-based debugger would use the file's lines.
