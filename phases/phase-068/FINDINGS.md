# Phase 068 — findings outside this phase

## TAB completion still uses the process directory

`src/repl/completer.rs:412` completes a path argument with `current_dir()`, which
is the process directory. After `.cd`, `:load <TAB>` therefore still offers the
process tree while `:load <name>` resolves against the session directory. `.ls`
makes the session directory visible but does not move completion.

This needs the editor's `SessionCompleter` to receive the session `cwd` (the same
snapshot mechanism it already uses for names), or `ReplCompleter::complete_path`
to take the directory as an argument. Out of scope here because it changes the
editor/REPL plumbing and the phase's shell/directory commands are complete and
gated without it.

## `.debug <relative file>` still uses the process directory

`src/repl/debug_repl.rs:440` reads a debug file with `std::fs::read_to_string`
directly. The phase finding named `:load`, `:save` and `:export`; the debug
sub-REPL's file argument should use the same session resolver in a follow-up.
