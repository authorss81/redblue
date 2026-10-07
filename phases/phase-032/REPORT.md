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
| src/stdlib.rs | +286 −7 | `MODULES` (`src/stdlib.rs:11`) gains `formats`, `list`, `math`, `text` — ten names, all documented. `MODULE_FUNCTIONS` (`src/stdlib.rs:22`): the `module_member` → builtin table, every module a document names and the function of each. `is_module_function` / `resolve_module_function` / `dotted` / `named_as` / `requirement` / `arity` — resolution, the error naming, and the argument shape of a module function; the review round widened `requirement`/`arity` to every module function whose count is fixed, and `named_as` now finds the builtin by a whole-word match in either spelling, so `math.random` is told about `math.random` and not about the `random_number` it reaches. `call_module_function` (`src/stdlib.rs:121`): the **single** place a module name is resolved, so the two VMs cannot drift. `builtins()` registers each qualified name. `builtin_function`: `split` and `join` implemented (registered before, answered by nothing), and `abs`/`floor`/`ceil`/`round`/`sqrt` now answer `None` for a non-number rather than `nothing` — "wrong argument" is not "no answer". |
| src/vm.rs | +11 −0 | `Vm::call` routes a `Value::Builtin` through `stdlib::call_module_function`, and reports the unknown function it falls back to |
| src/bytecode/vm.rs | +12 −0 | `BytecodeVm::call_named` does the same, one line longer because the bytecode VM pushes and advances |
| src/runtime.rs | +68 −7 | the review round: `builtin` refuses a call with an argument too many, `random_number` refuses arguments that are not numbers rather than answering `(0.0, 1.0)`, and `console.log`/`console.error` refuse no value to print |
| SPEC.md | +58 −15 | §Standard Library corrected where it promised a call the parser cannot read; `json`/`csv` documented as what they are, the aliases of `formats`; `text.length`'s bytes, `math.random`'s arguments and `console.log`'s one value said in as many words |
| README.md | +45 −2 | §Standard Library now lists every module, so the two documents agree |
| AGENTS.md | +5 −0 | the review round: §Standard Library says a module is how each function is called, since a bare `uppercase("t")` is an `Unknown function` |
| tests/stdlib_module_docs_test.rs | +1077 | 18 new `#[test]` functions (15 named `edge_*`) |
| tests/bytecode_vm_test.rs | +20 −0 | the review round found three corpus walks racing over `examples/files.rb`'s `output.txt`; a `CORPUS_WALK` lock keeps them apart without skipping a program |

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

The review round found three more, all of the same class, and all of them in
the section the lex/parse test reads:

| SPEC.md line as found | What it promised | What it actually gave | Corrected to |
|---|---|---|---|
| 855-856 | `might fail files.write(..)` / `might fail files.append(..)` | `ParserError: Unexpected token MightFail` — `might fail` is a reserved word no statement parses | a `try` … `catch error` … `end` block, with a line saying `try`/`catch` is the spelling of a recoverable failure today |
| 857-859 | `if files.exists("config.rb")` … `end` with no `then` | `ParserError: Expected Then but got End` | `if files.exists("config.rb") then`, which is the form §Expressions documents |
| §text, §math, §console, §files (silent) | nothing said about `text.length`'s unit, `math.random`'s arguments, `console.log()` and `files.read("a", "b")` | bytes, one number or two, a silent blank line, and a read of the first argument | each says what it takes: bytes (`text.length("🎉")` is 4), one or two numbers, one value to print, and no argument more than the signature |

## Tests added

`tests/stdlib_module_docs_test.rs`, 18 new `#[test]` functions — 15 from the
first run and three from the review round, named in the review table above:

| Test | Edge class covered |
|---|---|
| `documented_module_names_are_exactly_the_modules_the_code_has` | the finding itself: the documented module names **and** the implemented ones, both directions, so a name in `MODULES` that neither document mentions fails too |
| `edge_documented_calls_all_lex_parse_and_resolve` | malformed input: every ```redblue block in both documents is lexed and parsed **as a whole program**, with nothing skipped, so a documented call the parser cannot read fails |
| `edge_every_documented_module_function_is_routed` | the second half of the finding: every `word.word(` a document shows is a module the code has and a member `MODULE_FUNCTIONS` routes — including a `word` the code has no module for |
| `every_documented_module_can_be_imported_and_called` | `MODULES` and `is_module` agree |
| `module_functions_reach_the_builtins_that_already_exist` | the happy path, with the value each document prints — `text.uppercase("hello")` is `"HELLO"`, `math.round(3.7)` is `4`, `formats.*` is `json.*`/`csv.*` |
| `every_documented_module_runs_from_a_file` | `rb run` of a file calling all ten modules exits 0 and prints the documented values |
| `edge_a_module_function_refuses_wrong_arguments_by_naming_itself` | **no arguments, too many, wrong type** — 22 sources, each a caught `Runtime` error naming the function and carrying a span; plus a `try … catch error` proving it is catchable from Redblue |
| `edge_an_unknown_module_name_is_a_clean_error` | **asserts a failure**: `nosuchmodule.read("x")` and two siblings are a clean error, never a silent `nothing` |
| `edge_an_import_alias_reaches_the_same_module_function` | the second way a module name is written: `import text as T` |
| `edge_both_vms_resolve_a_module_function_the_same_way` | the tree-walker and the bytecode VM agree on five calls **and on the refusal**, so a `.rbc` cannot mean something else |
| `edge_empty_singleton_and_boundary_arguments_are_answered` | empty / zero / nothing: `""`, `[]`, `math.sqrt(0)`, `math.abs(0)`, `math.round(±0.5)`, `split("", …)`, `join([], …)`, `split("abc", "")` |
| `edge_unicode_and_escapes_survive_a_module_function` | emoji, CJK, RTL, a combining mark, `\n` `\t` `\"` through `text.length`, `text.split`, `text.join`, `text.trim`, `text.uppercase` |
| `edge_a_math_function_that_has_no_answer_is_nothing_not_a_number` | numeric boundary: `math.sqrt(-1)` is `nothing` and never a `NaN` in `Value::Number`; `math.abs(-0.5)` is not rounded away |
| `edge_spec_does_not_promise_a_call_the_parser_cannot_read` | the function literal cannot come back into §Standard Library |
| `edge_the_examples_still_run` | resource/state: every file in `examples/` still exits 0, each run in a directory of its own so two tests cannot race over the same output file |
| `edge_a_bare_builtin_name_is_not_a_second_spelling_of_a_module_function` | the review round: seven bare names are an `Unknown function` and their module spellings answer, which is what README.md now says |
| `edge_math_random_answers_a_number_or_refuses_by_name` | the review round: a draw is inside its range; five wrong calls and the bare name's are refusals, never a plausible wrong number |
| `edge_a_documented_argument_count_is_the_count_the_function_takes` | the review round: six calls at their documented count answer, `time.format(0)` still answers because its format is optional, and one argument too many is refused through the module spelling and the bare one |

### Edge-case matrix

- **empty / zero / nothing** — covered. `edge_empty_singleton_and_boundary_arguments_are_answered`: `text.length("")`, `text.uppercase("")`, `text.trim("")`, `split("", ",")`, `join([], ",")`, `list.length([])`, `math.sqrt(0)`, `math.abs(0)`.
- **singleton** — covered. `text.length("a")`, `text.join(["a"], ",")`, `list.length([7])`, `split("a", ",")`.
- **boundary** — covered. `math.round(0.5)` and `math.round(-0.5)` (both round away from zero), `math.sqrt(0)`, index-equivalent edges of `split`/`join`.
- **out_of_bounds** — N/A + why. No module function this phase added takes an index; the ones that do (`files.read`, `csv.parse`, `json.parse`) are covered by `tests/stdlib_modules_test.rs` (`edge_csv_row_index_out_of_bounds_is_a_clean_error`, `edge_files_read_of_a_missing_file_is_a_catchable_error`). `text.length` on a whole value has no index.
- **type_mismatch** — covered, three times over. `edge_a_module_function_refuses_wrong_arguments_by_naming_itself`: `text.uppercase(1)`, `text.split(1, 2)`, `text.join("not a list", ",")`, `math.sqrt("nine")`, `list.length(5)`, `formats.parse_json(5)`, `formats.parse_csv(5)`, `math.random("hi", "there")`, `time.sleep("soon")`, and `nothing` where text is expected. `edge_math_random_answers_a_number_or_refuses_by_name` is the one place where a wrong argument used to answer a plausible number instead.
- **numeric_boundary** — covered. `math.round(±0.5)`, `math.sqrt(0)`, `math.sqrt(-1)` → `nothing` (never `NaN`), `math.abs(-0.5)`. `2^53±1`, `±Infinity` and `1/0` are language-wide and covered by `tests/numeric_edge_test.rs`; no module function changes a numeric literal's representation.
- **unicode** — covered. `edge_unicode_and_escapes_survive_a_module_function`: 🎉, 中文, an RTL mark, a combining mark, `\n` `\t` `\"`, plus the split/join identity on them, and the byte counts the review round asked SPEC.md to state — `🎉` is 4, `世界` is 6, `e`+U+0301 is 3.
- **nesting_recursion** — N/A + why. No module function recurses or builds a structure of its own: `split`/`join`/`length`/`uppercase`/… are one-level over a flat value. `formats.parse_json` nests arbitrarily but its nesting is covered by `tests/stdlib_modules_test.rs` (`json_round_trips_nesting_unicode_and_empty_containers`). Recursion depth is `tests/call_depth_test.rs`.
- **duplicate_missing_keys** — N/A + why. No module function takes a record or reads a field. The functions that do — `json.parse`, `csv.parse` — are unchanged by this phase and their duplicate-key and missing-field edges are covered by `tests/stdlib_modules_test.rs` (`edge_json_duplicate_key_keeps_the_last_value`, `edge_json_missing_field_is_nothing_and_present_field_is_not`).
- **malformed_input** — covered. `edge_documented_calls_all_lex_parse_and_resolve` reads every ```redblue block in both documents as a whole program and skips nothing, so an unterminated text, a stray token, a reserved word used as a statement or a missing `then` in either document fails; `edge_documented_module_function…` also covers `formats.parse_json(5)`; the JSON/CSV payload edges are `tests/stdlib_modules_test.rs` (`edge_json_malformed_input_is_an_error`, `edge_csv_unterminated_quote_is_an_error`).
- **resource_limit** — N/A + why. No module function this phase added allocates without bound or recurses: `split` is one pass over its input and `join` is one pass over its list, so the step and iteration counters in `src/vm.rs` govern them exactly as they govern any other expression. `network.get` is bounded by the published `NETWORK_TIMEOUT_SECS` (`tests/stdlib_modules_test.rs`), and the one call a file cannot make offline is caught rather than reached.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test` | **644 passed, 0 failed** (31 test binaries, incl. the doc test) |
| `cargo test --all-targets` | **643 passed, 0 failed** (30 test binaries; the 644th is the doc test) |
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

## Review round — what the reviewer found, and what it changed

Six findings; four were BLOCKER or MAJOR and are fixed in the code, the
documents and the tests. The two MINOR findings are fixed too. Nothing was
weakened to make them go away: no `#[ignore]`, no `skip`, no
`allow(clippy::…)`, and no test deleted — the tests that could not fail were
made able to fail.

| # | Finding | Fix |
|---|---|---|
| 4 | **BLOCKER** `module_call` returned `None` for any receiver that is not a module, so the routing test skipped a documented `nosuchmod.foo(..)` instead of failing it | `tests/stdlib_module_docs_test.rs`: the receiver is no longer filtered by `is_module`, and `edge_every_documented_module_function_is_routed` asserts on **every** `word.word(` — the module first, then the member. Re-injecting `nosuchmod.read("x")` or `text.nosuchfunction("hi")` into §Standard Library now fails it (both verified) |
| 3 | **MAJOR** the lex/parse test skipped every `network.`/`time.` line, every non-module line and every line starting with `if `/`end`/`might fail`/`//`, so "every line must lex and parse" checked a subset | the test reads each ```redblue block **whole** and skips nothing: the parser gets `if … then … end` as a program, `network.get` and `time.sleep` are read like any other line (they are read, not run), and a block that will not parse fails. That made two SPEC promises visible and they are corrected — `might fail` is a reserved word no statement parses, and `if` needs `then` (FINDINGS §5) |
| 1 | **MAJOR** README.md:192 promised that `uppercase("hi")` and `text.uppercase("hi")` reach one function; a bare builtin has never been dispatched | the sentence now says the truth — a module is the only spelling, and a bare name is an `Unknown function`. `src/stdlib.rs`'s comment said the same false thing and is corrected. `edge_a_bare_builtin_name_is_not_a_second_spelling_of_a_module_function` pins both halves, so the day the bare name works, a test fails instead of a document going stale |
| 2 | **MAJOR** `math.random("hi", "there")` answered a nondeterministic number, because `runtime::builtin("random_number")` fell through to `(0.0, 1.0)` | `random_number` takes one number or two and refuses anything else by name, with a span; a third argument is refused too. `edge_math_random_answers_a_number_or_refuses_by_name` covers the draw, both one- and two-argument draws, five refusals and the bare name's refusal |
| 5 | **MINOR** `console.log()` printed nothing, and `files.read("a", "b")` read `"a"`, against signatures that document one argument | `stdlib::arity` states a count for every module function whose count is fixed, `runtime::builtin` refuses a call with an argument too many (so the bare name answers the same way), and `console.log`/`console.error` refuse no value to print. `time.format`'s count is one *or* two, so it is the one the table states nothing for. 22 sources in `edge_a_module_function_refuses_wrong_arguments_by_naming_itself` and `edge_a_documented_argument_count_is_the_count_the_function_takes` |
| 6 | **MINOR** `text.length` counts bytes and neither SPEC nor README said so | SPEC.md §text says it, with the emoji and CJK cases, and the unicode test pins `🎉` → 4, `世界` → 6, `e`+combining mark → 3 instead of recomputing the length in Rust. `tests/test_text.rb` has asserted bytes since before this phase, so bytes are the decision the documents now record |

### The gate that was already broken, and the fix

Running the gates surfaced a pre-existing flake rather than a new one: three
tests in `tests/bytecode_vm_test.rs` walk the whole corpus, they run in the
same process, and `examples/files.rb` writes, renames and deletes `output.txt`
in the working directory — so two walks side by side interleave over one file
and the second one to reach it fails to read a file the first had deleted.
`cargo test` failed about one run in three before this fix, on HEAD as well as
on the phase. A `CORPUS_WALK` mutex is held for the length of each walk: no
program is skipped, no assertion is loosened, no test is ignored. Re-run five
times, stable.

### Gate results, re-run after all of the above

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test` | **644 passed, 0 failed** (31 test binaries, incl. the doc test) |
| `cargo test --all-targets` | **643 passed, 0 failed** (30 test binaries; the 644th is the doc test) |

Three tests more than the first run's 641: the bare-name one, the
`math.random` one and the argument-count one. Nothing was removed.

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
  literal. FINDINGS §9.
- **`PI` and `E` are unreachable.** They are in the globals map but the analyzer
  never learns they are bound, so `say PI` is `Unknown variable 'PI'`, while
  `AGENTS.md` and the REPL completer both promise them. FINDINGS §8.
- **`SPEC.md:634` still shows `math.PI`** in the Properties example, outside
  §Standard Library and outside this phase's test's reach. FINDINGS §4.
- **`text` and `list` are not reserved words**, which is what lets
  `list.length(..)` parse as a member call while `set list to []` still works.
  This phase relies on it and did not change it. FINDINGS §11.
- **`might fail` is documented five times outside §Standard Library** and no
  statement parses it. §files is corrected here because this phase's test reads
  that section; §Error Handling and `docs/GRAMMAR.md` are the next spec-drift
  phase's. FINDINGS §5.
---

# Round two — the review findings

Seven findings, two BLOCKERs and five MAJOR. All seven are fixed in the code, the
documents and the tests; the detail and the reproduction of each is
`FINDINGS.md` §12–§18. Nothing was weakened to make them go away: no `#[ignore]`,
no `skip`, no `allow(clippy::…)`, and no test deleted.

| # | Severity | Finding | Fix |
|---|---|---|---|
| 1 | **BLOCKER** | `time.sleep(-1)` and `time.sleep(1e20)` reached `Duration::from_secs_f64`, which panics; a panic in the interpreter thread aborts the process rather than raising something a `try` can catch | `runtime::sleep_duration` refuses a value that is not finite or not in `[0, MAX_SLEEP_SECS]` and builds the `Duration` from whole seconds and a nanosecond remainder. `MAX_SLEEP_SECS` is one year. `time.sleep` is documented with its range |
| 2 | **BLOCKER** | `time.format` cast its argument with `as u64`, which saturates: `time.format(-1)` answered `1970`, and `time.format(1e20)` overflowed the `SystemTime` addition — a panic, and so the same abort | `runtime::timestamp_seconds` requires a whole number of seconds from zero below `i64::MAX`; what `chrono::DateTime::from_timestamp` still refuses is a `Runtime` error naming `time.format`. SPEC.md's new `### time` section says so |
| 3 | MAJOR | `random_number`, `random_choice` and `random_shuffle` read the clock with `unwrap()` and took the reading itself as the draw: 20 bits of a monotonic counter, no seed, and a `random_shuffle` that reused one reading for every step | `runtime::with_random` over a per-thread `SplitMix64` seeded from a *checked* clock read; `math.seed(n)` makes a run repeatable; Fisher-Yates with a draw per step |
| 4 | MAJOR | `runtime::builtin` compared `args.len() > expected` and `call_module_function` compared `given != expected`, so a bare `length()` and a `text.length()` failed in different words for one mistake | both compare `!=`. Three `tests/stdlib_modules_test.rs` entries pinned the old wording for a too-few call and now pin the count refusal — same entries, same assertions, corrected wording |
| 5 | MAJOR | `text.join` stringified every element, so `text.join([1, yes, [1]], ",")` answered `"1,yes,[1]"` while `text.uppercase(1)` refuses | `runtime::builtin("join")` refuses a non-text element by position and type; `stdlib::builtin_function("join")` holds the same rule behind it |
| 6 | MAJOR | README.md wrote `math.random(min, max)` and `time.format(ts, format)` as fixed-arity; both take an optional argument | both spellings are written down in README.md and SPEC.md, and SPEC.md gains the `### time` section it was missing |
| 7 | MAJOR | AGENTS.md listed `PI`, `E`, `pow`, `sin`, `cos`, `tan`, `log`, `exp`, `contains`, `starts_with`, `ends_with` and `replace` as callable and spelled all of them bare; none is answered | the two sections list only what runs, and the prose above says what is absent and why |

### New `#[test]` functions

Ten, in `tests/stdlib_module_docs_test.rs`. Each was run against the old
behaviour of its own finding and **fails** there, so each is able to fail:

| Test | Finding | How it fails on the old code |
|---|---|---|
| `edge_time_sleep_refuses_a_number_it_cannot_wait_for` | 1 | `assert_refused` rejects `The interpreter thread stopped unexpectedly` explicitly, so the abort is the failure |
| `edge_a_timestamp_that_is_not_a_whole_number_of_seconds_is_refused` | 2 | `time.format(-1)` answered `1970-01-01 00:00:00`, so `expect_err` gets `Ok(..)` |
| `edge_a_seed_makes_every_draw_after_it_repeatable` | 3 | `math.seed(1)` drew something different on the second run |
| `edge_draws_spread_across_their_range_instead_of_riding_the_clock` | 3 | 200 draws covered a span far under 0.5, because they were one monotonic counter |
| `edge_random_choice_and_shuffle_draw_from_the_same_generator` | 3 | the seeded `random_choice` did not pick what it picked before, and one permutation for the whole length was not a shuffle |
| `edge_the_runtime_never_unwraps_a_clock_and_never_builds_a_duration_from_a_bare_float` | 1, 3 | reads `src/runtime.rs` with its comments stripped; both the `from_secs_f64` and the `duration_since(..).unwrap()` are found again |
| `edge_a_bare_name_and_a_module_name_refuse_the_same_count` | 4 | thirteen bare/module pairs, each checked for `takes N argument(s), given M` |
| `edge_join_refuses_a_list_that_is_not_a_list_of_text` | 5 | seven wrong lists answered `"1,yes,[1]"`-shaped text instead of being refused |
| `edge_the_documents_state_the_optional_counts_the_code_takes` | 6 | README.md's §Standard Library held neither `math.random(max)` nor `time.format(ts)` |
| `edge_the_agents_document_does_not_promise_a_name_the_code_does_not_answer` | 7 | AGENTS.md's ```redblue blocks called `math.pow(..)`, `text.contains(..)` and eleven others |
| `edge_both_vms_refuse_the_same_and_repeat_the_same` | 1–4 | runs all of it again on the bytecode VM and compares the two refusals verbatim |

`assert_refused` and `run_rb` were added beside them. `assert_refused` is the
part that matters: `run_isolated` reports a panicked interpreter thread as
`Runtime("The interpreter thread stopped unexpectedly", Span::unknown())`, so a
test that asked only "is it an error?" would have passed on both BLOCKERs. It
asserts the message is not that one, names the function, and carries a span.
`run_rb` is needed for the constants: `PI` is in `stdlib::builtins()` and the
analyzer never learns it is bound, so a bare tree-walk prints `3.141592653589793`
and only the whole pipeline refuses it.

### Gates, re-run after all of the above

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test` | **658 passed, 0 failed** (31 test binaries, incl. the doc test) |
| `cargo test --all-targets` | **657 passed, 0 failed** (30 test binaries; the 658th is the doc test) |
| every file in `examples/` exits 0 | verified by `edge_the_examples_still_run` and by hand |
| every file in `modules/` exits 0 | verified by hand: `MathUtils.rb` → 0, `SuiteKit.rb` → 0 |

Fourteen more than round one's 644: the ten above and three unit tests in
`src/runtime.rs` for the generator itself — the mask a bounded draw is taken
from, a bounded draw at every boundary, and a sequence that advances from every
seed including `0`, which is what `math.seed(0)` gives and what a plain
`xorshift` would be stuck on forever. Nothing was removed.

### One defect found while writing those unit tests

`Random::below` asked for `1 << (u64::BITS - bound.leading_zeros())` bits, which
is a shift of 64 — and a panic in Rust — for any bound with its top bit set. No
call site reaches one today: the bounds are a list's length and an index. It is
written out as `low_bits(bits)` instead, which masks all 64 bits for a count that
does not fit, and `edge_the_mask_a_draw_is_taken_from_reaches_the_whole_range`
pins both ends of it. That is the third panic reachable from a Redblue program
in this phase's files, and the reason the source-scanning test exists at all.

---

# Round three — the review findings

Eight findings: one BLOCKER, six MAJOR and one MINOR. All eight are fixed in the
code, the documents and the tests; the detail and the reproduction of each is
`FINDINGS.md` §20–§27. Nothing was weakened to make them go away: no
`#[ignore]`, no `skip`, no `allow(clippy::…)`, and no test deleted.

| # | Severity | Finding | Fix |
|---|---|---|---|
| 1 | **BLOCKER** | `Random::below` retried a rejected draw by calling *itself*, so every draw of `math.random` / `random_choice` / `random_shuffle` spent a stack frame per rejection, and a long streak overflows a stack — an abort, not a catchable error | a `loop`. `edge_a_bounded_draw_retries_in_a_loop_and_not_in_a_call_to_itself` reads the function's body out of `src/runtime.rs` with comments stripped, fails on a `self.below(` inside it or on the absence of a `loop`, and runs 6,000 real draws |
| 2 | MAJOR | `math.random` formed `max - min` first, which is `infinity` for a range as ordinary as `-1e308` to `1e308`, so **every** draw from such a range was refused with `infinity is not a finite number` — a message naming no function | `min * (1.0 - r) + max * r` scales each end into `[0, 1]` before adding, so the width is never formed; what is left is checked and refused by the function's own name. `tests/numeric_edge_test.rs` asserted the old refusal and now asserts the stronger property (see below) |
| 3 | MAJOR | `type_of` answered `args.first().map(..).unwrap_or("nothing")`, so `type_of()` returned the very string `type_of(nothing)` returns — a missing argument produced plausible data | `stdlib::arity` states the count, `runtime::builtin`'s gate covers every stated count rather than only module-owned names, `stdlib::display_name` keeps `type_of` from being reported as `type.of`, and the arm matches `[value]` |
| 4 | MAJOR | an unknown module fell through to `call` and reported the internal `module_member` encoding: `Unknown function 'thing_read'` for `thing.read(...)`, in `src/vm.rs` and again in `src/bytecode/vm.rs` | both refuse before the call and name the program: `Module 'text' has no function 'nosuchfn'` for a receiver that is a module, `Unknown function 'thing.read'` for one that is not |
| 5 | MAJOR | `edge_an_unknown_module_name_is_a_clean_error` asserted only `message.contains("nosuch")`, which `nosuchmodule_read` passes; its `Error::Analyzer` arm was unreachable because `eval_err` skips the analyzer | five sources pinned on the **exact** message, each refusing any message containing `_`, each carrying a span, plus a whole-pipeline check through `rb run` |
| 6 | MAJOR | SPEC.md §Properties showed `give back math.PI * this.radius * this.radius`, which is `Unknown variable 'math'` — the drift round one fixed inside §Standard Library and left outside it | the example declares `constant PI to 3.14159` and reads `PI`, with a line saying why; `edge_the_properties_example_is_a_program_that_runs` runs the block through `rb run` and checks the area is `12.56636` |
| 7 | MAJOR | SPEC.md §Keywords **and** the Appendix both listed `might fail`, which lexes to `TokenKind::MightFail` and no arm of the parser accepts | the row is out of both tables; the prose under §Keywords names what the table no longer does, and `edge_the_spec_keyword_tables_do_not_promise_a_spelling_the_parser_cannot_read` reads the table rows, the prose, seven `ParserError`s, nine reserved words, and **every** ```redblue block of SPEC.md and `docs/GRAMMAR.md` |
| 8 | MINOR | `call_module_function`'s fallback reported `Unknown function '{name}'` — the encoding — while the `requirement` branch above it named the written spelling | `written`, the same fix as finding 4 applied to the other door |

### §5 closed for SPEC.md, because §26 pointed at it

§Keywords now says a recoverable failure is `try`/`catch`/`end` and points the
reader at §Error Handling for it. That made §Error Handling's four examples a
promise the correction itself depended on, so they were corrected: two now run as
they stand, `catch error of FileError` (a second `catch` is a `ParserError`
today) became `### One catch` and says so, and `to might fail divide(a, b)` /
`give back error "..."` became `### Where a failure comes from`, which names what
does raise a failure — `files.read` of a missing file, `1 / 0`, `xs[9]` — and
that a function cannot raise one of its own. `docs/GRAMMAR.md`'s `### Try/Catch`
example is corrected the same way; its grammar *productions* are left alone,
because that document specifies what the language is to be, and a note at §5.11
now names the productions that are not implemented yet.

### The one existing test that had to change

`tests/numeric_edge_test.rs` held
`edge_random_number_refuses_a_range_whose_width_overflows`, asserting
`message.ends_with("is not a finite number")`. That refusal **was** finding 2:
the range is drawable, and refusing it was the defect. The entry is kept, one
test where there was one test, and its assertion is replaced by the property that
matters — 64 draws from `-1e308` to `1e308` each finite and inside the range, a
draw from `-1e308` to `0` that is below zero, and `random_number(1, 2)` untouched.
It is renamed `edge_a_draw_from_a_range_wider_than_a_double_is_still_a_finite_number`
so the name stops claiming the refusal. Nothing was deleted, nothing was skipped,
and the assertion is stricter than the one it replaces.

### New `#[test]` functions

Five, and each was run against the old behaviour of its own finding and
**fails** there:

| Test | Finding | How it fails on the old code |
|---|---|---|
| `edge_a_bounded_draw_retries_in_a_loop_and_not_in_a_call_to_itself` | 1 | the body read out of `src/runtime.rs` contains `self.below(` and no `loop` |
| `edge_math_random_draws_from_a_range_too_wide_to_subtract` | 2 | `math.random(-1e308, 1e308)` answers `inf`; SPEC.md §math does not state the wide-range claim |
| `edge_type_of_takes_exactly_one_argument` | 3 | `type_of()` answers `"nothing"`, and `stdlib::arity("type_of")` is `None` |
| `edge_the_properties_example_is_a_program_that_runs` | 6 | the block still reads `math.PI`, and `rb run` refuses it with `Unknown variable 'math'` |
| `edge_the_spec_keyword_tables_do_not_promise_a_spelling_the_parser_cannot_read` | 7 | `### Keywords` still has the row `\| Functions \| to, takes, needs, might fail \|` |
| `edge_both_vms_name_an_unknown_module_member_the_way_the_program_wrote_it` | 3, 4, 8 | the bytecode VM reports `thing_read` / `xs_nosuchfunction`, and does not agree with the tree-walker |

`edge_an_unknown_module_name_is_a_clean_error` (finding 5) was rewritten rather
than added, so the entry count there is unchanged.

`numeric_edge_test.rs`'s renamed entry is the seventh, and it fails on the old
code too — `math.random(-1e308, 1e308)` was `inf`.

### Helpers added to `tests/stdlib_module_docs_test.rs`

`subsection(document, heading)` reads a `### ` section to the next heading of any
level (`standard_library_section` stops only at a `## `, which would read every
section of §Standard Library as one), `table_rows(section)` reads the `|`
lines only, so the prose that names the reserved words is not mistaken for the
drift it corrects, and `redblue_programs(document)` reads only the
```` ```redblue ````-tagged fences — `docs/GRAMMAR.md` also uses bare ``` fences
for its grammar productions, and a production is not a program.

### Gates, re-run after all of the above

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test` | **664 passed, 0 failed** (31 test binaries, incl. the doc test) |
| `cargo test --all-targets` | **663 passed, 0 failed** (30 test binaries; the 664th is the doc test) |
| every file in `examples/` exits 0 | verified by `edge_the_examples_still_run` and by hand |
| every file in `modules/` exits 0 | verified by hand: `MathUtils.rb` → 0, `SuiteKit.rb` → 0 |

Six more than round two's 658: the six above, less the one renamed (which is a
rename, so it is counted once either way) — that is, five new entries plus the
strengthened one. Zero `#[ignore]`, zero `.skip`, zero `allow(clippy::…)`, zero
deleted tests, zero newly-failing pre-existing tests.
