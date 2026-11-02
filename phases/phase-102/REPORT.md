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
without enforcing required flags.

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

## Tests added

| Test | Edge class covered |
|---|---|
| declared_flags_parse_into_a_typed_record | string, number and boolean flags; repeated flag (last wins); flag after a positional; generated usage |
| edge_path_flags_are_path_values_and_defaults_fill_in | path value type; default applied; absent boolean is `no` |
| edge_flag_value_forms_cover_equals_empty_and_no_arguments | `--flag=value`; empty string value; no arguments at all |
| edge_help_skips_required_flags_and_double_dash_ends_flags | help skips a required flag; `--` ends flag parsing |
| failures_name_the_flag_that_was_wrong | wrong type; missing required; unknown flag; malformed declaration; failure asserted with exact messages |
| cli_module_prints_usage_and_skips_the_body_for_help | `Cli.run_with` prints help without running the body; body runs without help; `Cli.run` reads `sys.argv()` |

## Test matrix

- empty — covered: empty argument list, empty string as a value, empty `arguments` field.
- singleton — covered: one declared flag, one positional, one value.
- boundary — covered: no arguments, first/last argument, `--` as the terminator, `--flag=value`.
- out_of_bounds — covered: a missing value after a flag is a catchable error rather than an index panic.
- type_mismatch — covered: a number flag given text, a non-list declaration, a non-text argument-list item, a wrong default type.
- numeric_boundary — covered: `NaN` and infinite text are refused; finite numbers become `Value::Number`.
- unicode — covered: text and path values keep their text unchanged; parsing only inspects ASCII flag prefixes.
- nesting_recursion — N/A: declarations are a flat list of records; parsing is a single loop.
- duplicate_missing_keys — covered: a repeated flag is last-wins; a duplicate declared name and a missing `name`/`type` are refused.
- malformed_input — covered: unknown declaration field, reserved flag name, unusable name, `takes_value: no` on a non-boolean, required with default.
- resource_limit — N/A: the parser allocates in proportion to the declaration and argument list; it runs no recursion, I/O or unbounded work.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2470 passed, 0 failed |
| rbops/verify.sh | pass |

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
