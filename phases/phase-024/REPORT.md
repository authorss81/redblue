# Phase 024 — Implement `constant`, or fix the module that assumes it

## Reproduction of the finding

The finding reproduces on this checkout, unchanged:

```
$ cargo run --bin rb -- run modules/MathUtils.rb
Error: ParserError: Expected function name
  --> modules/MathUtils.rb:4:16
4 | constant PI to 3.14159
  |                ^
$ echo $?
1
```

`constant` was in no keyword table (`src/lexer.rs`, 49 keywords, `give` and
`back` present), so `constant` lexed as an identifier and the `to` after it
reached `Parser::parse_callable` as a `to` with no function name in front of it.

## Round 1 review

The reviewer found five defects. All five are fixed in this round; the two
BLOCKERs that were about the tests are fixed by making the tests run the
pipeline `rb run` runs, which is the change that exposed a sixth defect the
tests had been hiding (FINDINGS §8).

| # | Finding | Fix |
|---|---|---|
| 1 | **BLOCKER** `import_binds_a_modules_constants` used `run_isolated`, which bypasses `analyzer::analyze`, so it passed while `rb run` rejected `give back TAU` as `Unknown variable` — `Statement::Import` declared nothing. | `Statement::Import` declares the module's `set` and `constant` names into the scope it is written in, read out of the module file by `vm::module_bound_names`. Every test in `tests/constant_test.rs` now runs `redblue::run_source_value` — lexer, parser, analyzer, VM. |
| 2 | **BLOCKER** A function body was analysed where it is written, so a body declared above a `constant` was refused, contradicting the spec's promise that a body runs later. | `analyzer` collects the names a `constant` binds and the names an `import` binds before the walk, and consults them (`bound_later`) only inside a body that runs at a call — `deferred_depth`. A read at the top level before the declaration is still the unknown variable any other name read too early gives. |
| 3 | **BLOCKER** `Statement::Constant` compiled to the same `Opcode::Store` a `set` compiles to, so the refusals existed only in the tree-walking VM. | `Opcode::DeclareConst` (`DECLARE_CONST`, byte 46) plus a `FORMAT_VERSION` bump from 2 to 3, because a version-2 file cannot say a name is read-only. `docs/BYTECODE.md` updated: the instruction row, the version row, the compiled form of a declaration. |
| 4 | **MAJOR** `load_module` bound a module's `set` with a raw `globals.insert`, around the constant refusal `set_var` enforces. | `Vm::bind_module_name` — the refusal is `Vm::refuse_constant_rebind`, shared with `set_var`, and the write lands in `globals` because a module's names are names of the whole program. |
| 5 | **MAJOR** `load_module` bound a module's `constant` with the fallible `bind_constant`, so a second `import MathUtils` was a duplicate declaration. | `Vm::modules` records the module programs already run, so a second import of one is a no-op rather than a second binding of the same names. |

Each fix is pinned by a test that was watched failing without it, recorded per
test under "Tests added" below.

## The decision

Redblue has module-level constants, and `modules/MathUtils.rb` has been written
against them since the module system was written: `SPEC.md` §Modules already
showed `constant PI to 3.14159`, and `docs/GRAMMAR.md` §4.1 already carried the
production. The spec was right and the implementation was missing, so the
keyword is implemented rather than the module rewritten.

`constant NAME to <expr>` binds a name of the whole program to the value the
expression has where the declaration runs:

- the value goes into `Vm::globals`, so a read resolves through the one name
  lookup the VM already had — a function body declared above the `constant`
  reads it when it is called, which is what lets a module's functions share one
  value; the analyzer collects the constant names before it walks the statements
  so that a body written above the declaration is not refused for a read the
  call will answer;
- the *name* goes into a new `Vm::constants` set, and that is what makes it a
  constant: a second declaration is refused
  (`Constant 'TAU' is already declared`) and `Vm::set_var` refuses a write that
  would land on that name (`Cannot assign to constant 'TAU'`);
- a refused declaration leaves the first binding in place;
- a local of the same name — a parameter, a loop variable — shadows it for the
  length of its scope, because a name a live local scope holds is written there;
- reading it before the declaration is the unknown-variable error any other name
  read too early gives, from the analyzer where it can see the read and from the
  VM where it cannot.

`set x: number to 10` is a parse error today, so `constant x: number to 1` is a
parse error too: the optional `':' type_expression` of GRAMMAR §4.1 is not
implemented for `set` either. That gap is recorded, not fixed here.

## What changed

| File | Lines | What |
|---|---|---|
| `src/lexer.rs` | +2 −0 | `("constant", TokenKind::Constant)` in `KEYWORDS` — the one table the lexer and the editor grammar both read |
| `src/parser.rs` | +37 −0 | `Statement::Constant { name, value }`, `parse_constant`, and the `parse_statement_inner` arm |
| `src/analyzer.rs` | +8 −0 | an analyzer arm: the value is analysed and the name declared, so a read after the declaration is in scope |
| `src/analyzer.rs` | +181 −30 | round 1: the names a `constant` and an `import` bind are collected before the walk and consulted inside a body that runs at a call; `import` declares the module's names into its scope; `set` and `constant` declare where the VM binds them (FINDINGS §8) |
| `src/lib.rs` | +15 −0 | `run_source_value` — the whole pipeline returning the value of the last statement, so a caller cannot test around the analyzer |
| `src/vm.rs` | +66 −18 | `Vm::constants`, `Vm::bind_constant`, the execute arm, the module loader's `constant`, and `set_var` returning `Result` so a write onto a constant is refused |
| `src/vm.rs` | +55 −22 | round 1: `vm::module_source` / `vm::module_bound_names` (one search path, shared by the loader and the analyzer), `Vm::modules` so a re-import is a no-op, and `bind_module_name` / `refuse_constant_rebind` so neither a module's `set` nor the name an import binds can rebind a constant |
| `src/bytecode/opcode.rs` | +11 −0 | round 1: `Opcode::DeclareConst`, at byte 46 — `ALL`, `name` and `takes_constant_index` |
| `src/bytecode/format.rs` | +3 −1 | round 1: `FORMAT_VERSION` 2 → 3, with the reason in its own doc comment |
| `docs/BYTECODE.md` | +22 −6 | round 1: version 3, the `DECLARE_CONST` row, the version-3 row of the change table, the compiled form of a declaration |
| `src/formatter.rs` | +6 −0 | `constant NAME to <expr>` round-trips through `rb format` |
| `src/linter.rs` | +7 −0 | a constant binds a name as a `set` does, so it is not reported as an unused variable |
| `src/bytecode/codegen.rs` | +2 −2 | `constant` compiles to `DECLARE_CONST` — round 1; it compiled to the store a `set` compiles to |
| `tooling/vscode/syntaxes/redblue.tmLanguage.json` | +1 −1 | regenerated with `rb grammar`, which `shipped_grammar_file_matches_the_generator` requires |
| `SPEC.md` | +32 −1 | `constant` in the keyword table, and a Constants subsection stating the rules |
| `docs/GRAMMAR.md` | +8 −1 | `constant` in the reserved-keyword list, and the runtime rules under §4.1 |
| `tests/constant_test.rs` | +413 −0 | 20 tests, below — rewritten in round 1 to run the whole pipeline |
| `tests/bytecode_test.rs` | +71 −0 | round 1: the `DECLARE_CONST` instruction, its byte, and the refusal of a version-2 file |
| `tests/linter_test.rs` | +2 −5 | the corpus no longer skips `modules/`, and the comment that said why is gone |
| `tests/loop_bounds_test.rs` | +17 −9 | the shipped-file walk now includes `modules/`, which it skipped because the file did not parse |

The two test edits remove a workaround for this exact defect. `modules/` was
excluded from the linter corpus ("`modules/MathUtils.rb` uses `constant`, which
no phase has implemented yet") and from the shipped-file walk ("`to
circle_area(radius)` — Expected function name"). Both corpora pass with the
module included.

## Tests added

`tests/constant_test.rs`, 20 tests. Every one of them runs the whole pipeline —
`redblue::run_source_value`, which is lexer, parser, analyzer, VM, the same
sequence `rb run` runs. Round 1 replaced `run_isolated`, which skips the
analyzer and let a program `rb run` refuses pass here; the "Fails without" column
names what each test fails on when its fix is taken back out.

| Test | Edge class covered | Fails without |
|---|---|---|
| `constant_declares_a_readable_name` | the happy path — a declaration binds the value it was given | the keyword (parse error: `Expected function name`) |
| `constant_is_read_by_a_function_body` | nesting_recursion — a constant composed from another constant, read from inside a function | the keyword |
| `shipped_mathutils_module_runs_and_binds_its_constants` | the phase's definition of done — the shipped file runs, and `PI`/`TAU` are bound by running it | the keyword (`modules/MathUtils.rb` did not parse) |
| `import_binds_a_modules_constants` | the module loader's arm — `import MathUtils` binds `TAU`, and it is still a number | the analyzer's `import` arm: `Unknown variable 'TAU'` |
| `edge_a_body_declared_above_an_import_reads_the_modules_names` | nesting_recursion — a body written above the `import` reads a module name at the call | the later-names collection |
| `edge_duplicate_constant_is_refused` | duplicate_missing_keys — **asserts a failure**; second declaration refused, first value kept | `Vm::constants` |
| `edge_constant_declared_again_by_a_second_call_is_refused` | duplicate_missing_keys / nesting — **asserts a failure**; a `constant` inside a function is still a program-level name | `Vm::constants` |
| `edge_constant_cannot_be_reassigned` | type_mismatch (a write where the declaration said read-only) — **asserts a failure** | `set_var`'s refusal |
| `edge_constant_cannot_be_reassigned_from_a_function_body` | nesting — **asserts a failure**; the `set` resolves to no local and would have hit the constant | `set_var`'s refusal |
| `edge_an_import_cannot_rebind_a_constant_with_a_set` | type_mismatch — **asserts a failure**; a module's own `set` is not a way around the refusal | `bind_module_name` (round 1) |
| `edge_a_module_constant_and_a_program_constant_of_one_name_is_refused` | duplicate_missing_keys — **asserts a failure**; the other order is a duplicate declaration | `Vm::constants` |
| `edge_importing_the_same_module_twice_binds_its_names_once` | duplicate_missing_keys — a second import binds once, and the names survive | `Vm::modules` (round 1) |
| `edge_constant_used_before_declaration_is_an_error` | malformed_input — **asserts a failure** twice: the analyzer's report, and the VM's for a body called before the declaration | `deferred_depth` scoping |
| `edge_a_body_declared_above_a_constant_reads_it_when_it_runs` | nesting_recursion — the spec's promise, through the whole pipeline | the constant pre-pass (round 1) |
| `edge_a_call_before_the_declaration_is_a_runtime_error` | malformed_input — **asserts a failure**; a body is later, a call is not | the constant pre-pass |
| `edge_a_body_reading_a_name_nothing_binds_is_still_an_error` | malformed_input — **asserts a failure** ×2; the later-names fallback is narrow | `deferred_depth` scoping |
| `edge_a_module_name_read_before_the_import_is_an_error` | malformed_input — **asserts a failure**; an import binds from where it is written | the later-names scoping |
| `edge_constant_is_shadowed_by_a_local` | boundary — a parameter and a loop variable shadow; the constant is unchanged after both scopes | `declare_assigned` (FINDINGS §8) |
| `constant_syntax_errors_are_reported` | malformed_input — **asserts a failure** ×3: a number and `to` where the name goes, and no `to` at all | the parser arm |
| `constant_round_trips_through_the_formatter` | the formatter's arm | the formatter |

`tests/bytecode_test.rs`, 3 more, all from round 1:

| Test | Edge class covered | Fails without |
|---|---|---|
| `a_constant_compiles_to_its_own_instruction_not_to_a_store` | type_mismatch — the compiled instruction says read-only; the disassembly names it | `Opcode::DeclareConst` in codegen |
| `edge_the_constant_instruction_has_a_byte_of_its_own` | resource_limit / format stability — byte 46, decoding back, and the table ending at the last instruction | the opcode |
| `edge_a_version_2_file_is_refused_because_it_cannot_say_a_name_is_read_only` | malformed_input — **asserts a failure**; a file that cannot say a name is read-only is refused, not guessed at | `FORMAT_VERSION` |

Quota: 20 `#[test]` functions in `constant_test.rs` (floor 3); 14 named `edge_*`
(floor 1); 11 assert a failure is produced (floor 1); 0 new `#[ignore]`,
`// skip`, or `allow(clippy::` suppressions; 0 pre-existing tests failing.

Every test in the file was written to fail without the change. With the keyword
absent, all of them fail on the parse that was the finding. The behaviour the
keyword could not explain was watched fail on its own claim in this round, by
taking each fix back out one at a time:

- removing the analyzer's `import` arm → `import_binds_a_modules_constants` and
  `edge_importing_the_same_module_twice_binds_its_names_once` fail;
- removing the later-names fallback → `edge_a_body_declared_above_a_constant_reads_it_when_it_runs`,
  `edge_a_body_declared_above_an_import_reads_the_modules_names`,
  `edge_a_call_before_the_declaration_is_a_runtime_error` and
  `edge_constant_used_before_declaration_is_an_error` fail;
- emitting `Opcode::Store` for a declaration →
  `a_constant_compiles_to_its_own_instruction_not_to_a_store` fails;
- leaving `FORMAT_VERSION` at 2 → the version-2 refusal test fails;
- `globals.insert` for a module's `set` →
  `edge_an_import_cannot_rebind_a_constant_with_a_set` fails;
- no module record → `edge_importing_the_same_module_twice_binds_its_names_once` fails;
- the old `set` scoping in the analyzer → `edge_constant_is_shadowed_by_a_local` fails.

### Mandatory edge-case matrix

- **empty** — `constant_syntax_errors_are_reported` covers the empty name
  (`constant to 1`) and a declaration with no `to` at all (`constant PI` →
  `Expected To but got Eof`). An empty *value* cannot be written: `to` must be
  followed by an expression, and a file containing only `constant` is refused
  the same way.
- **singleton** — `constant_declares_a_readable_name`: exactly one constant in
  one program.
- **boundary** — `edge_constant_is_shadowed_by_a_local` (a local of the same
  name is the boundary between shadowing and overwriting, checked on both
  sides: the shadow wins inside the scope, the constant survives after it) and
  `edge_constant_declared_again_by_a_second_call_is_refused` (the first call is
  the singleton, the second is the duplicate).
- **out_of_bounds** — N/A. `constant` has no index and no length: a declaration
  is a name and a value, so there is no position to be past. The nearest thing
  is the second declaration, covered as duplicate_missing_keys.
- **type_mismatch** — `edge_constant_cannot_be_reassigned`,
  `edge_constant_cannot_be_reassigned_from_a_function_body` and
  `edge_an_import_cannot_rebind_a_constant_with_a_set`: a write where the
  declaration said the name is read-only, from a `set` in the program, from a
  `set` in a body, and from a module's own `set`. In the file format,
  `a_constant_compiles_to_its_own_instruction_not_to_a_store` is the same
  refusal as an instruction rather than a `STORE`. The *value* is deliberately
  untyped — `constant` accepts any expression and the pipeline's existing type
  checks apply to it unchanged — so `import_binds_a_modules_constants` asserts
  the imported value is still a number and not a name that merely reads like
  one.
- **numeric_boundary** — covered where it applies and N/A beyond that:
  `constant PI to 3.14159` is asserted exactly, and every numeric literal in
  the language goes through `Value::number`, which rejects NaN and infinity, so
  a constant cannot hold a non-finite value. There is no arithmetic *on*
  constants in this change to push a value to 2^53 or i64 overflow.
- **unicode** — N/A. A constant's name is an identifier and its value is an
  ordinary expression, so the lexer's unicode identifier rules and its text
  literals apply unchanged and there is no constant-specific path:
  `lexer_robustness_test` and `span_test` already cover them for every
  construct.
- **nesting_recursion** — `constant_is_read_by_a_function_body`,
  `edge_constant_cannot_be_reassigned_from_a_function_body`,
  `edge_constant_declared_again_by_a_second_call_is_refused`,
  `edge_a_body_declared_above_a_constant_reads_it_when_it_runs` and
  `edge_a_body_declared_above_an_import_reads_the_modules_names`: the two
  orders round-1 added — a body written above the declaration and above the
  `import` reads the name when it is called. A `constant` does not nest: one
  declaration inside a `for`, `if` or `to` body declares the same program-level
  name on every entry, which is what the second-call test pins. Recursion through
  a function that reads constants is bounded by `MAX_CALL_DEPTH`, unchanged by
  this phase.
- **duplicate_missing_keys** — `edge_duplicate_constant_is_refused`,
  `edge_constant_declared_again_by_a_second_call_is_refused`,
  `edge_a_module_constant_and_a_program_constant_of_one_name_is_refused` and
  `edge_importing_the_same_module_twice_binds_its_names_once`. The "missing"
  half is `edge_constant_used_before_declaration_is_an_error`,
  `edge_a_call_before_the_declaration_is_a_runtime_error` and
  `edge_a_module_name_read_before_the_import_is_an_error`.
- **malformed_input** — `constant_syntax_errors_are_reported` (three
  diagnostics), `edge_constant_used_before_declaration_is_an_error`,
  `edge_a_call_before_the_declaration_is_a_runtime_error`,
  `edge_a_body_reading_a_name_nothing_binds_is_still_an_error`,
  `edge_a_module_name_read_before_the_import_is_an_error` and, in the format,
  `edge_a_version_2_file_is_refused_because_it_cannot_say_a_name_is_read_only`.
  The two "is still an error" tests are the other half of the round-1 change:
  they say what the later-names fallback does *not* cover, so a name no `constant`
  and no `import` binds is still the unknown variable it was.
- **resource_limit** — N/A. A `constant` costs one table entry and one
  evaluation of its expression, once; it introduces no loop, no recursion and no
  output. The programs these tests run are still bounded by the existing
  `MAX_STEPS`, `MAX_CALL_DEPTH` and `MAX_ITERATIONS` budgets, and
  `shipped_mathutils_module_runs_and_binds_its_constants` runs the shipped file
  through the default `Vm::new()` limits rather than a raised one.

## Gates

Run on the tree this report describes:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 460 passed, 0 failed, 0 ignored (20 in `constant_test`, 3 more in `bytecode_test`) |
| `cargo test --doc` | 1 passed (`src/bytecode/mod.rs` round-trip example) |
| `./rbops/verify.sh phase-024` | **not run** — there is no `rbops/` in this checkout: `ls: cannot access 'rbops/verify.sh': No such file or directory`. Recorded as not run, not as passed. See FINDINGS §1. |
| `rb run examples/*.rb` (backwards compatibility) | 6/6 exit 0 |
| `rb run modules/*.rb` | 2/2 exit 0 |
| `rb compile modules/MathUtils.rb` | exit 0, `.rbc` written (it could not be compiled before: the file did not parse) |
| `rb lint modules/MathUtils.rb` | exit 0, no findings |
| `rb compile modules/MathUtils.rb` + `rb dis` | exit 0; the disassembly carries `DECLARE_CONST 1 ; line 4: PI`, so a `.rbc` says the name is read-only |
| `rb test` (the `.rb` suite, 232 tests) | 232 passed, 0 failed — including `tests/test_modules.rb`'s "the same module can be imported twice", which the loader's module record now backs |

## Definition of done

- [x] the decision is written into `SPEC.md` (§Modules → Constants, and the
      keyword table) and `docs/GRAMMAR.md` (§1.3 and §4.1)
- [x] `rb run modules/MathUtils.rb` exits 0
- [ ] the entry is removed from `rbops/baseline.json` — **cannot be done from
      here**: `rbops/baseline.json` does not exist in this checkout, and rule 1
      of AGENTS.md forbids creating or editing anything under `rbops/`. See
      FINDINGS §1. The defect the entry recorded is gone.
- [x] edge tests: duplicate constant name
      (`edge_duplicate_constant_is_refused`), constant reassignment
      (`edge_constant_cannot_be_reassigned`, its function-body form and
      `edge_an_import_cannot_rebind_a_constant_with_a_set`), constant used before
      declaration (`edge_constant_used_before_declaration_is_an_error`,
      `edge_a_call_before_the_declaration_is_a_runtime_error`), constant shadowed
      by a local (`edge_constant_is_shadowed_by_a_local`)
- [x] round 1: no test of this phase runs around the analyzer, every review
      finding is fixed, and each fix is pinned by a test that fails without it
      (the "Fails without" column above)

## Invariants touched

- None of the language's. `.rb` is still the extension, `say` still prints,
  `to … end` / `if … end` / `for … end` are untouched, `set x to …` is still
  assignment, `Value`'s variants and `Error`'s variants are unchanged, and
  trailing-comma and `{interp}` syntax is unchanged.
- The `.rbc` format version moved from 2 to 3 in round 1. Every byte value
  already written keeps its number — `DECLARE_CONST` takes the next free one,
  46 — and a version-2 file is refused rather than read, which is the rule
  `docs/BYTECODE.md` states. A `.rbc` compiled by an earlier build is not
  readable by this one; that is the documented consequence of adding an
  instruction, and there is nothing in the repository to migrate.
- The analyzer's scope model for `set` and `constant` now matches the VM's
  (FINDINGS §8). This removes false "unknown variable" reports on programs the
  interpreter runs; it does not remove any report the VM would fail on.
- What did change is the language's surface: `constant` is now a reserved word,
  so a program that used `constant` as a *variable name* — `set constant to 1`
  — is now a parse error. That is what the phase asked for, and it is the same
  trade every added keyword makes. The TextMate grammar, the reserved-keyword
  list in `docs/GRAMMAR.md` §1.3 and the shipped grammar file were all updated
  in this phase so no two of them disagree.

## Known gaps / follow-ups

- **fixed in round 1** — `rb compile` compiles a `constant` to `DECLARE_CONST`
  now, at a byte value of its own, with `FORMAT_VERSION` at 3. What is still
  true is that nothing *runs* a `.rbc`, so the enforcement of the refusal is
  still only the tree-walking VM's; the format no longer loses the distinction.
  FINDINGS §2.
- `import MathUtils` binds the module's `set` and `constant` names, and the
  analyzer now knows them, but a module's *functions* are still not bound to any
  name, so `MathUtils.circle_area(5)` fails at runtime with
  `Unknown function 'MathUtils_circle_area'` rather than working. Pre-existing
  (`src/vm.rs`, the comment in `load_module`), unchanged here. FINDINGS §3.
- SPEC.md's module example uses `module MathUtils … export all … end`, a
  `module` block form that still does not parse (`module` is a keyword with no
  parser arm). Pre-existing spec drift; the shipped module file uses the
  top-level form this phase implements. FINDINGS §4.
- GRAMMAR §4.1's optional `':' type_expression` on a declaration is not
  implemented, for `constant` or for `set`. Pre-existing. FINDINGS §5.
- `rb format --check modules/MathUtils.rb` still reports a reformat: the
  formatter does not preserve a blank line between top-level statements, so it
  reports every `.rb` file in the repository that has one — all six `examples/`
  files included. The two `constant` lines themselves round-trip byte for byte
  (`constant_round_trips_through_the_formatter`). Pre-existing and generic, not
  a `constant` defect; no format-check corpus includes `modules/`. FINDINGS §6.