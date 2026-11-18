# Phase 118 — Introspect the session: `:builtins`, `:modules` and `:prompt`

## What changed
| File | Lines | What |
|---|---|---|
| src/repl/commands.rs | +53 −20 | Added `:builtins`/`.builtins` and `:modules`/`.modules` to the command table and enum; a bare `:prompt` now asks for the current prompt, while `:prompt none` restores the default; `:help` names the no-argument form. |
| src/repl/completer.rs | +46 −33 | The completer's builtin and module vocabularies now come from the shared `stdlib` functions instead of a hand-kept list; `builtin_names()`/`module_names()` expose those sets to the agreement tests. |
| src/repl/introspect.rs | +191 −2 | Added `builtin_lines()` and `module_lines()`, both sorted from the same registry the completer uses, with module annotations for builtins that have a dotted spelling, including the `list_map`/`map` pair. |
| src/repl/mod.rs | +266 −6 | Dispatches `:builtins` and `:modules`; `:prompt` answers with the prompt the next line is read at, and `:prompt none` restores `>>> `. Added the command, registry-agreement, prompt, block and sub-REPL edge tests. |
| src/stdlib.rs | +65 −0 | Added `builtin_vocabulary()`, `module_vocabulary()` and `module_of_builtin()`, the shared registry views used by completion and introspection; `module_of_builtin` also compares the member spelling so `map` is annotated `(list)` like `filter` and `reduce`. |
| SURVEY.md | +1 −1 | Re-anchored the `regex_builtin` citation after the new `src/stdlib.rs` lines moved it. |
| tests/unreviewed_work_ack_test.rs | +39 −0 | Acknowledged the already-landed phase-117 resume commit by its SHA and added the test that pins it to the SHA rather than the in-flight subject rule, matching the phase-117 acknowledgement of phase-114. |

## Reproduction
Before this phase the REPL rejected the two introspection commands and a bare `:prompt` printed nothing:

```
$ printf ':builtins\n:modules\n:prompt\n:quit\n' | target/debug/rb
>>> Unknown command 'builtins'. Type :help for available commands.
>>> Unknown command 'modules'. Type :help for available commands.
>>> >>> Goodbye!
```

After the change:

```
$ printf ':builtins\n:modules\n:prompt\n:quit\n' | target/debug/rb
>>> builtins (…):
  abs (math)
  …
  zip
>>> modules (24):
  base64
  …
  zip
>>> >>> 
>>> Goodbye!
```

## Tests added
| Test | Edge class covered |
|---|---|
| parse_reads_builtins_and_modules_as_commands | both sigils parse to the new command variants |
| builtins_lists_every_registered_name_and_nothing_else | singleton/complete registry: every `BUILTIN_NAMES` entry plus the global constants, and no unregistered name |
| edge_builtins_and_modules_are_deterministic_across_calls | determinism: two consecutive calls produce identical bytes |
| edge_builtins_and_completion_share_one_registry | duplicate/missing keys: the completer and `:builtins` name the same list in the same order |
| modules_lists_every_module_and_completion_agrees | complete registry: every `MODULES` entry is listed, resolves as a module, and is offered by TAB |
| edge_a_builtin_without_a_module_has_no_empty_annotation | empty field: a builtin with no module prints no empty `()` annotation |
| builtins_annotates_a_known_builtin_with_its_module | positive module annotation: `abs (math)`, `files_read (files)` and `map (list)` are present exactly |
| edge_an_empty_prompt_still_reads_and_evaluates_a_program | empty: an empty prompt still reads and evaluates the next line |
| edge_a_prompt_of_only_spaces_is_still_a_prompt | boundary: whitespace-only prompt is display text, not "no argument" |
| edge_a_prompt_wider_than_the_terminal_is_kept_whole | boundary/resource: a 300-byte prompt is printed untruncated and the next line still runs |
| edge_repl_significant_text_in_a_prompt_does_not_break_continuation | malformed/ambiguous display text: `>>>` in the prompt does not take over the numbered continuation prompt |
| edge_a_prompt_with_an_unknown_placeholder_is_refused | failure assertion: an unknown placeholder is rejected and the old prompt is kept |
| edge_prompt_none_restores_the_default_format_and_the_rendered_prompt | restore: `:prompt none` resets both the stored format and the next rendered prompt |
| edge_a_prompt_of_none_with_a_trailing_space_is_kept_verbatim | sentinel boundary: `none ` is literal display text, not the restore word |
| edge_a_bare_prompt_prints_the_current_prompt_without_changing_it | dispatch output: bare `:prompt` prints the rendered prompt and leaves it in place |
| edge_a_prompt_of_none_with_a_trailing_space_is_literal | dispatch output: a piped `none ` prompt is printed literally |
| edge_a_bare_prompt_answers_at_the_prompt_the_next_line_is_read_at | state: a block answers `..  N ` and a sub-REPL answers `debug> ` |
| edge_the_phase_117_delivery_is_acknowledged_by_sha_not_the_rule | history audit: the prior delivery is SHA-pinned, not added to the in-flight rule |

## Review follow-ups (round 2 — review findings)
- `module_of_builtin` now compares the member spelling as well as the resolved
  registry name, so `list.map` annotates the flat `map` as `(list)` just as it
  already annotated `filter` and `reduce`. `builtins_annotates_a_known_builtin_with_its_module`
  pins `map (list)` alongside `abs (math)` and `files_read (files)`.
- `prompt_answer` now delegates to `prompt_for_next_line`, so a bare `:prompt`
  answers with the numbered continuation prompt while a block is open and the
  sub-REPL's prompt while one is, instead of always reporting the primary
  format. A new edge test pins both states.
- The `:help` row for `:prompt` now states that a no-argument call shows the
  current prompt, so help and behaviour agree.
- The phase-117 resume commit is acknowledged by its SHA with a test that pins
  it outside the in-flight subject rule. This is the same sanctioned handoff
  phase-117 used to acknowledge phase-114: the phase after a delivery records
  the prior delivery's SHA, so history is never rewritten and the audit is not
  widened to let a later phase certify another phase's delivery.

Edge-case matrix dispositions:
- empty — covered: empty prompt still evaluates; an absent module annotation is omitted rather than printed as an empty field.
- singleton — covered: every registry name is listed exactly once, and each module is asserted individually.
- boundary — covered: whitespace-only prompt, a prompt wider than the terminal, and the existing 1024-byte prompt limit.
- out_of_bounds — N/A: the new commands take no arguments and take no index from user input.
- type_mismatch — N/A: `:builtins`/`:modules` take no argument; `:prompt` accepts display text, never a typed value.
- numeric_boundary — N/A: no arithmetic or numeric conversion is involved.
- unicode — N/A to the new code paths: prompts are stored and printed as opaque text, and no new parsing or encoding of them was added.
- nesting_recursion — N/A: both listings are flat table reads with no recursion.
- duplicate_missing_keys — covered: vocabulary is sorted/deduplicated once, and the agreement tests pin the exact set.
- malformed_input — covered: an unknown `:prompt` placeholder is refused and leaves the old prompt intact.
- resource_limit — covered: an over-wide prompt is still bounded by the existing `MAX_PROMPT_BYTES` limit and the 300-byte case is printed whole.

## Gates
| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 3172 passed, 0 failed, 0 ignored across 87 suites |
| rbops/verify.sh phase-118 | pass |

## Invariants touched
- None of the language invariants (`.rb`, `to … end`, `set`, `say`, `Value`, `Error`) changed.
- REPL behaviour: a bare `:prompt` now prints the current prompt instead of restoring the default; `:prompt none` restores `>>> `.

## Known gaps / follow-ups
- The command takes only the existing prompt text; there is no per-session persistence of a custom prompt. That is a separate feature.
- A prompt consisting only of spaces is available to the stored format (and tested), but is not directly settable through the command parser because whitespace after `:prompt` is treated as the separator. This matches the existing display-text argument contract rather than adding a second quoting rule.
- `:prompt none` is the restore sentinel, so a literal prompt text of exactly `none` cannot be set through the command. The sentinel is the *exact* word with no trailing display whitespace: `:prompt none ` (with the space a prompt normally ends in) is the literal prompt `none `.
- When a builtin is reachable through more than one module, `:builtins` annotates it with the first matching row of `MODULE_MEMBERS` rather than every spelling. For example, `length` shows `list`, because that row comes first; the member table is a fixed const, so the choice is deterministic.
