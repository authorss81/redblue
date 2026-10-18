# Phase 060 — Export sessions and complete file paths: `.export` plus TAB for `:load`/`:save`

## State of the phase when this run started

The phase's work was **already committed** in `b5461ae` ("resume phase-060: adopt
preserved work"), which carries `src/repl/transcript.rs` (new), the path half of
`src/repl/completer.rs`, `ReplCommand::Export` in `src/repl/commands.rs`, the
wiring in `src/repl/mod.rs`, `strip_escapes` in `src/repl/style.rs` and
`tests/repl_test.rs`. This run did not re-derive any of it. It:

1. ran the four gates first, as instructed,
2. fixed the two pre-existing gate breaks they reported,
3. added the three edge-case tests the definition of done leaves room for,
4. re-ran every gate and wrote this report against what they printed.

**The finding no longer reproduces, because the feature is in the tree.** Checked
on this tree, not taken on trust:

```
$ grep -n 'Export' src/repl/commands.rs | head -3
29:    Export(Option<String>),
243:        build: |argument| ReplCommand::Export(argument),

$ printf 'say "one"\nsay "two"\n.export\n:quit\n' | ./target/debug/rb
>>> # Redblue session

```redblue
say "one"
```
one

```redblue
say "two"
```
two
>>> Goodbye!

$ printf 'say "one"\n.export /no-such-dir-x/session.md\n:quit\n' | ./target/debug/rb
>>> one
>>> Could not export session to '/no-such-dir-x/session.md': No such file or directory (os error 2)
```

The "before" side of the finding (no `.export` in `commands.rs`; a completer at
`completer.rs:297-302` that handled `:`/`.` sigils and nothing else) was verified
by the attempt whose commit this tree carries; it cannot be re-run here without
reverting the phase, so it is reported as that run recorded it and not re-verified
by this one. Nothing in this report is a claim about behaviour this run did not
watch happen.

## What changed

`must_touch: ["src/"]` is satisfied: **six files under `src/`**, one of them new —
**+1450 −10** under `src/`. Over the whole phase (`git diff 222aa83..HEAD` plus the
working tree) it is **+2388 −26** over 11 files.

| File | Lines | What |
|---|---|---|
| `src/repl/transcript.rs` | +560 (new) | The document. `Exchange` (what was typed, what it said) and `Transcript` (the exchanges, bounded at `LIMIT` = 500, oldest dropped). `markdown()` writes the `redblue` fence and the text after it and strips escapes on the way out; `read_markdown()` is its other half. `fence_width`/`fence_language` widen a fence around an input that prints one. 13 tests. |
| `src/repl/completer.rs` | +447 −1 | `path_argument` (where the cursor is, when it is in a path argument), `split_fragment`, `complete_path_in` (a named directory rather than the process's, so a test can use a scratch one), `sort_key`, `quote_if_spaced`, `is_directory` (follows a symlink), `current_dir`, `ReplCompleter::complete_path`. `MAX_PATH_CANDIDATES` = 200. 8 tests. |
| `src/repl/commands.rs` | +181 −6 | `ReplCommand::Export(Option<String>)` and its row in `COMMAND_TABLE`; `FILE_USAGE`, the `<filename>` placeholder that says which commands take a path; `ReplCommand::takes_a_file_path`; `ReplCommand::unquote_path`. 2 tests, plus rows in `parse_reads_the_whole_alias_table`. |
| `src/repl/mod.rs` | +82 −3 | `Repl::transcript` and `Repl::last_output`; `execute_code_from` records one exchange per line run; `run_program` keeps what the VM said before printing it on the error path; `export_session`; the `ReplCommand::Export` arm; `:reset` clears the transcript with the bindings. |
| `src/repl/style.rs` | +106 | `strip_escapes` — `ESC [ … @…~` and a bare escape byte, walking chars rather than bytes. 3 tests. |
| `src/repl/editor.rs` | +74 | `SessionCompleter::complete` asks for a path first and falls back to the vocabulary when the cursor is not in one. 2 tests. |
| `tests/repl_test.rs` | +419 | End to end through the `rb` binary: 8 tests, with the extraction helpers (`plain`, `exported_document`, `fenced_blocks`). |
| `tests/memory_model_test.rs` | +17 −13 | **Not this phase's code.** Pre-existing gate break, fix below. |
| `tests/unreviewed_work_ack_test.rs` | +146 −3 | **Not this phase's code.** Pre-existing gate break, fix below. |

### Two decisions in the diff worth checking

**Where the output text comes from.** The VM prints `say` output itself
(`Vm::run`), and the session printed the `= value` line and the rendered failure
itself, so no one place held a line's output as text. `run_program` now takes the
VM's buffer into `last_output` before printing it on the error path, and
`execute_code_from` assembles an exchange's output from that plus the `=` line
plus the rendered failure. Both are stored already stripped, and `markdown()`
strips again, so a future painted string recorded here still cannot leak into a
document.

**Why `FILE_USAGE` rather than a list of commands.** `takes_a_file_path` asks the
command table whether the entry's usage is the `<filename>` placeholder. One
answer to "which commands take a path" instead of two, so a sixth such command
says so by declaring the same thing — and it is what
`edge_only_the_commands_that_take_a_path_take_a_path` pins, including the half
that is easy to forget: a sigil is required, because `load x` with no sigil is
Redblue source calling a function called `load`, not a command. `.export` is in
that set because its argument is a filename by the same declaration as the four
the DoD names; the predicate, not a list of four, is what decides.

### Two pre-existing gate breaks, fixed here

Both were red on `main` before this run touched anything, and both were in
`tests/`, from commits belonging to other phases. Neither is weakened by the fix.

| File | Symptom | Fix |
|---|---|---|
| `tests/unreviewed_work_ack_test.rs` | `cargo fmt --check` reported a diff, and `clippy -D warnings` rejected the body as `assert!(true)` — an `assertions_on_constants` error, so the file did not compile under the gate | The placeholder (introduced by `222aa83`, "replace invalid placeholder causing compile failure") became three tests that read `tests/checkpoint_ancestry_ack.txt` with no `git`: the list is non-empty, every entry has a non-empty sha and a non-empty reason, no sha is listed twice, and `wip_merges_on_main=` is pinned and parses |
| `tests/memory_model_test.rs` | `clippy -D warnings` rejected `Arc::new(Node { .. })` as `arc_with_non_send_sync` — `Node` holds a `RefCell` and is neither `Send` nor `Sync`, so this was a compile error under the gate | The node helper holds `Rc<Node>` (`Rc::clone`, `Rc::strong_count`, `Rc::downgrade`). No `#[allow]` was added, and the four assertions that depend on the handle's behaviour — strong counts of 2, a weak count of 1 per survivor, the `Weak` string check — are unchanged. The `Arc` uses that *are* `Send` (`Arc<()>`, `Arc<Vec<Value>>`) are untouched |

Both fixes are `#[allow]`-free and assertion-preserving. Recorded as **F4** in
`FINDINGS.md`: a phase can leave `main` with a red gate, and the next phase pays
for it without being told.

## Tests added

**38 new `#[test]` functions, 27 of them named `edge_*`**, plus 4 rows added to the
existing `parse_reads_the_whole_alias_table`.

| Test | Edge class covered |
|---|---|
| `transcript::an_exported_session_is_markdown_with_one_redblue_block_per_line` | the DoD's first bullet: one `redblue` fence per line run, each holding what was typed, each output as a text block after it |
| `repl_test::edge_export_writes_a_two_line_session_as_two_fenced_markdown_blocks` | the same through the binary: exactly two ` ```redblue ` fences, the whole document line for line, no `ESC`, no prompt |
| `transcript::a_session_read_back_from_its_own_markdown_is_the_session` | round trip, unit half — write then read, no terminal |
| `repl_test::edge_a_session_exported_to_a_file_runs_again_from_the_fences_read_back` | the DoD's round trip end to end: export → file → fences read back → a fresh session says `one` and `two` once each |
| `transcript::edge_no_escape_sequence_can_reach_the_document` | escapes — a coloured result and a coloured failure reach the document with the text intact and no `ESC` |
| `transcript::edge_a_line_that_prints_a_fence_is_written_inside_a_longer_one` | escapes / malformed — `say "```"` closes the block it is written inside unless the fence is widened; the widened block still reads back |
| `transcript::a_document_written_by_hand_is_read_the_same_way` | malformed — a `rust` block is not read, a widened block is, text outside a block is not, a fence inside a widened block is content |
| `transcript::edge_an_unclosed_fence_at_the_end_of_a_document_reads_as_nothing` | malformed — a truncated export does not read back as half a line of Redblue |
| `transcript::edge_a_session_that_has_run_nothing_says_so` | empty — a heading and a sentence, no fence, nothing to read back |
| `repl_test::edge_export_of_a_session_that_has_run_nothing_says_so` | empty, through the binary |
| `transcript::an_exchange_that_said_nothing_writes_no_output_block` | boundary — a `set` followed by a `say`, the whole document asserted exactly |
| `transcript::edge_the_fence_is_one_backtick_longer_than_the_longest_run_inside_it` | boundary — 0, 1, 2, 3 backticks and two separate runs |
| `transcript::edge_a_long_session_keeps_the_newest_exchanges_and_no_more` | resource — 505 exchanges in, 500 out, the oldest gone, the document bounded with it |
| `transcript::a_block_is_one_exchange_holding_every_line_it_was_typed_on` | nesting — a block is one exchange, indentation and all |
| `transcript::an_exchange_keeps_its_input_apart_from_its_output` | the `Exchange` pair the renderer reads apart |
| `transcript::edge_a_cleared_transcript_exports_an_empty_session` | state — `:reset` clears the record |
| `completer::a_file_command_completes_the_paths_it_could_load` | the finding — `sr` → `src/`, `no` → `notes.txt`, `zzz` → nothing, in a scratch directory |
| `completer::an_empty_fragment_offers_the_directory_and_replaces_only_the_argument` | empty / boundary — `:load ` at 6, `:load src/li` at 12, `.save out.txt` at 13: the offset the replacement starts at, not just the candidates |
| `completer::edge_a_filename_with_a_space_in_it_is_offered_quoted` | the DoD's "never split" — `my file.rb` is offered `"my file.rb"` |
| `repl_test::edge_a_quoted_path_with_a_space_in_it_is_the_file_it_names` | and the form TAB offers is a form `:load` opens |
| `completer::edge_nothing_to_offer_is_no_candidates_rather_than_a_failure` | empty / out of bounds / malformed — an empty directory, a prefix nothing matches, a directory that does not exist, a path that is a file, `pos` past the end of the line, `pos` inside a character |
| `completer::edge_a_dot_entry_is_not_offered` | state — `.hidden` is not offered and the file is still there |
| `completer::edge_a_long_listing_is_capped_and_the_order_does_not_depend_on_the_disk` | resource / determinism — 210 entries, 200 offered, the same list twice, the cut is the first of the sorted list rather than readdir order |
| `completer::a_symlink_to_a_directory_is_offered_as_a_directory` | resource — the link is followed, so the next TAB walks into it (`#[cfg(unix)]`) |
| `completer::a_path_is_offered_only_in_the_argument_of_a_command_that_takes_one` | the DoD's third bullet as a predicate — twelve lines at given positions, including `:loadx s`, `::load s`, bare `load s` and `say "s` |
| `editor::edge_tab_after_a_command_that_takes_a_path_offers_paths` | the DoD's second bullet through `EditSource`: `:load `, `:load s`, `:load sr` all offer `src/`; `:load ` offers a file; `sr` narrows; `say` and `:save` are *not* offered in the argument |
| `editor::edge_tab_outside_a_file_argument_offers_no_path` | the other side of the gate — `say "a`, `set x to "s`, `sr`, `say "src/` offer nothing |
| `commands::edge_export_is_a_command_whose_path_may_be_left_out` | `.export p` → `Export(Some(..))`, `.export` and `.export␣␣␣` → `Export(None)`, and it is offered by the completer |
| `commands::edge_only_the_commands_that_take_a_path_take_a_path` | the predicate — the five commands and their eight words, and eleven lines that are or are not path arguments |
| `style::stripping_a_painted_string_gives_the_text_back` | all fourteen paint roles round-trip |
| `style::edge_stripping_a_plain_string_changes_nothing` | empty — the empty string, indentation, a tab, a disabled style painting nothing |
| `style::edge_an_escape_that_is_not_a_paint_sequence_leaves_only_text` | escapes — a bare escape byte, `[2J[H`, and the backticks of a fence |
| `repl_test::edge_export_to_a_path_that_cannot_be_written_names_the_path` | **failure**: `target/tmp/no-such-directory-for-export/session.md` → `Could not export … <path>`, and nothing is left at the path |
| **`repl_test::edge_a_session_of_unicode_exports_and_reads_back_unchanged`** | **added this run** — unicode: an emoji outside the BMP, `e`+U+0301 (two characters, not `é`), Arabic, CJK; the input and its output survive verbatim and read back as themselves |
| **`repl_test::edge_export_to_the_same_path_twice_replaces_the_document`** | **added this run** — state: a second export to one path replaces the first rather than appending, and the file holds the second session's two exchanges and none of the first's |
| **`repl_test::edge_a_session_that_ran_a_crlf_file_exports_and_reads_back`** | **added this run** — malformed input: a program written with CRLF is recorded as one block with no `\r` anywhere in the document |
| `tests/unreviewed_work_ack_test.rs::unreviewed_work_ack` + 2 `edge_*` | the pre-existing placeholder, made real (see above) |

**A failure is asserted**, not only successes:
`repl_test::edge_export_to_a_path_that_cannot_be_written_names_the_path` (a clean
error naming the path, and no file created at it),
`completer::edge_nothing_to_offer_is_no_candidates_rather_than_a_failure` (`None`
for a cursor past the end and inside a character),
`style::edge_an_escape_that_is_not_a_paint_sequence_leaves_only_text`,
`unreviewed_work_ack_test::edge_an_empty_acknowledgement_list_is_not_agreement`.

**A test was watched failing for the right reason.** `edge_export_to_the_same_path_twice_replaces_the_document` was run against a deliberately mutated `export_session` that opened the file with `.append(true)` instead of `std::fs::write`:

```
thread 'edge_export_to_the_same_path_twice_replaces_the_document' panicked at tests/repl_test.rs:1669:5:
the second export replaced the first rather than being appended to it (the first wrote "# Redblue session\n\n```redblue\nsay \"first\"\n```\nfirst\n\n")
test result: FAILED. 0 passed; 1 failed
```

The mutation was reverted (`git diff --stat src/` is empty again).

No test was weakened, skipped, ignored or suppressed. `git diff 222aa83` over this
phase contains no `#[ignore]`, no `// skip` and no `allow(clippy::`. No pre-existing
test changed except by gaining rows, except the two files in the table above, which
had to be repaired to compile at all.

Determinism: every colour assertion strips escapes before asserting
(`repl_test::plain`) or compares against `Style::forced(…)`, so changing the
palette cannot fail this suite. Every path-completion unit test runs in a scratch
directory under `target/tmp/` created and emptied by the test itself. The two
editor tests are the exception: they read the checkout deliberately and assert
`contains("src/")` rather than an exact list. No test reads wall-clock time.

## Definition of done

| Item | Where it is verified |
|---|---|
| `.export` to stdout of a two-line session produces Markdown with exactly two ` ```redblue ` fences and the outputs as text blocks, escapes stripped first | `repl_test::edge_export_writes_a_two_line_session_as_two_fenced_markdown_blocks` (whole document asserted line for line, `!document.contains('\u{1b}')`), `transcript::an_exported_session_is_markdown_with_one_redblue_block_per_line`, `transcript::edge_no_escape_sequence_can_reach_the_document` |
| TAB after `:load ` offers files in the current directory; TAB after `:load sr` narrows to `src/` with its trailing separator | `editor::edge_tab_after_a_command_that_takes_a_path_offers_paths` (`:load ` offers `Cargo.toml`; `:load s`, `:load sr` offer `src/`; the narrowed list is shorter than the whole), `completer::a_file_command_completes_the_paths_it_could_load`, `completer::an_empty_fragment_offers_the_directory_and_replaces_only_the_argument` (offsets 6, 12, 13) |
| TAB path completion never escapes the argument — `say "a` offers no files | `editor::edge_tab_outside_a_file_argument_offers_no_path`, `completer::a_path_is_offered_only_in_the_argument_of_a_command_that_takes_one` (12 lines, positive and negative) |
| Round trip: `.export session.md`, a fresh session reads the fenced blocks back and reproduces the outputs, pinned on a fixed two-line session | `repl_test::edge_a_session_exported_to_a_file_runs_again_from_the_fences_read_back` (each line says its word exactly once), `transcript::a_session_read_back_from_its_own_markdown_is_the_session` |
| `edge_*`: export to an unwritable path names the path | `repl_test::edge_export_to_a_path_that_cannot_be_written_names_the_path` |
| `edge_*`: TAB after `:load ` in an empty directory — no candidates, no crash | `completer::edge_nothing_to_offer_is_no_candidates_rather_than_a_failure` (`complete_path_in(&empty, "")` and `("anything")` both empty) |
| `edge_*`: a filename containing a space is offered quoted, never split | `completer::edge_a_filename_with_a_space_in_it_is_offered_quoted`, `repl_test::edge_a_quoted_path_with_a_space_in_it_is_the_file_it_names` |

## Edge-case matrix

| Row | Covered by |
|---|---|
| empty / zero / nothing | `transcript::edge_a_session_that_has_run_nothing_says_so`, `repl_test::edge_export_of_a_session_that_has_run_nothing_says_so` (nothing run, nothing fenced), `completer::edge_nothing_to_offer_…` (an empty directory), `completer::an_empty_fragment_…` (`:load ` offers the whole directory), `style::edge_stripping_a_plain_string_changes_nothing` (the empty string), `transcript::an_exchange_that_said_nothing_writes_no_output_block` (a `set` says nothing, so no output block) |
| singleton and boundary | `transcript::an_exported_session_is_markdown_with_one_redblue_block_per_line` (a two-line session, one block each), `completer::an_empty_fragment_…` (the boundary is the argument start at offsets 6/12/13, not the line), `transcript::edge_the_fence_is_…` (0–3 backticks), `repl_test::edge_a_quoted_path_…` (one spaced name) |
| out of bounds | `completer::edge_nothing_to_offer_…`: `path_argument(":load ", 1000)` is `None` (cursor past the end) and `path_argument(":loa\u{1F600}", 4)` is `None` (inside a character) — `line.get(..pos)` says so rather than a slice panicking. `strip_escapes` walks chars, not bytes; `Transcript` has no indexing at all. Reading a document with no block, or one whose last block is unclosed, is `a_document_written_by_hand_…` and `edge_an_unclosed_fence_…` |
| type mismatch | The path logic is reached by a *command word*, not by a guess: `completer::a_path_is_offered_only_in_…` covers `:loadx s` (no such command), `::load s` (two sigils), bare `load s` (source, not a command) and `say "s` (a string) — none get a path. In the transcript, an input that is a block is not a line and a multi-line rendered failure is not a line either: `a_block_is_one_exchange_…` and the `nosuchvar` transcript |
| numeric boundary | N/A — this change introduces no arithmetic. Nothing in `src/repl/` computes a number from user input: `fence_width` counts backtick runs (`edge_the_fence_is_…` at 0–3), the listing is capped at 200 (`edge_a_long_listing_…`), the transcript at 500 (`edge_a_long_session_…`). The language's numeric boundaries are `tests/numeric_edge_test.rs`, untouched by this phase |
| unicode | `repl_test::edge_a_session_of_unicode_exports_and_reads_back_unchanged` (emoji outside the BMP, `e`+U+0301, Arabic, CJK — input and output verbatim, and reading the fences back gives the same strings); `completer::edge_nothing_to_offer_…` (`pos` inside a multi-byte character returns `None`, not a panic); `style::stripping_a_painted_string_gives_the_text_back` (each of the fourteen paint roles stripped back to the text it wraps) |
| nesting / recursion | `transcript::a_block_is_one_exchange_holding_every_line_it_was_typed_on` (a three-line block is one exchange and reads back as one), `completer::an_empty_fragment_…` (`:load src/li` — a directory inside the fragment is walked, not offered as a name), `completer::a_symlink_to_a_directory_is_offered_as_a_directory` (a link is followed, so nesting is reachable). No recursion limit is needed: `complete_path_in` recurses per TAB, not per keystroke, and `markdown`/`read_markdown` are single passes over a bounded document |
| duplicate and missing keys | N/A — there is no map here. The nearest equivalent is the command table, where a name declared twice would give two answers, and that is what `edge_only_the_commands_that_take_a_path_take_a_path` pins by asserting the set of commands the predicate accepts. The transcript has no keys: an exchange is (input, output) and two identical exchanges are two exchanges (`edge_a_long_session_…` counts them) |
| malformed input | `transcript::a_document_written_by_hand_…` and `edge_an_unclosed_fence_…` (truncated document), `completer::edge_nothing_to_offer_…` (unreadable directory, a path that is a file), `repl_test::edge_a_session_that_ran_a_crlf_file_exports_and_reads_back` (**added this run** — CRLF), `style::edge_an_escape_that_is_not_a_paint_sequence_leaves_only_text` (`[2J[H`, a bare escape byte). A BOM or non-UTF-8 *filename* is skipped rather than offered: `complete_path_in` drops an entry whose name is not UTF-8 (`to_str().map(str::to_string)`), which is the same "offer nothing, do not fail" answer |
| resource limit | `transcript::edge_a_long_session_keeps_the_newest_exchanges_and_no_more` (500 exchanges), `completer::edge_a_long_listing_is_capped_and_the_order_does_not_depend_on_the_disk` (200 candidates, sorted before the cut), `repl_test::edge_export_to_a_path_that_cannot_be_written_names_the_path` (a write that cannot happen is reported, not retried). Every session-test helper kills the child and fails after `SESSION_LIMIT` rather than hanging |

## Gates

Run in this working directory, in this order, on this tree.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no diff |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0, zero warnings |
| `cargo test --all-targets` | **pass** — **1825 passed, 0 failed, 0 ignored**, across 67 test binaries, exit 0 |
| `./rbops/verify.sh phase-060` | **could not be run — the file is not in this checkout** |

The fourth gate is missing, not skipped: `ls rbops` → *No such file or directory*,
and `find . -maxdepth 3 -name verify.sh -not -path './target/*'` returns nothing.
This tree carries `phases/`, `AGENTS.md` and `src/`; the dispatching pipeline
lives elsewhere and `rbops/` was not mounted here. Recorded as **F1** in
`FINDINGS.md` and reported identically by phases 057, 058 and 059. **No claim is
made about what `verify.sh` would have printed.** It was not run and it is not
claimed.

What was run in its place:

| Check | Result |
|---|---|
| `cargo test --doc` | pass — 2 passed, 0 failed |
| every `examples/*.rb` and `modules/*.rb` through `./target/debug/rb run` | 9 of 9 exit 0 |
| `cargo fmt --all -- --check`, `clippy`, `cargo test` before any change | 2 pre-existing failures, both fixed and re-run green (table above) |

## Invariants touched

**None.** No change to the `.rb` extension, to `to … end` / `if … end` / `for … end`,
to `set x to <expr>`, to `say`, to the `Value` variants, to the `Error` variants,
or to the trailing-comma and `{interp}` string syntax. The lexer, parser, analyzer
and VM are untouched by this diff: every changed line under `src/` is in `repl/`.
What is new is a REPL command (`.export`) and where TAB offers paths — neither is
part of the language surface. `:help` gained one line because it is generated from
the command table `.export` is a row of.

## Known gaps / follow-ups

- `rbops/verify.sh` is absent from this checkout, so gate four was neither run nor
  claimed → **F1** in `FINDINGS.md` (third phase to report it).
- Two red gates on `main` before this run, from commits belonging to other phases,
  were fixed here rather than left for the phase that broke them → **F4** in
  `FINDINGS.md`.
- TAB completes the *argument* of a command that takes a path, not a filename
  *inside a program* (`files.read("…")`, `import <name>`), which is a different
  piece of work with a different predicate → **F2** in `FINDINGS.md`.
- An exported block carries no indentation, because the read loop trims each line
  before running it. Nothing about the run changes; an exported block that looks
  un-indented reads as a defect of the export → **F3** in `FINDINGS.md`, a decision
  rather than code.
- The four file commands are `:load`, `:save`, `:run`, `:restore` and `.export`.
  `.cd` does not exist in this REPL, so there is nothing else that takes a
  directory; if one is added, declaring `FILE_USAGE` on it is all the completer
  needs, and `edge_only_the_commands_that_take_a_path_take_a_path` is what will say
  so.