# Phase 021 — Write the Redblue compiler in Redblue (bootstrap S2)

## The stated finding does not reproduce

The evidence line is `The self-hosted path does not exist yet.` It does not
reproduce. `bootstrap/compiler.rb` exists (2 748 lines), runs under the Rust
`rb`, passes `rb lint` with no output, and `cargo test --test bootstrap_selfhost_test`
was **green on arrival** at 8 passed / 0 failed.

Re-verified independently of that suite before changing anything:

| Check | Result |
|---|---|
| Whole corpus, all 16 families | 308 programs stage 1 accepts → **308 byte-identical, 0 differ** |
| `examples/*.rb` + `modules/*.rb` | 8 of 8 byte-identical |
| `tests/*.rb` — large real programs | 5 of 5 byte-identical |
| 179 `f64` values — random 64-bit patterns + subnormal boundaries | **179 byte-identical, 0 differ** |
| 19 text / unicode / escape cases | **19 byte-identical, 0 differ** |
| Block nesting at 63 / 64 / 65 levels | 63 and 64 identical; 65 refused by both |
| `rb lint bootstrap/compiler.rb` | exit 0, no output |

Per AGENTS.md, a stale finding must not be "fixed" by inventing a change. So
this run fixed the one real defect found next to it, which is on the same rung
and blocks the next one.

## Reproduce (before the fix)

```
$ rb compile target/tmp/rb021/s3/in.rb -o s1.rbc          # a 12-byte program
$ rb vm s1.rbc first second
Redblue v0.1.0 - programming language as readable as plain English
Usage:
  rb              Start interactive REPL
  ... 24 lines of help ...
$ echo $?
0
```

`rb vm` printed the **help text and exited 0**, having run nothing. It has no arm
for a program's own arguments: `rb run` takes them (`src/lib.rs:248`), `rb vm`
did not, so `rb vm a.rbc b c d` matched no arm and fell through to
`_ => print_help()` — which returned normally instead of exiting non-zero.

Consequences, both real:

- A program run as bytecode could not read `sys.argv()` at all. The arguments
  were discarded and the run reported success.
- The next ladder rung — `rb vm stage1.rbc in.rb out.rbc`, S3 — was not
  expressible.

## What changed

| File | Lines | What |
|---|---|---|
| `src/lib.rs:248` | +12 −6 | the `rb run [args...]` arm becomes `rb run`/`rb vm`, so both hand everything after the path to `sys.argv()`; its `n >= 4` guard becomes `n >= 3` so `rb run file.rb` reaches this arm instead of the bare-`4` one |
| `src/lib.rs:292` | +5 −1 | the final `_ =>` arm exits 1 instead of returning, so a usage error is visible to a shell checking `$?` |
| `src/lib.rs:345` | +2 −4 | `vm_command` returns bare messages; it now has two callers and both add the `Error: ` prefix, so one of them was emitting `Error: Error:` |
| `tests/bootstrap_selfhost_test.rs` | +139 −0 | two `edge_*` tests, a `VmRun` helper and a `stage1_file` helper |

`src/lib.rs` totals **+19 −11**. `bootstrap/compiler.rb` is **unchanged** —
there was nothing wrong with it.

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_the_bytecode_vm_hands_arguments_to_the_program_it_runs` | **empty** (0 args) / **singleton** (1 arg) / **boundary** (3 args, so a partial read that takes only the first is caught) |
| `edge_the_bytecode_vm_reports_a_bad_invocation_as_a_failure` | **malformed input** and **asserts a failure**: non-bytecode path, missing file, and a 4-argument invocation — all three must exit non-zero |

**Both were watched failing first, for the right reason.** The first failed with
`rb vm on a program that asks for no arguments failed: Error: RuntimeError: Index
0 is out of bounds` — the argument channel was absent. The second failed by
printing 24 lines of help text into the assertion message: it exited 0.

Unchanged by this run: `stage2_is_byte_identical_for_each_statement_kind`,
`edge_source_shapes_are_byte_identical`, `edge_malformed_source_is_reported_rather_than_compiled`,
`stage2_is_byte_identical_on_its_corpus_families`, `edge_each_branch_compiles_the_statements_it_was_given`,
`edge_the_two_engines_run_a_branching_program_the_same`, `edge_for_range_pushes_only_the_bounds_it_was_given`,
`edge_a_broken_branch_is_refused_and_writes_nothing`.

## Edge-case matrix (AGENTS.md §3.2)

- **empty** — covered: 0 arguments is the first case of
  `edge_the_bytecode_vm_hands_arguments_to_the_program_it_runs`. It is the
  boundary that matters, because the fix moved the arm's guard from `n >= 4` to
  `n >= 3` and 0 arguments is what that guard has to keep working.
- **singleton** — covered: exactly 1 argument, the shape `rb run` already had and
  `rb vm` did not.
- **boundary** — covered: 3 arguments, so an implementation that read only the
  first would fail; and the guard move itself, which is the code's boundary.
- **out_of_bounds** — **N/A.** This change adds no index, slice or lookup and
  moves no bounds check. `sys.argv()` is already an empty list when there are no
  arguments, and the test program's own `vm_args[0]` read is guarded by
  `length(vm_args) is 0`. Index behaviour is unchanged.
- **type_mismatch** — **N/A.** No value crosses a type boundary here; `args` is
  `Vec<String>` before and after. `vm_command`'s error strings are the only
  strings that changed shape, and both callers' assertions cover them.
- **numeric_boundary** — **N/A.** No arithmetic. (The compiler's *own* numeric
  boundary handling — 179 `f64` values, subnormals included — was re-verified
  above as byte-identical, but this phase's diff does not touch it.)
- **unicode** — **N/A.** No text encoding changes; `env::args()` decoding is
  untouched. The compiler's unicode handling was re-verified above.
- **nesting_recursion** — **N/A.** `run_cli` dispatches on `args.len()` in a
  flat `match` with no recursion, and this adds no nesting.
- **duplicate_missing_keys** — **N/A.** No record, map or field lookup is added.
- **malformed_input** — covered: the second test's three refusals, including the
  not-a-`.rbc` path and a file that does not exist.
- **resource_limit** — **N/A.** This adds no loop, recursion or allocation over
  program input; `set_program_args` copies the argument vector exactly as it
  already did for `rb run`.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass (exit 0, no diff) |
| `cargo clippy --all-targets -- -D warnings` | pass (exit 0, zero warnings) |
| `cargo test --all-targets` | **1018 passed, 0 failed, 0 ignored** |
| `./rbops/verify.sh phase-021` | **not run — `rbops/` is not in this checkout** |
| `rb lint bootstrap/compiler.rb` (phase DoD) | pass (exit 0, no output) |

`cargo test --test bootstrap_selfhost_test` alone: **10 passed, 0 failed**.

**On the fourth gate:** `rbops/verify.sh` is not present in the working tree —
`ls rbops/` returns `No such file or directory`, and the dispatch instructions
place the RBOPS pipeline outside this checkout and forbid inspecting it. So I
could not run it and am not claiming it. Everything it checks that I could run
is above and green.

Zero new `#[ignore]`, zero `// skip`, zero `allow(clippy::…)`. Zero newly-failing
pre-existing tests.

## Definition of done

- [x] `bootstrap/compiler.rb` is itself valid Redblue and passes `rb lint` —
      exit 0, no output. Unchanged by this phase.
- [x] Running it under the Rust `rb` on the corpus produces `.rbc` files — 308 of
      them, every one starting `RED\x1a`.
- [x] Output is byte-identical to the Rust `rb compile` for the corpus — every
      program in the compared families that the frontend accepts, re-verified
      here from scratch (see the table above). Not the whole corpus: 53 of its
      361 files are frontend-refused — 44 in `malformed` and 9 elsewhere — and a
      refused program has no bytes to agree about. Those refusals are pinned by
      the malformed-input tests.
- [x] No Rust-only fast path is reachable from this path — unchanged by this
      phase; `compiler.rb` names no `compile`/`codegen`/`Chunk`.

## Invariants touched

- **None.** No `.rb` extension change, no `end`/brace change, no `set … to`
  change, no `Value` variant change, no `Error` variant change, no grammar
  change, no change to what either interpreter *executes*. This change adds a CLI
  argument channel and an exit code; it does not change what a program means.

## Known gaps / follow-ups

- **S3 is still not claimed, and is now further from reachable than this report's
  first version implied.** The `rb vm` argument channel is fixed, but stage 1's
  own output of `bootstrap/compiler.rb` **does not run**:
  `rb vm stage1.rbc in.rb out.rbc` dies with
  `bytecode asked for 2 values its frame never pushed` at line 2176
  (`BUILD_RECORD 4`). Verified pre-existing by stashing this phase's diff. Ten
  reduction attempts all ran correctly, so it needs something in the 2 748-line
  program that the reducer does not yet cover. → `FINDINGS.md` §3.
- **`to (x) … end` as an expression** is refused by stage 2 with a message
  (`bootstrap/compiler.rb:1998`); stage 1 refuses it too. A gap, not a feature.
- **9 corpus programs in non-`malformed` families are analyzer-refused**, so the
  walk skips them. Listed in `FINDINGS.md` §2; filing is the auditor's call.
- **The `give back` tree-walker bug** from `FINDINGS.md` §1 is untouched.
