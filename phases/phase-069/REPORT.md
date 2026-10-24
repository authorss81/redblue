# Phase 069 — Find things without leaving: `.search` across docs, history, bindings and help

## Reproduction

Before the change, `.search` was not in the command table, so the session treated
the whole line as a mistyped command:

```
$ printf '.search split\n:quit\n' | rb
>>> Unknown command 'search'. Type :help for available commands.
```

After the change, the same line answers from the registry (the same shape
`.sighelp` reads), and the other three sources are searched with it:

```
$ printf '.search split\n:quit\n' | rb
>>> Search results for 'split':
  [builtin] split — split(text, separator) -> list of text; an empty separator has no answer
```

A keyword that matches nothing names every source as empty rather than printing
a blank page, a one-character keyword is refused with the minimum stated, and
regex characters are matched literally:

```
$ printf '.search zzzz\n.search x\nset xs to [1, 2]\n.search [1\n:quit\n' | rb
>>> No matches for 'zzzz'.
  [builtin] (no matches)
  [help] (no matches)
  [binding] (no matches)
  [history] (no matches)
>>> Error: .search needs at least 2 characters; 'x' is too short.
>>> >>> Search results for '[1':
  [binding] xs = [1, 2]
  [history]    3  set xs to [1, 2]
```

## What changed

| File | Lines | What |
|---|---|---|
| `src/repl/commands.rs` | +27 −0 | `ReplCommand::Search(String)`, its `COMMAND_TABLE` row (`:search <keyword>`, so `:help`, the completer and the parser all pick it up from one place), `argument_text` coverage, and the parse test/rows |
| `src/repl/mod.rs` | +402 −0 | `MIN_SEARCH_KEYWORD`, dispatch, `print_search`, `search_answer`, and the nine search tests |

`search_answer` gathers four labelled sources and orders the hits by how closely
they name the query: exact name first, then a name the query starts, then a
match anywhere in the text. The four sources are:

| Source | Read from | Label |
|---|---|---|
| builtin docs/signatures | `stdlib::BUILTIN_SIGNATURES` — the same table `.sighelp` reads | `[builtin]` |
| `:help` text | `ReplCommand::help_lines()` — the same lines `:help` prints | `[help]` |
| session bindings | `Repl::session_bindings()` — the same names `:vars` lists | `[binding]` |
| history | `ReplHistory`, with 1-based line numbers as `:history` prints them | `[history]` |

The command is read-only: it clones the bindings, reads history by index, and
never executes, binds, reorders or writes anything. The line currently being run
is already in the history when the command sees it, so a search skips that one
line; otherwise every query would match itself and no source could ever be
reported empty.

## Tests added

| Test | Edge class covered |
|---|---|
| `search_is_a_command_and_not_a_typo` (commands.rs) | malformed input — `.search split` reaches a command, not `Unknown` |
| `search_finds_each_source_and_labels_it` | builtin + binding + history, each asserted by its own label and content |
| `search_finds_the_help_text_too` | `:help` source — `:sighelp` is found in the help table |
| `search_ranks_an_exact_name_above_a_substring` | boundary/ranking — exact `split` before `splitter` in one fixed query |
| `search_history_hits_show_the_number_that_recalls_them` | history numbering — lines 1 and 2 carry their own numbers |
| `edge_search_with_no_matches_names_each_empty_source` | empty — all four sources named empty, including the search line itself |
| `edge_search_refuses_a_one_character_keyword_and_bare_search` | malformed input / failure — one character and whitespace are refused, bare `.search` is a usage error |
| `edge_search_treats_regex_characters_literally` | malformed input — `.*` and `[1` are text, not patterns |
| `edge_search_leaves_the_session_exactly_as_it_was` | state/resource — bindings, history order and last output unchanged |
| `edge_search_is_case_insensitive_and_carries_unicode` | unicode — `HÉLLO` finds `héllo 😀` and prints it unchanged |

### Mandatory edge-case matrix

| Row | Status |
|---|---|
| empty | covered — no-match query names all four sources empty; empty and whitespace-only keywords are refused; a fresh session has no bindings or history |
| singleton | covered — one exact builtin hit and one binding hit in the ranking test; history numbers are asserted per line |
| boundary | covered — exact name versus substring ranking, and the two-character minimum versus a one-character refusal |
| out_of_bounds | N/A — no indexing or positional access is added; history is walked by `0..len` with a `get`, and a missing source is reported empty |
| type_mismatch | N/A — the argument is always text; there is no typed value to coerce |
| numeric_boundary | N/A — no arithmetic or numeric formatting is added; history numbers are the session's own `usize` index + 1 |
| unicode | covered — `edge_search_is_case_insensitive_and_carries_unicode` |
| nesting_recursion | N/A — no nested structure is walked; the search is four flat scans over the registry, help lines, bindings and history |
| duplicate_missing_keys | N/A — no keyed collection is added; missing matches are covered by the per-source empty report |
| malformed_input | covered — bare `.search`, whitespace-only, one-character, `.*` and `[1` |
| resource_limit | covered — the scans are bounded by the registry, the help table and `HISTORY_LIMIT` (1000); the command allocates a bounded hit list and does not run or recurse |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — zero warnings |
| `cargo test --all-targets` | 2151 passed, 0 failed, 0 ignored |
| `../rbops/verify.sh phase-069` | pass — `VERIFY PASS — phase-069` |

## Invariants touched

- None in the language: no grammar, keyword, `Value`/`Error` variant, `.rb`
  extension or existing test changed.
- REPL-only: `.search`/`:search` is a new command-table entry, so it is also
  documented by `:help` and completed by TAB from the same table.

## Known gaps / follow-ups

- None. The four sources the phase names are the four the command reads; it does
  not index `SPEC.md`, `ROADMAP.md` or example files, which were not part of the
  finding.
