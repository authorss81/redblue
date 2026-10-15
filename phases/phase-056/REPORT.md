# Phase 056 — Teach the REPL to answer questions about code: `.type`, `.doc`, `.which`, `.sighelp`

## What changed

The four commands land in a new pure module (`src/repl/introspect.rs`) plus three
tables/facts that had to exist somewhere anyway. Nothing in the language surface
moved. Line counts below are `git diff --numstat 14824b9..HEAD` (the base is the
phase-055 checkpoint), plus the uncommitted `tests/` fix described in
[Resume round](#resume-round--what-this-run-changed).

| File | Lines | What |
|---|---|---|
| `src/repl/introspect.rs` | +1347 −0 | **New file.** The pure half: `Origin`, `Definition`, `classify`, `which_lines`, `doc_lines`, `sighelp_lines`, `type_lines`, `expression_of`, `calls_in`, `imported_names`, `declares_a_name`. No VM, no terminal, so every answer is assertable. |
| `src/repl/commands.rs` | +109 −0 | Four `COMMAND_TABLE` rows building `ReplCommand::{Type, Doc, Which, Sighelp}` (commands.rs:293, :301, :309, :317) — so `:help` and the completer pick them up from the one table rather than a second list — and `ReplCommand::argument_text` (commands.rs:537). |
| `src/repl/mod.rs` | +1506 −8 | `definitions: HashMap<String, Definition>` on `Repl` (mod.rs:425); `execute_code_from(file, line_offset, code)` (mod.rs:994) so a session line and a file's line share one recorder; `record_definitions` (mod.rs:1044) which reads `set`/`constant`/`to`/`import`/`module` out of the source, not out of a before/after binding diff; dispatch plus the four assertable cores `type_answer` (mod.rs:1589), `module_doc_answer` (:1755), `which_answer` (:1772), `sighelp_answer` (:1821). `:load` now passes its real path (:1320) so `.which` reports `path:line` instead of `<session>:0`. |
| `src/stdlib.rs` | +343 −0 | `MODULES` (:19), `MODULE_MEMBERS` (:62), `is_module` (:255), `is_builtin` (:788), `module_has_member` (:138), `member_resolves` (:230), `resolve` (:841), `builtin_signature` (:1137), `BUILTIN_SIGNATURES` (:936), `EFFECTFUL_BUILTINS` (:1216) / `is_effectful` (:1246), and four drift guards: `modules_without_functions`, `members_without_functions`, `signatures_covering_nothing`, `builtins_without_signatures`, `effectful_names_that_are_not_builtins`. |
| `src/interpreter.rs` | +215 −8 | `Vm::fork` (:675) — a VM with every binding a program can read and none of the history (no output buffer, no step budget, no recorded failures, no debugger hook); `Vm::run_quietly` (:1354); `Vm::module_behind` (:745); `Vm::published_member` (:1058) — "did a program's module publish this member", which is not the same question as "is the name bound" (every builtin is bound in `globals`); `Vm::declares_module` (:1188). |
| `tests/repl_test.rs` | +201 −0 | Seven `edge_*` tests that drive the **real `rb` binary**, so "printed nothing", "the file still reads `original`" and "the session did not move" are observed rather than asserted from inside the process. |
| `tests/unreviewed_work_ack_test.rs` | +6 −2 | Acknowledgement + `MEASURED` row for this phase's resume commit. Data edited, no existing assertion touched — see [Resume round](#resume-round--what-this-run-changed). |
| `phases/phase-056/REPORT.md` | this file | — |

### Where the facts come from

| Command | Source of truth | Hand-written copy? |
|---|---|---|
| `.doc <module>` | `stdlib::MODULE_MEMBERS` + `stdlib::builtin_signature` | none |
| `.which <name>` | `stdlib::is_builtin` / `Vm::published_member` / `Repl::definitions` | none |
| `.sighelp <name>` | `stdlib::BUILTIN_SIGNATURES`, falling back to a session `Function`'s own `params` | none |
| `.type <expr>` | the session's own parser + `Vm::run_quietly` on a `Vm::fork`, plus `Repl::definitions` | none |

Tests fail the build the moment any of those tables drifts:
`every_registered_builtin_has_a_written_shape` (a builtin with no shape, or a
shape for a builtin that does not exist), `doc_lists_exactly_what_the_registry_holds`
(a module row that is not the registry's row for it) and
`edge_every_effectful_name_is_a_registered_builtin` (a refusal table entry that
names no builtin).

## Reproduction

Before the change, the command table in `src/repl/commands.rs` held seventeen
commands and none inspected code:

```
$ printf ':type [1,2]\n:quit\n' | rb
Error: Unknown command ':type'. Type ':help' for a list.
```

After, from the binary in this tree:

```
$ printf ':type [1,2]\n:which uppercase\n:sighelp split\n:quit\n' | rb
>>> value: [1, 2]
kind:  list (2 item(s))
from:  evaluated at the prompt
>>> uppercase: builtin
>>> split — builtin
  split(text, separator) -> list of text; an empty separator has no answer
```

and the refusal paths are clean, not panics:

```
$ printf ':type nosuchname\n:doc nosuchmodule\n:sighelp say\n:type\n:quit\n' | rb
>>> Error: AnalyzerError: Unknown variable 'nosuchname'
  --> 1:1
>>> Error: 'nosuchmodule' is not a module. The modules are: bytes, console, csv, files, json, list, math, network, sys, text, time
>>> Error: 'say' is a keyword, not something to call: it is written say.
>>> Usage: :type <expression>
```

## Tests added

**51 new `#[test]` functions, 36 of them named `edge_*`.** All are in-tree
(`cargo test` covers them); no new test *file* was added. Counts are derived by
diffing the set of test-function names at `14824b9` against `HEAD` — see
[Resume round](#resume-round--what-this-run-changed) for how they were measured.
No test was deleted, renamed, weakened, `#[ignore]`d or `.skip`ped.

| Test | File | Edge class covered |
|---|---|---|
| `parse_answers_questions_about_code` | commands.rs | malformed input — all four parse, both sigils, argument preserved |
| `every_registered_builtin_has_a_written_shape` | introspect.rs | duplicate/missing — registry and shape table are one table |
| `doc_lists_exactly_what_the_registry_holds` | introspect.rs | duplicate/missing — `.doc` derived from `MODULE_MEMBERS`, not a fourth copy |
| `edge_every_effectful_name_is_a_registered_builtin` | introspect.rs | duplicate/missing — the refusal table and the registry are one story |
| `edge_a_read_a_draw_and_a_clock_reading_are_refused_as_effects_too` | introspect.rs | resource — `random`, `time.now`, `time.sleep` are effects too |
| `edge_a_question_refuses_a_statement_and_names_the_keyword` | introspect.rs | malformed — ten statements, one expression, two boundaries |
| `edge_calls_are_found_wherever_an_expression_hides_one` | introspect.rs | nesting — arguments, receivers, a function literal's body |
| `edge_a_method_call_on_a_value_is_not_named_after_a_module` | introspect.rs | type — a call on a value must not resolve to a library refusal |
| `edge_a_body_is_walked_in_source_order_and_stops_at_its_end` | introspect.rs | nesting — the AST walk stops at the body's `end` |
| `edge_doc_refuses_a_module_that_is_not_one` | introspect.rs | **edge** — unknown module, and the empty name |
| `edge_which_says_unknown_for_a_name_nothing_binds` | introspect.rs | **edge** — `.which` on `""`, and the near-miss `"uppercase "` |
| `edge_which_prints_a_member_the_way_it_is_called_and_never_the_registrys_spelling` | introspect.rs | **edge** — `files.read` is printed as `files.read`, not `files_read` |
| `which_reports_the_session_binding_over_a_builtin` | introspect.rs | type — lookup order: session before registry |
| `sighelp_shows_the_separator_form_of_split` | introspect.rs | boundary — the separator argument people leave out |
| `sighelp_answers_a_module_member_spelling` | introspect.rs | type — `files.read` and `files_read` are one function |
| `edge_sighelp_refuses_a_keyword_rather_than_inventing_a_shape` | introspect.rs | **edge** — `.sighelp say` / `.sighelp end` |
| `type_reports_value_kind_and_size` | introspect.rs | singleton/boundary — `[1, 2]`, item count |
| `type_points_at_the_line_a_name_was_bound_on` | introspect.rs | duplicate/missing — a bound name carries its `file:line` |
| `edge_type_describes_the_empty_values_and_counts_bytes` | introspect.rs | **edge**/unicode — `nothing`, `[]`, `""`, `héllo` = 6 bytes, `🙂` = 4 |
| `type_reports_the_value_its_kind_and_the_line_it_was_written_on` | repl/mod.rs | duplicate/missing — value + kind + `<session>:2` + the quoted line |
| `type_answers_about_an_expression_the_session_never_bound` | repl/mod.rs | boundary — a literal has no history line, so "evaluated at the prompt" |
| `type_calls_a_session_function_without_running_it_against_the_session` | repl/mod.rs | duplicate/missing — the `fork`, with the counter as the witness |
| `which_distinguishes_a_builtin_a_session_binding_and_an_unknown_name` | repl/mod.rs | duplicate/missing — the three lookup outcomes from one command |
| `which_tells_a_stdlib_member_from_a_modules_own_member` | repl/mod.rs | type — `files.read` vs `MathUtils.circle_area` after `import MathUtils` |
| `doc_lists_the_members_the_registry_registers` | repl/mod.rs | duplicate/missing — every registered module, every registered member |
| `sighelp_answers_for_every_registered_builtin` | repl/mod.rs | boundary — all of `BUILTIN_NAMES`, derived |
| `edge_type_reports_the_sessions_own_errors_and_survives_them` | repl/mod.rs | **edge**/type_mismatch/out_of_bounds — `1 + "a"`, unterminated `"abc`, `1 / 0`, `[1][9]`; `[-1]`/`[0]` resolve; session still works after |
| `edge_type_of_an_unknown_name_is_a_clean_error` | repl/mod.rs | **edge** — `.type nosuchname` is an analyzer error, not a panic |
| `edge_type_with_no_argument_is_refused_before_anything_runs` | repl/mod.rs | **edge**/malformed — bare `:type` is a usage error and binds nothing |
| `edge_type_refuses_a_program_and_the_session_never_knows_it_happened` | repl/mod.rs | **edge** — six programs refused; nothing bound, `_` unmoved, session alive |
| `edge_type_refuses_a_call_that_changes_the_world_and_the_file_survives` | repl/mod.rs | **edge**/resource — nine refusals, and the file still reads `original` |
| `edge_type_refuses_a_call_written_through_an_import_alias` | repl/mod.rs | **edge** — `F.delete` is `files_delete`; `J.parse` still answers |
| `edge_type_refuses_a_question_whose_call_hides_behind_a_declared_function` | repl/mod.rs | **edge** — a `to` function the session wrote |
| `edge_the_refusal_is_the_questions_and_not_the_language` | repl/mod.rs | type — `.type` refusing is not the same as the REPL rejecting the code |
| `edge_a_deeply_nested_question_is_refused_rather_than_overflowing` | repl/mod.rs | nesting/resource — deep nesting is a refusal, not a stack overflow |
| `edge_a_reset_forgets_the_lines_of_the_bindings_it_forgets` | repl/mod.rs | **edge** — `:reset` clears `definitions` with the bindings |
| `edge_which_reports_the_latest_set_of_a_name` | repl/mod.rs | **edge** — a re-`set` reports line 3, not the first `set` |
| `edge_which_names_every_name_one_import_binds` | repl/mod.rs | duplicate/missing — `import files, network as net` binds four names |
| `edge_which_names_a_module_the_session_declared_itself` | repl/mod.rs | duplicate/missing — `module Foo` binds no value but is not "unknown" |
| `edge_a_value_read_by_a_set_stays_on_the_line_that_bound_it` | repl/mod.rs | **edge** — `set x to y` is one declaration, not two |
| `edge_doc_of_an_unknown_module_is_refused_by_name` | repl/mod.rs | **edge** — the refusal names the module asked for *and* the ones that exist |
| `edge_sighelp_of_an_empty_name_and_of_a_session_function` | repl/mod.rs | **edge** — empty name; a session `Function`'s declared params/arity |
| `edge_help_names_every_command_that_inspects_code` | repl/mod.rs | duplicate/missing — `:help` names all four (the "help cannot omit a command again" assertion) |
| `edge_the_completer_offers_the_introspection_commands` | repl/mod.rs | boundary — the completer offers all four, so they can be typed |
| `edge_type_of_a_statement_prints_nothing_and_binds_nothing` | repl_test.rs | **edge**/stdout — the only place "printed nothing" is observable |
| `edge_type_cannot_delete_a_file` | repl_test.rs | **edge**/io — the file on disk is the witness |
| `edge_type_cannot_delete_a_file_through_a_function_the_session_declared` | repl_test.rs | **edge**/io — the indirection does not launder the refusal; the file is the witness |
| `edge_type_cannot_move_the_random_stream_or_read_the_clock` | repl_test.rs | **edge**/determinism — two `.type` calls give the same value |
| `edge_type_cannot_print_through_a_function_the_session_called` | repl_test.rs | **edge**/stdout — no output reaches the terminal |
| `edge_type_of_a_call_does_not_change_the_session_it_runs_against` | repl_test.rs | **edge** — `:inspect` still reads the live value |
| `edge_which_names_every_module_one_import_binds_and_a_reset_forgets_their_lines` | repl_test.rs | duplicate/missing — findings 2 and 4 through the real binary |

### Test-requirement checklist

| Row | Status |
|---|---|
| empty | covered — `edge_type_describes_the_empty_values_and_counts_bytes` (`nothing`, `[]`, `""`), `edge_which_says_unknown_for_a_name_nothing_binds` (`""`), `edge_sighelp_of_an_empty_name_and_of_a_session_function`, `edge_doc_refuses_a_module_that_is_not_one` (`""`) |
| singleton | covered — `type_reports_value_kind_and_size` (`[1, 2]`, 2 items); `which_tells_a_stdlib_member_from_a_modules_own_member` (one module, one member); a one-parameter session function is read from `Function::params`; `[-1]`/`[0]` on a three-element list pin the two index ends |
| boundary | covered — `sighelp_shows_the_separator_form_of_split` (the optional-looking second argument), `edge_which_reports_the_latest_set_of_a_name`, `edge_the_completer_offers_the_introspection_commands`, `[-1]` and `[0]` |
| out_of_bounds | covered — `edge_type_reports_the_sessions_own_errors_and_survives_them` asserts `[1][9]` is a clean `Index 9 is out of bounds` error rather than a panic, **and** that `[-1]`/`[0]` resolve to the last and first item: SPEC.md §"List Operations" makes a negative index count from the end, so `-1` is not out of bounds and `.type` reports `3`, not an error. Pinning that stops a later "fix" turning it into one. |
| type_mismatch | covered — `edge_type_reports_the_sessions_own_errors_and_survives_them` (`1 + "a"`, `1 / 0`); `sighelp_answers_a_module_member_spelling` (dotted vs flat); `edge_a_method_call_on_a_value_is_not_named_after_a_module` |
| numeric_boundary | N/A — no arithmetic is introduced. `.sighelp` *describes* `sqrt`/`pow` overflow behaviour as text; the numeric edge cases it mentions (`0/0`, `1/0`) are exercised as `.type` inputs in `edge_type_reports_the_sessions_own_errors_and_survives_them`, which asserts the error is clean. |
| unicode | covered — `edge_type_describes_the_empty_values_and_counts_bytes` asserts `héllo` is 6 **bytes** and `🙂` is 4, which is the boundary between bytes and characters the shape line reports |
| nesting_recursion | covered — `edge_calls_are_found_wherever_an_expression_hides_one` (arguments, receivers, a function literal's body), `edge_a_body_is_walked_in_source_order_and_stops_at_its_end`, `edge_a_deeply_nested_question_is_refused_rather_than_overflowing`, `sighelp_answers_a_module_member_spelling` (`Module.member` dotted path) |
| duplicate_missing_keys | covered — `every_registered_builtin_has_a_written_shape` (shape with no builtin *and* builtin with no shape), `doc_lists_exactly_what_the_registry_holds`, `edge_which_reports_the_latest_set_of_a_name` (a re-`set` overwrites the recorded span rather than leaving a stale one), `edge_which_names_every_name_one_import_binds` |
| malformed_input | covered — `edge_type_with_no_argument_is_refused_before_anything_runs`, `edge_doc_of_an_unknown_module_is_refused_by_name`, `edge_sighelp_of_an_empty_name_and_of_a_session_function`, `parse_answers_questions_about_code` (both sigils), and the unterminated string in `edge_type_reports_the_sessions_own_errors_and_survives_them` |
| resource_limit | covered — `edge_a_deeply_nested_question_is_refused_rather_than_overflowing` (no unbounded recursion of the command's own), `edge_type_refuses_a_call_that_changes_the_world_and_the_file_survives` (a `repeat` reached through a call still answers with `RuntimeError: Maximum of 1000000 iterations reached` rather than hanging, and its `say` output is buffered on the fork and never reaches the terminal) |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — zero warnings |
| `cargo test --all-targets` | pass — **1709 passed, 0 failed, 0 ignored** across 62 test binaries (stable over three consecutive runs) |
| `./rbops/verify.sh phase-056` | **not run — `rbops/` does not exist in this checkout** |

On the fourth gate, plainly: this working tree has no `rbops/` directory
(`ls rbops` → `No such file or directory`; `find . -name verify.sh` finds nothing
outside `target/`), so `./rbops/verify.sh phase-056` cannot be executed here and
its verdict is **not claimed**. The pipeline that invokes this phase carries the
script. The three gates that can be run were run, and the test-suite assertions
`verify.sh` is known to enforce are satisfied by measurement, not by assertion:
51 new `#[test]` functions (floor is 3), 36 named `edge_*` (floor is 1), a
refusal or error result asserted by 33 of them (floor is 1 — counted as the new
tests whose body matches `is_err()` / `expect_err` / a negated `is_ok` / an
asserted `Error:` line), and no `#[ignore]`,
`// skip` or `allow(clippy::` was introduced — `git diff 14824b9..HEAD -- src/ tests/`
returns no match for any of the three.

## Invariants touched

None. No grammar change, no `Value` variant added, no `Error` variant added, no
change to `say`, `set x to …`, or `… end`. `.rb` is still the source extension.
All 9 of the `examples/*.rb` + `modules/*.rb` programs (7 + 2) still run —
`for f in examples/*.rb modules/*.rb; do ./target/debug/rb run "$f"; done` exits 0
for every one of them.

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
  exhaustive. No module currently hits that branch, and
  `doc_lists_exactly_what_the_registry_holds` would notice if one did.
- **`.sighelp` cannot describe a member of a module this session built.** It reads
  shapes from `BUILTIN_SIGNATURES` and from a session `Value::Function`'s own
  `params`; a `Module.member` published by a program's `module` block has no shape
  table to read. `.which` is what points at it.
- **`.which` on a name bound by the VM but never recorded says "unknown".** This is
  deliberate (documented on `origin_of`) — inventing a line 0 would point at
  nothing — but it does mean `:restore` from a saved session reports `unknown` for
  every restored name, because `definitions` is not serialised by `:save`.
- **The refusal is a guard the interpreter honours, not a sandbox.** It has two
  layers: a static walk of the question (transitive through the session's own `to`
  declarations, the members of modules and objects the session declared, and
  terminating on mutual recursion — `edge_type_refuses_a_question_whose_call_hides_behind_a_declared_function`)
  and a flag on the forked VM that refuses the effect *where it fires*, which is
  what stops the one shape no name lookup can see: a function value handed to `map`.
  Neither layer is an OS boundary. `EFFECTFUL_BUILTINS` (stdlib.rs:1216) is a
  property of the *registry*, not of a program's source, so a session function that
  reaches the filesystem through something the table does not name is bounded by the
  table's completeness rather than by anything this phase added. `edge_every_effectful_name_is_a_registered_builtin`
  pins the half that is checkable — every name in the table is a real builtin — and
  the other half, "every builtin with an effect is in the table", stays a property of
  the table a human writes. It is stated here rather than left for the next reader to
  infer from the code.
- **`rbops/verify.sh` was not run** (see Gates). Nothing in this report asserts its
  verdict.

---

# Resume round — what this run changed

Two things, both about the report rather than the feature.

## 1. `unreviewed_work_in_head_history_is_acknowledged` was red

Running the gates first — as the resume instruction requires — produced exactly one
failure, and it was not in the new code:

```
---- unreviewed_work_in_head_history_is_acknowledged stdout ----
unreviewed-work commits selected: 25 | acknowledged: 25 | unacknowledged: 1
 | stale entries: 1 | UNACKNOWLEDGED: 1a7df335b8b3f656f599d5ff22856918b63858b7
   resume phase-056: adopt preserved work | STALE: 617e466704fc55823329fd58449db7fb7aec99f0
```

The pipeline re-dispatched the phase as a *new* resume commit `1a7df33`, which
replaced the previous attempt's `617e4667`; `617e4667` is no longer an ancestor of
`HEAD`. `scan` (tests/unreviewed_work_ack_test.rs:394) treats an unacknowledged
selected commit and an acknowledgement naming a commit the history does not contain
as **both** failures, so the fix is the same one entry: `ACKNOWLEDGED` and
`MEASURED` now name `1a7df33`, and the old SHA is gone rather than left to rot.

Nothing else in that file changed — two lines in `ACKNOWLEDGED`, two in `MEASURED`.
No assertion, no fixture and no helper was touched, and the file's own failure
message prescribed exactly this edit.

## 2. This report's numbers, re-derived

The previous text of this report claimed `+568` lines for `introspect.rs`, `+14`
for `interpreter.rs`, "26 new `#[test]` functions" and "1700 passed". Those were
true when written and are wrong now — later review rounds in the same phase added
code the table did not mention. Every figure above is re-derived:

- line counts: `git diff --numstat 14824b9..HEAD` (base = the phase-055 checkpoint
  `14824b9`);
- test counts: the set of test-function names at `14824b9` subtracted from the set at
  `HEAD`, per file — 18 new in `introspect.rs`, 1 in `commands.rs`, 25 in
  `repl/mod.rs`, 7 in `repl_test.rs`, 0 elsewhere; 51 total, 36 of them `edge_*`;
- suite size: `cargo test --all-targets` three times, 1709 every time.

No test was removed between the two revisions (the same name-set subtraction in the
other direction is empty), so the count went up and not sideways.

## Gates, re-run at the end of this run

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — zero warnings |
| `cargo test --all-targets` | pass — **1709 passed, 0 failed, 0 ignored** (62 binaries) |
| `./rbops/verify.sh phase-056` | **not runnable — `rbops/` does not exist in this checkout.** Its verdict is not claimed, exactly as above. |

## Review round 1 — the findings, and what was done about them

Four findings, all reproduced against this tree before anything was changed. One
was a BLOCKER, three were MAJOR; all four are fixed, and each fix is pinned by a
test that fails without it.

| # | Finding | Fix |
|---|---|---|
| 1 | **BLOCKER** — `.type` ran arbitrary Redblue through `run_code`, so `files.delete`/`files.write`/`network.get`/`time.sleep` executed, `say` printed, and `set` bound a session name no line pointed at | `introspect::expression_of` (one expression, or a refusal naming the keyword) + `stdlib::EFFECTFUL_BUILTINS` read off the AST by `introspect::calls_in` + `Vm::fork`/`Vm::run_quietly` |
| 2 | **MAJOR** — `:reset` cleared the VM, `variables` and the buffer but not `definitions`, so `.which` reported `<session>:N` for bindings the reset had thrown away | `reset_session` clears `definitions` with the bindings (mod.rs:901) |
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