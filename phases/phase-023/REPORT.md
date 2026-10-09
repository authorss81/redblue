# Phase 023 — Ship the self-hosted compiler as `rb` (bootstrap S4)

## Status

Round 3. S4 itself was already implemented and green; this round found and
fixed two defects in **the verification of S4**, one of which made a test
impossible to fail and the other of which made a test fail intermittently on
`main`. No production behaviour of `rb` changed: every line of the diff is in
`mod tests` inside `src/bootstrap.rs` and in `tests/bootstrap_ship_test.rs`.

The finding the phase was opened for is `Final rung.` — it is a placeholder,
not a defect description, so it could not be re-verified as written. What was
verifiable, and did reproduce, is below.

## Reproduction (before)

Two commands, both on the committed tree at `daeb73b`.

### 1. A test that could not fail

```
$ cargo test --lib bootstrap::tests      # all 5 pass
```

Every one of those passes, including
`a_stage_two_that_is_not_the_fixed_point_ships_nothing`, which exists to pin the
one claim S4 makes: a stage 2 that is not the fixed point must be refused.

It was not pinning it. `build_with` reads the compiler's source before it
consults the stage 2 it was handed, and the test handed it
`Path::new("unused")` — a path that does not exist — so `read_compiler`
refused and `verify` was never reached. Both assertions in the test then passed
on the *missing file*, and neither on the fixed point.

Proof, by neutering the code the test claims to pin:

```
$ # verify() -> Ok(()) unconditionally
$ cargo test --lib bootstrap::tests
test bootstrap::tests::a_stage_two_that_is_not_the_fixed_point_ships_nothing ... ok
test result: ok. 5 passed; 0 failed
```

A test that survives the deletion of the thing it tests is not a test.

### 2. A test that failed on `main`, intermittently

```
$ cargo test --all-targets
running 112 tests
test bootstrap::tests::a_stage_two_run_restores_the_arguments_it_published ... FAILED
thread '...' panicked at src/bootstrap.rs:653:9:
assertion `left == right` failed: a stage-2 run left its input and output paths
in sys.argv()
  left: []
 right: [".../target/tmp/bootstrap-unit/boom.rb", ".../target/tmp/bootstrap-unit/boom.rbc"]
test result: FAILED. 111 passed; 1 failed
```

This one is a genuine race, not a flake in the usual sense. The test sampled
`sys.argv()` with a bare `runtime::take_program_args()` and compared it after
the runs. `sys.argv()` is a process-global; `STAGE2` makes a publish-and-restore
pair atomic against another *run* but does nothing for a bare read. Two tests in
the same binary run in parallel threads, and the two of them shared the scratch
paths `boom.rb`/`boom.rbc` — so one test's `run_compiler_on` could be publishing
its paths at the moment the other sampled "what was there before", and the
restore then correctly put back `[]`.

Frequency on the committed tree — 40 consecutive runs of the lib test binary
against a **cold** scratch directory (`target/tmp/bootstrap-unit` emptied
first, so `boom.rb` did not yet exist and the two tests' writes raced to
create it):

```
$ for i in $(seq 1 40); do $BIN --test-threads=16; done | grep '^test result' | sort | uniq -c
      2 test result: FAILED. 111 passed; 1 failed; ...
     38 test result: ok. 112 passed; 0 failed; ...
```

Against a **warm** scratch directory, the same committed tree fails every time
— the collision needs the files to exist for one test's
`fs::remove_file(output_path)` to delete the other's input mid-run:

```
     60 test result: FAILED. 112 passed; 2 failed; ...
```

So this is not a rare flake: on a machine where the scratch directory has been
used once — which is the normal case in CI, where the earlier targets in the
same run have already run — it fails every time.

After the fix, 60 consecutive runs against the warm directory, zero failures:

```
     60 test result: ok. 114 passed; 0 failed; 0 ignored
```

## What changed

| File | Lines | What |
|---|---|---|
| `src/bootstrap.rs` | +195 −11 | one doc comment on `STAGE2`, the rest inside `mod tests`. `a_stage_two_that_is_not_the_fixed_point_ships_nothing` is handed a real compiler file and now asserts the refusal names the fixed point *and* the flipped byte; `argv_while_no_run_is_publishing` samples `sys.argv()` under `STAGE2`; the two colliding scratch paths (`boom.rb`/`boom.rbc`, `argv-boom.rb`/`argv-boom.rbc`) are given names no other test uses; two new `edge_*` tests |
| `tests/bootstrap_ship_test.rs` | +106 −0 | two new tests on the shipped compiler: non-ASCII source, and an empty program |

No production code changed. No signature, no visibility, no error message, no
threshold. The one hunk outside `mod tests` is a doc comment on `static STAGE2`
recording what the lock does *not* do — it serialises runs, it does not make
`PROGRAM_ARGS` safe to read at an arbitrary moment. That is the contract the
test bug in defect 2 broke, and it was not written down anywhere.

### The three fixes

1. **`a_stage_two_that_is_not_the_fixed_point_ships_nothing` now reaches
   `verify`.** It writes a real compiler to `refused-compiler.rb` and passes
   that. The refusal is then matched against `"the fixed point does not hold"`
   and against `format!("byte {last}")` — the byte that was flipped — so a
   refusal for any *other* reason fails the test. It also asserts
   `stage1.rbc` was not written, which a half-finished build would have left
   looking like a successful one.
2. **`argv_while_no_run_is_publishing`** takes `STAGE2` before reading
   `PROGRAM_ARGS`, so a sample cannot land inside another run's publish window.
   The lock is held for the read and released immediately; the two
   `run_compiler_on` calls the test makes still take it themselves.
3. **Unique scratch paths.** `boom.rb`/`boom.rbc` were each used by two tests
   in one binary, and `run_compiler_on` *deletes* its output path before
   running — so two tests sharing one path delete each other's input while the
   other's VM is mid-run. Now `out-of-bounds.*` and
   `argv-out-of-bounds.*`.

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_the_fixed_point_comparison_names_the_offset_that_differs` (unit, `src/bootstrap.rs`) | **empty, singleton, boundary** — `verify` on two empty files and two one-byte files; six disagreeing pairs; **the two a `zip` cannot see**, where every shared byte agrees and the files differ only in length (stage 2 a prefix of stage 1, and the reverse), whose first difference is one past the end of the shorter file. Each asserts the *reported offset*, so a refusal that says nothing about where is a failure. A final case asserts the offset is the **first** difference and not merely one of them |
| `edge_a_stage_two_run_that_writes_no_bytecode_is_refused` (unit, `src/bootstrap.rs`) | **empty + failure** — three shapes through `run_compiler_on`: a compiler that exits cleanly having written **no file**, one that writes a file with **no bytes** in it, and one that writes bytes. The first two must be `Err` naming the missing file; the third must be `Ok` and leave exactly what it wrote — so the two refusals are the absence of a compiler, not a refusal of everything |
| `edge_the_shipped_compiler_agrees_with_the_frontend_on_non_ascii_source` (`tests/bootstrap_ship_test.rs`) | **unicode** — CJK, RTL, two astral-plane emoji, a combining mark, an escaped quote and a backslash, and a plain ASCII string beside them. Asserts `source.len() > source.chars().count()` first, so the test cannot silently become an ASCII test, then compares the shipped compiler's bytes to `compile_source`'s |
| `edge_the_shipped_compiler_agrees_with_the_frontend_on_an_empty_program` (`tests/bootstrap_ship_test.rs`) | **empty** — the singleton of the family above: a program with no statements at all. Asserts the frontend's encoding is non-empty, so there is an artifact to compare, then that the shipped compiler produces the same bytes |

**Test quota: met.** 4 new `#[test]` functions (floor is 3), 4 named `edge_*`
(floor is 1), and failure-asserting tests throughout — two of the four require
an `Err`, and `a_stage_two_that_is_not_the_fixed_point_ships_nothing` requires
one too. Zero new `#[ignore]`, zero `// skip`, zero `allow(clippy::…)`.

**Red before green, and the reds were the interesting part.** The first defect's
red is not "the test fails", because the test could not fail. It is the mutant:
`verify` answering `Ok(())` to every pair still left all five tests passing.
After the fix, the same mutant kills
`a_stage_two_that_is_not_the_fixed_point_ships_nothing` **and**
`edge_the_fixed_point_comparison_names_the_offset_that_differs`. The second
mutant — `verify`'s message dropping the offset — kills both of those too:

```
test a_stage_two_that_is_not_the_fixed_point_ships_nothing ... FAILED
test edge_the_fixed_point_comparison_names_the_offset_that_differs ... FAILED
the refusal does not name byte 82, which is the byte that was flipped
differing first byte: the refusal does not name byte 0 ...
```

A third mutant — `run_compiler_on` accepting an empty output file — is killed by
`edge_a_stage_two_run_that_writes_no_bytecode_is_refused`. All mutants were
reverted; the tree carries none of them.

**The race's red is the frequency table above**: 2 failures in 40 runs on the
committed tree, 0 in 40 after the fix.

## Edge-case matrix (AGENTS.md §3.2)

| Row | Status |
|---|---|
| empty | covered — `verify` on two empty files; a stage-2 run that writes a file with no bytes; a shipped-compiler run over a program with no statements; the empty compiler on both `build` and `rollback` (round 2) |
| singleton | covered — `verify` on two one-byte files; a stage-2 run that writes exactly the bytes it was asked to, checked against the file on disk |
| boundary | covered — `verify` differing at byte 0 and at the **last** byte; the two length-mismatch cases where the first difference is one past the end of the shorter file; the tamper in both the build's own refusal and the integration test flips the **last** byte |
| out_of_bounds | **N/A** — nothing indexes a list or slice by an untrusted value. The only indexing is a test flipping `len() − 1`, guarded by `checked_sub` with an `expect` on a file it has just written |
| type_mismatch | covered — `run_compiler_on` refuses a compiler that produced no artifact where one was required, which is the closest analogue this module has; a *silent* compiler (valid source, valid run, wrong shape) is refused by `build` (round 2) |
| numeric_boundary | **N/A** — this phase adds no arithmetic. The values are `Vec<u8>` and `usize` offsets; the only numeric edge is the offset `verify` reports, and it is now pinned for every disagreement shape |
| unicode | covered — **this round**, `edge_the_shipped_compiler_agrees_with_the_frontend_on_non_ascii_source`. Round 2 recorded this row as a gap for the S4 path specifically (non-ASCII was covered for S3, not for the shipped file); it is no longer a gap |
| nesting_recursion | covered — `edge_the_shipped_compiler_agrees_with_the_frontend_on_a_corpus_family` compiles nested `if`/`else` inside a `repeat` with the shipped file and compares byte for byte |
| duplicate_missing_keys | **N/A** — no records are built or read in this module. The compiler's own state is the byte-level constant pool, which is compared as bytes |
| malformed_input | covered — a missing compiler file (now the *intended* path in the fixed-point test rather than an accident that masked it), an empty compiler, a compiler with a syntax error (refused as `Error::Parser` at its line), a stage-2 refusal (positioned, round 2), an unknown CLI flag, and a stage 2 that is not a valid fixed point |
| resource_limit | covered — the step budget governs stage 2 through the same `BytecodeVm`; `STAGE2` serialises stage-2 runs because `sys.argv()` is process-global; and **this round** the scratch-path collision, which is a resource collision between two parallel tests over one file |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | **1063 passed, 0 failed, 0 ignored** across 37 targets |
| `./rbops/verify.sh phase-023` | **not run — `rbops/` is not in this checkout** (`ls rbops` → `No such file or directory`). The four project-level checks it stands for were run by hand instead |

`cargo test --all-targets` was run to completion: 37 `test result: ok` lines, no
`FAILED`, no `panicked at`, no `error`. The lib target is 114 (was 112 — the two
new unit tests) and `bootstrap_ship_test` is 16 (was 14 — the two new
integration tests). The full run takes ~19 min, dominated by the six
self-compilations that finding 2 below is about.

Substituted for the missing `verify.sh`, and their results:

| Check | Result |
|---|---|
| `rb` runs every `examples/*.rb` | pass — no non-zero exit from any of them |
| `rb test` (the in-language suite) | pass — 340 run, 340 passed, 0 failed |
| `rb test tests/suite.rb` | pass — 19 total, 19 passed, 0 failed |

Not re-run this round: `./bootstrap/build.sh` and `--rollback`. They exercise
`src/bootstrap.rs`'s *production* paths, and this round changed no production
path. Round 2 ran both and recorded 41 s and 17 s. If this round's claim about
the release were wrong, it would have to be because the production build path
changed, and `git diff` shows every hunk is inside `mod tests`.

## Invariants touched

- **None.** No language surface changed. No `.rb` extension, no `to … end`, no
  `set x to <expr>`, no `Value` variant, no `Error` variant, no `parse_*`
  signature, no grammar. The only files touched are `src/bootstrap.rs` (inside
  its `#[cfg(test)] mod tests`) and a test file. The fixed point itself is
  untouched and unproven-untouched by this round: `git diff` on
  `bootstrap/compiler.rb` and on `verify` is empty.

## Definition of done

Carried forward from round 2, and re-checked this round where re-checkable.

- [x] **release pipeline builds `rb` via the stage2 compiler** — as far as is
      possible: `rb` is a Rust binary and no Redblue program emits machine code,
      so what S4 can mean is the *compiler the release carries*.
      `bootstrap/build.sh` produces it from stage 2's bytes and refuses when the
      fixed point does not hold, and `.github/workflows/release.yml` runs that
      script before it uploads anything. Unchanged this round.
- [x] **the resulting `rb` passes the full four-gate suite** — **1063 passed, 0
      failed, 0 ignored** across 37 targets, plus the three substituted project
      checks above. The run was repeated to completion after the last edit.
- [x] **bootstrap instructions reproducible from a clean checkout** —
      `./bootstrap/build.sh`. Unchanged this round.
- [x] **a rollback path to the Rust compiler is documented and tested** —
      `--rollback`, asserted by two tests that it produces the same bytes rather
      than merely exiting zero. Unchanged this round.

The finding this phase was opened for, `Final rung.`, is not a defect
description and could not be reproduced as written. What this round did
reproduce is above, and it was real.

## Known gaps / follow-ups

- **~19 min for `cargo test`, six self-compilations.** Unchanged from round 2
  and still the largest cost in the suite: `tests/bootstrap_ship_test.rs` makes
  three and `tests/bootstrap_selfhost_test.rs` makes three, each ~3 min in a
  debug build, each computing the same 162 298 bytes. A `build.rs` step or a
  shared fixture that produces `stage1.rbc` once would fix it and would remove
  the need to reason about the `STAGE2` lock at all. It changes the existing
  test layout, which this phase was told not to do. → FINDINGS §2.
- **`run_cli` still matches commands on argument count.** A silent non-zero
  exit remains the worst failure mode for a command a CI script calls, and the
  next command with a flag will hit it. A rewrite to a flag table is a refactor
  and belongs in its own phase. → FINDINGS §5.
- **The shipped artifact is a `.rbc`.** Unchanged and unchangeable here: `rb` is
  a Rust binary and no Redblue program emits an executable, so S4's strongest
  true statement is what round 2 implemented. Going further needs a
  Redblue-to-native backend as its own phase. → FINDINGS §3.