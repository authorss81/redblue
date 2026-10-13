# Phase 052 — findings for the auditor

Work that belongs to this phase was done in `src/repl/`. The items below were
found on the way and are **not** done. Each is anchored to a file and line as it
stands after phase-052.

## 1. The debugger prompt has no line editor

`src/repl/mod.rs` — `fn debug_stop` still reads with
`io::stdin().read_line(&mut input)` and prints `DEBUG_PROMPT` by hand.

So a session stopped at a `:break` gets a different, worse reader from the one
every other prompt uses: no TAB completion, no Up-arrow history, and Ctrl-C
delivers `SIGINT` to the process, killing the session instead of abandoning the
answer. `debug_stop` is the one place the REPL reads the terminal outside the
read loop, which is exactly why phase-052 put `LineSource` in
`src/repl/editor.rs` and left this behind.

**Suggested phase:** give `debug_stop` a `LineSource` too. It wants a different
answer set (`c`, `s`, `n`, `f`, and the `:break` line number), so it is a
`Validator`/`Hinter` on a second editor rather than a call to `session_loop`.
This phase did not touch it: doing it would have made the diff cover two prompts.

## 2. Two histories that have to be kept in agreement

`src/repl/mod.rs` — `Repl::history` (`ReplHistory`, `src/repl/history.rs`) and
`EditSource`'s `FileHistory` (`src/repl/editor.rs`) are separate stores.

`Repl::restore_history` seeds `Repl::history` from the line source's history
(`LineSource::history`, `src/repl/editor.rs`) at startup so `:history` lists what
Up-arrow offers, but nothing keeps them equal afterwards: `:restore` replaces
`Repl::history` wholesale without touching the editor's, and `:save` writes it to
a path of the user's choosing without touching `~/.redblue/history`. A session
that uses `:save` and `:restore` will see `:history` and Up-arrow disagree.

**Suggested phase:** make `:restore`/`load_from_file` the one way a session's
history changes, and route it through `EditSource` as well. Or drop the seeding
and say plainly in `:help` that `:history` is this session only.

## 3. `Repl::complete` is now reachable from two places

`src/repl/mod.rs:247` — `Repl::complete(word)` and
`src/repl/editor.rs` `EditSource::complete(line, pos)` both end up in
`ReplCompleter::complete_with`. They are the same engine, so there is no second
table to drift, but the tests in `src/repl/completer.rs` and in
`src/repl/editor.rs` now overlap.

**Suggested phase:** none needed unless the count grows. Recording it so a future
reviewer does not read the duplication as a second vocabulary.

## 4. `HOME` rather than a platform home directory

`src/repl/editor.rs` — `default_history_path` reads the `HOME` environment
variable and returns `None` when it is unset or relative. On Windows a session
usually has no `HOME`, so it keeps its history in memory with no warning.

`std::env::home_dir` was un-deprecated in recent Rust and would cover both, but
this phase did not add a platform dependency for it.

**Suggested phase:** one line, on a phase that is already touching the REPL's
startup path. Low severity.