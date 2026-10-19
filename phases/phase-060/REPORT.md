# Phase 060 — Export sessions and complete file paths: `.export` plus TAB for `:load`/`:save`

## What changed

This run found the phase's own feature broken at the one place its contract is
stated, fixed it, and cleared the red test the resume arrived with. `.export`,
TAB path completion, `.reload`, `.format` and format-on-enter were already in the
tree (commit `5b73229`) and are untouched.

`must_touch: [src/]` is satisfied: `src/repl/transcript.rs` changed. This run's
diff is **+389 −36** across 3 files (`git diff --numstat -- src/ tests/`).

| File | Lines | What |
|---|---|---|
| src/repl/transcript.rs | +283 −35 | `OUTPUT_LANGUAGE`; `Transcript::markdown` fences an output that would be read back as a block of input and sizes the input fence against the text it writes; `rendered_output`, `forges_an_input_block`, `opening_fence`, `push_line`; `Transcript::read_markdown` now steps over a block in another language whole instead of line by line |
| tests/repl_test.rs | +105 −0 | 3 new end-to-end tests: the forged-fence round trip, a spaced filename offered quoted, a cursor inside a character refused rather than sliced |
| tests/unreviewed_work_ack_test.rs | +1 −1 | The `5b73229` acknowledgement this resume needs, replacing the `420f7b7` row that commit does not descend from (see *The red gate this run inherited*) |

## The finding, reproduced first

`.export` and TAB path completion were both present and both worked when this run
started. The finding that had not been reproduced was the round trip — the
property `.export` exists for, stated in its own module doc:

```
$ printf 'say "```redblue"\nsay "two"\n.export round.md\n:quit\n' | ./target/debug/rb
>>> >>> Exported 2 exchange(s) to 'round.md'.

$ cat round.md
# Redblue session

````redblue
say "```redblue"
````
```redblue

```redblue
say "two"
```
two
```

The document holds **three** `redblue` fence openings for a two-line session.
The third one is the *output* of the first line. `Transcript::read_markdown`
cannot tell them apart — a line that opens a `redblue` fence is a line the
session ran, and this one was said rather than typed:

```
left:  ["say \"```redblue\"", "\n```redblue\nsay \"two\""]
right: ["say \"```redblue\"", "say \"two\""]
```

So the round trip returned a line the user never typed, glued to the next line
they did, and a fresh session run on it would run the output as code. Every
other shape round-tripped, which is why the two-line fixture the definition of
done names never reached it: `say "one"` cannot forge a fence.

## What the change does

- **Output that would forge a block is fenced as `text`.** `forges_an_input_block`
  asks the reader's own question of each rendered output line — does it open a
  fence with a language? — and `markdown()` wraps such an output in an
  `OUTPUT_LANGUAGE` fence widened by `fence_width`, the same rule the input
  fence already used. Bare output is what every ordinary session gets and what
  every ordinary session keeps getting: the fence appears in exactly one case.
- **The reader steps over a block in another language whole.** `read_markdown`
  tracked only a `redblue` fence, so a `text` block's opening line was scanned
  as though it were plain text and the block's own contents were scanned for a
  fence to open. It now remembers any other *named* fence and skips its
  contents to the matching close, which is what Markdown does and what keeps
  the writer's own `text` fences from being walked into.

Both halves are needed. The writer alone still failed — the `text` fence was
`read_markdown`'s *not*-`redblue` line, and the fence inside it was found — and
that failure is what produced the `["say \"```redblue\"", "````\n\n```redblue\nsay \"two\""]`
above before the reader was changed too.

The rule is deliberately narrow: a bare `` ``` `` with **no language** is not a
forge. It opens nothing, it closes nothing, and `edge_a_line_that_prints_a_
fence_is_written_inside_a_longer_one` — a session that printed three backticks —
still exports byte-for-byte as it did before this diff.

## Round 1 review: the input fence was measured against the wrong text

The first review of this diff found that the *input* side of this change had been
left on the painted text while the output side had been moved onto the rendered
one. `markdown()` wrote `strip_escapes(&exchange.input).lines()` but sized the
fence with `fence_width(&exchange.input)`. `strip_escapes` deletes a whole CSI
sequence, so it can **join** two backtick runs that were one apart, and a fence
measured against the painted input is then exactly as wide as the line written
inside it — which closes the block. Reproduced on this checkout before the fix:

```
$ printf '``\033[0m`\n.export f.md\n:quit\n' | ./target/debug/rb
$ cat f.md
# Redblue session

```redblue
```
```

`read_markdown` on that document returns `[""]`: the fence opens, the *content*
line closes it, the third line opens a block that never closes and is dropped. The
user's line is gone, and the report's "a document this writer wrote reads back as
the session that wrote it" was false for this input shape even after the first
round of changes.

The fix is the same one the output half already had: ask both questions of the
same text. `markdown()` now strips once, then sizes the fence from what it is
about to write. `edge_backticks_joined_by_an_escape_are_measured_after_the_escape_is_off`
pins it and was watched red with exactly the document above before the change and
green after.

Two smaller review points were also accepted: the test inventory said "6 new
tests, 4 named `edge_*`" when all six were (now seven of seven), the
`type_mismatch` row credited two tests that had nothing to do with telling a
`text` block from a `redblue` one, the `malformed_input` row named a test that
does not exist, and this diff's line counts were one off in both places.

## The red gate this run inherited

`cargo test` was red on arrival, in one test:
`unreviewed_work_in_head_history_is_acknowledged`. The acknowledgement list named
`420f7b7`, the previous phase-060 resume, which is **not an ancestor of HEAD** —
`5b73229` re-lands the same work — so `scan` reported it stale while the real
`HEAD` commit was unacknowledged. One row replaced with one row, carrying what
`5b73229` actually contains (+3476/−18, 10 files, +1940/−17 under `src/`) and why
the abandoned attempt is not listed, following the pattern phases 056 and 059
already established.

## Tests added

7 new `#[test]` functions, **all seven named `edge_*`**: 4 in
`src/repl/transcript.rs` and 3 in `tests/repl_test.rs`. No test was weakened,
skipped, ignored or suppressed; this diff adds no `#[ignore]`, no `// skip` and no
`allow(clippy::`. The one pre-existing test file touched is the acknowledgement
list, which gained a data row.

| Test | Edge class covered |
|---|---|
| `edge_output_that_prints_an_input_fence_is_not_read_back_as_one` | the finding — the forged block is gone, the output is still in the document, and an output that *cannot* forge is still written bare |
| `edge_output_fencing_a_line_that_closes_the_fence_holds` | **boundary** — output holding both a 3-run and a 4-run of backticks still reads back as output |
| `edge_a_redblue_fence_inside_a_block_in_another_language_is_not_input` | **malformed input** — a hand-edited document whose `text` block shows a session; the shown example is not read as input |
| `edge_backticks_joined_by_an_escape_are_measured_after_the_escape_is_off` | the round-1 MAJOR finding — the fence around an input is measured against the *stripped* text, so a line whose backtick runs are joined by an escape still reads back |
| `edge_an_export_whose_output_prints_a_fence_reads_back_as_the_session` | the same defect through the `rb` binary, asserted with the library's own reader rather than the line-counting helper — the count is what hid the bug |
| `edge_tab_offers_a_spaced_filename_quoted_and_never_split` | **unicode/whitespace** — `my file.rb` and `my dir/` offered quoted, on a scratch directory so the answer does not depend on what the checkout holds |
| `edge_a_cursor_inside_a_character_is_refused_rather_than_sliced` | **failure** — `path_argument` returns `None` at an offset inside `é` rather than slicing it in half, and still answers at the boundaries either side |

**Every new test was watched failing for the right reason.**
`edge_output_that_prints_an_input_fence_is_not_read_back_as_one` and
`edge_an_export_whose_output_prints_a_fence_reads_back_as_the_session` were red
with the exact `left`/`right` shown above before `markdown()` changed, and the
first was red a second time — with a different `left` — after the writer changed
and the reader had not. `edge_a_redblue_fence_inside_a_block_in_another_language_
is_not_input` and the forge test were both put back to red by deleting the
reader's one-line foreign-fence arm and re-running. The cursor test was red on
its third assertion, because my expectation of the fragment was wrong (`é`, not
`é"`); it was corrected to what `path_argument` returns rather than loosened.
`edge_backticks_joined_by_an_escape_are_measured_after_the_escape_is_off` was red
in round 1 with exactly the three-fence document quoted above, and green once
`markdown()` sized the fence from the text it writes.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — no output |
| `cargo clippy --all-targets -- -D warnings` | **pass** — `Finished dev profile`, no warning or error |
| `cargo test --all-targets` | **pass** — 1869 passed, 0 failed, 0 ignored, across 67 binaries |
| `./rbops/verify.sh phase-060` | **NOT RUN — `rbops/` does not exist in this checkout** |

```
$ ls rbops
ls: cannot access 'rbops': No such file or directory
$ find . -maxdepth 3 -name "verify*.sh" -not -path "./target/*"
(no output)
```

The fourth gate could not be executed: there is no `rbops/verify.sh` in the tree
this phase was given, and `find` finds no verify script anywhere. This report
does not claim it passed. `.github/workflows/ci.yml` runs
`cargo fmt --all -- --check`, `cargo clippy -- -D warnings` and the test suite —
the first three gates above — and those are green. See F1 in `FINDINGS.md`,
recorded identically by phases 057, 058 and 059.

`cargo test --doc` was run as well (2 passed), and backwards compatibility was
checked by hand: every file in `examples/` and in `modules/` run through
`rb run` with no error output.

## Invariants touched

- **A document this writer wrote reads back as the session that wrote it.** This
  diff **was** that break, twice: an output line of `` ```redblue `` came back as
  a line the session ran, and — caught in round 1 — an input whose backtick runs
  were joined by an escape came back as nothing at all, because the fence around
  it had been measured against the painted text. The writer fences such an output
  and the reader steps over any block in another language; the input fence is now
  measured against the stripped text it writes. The property holds for a session
  whose output contains a fence and whose input contains an escape, not only for
  one whose output is `one`.
- **What "reads back" is checked against.** [`Transcript::read_markdown`] has no
  in-tree caller outside tests: `.export` writes and nothing in the REPL reads a
  document back (`:load` replays a `:save` history file, which is a different
  format). So this invariant is asserted, not observed — which is what the round-1
  finding was really about, and it is stated the same way in the module doc now.
- No `Value` variant, no `Error` variant, no grammar change, no change to
  `to … end` / `set x to` / `say`, no change to the `.rb` extension.
- `.export`'s document shape is unchanged for every session that was not already
  broken: `edge_export_writes_a_two_line_session_as_two_fenced_markdown_blocks`
  asserts the whole document line for line and still passes without being
  touched, as do the Unicode, CRLF and repeat-export tests.
- No test was re-scoped. `edge_export_writes_a_two_line_session_as_two_fenced_
  markdown_blocks`, `an_exchange_that_said_nothing_writes_no_output_block` and
  `a_document_written_by_hand_is_read_the_same_way` were all left exactly as they
  were, which is the evidence that the fix is narrow.

## Edge-case matrix (AGENTS.md §3.2)

| Row | Status | Where |
|---|---|---|
| empty | covered | `.export` of a session that ran nothing (`edge_a_session_that_has_run_nothing_says_so`); TAB in an empty directory (`edge_nothing_to_offer_is_no_candidates_rather_than_a_failure`) |
| singleton | covered | one line in, one block out (`edge_an_exchange_keeps_its_input_apart_from_its_output`) |
| boundary | covered | `edge_output_fencing_a_line_that_closes_the_fence_holds` — a fence run one backtick wider than the one it must survive |
| out_of_bounds | covered | a cursor inside a character (`edge_a_cursor_inside_a_character_is_refused_rather_than_sliced`); `pos` past the line end returns `None`, not a panic |
| type_mismatch | covered | `edge_output_that_prints_an_input_fence_is_not_read_back_as_one` and `edge_a_redblue_fence_inside_a_block_in_another_language_is_not_input` — a fence carrying `text` where a `redblue` one is expected is read as output, not as a line the session ran |
| numeric_boundary | N/A | nothing in this phase does arithmetic; `.export` and TAB completion move strings and paths |
| unicode | covered | `edge_a_session_of_unicode_exports_and_reads_back_unchanged` (emoji, combining mark, RTL, CJK) and the `é` boundary in the cursor test |
| nesting_recursion | N/A | a document is a flat list of fences; there is no nesting to recurse through and no recursion in the reader |
| duplicate_missing_keys | N/A | a session has no keys — it has exchanges in order, and `read_markdown` returns them in that order |
| malformed_input | covered | `edge_a_redblue_fence_inside_a_block_in_another_language_is_not_input`, `edge_an_unclosed_fence_at_the_end_of_a_document_reads_as_nothing`, `a_document_written_by_hand_is_read_the_same_way`, `edge_backticks_joined_by_an_escape_are_measured_after_the_escape_is_off` |
| resource_limit | covered | `edge_a_long_session_keeps_the_newest_exchanges_and_no_more` (`LIMIT`), `edge_a_long_listing_is_capped_and_the_order_does_not_depend_on_the_disk` (`MAX_PATH_CANDIDATES`) |

## Known gaps / follow-ups

- `rbops/verify.sh` cannot run here (F1) — the fourth gate is unverified, not
  passed.
- The TAB completion is read from the process's working directory
  (`completer::current_dir`), so a session cannot complete against another
  directory; F2 covers the related blind-typing case and is still open.
- `.export` fences output only when it could be misread. A document containing
  an output line of `` ``` `` with no language still relies on the reader
  treating a bare run as opening nothing, which is Markdown's reading and is
  pinned by `edge_a_line_that_prints_a_fence_is_written_inside_a_longer_one`.
- `Transcript::read_markdown` has no production consumer (F10).
  `.export` writes a document and no command reads one back, so the reader half of
  this diff is currently reached only by tests and by `redblue::repl::Transcript`
  in this repo's own integration tests. That is a real gap in the round trip, not a
  property of the reader: the obvious consumer is a command that runs an exported
  document, and nothing in this phase asked for one.