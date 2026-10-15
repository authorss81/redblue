# Phase 056 — Teach the REPL to answer questions about code: `.type`, `.doc`, `.which`, `.sighelp`

## What changed

The four commands land in a new pure module (`src/repl/introspect.rs`) plus three
tables/facts that already had to exist somewhere. Nothing in the language surface
moved.

| File | Lines | What |
|---|---|---|
| `src/repl/introspect.rs` | +568 −0 | **New file.** The pure half: `Origin`, `Definition`, `classify`, `which_lines`, `doc_lines`, `sighelp_lines`, `type_lines`. No VM, no terminal, so every answer is assertable. |
| `src/repl/commands.rs` | +109 −0 | `ReplCommand::{Type, Doc, Which, Sighelp}`, four rows in `COMMAND_TABLE` (so `:help` and the completer pick them up from the one table), and `ReplCommand::argument_text`. |
| `src/repl/mod.rs` | +676 −2 | `definitions: HashMap<String, Definition>` on `Repl`; `execute_code_from(file, line_offset, code)` so a session line and a file's line share one recorder; `record_definitions` (reads `set`/`constant`/`to`/`import` out of the source, not out of a before/after binding diff); dispatch for the four commands; `type_answer`, `module_doc_answer`, `which_answer`, `sighelp_answer` as the assertable cores. `:load` now passes its real path so `.which` reports `path:line` instead of `<session>:0`. |
| `src/stdlib.rs` | +249 −0 | `BUILTIN_SIGNATURES` (shape of every registered builtin, written in Redblue terms), `MODULES`, `MODULE_MEMBERS`, `is_module`, `is_builtin`, `module_has_member`, `member_resolves`, `resolve`, `builtin_signature`, and the two drift guards `builtins_without_signatures` / `signatures_covering_nothing`. |
| `src/interpreter.rs` | +14 −0 | `Vm::published_member` — "did a program's module publish this member", which is not the same question as "is the name bound" (every builtin is bound in `globals`). This is what lets `.which` tell `files.read` from `MathUtils.circle_area`. |
| `tests/unreviewed_work_ack_test.rs` | +5 −0 | Acknowledgement entry + `MEASURED` row for the phase-056 resume commit `617e466`, which the file's own failure message prescribed. No existing assertion touched. |

### Where the facts come from

| Command | Source of truth | Hand-written copy? |
|---|---|---|
| `.doc <module>` | `stdlib::MODULE_MEMBERS` + `stdlib::builtin_signature` | none |
| `.which <name>` | `stdlib::is_builtin` / `Vm::published_member` / `Repl::definitions` | none |
| `.sighelp <name>` | `stdlib::BUILTIN_SIGNATURES`, falling back to a session `Function`'s own `params` | none |
| `.type <expr>` | the session's own `run_code`, plus `Repl::definitions` | none |

Two tests fail the build the moment a table drifts:
`every_registered_builtin_has_a_written_shape` (a builtin with no shape, or a shape
for a builtin that does not exist) and `doc_lists_exactly_what_the_registry_holds`
(a module row that is not the registry's row for it).

## Reproduction

Before the change, the command table in `src/repl/commands.rs` held seventeen
commands and none inspected code:

```
$ printf ':type [1,2]\n:which uppercase\n:quit\n' | rb
Error: Unknown command ':type'. Type ':help' for a list.
```

After:

```
>>> value: [1, 2]
kind:  list (2 item(s))
from:  evaluated at the prompt
>>> uppercase: builtin
>>> nosuchname: unknown
>>> split — builtin
  split(text, separator) -> list of text; an empty separator has no answer
>>> module json:
  json.parse      parse(text) -> the record, list or value it holds
  json.stringify  stringify(any) -> text
```

## Tests added

26 new `#[test]` functions, 12 of them named `edge_*`. All are in-tree (`cargo test`
covers them); no integration-test file was added.

| Test | Edge class covered |
|---|---|
| `repl::introspect::tests::every_registered_builtin_has_a_written_shape` | duplicate/missing — registry and shape table are one table |
| `repl::introspect::tests::doc_lists_exactly_what_the_registry_holds` | duplicate/missing — `.doc` is derived from `MODULE_MEMBERS`, not a fourth copy |
| `repl::introspect::tests::edge_doc_refuses_a_module_that_is_not_one` | **edge** — unknown module, and the empty name |
| `repl::introspect::tests::which_reports_the_session_binding_over_a_builtin` | type — lookup order: session before registry |
| `repl::introspect::tests::edge_which_says_unknown_for_a_name_nothing_binds` | **edge** — `.which` on the empty string, and near-misses (`"uppercase "`) |
| `repl::introspect::tests::sighelp_shows_the_separator_form_of_split` | boundary — the separator argument people leave out |
| `repl::introspect::tests::sighelp_answers_a_module_member_spelling` | type — `files.read` and `files_read` are one function |
| `repl::introspect::tests::edge_sighelp_refuses_a_keyword_rather_than_inventing_a_shape` | **edge** — `.sighelp say` / `.sighelp end`, a keyword, not a callable |
| `repl::introspect::tests::type_reports_value_kind_and_size` | singleton/boundary — `[1, 2]`, item count |
| `repl::introspect::tests::type_points_at_the_line_a_name_was_bound_on` | duplicate/missing — a bound name carries its `file:line` |
| `repl::introspect::tests::edge_type_describes_the_empty_values_and_counts_bytes` | **edge**/unicode — `nothing`, `[]`, `""`, `héllo` = 6 bytes, `🙂` = 4 |
| `repl::commands::tests::parse_answers_questions_about_code` | malformed input — all four commands parse, both sigils, argument preserved |
| `repl::tests::introspection::type_reports_the_value_its_kind_and_the_line_it_was_written_on` | duplicate/missing — value + kind + `<session>:2` + the quoted line |
| `repl::tests::introspection::type_answers_about_an_expression_the_session_never_bound` | boundary — a literal has no history line, so it says "evaluated at the prompt" |
| `repl::tests::introspection::edge_type_reports_the_sessions_own_errors_and_survives_them` | **edge**/type_mismatch/out_of_bounds — `1 + "a"`, unterminated `"abc`, `1 / 0`, `[1][9]`; `[-1]` and `[0]` resolve; session still works after |
| `repl::tests::introspection::edge_type_of_an_unknown_name_is_a_clean_error` | **edge** — `.type nosuchname` is an analyzer error, not a panic |
| `repl::tests::introspection::edge_type_with_no_argument_is_refused_before_anything_runs` | **edge**/malformed — bare `:type` is `MissingArgument`, binds nothing |
| `repl::tests::introspection::which_distinguishes_a_builtin_a_session_binding_and_an_unknown_name` | duplicate/missing — the three lookup outcomes from one command |
| `repl::tests::introspection::which_tells_a_stdlib_member_from_a_modules_own_member` | type — `files.read` vs `MathUtils.circle_area` after `import MathUtils` |
| `repl::tests::introspection::edge_which_reports_the_latest_set_of_a_name` | **edge** — re-`set` reports line 3, not the first `set` |
| `repl::tests::introspection::doc_lists_the_members_the_registry_registers` | duplicate/missing — every registered module, every registered member |
| `repl::tests::introspection::edge_doc_of_an_unknown_module_is_refused_by_name` | **edge** — refusal names the module asked for *and* the ones that exist |
| `repl::tests::introspection::sighelp_answers_for_every_registered_builtin` | boundary — all of `BUILTIN_NAMES`, derived |
| `repl::tests::introspection::edge_sighelp_of_an_empty_name_and_of_a_session_function` | **edge** — empty name; a session `Function`'s declared params/arity |
| `repl::tests::introspection::edge_help_names_every_command_that_inspects_code` | duplicate/missing — `:help` names all four (the "help cannot omit a command again" assertion) |
| `repl::tests::introspection::edge_the_completer_offers_the_introspection_commands` | boundary — the completer offers all four, so they can be typed |

### Test-requirement checklist

| Row | Status |
|---|---|
| empty | covered — `edge_type_describes_the_empty_values_and_counts_bytes` (`nothing`, `[]`, `""`), `edge_which_says_unknown_for_a_name_nothing_binds` (`""`), `edge_sighelp_of_an_empty_name_and_of_a_session_function`, `edge_doc_refuses_a_module_that_is_not_one` (`""`) |
| singleton | covered — `type_reports_value_kind_and_size` (`[1, 2]`, 2 items); `which_tells_a_stdlib_member_from_a_modules_own_member` (one module, one member); a one-parameter session function is read from `Function::params`; `[-1]`/`[0]` on a three-element list pin the two index ends |
| boundary | covered — `sighelp_shows_the_separator_form_of_split` (the optional-looking second argument), `edge_which_reports_the_latest_set_of_a_name`, `edge_the_completer_offers_the_introspection_commands`, `[-1]` and `[0]` |
| out_of_bounds | covered — `edge_type_reports_the_sessions_own_errors_and_survives_them` asserts `[1][9]` is a clean `Index 9 is out of bounds` error rather than a panic, **and** that `[-1]`/`[0]` resolve to the last and first item: SPEC.md §"List Operations" makes a negative index count from the end, so `-1` is not out of bounds and `.type` reports `3`, not an error. Pinning that stops a later "fix" turning it into one. |
| type_mismatch | covered — `edge_type_reports_the_sessions_own_errors_and_survives_them` (`1 + "a"`, `1 / 0`); `sighelp_answers_a_module_member_spelling` (dotted vs flat); `which_tells_a_stdlib_member_from_a_modules_own_member` |
| numeric_boundary | N/A — no arithmetic is introduced. `.sighelp` *describes* `sqrt`/`pow` overflow behaviour as text; the numeric edge cases it mentions (`0/0`, `1/0`) are exercised as `.type` inputs in `edge_type_reports_the_sessions_own_errors_and_survives_them`, which asserts the error is clean. |
| unicode | covered — `edge_type_describes_the_empty_values_and_counts_bytes` asserts `héllo` is 6 **bytes** and `🙂` is 4, which is the boundary between bytes and characters the shape line reports |
| nesting_recursion | N/A — the four commands are non-recursive and hold no stack of their own; `record_definitions` is a flat `tokens.windows(2)` scan. The nesting that does exist (a `Module.member` dotted path) is covered by `sighelp_answers_a_module_member_spelling`. |
| duplicate_missing_keys | covered — `every_registered_builtin_has_a_written_shape` (shape with no builtin *and* builtin with no shape), `doc_lists_exactly_what_the_registry_holds`, `edge_which_reports_the_latest_set_of_a_name` (a re-`set` overwrites the recorded span rather than leaving a stale one) |
| malformed_input | covered — `edge_type_with_no_argument_is_refused_before_anything_runs`, `parse_answers_questions_about_code` (both sigils), and the unterminated string in `edge_type_reports_the_sessions_own_errors_and_survives_them` |
| resource_limit | N/A — nothing here loops unboundedly. `record_definitions` is O(tokens) per line and stores one `Definition` per bound name; `.type` evaluates exactly one expression and adds no recursion of its own. |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — zero warnings |
| `cargo test --all-targets` | pass — **1684 passed, 0 failed, 0 ignored** |
| `./rbops/verify.sh phase-056` | **not run — `rbops/` does not exist in this checkout** |

On the fourth gate, plainly: this working tree has no `rbops/` directory
(`ls rbops` → `No such file or directory`), so `./rbops/verify.sh phase-056` cannot
be executed here and its verdict is **not claimed**. The pipeline that invokes this
phase carries the script. The three gates that can be run were run, and the two
test-suite assertions `verify.sh` is known to enforce are satisfied by construction:
26 new `#[test]` functions (floor is 3), 12 named `edge_*` (floor is 1), a refusal or
error result is asserted by 7 of them (floor is 1), and no `#[ignore]`, `// skip` or
`allow(clippy::` was introduced.

## Invariants touched

None. No grammar change, no `Value` variant added, no `Error` variant added, no
change to `say`, `set x to …`, or `… end`. `.rb` is still the source extension.

The one behavioural change outside the four commands is that `:load` now passes its
real path to `execute_code_from` instead of the literal `<session>`, so a name a
loaded file binds is reported as `path:line`. That is additive: `.which` on such a
name previously had no answer to give at all (it was not in `definitions`), and
`<session>` for a file that exists on disk would have been a lie.

## Known gaps / follow-ups

- **`.type` only reports a span for a bare identifier.** `definition_of_expression`
  returns `None` for anything that is not `is_alphanumeric() || '_'`, so
  `.type nums[0]` says "evaluated at the prompt" even though `nums` has a recorded
  line. Attributing a sub-expression to the line that bound its root name needs a
  resolver walk, which is a bigger change than this phase's four commands.
- **`.doc` describes flat builtins only through their signatures.** A module in
  `MODULES` with an empty `MODULE_MEMBERS` row prints
  `(no members: every function it has is a flat builtin)`, which is true but not
  exhaustive — it does not enumerate the `module_member` names the module answers.
  No module currently hits that branch.
- **`.sighelp` cannot describe a member of a module this session built.** It reads
  shapes from `BUILTIN_SIGNATURES` and from a session `Value::Function`'s own
  `params`; a `Module.member` published by a program's `module` block has no shape
  table to read. `.which` is what points at it.
- **`.which` on a name bound by the VM but never recorded says "unknown".** This is
  deliberate (documented on `origin_of`) — inventing a line 0 would point at
  nothing — but it does mean `:restore` from a saved session reports `unknown` for
  every restored name, because `definitions` is not serialised by `:save`.
- **`rbops/verify.sh` was not run** (see Gates). Nothing in this report asserts its
  verdict.

---

# Review round 1 — the findings, and what was done about them

Four findings, all reproduced against this tree before anything was changed. One
was a BLOCKER, three were MAJOR; all four are fixed, and each fix is pinned by a
test that fails without it.

| # | Finding | Fix |
|---|---|---|
| 1 | **BLOCKER** — `.type` ran arbitrary Redblue through `run_code`, so `files.delete`/`files.write`/`network.get`/`time.sleep` executed, `say` printed, and `set` bound a session name no line pointed at | `introspect::expression_of` (one expression, or a refusal naming the keyword) + `stdlib::EFFECTFUL_BUILTINS` read off the AST by `introspect::calls_in` + `Vm::fork`/`Vm::run_quietly` |
| 2 | **MAJOR** — `:reset` cleared the VM, `variables` and the buffer but not `definitions`, so `.which` reported `<session>:N` for bindings the reset had thrown away | `reset_session` clears `definitions` with the bindings |
| 3 | **MAJOR** — `declares_a_name` did not match `module`, although the recorder documented `module M`, so `.which Foo` said "unknown" for a module the session had just declared | `TokenKind::Module` is a declaring keyword, and a declared module is recognised by `Vm::declares_module` — it binds no value of its own, so "was the name bound?" is the wrong question for it |
| 4 | **MAJOR** — `record_definitions` read only the `[Import, Identifier]` pair, so `import files, network as net` recorded `files` alone and `.which network`/`net` said "unknown" | `imported_names` reads the rest of the line: every module name and every alias, both of which the VM binds |

A fifth defect fell out of rewriting `record_definitions` for #4 and is fixed
with it: `set x to y` was read as two declarations, because the `to` in it is a
word and the `to` in `to f(…)` is a word too. `y`'s recorded line moved to a line
on which it was only *read* — the exact failure reading the source rather than
comparing bindings was meant to avoid.

## Why finding 1 is three fixes and not one

Any one of the three closes a hole the other two leave open:

- **The statement gate** stops `set`, `say`, `print`, `import`, `module`, `to`,
  loops and tests. It is also what stops a name being bound by a *command*:
  `definitions` is read out of source, and a name bound by `:type` was written on
  no line `:history` holds, so `.which` would have called it unknown while the
  next typed line reached it.
- **The effect gate** stops what leaving the process cannot undo. `EFFECTFUL_BUILTINS`
  is a table beside `BUILTIN_NAMES` with a drift guard
  (`effectful_names_that_are_not_builtins`, asserted empty in
  `edge_every_effectful_name_is_a_registered_builtin`), and a call is read off the
  AST rather than off the run — so `xs.map(to(x) files.write(…) end)` is refused
  too. Aliases go through `Vm::module_behind` first, so `import files as F` then
  `F.delete(…)` is the same refusal rather than a way around it.
- **The fork** stops what the session would feel. `Vm::fork` copies the bindings a
  program can *read* — globals, constants, live scopes, modules, aliases,
  declared modules, published members, objects — and none of the history: no
  output buffer, no spent step budget, no recorded failures, no debugger hook.
  `type_calls_a_session_function_without_running_it_against_the_session` was
  checked by putting the session's own VM back in place: the assertion fails with
  `left: Some(Number(4.0)), right: Some(Number(3.0))`, which is the mutation the
  finding describes.

A `repeat` reached through a called function is still bounded by the VM's own
iteration and step limits, and answers with `RuntimeError: Maximum of 1000000
iterations reached` rather than hanging; its `say` output is buffered on the fork
and never reaches the terminal.

## What finding 1 does *not* claim

A call to a `to` function the session wrote is evaluated, and nothing can be known
about its body before it runs. `stdlib::EFFECTFUL_BUILTINS` is a property of the
registry, not of a program's source, so a session function that calls
`files.delete` is not on the table. The fork is what keeps the *session* safe in
that case; the filesystem is not. This is a limitation, not a fix, and it is
recorded here rather than left for the next reader to find out from the code.

## Gates, re-run after the fixes

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — zero warnings |
| `cargo test --all-targets` | pass — **1700 passed, 0 failed, 0 ignored** (1684 before, +16) |
| `./rbops/verify.sh phase-056` | **still not runnable — `rbops/` does not exist in this checkout** (`find . -name verify.sh` finds nothing outside `target/`). Its verdict is not claimed, exactly as above. |

No `#[ignore]`, no `// skip`, no `allow(clippy::`, and no test was deleted or
weakened: the sixteen new tests are additions, and the twelve `edge_*` tests the
first round added still pass unchanged.

### The sixteen

| Test | Class |
|---|---|
| `introspect::tests::edge_every_effectful_name_is_a_registered_builtin` | duplicate/missing — the refusal table and the registry are one story |
| `introspect::tests::edge_a_question_refuses_a_statement_and_names_the_keyword` | malformed — ten statements, one expression, and the two boundaries |
| `introspect::tests::edge_calls_are_found_wherever_an_expression_hides_one` | nesting — arguments, receivers, and a function literal's body |
| `introspect::tests::edge_a_method_call_on_a_value_is_not_named_after_a_module` | type — a call on a value must not resolve to a library refusal |
| `repl::tests::introspection::edge_type_refuses_a_program_and_the_session_never_knows_it_happened` | **edge** — six programs refused; nothing bound, `_` unmoved, session alive |
| `repl::tests::introspection::edge_type_refuses_a_call_that_changes_the_world_and_the_file_survives` | **edge**/resource — nine refusals, and the file still reads `original` |
| `repl::tests::introspection::edge_type_refuses_a_call_written_through_an_import_alias` | **edge** — `F.delete` is `files_delete`; `J.parse` still answers |
| `repl::tests::introspection::type_calls_a_session_function_without_running_it_against_the_session` | duplicate/missing — the fork, with the counter as the witness |
| `repl::tests::introspection::edge_a_reset_forgets_the_lines_of_the_bindings_it_forgets` | **edge** — finding 2 |
| `repl::tests::introspection::edge_which_names_every_name_one_import_binds` | duplicate/missing — finding 4, both alias spellings |
| `repl::tests::introspection::edge_which_names_a_module_the_session_declared_itself` | duplicate/missing — finding 3 |
| `repl::tests::introspection::edge_a_value_read_by_a_set_stays_on_the_line_that_bound_it` | **edge** — the fifth defect |
| `repl_test::edge_type_of_a_statement_prints_nothing_and_binds_nothing` | **edge**/stdout — the only place "printed nothing" is observable |
| `repl_test::edge_type_cannot_delete_a_file` | **edge**/io — the file is the witness |
| `repl_test::edge_type_of_a_call_does_not_change_the_session_it_runs_against` | **edge** — `:inspect` reads the live value |
| `repl_test::edge_which_names_every_module_one_import_binds_and_a_reset_forgets_their_lines` | duplicate/missing — findings 2 and 4 through the real binary |