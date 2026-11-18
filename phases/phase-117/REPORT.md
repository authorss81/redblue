# Phase 117 — Selectable REPL colour themes

## What changed
| File | Lines | What |
|---|---|---|
| src/repl/style.rs | +450 −67 | `Theme` holds a name, a description and one SGR parameter per `Paint` role; ships `dark`, `light` and `none`; `Theme::resolve` is case-insensitive and whitespace-tolerant; `Style` carries the selected theme. The resumed pass adds `Theme::names()` as the single source of the available names used by `:theme`'s error. |
| src/repl/commands.rs | +63 −0 | `:themes` / `.themes` parses to `Themes`; `:theme <name>` / `.theme <name>` parses to `Theme`; a bare `:theme` is a `MissingArgument` naming `<name>`. |
| src/repl/mod.rs | +229 −1 | Dispatches both commands, renders the `:themes` listing through the real `Style`, switches `self.style` in place, and hands the current style to the live line source before every read so the switch is immediate. |
| src/repl/editor.rs | +121 −10 | The prompt and the line being typed adopt the session's current style/theme on `set_style`, so a `:theme` switch repaints the live editor without a restart. |
| SPEC.md | +40 −0 | Documents the named theme model, the three shipped themes, `:themes`, `:theme <name>`, unknown-name behaviour and the no-colour guarantee. |
| README.md | +19 −0 | Documents the same command and the shipped theme names. |
| tests/repl_color_test.rs | +14 −1 | Pins the documented precedence: an explicit `REDBLUE_COLOR` wins over `NO_COLOR`, while `auto` defers to it. No assertion was removed or loosened. |
| tests/repl_test.rs | +48 −0 | Integration tests for `:themes`, runtime `:theme` switching, the no-colour theme, unknown names, and a bare `:theme` failure. |

## Reproduction
Before this phase `src/repl/style.rs` mapped each role in a hardcoded `sgr()` match and the REPL had no theme command. The implemented session:

```
$ printf ':themes\n:theme light\n:theme nope\nsay "after"\n:quit\n' | REDBLUE_COLOR=never target/debug/rb
>>> Colour themes (switch with :theme <name>):
 * dark   default palette for a dark terminal background   ...
   light  palette for a light terminal background          ...
   none   no colour; identical to colour off               ...
>>> Theme set to light.
>>> Error: unknown theme 'nope'. Available themes: dark, light, none.
>>> after
>>> Goodbye!
```

## Tests added
| Test | Edge class covered |
|---|---|
| edge_themes_command_lists_themes_and_theme_switches_at_runtime | happy path plus immediate runtime switch in a piped session |
| edge_unknown_and_no_colour_theme_leave_the_session_usable | malformed input / failure: unknown name, no-colour switch, following program still runs |
| edge_bare_theme_reports_the_missing_name_and_keeps_the_session_usable | failure assertion: bare `:theme` reports `Usage: :theme <name>` and the session survives |
| edge_theme_switch_is_immediate_and_no_colour_is_byte_identical | boundary: next paint uses the new palette; `none` bytes equal `Style::forced(false)` |
| edge_a_theme_switch_reaches_the_live_line_source | state: the live prompt/highlighter receives the switched style |
| edge_unknown_theme_reports_names_and_leaves_session_usable | failure assertion: error names every theme, old theme kept, following program runs |
| edge_theme_names_resolve_case_insensitively_and_ignore_whitespace | human input: mixed case, tabs, newlines and surrounding spaces |
| edge_unknown_theme_is_an_error_naming_every_available_theme | failure assertion: empty/whitespace/unicode/NUL names are errors naming all themes |
| no_colour_theme_is_byte_identical_to_a_disabled_style | boundary: byte equality for every role and empty/unicode text |
| every_shipped_theme_has_a_unique_name_that_resolves | duplicate/missing keys: unique non-empty names all resolve |
| every_documented_theme_name_resolves | docs: SPEC.md and README.md name every shipped theme and each resolves |
| edge_theme_commands_parse | malformed input: both sigils, whitespace, and bare `:theme` as missing argument |
| edge_switching_theme_repaints_the_live_prompt_and_line | state: editor prompt and highlighted line use the switched theme |
| edge_theme_names_are_unique_and_resolve_in_listing_order | boundary: listing order, uniqueness and resolution agree |
| edge_a_colour_theme_holds_one_colour_for_every_paint_role | boundary: every colour theme has exactly one colour per role |
| edge_no_colour_theme_keeps_every_token_class_but_no_palette | empty: `none` has no palette and still paints every role plain |

Edge-case matrix dispositions:
- empty — covered: `none` has no palette; empty text paints unchanged; empty/whitespace theme names are errors.
- singleton — covered: exactly one colour per role and one name per theme are asserted.
- boundary — covered: listing order, case/whitespace resolution, and `nothing`/empty text bytes.
- out_of_bounds — N/A: themes are a fixed in-memory table, no index is taken from user input.
- type_mismatch — N/A: the command takes text only; a number is just a name and is rejected like any unknown name.
- numeric_boundary — N/A: no arithmetic or floating-point value is involved in theme selection.
- unicode — covered: unicode text paints byte-identically; unicode theme names resolve to a clean error.
- nesting_recursion — N/A: theme selection is a flat lookup with no recursion.
- duplicate_missing_keys — covered: duplicate theme names, duplicate role colours, and missing roles are tested.
- malformed_input — covered: bare `:theme`, empty/whitespace names, unknown names, and NUL in a name.
- resource_limit — N/A: the palette is a static three-entry array; no allocation grows with input.

## Gates
| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 3156 passed, 0 failed, 0 ignored across 87 suites |
| rbops/verify.sh phase-117 | pass |

## Invariants touched
- None. The source extension, grammar, `say`, assignment, `Value` variants and `Error` variants are unchanged. Colour remains off unless enabled, and the `none` theme is byte-identical to colour off.

## Known gaps / follow-ups
- No per-role colour overrides yet. This phase deliberately ships whole themes only, as required; a user who wants one role changed can copy a theme value in a later phase.
- Theme choice is per session and is not persisted between runs. That is a separate feature from making the palette data.
