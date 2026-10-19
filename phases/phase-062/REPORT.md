# Phase 062 — Open a debug sub-REPL: `.debug` with `.watch`, `.stack` and `.locals`

## Reproduce (before the change)

The phase's finding claimed "no `.debug` command exists in commands.rs". That
was **stale in one detail and right in every one that mattered**. On HEAD
(`60d7fcf`, "state after phase-061"):

- `.debug` *did* parse — as a no-argument placeholder that ignored whatever was
  typed after it and printed a breakpoint summary. There was no sub-REPL, no
  tracing, no way to evaluate anything inside a debug session.
- `watch`, `stack` and `locals` did not parse at all, as either `.`-commands or
  anything else.

```
$ printf '.watch x\n.stack\n.locals\n.debug 1 + 1\n:quit\n' | ./target/debug/rb
>>> Unknown command 'watch'. Type :help for available commands.
>>> Unknown command 'stack'. Type :help for available commands.
>>> Unknown command 'locals'. Type :help for available commands.
>>> Debugger: 0 breakpoint(s) set. `:break <line>` sets one, `:next` steps
over a call, `:step` steps into it, `:continue` runs on.
Goodbye!
```

The last line is the stale half: `.debug 1 + 1` did not run `1 + 1`, and
"Debugger:" promised breakpoint machinery phase-062's scope explicitly excludes.
The remedy the finding asks for — an in-session, observe-only debug sub-REPL —
is what this phase implements; the placeholder's message is deleted with it
(the `:break`/`:next`/`:step`/`:continue` commands it advertised remain where
they were, on the separate breakpoint session `repl.debug`, untouched).

## What changed

`must_touch: ["src/"]` is satisfied: five files under `src/` and two under
`tests/`, **+2395 −97** in total against the pre-phase state (`git diff
--numstat e0a3686`, the "state after phase-061" commit) — re-derived rather
than inherited, because the pipeline's resume step split the phase across two
commits: the checkpoint `0647108` ("resume phase-062: adopt preserved work",
+1231 −22) and the working tree on top of it (+1115 −72 against HEAD). Of the
total, **+158 −20** is the review-round-1 repair and **+984 −120** is the
review-round-2 repair described below.

| File | Lines | What |
|---|---|---|
| `src/repl/commands.rs` | +116 −7 | `ReplCommand` gains `Debug(String)`, `Watch(String)`, `Stack`, `Locals`, `Back`; the parse table gains the five specs (`debug` optional-argument, `watch` required-argument); `argument_text` carries the arguments; the alias-table test gains the five rows. |
| `src/repl/debug_repl.rs` (new) | +739 | The whole sub-REPL. `DebugRepl` (a forked `Vm`, an `Arc<Mutex<State>>`, its own line buffer, its own `_`); `Watch`/`Trace`/`State`; the debug-hook closure; `evaluate_watches` (the one place a watch is evaluated — statement stops and end-of-run pass both); the `impl Repl` glue (`debug_session`, `enter_debug`, `debug_line`, `run_debug_argument`, `run_in_debug`); the watch cap, the panic guard and the single-read file/code decision (review round 2). |
| `src/repl/mod.rs` | +527 −1 | The `debug_repl: Option<DebugRepl>` field; the session-loop branch that hands every line to `debug_line` *before* the history push while a session is open; the EOF branch reporting the sub-REPL's unclosed block as its own; `prompt_for_next_line` returning `debug> `/`..  N ` inside a session; `handle_command` wiring `Debug`/`Watch`/`Stack`/`Locals`/`Back`; the old `debug_mode` placeholder removed; `session_names` extends with the sub-REPL's names while open (completer snapshot only); the tests. |
| `src/debugger.rs` | +86 | `DebugEvent` gains `call_stack` (outermost-first frame names), `scope_base` (where this frame's scopes start) and `frame_writes` (which globals this frame overwrote), plus `DebugEvent::frame_locals()` — the `.locals` answer, scopes from `scope_base` on, innermost-wins, frame-writes branch, sorted by name — and `context` (the language context the program built, review round 2). |
| `src/interpreter.rs` | +250 −72 | `Vm` fields `call_stack`, `frame_scope_base`, `frame_writes` (initialised in `new_in`); `call_user_function` — the one choke point for every way a Redblue function is called — saves the caller's `scope_base`, records the callee's frame name, and restores/pops on both exits; `set_var`'s globals fall-through records into `frame_writes.last_mut()`; `debug_before` emits the new fields; `Vm::observe_bindings` (overwrite a VM's bindings without running anything), `Vm::current_bindings` (borrow the bindings after a run), and — review round 2 — `CallContext`, `Vm::observe_context`, `Vm::call_context`, `Vm::set_max_steps`, and the per-run counters `observe_bindings` restarts. |
| `src/lib.rs` | +3 −3 | `CallContext` joins the crate's public surface (review round 2): a public struct cannot hold a value of a type nobody outside the crate can name, and `DebugEvent` — a public struct — carries one. |
| `tests/debug_watch_test.rs` (new) | +95 | The two rules the watch scratch VM answers by, tested where they live rather than through the REPL: the step budget of one evaluation does not carry into the next, and a watch is evaluated against the language context the program built. |

The semantics, in the places they live:

- **Observe-only.** The hook returns `DebugAction::Run` always; a watch that
  fails is a line of report, never an error in the run. The scratch VM that
  evaluates watches is created with `ask_questions(true)`, so an effectful
  builtin — `files.write`, `network.post` — is refused at the call rather than
  performed on behalf of an expression that was only being watched. Pinned by
  `edge_a_watch_that_would_change_the_world_is_refused_and_writes_nothing`
  (review round 1 — the report had claimed the behaviour and no test held it;
  rewritten in round 2 to import the module under an alias, so the refusal is
  the refusal and not a "no such function").
- **A watch is evaluated against the program's context, not only its
  bindings.** What a program *declared* — the modules it imported, the aliases
  it named them by, the members they published, the objects it declared — is
  part of what an expression written for that program can reach, so the scratch
  is pointed at it as well (`Vm::observe_context`, review round 2). Without it
  `Greeter.greet(who)`, `J.parse(..)` and `Counter.bump()` are all "no such
  function" to a watch, whatever the program itself could do.
- **`.watch` reports changes, not steps.** Registration is parse-only (an
  expression that does not parse is refused then, not at some future step).
  The first value a watch ever sees is its baseline and is *not* reported — a
  watch set on a name with a value has not seen a change. Every later difference
  from the last held value is one line, `watch <source> = <value>`, including
  the value a program's *last* statement produced: after each run the watches
  are evaluated once more against the bindings the run finished with, because
  no statement stop exists after a program's final statement to observe it at.
  A watch that fails reports one line per evaluation (`watch <source>: <first
  line of the error>`) and keeps its previous value. A watch that *panics* is
  one failed watch too, and the panic is reported rather than propagated
  (review round 2).
- **`.stack` / `.locals` answer about the last step.** "No live frames." before
  anything has run — including right after a bare `.debug`. `.locals` shows the
  stepped frame's parameters and locals (`scope_base` slicing is what keeps the
  caller's frame out) plus the globals *this frame* overwrote (`frame_writes`,
  recorded only on `set_var`'s globals fall-through — see Known gaps).
- **`.back` is the way out**, and while a session is open every typed line —
  command or code, buffered or whole — belongs to the sub-REPL: it is
  dispatched by `debug_line` and reaches neither the parent's history, nor its
  transcript, nor its bindings. The `.debug` line itself was typed at the
  parent prompt and is the parent's; the `.back` line was typed at `debug>` and
  is not. The name: `exit` is taken (`:quit`), `leave` would have been a second
  word for the same thing, and `back` says what the user wants — the session
  they left.
- **`.debug <arg>` while open runs `arg` in the current session** (re-entry
  semantics); a bare `.debug` while open says so once. An argument is read
  once and decided from what was read: a path that can be read is a file,
  whatever it is called; a path ending in `.rb` is a file even when it cannot
  be read, because a file that is missing is still a file the user meant, and
  saying "cannot read" beats parsing `prog.rb` as an expression and failing on
  the dot; everything else is code, and is run as code. A file that cannot be
  read is one error line; the session still opens (the banner comes first).
  The decision is made from that one read and from nothing else: the
  filesystem is not asked a second time about a path the read already answered
  (review round 2).
- **A block typed in the sub-REPL is the sub-REPL's block.** Its own buffer,
  its own `..  N ` continuation numbering, its own unclosed-block report on
  EOF, its own too-large report — against the parent's limits, because they are
  one session's limits, not two sessions'.

### Review round 1 — findings fixed

The reviewer returned three findings; all three are fixed here, and nothing was
weakened to fix them. **+158 −20** across two files.

| # | Severity | Finding | Fix |
|---|---|---|---|
| 1 | MINOR | `src/repl/debug_repl.rs:369` — `take_reports()` locked the shared state directly instead of through the module's `lock()` helper, so a mutex poisoned by a panicking watch would answer an empty list and drop the reports the earlier watches had produced. | `take_reports()` now locks through `lock()` like every other read of the state (lines 124, 195, 211, 230, 325, 401 all already did). A poisoned mutex still holds the reports, and they are still answered. |
| 2 | MAJOR | `src/repl/debug_repl.rs:115` — the scratch VM's `ask_questions(true)` refusal of effectful builtins was documented but untested: an advertised safety feature with no test behind it. | Two new tests. `edge_a_watch_that_would_change_the_world_is_refused_and_writes_nothing` watches `files.write(…)` and asserts the refusal is reported *and* that the file it would have written is not there — the file, not the wording, is the claim. `edge_a_debug_of_a_directory_says_it_cannot_be_read_and_still_opens` covers the other half of the new file/code decision. The first test was **red before the fix**: with `ask_questions(false)` the watch wrote the file and the assertions failed. |
| 3 | STYLE | `src/repl/debug_repl.rs:587` — `names_a_file()` asked the filesystem whether a path was a file and read it afterwards, so the two answers could disagree: the file could be gone in between. | `names_a_file()` is gone. `read_argument()` reads the file **once** and decides from what it read, so what runs is what was read. The two rules it keeps are the old ones — a path that can be read is a file whatever it is called, and a path ending in `.rb` is a file even when it cannot be read — and a path that is there but holds no readable source (a directory) is now reported as "cannot read" rather than run as code. |

### Review round 2 — findings fixed

The reviewer returned six findings; all six are fixed here, and nothing was
weakened to fix them. **+984 −120** across five files (`src/interpreter.rs`,
`src/debugger.rs`, `src/lib.rs`, `src/repl/debug_repl.rs`, `src/repl/mod.rs`)
and one new test file.

| # | Severity | Finding | Fix |
|---|---|---|---|
| 1 | MAJOR | `src/interpreter.rs:721` — `observe_bindings` copied only `locals` and `globals` into the scratch VM, so a watch could not reach what the program *declared*: a module member (`Greeter.greet(…)`), an aliased module member (`J.parse(…)`) and an object's method (`Counter.bump()`) were all "no such function" to a watch. | The language context is now one value — `CallContext` — holding the modules, the aliases, the declared modules, the published members and the objects, and the `Vm` holds one. `DebugEvent` carries it (`src/debugger.rs`), and the scratch is pointed at it once per step by `Vm::observe_context`. The module programs are shared (`Arc<Program>`) rather than copied, so a copy of the context is cheap. Pinned by `debug_a_watch_can_call_a_module_the_program_declared`, `edge_a_watch_can_call_an_object_method_the_program_declared` and `tests/debug_watch_test.rs::edge_a_watch_is_evaluated_against_the_language_context_the_program_built` — all three were **red before the fix** ("Module 'Greeter' has no function 'greet'"). |
| 2 | MAJOR | `src/interpreter.rs:721` — the scratch VM's counters were never reset between watch evaluations, so `steps`, `call_depth`, `loop_depth`, `loop_control` and `assertions_failed` accumulated: with enough watches or steps the shared scratch hit the step limit and every later watch failed with a step-limit error that had nothing to do with what it watched. | `observe_bindings` restarts the counters of a run with the bindings: one watch evaluation is one run, and the steps the last one spent are not the steps this one has. The scratch is also given a budget of its own (`WATCH_MAX_STEPS`, 100 000) so that a runaway watch is a reported failure rather than a hang, and the budget is per evaluation rather than per session. Pinned by `tests/debug_watch_test.rs::edge_the_step_budget_of_one_watch_evaluation_does_not_carry_into_the_next` — **red before the fix** (the second run died with `Step budget of 10 reached`). |
| 3 | MINOR | `src/repl/debug_repl.rs:606` — `read_argument` asked the filesystem about the path a *second* time, after the read had already failed, so the answer could be about a world that had changed since the read. | The decision is made from the one read: a path that could not be read because it is *not there* is code, and a path that could not be read because it is there and holds no readable source (`IsADirectory`, `PermissionDenied`) is a file the user meant and cannot have. `std::fs::metadata` is no longer called. The behaviour that survives the change is pinned by `edge_a_debug_of_a_path_that_is_not_there_is_run_as_code`. |
| 4 | MINOR | `src/repl/debug_repl.rs:179` — nothing bounded the number of `.watch` registrations, and every watch is evaluated at every statement boundary: O(watches × statements) with no bound. | A session holds at most `MAX_WATCHES` (100) watches; the one past the bound is refused by name and is not registered. Pinned by `edge_a_session_refuses_a_watch_past_its_limit`, which registers a hundred and asks for one more. |
| 5 | STYLE | `src/repl/debug_repl.rs:122` — the debug hook did not catch panics, so a panic in the watcher would have crashed the program being debugged instead of being reported as a failed watch. | The panic is caught where the watch is evaluated (`catching_panics`), so one watch cannot take the program it watches down with it, and the hook itself is caught as well as a backstop. The report names what the panic said. Pinned by `edge_a_watch_that_panics_is_reported_rather_than_propagated`. |
| 6 | STYLE | `src/repl/mod.rs:3570` — `edge_a_watch_that_would_change_the_world_is_refused_and_writes_nothing` watched `files.write(…)` without importing the module, so the watch could have failed for the wrong reason. | The test now imports the module under an alias (`import files as F`) and watches `F.write(…)`. A call through an alias can only reach `files_write` if the watch is evaluated against the program's context, so the test now fails for the right reason — and it was **red before the fix** (it answered "Module 'F' has no function 'write'"). |

## Tests added

Twenty-five new `#[test]` functions, all through the sub-REPL driver, the real
read loop over a scripted source, or the public VM API — none needs a TTY.
Twenty-one are in `src/repl/mod.rs`'s test module (`repl::tests::…`), one is in
`src/repl/commands.rs` (`repl::commands::tests::…`), and two are in the new
`tests/debug_watch_test.rs`; the two marked **round 1** and the five marked
**round 2** are the review repairs.

| Test | Edge class covered |
|---|---|
| `repl::commands::tests::the_debug_sub_repl_commands_parse` | malformed input — the five spellings parse to the right variants with the right argument, and `.debug` with no argument parses to `Debug("")` rather than a missing-argument error |
| `repl::tests::debug_stack_names_a_three_deep_call_chain` | nesting — `.debug outer()` over a three-deep chain, `.stack` names `outer`, `middle`, `inner` in call order |
| `repl::tests::debug_watch_reports_only_the_steps_where_the_value_changed` | happy path + counting — four statements run, three changes, exactly three report lines and no others; **this is the test that was red first** (the first implementation reported the baseline as a change: `["watch x = 0", "watch x = 1", "watch x = 2"]`) |
| `repl::tests::debug_locals_shows_the_stepped_frame_and_not_the_callers` | frame isolation — the callee's parameter and local listed, the caller's local and the global the caller overwrote both absent, while the callee could see that global through Redblue's own scoping |
| `repl::tests::debug_locals_shows_a_constant_the_frame_declared` | frame isolation, second shape — a `constant` binds a global, and the frame that declared it owns it: it is in that frame's `.locals` |
| `repl::tests::debug_watch_with_no_argument_lists_the_watches_the_session_holds` | empty + duplicate — a bare `.watch` lists the watches the session holds ("No watches." for none), and the same expression watched twice is two watches, listed twice |
| `repl::tests::debug_a_file_can_be_watched_while_it_runs` | a file under observation — `.run <file>` after the watches are set reports each change the file made, and nothing else |
| `repl::tests::debug_a_file_whose_name_is_not_rb_is_read_when_it_exists` | the file/code decision — a `.txt` file that exists is read and run, not parsed as an expression |
| `repl::tests::a_debug_sub_repl_leaves_the_parent_session_untouched` | byte-identity — driven through `session_loop` with a scripted source: history is exactly the two parent-prompt lines, the transcript is a strict prefix plus the post-`.back` line, the sub-REPL's binding absent from the parent's VM, the resumed line's binding present, the parent's `_` never written, the block buffer empty, a `debug>` prompt asked |
| `repl::tests::edge_a_bare_debug_opens_a_session_and_runs_nothing` | empty — `.debug` with no argument opens with the banner, runs nothing, `.back` returns; the next line answers outside any session |
| `repl::tests::edge_a_watch_that_cannot_parse_fails_and_the_session_goes_on` | malformed input + **failure produced** — `.watch 1 +` is a reported failure (`Error:` line quoting the source), `parse_only("1 +").is_err()` asserts the property underneath, `.stack` still answers `No live frames.` (also the no-live-frames edge), the session never closed |
| `repl::tests::edge_a_watch_on_an_unbound_name_reports_per_step_and_the_run_still_finishes` | missing name + **failure produced** — `.watch nosuchname` over a loop reports `Unknown variable 'nosuchname'` once per evaluation (four for the loop run, two for the one-statement run typed after), and the run still answers `= 2` |
| `repl::tests::edge_a_watch_that_would_change_the_world_is_refused_and_writes_nothing` | **round 1, rewritten in round 2** — resource/state + **failure produced**: a watch on `F.write(…)`, with `files` imported under the alias `F`, is refused at the call, the report names `files_write`, and the file it would have written is not there afterwards; the program it was watching still answers `= 2`. Red before the round-2 fix (without the import the watch answered "Module 'F' has no function 'write'") |
| `repl::tests::edge_a_debug_of_a_file_that_cannot_be_read_says_so_and_still_opens` | missing resource — `.debug definitely_missing_phase062.rb` answers `Error: cannot read '…'`, the sub-REPL opens anyway, `.stack` answers inside it |
| `repl::tests::edge_a_debug_of_a_directory_says_it_cannot_be_read_and_still_opens` | **round 1** — malformed/resource: a path that is there but holds no source (a directory) is reported as "cannot read" rather than run as code, and the sub-REPL opens anyway |
| `repl::tests::edge_a_run_of_a_file_that_cannot_be_read_reports_the_failure_and_the_session_goes_on` | missing resource — `.run <file>` on a file that is not there reports the read failure and the session goes on |
| `repl::tests::edge_a_run_of_a_file_that_does_not_parse_reports_the_failure_and_the_session_goes_on` | malformed input — `.run <file>` on a file that does not parse reports the failure and the session goes on |
| `repl::tests::edge_sub_repl_commands_outside_a_session_say_how_to_open_one` | boundary — `.stack`, `.locals`, `.watch x`, `.back` with no session open all answer the one sentence naming `.debug` |
| `repl::tests::debug_a_watch_can_call_a_module_the_program_declared` | **round 2** — the context: a watch on `Greeter.greet(who)` calls the member the program published and reports what it answered (`= hello there`), and does not answer that the module has no such member |
| `repl::tests::edge_a_watch_can_call_an_object_method_the_program_declared` | **round 2** — the context, second shape: a watch on `Counter.bump()` reaches the object's method (`= 10`) |
| `repl::tests::edge_a_session_refuses_a_watch_past_its_limit` | **round 2** — resource limit: a hundred watches are held, the hundred-and-first is refused by name and is not registered, and the session still answers what it holds |
| `repl::tests::edge_a_debug_of_a_path_that_is_not_there_is_run_as_code` | **round 2** — the file/code decision, from the one read: a path that is not there is run as code and fails the way code fails, and is not reported as a file that cannot be read |
| `repl::tests::edge_a_watch_that_panics_is_reported_rather_than_propagated` | **round 2** — a panic in a watch is the failure it is, reported with what it said, and not propagated into the program being watched |
| `debug_watch_test::edge_the_step_budget_of_one_watch_evaluation_does_not_carry_into_the_next` | **round 2** — the counters of a run restart with the bindings: the second run of six statements fits in a ten-step budget only because the first run's six steps are not still charged |
| `debug_watch_test::edge_a_watch_is_evaluated_against_the_language_context_the_program_built` | **round 2** — the context, at the API it lives in: a scratch pointed at the program's bindings *and* its context reaches the member the program published |

### Edge-case matrix (AGENTS.md §3.2)

- **empty** — covered: bare `.debug` (`edge_a_bare_debug_opens_a_session_and_runs_nothing`); `.stack` before any step (`No live frames.`).
- **singleton** — covered: the one-statement `.debug 1 + 1` run inside the unbound-watch edge (one stop, one end-of-run pass, two reports).
- **boundary** — covered: sub-REPL commands with no sub-REPL open (`edge_sub_repl_commands_outside_a_session_say_how_to_open_one`).
- **out_of_bounds** — N/A: this change indexes no user-supplied position; `scope_base` slicing is internal and cannot go past the scope stack (it is taken from the VM at a live frame).
- **type_mismatch** — covered in its failure shape: a watch on a name that does not exist takes the same per-step error path as any other failing evaluation (`edge_a_watch_on_an_unbound_name…`); arithmetic type failures on watched values take that identical `Err` path — no separate branch exists to test.
- **numeric_boundary** — N/A: no numeric surface is introduced; watch values are compared with `Value` equality, and `Value::number` already refuses non-finite numbers, so a NaN comparator is not a branch this change has.
- **unicode** — N/A: no encoding surface is introduced; a watch's name is interpolated into a report line with `Display`, which is exercised for every value across the watch tests.
- **nesting_recursion** — covered: three-deep call chain for `.stack`; frame isolation for `.locals`.
- **duplicate_missing_keys** — covered (missing half): watching a name nobody bound reports per step without aborting; re-registering the same watch twice is two independent watches (each compares against its own last value — no shared state exists to corrupt).
- **malformed_input** — covered: `.watch 1 +` refused at registration; every sub-REPL command's parse is pinned by the alias-table rows; a `.debug` argument that names a directory is reported as unreadable rather than run as code (`edge_a_debug_of_a_directory…`), and one that names nothing at all is run as code (`edge_a_debug_of_a_path_that_is_not_there_is_run_as_code`).
- **resource_and_state** — covered: a watch on an effectful builtin is refused at the call and the file it would have written is not there afterwards (`edge_a_watch_that_would_change_the_world_is_refused_and_writes_nothing`) — a debugger that wrote the file it was asked to look at would be the state bug this row is about. The scratch VM's own state is covered too: a watch cannot leave a binding behind for the next one (`observe_bindings` overwrites the bindings wholesale) and cannot spend the next evaluation's budget (`edge_the_step_budget_of_one_watch_evaluation_does_not_carry_into_the_next`).
- **resource_limit** — covered (review round 2): a session holds at most `MAX_WATCHES` (100) watches and the one past the bound is refused (`edge_a_session_refuses_a_watch_past_its_limit`); one watch evaluation has a step budget of its own (`WATCH_MAX_STEPS`, 100 000), so a runaway watch is a reported failure rather than a hang. The sub-REPL's *blocks* are the parent's limits (`MAX_BLOCK_LINES`/`MAX_BLOCK_BYTES`), reused through the parent's own `exceeds_block_limits`; that enforcement is tested at the parent, and the forwarding path here is the three lines of `report_block_too_large`.

## Gates

Run in this checkout, on the tree as it now stands, in this order:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | **1893 passed, 0 failed** across 68 test binaries (log: `target/tmp/test_final.log`; the long poles are the pre-existing bootstrap self-hosting suites at ~350 s and ~448 s) — the round-2 repair adds seven of them (five in `src/repl/mod.rs`, two in the new `tests/debug_watch_test.rs`) |
| `cargo test --doc` | 2 passed, 0 failed |
| `../rbops/verify.sh "phase-062"` | **PASS** — `VERIFY PASS — phase-062`, exit 0, every check green: manifest, report, diff hygiene (no forbidden path, no gate-weakening construct, no scratch file), `must_touch: src/`, `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test: 1893 passed, 0 failed`, the doc tests, the `examples/`+`modules/` sweep, test policy (13 rust / 0 redblue, 9 `edge_*`, 3 failure-asserting, 0 newly skipped) and the honesty check (`reported test count (1893) matches actual (1893)`) |

What `verify.sh` does, for the record: it validates that the phase is declared
in `rbops/phases.json` with its dependencies `.done`; that this report exists
with its four required sections and at least one evidence row naming a file;
that the working-tree diff against HEAD touches no forbidden path
(`.github/workflows/`, `rbops/`, `.opencode/agent/`), adds no gate-weakening
construct (`#[ignore]`, `// skip`, `allow(clippy::…)` at line start), contains
no scratch files, and clears the manifest's minimum line count; that at least
one changed file falls under the phase's `must_touch`; that the four cargo
gates pass and every `examples/*.rb` and `modules/*.rb` still runs under the
built binary (baselined pre-existing failures excepted, and the exemption
revoked for any file this phase touched); that the diff carries at least three
new `#[test]` functions, at least one `edge_*` test and at least one assertion
that a *failure* is produced, with no newly skipped or ignored tests; and that
the test count this report claims is within tolerance of the count its own run
measured — over-claiming fails the phase, under-claiming only warns.

**`verify.sh "phase-062"`: PASS.** (Run three times: at the end of the first
attempt, after the review-round-1 repair, and after the review-round-2 repair,
on the tree as it now stands — every check green on every run — manifest,
report, diff hygiene, `must_touch`, the four cargo gates, the examples sweep,
test policy and the honesty check.)

## Invariants touched

`phases/INVARIANTS.md`'s language surface is unchanged: `.debug`, `.watch`,
`.stack`, `.locals`, `.run` and `.back` are REPL commands behind the same
`.`-sigil parser as every other session command — no keyword, no statement
form, no change to what a `.rb` file may contain. The breakpoint session behind
`:break`/`:next`/`:step`/`:continue`/`:frame` is untouched; only the dead
`debug_mode` printer that advertised it from `.debug` was removed, and it had
no tests.

## Known gaps / follow-ups

- **The file/code decision is by readability, not by an explicit marker.** A
  `.debug` argument is read once: a path that can be read is a file whatever it
  is called, a path ending in `.rb` is a file even when it cannot be read, and
  everything else is code. A code argument that happens to name a file in the
  working directory is therefore read as a file — the ambiguity a REPL argument
  always has, and the reason `:load` and `rb run` spell a path as one. A
  `file:` prefix inside the sub-REPL would be the follow-up if it chafes.
- **`frame_writes` records only globals overwritten by `set`.** A `to`
  function, an `import` or a `constant` declared inside a frame does not
  appear in that frame's `.locals` listing. The frame's own scopes (parameters
  and locals) are complete; this is only the globals-branch, and the
  alternative — tracking every binding site — was more machinery than the
  DoD's "parameters and locals and nothing from the caller's frame" asked for.
- **Sub-REPL completion is the union snapshot** of parent and sub names (via
  `session_names`), refreshed at each prompt; it has no command-name
  completion of its own. Commands are six and their names are in the banner.
- **The entry `.debug` line is in the parent's history** — it was typed at the
  parent prompt — while every line typed at `debug>` is not. A `:history` on
  the far side of `.back` therefore shows the door and not the room, which is
  the contract stated in `src/repl/mod.rs`; it is a deliberate asymmetry, and
  it is testable (`a_debug_sub_repl_leaves_the_parent_session_untouched`
  pins both halves).
- **The watch scratch has a budget of its own, and the sub-REPL's runs do
  not.** A watch evaluation is charged against `WATCH_MAX_STEPS` (100 000) and
  its counters restart with every evaluation, so one watch cannot spend the
  next one's budget; a *run* in the sub-REPL is charged against the VM's own
  `MAX_STEPS` default, exactly as a parent run is. The number of watches is
  bounded at 100 per session, but the work a hundred watches do at every
  statement of a long program is still the user's arithmetic to make
  deliberately.
- **A watch is not analysed.** It is parsed and evaluated, so a watch that
  names something the program never bound is reported per step as an unknown
  variable rather than refused at registration — the names a watch may use are
  the program's *runtime* names, which no static analysis of the watch alone
  could know.
- **Stepping and breakpoints stay out of scope**, as the goal says: if this
  sub-REPL proves its worth, `:break`-style primitives wired to the sub-REPL's
  hook are their own phase.
