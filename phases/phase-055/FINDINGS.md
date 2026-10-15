# Phase 055 — FINDINGS

## 1. `!!` repeats one line, so after a block it repeats the closing `end`

**Where:** `src/repl/mod.rs`, `Repl::repeat_target`.

This REPL's history is one entry per line — `:history` numbers them that way,
and Up-arrow walks them that way — so "the last line typed" is well defined and
is what this phase's definition of done asks for. The consequence is that after

```
repeat 2 times
    say "line"
end
```

`!!` repeats `end`, which parses as nothing:

```
>>> Error: ParserError: Unexpected token 'end'
  --> 1:1
```

Pinned as intended behaviour by
`tests/repl_test.rs::edge_repeating_the_last_line_of_a_block_is_a_clean_error_not_a_dead_session`
— a clean error and a live session, which is the important part.

Both Ipp and inscript repeat a *command*, and in those a command may span lines.
Doing that here means keeping the buffer `execute_line` runs, not just the last
line: a `last_program: Option<String>` set when a block closes, with `!!`
preferring it when the last typed line was part of a block. That is a change in
what `!!` means, so it belongs in its own phase rather than being smuggled into
a display-polish one.

## 2. A repeated `:prompt` lost its trailing space — fixed in round 1

**Where:** `src/repl/commands.rs` (`verbatim_argument`), `src/repl/mod.rs`
(`session_loop`).

`session_loop` recorded `line.trim()` in the history and dispatched from the line
as typed, so `:prompt rb> ` typed once gave a prompt of `rb> ` and `!!` gave
`rb>`. The finding originally called this a trade-off belonging to a phase that
revisits the history format; the review (round 1, finding 2) pointed out that a
defect the phase knows about and ships is still a defect, and that the trade-off
was avoidable without touching the history format.

Fixed by recording a line the way it is dispatched, for the commands whose
argument *is* display text and no others:
`ReplCommand::keeps_argument_whitespace` asks the table, and
`edge_a_repeated_prompt_keeps_the_space_that_is_part_of_it` pins both sides —
`:prompt rb> ` keeps its space in the history, and `set x to 1   ` is still
recorded `set x to 1`, so nothing new reaches the history file.

The residual question the finding raised is still open, and is smaller than it
was: `:history` can now print a trailing space, and one may be written to a
`:save` file. Both are invisible in a terminal and both round-trip — the save
format is one entry per line and the loader reads whole lines — so the question
now is only whether a history format should be able to hold a trailing space at
all, which is a decision for whoever revisits it.

## 3. The prompt format is per-session

**Where:** `src/repl/mod.rs`, `Repl::prompt_format`.

`:prompt rb> ` is forgotten when the session ends. There is an rc-file question
here (`~/.redblue/history` is the only file the session owns today) and no
decision has been taken about it, so nothing is persisted. A user who likes
`:prompt` currently retypes it once per session; the finding is that the
mechanism for not doing so does not exist yet.

## 4. The finding's line numbers had moved

**Where:** the phase prompt's evidence, `src/repl/mod.rs`.

It cites `src/repl/mod.rs:147` for the banner and `:158` for the prompt; on
`main` at the start of phase-055 they were `src/repl/mod.rs:302` and `:357`, moved
by phase-052. Every defect the finding describes reproduced exactly as written
otherwise, so the phase was worked rather than closed as stale — but the file:line
pairs in a generated phase prompt should be re-anchored by the auditor when it
re-checks a finding, since they go stale in a tree where every phase touches the
same files.

## 5. `cargo test` is slow enough that the pipeline may time out on it

Two of the test binaries take over five minutes each
(`tests/bootstrap_selfhost_test.rs` at ~345 s, `tests/bootstrap_ship_test.rs` at
~421 s on this runner, measured from `cargo test` output). A 120-minute phase
budget absorbs that once; a phase that edits the VM as well would not get a
second run. Not this phase's problem to fix, and no phase should be given less
time because of it, but the auditor may want to know that the two bootstrap
suites dominate the wall clock.
