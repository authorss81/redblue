# FINDINGS — phase-117

The phase's own finding reproduced on `main` and was fixed; nothing here blocks
the phase. Two things noticed along the way that are *not* this phase's work.

## 1. The chosen theme does not survive the session

`:theme light` holds for the session it was typed in. A new `rb` starts on `dark`,
whatever the last session ended on.

- `src/repl/mod.rs:517` — `Repl::new` sets `style: Style::detect()`, which is
  `Palette::dark()` plus detection.
- `src/repl/style.rs:409` — `Style::detect`.

A `REDBLUE_THEME` environment variable (read next to `REDBLUE_COLOR` in
`resolve_enabled`) or a config file beside the history file would fix it. Neither
was asked for, and shipping persistence unasked is the "no unbounded speculation"
rule, so it is left here.

## 2. The banner is painted before the first line is read, so it cannot follow a theme

`Repl::run` prints the wordmark and the greeting before the session loop starts,
and the loop is where `:theme` is dispatched. A session that starts with colour on
and switches to `none` on its first line therefore has a coloured wordmark on
screen that nothing ever repaints.

- `src/repl/mod.rs:566-570` — `print_banner` / `print_greeting`, both before
  `session_loop`.

This is inherent to a banner drawn once rather than a defect: a theme cannot
retro-colour output that was already written. The alternative — asking the theme
before the banner — would mean a configuration question at startup, which is a
different feature.

## 3. `verify.sh` is not in this checkout

There is no `rbops/` directory in the working tree, so the project-specific gate
could not be run from here. The three Rust gates (`fmt`, `clippy -D warnings`,
`cargo test`) were run in full and are green; the pipeline that owns `verify.sh` has
to run it. Noted in `REPORT.md` as well.
