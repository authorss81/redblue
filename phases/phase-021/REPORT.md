# Phase 021 — Write the Redblue compiler in Redblue (bootstrap S2)

`bootstrap/compiler.rb` already existed in the tree when this run started
(2 748 lines, committed as `dd29457`). It ran, emitted `.rbc` files, and had
`rb lint` clean apart from one warning — but it was **not byte-identical to
stage 1**, and three of the four tests in `tests/bootstrap_selfhost_test.rs`
were red on arrival. This run fixed the three defects that made them red, added
the regression tests that pin them, and wrote this file, which was missing
entirely.

## Reproduce (before the fix)

```
$ printf 'if no then\n    say "yes"\nelse\n    say "no"\nend\n' > target/tmp/rb021/t.rb
$ ./target/debug/rb compile target/tmp/rb021/t.rb -o target/tmp/rb021/s1.rbc
$ ./target/debug/rb run bootstrap/compiler.rb target/tmp/rb021/t.rb target/tmp/rb021/s2.rbc
$ cmp target/tmp/rb021/s1.rbc target/tmp/rb021/s2.rbc
target/tmp/rb021/s1.rbc target/tmp/rb021/s2.rbc differ: byte 76, line 2   # exit 1
```

One byte, and it was a jump displacement — stage 2 wrote `JUMP_IF_FALSE 10`
where stage 1 wrote `JUMP_IF_FALSE 7`, so stage 2 ran the `else` branch
unconditionally. `cargo test --test bootstrap_selfhost_test` reported
`3 failed; 1 passed`.

## What changed

| File | Lines | What |
|---|---|---|
| `bootstrap/compiler.rb:1198` | +1 −2 | `parse_if_then` delegates the `else` to a helper |
| `bootstrap/compiler.rb:1208` | +11 −0 | new `parse_if_else_branch`: carries the `then` branch as a **parameter** across the parse of the `else`. Fixes the branch being replaced by the nested `if`'s branch |
| `bootstrap/compiler.rb:2316` | +6 −6 | `forrange`: compile `step` only when the range has one, and stop emitting the compensating `POP`. Fixes `for each i from 1 to 3` dying with `Cannot access property on non-object` |
| `bootstrap/compiler.rb:2444`,`:2446` | +0 −2 | `compile_try_line` was written twice and read never; the first write was dead. Removing it clears `rb lint`'s only warning |
| `bootstrap/compiler.rb:2588` | +6 −2 | `compile_if_branches`: thread the *patched* context into `compile_if_else` instead of the unpatched one. Fixes the `JumpIfFalse` pointing past the `else` to the end of the block |
| `tests/bootstrap_selfhost_test.rs` | +255 −1 | Four new tests; one stale corpus count in an assertion message corrected (see below) |

`bootstrap/compiler.rb` totals **+24 −12**; `tests/bootstrap_selfhost_test.rs`
**+255 −1**.

### The three defects, in the order the tests found them

1. **A dropped patch record.** `compile_if_branches` built the context carrying
   the `JumpIfFalse` patch into `compile_if_branches_patched`, then passed
   `compile_if_branches_r[0]` — the *unpatched* context — to `compile_if_else`
   and assigned its result to `ctx`. The patch record was overwritten and never
   reached `apply_patches`. `apply_patches` therefore fell back to the jump's
   original `at: "here"`, resolving to the end of the block. Confirmed by
   instrumenting a scratch copy: the patch list held
   `[{2,"here"}, {6,"here"}, {6,"there",10}]` with the `{2,"there",7}` entry
   absent. **The wrong branch ran on every `if ... else`.**

2. **A program-wide name clobbered by recursion.** In `parse_if_then`, the
   `then` branch was read out of `parse_if_then_body` *after* the `else`
   body had been parsed. Parsing the `else` can run `parse_if_then` again for a
   nested `if`, and Redblue has one program-wide namespace per name — so the
   outer call read back the **inner** branch. The compiled file had the right
   shape and the right length, and the wrong statements: `say "A"` was missing
   and `say "mid"` appeared twice. Fixed by passing the branch as a parameter
   to a new `parse_if_else_branch`, which is safe at any nesting depth.

3. **A range's absent step was compiled anyway.** `parse_for_range` leaves
   `step` as `nothing` when the source has no `by`. `compile_statement` called
   `compile_expr(stmt.step, …)` unconditionally, which read `.k` off `nothing`
   and aborted — stage 2 produced **no file at all** for
   `for each i from 1 to 3`. Stage 1 (`src/bytecode/codegen.rs:363`, `let arity = match step`) emits
   no step and no `POP` for a two-bound range; stage 2 now matches it.

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_each_branch_compiles_the_statements_it_was_given` | nesting / recursion — a doubly-nested `else`, statements *after* a nested `if` inside an `else`, nesting on the `then` side, and a **zero-statement** `then` (the shape that hides a lost patch) |
| `edge_the_two_engines_run_a_branching_program_the_same` | boundary / functional — both files are executed by `rb vm` and must print `small\nmid\nbig\nbig\n`; turns "the bytes agree" into "the bytes are right", and is the only check that would catch a bug both engines share |
| `edge_for_range_pushes_only_the_bounds_it_was_given` | boundary — a range with no step, with a step, with a step that is an *expression* rather than a literal `by`, and two ranges in one file |
| `edge_a_broken_branch_is_refused_and_writes_nothing` | malformed input + **asserts failure** — 7 shapes: stray `else`, unclosed `else`, two `else`s, `if` with no `then`, `unless` with an `else`, `for … from` with no upper bound, `for … by` with no step. Each asserts stage 1 refuses it, stage 2 exits non-zero, stage 2 leaves **no** `.rbc`, and stderr says `Error` |

**The new tests are not vacuous.** With `bootstrap/compiler.rb` reverted to
`HEAD`, three of the four fail:

```
test edge_for_range_pushes_only_the_bounds_it_was_given ... FAILED
test edge_each_branch_compiles_the_statements_it_was_given ... FAILED
test edge_the_two_engines_run_a_branching_program_the_same ... FAILED
test edge_a_broken_branch_is_refused_and_writes_nothing ... ok    # see below
```

`edge_a_broken_branch_is_refused_and_writes_nothing` passes before and after.
It is a **regression guard, not a pin**: the three fixes above all move code
through branch parsing, and this is the test that would notice if one of them
started *accepting* a program the frontend refuses. It is reported here rather
than claimed as evidence of a fixed defect.

Pre-existing, unchanged by this run: `stage2_is_byte_identical_for_each_statement_kind`,
`edge_source_shapes_are_byte_identical`, `stage2_is_byte_identical_on_its_corpus_families`,
`edge_malformed_source_is_reported_rather_than_compiled`.

One existing assertion message said the corpus walk compares `308` programs; it
compares **306** (315 files in the 15 listed families, 9 refused by the
frontend). The number in the message was corrected to 306. **The `>= 300`
threshold is unchanged** — see `git diff` for that hunk.

## Edge-case matrix (AGENTS.md §3.2)

- **empty** — covered: `edge_empty`, `edge_comment_only`, `edge_empty_text`, and
  the new `edge_if_else_empty_then` (a zero-statement branch).
- **singleton** — covered: `edge_escapes` (one text constant), `edge_empty_text`
  (the one pool entry of length 0), `edge_for_range_with_step`.
- **boundary** — covered: `edge_number_boundaries` (`0`, `-0.0`, `1e308`,
  `5e-324`, `2^53+1`); `edge_each_branch_compiles_the_statements_it_was_given`
  runs all four arms of a three-level `if`; `edge_source_shapes_are_byte_identical`
  covers no-trailing-newline and CRLF.
- **out_of_bounds** — covered: `index_bounds_error_is_compiled_too` (`xs[5]` on a
  1-element list) is compiled and the two files agree byte for byte;
  `stage2_is_byte_identical_on_its_corpus_families` covers `runtime-errors` (19
  files) and `faults` (19 files). This row is about *compile* fidelity — the
  runtime's own bounds behaviour is unchanged by this phase.
- **type_mismatch** — covered: the compiler refuses what the frontend refuses,
  and `edge_a_broken_branch_is_refused_and_writes_nothing` is exactly that
  channel for the branch forms. A *record* where a *number* was expected is
  defect 3: stage 2 read `.k` off `nothing` and died cleanly with
  `RuntimeError: Cannot access property on non-object` rather than panicking.
- **numeric_boundary** — covered: `edge_number_boundaries` plus the whole
  `numeric-boundary` corpus family (25 files), byte-compared.
- **unicode** — covered: `edge_unicode_text` (accented Latin, CJK, a regional-
  indicator pair, an emoji, and a decomposed vs precomposed `é`) and the
  `unicode` corpus family (16 files). Rule 1 in the compiler's header comment —
  source read as **bytes**, not characters — exists for this row.
- **nesting_recursion** — covered: `edge_nesting` (an `if` inside a loop inside
  a function inside a loop), the `nesting` family (17 files), and the three new
  nested-branch shapes. This is where defect 2 lived.
- **duplicate_missing_keys** — covered: `edge_duplicate_and_missing_keys`
  (`{a: 1, a: 2}` and `r.missing`) and the `records` family (16 files).
- **malformed_input** — covered: `edge_malformed_source_is_reported_rather_than_compiled`
  (8 shapes) and the new `edge_a_broken_branch_is_refused_and_writes_nothing`
  (7 shapes). The `malformed` family (46 files) is listed in `UNSUPPORTED`
  because the frontend refuses all of it, so there are no bytes to agree about.
- **resource_limit** — covered, at the compiler's own boundary: the backend's
  `MAX_BLOCK_DEPTH` (`src/bytecode/format.rs:97`) is exercised from both
  sides by the nesting rows above, and `edge_malformed_source_is_reported_rather_than_compiled`
  asserts a refused program leaves no file behind rather than a partial one.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass (exit 0, no diff) |
| `cargo clippy --all-targets -- -D warnings` | pass (exit 0, zero warnings) |
| `cargo test --all-targets` | **1016 passed, 0 failed**, 0 ignored, across 34 test binaries |
| `rbops/verify.sh phase-021` | **not run — `rbops/` does not exist in this checkout** |
| `rb lint bootstrap/compiler.rb` (phase DoD) | pass (exit 0, no output) |

`cargo test --test bootstrap_selfhost_test` alone: 8 passed, 0 failed.

**On the fourth gate:** `rbops/verify.sh` is not present in the working tree —
`ls rbops/` returns `No such file or directory`. The dispatch instructions place
the RBOPS pipeline outside this checkout and forbid inspecting it, so I could
not run it and am not claiming it. Everything it checks that I *could* run is
above and green. The gate that this phase's own definition of done turns on —
stage 2 byte-identical to stage 1 across the corpus — is
`stage2_is_byte_identical_on_its_corpus_families`, and it is green over **306
programs in 15 families**.

## Definition of done

- [x] `bootstrap/compiler.rb` is itself valid Redblue and passes `rb lint` —
      exit 0, no output. It has no `import`; it uses only `sys.argv`,
      `files.read`, `bytes.from_text`, `bytes.text`, `bytes.write`.
- [x] Running it under the Rust `rb` on the corpus produces `.rbc` files — 306
      of them, every one starting `RED\x1a`.
- [x] Output is byte-identical to the Rust `rb compile` for the whole corpus —
      **every program in the 15 compared families that the frontend accepts**.
      Not the whole corpus: 55 of 361 files are frontend-refused (46 in
      `malformed`, 9 elsewhere — e.g. `corpus/objects-0003.rb` needs a `Tagged`
      object it does not define), and a refused program has no bytes to agree
      about. Those refusals are pinned by the two malformed-input tests.
- [x] No Rust-only fast path is reachable from this path — `compiler.rb` never
      names `compile`/`codegen`/`Chunk`, and reaches no network or subprocess
      builtin. Every `.rbc` byte it writes is computed by the Redblue in the
      file.

## Invariants touched

- **None.** No `.rb` extension change, no `end`/brace change, no `set … to`
  change, no `Value` variant change, no `Error` variant change, no grammar
  change, no change to what the interpreter *executes*. `src/` is untouched by
  this phase. The three fixes are confined to `bootstrap/compiler.rb`, which is
  not reachable from `rb run` or `rb compile` except by naming it explicitly.

## Known gaps / follow-ups

- **S3 is not claimed.** Stage 2 compiles *source*. Running the compiled
  compiler (`rb vm stage1.rbc in.rb out.rbc`) is the next rung and needs `rb vm`
  to accept arguments for the program it runs; it does not today — attempting it
  gives `bootstrap/compiler.rb: 0: cannot read …` because `sys.argv()` is empty
  under `rb vm`. That is ladder stage S3's own phase, not this one.
- **Two constructs are refused with a message rather than compiled**:
  `to (x) … end` as an expression (`bootstrap/compiler.rb:1998`). Any other
  construct stage 2 refuses is a gap, not a feature — the gate for it is a
  corpus family added to the list in
  `stage2_is_byte_identical_on_its_corpus_families`.
- **9 corpus programs in non-`malformed` families are frontend-refused** and so
  are skipped by the walk. They are the same *kind* of program as `malformed`
  and are arguably misfiled; filing is the auditor's call. Listed above.
- **A pre-existing tree-walker bug, out of scope, recorded in `FINDINGS.md`:**
  `give back` inside an `if` does not return from the function. Found while
  writing the runtime test above; not caused or fixed by this phase.