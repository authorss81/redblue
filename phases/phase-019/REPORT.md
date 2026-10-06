# Phase 019 — Bytecode virtual machine (bootstrap S1b)

## Reproduction of the finding

The finding as written — *"No bytecode execution path exists"* — **no longer
reproduces on `main`**. The interrupted attempt had already written one:

```
$ ./target/release/rb compile target/tmp/demo.rb
Compiled target/tmp/demo.rb -> target/tmp/demo.rbc
$ ./target/release/rb vm target/tmp/demo.rbc
total is 4
6.28318
```

Stale-phase rule not triggered: there was real work to finish, and it was not
finished. What reproduced instead was that **the checkpoint merge did not
compile**, so the delivered phase was worth nothing:

```
$ cargo check --all-targets --message-format short
src/vm.rs:364:29: error[E0425]: cannot find value `name` in this scope
src/bytecode/codegen.rs:196:36: error[E0599]: no variant ... `DeclareConst`
src/vm.rs:731:26: error[E0061]: this method takes 1 argument but 2 were supplied
error: could not compile `redblue` (lib) due to 9 previous errors
```

and, once that was fixed, a genuine behavioural divergence between the two VMs:

```
$ printf 'import MathUtils\nsay PI\n' > x.rb
$ rb run x.rb          ->  3.14159
$ rb vm  x.rbc         ->  3.141592653589793
```

### This run: the resumed tree had two unresolved merge conflicts

The tree arrived with `<<<<<<<` markers still on disk in
`tests/bytecode_vm_test.rs` and `phases/phase-019/FINDINGS.md`, and **no
REPORT.md at all** — which alone fails the phase. Resolved first, keeping both
sides:

| Conflict | Resolution |
|---|---|
| `tests/bytecode_vm_test.rs:121` — the `NOT_COMPARABLE` doc and list | kept the recovery branch's documented intent (*named, not pattern-matched*, and checked for staleness) and dropped `examples/random.rb` from the list, because `git log --all -- examples/random.rb` is empty — that file has never existed on any branch, so the name was a hole in the corpus that looked like coverage. `corpus()` now asserts every name in `NOT_COMPARABLE` is still a file, which is the check the doc had promised but the code did not do. |
| `tests/bytecode_vm_test.rs:2088` — the tail | the recovery branch's file ends at `edge_both_vms_answer_the_same_at_a_nesting_depth_neither_overflows` (`git show origin/rbops-recovery/phase-019:tests/bytecode_vm_test.rs \| wc -l` → 2080), i.e. it contributes nothing after the common tail. Nothing of it was lost: the newer side's nine constants/modules tests are kept verbatim. |
| `phases/phase-019/FINDINGS.md` | both sides are distinct, verified findings, so both are kept. The newer side is §1–§8 (the merge and the round-1 review); the recovery side's §1–§8 became §9–§16 under a `Carried from the recovery branch` heading. Five `file:line` citations in the recovered half had drifted on the newer tree and were corrected against the files as they are now (`src/vm.rs:529-536`→`680-687`, `src/bytecode/vm.rs:1430/1436`→`1495/1501`, `src/value.rs:169`→`174`, `src/runtime.rs:248`→`263`); the other eleven were re-read against the file each names and are accurate, and §1's `src/vm.rs:731` — a line inside quoted pre-fix compiler output that no longer holds that code — now says so and points at `load_module` (`src/vm.rs:378`). §16's `examples/random.rb` was corrected the same way. |

Re-verified on the resolved tree, with `target/debug/rb` — the three defects the
recovered findings describe are fixed, not merely claimed:

```
$ printf 'import MathUtils\nsay PI\n' > a.rb
  rb run a.rb -> 3.14159        rb vm a.rbc -> 3.14159      (equal)
$ printf 'import MathUtils\nimport MathUtils\nsay PI\n' > b.rb
  rb run b.rb -> 3.14159        rb vm b.rbc -> 3.14159      (equal)
$ nested.rb (inner try caught inside an outer try)
  rb run nested.rb -> acfbF     rb vm nested.rbc -> acfbF   (equal)
```

## Round 1 — the review findings

The first pass fixed that divergence and introduced three of its own, which the
round-1 review found. All seven findings are fixed here; FINDINGS.md §7 and §8
carry each one with the command that reproduced it, the mechanism, and the test
that now pins it. In short:

| Severity | Finding | Fix |
|---|---|---|
| BLOCKER | a handled inner `try` popped the *enclosing* handler, running its `finally` early and leaving its region unprotected | resume *past* the region end on the failure path |
| BLOCKER | every `NOP` closed a region, on the success path and in the failure path's scan | `END_TRY_MARKER` on the operand: only that value is a region end, so a filler closes nothing |
| BLOCKER | bytecode `import` had no already-loaded set, so a second import re-ran the module and failed on its own `constant` | `BytecodeVm::modules`, recorded before the module runs |
| MAJOR | `store` refused a loop variable for shadowing a constant, contradicting its own comment and the tree-walker | the refusal moves to the plain-binding path only |
| MAJOR | a test claimed the compiler emits a filler `NOP`; it does not, so the test asserted nothing | replaced by a hand-built chunk with a filler inside the region, on both paths |
| MINOR | `import`'s doc said "only the `set` statements" while the code kept `Constant` too | the doc says both |
| MINOR | the tree-walker read and parsed a module file twice, and the second read could fail after partial bindings | `module_bindings` takes the parsed `Program`; the file is read once |

Nothing was weakened to get there: no test was deleted or skipped, no gate
loosened, and the frozen byte table keeps `DeclareConst` on byte 46.

## What changed

| File | Lines | What |
|---|---|---|
| `src/bytecode/opcode.rs` | +43 −9 | restored `DeclareConst` to the table (the merge had dropped it while `codegen.rs` still emitted it); gave the reserved `Nop` an `END_TRY_MARKER` operand that makes it the end-of-try marker, so a filler `Nop` stays a filler |
| `src/bytecode/vm.rs` | +166 −72 | `DECLARE_CONST` executes, with a read-only name set and a `Cannot assign to constant 'NAME'` refusal matching the tree-walker; the marked `NOP` closes a protected region and runs its `finally`, and a handled failure resumes *past* it; a module is loaded once; the constant refusal is skipped on a loop variable's own binding |
| `src/bytecode/codegen.rs` | +7 −9 | emits the marked `Nop` instead of the `EndTry` byte the frozen table cannot hold |
| `src/runtime.rs` | +33 −18 | `module_program` and `module_bindings` — the module loader, shared so both VMs select the same declarations; `module_bindings` reads a parsed `Program` so a module file is read once, and returns the `constant`/`set` distinction so each caller can use its own refusal |
| `src/bytecode/disasm.rs` | +8 −1 | says `end of a protected region` on a marked `NOP`, so `rb dis` shows which one closes a region |
| `docs/BYTECODE.md` | +19 −1 | the `NOP` row and a paragraph on `END_TRY_MARKER`: it is no longer "never emitted by this compiler", and the filler is no longer the same instruction |
| `src/vm.rs` | +48 −16 | repaired the merge's half-written `load_module`; resolves the module path once via `module_path` instead of reading the source and re-deriving the path; binds a module's `constant` through `bind_constant` so an imported constant is read-only; parses the module once |
| `tests/bytecode_vm_test.rs` | +319 −4 | the tests below |
| `tests/bytecode_test.rs` | +63 −1 | one test: what a `try` compiles to, on the format side |

Line counts are `git diff --numstat e97983a -- src/ docs/ tests/`, which is the
merge this resumed run sits on top of; `must_touch: ["src/"]` is satisfied by
seven changed files under `src/` (`opcode.rs`, `codegen.rs`, `bytecode/vm.rs`,
`disasm.rs`, `mod.rs`, `runtime.rs`, `vm.rs`). FINDINGS §7 and §8 give the
round-1 split.

## Tests added

| Test | Edge class covered |
|---|---|
| `a_constant_binds_and_is_read_only_on_both_vms` | duplicate/missing keys — a `constant` binds, reads back, and refuses a later `set` |
| `edge_a_constant_declared_twice_is_refused_and_the_first_value_survives` | duplicate keys — **asserts a failure is produced**: `Constant 'A' is already declared` |
| `edge_a_modules_constant_binds_rather_than_falling_through_to_the_builtin` | **asserts a failure would be produced** — the regression this phase found; fails with `3.141592653589793` without the fix |
| `edge_a_modules_constant_cannot_be_rebound_by_the_importing_program` | **asserts a failure is produced** — an imported constant is read-only to the importer |
| `edge_a_filler_nop_does_not_close_a_protected_region` | resource/state — hand-built chunks with a filler `NOP` inside a protected region, on both paths |
| `edge_a_finally_runs_exactly_once_on_each_path` | resource/state — a `finally` runs once on the success path and once on the caught-failure path, so it cannot run twice |
| `edge_a_handled_inner_try_leaves_the_enclosing_region_protected` | resource/state — a handled inner `try` leaves the enclosing region protected, with its `finally` still owed to the end |
| `edge_importing_the_same_module_twice_loads_it_once` | duplicate keys — the second `import` of a module is a no-op |
| `edge_a_loop_variable_shadows_a_constant_instead_of_being_refused` | duplicate keys — a loop variable shadows a constant rather than being refused |
| `a_trys_region_ends_at_one_marked_nop_and_writes_no_other_filler` (format) | malformed_input — the region end is a marked `NOP`, the marker is unreachable as an index, and the compiler writes no filler |
| `corpus()` — the staleness assertion this run added | resource/state — every name in `NOT_COMPARABLE` is asserted to still be a file, so an exclusion list cannot quietly name a program that is not there (it is what caught the inherited `examples/random.rb` entry) |

Red-before-green was watched for all of them by the run that wrote them.
Reverting the round-1 fixes one at a time (`tests/bytecode_vm_test.rs`, 29 other
tests still passing) failed exactly these four:

| Test | Failure without the fix |
|---|---|
| `edge_a_filler_nop_does_not_close_a_protected_region` | `left: ["ftn"], right: ["tnf"]` on the success path; on the failure path `Err(bytecode left the operand stack empty…)` against `["tc"]` |
| `edge_a_handled_inner_try_leaves_the_enclosing_region_protected` | `left: ["acfbF"], right: ["acfFb"]`, and the second program loses the enclosing catch |
| `edge_importing_the_same_module_twice_loads_it_once` | `RuntimeError: Constant 'PI' is already declared` |
| `edge_a_loop_variable_shadows_a_constant_instead_of_being_refused` | `RuntimeError: Cannot assign to constant 'X'` |

This run did not re-run that revert experiment — it did not change any of those
fixes. What it did instead is re-verify on the resolved tree that each of the
four is still fixed, from the CLI, as shown above: both VMs print `acfbF` for
the nested-`try` program and `3.14159` for the double-imported module.

The differential corpus this phase required was already in place and is green:
`a_corpus_of_programs_runs_identically_on_both_vms` runs **347 programs** —
20 read from disk (the 21 `.rb` files under `examples/`, `modules/` and `tests/`
less `examples/time.rb`) plus 327 generated ones — and asserts
`>= MINIMUM_CORPUS` (200) so the floor cannot be quietly lowered. The count was
measured by temporarily raising `MINIMUM_CORPUS` until the assertion reported it,
then restoring the constant to 200; the tree as delivered reads
`const MINIMUM_CORPUS: usize = 200;`.

## Edge-case matrix

| Row | Covered? |
|---|---|
| empty / zero / nothing | covered — `edge_a_decoded_chunk_runs_identically_to_the_compiled_one` over the whole corpus, which contains empty-text, empty-list and `nothing` cases |
| singleton / boundary | covered — index `0` and `len-1` in the corpus's index group |
| out_of_bounds | covered — `edge_an_out_of_bounds_index_is_a_clean_failure` pins the exact message and asserts both VMs refuse it identically |
| type_mismatch | covered — the corpus's type-mismatch group, including record-where-list-expected |
| numeric_boundary | covered — the corpus's numeric group plus `edge_the_iteration_cap_names_the_kind_of_loop_that_hit_it` |
| unicode | covered — the corpus's unicode group: combining marks, emoji length, CJK, RTL |
| nesting_recursion | covered — `edge_both_vms_stop_unbounded_recursion_at_the_same_depth`, `edge_both_vms_answer_the_same_at_a_nesting_depth_neither_overflows` |
| duplicate_missing_keys | covered — the two `constant` tests above, plus a module imported twice and a loop variable that shadows a constant |
| malformed_input | covered — `edge_rb_vm_refuses_a_file_that_is_not_bytecode`, `edge_rb_vm_refuses_a_source_file`, `edge_bytecode_that_asks_for_a_value_it_never_pushed`, `edge_a_jump_past_the_end_of_a_block_does_not_read_past_the_code`, `a_trys_region_ends_at_one_marked_nop_and_writes_no_other_filler` |
| resource_limit | covered — `edge_both_vms_enforce_the_same_step_budget`, `edge_the_step_budget_is_a_configurable_limit_not_a_hard_coded_one`, the depth tests, `edge_a_caught_depth_failure_leaves_the_bytecode_vm_usable`, `edge_a_finally_runs_exactly_once_on_each_path`, `edge_a_filler_nop_does_not_close_a_protected_region`, `edge_a_handled_inner_try_leaves_the_enclosing_region_protected` |

Test-file facts, counted on the delivered tree: `tests/bytecode_vm_test.rs`
holds **33 `#[test]` functions, 28 of them named `edge_*`**, nine of them added
by this phase's diff. `tests/bytecode_test.rs` adds one. **Ten** tests assert a
failure is produced — `a_constant_binds_and_is_read_only_on_both_vms`,
`edge_a_constant_declared_twice_is_refused_and_the_first_value_survives`,
`edge_a_frame_cannot_pop_below_its_own_stack_base`,
`edge_a_modules_constant_cannot_be_rebound_by_the_importing_program`,
`edge_an_out_of_bounds_index_is_a_clean_failure`,
`edge_both_vms_enforce_the_same_step_budget`,
`edge_both_vms_stop_unbounded_recursion_at_the_same_depth`,
`edge_bytecode_that_asks_for_a_value_it_never_pushed_is_refused`,
`edge_the_iteration_cap_names_the_kind_of_loop_that_hit_it`,
`edge_the_step_budget_is_a_configurable_limit_not_a_hard_coded_one` — plus
`edge_the_two_vms_report_the_same_failure_for_every_corpus_program`, which
asserts the two VMs reach the *same* outcome — the same value, or the same error
kind and message — on every one of the 347 corpus programs, including the
malformed ones that fail in the frontend before either VM runs, and
`edge_rb_vm_refuses_a_file_that_is_not_bytecode` /
`edge_rb_vm_refuses_a_source_file`, which assert a non-zero exit from `rb vm`.
Zero `#[ignore]`, zero `// skip`, zero `allow(clippy::` added by this phase.

## Gates

Run in this session, on the resolved tree, in this order:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — no diff |
| `cargo clippy --all-targets -- -D warnings` | **pass** — `Finished dev profile`, 0 warnings |
| `cargo test --all-targets` | **494 passed, 0 failed, 0 ignored**, summed over 22 test binaries (one of which contains no tests); `cargo test --test bytecode_vm_test` alone: 33 passed, 0 failed; `cargo test --doc`: 1 passed |
| `./rbops/verify.sh phase-019` | **not run — `rbops/verify.sh` does not exist in this checkout** (`/bin/bash: line 1: ./rbops/verify.sh: No such file or directory`, exit 127) |

On the fourth gate, honestly and without substituting for it: the file is
absent, and the task instructions state that the pipeline which dispatched this
phase lives outside the project checkout, so I did not go looking for it
elsewhere. The three gates that do exist were run and are green. In their place
I ran the examples the contract names as the backwards-compatibility check,
comparing `rb run` against `rb vm`:

```
$ for f in examples/*.rb modules/*.rb; do
      rb compile $f -o chk.rbc && diff <(rb run $f) <(rb vm chk.rbc); done
same  examples/files.rb
same  examples/fizzbuzz.rb
same  examples/formats.rb
same  examples/hello.rb
same  examples/test_arithmetic.rb
DIFF  examples/time.rb
same  modules/MathUtils.rb
same  modules/SuiteKit.rb
```

`examples/time.rb` is the one program that differs, and it differs between two
runs of the *same* VM (`diff <(rb run examples/time.rb) <(rb run
examples/time.rb)` → differs), because it prints the wall clock. That is why it
is the sole entry in `NOT_COMPARABLE` and why that entry is excluded from the
differential test; `redblue_suite_test.rs` and the examples run still execute it.

## Invariants touched

- None of the language surface in the AGENTS.md §2 invariants table. `.rb` extension,
  `to … end` / `if … end` / `for … end`, `set x to`, `say`, the `Value` variants
  and the `Error` variants are all unchanged, and the pre-existing tree-walking
  tests still pass.
- `docs/BYTECODE.md:172` — the `NOP` row, plus a paragraph on `END_TRY_MARKER`
  after the reserved-operand list. The byte table is unchanged (no renumbering,
  no version bump), but the `NOP` row read "a filler, never emitted by this
  compiler", which became false: the compiler now emits it once per `try` to end
  the protected region, and round 1 made the operand — not the byte — what
  distinguishes that emission from the filler. A format doc that misdescribes
  the format is a defect, so both were corrected. See the table row above for
  what changed.

## Known gaps / follow-ups

- The end-of-try marker has no mnemonic of its own — the byte table is frozen by
  a pre-existing test and by `docs/BYTECODE.md`. It rides on `Nop`'s operand as
  `END_TRY_MARKER`, which `rb dis` prints as `end of a protected region`.
  FINDINGS.md §2 and §7b explain what a future format-version phase would have to
  change.
- A module's `to … end` functions remain unreachable by name. Pre-existing,
  unchanged, both VMs agree → FINDINGS.md §5 and §14.
- `Value`'s nesting depth is still a native-stack limit → FINDINGS.md §6 and §10.
- `break` and `skip` are no-ops in both VMs → FINDINGS.md §9.
- `rbops/verify.sh` was not run because it is not present → the Gates table above.
- Eight further findings that belong to other phases — the unlexable comparison
  operators, top-level `constant`, `stdlib::builtin_function`, the `import` name
  binding — are recorded in FINDINGS.md §11–§16 for the auditor to promote.