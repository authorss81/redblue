# Phase 022 — Self-hosting fixed point (bootstrap S3)

## Status: DONE. `stage1.rbc == stage2.rbc`, byte for byte, with no limit
## overridden.

## What changed

| File | Lines | What |
|---|---|---|
| `bootstrap/compiler.rb` | +99 −44 | the encoder, the lexer's token list and the patcher's output list are grown with `append` on a named global instead of `set … to push(…)`; `compile_while` passes its `JumpIfFalse` slot as a parameter |
| `src/interpreter.rs` | +30 −3 | `MAX_STEPS` default 10 000 000 → 32 000 000, with the measurement that justifies it; two pre-existing clippy one-liners |
| `src/runtime.rs` | +1 −1 | one pre-existing clippy lifetime elision, in `append_target` |
| `tests/bootstrap_selfhost_test.rs` | +393 −0 | four tests: the fixed point, three-run determinism, the step budget, and the nested `while` on its own |
| `docs/BOOTSTRAP.md` | new | the ladder, the one rule, the two defects S3 found, and how to run it by hand |
| `phases/phase-022/FINDINGS.md` | rewritten | what was actually slow, what was measured, and four follow-ups that are not this phase's |

The three functional changes, in the order they were needed:

1. **`encode`/`encode_block` build the file with `append`, not `cat`.** `cat`
   copies the list it is given, so a `.rbc` of *n* bytes cost *n* copies of a
   list that grew to *n* elements. Compiling `bootstrap/compiler.rb` means
   encoding 162 298 bytes; that alone was most of six minutes. `put` writes each
   byte through `append("G_BYTES", …)`, which grows the named list in place.
   `encode` was 24.0 s of a 27.2 s run at 800 statements; it is under a second
   now, and the run is 3.5 s.
2. **`lex` appends to `G_TOKENS`.** Same shape, on the token list: 18 251 tokens
   for this file. Lexing the compiler went from 8 s to 2 s.
3. **`compile_while` passes its jump slot as a parameter.** It held the slot of
   its own `JumpIfFalse` in a name across the call that compiled its body, and a
   Redblue function's names are program-wide — so a `while` inside the body took
   the outer loop's slot and patched the wrong jump. This is the defect the
   fixed point found in itself; see "Known gaps" for what it produced.

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_the_self_hosted_compiler_compiles_itself_byte_identically` | **the fixed point** — `stage1.rbc == stage2.rbc`, and stage 2's file decodes to stage 1's chunk |
| `edge_three_consecutive_self_compilations_are_byte_identical` | resource / determinism — three runs, three identical files |
| `edge_the_self_compilation_still_obeys_the_step_budget` | **resource_limit** — a starved run fails, names the budget, and writes nothing |
| `edge_a_while_loop_nested_in_another_patches_its_own_jump` | nesting_recursion (2 and 3 deep), boundary (empty bodies), singleton (a lone loop filling its block), branch nesting (`while` in an `else`) — seven programs, each compared byte for byte |

**Test quota: met.** 4 new `#[test]` functions (floor is 3), 4 named `edge_*`
(floor is 1), and a failure-asserting test
(`edge_the_self_compilation_still_obeys_the_step_budget`, which requires a
non-zero exit, `Step budget` on stderr, and no output file). Zero new
`#[ignore]`, zero `// skip`, zero `allow(clippy::…)`.

**Red before green.** `edge_a_while_loop_nested_in_another_patches_its_own_jump`
was written first and failed on the `nested_while` shape — stage 1 wrote operand
`35`, stage 2 wrote `30`, which is the mis-patched jump. It passed only after
`compile_while` was changed.
`edge_the_self_hosted_compiler_compiles_itself_byte_identically` was then
written and watched fail with

```
Error: RuntimeError: Step budget of 10000000 reached before the program finished
```

before `MAX_STEPS` was raised.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — exit 0, no diff |
| `cargo clippy --all-targets -- -D warnings` | **pass** — exit 0, zero warnings |
| `cargo test --all-targets` | **pass** — 1040 passed, 0 failed, 0 ignored |
| `./rbops/verify.sh phase-022` | **not run — `rbops/` is not in this checkout** |

`ls rbops/` → `No such file or directory`. The dispatch instructions place the
RBOPS pipeline outside this checkout and forbid inspecting it, so the fourth gate
could not be run and is not claimed.

Baseline on the commit this phase started from: **1036** passing. Now **1040** —
exactly the four tests above, with zero pre-existing tests newly failing and
zero newly ignored.

**The fourth gate was red before this phase and had to be made runnable.**
`cargo clippy --all-targets -- -D warnings` reported four errors on a clean stash
of the starting commit: `src/runtime.rs:294` (`needless_lifetimes`) and
`src/interpreter.rs:1580`/`1587` (`explicit_auto_deref`, `cloned_ref_to_slice_refs`).
All four are one-liners in `append_target` and `apply_map`, none is in the
changed logic, and all four are fixed here so the gate could be run at all.
This is flagged because it is not this phase's work and must not be reviewed as
it — see `FINDINGS.md` §4.

## Test-requirement matrix (AGENTS.md §3.2)

| Row | Covered by | |
|---|---|---|
| **empty** | `nested_while_with_empty_bodies`, `edge_source_shapes_are_byte_identical`'s `edge_empty`, `edge_stage3_is_byte_identical_on_the_awkward_shapes`'s `empty`. An empty file produces an empty pool, an empty token list and an empty `G_BYTES`, and is byte-compared. | covered |
| **singleton** | `lone_while_filling_its_block` — a block containing one loop and one statement, where "one past the loop" and "end of the block" are two different offsets. Also `edge_source_shapes_are_byte_identical`'s `edge_comment_only` and `edge_empty_text`. | covered |
| **boundary** | `nested_while_with_empty_bodies`: the condition's `JumpIfFalse` sits one instruction from the jump back to the top, so a patch that is off by one instruction is visible. Also the outer loop's exit when the inner loop is the outer loop's **last** statement, in all six nested shapes. | covered |
| **out_of_bounds** | The jump operand *is* the out-of-bounds case here: before the fix, stage 2 wrote `222` into a block of 222 instructions — a target one past the end. It is asserted by the byte comparison, and the new operand is asserted to land on a real instruction by `edge_the_two_engines_run_a_branching_program_the_same` running the file. **Not** a list index: the change touches no list indexing. | covered |
| **type_mismatch** | **N/A** — no signature, no value type, no type check changed. `append` rejects a name that is not bound to a list with `Cannot append to 'x': it holds a …, not a list`, and it was not changed; that refusal is pre-existing and covered in `tests/loop_bounds_test.rs`. | N/A |
| **numeric_boundary** | **N/A** — no arithmetic changed. `u32_le` still writes four little-endian bytes of `v` by exact division by 256, untouched. The numbers in the compiler's own constant pool (every boundary value `tests/numeric_edge_test.rs` pins) go through it and are byte-compared by the fixed point. | N/A |
| **unicode** | `edge_source_shapes_are_byte_identical`'s `edge_unicode_text` and `edge_escapes`, and the `unicode` corpus family: every one byte-compared through the changed `lex`. The changed `lex` accumulates *tokens*, not characters, so text is not a boundary of the new code — but it is the boundary of `byte_at`/`char_width`, which were not changed. | covered |
| **nesting_recursion** | `edge_a_while_loop_nested_in_another_patches_its_own_jump`: loops nested **three** deep (`three_nested_while`), through a `for each` (`while_inside_for_each`), and through an `else` branch (`while_in_else_containing_while`). The self-compilation is itself the four-deep case. | covered |
| **duplicate_missing_keys** | **N/A** — no record key is read or written differently. `G_POOL`, `G_INTERN`, `G_TOKENS` and `G_PATCHED` hold records whose keys are fixed at each construction site; the duplicate-key behaviour is the parser's and is untouched (`edge_duplicate_and_missing_keys` in `edge_source_shapes_are_byte_identical` still passes). | N/A |
| **malformed_input** | `edge_malformed_source_is_reported_rather_than_compiled` (8 shapes) and `edge_stage3_refuses_every_malformed_corpus_program` (all 46), both unchanged and both green — the `lex` change is in the path every refusal travels. An unterminated `while` is refused by the frontend and stage 2 refuses it the same way. | covered |
| **resource_limit** | `edge_the_self_compilation_still_obeys_the_step_budget` — `REDBLUE_MAX_STEPS=1000`, non-zero exit, `Step budget` on stderr, **no output file**. Plus the fixed point itself: 16 062 500 steps against a 32 000 000 default, which is the measurement the constant's doc comment records. | covered |

## Definition of done

- [x] **`rb vm stage1.rbc → stage2.rbc`, byte-identical to `stage1.rbc`** — met,
      with the default limits and no environment override. Verified by the test
      above and by hand:

      ```bash
      rb compile bootstrap/compiler.rb -o stage1.rbc        # 0.02 s
      rb vm stage1.rbc bootstrap/compiler.rb stage2.rbc     # 13.4 s
      cmp stage1.rbc stage2.rbc                              # identical, 162 298 bytes
      ```

      and stage 2's file decodes to stage 1's chunk, so it is a `.rbc` and not
      something the right length. The corpus side of the box — stage 2 ==
      stage 1 for every corpus program — is `edge_stage3_is_byte_identical_on_every_corpus_program`,
      green and untouched.
- [x] **the fixed-point check runs in CI on every push** — the two tests are in
      `tests/bootstrap_selfhost_test.rs`, so `.github/workflows/ci.yml`'s
      `cargo test` step runs them on every push to `main` and every pull request.
      (That workflow was read, not modified — it is not this phase's to touch.)
      Three self-compilations run at once, so the added cost is 3 m 25 s, not
      three times that.
- [x] **three consecutive runs produce identical bytes** —
      `edge_three_consecutive_self_compilations_are_byte_identical`, and by hand:
      13.39 s / 13.45 s / 13.40 s, three files, `cmp` clean.
- [x] **documented in `docs/BOOTSTRAP.md`** — the ladder, the one rule, why S3 is
      the hard rung, the two defects it found, `push` copies and why, the table of
      what checks what, and how to run and diff it by hand.

## Invariants touched

- **None of the language surface.** No `.rb` extension, no `end`-vs-brace, no
  `set … to`, no `Value` variant, no `Error` variant, no grammar change, no
  builtin added or removed, no `FORMAT_VERSION` bump, no `Value` payload change.
- **One published default moved: `MAX_STEPS` 10 000 000 → 32 000 000.** This is a
  resource guard, not a gate, and it is raised because the repository's own
  compiler — a bounded program that terminates in 13 s — needs 16 062 500 steps.
  It is still a bound, `MAX_ITERATIONS` still caps a single loop at a million, and
  `REDBLUE_MAX_STEPS` still lowers either. The doc comment records the
  measurement and the test that pins it.
- **`bootstrap/compiler.rb`'s output bytes are unchanged** for every program in
  `corpus/`, `examples/`, `modules/` and the hand-written shape list — the
  encoding and lexing changes are a cost change, not a layout change, and every
  one of those tests compares bytes.

## Known gaps / follow-ups

- **`intern` is a linear scan of the whole intern table per emitted name.** Most
  of the 6 s the code generator now spends. A hash map in the language would fix
  it; that is a language change, not this phase's. → `FINDINGS.md` §4.
- **`docs/BYTECODE.md:3` says "Format version: 4"; `FORMAT_VERSION` is 5.** A
  stale header line from phase-020, in another phase's area. Not fixed here.
  → `FINDINGS.md` §4.
- **`bootstrap/compiler.rb` does not satisfy `rb format --check`.** It did not
  before this phase either. If a formatter phase takes `bootstrap/` into scope,
  the fixed point has to be re-verified afterwards. → `FINDINGS.md` §4.
- **The self-compilation costs 3 m 25 s of `cargo test`.** A test build runs the
  interpreter about 11× slower than a release build. Affordable now; it is the
  largest single cost in the suite and it grows with the compiler.
- **The step count has about 2× headroom.** If a future phase doubles the
  compiler's step count, the fixed-point test fails with
  `Step budget of 32000000 reached` — loudly, and naming the constant.