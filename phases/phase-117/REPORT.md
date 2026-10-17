# Phase 117 — Let the user pick the REPL theme instead of hardcoding one palette

## Reproduction (before any change)

```
$ printf ':theme light\n:themes\nsay "x"\n' | REDBLUE_COLOR=always ./target/debug/rb
...
[1;34m>>> [0mUnknown command 'theme'. Type :help for available commands.
[1;34m>>> [0mUnknown command 'themes'. Type :help for available commands.
[1;34m>>> [0mx
[1;34m>>> [1;36mGoodbye![0m
```

The finding reproduces as filed: no command selects a theme, and the whole palette
was one `match` inside `Paint::sgr` (`src/repl/style.rs:72-97` before this phase —
Prompt `1;34`, Heading `1;36`, Result `0;32`, Error `0;31`, Dim `2;37`, …), so the
only way to change a colour was to edit the implementation.

## What changed

| File | Lines | What |
|---|---|---|
| `src/repl/style.rs` | +286 −39 | `Paint::sgr()` deleted. New `Palette` — a named table of `(Paint, SGR)` — with `DARK`, `LIGHT` and `PLAIN` as values; `Palette::{ALL, dark, light, plain, by_name, is_plain, sgr, sample, listing, unknown_name_error}`. `Style` gains a `palette` field, `with_theme`, `palette`, `theme_name`, `sgr` and `set_theme(name) -> Result<Palette, String>`. `Paint::ALL` added. |
| `src/repl/commands.rs` | +30 −2 | `ReplCommand::{Themes, Theme(String)}` and two entries in `COMMAND_TABLE` (`themes`, `theme <name>`), so both are parsed, completed and listed by `:help` from the table. |
| `src/repl/mod.rs` | +36 −1 | Dispatches both commands; `set_theme` failure prints the message in `Paint::Error`; hands the style to the line reader every line so the prompt follows the switch. |
| `src/repl/editor.rs` | +25 | `LineSource::set_style` (default no-op) implemented for `PlainSource` and `EditSource`, so a mid-session switch reaches the prompt and the highlighter. |
| `tests/repl_theme_test.rs` | +500 (new) | New: 11 `#[test]`s over the palette API, the byte guarantees and the docs. |
| `tests/repl_test.rs` | +103 | New: 3 end-to-end tests driving `rb` over a pipe. |
| `tests/doc_blocks_{runs,fragments,known_fail}.txt` | 75 lines renumbered (29+7+39) | The doc-block ledger is keyed by fence line number; the new README/SPEC sections moved every block below them. Renumbered by block order, not by hand. |
| `README.md` | +16 | `### REPL themes`: the three themes, `:themes`, `:theme <name>`, and that colour-on/off is a separate question. |
| `SPEC.md` | +38 −4 | New `## The REPL` section with `### REPL themes`; TOC renumbered. |

The palette is one value per theme — `DARK`, `LIGHT`, `PLAIN` are `const Palette`
literals — and `Style::paint` looks the role up in it. Adding a theme is adding a
value; there is no `match` left to edit and no `Paint::sgr` to change.

`none` is a palette whose table is empty rather than a flag on the style, which is
what makes "byte-identical to colour off" a property the tests can check: `paint`
has no colour to reach for and returns the text itself.

## Tests added

| Test | Edge class covered |
|---|---|
| `the_dark_theme_is_exactly_the_palette_this_build_shipped_before_themes` | regression: all 14 shipped SGRs pinned, and `Paint::ALL` length pinned so a new role cannot slip past |
| `every_theme_paints_every_role` | boundary/completeness: no palette may leave a role uncoloured |
| `edge_brand_roles_stay_distinct_in_every_theme` | boundary: the `repl_color_test` brand/prompt/result guarantee, held of every theme |
| `edge_theme_names_resolve_case_insensitively_and_ignore_surrounding_whitespace` | empty (`""`, `"   "`, `"\t"`), near miss (`darks`, `dark dark`), unicode (`därk`, `ligℏt`, `ｎｏｎｅ`), case and surrounding space |
| `an_unknown_theme_name_is_refused_and_names_the_themes_that_exist` | **asserts a failure**: `set_theme("neon").expect_err(..)` names every shipped theme and the name asked for, and leaves the style untouched |
| `the_theme_command_carries_the_name_it_was_given` | parsing: both sigils, argument carried, `:theme` alone is `MissingArgument`, `:themes` takes none |
| `switching_a_theme_takes_effect_on_the_next_line` | immediate effect; and a palette cannot turn colour on when the style is off |
| `the_no_colour_theme_emits_exactly_the_bytes_colour_off_does` | empty, ascii, emoji, combining mark: `as_bytes()` compared against `Style::forced(false)` for every role |
| `edge_the_plain_theme_is_the_one_that_paints_nothing` | missing-key safety: a palette with no entry for a role paints it plain, and `is_plain` means exactly that |
| `the_theme_listing_renders_each_sample_through_the_real_style` | `:themes` samples are painted (contains each theme's own SGR), the `none` sample carries no escape, current theme marked exactly once |
| `every_documented_theme_name_actually_resolves` | malformed docs: every `- \`name\`` bullet in the README and SPEC theme sections resolves, and both commands are named there |
| `theme_switches_at_runtime_and_the_next_line_uses_it` (e2e) | the dark prompt escape before the switch, the light one after, in order; a program still runs |
| `edge_themes_lists_every_theme_with_a_sample_of_its_own_colours` (e2e) | `:themes` end to end, per-theme sample colours, `none` sample plain |
| `edge_an_unknown_theme_is_refused_and_the_session_still_runs_programs` (e2e) | **asserts a failure**: error text, `dark, light, none` offered, `Usage: :theme` for the bare command, session still runs and still on its original theme |

`parse_reads_the_whole_alias_table` in `src/repl/commands.rs` gained three rows
(`:themes`, `:theme light`, `:theme none`) — strengthened, not weakened.

`tests/repl_color_test.rs` is **unchanged** and still passes: 17/17. That is the
point — colour is now data, and the guarantees did not move.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | pass — 1799 passed, 0 failed, 0 ignored (65 binaries) |
| `cargo test --doc` | pass — 2 passed |
| `./rbops/verify.sh phase-117` | **not run: there is no `rbops/` directory in this checkout** |

On the fourth gate: `ls` of the project root shows no `rbops/` and no
`rbops/phases.json`, and `bash ./rbops/verify.sh phase-117` produces no output and
no exit status. The pipeline that dispatches this phase lives outside the working
directory. The three Rust gates were run in full; the project-specific gate has to
be run by whatever owns `verify.sh`.

Two byte-level checks run by hand, both against a `git stash`ed `HEAD` build:

- `REDBLUE_COLOR=always`, empty session: **identical bytes** to `HEAD`.
- colour off (pipe), empty session: **identical bytes** to `HEAD`.

## Edge-case matrix (AGENTS.md §3.2)

- **empty** — covered: `""`/`"   "`/`"\t"` resolve to no theme; `:theme` with no
  argument is `Usage: :theme`; empty text painted through `none` emits zero escapes.
- **singleton / boundary** — covered: one entry per role for every palette, and
  `SHIPPED_DARK.len() == Paint::ALL.len()`; the first and last roles are exercised
  by iterating `Paint::ALL` rather than a hand-written list.
- **out_of_bounds** — N/A: no user input indexes anything. A theme name is matched
  against a table by linear search and a miss is `None`; there is no index to be out
  of range of.
- **type_mismatch** — N/A: the argument is a theme name, not a value. The closest
  thing — a name of the wrong *shape* — is the `darks`/`dark dark` near-miss case.
- **numeric_boundary** — N/A: no numbers in the surface.
- **unicode** — covered: emoji, combining mark and multibyte text in the
  byte-identity test; non-ASCII theme names resolve to nothing rather than to a
  near match.
- **nesting_recursion** — N/A: a table lookup and a `format!`. No recursion.
- **duplicate_missing_keys** — partially covered: the *missing* half is
  `edge_the_plain_theme_is_the_one_that_paints_nothing` (a palette may leave a role
  out and it paints plain) plus the lookup doc on `Palette::sgr`. The *duplicate*
  half is untested: `Palette` is built only from `const` literals in this module, so
  a duplicate entry cannot be constructed from outside and the first-match rule is
  not pinned by a test. Recorded as a gap below.
- **malformed_input** — covered: bare `:theme`, whitespace-only argument, unknown
  name, both sigils, accented name, empty name.
- **resource_limit** — N/A: `:themes` prints three lines and a theme lookup is a
  scan of fourteen entries. No loop, no recursion, no allocation beyond the message.

## Invariants touched

- None of the language. `Value`'s variants, the `Error` kinds, the `.rb` extension,
  `set x to`, `say`, and `to … end` are all untouched; this phase adds two REPL
  commands and moves colour data into a value.
- `Paint::sgr` — a private method — was removed. It was not public API and had no
  callers outside this module.
- Two new public items on `redblue::repl::style`: `Palette` and `Paint::ALL`, plus
  five methods on `Style`. `Theme` (the token-class-to-role map used by the
  highlighter) keeps its name and its meaning; renaming it would have touched five
  files for no gain.

## Known gaps / follow-ups

- **The theme is not remembered across sessions.** `:theme light` lasts for the
  session; a new `rb` starts on `dark`. A `REDBLUE_THEME` environment variable or a
  config file would fix that, and either is a separate phase — neither is required
  here, and shipping one unasked would be the "unbounded speculation" rule.
- **The banner is painted before the first line is read**, so a session that starts
  coloured and then picks `none` has a coloured wordmark already on screen. The
  switch applies from the switch onwards; a theme cannot retro-colour what was
  printed before it was chosen. Not a defect, but a user could read it as one.
- **A duplicate `(role, SGR)` pair in a palette is not tested**, because no
  constructor for one is public. If a future phase makes palettes constructible
  (a config file, say), the first-match rule needs a test with it.
- **`verify.sh` has not seen this phase** — see the Gates table. If it rejects the
  phase, the failure is in the fourth gate and not in the three Rust gates, which
  were all run and all green.
