# Phase 032 — Reconcile the stdlib module list in SPEC.md with src/stdlib.rs

## The finding, reproduced first

The phase's evidence reproduced exactly, before anything was changed:

```
$ printf 'say text.uppercase("hi")\n' > target/tmp/a.rb
$ ./target/debug/rb run target/tmp/a.rb
Error: AnalyzerError: Unknown variable 'text'
  --> target/tmp/a.rb:1:1
1 | say text.uppercase("hi")
  | ^
exit=1

$ printf 'say formats.parse_json("{}")\n' > target/tmp/b.rb
$ ./target/debug/rb run target/tmp/b.rb
Error: AnalyzerError: Unknown variable 'formats'
  --> target/tmp/b.rb:1:1
1 | say formats.parse_json("{}")
  | ^
exit=1
```

Probing further found the cause was one layer below the missing names.
`src/stdlib.rs` registers `uppercase`, `split`, `abs`, `sqrt` … as
`Value::Builtin`, and **no VM dispatched a `Value::Builtin` at all** — so the
flat spellings were dead too:

```
$ printf 'say uppercase("hi")\n' | rb run -   # via a file
Error: RuntimeError: Unknown function 'uppercase'
$ printf 'say split("a,b", ",")\n' | rb run -
Error: RuntimeError: Unknown function 'split'
$ printf 'say json.parse("{\"a\": 1}")\n' | rb run -
1
```

`Vm::call` matched only `Value::Function`, so every builtin that was not already
an arm in `runtime::builtin` was unreachable under any spelling. The fix is one
table and one dispatch, and the documents become true rather than being trimmed
to what happened to work.

## What changed

| File | Lines | What |
|---|---|---|
| src/stdlib.rs | +152 −14 | `MODULES` (`src/stdlib.rs:11`) gains `formats`, `list`, `math`, `text` — ten names, all documented. `MODULE_FUNCTIONS` (`src/stdlib.rs:22`): the `module_member` → builtin table, every module a document names and the function of each. `is_module_function` / `resolve_module_function` / `dotted` / `named_as` / `requirement` / `arity` — resolution, the error naming, and the argument shape of a module function. `call_module_function` (`src/stdlib.rs:121`): the **single** place a module name is resolved, so the two VMs cannot drift. `builtins()` registers each qualified name. `builtin_function`: `split` and `join` implemented (registered before, answered by nothing), and `abs`/`floor`/`ceil`/`round`/`sqrt` now answer `None` for a non-number rather than `nothing` — "wrong argument" is not "no answer". |
| src/vm.rs | +11 −0 | `Vm::call` routes a `Value::Builtin` through `stdlib::call_module_function`, and reports the unknown function it falls back to |
| src/bytecode/vm.rs | +12 −0 | `BytecodeVm::call_named` does the same, one line longer because the bytecode VM pushes and advances |
| SPEC.md | +42 −21 | §Standard Library corrected where it promised a call the parser cannot read; `json`/`csv` documented as what they are, the aliases of `formats` |
| README.md | +46 −1 | §Standard Library now lists every module, so the two documents agree |
| tests/stdlib_module_docs_test.rs | +851 | 15 new `#[test]` functions (12 named `edge_*`) |

### SPEC.md corrections, each one named

The phase required correcting the `list.map([1, 2, 3], to (x) give back x * 2)`
example of SPEC.md:849, which depends on phase-034's function literal. Probing
the rest of §Standard Library found four more promises of the same kind, all
now corrected:

| SPEC.md line as found | What it promised | What it actually gave | Corrected to |
|---|---|---|---|
| 849 | `list.map([1, 2, 3], to (x) give back x * 2)` | `ParserError` — `to (x)` is not in the grammar | `list.length([1, 2, 3])`, with a line saying `map`/`filter`/`reduce` wait for the function literal |
| 822 | `text.split("a,b,c", by ",")` | `AnalyzerError: Unknown variable 'by'` — `by` is the range-loop step marker | `text.split("a,b,c", ",")`, with a line saying `by` is a loop step and nothing else |
| 776 | `set response to wait network.get(...)` | `ParserError: Unexpected token Wait` | `set response to network.get(...)` |
| 829 | `set pi to math.PI` | `Unknown variable 'math'` — a module has functions, not members; bare `PI` is `Unknown variable 'PI'` | `math.PI` removed; a line saying a constant is written `constant PI to 3.14159` |
| 864 | `formats.parse_json('{"name": "Alice"}')` | `LexerError: Unexpected character '\''` — single quotes are not a text literal | double quotes with `\"` escapes |
| 809-814 | `ask "Your name?"` under §console | `ParserError: Unexpected token Ask` — reserved word no statement parses | `console.log` / `console.error` / `console.clear`, with a line saying terminal input has no spelling today |

## Tests added

`tests/stdlib_module_docs_test.rs`, 15 new `#[test]` functions:

| Test | Edge class covered |
|---|---|
| `documented_module_names_are_exactly_the_modules_the_code_has` | the finding itself: the documented module names **and** the implemented ones, both directions, so a name in `MODULES` that neither document mentions fails too |
| `edge_documented_calls_all_lex_parse_and_resolve` | malformed input: every line in both documents' ```redblue blocks is lexed and parsed, so a documented call the parser cannot read fails |
| `edge_every_documented_module_function_is_routed` | the second half of the finding: every `module.function` a document shows is in `MODULE_FUNCTIONS` |
| `every_documented_module_can_be_imported_and_called` | `MODULES` and `is_module` agree |
| `module_functions_reach_the_builtins_that_already_exist` | the happy path, with the value each document prints — `text.uppercase("hello")` is `"HELLO"`, `math.round(3.7)` is `4`, `formats.*` is `json.*`/`csv.*` |
| `every_documented_module_runs_from_a_file` | `rb run` of a file calling all ten modules exits 0 and prints the documented values |
| `edge_a_module_function_refuses_wrong_arguments_by_naming_itself` | **no arguments, too many, wrong type** — 15 sources, each a caught `Runtime` error naming the function and carrying a span; plus a `try … catch error` proving it is catchable from Redblue |
| `edge_an_unknown_module_name_is_a_clean_error` | **asserts a failure**: `nosuchmodule.read("x")` and two siblings are a clean error, never a silent `nothing` |
| `edge_an_import_alias_reaches_the_same_module_function` | the second way a module name is written: `import text as T` |
| `edge_both_vms_resolve_a_module_function_the_same_way` | the tree-walker and the bytecode VM agree on five calls **and on the refusal**, so a `.rbc` cannot mean something else |
| `edge_empty_singleton_and_boundary_arguments_are_answered` | empty / zero / nothing: `""`, `[]`, `math.sqrt(0)`, `math.abs(0)`, `math.round(±0.5)`, `split("", …)`, `join([], …)`, `split("abc", "")` |
| `edge_unicode_and_escapes_survive_a_module_function` | emoji, CJK, RTL, a combining mark, `\n` `\t` `\"` through `text.length`, `text.split`, `text.join`, `text.trim`, `text.uppercase` |
| `edge_a_math_function_that_has_no_answer_is_nothing_not_a_number` | numeric boundary: `math.sqrt(-1)` is `nothing` and never a `NaN` in `Value::Number`; `math.abs(-0.5)` is not rounded away |
| `edge_spec_does_not_promise_a_call_the_parser_cannot_read` | the function literal cannot come back into §Standard Library |
| `edge_the_examples_still_run` | resource/state: every file in `examples/` still exits 0, each run in a directory of its own so two tests cannot race over the same output file |

### Edge-case matrix

- **empty / zero / nothing** — covered. `edge_empty_singleton_and_boundary_arguments_are_answered`: `text.length("")`, `text.uppercase("")`, `text.trim("")`, `split("", ",")`, `join([], ",")`, `list.length([])`, `math.sqrt(0)`, `math.abs(0)`.
- **singleton** — covered. `text.length("a")`, `text.join(["a"], ",")`, `list.length([7])`, `split("a", ",")`.
- **boundary** — covered. `math.round(0.5)` and `math.round(-0.5)` (both round away from zero), `math.sqrt(0)`, index-equivalent edges of `split`/`join`.
- **out_of_bounds** — N/A + why. No module function this phase added takes an index; the ones that do (`files.read`, `csv.parse`, `json.parse`) are covered by `tests/stdlib_modules_test.rs` (`edge_csv_row_index_out_of_bounds_is_a_clean_error`, `edge_files_read_of_a_missing_file_is_a_catchable_error`). `text.length` on a whole value has no index.
- **type_mismatch** — covered, twice. `edge_a_module_function_refuses_wrong_arguments_by_naming_itself`: `text.uppercase(1)`, `text.split(1, 2)`, `text.join("not a list", ",")`, `math.sqrt("nine")`, `list.length(5)`, `formats.parse_json(5)`, `formats.parse_csv(5)`, and `nothing` where text is expected.
- **numeric_boundary** — covered. `math.round(±0.5)`, `math.sqrt(0)`, `math.sqrt(-1)` → `nothing` (never `NaN`), `math.abs(-0.5)`. `2^53±1`, `±Infinity` and `1/0` are language-wide and covered by `tests/numeric_edge_test.rs`; no module function changes a numeric literal's representation.
- **unicode** — covered. `edge_unicode_and_escapes_survive_a_module_function`: 🎉, 中文, an RTL mark, a combining mark, `\n` `\t` `\"`, plus the split/join identity on them.
- **nesting_recursion** — N/A + why. No module function recurses or builds a structure of its own: `split`/`join`/`length`/`uppercase`/… are one-level over a flat value. `formats.parse_json` nests arbitrarily but its nesting is covered by `tests/stdlib_modules_test.rs` (`json_round_trips_nesting_unicode_and_empty_containers`). Recursion depth is `tests/call_depth_test.rs`.
- **duplicate_missing_keys** — N/A + why. No module function takes a record or reads a field. The functions that do — `json.parse`, `csv.parse` — are unchanged by this phase and their duplicate-key and missing-field edges are covered by `tests/stdlib_modules_test.rs` (`edge_json_duplicate_key_keeps_the_last_value`, `edge_json_missing_field_is_nothing_and_present_field_is_not`).
- **malformed_input** — covered. `edge_documented_calls_all_lex_parse_and_resolve` (an unterminated text, a stray token or an empty line in either document fails); `edge_documented_calls…` also covers `formats.parse_json(5)`; the JSON/CSV payload edges are `tests/stdlib_modules_test.rs` (`edge_json_malformed_input_is_an_error`, `edge_csv_unterminated_quote_is_an_error`).
- **resource_limit** — N/A + why. No module function this phase added allocates without bound or recurses: `split` is one pass over its input and `join` is one pass over its list, so the step and iteration counters in `src/vm.rs` govern them exactly as they govern any other expression. `network.get` is bounded by the published `NETWORK_TIMEOUT_SECS` (`tests/stdlib_modules_test.rs`), and the one call a file cannot make offline is caught rather than reached.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test` | **641 passed, 0 failed** (31 test binaries, incl. the doc test) |
| `cargo test --all-targets` | **640 passed, 0 failed** (30 test binaries; the 641st is the doc test) |
| `./rbops/verify.sh phase-032` | **NOT RUN — `rbops/verify.sh` does not exist in this checkout.** There is no `rbops/` directory here at all (`ls rbops` → no such file), and AGENTS.md rule 1 forbids creating or editing one. The four commands above are the gates I could run; the fifth is unrunnable from this checkout and I am not claiming a result for it. |
| every file in `examples/` exits 0 | verified by `edge_the_examples_still_run` and by hand |
| every file in `modules/` exits 0 | verified by hand: `MathUtils.rb` → 0, `SuiteKit.rb` → 0 |

Zero new `#[ignore]`, zero `.skip`, zero `allow(clippy::…)`, zero weakened or
deleted existing tests, zero newly-failing pre-existing tests.

One pre-existing test caught a defect **in my own new test** and was not
touched to hide it: `a_corpus_of_programs_runs_identically_on_both_vms` failed
on `examples/files.rb` because my first version of `edge_the_examples_still_run`
ran examples in the checkout root and raced it over `output.txt`. The new test
now copies each example into `target/tmp/phase032-examples` and runs it there.
Re-run twice, stable.

## Invariants touched

- None. `Value`'s variants, the `Error` family, the `.rb` extension, `end`-based
  blocks, `set x to <expr>`, `say`, the `{interp}` and trailing-comma syntax, and
  every test in `tests/` are unchanged.
- The stdlib **names** the language accepts have grown, which is the phase's
  stated goal: `text`, `math`, `list` and `formats` are now module names, and
  `text.split` / `text.join` now answer instead of raising `Unknown function`.
  No previously-rejected program is now accepted except those SPEC.md and
  README.md already documented.
- `math.abs`/`floor`/`ceil`/`round`/`sqrt` now answer `None` — a wrong argument —
  where they answered `nothing`. Reachable only through a module spelling, which
  is new, so no existing program changes. `stdlib::builtin_function("sqrt", …)`
  is `pub`, so `tests/numeric_edge_test.rs:390` is unaffected and still passes.

## Known gaps / follow-ups

All in `phases/phase-032/FINDINGS.md`, file:line anchored:

- **Bare builtin names still say `Unknown function`.** `say uppercase("hi")` fails
  while `text.uppercase("hi")` works. `Vm::call` routes a `Value::Builtin` only
  when the name is a module function, so nothing outside this phase's scope
  changed. Dispatching every registered builtin would turn 20 dead names into
  functions-or-errors — its own phase. FINDINGS §3.
- **`split` and `join` were the only two registered-but-absent builtins
  implemented here**, because the documents name them. `contains`, `starts_with`,
  `ends_with`, `replace`, `push`, `pop`, `shift`, `map`, `filter`, `reduce`,
  `pow`, `sin`, `cos`, `tan`, `log`, `exp`, `is_*`, `to_*` are still registered
  and answered by nothing. FINDINGS §2.
- **`list.map`/`filter`/`reduce` do not exist.** SPEC.md §Standard Library now
  documents `list.length` and says the other three wait for the function
  literal. FINDINGS §6.
- **`PI` and `E` are unreachable.** They are in the globals map but the analyzer
  never learns they are bound, so `say PI` is `Unknown variable 'PI'`, while
  `AGENTS.md` and the REPL completer both promise them. FINDINGS §5.
- **`SPEC.md:634` still shows `math.PI`** in the Properties example, outside
  §Standard Library and outside this phase's test's reach. FINDINGS §4.
- **`text` and `list` are not reserved words**, which is what lets
  `list.length(..)` parse as a member call while `set list to []` still works.
  This phase relies on it and did not change it. FINDINGS §7.