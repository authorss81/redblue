# Phase 118 — Introspect the session: `:builtins`, `:modules` and `:prompt`

## What changed
| File | Lines | What |
|---|---|---|
| src/repl/commands.rs | +115 −21 | Added `:builtins`/`.builtins` and `:modules`/`.modules` to the command table and enum; a bare `:prompt` now asks for the current prompt, while `:prompt none` restores the default; `:help` names the no-argument form. A quoted empty or whitespace-only argument (`:prompt ""`, `:prompt "    "`) names those prompts, while quotes around other text stay part of it. |
| src/repl/completer.rs | +46 −33 | The completer's builtin and module vocabularies now come from the shared `stdlib` functions instead of a hand-kept list; `builtin_names()`/`module_names()` expose those sets to the agreement tests. |
| src/repl/introspect.rs | +237 −2 | Added `builtin_lines()` and `module_lines()`, both sorted from the same registry the completer uses, with module annotations for builtins that have a dotted spelling, including the `list_map`/`map` pair. |
| src/repl/mod.rs | +450 −6 | Dispatches `:builtins` and `:modules`; `:prompt` answers with the prompt the next line is read at, and `:prompt none` restores `>>> `. Added the command, registry-agreement, prompt, unicode-prompt, oversize-prompt, command-looking-prompt, quoted-empty/quoted-whitespace-prompt, block and sub-REPL edge tests. |
| src/stdlib.rs | +71 −0 | Added `builtin_vocabulary()`, `module_vocabulary()` and `module_of_builtin()`, the shared registry views used by completion and introspection; `module_of_builtin` follows the operation a dotted pair reaches, so `list.map`/`list_map` annotate `(list)` while `console.log` and `files.append` do not annotate the unrelated flat `log` and `append`. |
| SURVEY.md | +1 −1 | Re-anchored the `regex_builtin` citation after the new `src/stdlib.rs` lines moved it. |
| tests/repl_test.rs | +31 −2 | The REPL surface tests were updated for the new commands and prompt behaviour. |
| tests/stdlib_gap_survey_test.rs | +2 −2 | The survey's regex citation anchor follows the moved `src/stdlib.rs` line. |
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
| builtins_annotates_a_name_by_the_operation_the_dotted_pair_reaches | collision: `log (math)` rather than console, bare `append` rather than files, and `list_map (list)` still annotated |
| edge_an_empty_prompt_still_reads_and_evaluates_a_program | empty: an empty prompt still reads and evaluates the next line |
| edge_a_unicode_prompt_is_kept_whole_and_still_reads_a_program | unicode: CJK/RTL/emoji/combining text is printed byte-for-byte and the next line still runs |
| edge_an_oversize_unicode_prompt_is_refused_by_bytes_not_characters | failure assertion: a prompt whose characters fit but whose UTF-8 bytes exceed the limit is refused and the old prompt kept |
| edge_a_prompt_that_looks_like_a_command_is_not_executed | display vs parsing: `:quit ` as a prompt is printed, not executed, and the next line still runs |
| edge_a_prompt_of_only_spaces_is_still_a_prompt | boundary: whitespace-only prompt is display text, not "no argument" |
| parse_reads_a_quoted_empty_or_whitespace_prompt_as_the_prompt | empty/duplicate: `:prompt ""` is empty, `:prompt "    "` keeps its spaces, and other quoted text keeps its quotes |
| edge_a_quoted_empty_prompt_is_set_through_the_command_and_still_evaluates | empty: the command sets an empty prompt and the following line still runs |
| edge_a_quoted_whitespace_prompt_is_set_through_the_command | boundary: a prompt of only spaces is settable through `:prompt` |
| edge_a_quoted_prompt_with_text_keeps_its_quotes | display text: quoting is the empty/whitespace escape, not general unquoting |
| edge_a_quoted_oversize_prompt_is_refused_and_the_old_prompt_kept | failure assertion/resource: a quoted prompt past `MAX_PROMPT_BYTES` is refused and the old prompt kept |
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
- `module_of_builtin` follows the operation a dotted pair reaches rather than
  the member spelling alone: `list.map` annotates the flat `map` as `(list)`
  because `list_operation` says `list_map` and `map` are one operation, while
  `console.log` and `files.append` resolve to their own registered names and no
  longer annotate the unrelated flat `log` and `append`. The new test
  `builtins_annotates_a_name_by_the_operation_the_dotted_pair_reaches` pins
  `log (math)`, a bare `append`, and `list_map (list)`, alongside
  `builtins_annotates_a_known_builtin_with_its_module` pinning `map (list)`,
  `abs (math)` and `files_read (files)`.
- SURVEY.md now cites `src/stdlib.rs:1851`, and the citation test requires the
  cited line to be `fn regex_builtin` rather than any line that mentions regex,
  so the anchor cannot silently drift to a nearby comment again.
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

## Resume follow-up (round 3 — the two prompts the parser could not spell)

The round-2 report listed a gap: an empty prompt and a whitespace-only prompt
were only reachable through the private `set_prompt_format` used by tests, not
through `:prompt`, because `argument` treats whitespace after the command word as
the separator. That is the definition-of-done line "a prompt set to an empty
string still parses input correctly and still evaluates a program" and the
"prompt of only spaces" edge case, so it is now closed in the command parser:

- `ReplCommand::prompt_text` treats a whole argument wrapped in one pair of
  quotes as an escape **only when the quoted text is empty or all whitespace**.
  `:prompt ""` sets the empty prompt and `:prompt "    "` sets four spaces.
- Quoting is not a general unquoting rule: `:prompt "rb> "` still sets the
  literal prompt `"rb> "`, which is the display-text contract the existing
  parser test pins.
- The escape runs before `check_prompt_format`, so a quoted prompt that exceeds
  `MAX_PROMPT_BYTES` is refused by the same limit and the old prompt is kept;
  the new failure-asserting test proves it.
- Five tests were added (one parser test, four surface tests). The full suite is
  now 3183 passed, 0 failed, 0 ignored across 87 suites.

Edge-case matrix dispositions:
- empty — covered: `:prompt ""` sets the empty prompt through the command and the following line still evaluates; an absent module annotation is omitted rather than printed as an empty field.
- singleton — covered: every registry name is listed exactly once, and each module is asserted individually.
- boundary — covered: `:prompt "    "` sets a whitespace-only prompt through the command, a prompt wider than the terminal is kept whole, and the existing 1024-byte prompt limit is exercised on both sides.
- out_of_bounds — covered: a trailing word after `:builtins`/`:modules` is ignored like every command with no usage, and neither takes an index from user input.
- type_mismatch — N/A: `:builtins`/`:modules` take no argument; `:prompt` accepts display text, never a typed value.
- numeric_boundary — N/A: no arithmetic or numeric conversion is involved.
- unicode — covered: a prompt containing CJK, RTL, emoji and a combining mark is printed byte-for-byte and the following line still evaluates.
- nesting_recursion — N/A: both listings are flat table reads with no recursion.
- duplicate_missing_keys — covered: vocabulary is sorted/deduplicated once, and the agreement tests pin the exact set.
- malformed_input — covered: an unknown `:prompt` placeholder is refused and leaves the old prompt intact.
- resource_limit — covered: an over-wide prompt is still bounded by the existing `MAX_PROMPT_BYTES` limit and the 300-byte case is printed whole.

## Gates
| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 3183 passed, 0 failed, 0 ignored across 87 suites |
| rbops/verify.sh phase-118 | pass — VERIFY PASS, 3183 passed / 0 failed. The gate's report-honesty check emitted one WARN because the gate's copy of this file still cited 3178; this file now cites 3183. |

## Invariants touched
- None of the language invariants (`.rb`, `to … end`, `set`, `say`, `Value`, `Error`) changed.
- REPL behaviour: a bare `:prompt` now prints the current prompt instead of restoring the default; `:prompt none` restores `>>> `.

## Known gaps / follow-ups
- The command takes only the existing prompt text; there is no per-session persistence of a custom prompt. That is a separate feature.
- A quoted argument is only unquoted when it is empty or all whitespace. General quoted prompt text still keeps its quotes; the escape exists for the two prompts the separator cannot spell, not as a second quoting rule.
- `:prompt none` is the restore sentinel, so a literal prompt text of exactly `none` cannot be set through the command. The sentinel is the *exact* word with no trailing display whitespace: `:prompt none ` (with the space a prompt normally ends in) is the literal prompt `none `.
- When a builtin is reachable through more than one module, `:builtins` annotates it with the first matching row of `MODULE_MEMBERS` rather than every spelling. For example, `length` shows `list`, because that row comes first; the member table is a fixed const, so the choice is deterministic.
