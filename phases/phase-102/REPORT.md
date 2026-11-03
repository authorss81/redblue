# Phase 102 — Let a Redblue program parse its own command line

## What changed

The finding reproduced on the phase base before any edit: `sys` exposed only
`argv`, so `sys.parse([])` failed with
`RuntimeError: Module 'sys' has no function 'parse'`. Every command-line program
had to write the same flag loop and its own usage text.

`sys.parse(declaration)` now reads `sys.argv()` against a list of flag records
and answers a record with one field per flag plus `arguments`, `help` and
`usage`. `sys.parse(declaration, arguments)` reads an explicit list instead.
A flag declaration has `name` and `type` (`text`, `number`, `yes/no` or `path`)
and may add `takes_value`, `default`, `required` and `help`. A `yes/no` flag
defaults to taking no value; every other type takes one. A path flag is wrapped
with phase-100's `path::value`, so `type_of` answers `path` and every filesystem
call accepts it.

The parser accepts `--flag value`, `--flag=value`, repeated flags (last wins),
flags after positionals, `--` to end flag parsing, and an empty string as a
value. An unknown flag, a missing required flag, a value of the wrong type, a
missing value and a malformed declaration are catchable `RuntimeError`s naming
the flag and the expectation. `--help` and `-h` answer the generated usage
without enforcing required flags, including when they stand where a value was
expected. A value-taking flag whose next token is `--`, `--help`, `-h` or
another declared flag now reports a missing value naming the flag and its type;
it used to swallow that token as the value and report an unrelated wrong-type
failure. A token that looks like a long flag but names no declared flag is now
reported as an unknown flag instead of being stored as the preceding value, so
`--name --bogus` no longer silently makes `--bogus` a text value. The declared
types are exactly `text`, `number`, `yes/no` and `path`; the undocumented
`boolean` alias is gone.

The usage text is generated from the same declaration, in declaration order,
so a flag added to the declaration appears in help without a second edit. The
`modules/Cli.rb` module wraps this for programs: `Cli.run(declaration, body)`
prints help and does not call the body when help was asked for, and
`Cli.run_with(declaration, body, arguments)` takes the argument list explicitly.
The module is written against the language's early-return rule, so help is an
`if`/`else` rather than a `give back` inside the branch.

| File | Lines | What |
|---|---|---|
| `src/cli.rs` | +489 | declaration validation, typed value conversion, argv parsing and generated usage |
| `src/runtime.rs` | +40 | `sys_parse` dispatcher arm reading `sys.argv()` or an explicit text list |
| `src/stdlib.rs` | +27 −12 | the `sys.parse` member and the module-only spelling that keeps bare `sys_parse` off the flat surface |
| `src/lib.rs` | +1 | registers the private `cli` module |
| `modules/Cli.rb` | +38 | `Cli.run` and `Cli.run_with`, with help skipping the body |
| `tests/module_system_test.rs` | +251 | six CLI tests, including three named `edge_*` and four catchable-failure assertions |
| `SPEC.md` | +13 −1 | documents `sys.parse`, the declaration fields and the `Cli` wrapper |
| `README.md` | +2 | lists `sys.parse` |
| `SURVEY.md` | +9 −8 | member count 91 → 92, the `sys` row, module-only `sys.parse`, re-anchored registry lines |
| `tests/bootstrap_selfhost_test.rs` | +2 −2 | examples-and-modules count 9 → 10 because `modules/Cli.rb` is now in the corpus |
| `tests/doc_blocks_runs.txt` | +5 −5 | re-anchors the SPEC blocks after the `sys` section |
| `src/cli.rs` (resume) | +38 −10 | a value-taking flag followed by `--` or a declared flag reports a missing value instead of consuming it; `--help`/`-h` still answers help |
| `tests/module_system_test.rs` (resume) | +130 | four more CLI tests: missing value, generated usage, boolean/Unicode values, Redblue catchability |
| `src/stdlib.rs` (review) | +4 | a `sys_parse` shape so `:sighelp sys.parse` answers like the other module-only members |
| `tests/module_system_test.rs` (review) | +190 | declaration shape, field, name, default, argument-list and numeric-boundary failures; help in value position; every CLI case now runs on both engines |
| `src/cli.rs` (review round 2) | +9 −4 | an undeclared `--name` in value position reports an unknown flag; the undocumented `boolean` alias for `yes/no` is removed |
| `tests/module_system_test.rs` (review round 2) | +30 −18 | the missing-value branches run through the two-engine harness; unknown flags in value position are checked for both a number and a text flag; `boolean` is asserted to be an unknown type |
| `tests/unreviewed_work_ack_test.rs` (resume) | +1 | acknowledges the phase-102 resume delivery commit by subject |
| `src/cli.rs` (resume round 2) | +10 −4 | a negative number such as `-5` is a positional value rather than an unknown short flag; an unknown short flag such as `-x` is still refused |
| `tests/module_system_test.rs` (resume round 2) | +83 | negative positionals and the unknown short-flag failure, repeated flags after a positional, help skipping a body that would fail, and an empty declaration |
| `tests/module_system_test.rs` (review round 3) | +28 −5 | an inline value on a value-less `yes/no` flag is refused (`=yes` and `=false`), and the two-engine harness now compares the failure spans as well as the label and message |

## Tests added

| Test | Edge class covered |
|---|---|
| declared_flags_parse_into_a_typed_record | string, number and boolean flags; repeated flag (last wins); flag after a positional; generated usage |
| edge_path_flags_are_path_values_and_defaults_fill_in | path value type; default applied; absent boolean is `no` |
| edge_flag_value_forms_cover_equals_empty_and_no_arguments | `--flag=value`; empty string value; no arguments at all |
| edge_inline_value_on_a_value_less_flag_is_refused | malformed input / boundary: `--verbose=yes` and `--verbose=false` on a default `yes/no` switch are refused with "Flag '--verbose' does not take a value" on both engines |
| edge_help_skips_required_flags_and_double_dash_ends_flags | help skips a required flag; `--` ends flag parsing |
| failures_name_the_flag_that_was_wrong | wrong type; missing required; unknown flag; malformed declaration; failure asserted with exact messages |
| cli_module_prints_usage_and_skips_the_body_for_help | `Cli.run_with` prints help without running the body; body runs without help; `Cli.run` reads `sys.argv()` |
| edge_missing_value_before_another_flag_names_the_flag_and_type | out of bounds / missing value: a declared flag or `--` is not consumed as a value, and an undeclared `--bogus` is an unknown flag on both engines |
| edge_usage_lists_a_flag_added_to_the_declaration | boundary: help is regenerated when a flag is added to the declaration, including its type, default and help text |
| edge_boolean_values_and_unicode_survive_the_parse | unicode / type mismatch: explicit `yes`/`false` for a boolean, Unicode text and Unicode path values |
| edge_parse_failures_are_catchable_in_redblue | failure: a wrong-type parse is caught by Redblue `try`/`catch` and execution continues |
| declaration_shape_failures_name_the_declaration | malformed declaration: non-list declaration, non-record item, non-text name, missing name/type, non-text type, unknown type including the removed `boolean` spelling |
| malformed_declarations_are_refused | malformed input: unknown field, `takes_value: no` on a non-boolean, required with default, wrong-typed `takes_value`/`required`/`help` |
| defaults_must_have_the_declared_type | type mismatch: a text, number, yes/no or path default of the wrong type |
| duplicate_reserved_and_unusable_names_are_refused | duplicate/missing keys: duplicate declared name, reserved names (`arguments`, `help`, `usage`), empty and unusable names |
| argument_list_failures_name_the_item | type mismatch: a non-list argument list and a non-text item, named by position |
| numeric_boundary_refuses_non_finite_text_and_keeps_negatives | numeric boundary: `NaN`, `inf`, `-inf` and `1e400` text refused; finite and negative values kept |
| edge_negative_positionals_are_values_not_unknown_flags | boundary / failure: `-5`, `-2` and `-0.5` are positional values and a negative flag value, while `-x` is still a catchable unknown-flag error naming itself |
| repeated_flags_last_wins_after_a_positional | duplicate/missing keys: a repeated flag keeps its last value; flags between positionals are parsed, not stored as arguments |
| edge_help_does_not_run_a_body_that_would_fail | boundary / resource: help prints usage and succeeds even though the body would raise a runtime error if it ran |
| edge_empty_declaration_and_no_arguments | empty: an empty declaration with no arguments yields an empty positional list and usage that still describes help |
| edge_help_in_value_position_answers_usage | out of bounds / help: `--help` and `-h` in value position answer usage and skip a required flag |

Every case in this module runs through both the tree-walking interpreter and
the bytecode VM and asserts the same value or failure message and the same
printed lines, so `sys.parse`, `Cli.run` and the failure paths are exercised on
both engines.

## Test matrix

- empty — covered: empty argument list, empty declaration, empty string as a value, empty `arguments` field.
- singleton — covered: one declared flag, one positional, one value.
- boundary — covered: no arguments, first/last argument, `--` as the terminator, a value-taking flag's `--flag=value`, an inline value on a value-less flag being refused, and a negative number as a positional rather than a short flag.
- out_of_bounds — covered: a missing value after a flag is a catchable error rather than an index panic; a following `--`, declared flag or undeclared long flag is not swallowed as the value (the last is an unknown-flag failure), while `--help`/`-h` in that position answers help; an unknown short flag `-x` is still refused.
- type_mismatch — covered: a number flag given text; a non-list declaration; a non-record declaration item; a non-text name, type or argument-list item; a wrong default type; a non-list argument list; and an explicit yes/no value on a boolean flag.
- numeric_boundary — covered: `NaN`, `inf`, `-inf` and `1e400` text are refused with the finite-number message; finite numbers become `Value::Number`; negative values still parse because only another flag ends a value.
- unicode — covered: text and path values keep their Unicode text unchanged; parsing only inspects ASCII flag prefixes.
- nesting_recursion — N/A: declarations are a flat list of records; parsing is a single loop.
- duplicate_missing_keys — covered: a repeated flag is last-wins; a duplicate declared name and a missing `name`/`type` are refused.
- malformed_input — covered: unknown declaration field, reserved flag name, unusable name, `takes_value: no` on a non-boolean, required with default, and wrong-typed `takes_value`/`required`/`help` fields.
- resource_limit — N/A: the parser allocates in proportion to the declaration and argument list; it runs no recursion, I/O or unbounded work.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2486 passed, 0 failed |
| rbops/verify.sh | pass |

`verify.sh` reads the pipeline's pre-harvest copy of this report, so its
honesty check still sees the previous count and prints an under-claim warning;
the table above is this file's actual run (2486).


## Invariants touched

- None. `sys` gains a module-only member; no `Value` variant, error variant,
  grammar rule or existing spelling changed.

## Known gaps / follow-ups

- `sys.parse` is reachable only as `sys.parse`; the bare name `sys_parse` is
  deliberately not a flat builtin, consistent with the module-only text
  utilities.
- Repeated flags use the last value. There is no accumulator form for a flag
  that may be given more than once.
- `Cli.run_with` needs an explicit argument list because function parameters do
  not support default values in the current parser.
- Short aliases are limited to `-h`; other flags are long-form `--name` only.
