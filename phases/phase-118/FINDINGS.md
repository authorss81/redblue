# phase-118 — FINDINGS

Work that this phase did not do, with evidence. Nothing here is a phase failure;
none of it belongs to "introspect the session".

## 1. Bare `:prompt` restores the default instead of only showing it

**Where:** `src/repl/commands.rs:83` (`Prompt(Option<String>)`) and
`src/repl/commands.rs:784` (`parse_reads_a_prompt_as_display_text_or_as_no_argument`),
plus `src/repl/mod.rs:4725`.

The phase's definition of done says "`:prompt` prints the current prompt". On
`main` it does the opposite: `ReplCommand::parse(":prompt")` is
`Prompt(None)`, and `set_prompt_format(None)` assigns `DEFAULT_PROMPT`. Two
existing tests pin that, and a test may not be weakened, so the showing half was
added *after* the set — `:prompt rb> ` prints `Prompt: "rb> "`, and a bare
`:prompt` prints `Prompt: ">>> "` for the default it just restored. Both shapes
now report the prompt the session is at; one of them still resets it first.

If the auditor wants a command that shows without setting, the cheapest is a
second spelling (`:prompt?` or `prompt` with `optional_argument` plus a
`show_prompt_only` flag), and it is a table change plus one arm in
`handle_command`. That is a change to how `:prompt` reads, so it is not this
phase's change to make.

## 2. The completer's builtin table is a hand-kept subset of the registry

**Where:** `src/repl/completer.rs:84` (`builtins: module_names()...`) — `builtins: module_names().into_iter().chain(vec![...24 names...])`.

`BUILTIN_NAMES` holds 69 names; the completer offers 24 flat ones plus the 11
modules, so `to_number`, `type_of`, `json_parse`, `starts_with`, `ends_with`,
`replace`, `shift` and 56 others are callable, listed by `:builtins`, and not
completable. This phase's `the_completer_and_the_listing_commands_agree_on_the_
name_set` asserts the direction that can actually be wrong (every name completion
offers is one `:builtins` or `:modules` lists), so the two tables cannot drift
into offering a fiction; the reverse — completion offering less than the language
has — is untouched.

Deriving the completer's table from `BUILTIN_NAMES` would fix it and would change
what tab-completion offers, which is a behaviour change to completion rather than
introspection. Not smuggled into this phase.

## 3. `rbops/verify.sh` is not in this checkout

`git ls-files '^rbops/'` returns nothing and the directory does not exist on
disk; `.github/` is tracked. The phase prompt states the pipeline lives elsewhere,
so the fourth gate could not be run here and REPORT.md says so rather than
claiming a result. Noted for the auditor only: a gate an agent is asked to run
but cannot reach is a gate that has to be run by the dispatcher.