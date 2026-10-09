# Phase 032 — FINDINGS

Work found while reconciling the stdlib module list in SPEC.md with
`src/stdlib.rs` that does **not** belong to this phase. Each entry is
file:line anchored so the auditor can promote it.

## 1. `stdlib::builtin_function` was dead code, and that was the whole defect — FIXED here

**Status:** fixed. `src/stdlib.rs::builtin()` is now the single resolver both
engines call.

The finding that produced this phase was "`MODULES` has no entry for `text`,
`math` or `formats`". That was true and it was not the cause. The cause was one
layer down and it was worse:

```rust
// src/stdlib.rs, before this phase
pub fn builtin_function(name: &str, args: Vec<Value>) -> Option<Value> { … }
```

had **no caller in `src/`**. Both VMs asked `runtime::builtin`, which
implemented 34 names and did not include `abs`, `floor`, `ceil`, `round`,
`sqrt`, `uppercase`, `lowercase` or `trim`. So every one of those was inserted
into the global table as `Value::Builtin`, was never dispatched, and answered:

```
$ say uppercase("hi")
Error: RuntimeError: Unknown function 'uppercase'
```

`Unknown function` is the message for a name *nothing implements*, applied to
names the language documents in both SPEC.md and README.md.

### Why the test suite was green

`tests/numeric_edge_test.rs:387` calls `builtin_function("sqrt", …)` directly.
That test is green whether or not a *program* can reach `sqrt`, which is the
entire thing that was broken. A unit test against unreachable code is the
mechanism that let a whole builtin surface stay dead: it asserts the function
does the right thing, never that the language can get there.

**`tests/stdlib_module_docs_test.rs` is written against that lesson.** Every
test in it goes through a real `rb run`. None of them calls `builtin_function`.

## 2. `text` and `math` were not modules — FIXED here

**Status:** fixed. `MODULES` gains `text` and `math`; the dotted spellings
`text.uppercase` and `math.sqrt` run.

## 3. `formats` is not a module, and adding it would have been wrong — SPEC.md corrected

**Status:** corrected in SPEC.md, not implemented.

SPEC.md had a whole § formats documenting `formats.parse_json`,
`formats.to_json` and `formats.parse_csv`. None was ever a name in the
language; JSON and CSV are their own modules and have always worked as
`json.parse`, `json.stringify` and `csv.parse`.

The section was corrected rather than the namespace invented. Adding `formats`
to `MODULES` would have made a misspelling resolve to a namespace and then fail
later and less clearly — `Module 'formats' has no function 'parse_json'` — than
it does now, which is `Unknown variable 'formats'`. A name that does not exist
should say so at the point of use.

`edge_a_module_has_functions_and_not_members` pins this as a refusal, so a
later phase cannot quietly add the module and make the assertion false.

## 4. `math.PI` is not a name — SPEC.md corrected

**Status:** corrected in SPEC.md § Properties, and `PI`/`E` made reachable as
globals.

SPEC.md's `Circle` example read `give back math.PI * this.radius * …`. A module
has functions and no members, so this is `AnalyzerError: Unknown variable
'math'`. `PI` is the global.

Fixing the example exposed a second defect: `PI` and `E` are in the global
table but `Analyzer::name_is_bound` did not know it, so `say PI` worked while
`PI.times(..)` — the same name as a *method receiver* — was refused. Two
spellings of one global disagreed. `name_is_bound` now answers for a builtin
global, and only when the program has not claimed the name itself: a program
may declare `constant PI`, and reading it before the declaration is still the
error `edge_constant_used_before_declaration_is_an_error` pins.

## 5. `sqrt` of a negative — both existing contracts kept

**Status:** resolved; both hold.

Two pre-existing tests meet here and they are worth reading together:

- `numeric_edge_test.rs` asserts `builtin_function("sqrt", [-1])` is
  `Some(Value::Nothing)`.
- the same file asserts `set x to sqrt(-1)` is a `RuntimeError` at the language
  level.

Before this phase the second was green for the wrong reason: `sqrt` answered
`Unknown function`, which is also a `RuntimeError`. Both now hold honestly —
the helper answers `nothing`, and `stdlib::builtin` turns `nothing` into a
refusal **naming the function**, so a program is told `sqrt has no answer for
these arguments` rather than handed a value with no number in it.

The same rule covers every builtin that ran and had nothing to give: `log` of
zero, an overflowing `pow`, `tan` of a pole.

## 6. A builtin called wrongly is refused by name — FIXED here

**Status:** fixed.

`uppercase(1)` answered `Unknown function 'uppercase'`, which is a lie about a
name that exists. It is now `uppercase was called with arguments it cannot
use`. `Unknown function` is reserved for a name nothing implements.

## 7. `contains` is two functions and one name — FIXED here

**Status:** fixed.

`contains(text, needle)` is the text search; `contains(list, value)` is
membership, and `bootstrap/compiler.rb` uses the list form on every
`skip_newlines` dispatch (`bootstrap/compiler.rb:1219`, `:2631`).

This was found by the fixed point rather than by a test: implementing
`contains` as text-only broke 13 self-hosting tests with
`contains was called with arguments it cannot use`. A language change that
breaks the self-hosted compiler is caught by the compiler before it reaches a
release, which is the property the whole ladder exists for.

## 8. Still registered but unimplemented — NOT fixed, and named in the test

`tests/stdlib_module_docs_test.rs::edge_no_registered_builtin_is_unreachable_from_a_program`
walks `stdlib::builtins()` and asks whether each name is answerable. It passes
only because `KNOWN_GAPS` spells out every remaining one **with its reason**:

| Name | Why it is still a gap |
|---|---|
| `filter`, `reduce`, `list_map` | Higher-order: they must call a Redblue function, which needs a walker's captured scopes. `map` is implemented; these are not. |
| `console_clear`, `time_unix`, `csv_parse`, `network_get`, `network_post` | Registered in the module table; not implemented in `runtime::builtin`. |

Implementing a language surface is a language change and belongs to its own
phase. What does **not** belong to a later phase is *not knowing* which names
are gaps — so the list is in the test, in the file that would fail if a gap
were closed without the list being updated.

`text.split`, `text.join`, `starts_with`, `ends_with`, `replace`, `pow`, `sin`,
`cos`, `tan`, `log`, `exp`, `is_number`, `is_text`, `is_list`, `is_record`,
`pop`, `shift` and `to_list` were in this table and are now implemented, so they
were removed from it.

## 9. The corpus recorded the bug as expected output — ONE FILE regenerated

**Status:** fixed, and it is the most interesting thing here.

`corpus/value-tails-0020.rb` is the program `uppercase("a")`. Its checked-in
expectation was:

```
#label RuntimeError
#message Unknown function 'uppercase'
```

The corpus is the language's specification by example, and it had specified the
bug. That file is now the generator's output verbatim, `#value text:A`.

It was regenerated through `RB_WRITE_CORPUS=1`, which by design **never writes
`corpus/`** — it prints a refreshed copy under `target/tmp/` and leaves the
comparison to fail on the difference, because a test that rewrites the golden
files it is checking launders a regression into a green run. Exactly one file
differed. It was copied from the generator's output byte for byte, trailing
newline included.

## 10. `bootstrap/compiler.rb` does not satisfy `rb format --check`

Carried from phase-021's FINDINGS and unchanged here. It did not before this
phase either. No gate checks it, and reformatting ~2 850 lines would require
re-verifying the fixed point afterwards. A formatter phase should decide
whether `bootstrap/` is in scope.

## 11. `docs/BYTECODE.md:3` says "Format version: 4"; `FORMAT_VERSION` is 5

Carried from phase-022. The version table at `docs/BYTECODE.md:73` already
documents 5, so only the header line is stale. One line, another phase's area.