# Phase 032 — REPORT

## What this phase found

The phase was opened for "`MODULES` has no entry for `text`, `math` or
`formats`". That was real, and it was not the cause.

The cause was that **`src/stdlib.rs::builtin_function` had no caller in `src/`**.
Both engines asked `runtime::builtin`, which implements 34 names and did not
include `abs`, `floor`, `ceil`, `round`, `sqrt`, `uppercase`, `lowercase` or
`trim`. Every one of those was registered in the global table as
`Value::Builtin`, dispatched by nothing, and answered:

```
$ say uppercase("hi")
Error: RuntimeError: Unknown function 'uppercase'
$ say abs(-3)
Error: RuntimeError: Unknown function 'abs'
$ say sqrt(9)
Error: RuntimeError: Unknown function 'sqrt'
$ say text.uppercase("hi")
Error: AnalyzerError: Unknown variable 'text'
```

`Unknown function` is the message for a name nothing implements. It was being
answered for names SPEC.md and README.md both document.

## Why the suite was green

`tests/numeric_edge_test.rs:387` calls `builtin_function("sqrt", …)` directly.
It passes whether or not a program can reach `sqrt` — which is the entire thing
that was broken. That is the mechanism that let the surface stay dead: the
implementation was tested, the *reachability* was not.

`tests/stdlib_module_docs_test.rs` is written against that lesson. Every one of
its 9 tests goes through a real `rb run`. None calls `builtin_function`.

## What was fixed

**One resolver.** `stdlib::builtin` is now the single place a builtin name
resolves, and both `interpreter.rs` and `bytecode/vm.rs` call it. A builtin
cannot answer in the tree-walker and be missing from the bytecode VM.

- **`MODULES` gains `text` and `math`.** Dotted spellings now run.
- **`formats` was *not* added.** SPEC.md's § formats documented
  `formats.parse_json` / `to_json` / `parse_csv`; none was ever a name.
  JSON and CSV are their own modules. The spec was corrected rather than the
  namespace invented — a name that does not exist should say so where it is
  used, not resolve to a namespace and fail later.
- **`math.PI` corrected to `PI`.** A module has functions, not members. Fixing
  the example exposed that `PI`/`E` were unreachable as *method receivers*, so
  `Analyzer::name_is_bound` now answers for an unshadowed builtin global. A
  program may still declare its own `constant PI`, and reading that before the
  declaration is still an error.
- **~20 registered builtins implemented**: `abs`, `floor`, `ceil`, `round`,
  `sqrt`, `pow`, `sin`, `cos`, `tan`, `log`, `exp`, `uppercase`, `lowercase`,
  `trim`, `split`, `join`, `contains`, `starts_with`, `ends_with`, `replace`,
  `is_number`, `is_text`, `is_list`, `is_record`, `pop`, `shift`, `to_list`.
- **Bad arguments are refused by name.** `uppercase(1)` is
  `uppercase was called with arguments it cannot use`, not
  `Unknown function 'uppercase'`. `Unknown function` now means only that
  nothing implements the name.
- **A builtin with no answer is refused by name.** `sqrt(-1)` is
  `sqrt has no answer for these arguments`, which keeps `NaN` out of
  `Value::Number` without lying that the function is missing.

## Two regressions this phase caused, and what caught them

**The fixed point caught `contains`.** I implemented `contains` as text-only.
The self-hosted compiler uses the *list* form on every `skip_newlines`
dispatch, and 13 self-hosting tests failed with
`contains was called with arguments it cannot use`. `contains(list, value)` is
now membership as well as text search. A language change that breaks the
self-hosted compiler is caught by the compiler, which is the property the
ladder exists for.

**An existing test caught `map`.** Routing every call through the new resolver
made the shared resolver claim `map`, which has to *call* a Redblue function
and so needs the walker. `map`/`list_map` are answered before the resolver.
Had I not run the suite, this would have shipped a language where `map` was
dead.

**An existing test caught the shadowing rule.** Accepting builtin globals
blanketly in the analyzer broke
`edge_constant_used_before_declaration_is_an_error`: a program declaring its
own `constant PI` must still be an error to read before the declaration. The
builtin only answers when the program has not claimed the name.

## The corpus recorded the bug

`corpus/value-tails-0020.rb` is `uppercase("a")`. Its checked-in expectation
was `Unknown function 'uppercase'` — the corpus is the language's
specification by example, and it had specified the defect.

It was regenerated through `RB_WRITE_CORPUS=1`, which by design never writes
`corpus/`: it prints a refreshed copy under `target/tmp/` and leaves the
comparison to fail, because a test that rewrites the goldens it is checking
launders a regression into a green run. Exactly one file differed and it was
copied from the generator's output byte for byte, trailing newline included.

## Tests

9 new tests in `tests/stdlib_module_docs_test.rs`, all `edge_*`:

| Test | Pins |
|---|---|
| `edge_every_flat_builtin_the_stdlib_registers_answers` | 19 flat spellings, through a real run |
| `edge_a_module_spelling_of_a_documented_builtin_runs` | `text.*`, `math.*`, `json.*`, `csv.*`, `list.map` |
| `edge_a_module_has_functions_and_not_members` | `PI` is global, `math.PI` and `formats` are refusals |
| `edge_text_split_and_join_round_trip` | split, join, and the round trip in one expression |
| `edge_sqrt_of_a_negative_is_refused_by_name` | refusal by name; `sqrt(0)` and `sqrt(9)` still answer |
| `edge_a_builtin_refuses_bad_arguments_by_name` | `uppercase(1)`, `text.split("a", 1)` |
| `edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does` | both engines agree on 6 cases |
| `edge_no_registered_builtin_is_unreachable_from_a_program` | walks the registry; `KNOWN_GAPS` names every remaining gap with its reason |
| `edge_an_unknown_module_function_names_the_dotted_spelling` | `nosuchmodule.read`, not `nosuchmodule_read` |

**Red before green was proven**, not asserted: with the fix reverted, 6 of the 7
tests that existed at that point failed. The two that passed initially passed
for the wrong reason and were rewritten — the unknown-module case was being
refused by the *analyzer* rather than the VM, and `sqrt` was pinned against the
dead helper rather than through a program.

Nothing was weakened to make anything pass: no `#[ignore]`, no `// skip`, no
`allow(clippy::…)`, no deleted test.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --check` | clean |
| `cargo clippy --all-targets -- -D warnings` | zero warnings |
| `cargo test` | 1122 passed, 0 failed across 34 binaries, including `bootstrap_selfhost_test` (20) and `bootstrap_ship_test` (14) |
| every file in `examples/` and `modules/` exits 0 | 8/8 |
| `./rbops/verify.sh phase-032` | **not run — there is no `rbops/` directory in this checkout.** AGENTS.md forbids creating one. No result is claimed for that gate. |

## What this phase did not do

`filter`, `reduce`, `console_clear`, `time_unix`, `csv_parse`, `network_get`
and `network_post` are registered and still unimplemented. They are named in
`KNOWN_GAPS` in the test with their reasons, so the list cannot silently drift,
but implementing them is a language-surface change this phase did not own.
FINDINGS.md §8 carries the full table.

`rb format --check` on `bootstrap/compiler.rb` and the stale version line in
`docs/BYTECODE.md` are carried in FINDINGS.md §10 and §11. Neither is this
phase's area.

The reviewer of the blocked run raised findings against `src/vm.rs`,
`src/runtime.rs:923` (`Random::below` recursing with no depth bound),
`runtime.rs:736` (`math.random` overflowing to `inf`) and `runtime.rs:789`
(`type_of()` with no argument answering `"nothing"`). Those were measured
against the **7 October** tree: `src/vm.rs` was renamed to `src/interpreter.rs`
in phase-021, and none of the three symbols exists on `main` today. They are
not reproduced here and are not claimed as fixed.