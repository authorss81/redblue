# Phase 061 — Complete after the dot, edit in `$EDITOR`, and teach newcomers: member completion, `.edit`, `:tutorial`

## State of the phase when this run started

The phase's implementation was **already in the tree**, committed as `373513e`
("resume phase-061: adopt preserved work", +2247 −22 across 7 files, +1907 −22 of
it under `src/`). This run did not re-derive it and did not remove any of it. It
ran the gates first, as instructed, and found:

| Gate | Result at the start of this run |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | **1 failure**: `unreviewed_work_in_head_history_is_acknowledged` |
| `../rbops/verify.sh phase-061` | **FAIL, 4 checks** — `no implementation change — must_touch requires a change under one of: src/`, `no test asserts a failure is produced`, `test quota not met: 0 rust`, `no test named edge_*` |

The second row of that table is the one worth stopping on. `verify.sh` measures
the **working-tree diff**, not the phase's commits, so a run that adopts the
resume commit and changes nothing else reports "0 rust" tests and "no
implementation change" no matter how much of the phase is already on HEAD. This
run therefore had to do real work of its own under `src/` — which is where the
behavioural defect in "the finding that reproduced" below came from, rather than
from anywhere near the pipeline.

Every one of the phase's three features was already implemented, so the original
finding **does not reproduce in its stated form**: `json.` completes, `.edit`
exists, `:tutorial` and `:hint` exist, and `(` after a name shows a signature.
That was checked by running them, not by reading the diff:

```
$ printf ':tutorial\n:quit\n' | ./target/debug/rb
>>> Redblue tutorial — type each snippet, or ask for a hint with :hint

  1. A value, and a name for it
  …
  6. Where to go next
     Type `json.` and press TAB: the REPL offers the members of a module. Type
     `(` after a name to see its shape. For more, `:examples`.

  Stuck on a step? Type :hint for the step you are on.

$ printf ':tutorial\n:hint\n:quit\n' | ./target/debug/rb
>>> Hint for step 1 of 6: Type `set name to "World"`, then `say name` — the name
answers with the value it was given.

$ printf ':hint\n:quit\n' | ./target/debug/rb
>>> There is no tutorial step in progress — type :tutorial to start one, and
:examples for a catalogue of what Redblue can do.

$ printf 'set a to 1\n.edit\n:quit\n' | env -u EDITOR ./target/debug/rb
>>> Error: .edit has no editor to open: set EDITOR to the program you edit with,
for example `export EDITOR=vi`

$ printf ':help\n:quit\n' | ./target/debug/rb | grep -E 'tutorial|hint|edit'
  :tutorial, :tour                 Walk through the guided tour, one numbered step at a time
  :hint                            Hint for the tutorial step this session is on
  :edit                            Edit the block being written in $EDITOR, and run what comes back
```

What did reproduce was the gate: the resume commit carrying that work was itself
unacknowledged, and the audit says so in as many words.

## The finding that reproduced: the phase's own resume commit was not acknowledged

`cargo test --all-targets`:

```
---- unreviewed_work_in_head_history_is_acknowledged stdout ----
unreviewed-work commits selected: 31 | acknowledged: 30 | unacknowledged: 1
| stale entries: 0 | UNACKNOWLEDGED:
373513e4bba22305b6967f524ea02f1daa497532	resume phase-061: adopt preserved work

thread 'unreviewed_work_in_head_history_is_acknowledged' panicked at
tests/unreviewed_work_ack_test.rs:493:5:
1 commit(s) in HEAD's history carry unreviewed work and are not on the
acknowledgement list in this file
```

That is the same failure phase-060 hit and fixed the same way: the file's own
contract requires an entry, with a reason, for every commit carrying unreviewed
work, and the entry was missing. Adding it is the fix — the audit is right, the
list is incomplete. Nothing in the test was weakened, and no assertion was
loosened: `ACKNOWLEDGED` swapped one data row.

The row it replaced named `0e84349b84327cf76c6dcd4fb2a8d1e5ade4411c`, the phase's
*earlier* resume. That commit is not an ancestor of HEAD any more:

```
$ git merge-base --is-ancestor 0e84349b84327cf76c6dcd4fb2a8d1e5ade4411c HEAD; echo $?
1
```

and `scan` reports an acknowledgement naming a commit this history does not
contain as **stale**, which is the second assertion in the same test. So the
fix is a swap, not an addition: the entry now names `373513e`, says what it
carries (`+2247 −22` across 7 files, `+1907 −22` of it under `src/`), and records
that `0e84349` is not listed because it is not in this history — the same
sentence the phase-059 and phase-056 rows already use for the same situation.

```
$ cargo test --test unreviewed_work_ack_test
   → the whole file is green: 13 tests, 0 failed, 0 ignored
```

## A second flake, in `tests/repl_color_test.rs`, that the full suite found and isolation did not

The first full-suite run of this phase reported one failure in a seventeen-test
binary. `repl_color_test` is the only binary with seventeen tests, and it passed
on all fifteen isolated re-runs, so the failure was a race rather than a wrong
assertion. It was one, and it is now fixed.

`src/repl/banner.rs:97` reads `COLUMNS` out of the process environment.
`tests/repl_color_test.rs` had a `COLUMNS_LOCK` for that, and **two** tests took
it: the two that *wrote* `COLUMNS`. Locking only the writers is not enough,
because a reader races a writer just as surely as two writers race each other,
and five more tests in the file read the environment without the lock:

| raced pair | what the victim saw | which test failed |
|---|---|---|
| `the_banner_is_ascii_when_asked_and_unicode_otherwise` vs `edge_a_narrow_terminal_centres_without_panicking` | `COLUMNS=10` wrapping the wordmark, so the ASCII and Unicode renderings came out with different line counts | the `lines().count()` shape assertion |
| `forced_is_the_only_way_tests_control_colour` vs `detection_precedence_is_explicit_then_no_color_then_tty` | `Style::detect()` under a temporarily-set `REDBLUE_COLOR=always` | `assert!(!Style::detect().enabled())` |

All nine environment-touching tests in that file now take one lock, and the lock
is named `ENV_LOCK`/`pin_env` rather than `COLUMNS_LOCK`/`pin_columns`, because
it now serialises `COLUMNS`, `REDBLUE_ASCII`, `REDBLUE_COLOR` and `NO_COLOR`
rather than one of them. No assertion changed and no test was removed: the lock
makes each test see the environment it set up instead of a sibling's.

## The run before this one, and its two reviews

Kept as history, because the three tests below that this run's report describes
were added in it and a reviewer comparing diffs will want to know which is
which.

The first review of that run was not a formality: it found that the report
described `0e84349` and one acknowledgement row while the working tree carried
`+639 −114` across `src/repl/editor.rs` and `src/repl/mod.rs` — scratch-directory
hardening, `.edit` going through `edit_with`, and the tour progressing — none of
which the report mentioned, and that the report shipped a "known gap" —
*`:tutorial` sets `tutorial_step = Some(0)` and nothing advances it* — describing
a defect that that run's own diff fixes. Both are gone: the "What changed" table
below is written against `git diff HEAD`, and the tour advances.

The second review went further into the code and found four things worth
changing rather than describing:

- **`.edit` had no block limit.** The session holds at most `MAX_BLOCK_LINES`
  lines or `MAX_BLOCK_BYTES` bytes for a block it is typing, enforced in
  `execute_line` — and the file an editor saved was read with an uncapped
  `read_to_string` and parsed. `read_edited_block` now caps the *read* at
  `MAX_BLOCK_BYTES + 1` bytes (`Read::take`), so the bytes past the limit are
  never in memory, and applies the same `exceeds_block_limits` answer on the way
  out. Both sides of both limits are asserted: a block of exactly 500 lines and
  a block of exactly 65 536 bytes run, one line and one byte more do not.
- **The scratch directory was removed on every path except the two that come
  before the editor runs.** The `chmod` and the write that follow `create_dir`
  returned before the `remove_dir_all`, so those two failures leaked
  `redblue-edit-<pid>-<n>/` into `std::env::temp_dir()` — which is what
  `edge_a_failed_edit_leaves_no_copy_of_the_buffer_behind` claimed and could not
  show, because it only exercises failures that happen after the removal. The
  directory is now a `ScratchDir` guard removed in `Drop`, so every way out of
  the edit takes it, including a panic.
- **`:reset` forgot the bindings but kept the tour.** A session reset half-way
  through the tour answered `:hint` about a step whose snippet needed the
  bindings the reset threw away. `reset_session` clears `tutorial_step` and
  `tutorial_typed` with the rest, and a test pins it.
- **Two tests asserted what their comments did not say.** `signature_hint("say
  split(", 11)` was asserted `None` "and the end of the line itself is not after
  the bracket either" — 11 is past the end of a ten-byte line, and position 10
  (the end) *does* answer; `signature_hint("say é(", 7)` was asserted `None` "not
  after a `(` that is the last thing in it", when 7 is exactly after the `(` and
  the answer is `None` only because `é` is not a builtin. Both now assert the
  boundary they describe. And a test whose editor shell script uses GNU `stat -c`
  is gated on `target_os = "linux"` rather than `cfg(unix)`, so it cannot fail on
  a BSD or macOS host.

Nothing was weakened to achieve any of that: no `#[ignore]`, no `// skip`, no
`allow(clippy::`, no deleted test. The two tests that changed what they assert
assert more, not less.

## What changed

`must_touch: ["src/"]` is satisfied: `src/repl/editor.rs` and `src/repl/mod.rs`
are both changed, and the other two changed files are the acknowledgement row and
a test-file flake this run had to fix to get the gate green.

The numbers below are `git diff HEAD --numstat` — this run's working tree.

| File | Lines | What |
|---|---|---|
| `src/repl/editor.rs` | +160 −0 | `LineContext` and `context_at`: the question both editor hooks now ask before they answer anything. `signature_hint` refuses a `(` inside a text literal or a comment; `word_at` refuses a word in one, so TAB in a comment no longer eats the reader's prose. Two tests. |
| `src/repl/mod.rs` | +71 −0 | `edge_doc_and_completion_agree_about_which_names_are_modules`, pinning that `.doc` and the completer answer "is that a module" about the same names in both directions. One test. |
| `tests/repl_color_test.rs` | +35 −15 | The nine environment-touching tests serialised on one lock, `COLUMNS_LOCK`/`pin_columns` renamed `ENV_LOCK`/`pin_env` to match what it now guards. No assertion changed. |
| `tests/unreviewed_work_ack_test.rs` | +1 −1 | The acknowledgement row for this phase's resume commit, swapped from `0e84349` to `373513e`. Data only; no assertion touched. |
| `phases/phase-061/REPORT.md` | — | This file. |

### The phase's own implementation, for the record

That work is `373513e` ("resume phase-061: adopt preserved work"), +2247 −22
across 7 files, of which +1907 −22 is under `src/`. It is not rewritten here and
not described as if it were this run's. In one line each: `src/repl/mod.rs` has
`ScratchDir`/`private_scratch_dir`/`read_edited_block`/`edit_with` for `.edit`,
`advance_tutorial_step` for the tour, and the `.doc`/completion/sighelp surface;
`src/repl/completer.rs` has `complete_dotted`/`split_dotted` and the bare-dot
case; `src/repl/editor.rs` has `signature_hint` and the `Hinter` wiring;
`src/stdlib.rs` has `module_members`; `src/repl/commands.rs` has the three
command rows. Its 29 tests are listed in the next section.

### Two decisions in this run's diff worth checking

**The hooks ask where the cursor is before they answer.** `signature_hint` and
`word_at` both looked only backwards — for a `(` and a name, for the last space —
and neither knew whether the text it had found was Redblue. So `say "split(`, a
reader typing a string that happens to end in a call's name and a bracket, was
shown a signature in the middle of their own text, and `say 1 // json.` plus TAB
replaced the word in their comment with `json.parse`. The doc comment on
`signature_hint` already *claimed* the opposite ("or mid-string, where a hint
would be text appearing in the middle of what they are writing"), which is what
made it a defect rather than a design choice.

**`context_at` is a scan, not a parse.** It mirrors `Lexer::read_text` and the
lexer's `//` rule rather than inventing a second dialect, because a second
dialect is how the two drift. It cannot use the lexer itself because the line is
a *half-typed* one — `say "split(` is exactly the line a reader is looking at
when they should be looking at a hint, and `Lexer::tokenize` refuses it as an
unterminated string. A parse that could not read the half-typed line would
answer none of the cases this exists for. The test that pins the escapes
(`\"` does not close, `\\` is not an escape of the quote after it, `//` inside a
literal is text) is what holds the scan to the lexer's rules rather than to
plausible ones.

### Five decisions in `373513e` worth checking

**The tour advances by running, not by asking.** `advance_tutorial_step` is
called from `execute_code_from` after a run that worked, and nowhere else. A step
is taken when what the session has run since the step began *is* the step's
snippet: a prefix of it means the reader is part-way through and is still on the
step, anything else means this was something of their own and the next attempt
starts from the beginning. That is why `tutorial_typed` exists — `set name to
"World"` and `say name` are each a whole program, so neither run alone is step
one and only the two together are. Only successful runs advance it, so a step
typed with a typo in it is a step not yet taken, and the last step leaves no step
behind it.

**The scratch directory is removed in `Drop`, not on each path.** An edit has
more ways out than the one it succeeds by: the editor is not there, it exits
badly, its save cannot be read back, the save is not text, the block is past the
limit, the run fails. A `remove_dir_all` written at the end of each is a list to
keep in step with the paths above it, and a list is what grows a new path and
forgets it. `ScratchDir` is *owned* rather than borrowed, so holding it is what
makes the directory exist and dropping it is what ends it.

**A name is skipped, not reused.** `/tmp` is shared, so `create_dir` refusing a
planted name is a search rather than a failure: serials 0 to 63, and a report
naming the range when every one of them is taken. The planted directory is never
entered, so nothing of the session's is written inside it — asserted, with a
`decoy.rb` in the planted directory and a real edit succeeding in the next
serial.

**The block limit is on the read, not on the answer.** Capping after
`read_to_string` would leave the allocation the cap exists to prevent, so the
read is taken at `MAX_BLOCK_BYTES + 1` and that one byte over is what says "there
was more than the limit". A block of exactly the limit still runs — the same
boundary the typed path draws at `>`.

**The editor is a parameter, not the environment.** `edit_in(&self, editor:
Option<&str>, scratch_dir: &Path)` takes the program rather than reading
`$EDITOR` itself, because the environment is process-wide and a test cannot own
it. `edit_with` wraps it and is what `.edit` and the tests drive, so what is
asserted afterwards — the block that ran, the buffer that came back, the
directory that is gone — is asserted about the session's own state. The round
trip is pinned with no real terminal editor, no TTY, and no test's `$EDITOR`
reaching another's.

## Tests added

32 new `#[test]` functions across the phase, 25 of them named `edge_*`: 29 in
`373513e` (the phase's own implementation) and 3 in this run's working tree. All
live beside the code they pin.

### In this run's working tree (3, all `edge_*`)

| Test | Edge class covered |
|---|---|
| `repl::editor::tests::edge_a_call_written_as_text_is_not_a_call_being_made` | **the defect** — a `(` and a name inside a `"` literal show no signature (`say "split(`, `say "json.parse(`), a `(` in a comment shows none, and TAB in a comment or a literal offers nothing rather than replacing the word. The last two assertions are the guard against over-reach: a string that has *closed* puts the rest of the line back in code, so `say "a split(" + split(` still shows the signature and `say "a" + json.` still completes `json.parse`/`json.stringify` |
| `repl::editor::tests::edge_an_escaped_quote_does_not_close_a_text_and_a_slash_slash_inside_one_is_text` | **malformed input** — `\"` leaves the literal open, `\\` is not an escape of the quote after it, and `//` inside a literal is text: `say "http://x" + split(` is a *closed* string followed by a real call, not a string followed by a comment. This is what holds `context_at` to `Lexer::read_text`'s rules rather than to plausible ones |
| `repl::tests::introspection::edge_doc_and_completion_agree_about_which_names_are_modules` | **registry drift, both directions, every module** — for every name in `stdlib::MODULES`, `.doc <m>` answers (asserted with `unwrap_or_else(|e| panic!(…))` on the `Result`) and `<m>.` offers members that the `.doc` listing actually mentions. The refusal direction is the `is_err()` half: `nosuchmodule`, `JSON` and `json_parse` are refused by `.doc` *and* offer no members. The empty name is split out, because a bare `.` is a different question — which receivers exist — and is asserted as bare names |

### In `373513e` (29, of which 22 are `edge_*`)

### In the very first resume of this phase, `0e84349` (16, of which 9 are `edge_*`)

Those 16 tests were carried forward and strengthened into the 13 listed below;
`0e84349` is no longer an ancestor of HEAD, so nothing in this repository's
history runs them as they were written. They are listed because the report
above that describes them is still the record of what the phase first did.

| Test | Edge class covered |
|---|---|
| `completer::json_dot_offers_the_members_the_registry_holds` | **the finding** — `json.` offers exactly `json.parse`/`json.stringify`; `json.s` narrows to one |
| `completer::edge_every_module_member_offered_is_bound_and_every_bound_one_is_offered` | **both directions, every module** — offered iff `builtins()` binds it behind that spelling |
| `completer::edge_a_module_that_is_not_one_offers_nothing_and_does_not_error` | `nosuchmodule.` → `[]`, no error |
| `editor::typing_an_open_bracket_after_a_callable_surfaces_its_shape` | the separator form of `split`, and `json.parse` found under `json_parse` |
| `editor::edge_a_bracket_after_something_that_is_not_callable_shows_nothing` | `say (`, a cursor inside the name, and one past the end of the line |
| `editor::edge_members_and_paths_do_not_fight_over_one_tab` | **the DoD's gating row** — `json.` → members, `:load s` → paths, bare `.` → modules, each on its own context |
| `mod::onboarding::a_tutorial_is_a_numbered_tour_that_ends_at_examples` | every step numbered and named; the last names `:examples` and `:hint`; a tour of one step is refused by the assertion |
| `mod::onboarding::every_tutorial_snippet_is_a_program_the_session_runs` | **the DoD's row** — each of the 6 snippets runs through the session or the test fails with the step number |
| `mod::onboarding::edge_tutorial_with_colour_off_numbers_its_steps_and_emits_no_escapes` | colour forced off — numbering intact, no escape |
| `mod::onboarding::a_hint_outside_the_tour_names_the_tour` | **failure-shaped** — the answer is not blank |
| `mod::onboarding::a_hint_in_the_tour_names_the_step_it_is_for` | step 1 of 6 |
| `mod::onboarding::edit_runs_what_the_editor_left_behind` | **the DoD's row** — a script editor's text comes back, runs, and binds `edited` |
| `mod::onboarding::edge_edit_without_an_editor_names_the_variable_and_keeps_the_buffer` | **`EDITOR` unset** — the message names the variable, and the session buffer survives |
| `mod::onboarding::edge_an_editor_that_cannot_be_used_is_reported_rather_than_run` | a program that does not exist, and one that exits non-zero |
| `mod::onboarding::edge_the_onboarding_commands_are_reachable_every_way` | both sigils, `:tour` alias, TAB offering each, `:help` naming all three |
| `mod::onboarding::edge_a_painted_hint_keeps_the_hint_readable` | colour on — the painted hint is the plain hint painted, and nothing else |

### In the working tree of the run before this one (13, all `edge_*`)

| Test | Edge class covered |
|---|---|
| `mod::onboarding::edge_every_step_of_the_tour_can_be_walked_by_typing_it` | **the gap the old report shipped as a known one** — all six steps typed line by line, ending at no step at all |
| `mod::onboarding::edge_the_last_step_taken_is_the_end_of_the_tour` | the last step leaves no step behind it, and the answer then names `:examples` |
| `mod::onboarding::edge_only_the_step_its_own_snippet_moves_the_tour_on` | foreign code, an abandoned half-snippet, a stray line after it, and a run that failed — none of them skips a step |
| `mod::onboarding::edge_a_reset_forgets_the_tour_it_was_walked_through` | `:reset` mid-step clears the step *and* the half-typed snippet |
| `mod::onboarding::edge_the_scratch_file_is_readable_by_its_owner_and_nobody_else` | the editor is handed a `0600` file in a `0700` directory (Linux; the script uses GNU `stat -c`) |
| `mod::onboarding::edge_a_scratch_path_already_taken_is_refused_rather_than_followed` | a planted `redblue-edit-<pid>-0` is skipped for serial 1, never written into, and left as it was found |
| `mod::onboarding::edge_every_scratch_name_taken_is_reported_as_the_range_it_tried` | all 64 serials planted — the error names both ends of the search and nothing was written |
| `mod::onboarding::edge_an_edit_that_does_not_run_gives_the_block_back` | a block the editor saved that does not parse comes back into the buffer, still open, and finishes by hand |
| `mod::onboarding::edge_a_failed_edit_leaves_no_copy_of_the_buffer_behind` | a spawn failure and a non-zero exit each leave the directory empty |
| `mod::onboarding::edge_a_scratch_directory_is_taken_with_it_however_the_edit_ended` | the guard itself, given up on part-way; and a real read-back failure (the editor saves a directory) leaving nothing behind |
| `mod::onboarding::edge_a_block_the_editor_saved_past_the_session_limit_is_refused` | **both sides of both limits** — 500 lines and 65 536 bytes run, 501 lines and 65 537 bytes are refused unrun |
| `mod::onboarding::edge_an_editor_that_names_nothing_is_refused_rather_than_spawned` | `$EDITOR` holding `""`, `"   "` or `"\t "` is refused by name, and nothing is written |
| `mod::onboarding::edge_a_save_that_is_not_text_is_refused_rather_than_parsed` | a non-UTF-8 save is reported as not text rather than as an I/O failure |

Eight further tests from `373513e`'s predecessor were **changed** rather than
added, and each change is a strengthening: `a_hint_in_the_tour_names_the_step_it
_is_for` now walks step one and asserts step two's hint;
`edge_a_bracket_after_something_that_is_not_callable_shows_nothing` asserts the
boundary it names; `edge_members_and_paths_do_not_fight_over_one_tab` sorts what
it compares; `edit_runs_what_the_editor_left_behind`,
`edge_edit_without_an_editor_names_the_variable_and_keeps_the_buffer` and
`edge_an_editor_that_cannot_be_used_is_reported_rather_than_run` are driven
through `edit_with`, so they assert the session's own state rather than a scratch
file's; and the two `complete_*` tests unwrap the `Result` `execute_code` now
returns.

In **this** run no existing test changed what it asserts. The two pre-existing
test files touched are the acknowledgement list (one data row swapped) and
`tests/repl_color_test.rs` (a lock added to nine tests, no assertion changed).

**A failure is asserted**, not only successes. This run's
`edge_doc_and_completion_agree_about_which_names_are_modules` is the clearest:
`Repl::module_doc_answer` returns `Result<Vec<String>, String>`, and the test
asserts `is_err()` for `nosuchmodule`, `JSON` and `json_parse` while asserting
the completer offers those same names no members — a clean refusal in both
places rather than an answer for one. Across the phase as a whole,
`edge_edit_without_an_editor_names_the_variable_and_keeps_the_buffer` and
`edge_an_editor_that_cannot_be_used_is_reported_rather_than_run` cover the
failure channel of `.edit` in both directions.

No test was weakened, skipped, ignored or suppressed. The phase's diff adds no
`#[ignore]`, no `// skip` and no `allow(clippy::`. The only pre-existing test
file touched is the acknowledgement list, which gained one data row and had one
number in that row corrected.

## Gates

Run at the end of this run, against this working tree.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no output |
| `cargo clippy --all-targets -- -D warnings` | pass — `Finished dev profile`, no warning or error |
| `cargo test --all-targets` | pass — **1868 passed; 0 failed; 0 ignored; 0 filtered out**, exit 0, across 67 test binaries (the sum of every `test result:` line in the run) |
| `../rbops/verify.sh phase-061` | pass — `VERIFY PASS — phase-061`, including `cargo test: 1868 passed, 0 failed`, `test quota met: 3 rust / 0 redblue`, `edge case test present: 3`, `failure-asserting test present: 2`, `no newly skipped tests`, `no gate-weakening constructs added`, `implementation touched src/ (must_touch: src/)`, and `report honesty: reported test count (1868) matches actual (1868)` |

One note on that last line, so a reader who sees the run is not misled: while
this report is still uncommitted, `verify.sh`'s honesty check prints

```
WARN  report claims 1865 passing, actual run has 1868 — under-claimed, fix the number
```

because that one check reads `phases/phase-061/REPORT.md` **out of git** —
`git show HEAD:phases/phase-061/REPORT.md` is where `1865` still lives — while
every other check in the script reads the working tree (the diff-hygiene line
correctly reported `+267 -16` from the uncommitted work). The table above is the
working-tree file and says `1868`; the warning disappears the moment the pipeline
commits this run. Nothing was adjusted to silence it.

## Invariants touched

- **None.** No `Value` variant, no `Error` variant, no grammar change, no change
  to `to … end` / `set x to` / `say`, no change to the `.rb` extension.
- The REPL gained three commands (`:tutorial`/`:tour`, `:hint`, `.edit`) and TAB
  gained one new answer (members after a module dot, and module names after a
  bare `.`). No existing command changed meaning: `:example`/`:examples`,
  `:load`, `:save` and every other path-taking command are untouched, and
  `edge_members_and_paths_do_not_fight_over_one_tab` pins that the two kinds of
  completion do not answer for one another.
- Two behaviours changed for the better rather than in name only, both of which
  the diff's own tests pin:
  - **`.edit` is now bounded and private.** It writes
    `<temp>/redblue-edit-<pid>-<serial>/redblue-edit.rb` in a `0700` directory
    instead of a predictable file directly in a world-writable `/tmp`; it removes
    that directory on every way out; and it refuses a saved block past the same
    500-line / 64 KiB limit a typed block is held to.
  - **`:reset` now forgets the tour.** A reset is documented at
    `reset_session` as "a reset forgets what it was"; where it had got to in a
    tour is part of that.
  - **This run: TAB and the `(` hint stopped answering inside a `"` literal and
    inside a `//` comment.** The hint was drawn in the middle of a reader's own
    string, and TAB replaced a word in their comment. The suppression is scoped to
    the head of the line *before the cursor*, so everything after a closed
    string or after a `//` that was itself inside a string is still code and is
    still answered — `say "a" + json.` completes `json.parse` in both the old and
    the new code, and `edge_a_call_written_as_text_is_not_a_call_being_made`
    asserts it in the new one.
- `examples/*.rb` and `modules/*.rb` are unaffected — this phase changes what the
  REPL *offers* and *runs*, not what the language *means*.

## Edge-case matrix (AGENTS.md §3.2)

| Row | Status | Where |
|---|---|---|
| empty | covered | `edge_a_module_that_is_not_one_offers_nothing_and_does_not_error` (`nosuchmodule.` offers nothing); `edge_an_editor_that_cannot_be_used_is_reported_rather_than_run` — an editor that empties the file gives "nothing to run"; `edge_an_editor_that_names_nothing_is_refused_rather_than_spawned` — `""`, `"   "` and `"\t "` are refused by name, and nothing is written; `edge_doc_and_completion_agree_about_which_names_are_modules` — the empty name is refused by `.doc` and, separately, a bare `.` offers only bare receiver names |
| singleton | covered | `json_dot_offers_the_members_the_registry_holds` — `json.s` narrows a two-member module to exactly one; `a_hint_in_the_tour_names_the_step_it_is_for` — step 1 of 6; `edge_doc_and_completion_agree_about_which_names_are_modules` — every offered member must appear in that module's `.doc` listing, so a one-member module is checked as tightly as a many-member one |
| boundary | covered | `edge_members_and_paths_do_not_fight_over_one_tab` — the bare `.`, the boundary between "no dot" and "dotted", is decided before either branch; `edge_a_bracket_after_something_that_is_not_callable_shows_nothing` — `say (` is a `(` with no name behind it; `edge_a_block_the_editor_saved_past_the_session_limit_is_refused` — a block of exactly `MAX_BLOCK_LINES` lines and one of exactly `MAX_BLOCK_BYTES` bytes still runs, and one line / one byte more does not, on both limits; **this run** — `edge_a_call_written_as_text_is_not_a_call_being_made` — the boundary is the closing `"`: one quote in, the rest of the line is text; the second, and the call after it is answered again |
| out_of_bounds | covered | `edge_a_bracket_after_something_that_is_not_callable_shows_nothing` — a cursor **past the end of the line** (`signature_hint("say split(", 999)`) is `None` rather than a slice panic, and the end of the line (`signature_hint("say split(", 10)`) *does* answer, because a cursor at the end of `say split(` is after the bracket |
| type_mismatch | covered | `edge_a_bracket_after_something_that_is_not_callable_shows_nothing` — `(` after something that is not callable shows nothing rather than a signature; `edge_edit_without_an_editor_names_the_variable_and_keeps_the_buffer` — `$EDITOR` absent is a distinct outcome from a present-but-unrunnable one; **this run** — a `"` where a name is expected is text rather than a call, and a receiver that is not a module offers members on neither path |
| numeric_boundary | **N/A** | This phase parses no number. It completes words, edits a buffer, and prints text. The nearest arithmetic is `index + 1` and `TUTORIAL.len()` in a message, bounded by the table's own length, and the two block limits are `usize` comparisons against the session's own constants; the language's numeric boundaries are covered by `tests/numeric_edge_test.rs`. `context_at` counts no numbers at all — it walks `char`s and compares two of them. |
| unicode | covered | The tour's own text carries `…` (step 3, "`if … then … else … end`") and `"Hello, "` (step 5); `edge_tutorial_with_colour_off_numbers_its_steps_and_emits_no_escapes` asserts those survive into the printed lines with no escape left in them; `edge_a_bracket_after_something_that_is_not_callable_shows_nothing` — `say é(` at position 5 (inside the two-byte character), 6 (before the bracket) and 7 (after it, refused because `é` is not a builtin); **this run** — `context_at` walks `chars()` rather than bytes, so a `"` after a multi-byte character still opens and closes a literal; the scan never slices at an offset it did not produce from `char_indices`-compatible iteration |
| nesting_recursion | covered | `every_tutorial_snippet_is_a_program_the_session_runs` runs step 3 (`if/else/end` with both branches) and step 4 (`repeat 3 times` / `end`) through the real session, so the nesting a newcomer is shown first is the nesting the interpreter accepts |
| duplicate_missing_keys | **N/A** | No record is read or written by anything here. The completer answers a word, `:tutorial` prints a table, `.edit` reads and writes one scratch file. The nearest properties — a word with no module behind it, and a member prefix matching nothing — are covered by `edge_a_module_that_is_not_one_offers_nothing_and_does_not_error`, by the `None` case of `complete_dotted`, and, **this run**, by `edge_doc_and_completion_agree_about_which_names_are_modules` asserting that a *missing* module is missing from both the listing and the completion |
| malformed_input | covered | `edge_an_editor_that_cannot_be_used_is_reported_rather_than_run` — `$EDITOR` naming a program that does not exist, and naming one that exits non-zero; both are reported and neither is run. `edge_an_editor_that_names_nothing_is_refused_rather_than_spawned` — `editor.split_whitespace()` yielding no program at all, driven through `edit_in` directly because `editor_program` filters the blank away before `.edit` can see it. `edge_a_save_that_is_not_text_is_refused_rather_than_parsed` — a non-UTF-8 save. **This run**: `edge_an_escaped_quote_does_not_close_a_text_and_a_slash_slash_inside_one_is_text` — an escaped quote, an escaped backslash, and a `//` that is inside a literal rather than before one |
| resource_limit | covered | `.edit` writes exactly one file, `<temp>/redblue-edit-<pid>-<serial>/redblue-edit.rb`, in a `0700` directory of its own, and that **directory** is removed on every way out of the edit: `edge_a_failed_edit_leaves_no_copy_of_the_buffer_behind` (a spawn failure and a non-zero exit), `edge_a_scratch_directory_is_taken_with_it_however_the_edit_ended` (an early `?` and a read-back failure). `edge_a_block_the_editor_saved_past_the_session_limit_is_refused` — the session's own 500-line / 64 KiB block limit applies to what an editor saved, and it is applied to the read, so the bytes past it are never in memory. `edge_the_scratch_file_is_readable_by_its_owner_and_nobody_else` — the file is `0600` and the directory `0700` for as long as the editor is open. **This run**: `context_at` allocates nothing and allocates no more than the caller already holds — it walks `head.chars()` in place — so the line editor cannot be made to allocate by a keystroke; the one place that does allocate, the saved block, was capped in `373513e` and is re-asserted by `edge_a_block_the_editor_saved_past_the_session_limit_is_refused` |

## Known gaps / follow-ups

- **`.edit` shells out with no argument vector limit and no timeout.** A hung
  editor holds the session. That is what every editor integration does, and
  bounding it would need a policy (signal? timeout? what to do with the buffer?)
  that is a design decision rather than a bug fix. Not fixed here.
- **`.edit` writes to `std::env::temp_dir()`,** while the tests pass
  `target/tmp/` explicitly. Under a restrictive `TMPDIR` the command reports the
  failure cleanly rather than silently losing the buffer, so this is an
  inconvenience, not a defect.
- **`.edit` writes to a scratch file, not to a real path,** so a session that
  started with `:load something.rb` and then edited loses the connection to the
  file it came from. Reconnecting `.edit` to the loaded file is not in the DoD.
- **`:hint` tracks the step the tour is on, not how far the reader has got in
  general.** A step advances when its snippet is typed, so a reader who works
  through the tour out of order — or who has already written the programs in
  their own words — is still asked for the snippet of step one. Wiring `:hint`
  to the session's bindings rather than to the printed snippets would be a
  different design, and a larger one than this phase's DoD asks for.
- The completion half of TAB needs a TTY and is not covered end-to-end through
  the `rb` binary; it is covered through `EditSource::complete` /
  `EditSource::signature_hint`, which is the same code the terminal calls.
- **The `(` hint covers the registry, not the session's own functions.**
  `signature_hint` resolves the name through `stdlib::builtin_signature`, so
  typing `double(` after `to double(n) … end` shows nothing inline even though
  `:sighelp double` answers with the declared parameters and arity. The shape
  exists (`Repl::sighelp_answer` proves it) but reaching it from the editor needs
  the session to publish a second snapshot alongside `SessionNames`, and
  `session_loop` takes that snapshot as a parameter — so it is a signature change
  across every caller rather than a fix. Not this phase's DoD, and a larger change
  than a resume should smuggle in.
- **A member call on a name the session has shadowed is still completed.**
  `set json to 5` binds `json` as a value, and `json.` will then offer
  `json.parse`/`json.stringify` even though the analyzer would read `json` as the
  number. `complete_with` already receives the session's `names` and could
  withhold members for a receiver that is one of them, but `.which json` and the
  completer would then disagree in the other direction, so the right answer is a
  decision about which of them is wrong. Recorded rather than guessed at.
- **`context_at` reads one line, not a whole block.** TAB and the hint are
  per-keystroke on the line being typed, so a `/* */`-style block comment or a
  string opened on an earlier line of a multi-line block is not visible to it.
  Redblue has no block comment, and a text literal does not close on a newline
  only when it is unterminated — which is a line the reader is still typing.
  Reached through the editor only, never through `execute_line`.

## Re-verification of the phase's original finding

The prompt says to re-verify before changing anything and to stop if it no longer
reproduces. It no longer reproduces **in its stated form** — member completion,
`.edit`, `:tutorial`, `.hint` and the `(` signature hook are all present and
demonstrated above, with tests that pin each. That was checked by running the
binary, not by reading the diff, and the outputs are in "State of the phase when
this run started" above.

What *did* reproduce, in this run, was three things, and this run fixed all three
rather than inventing a change to cover work already done:

1. the phase's resume commit was not on the acknowledgement list, and the row
   that was on it named a commit this history does not contain — a swap, not an
   addition;
2. `tests/repl_color_test.rs` had a flake the full suite found and isolation did
   not, because its lock covered the two tests that *wrote* the environment and
   not the five that read it;
3. reading the phase's own two editor hooks against their doc comments turned up
   a real defect the phase introduced and no test covered: both hooks answered
   the whole line without asking whether the text they had found was Redblue, so
   the `(` hint was drawn inside a reader's own string literal and TAB ate a word
   out of a comment.

The first is a gate failure, the second is a gate flake, and the third is a
behavioural defect in the phase's own feature. All three are fixed, each with a
test that fails without the fix, and none of the three required weakening
anything.