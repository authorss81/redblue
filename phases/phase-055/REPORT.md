# Phase 055 — Polish the session surface: banner, prompt, numbered continuations, and `!!`

## Reproduction (before any change)

```bash
printf 'repeat 2 times\nsay 1\nend\n:quit\n' | cargo run --quiet --bin rb
```

```
Welcome to Redblue v0.1.0 - Interactive REPL
Type ':help' for commands, ':quit' to exit

>>> ..  ..  1
1
>>> Goodbye!
```

Three facts, all of them wrong on their face:

- the greeting says `v0.1.0` while `Cargo.toml:3` says `version = "0.2.0"` — the
  string was a literal in a `println!` (`src/repl/mod.rs:302`), so it could only
  ever be wrong or be fixed by hand;
- both continuation lines read `..  `, so line 2 and line 3 of a block are the
  same four characters and a pasted block cannot be mapped back to what the
  session read;
- no `!!` exists, and the greeting never says what TAB or the Up arrow do.

The finding cited `src/repl/mod.rs:147`/`:158`; on `main` at the start of this
run the same three defects are at `src/repl/mod.rs:302` and `src/repl/mod.rs:357`
(the line numbers moved in phase-052). Re-verified before changing anything:
still reproduces, exactly as quoted above.

## What changed

| File | Lines | What |
|---|---|---|
| `src/repl/banner.rs` | +50 −0 | `VERSION` (`env!("CARGO_PKG_VERSION")`), `ENGINE`, `print_greeting`, `greeting_text` — version, engine, and the TAB / Up-arrow / `!!` hint |
| `src/repl/mod.rs` | +180 −14 | the greeting call in `Repl::run`; `prompt_for_next_line` (numbered continuations), `render_prompt_format`, `set_prompt_format`, `check_prompt_format`, `repeat_target`, `repeat`; `!!` dispatch in `session_loop`; the `mod surface` test module |
| `src/repl/commands.rs` | +92 −2 | `ReplCommand::Prompt(Option<String>)` and its table row; `optional_argument` and `verbatim_argument` on `CommandSpec` and the two parse arms they drive |
| `src/repl/editor.rs` | +30 −7 | `highlight_prompt` (the prompt painted through `Paint::Prompt`); `PlainSource` carries the session's `Style` and paints its prompt with it |
| `tests/repl_test.rs` | +165 −1 | 9 end-to-end tests over the real binary; one pre-existing needle re-pointed (below) |

`must_touch: ["src/"]` is satisfied: four files under `src/repl/`.

Display only: nothing in the execution path moved. `execute_line`,
`needs_continuation`, `handle_command`'s dispatch and the VM are untouched except
for reading the next prompt's text from `prompt_for_next_line()` instead of an
inline `if`.

### Where the prompt is painted, and why not in the session

rustyline documents that a prompt handed to `readline` "should not be styled
(in case the terminal doesn't support ANSI) directly: use
`Highlighter::highlight_prompt` instead" (`rustyline-18.0.1/src/lib.rs:636`) —
it measures prompt columns to place the cursor and the line after it. So
`session_loop` hands over **plain** prompt text and the two readers colour it:
`SessionCompleter::highlight_prompt` for the editor, and `PlainSource`, which
now carries the session's `Style` and paints with `Paint::Prompt`. With colour
off both are byte-identical to the text they were given.

### `!!` repeats one history line

`repeat_target` walks the history backwards for the last entry that is not `!!`
and not empty, so `!!` is repeatable: a second `!!` reaches the same line rather
than the first `!!`. The line it reaches goes through `handle_command` exactly
as if it had been typed, so `!!` over `:vars` re-runs `:vars`. An empty history
is an `Err` naming `!!`, printed and skipped — never an index into nothing.

Because a block's lines are separate history entries, `!!` after a multi-line
block repeats the block's **last line** (`end`), which does not parse on its
own. That is the same line Up-arrow would have handed back, and the session
reports it cleanly and carries on (pinned by
`edge_repeating_the_last_line_of_a_block_is_a_clean_error_not_a_dead_session`).
Repeating the whole block instead is a follow-up: FINDINGS.md §1.

### `:prompt` and the two flags it needed in the command table

`:prompt <format>` fills `{engine}`, `{version}` and `{history}` — the only
placeholders that exist, checked before the format is stored, so a typo is
refused by name instead of being printed before every line forever. `:prompt`
with no argument restores `>>> `.

Two facts about an argument that no command had before, so the table carries
them (`src/repl/commands.rs`):

- `optional_argument` — `:prompt` alone is a command that restores the default,
  not one missing its argument. The other 23 commands report
  `Usage: :load <filename>` exactly as before.
- `verbatim_argument` — a prompt's trailing space is part of the prompt. Every
  other command's argument is still `trim`med at both ends, which two existing
  tests in `commands.rs` pin (`parse("  :inspect counter  ")` →
  `Inspect("counter")`, `parse(":load   /tmp/a  b.rb  ")` → `Load("/tmp/a  b.rb")`).

Both fields are `false` everywhere else: 23 mechanical lines each.

## Tests added

Rust, 9 in `tests/repl_test.rs` (the real binary over a pipe) and 8 in
`src/repl/mod.rs` (`mod surface`, the session driven over a `ScriptedSource` that
records every prompt it is asked at).

| Test | Edge class covered |
|---|---|
| `edge_the_banner_names_the_crate_version_and_not_a_stale_one` | stale version; engine and hint present (`v0.1.0` must be gone) |
| `a_block_is_read_at_a_prompt_that_numbers_its_continuations` | `..  2` once, `..  3` once, `..  4` never — boundary on both sides |
| `edge_bang_bang_repeats_the_last_line_with_its_output` | repeat includes the output, not a silent re-run |
| `edge_bang_bang_at_the_start_of_a_session_says_there_is_nothing_to_repeat` | **asserts a failure**: empty history is an error naming `!!`, session survives |
| `a_prompt_command_replaces_the_prompt_and_refuses_an_unknown_placeholder` | **asserts a failure**: `{nope}` refused by name, default restored |
| `edge_bang_bang_after_a_failed_line_repeats_the_failure` | failure is re-run, not skipped |
| `edge_bang_bang_still_repeats_after_a_reset` | history outlives `:reset` |
| `edge_bang_bang_twice_repeats_the_same_line_twice` | `!!` in the history is not itself a repeat target |
| `edge_repeating_the_last_line_of_a_block_is_a_clean_error_not_a_dead_session` | malformed input: a fragment repeats into a clean error |
| `repl::tests::surface::the_greeting_names_the_crate_version_the_engine_and_what_the_keys_do` | version comes from `CARGO_PKG_VERSION`, not a literal |
| `repl::tests::surface::edge_the_greeting_with_colour_off_is_plain_aligned_and_complete` | colour forced off: no escape, no blank line, no indent, every escape closed |
| `repl::tests::surface::a_block_is_read_at_a_prompt_numbered_by_the_line_of_the_block` | prompts captured: `>>> `, `..  2`, `..  3`, `>>> ` |
| `repl::tests::surface::a_prompt_format_becomes_the_prompt_the_next_line_is_read_at` | all three placeholders filled; `:prompt` alone restores the default |
| `repl::tests::surface::edge_an_unknown_placeholder_is_refused_by_name_and_the_prompt_is_kept` | **asserts a failure**: unknown placeholder and unclosed brace both `Err`; the old prompt survives |
| `repl::tests::surface::edge_repeat_reruns_a_line_that_failed_rather_than_skipping_it` | **asserts a failure** on empty history; repeat reaches the failed line; session continues |
| `repl::tests::surface::edge_a_second_repeat_reaches_the_same_line_as_the_first` | two `!!` in the history |
| `repl::tests::surface::edge_repeat_still_works_after_a_reset` | reset clears bindings, not history |

The five red before the change: the four integration tests above plus
`a_block_is_read_at_a_prompt_that_numbers_its_continuations`, all failing against
`main` (34 passed, 5 failed). `edge_the_banner_…` failed on
`Welcome to Redblue v0.1.0 - Interactive REPL`; the continuation tests failed on
`..  ` having no number; the `!!` tests failed on `Error: ParserError: Unexpected
token '!'`.

### Edge-case matrix

- **empty / "nothing"** — covered: `!!` as the first line of a session (nothing
  to repeat, error names `!!`); `:prompt` with no argument restores the default;
  `:prompt   ` (whitespace only) is read as no argument.
- **singleton** — covered: a three-line block asks for exactly one continuation
  per remaining line; `:prompt` with a one-placeholder format.
- **boundary** — covered: `..  2` and `..  3` each appear exactly once and
  `..  4` never does, for a block of three lines; `{history}` renders the
  history count at the moment of the prompt.
- **out_of_bounds** — N/A: nothing here indexes by user-supplied number. The one
  index-like read is `repeat_target`, which searches backwards and returns
  `None`; the empty-history case is asserted.
- **type mismatch** — covered: `!!` after a block repeats the closing `end`,
  which is not a program, and is reported as a clean parse error with the
  session alive.
- **numeric boundary** — N/A: no arithmetic. `{history}` is a count rendered
  with `usize`'s own `Display` and the continuation number is
  `buffer.len() + 1`, which the buffer limits (`MAX_BLOCK_LINES`) already bound.
- **unicode** — N/A by scope, with one caveat recorded in FINDINGS.md §2: a
  prompt format is passed through byte-for-byte, so `{version}`/`{engine}`
  (both ASCII constants) are safe, but a *user's* format containing wide
  characters is the terminal's to measure. No new indexing is done on it.
- **nesting / recursion** — N/A: the session surface holds no recursive
  structure. Nested blocks keep numbering (`..  2`, `..  3`, …) through the same
  one counter, which `MAX_BLOCK_LINES` bounds.
- **duplicate / missing keys** — N/A: no record keys are read. The nearest
  analogue is a placeholder the prompt does not know, covered by
  `edge_an_unknown_placeholder_is_refused_by_name_and_the_prompt_is_kept` (the
  missing-key case) and by the refusal of an unclosed `{`.
- **malformed input** — covered: an unclosed `{` in a prompt format; a format
  naming an unknown placeholder; `!!` in a session with nothing before it;
  repeating a block's closing `end`.
- **resource limit** — covered by construction and asserted: the prompt is one
  `String` per line (`render_prompt_format` replaces the format's placeholders in
  place, over a format `check_prompt_format` has bounded to `MAX_PROMPT_BYTES`, so
  the work per line is bounded too), and the continuation number comes from a
  buffer already capped by `MAX_BLOCK_LINES`/`MAX_BLOCK_BYTES`.
  `check_prompt_format` scans the format once with `find` over `&str`; a format of
  `{{{` is refused rather than rescanned.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass (no warnings) |
| `cargo test` | pass — 1651 passed, 0 failed, 0 ignored across 63 test binaries |
| `examples/*.rb` under `rb run` | pass — every example exits 0 |
| `./rbops/verify.sh phase-055` | **not run**: there is no `rbops/` directory in this checkout (`ls` shows `AGENTS.md … phases/ src/ tests/ …`), and the pipeline that owns it lives outside the project. Reported rather than claimed. |

## One pre-existing test edited

`tests/repl_test.rs`,
`edge_the_report_of_an_unclosed_block_counts_the_lines_it_did_not_echo`: its last
assertion was `assert_stdout_lacks("42")`, a vestige of a sibling test whose
block body was `say 6 * 7`. A 41-line block now prints `..  42` as the prompt
for the line that never came, so a bare `"42"` can no longer distinguish "the
block ran and printed 42" from "the session numbered its own lines". The needle
is now `"= 42"` — the value form, which is what the assertion is about — and a
comment says why. The assertion is not loosened in what it claims: the block
must not have run. It was reported here rather than changed silently because it
is the only pre-existing test this phase had to touch.

## Invariants touched

- None of the language invariants. No `.rb` grammar, no `Value` variant, no
  `Error` variant, no `say`, no `end`.
- Display only: the greeting's text, the prompt's text, one new `!!` input, and
  one new command. `:help`, completion and `parse` were extended, never
  re-scoped.
- `PlainSource` gained a `Style` field, so it is no longer a unit struct. It is
  `pub`, but it is constructed only by `line_source()` inside this crate; no test
  outside `src/repl/editor.rs` built one.

## Known gaps / follow-ups

- `!!` repeats one history line, so after a multi-line block it repeats the
  block's closing `end`. Repeating the whole executed block is the better
  behaviour and is not done → FINDINGS.md §1.
- ~~A repeated `:prompt rb> ` loses its trailing space~~ → fixed in round 1, see
  FINDINGS.md §2 for what the fix costs and what it leaves open.
- The prompt format is per-session. Nothing persists it to
  `~/.redblue/history` or any rc file, so `:prompt` has to be retyped in a new
  session. Persisting it is a feature decision, not a bug fix → FINDINGS.md §3.

---

# Round 1 — review findings

Four findings, one BLOCKER and three MAJOR. All four are fixed; no gate was
weakened, no test deleted or skipped, and no `allow` added. Every new test was
checked by mutating the code it names and watching it go red.

Tests added: 5 (one pre-existing test was renamed, not added or deleted).

## 1. BLOCKER — the piped-session escape test asserted nothing about output

**Where:** `src/repl/editor.rs`.

`edge_a_piped_session_emits_no_escape_codes_even_with_colour_forced_on` named an
output guarantee and asserted `!source.highlights()`. That stayed true whatever
`PlainSource` wrote, and `PlainSource::read_line` paints its prompt through
`Style::paint` — so a pipe under `REDBLUE_COLOR=always` has been emitting
escapes, and the test called that a pass. Two doc comments made the same claim
in prose (`LineSource::highlights`, `line_source`): "a session that never has an
editor has no way to emit an escape code at all".

Fixed by naming the guarantee that actually holds and asserting the bytes:

- `PlainSource::painted_prompt` is split out of `read_line`, so the bytes a piped
  session writes are something a test can hold rather than something a test has
  to capture from stdout.
- the test is renamed
  `edge_a_piped_session_paints_its_prompt_and_never_its_input` and asserts three
  things: no highlighter on the pipe path however colour is configured; the
  prompt is the prompt byte for byte with colour off (what a pipe gets by
  default); and with colour forced on it is exactly `paint(Prompt, ..)` — two
  escapes, one open and one close, the prompt text surviving between them.
- the two doc comments now say what is true: a pipe has no editor, so nothing it
  reads is echoed or coloured token by token, and a prompt is display text both
  readers paint.

Pinned end to end as well, because a unit test cannot tell a promise from an
implementation: `tests/repl_test.rs::edge_a_piped_session_prompts_plainly_until_colour_is_asked_for`
runs the real binary twice — once with `REDBLUE_COLOR` and `NO_COLOR` cleared
from the child, asserting the output contains no escape at all, and once with
`REDBLUE_COLOR=always`, asserting the prompt arrives painted. `session()` gained a
`session_with_env` sibling for this; the variables are set on the *child*, never
on the test process, because the environment is process-global.

Mutation-checked: painting the prompt as `Paint::Dim` instead of `Paint::Prompt`
fails the unit test.

## 2. MAJOR — a repeated `:prompt` lost its trailing space

**Where:** `src/repl/mod.rs` (`session_loop`), `src/repl/commands.rs`.

The history took `line.trim()` while dispatch took `line.trim_start()`, so
`:prompt rb> ` was recorded `:prompt rb>` and `!!` replayed it as a different
prompt — silently, and pinned as "known" in FINDINGS.md §2.

Fixed by recording a line the way it is dispatched, for the commands whose
argument *is* display text and no others.
`ReplCommand::keeps_argument_whitespace` asks the table (and the "which sigil,
which word, which entry" lookup it needed was factored into `spec_for`, so the
parser and the recorder cannot answer differently). `session_loop` pushes `typed`
for those lines and `input` for everything else.

`edge_a_repeated_prompt_keeps_the_space_that_is_part_of_it` pins both sides of
the decision: the prompts of a `:prompt rb> ` / `!!` / `:quit` session are
`>>> `, `rb> `, `rb> ` — and a `set x to 1   ` is still recorded `set x to 1`, so
nothing new reaches the history file.

Mutation-checked: recording the trimmed line again fails it.

## 3. MAJOR — `:prompt` had no parse-level row in the alias table

**Where:** `src/repl/commands.rs`.

`parse_reads_the_whole_alias_table` listed 23 commands and no `:prompt`, so the
two table flags this phase added — `optional_argument` and `verbatim_argument` —
had no parse-level test on either side.

Added three rows (`:prompt` → `Prompt(None)`, `:prompt   ` → `Prompt(None)`,
`:prompt rb> ` → `Prompt(Some("rb> "))`), a `Prompt(Some(value))` arm in the
carried-argument match, and a focused test
`parse_reads_a_prompt_as_display_text_or_as_no_argument` that asserts the three
shapes whole, behind both sigils, a placeholder left unsubstituted, and — the
other end of the rule — `:inspect counter  ` still trimmed.

Mutation-checked: setting `verbatim_argument: false` on `:prompt` fails three
tests.

## 4. MAJOR — the prompt format was unbounded

**Where:** `src/repl/mod.rs`.

`set_prompt_format` stored whatever it was given while blocks are capped by
`MAX_BLOCK_LINES`/`MAX_BLOCK_BYTES`, and `render_prompt_format` re-renders the
whole thing before every line — so one piped `:prompt` line set an allocation the
session then paid on every line it read.

`MAX_PROMPT_BYTES` (1024) is now enforced in `check_prompt_format`, beside the
placeholder check and for the same reason: a prompt is printed before every
line, so what it costs is paid forever. The refusal quotes the *size*, not the
format, so refusing an oversize prompt is not itself a flood — and because the
check runs before the format is stored, the unclosed-brace refusal that
interpolates it is bounded too.
`edge_an_oversize_prompt_format_is_refused_and_the_prompt_is_kept` asserts both
sides of the boundary (exactly at the limit is accepted, one byte over is
refused), that the message names the limit and stays under 200 bytes, and that
the prompt the session had is still the prompt.

Mutation-checked: skipping the size check fails it.

## Round 1 gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass (no warnings) |
| `cargo test` | pass — 1657 passed, 0 failed, 0 ignored across 63 test binaries |
| `rb run examples/*.rb` (7 files) | pass — every example exits 0 |

`./rbops/verify.sh phase-055` still cannot be run here: there is no `rbops/`
directory in this checkout. Reported rather than claimed, as in round 0.

---

# Round 2 — review findings

One BLOCKER, one MAJOR and two MINOR. All four are fixed; no gate was
weakened, no test deleted or skipped, and no `allow` added. Every new assertion
was checked by mutating the code it names and watching it go red.

Tests added: 4 (2 in `tests/repl_test.rs` over the real binary, 2 in
`src/repl/mod.rs`). One existing test was strengthened, not loosened: the two
continuation needles became `"..  2 "` / `"..  3 "`, which is a strictly longer
prefix and fails on the pre-fix prompt.

## 1. BLOCKER — a piped `:prompt` stored the newline the line arrived with

**Where:** `src/repl/mod.rs` (`session_loop`), `src/repl/editor.rs`
(`PlainSource::read_line`).

`std::io::Stdin::read_line` hands back the newline it stopped at. Round 1 made the
`:prompt` argument *verbatim*, so what round 0 had `trim`med away became the
prompt itself: `printf ':prompt rb> \n…' | rb` stored `"rb> \n"` and every prompt
after it printed a blank line of its own —

```
>>> rb> $
1$
rb> $
```

— and the history entry, the same string, was two lines in a file that is one
entry per line.

Fixed at both ends of the seam. `PlainSource` now honours the contract its own
`Line::Text` documents ("a line the user finished, without its newline"), and
`session_loop` strips the trailing `\r\n` itself rather than trusting a source to
have done it, so the guarantee holds for any source. Only the terminator goes:
trailing spaces are a prompt's own and round 1's fix depends on them surviving.

`edge_a_piped_prompt_command_leaves_no_newline_in_the_prompt_or_the_history` and
`edge_a_piped_prompt_command_leaves_the_newline_out_of_the_prompt` pin it.

Mutation-checked both ways: reverting either strip alone leaves the behaviour
correct and the end-to-end test green, and reverting both turns it red.

## 2. MAJOR — the surface tests fed a shape no pipe delivers

**Where:** `src/repl/mod.rs` (`mod surface::prompts_for`).

`ScriptedSource` was fed `Line::Text("…")` with no newline, so the numbered
continuations and `:prompt` tests exercised a line shape no `PlainSource` has
ever produced — which is why finding 1 passed them. `prompts_for` now appends the
newline a reader stops at, so every test in that module runs the shape the
integration tests drive the real binary with.

## 3. MINOR — a `}` with no `{` in front of it was accepted

**Where:** `src/repl/mod.rs` (`check_prompt_format`).

`:prompt foo}` was stored and rendered literally, which is the same class of
mistake the closed set exists to refuse — it is just the other half of the rule.
Refused now, with a message that names the brace, at the end of a format and
before a placeholder as well as at the start of one.

Pinned by two new assertions in
`edge_an_unknown_placeholder_is_refused_by_name_and_the_prompt_is_kept` (one for
each arm of the check, so neither can be dropped without a test going red) and
end to end by
`edge_a_prompt_format_that_closes_a_brace_it_never_opens_is_refused`.

Mutation-checked: disabling either arm turns the relevant test red.

## 4. MINOR — a continuation prompt had no gap before the line being typed

**Where:** `src/repl/mod.rs` (`prompt_for_next_line`).

`..  2` ran straight into whatever was typed, where `>>> ` has a space. Rendered
`..  2 ` now, and the pinned prompts in `mod surface` and the needles in
`tests/repl_test.rs` were updated to match — both now carry the trailing space, so
they fail on the prompt this replaces.

Mutation-checked: dropping the trailing space fails both.

## Round 2 gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass (no warnings) |
| `cargo test` | pass — 1660 passed, 0 failed, 0 ignored across 63 test binaries |
| `rb run examples/*.rb` (7 files) | pass — every example exits 0 |

`./rbops/verify.sh phase-055` still cannot be run here: there is no `rbops/`
directory in this checkout. Reported rather than claimed, as in rounds 0 and 1.
