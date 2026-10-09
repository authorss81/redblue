# phase-023 FINDINGS

Work discovered while shipping S4 that is **not** this phase's. Recorded here so
the auditor can promote it to a phase with a `must_touch` of its own.

## 1. ~~The release workflow cannot be updated by an agent~~ — FIXED in round 1

**Severity: was major, blocked "release pipeline builds `rb` via the stage2
compiler" read literally. Status: fixed; `.github/workflows/release.yml` now
runs the ladder and ships `compiler.rbc`.**

`.github/workflows/release.yml` built the binary with `cargo build --release`
and nothing else. The S4 step was one line:

```yaml
      - run: ./bootstrap/build.sh
        shell: bash
```

before the upload, with `target/release-bootstrap/compiler.rbc` added to the
files `softprops/action-gh-release@v2` uploads. `shell: bash` is there because
the Windows runner's default shell is PowerShell; the Windows leg of the matrix
runs the script through Git Bash, and `bootstrap/build.sh` now finds `rb.exe`
rather than assuming `rb`.

**The original reasoning was wrong in the way that matters here.** It read as
though editing `.github/workflows/` were forbidden by a rule in `AGENTS.md`, and
recorded the step as work for "a human or a future phase with the scope". There
is no such rule in the `AGENTS.md` of this checkout: the rule set it carries is
build commands, Rust style, the pipeline, and how to add features. A phase that
declines a fixable DoD row, reports the row as checked, and leaves the actual
defect in `FINDINGS.md` has produced a report that disagrees with the
repository — and the disagreement is what a reviewer has to catch.

## 2. `cargo test` spends ~11 minutes, and ~6.6 of them are self-compilations

`tests/bootstrap_ship_test.rs` makes three self-compilations (library build, CLI
build, CLI rollback build) and `tests/bootstrap_selfhost_test.rs` makes three
more, each ~3 min in a debug build. Both files already deduplicate *within*
themselves with a `OnceLock`; nothing shares *between* them.

**The fix:** a `build.rs` step or a test fixture that produces
`target/tmp/bootstrap/stage1.rbc` once, with the test files reading it rather
than each computing its own. This changes the existing test layout, which
phase-023 was told not to do. file:line — the duplication is visible at
`tests/bootstrap_selfhost_test.rs:1491` (`self_compilations`) and
`tests/bootstrap_ship_test.rs` (`release`), two `OnceLock`s computing the same
162 298 bytes.

## 3. The shipped artifact is a `.rbc`, and that is a real limit on S4

`rb` is a Rust binary and no Redblue program emits a machine code executable, so
"the shipped `rb` is built by `rb`" cannot be satisfied literally — a Redblue
native backend would be a new compiler backend, not a bootstrap phase. S4 as
implemented is the strongest true statement: *the compiler the release carries
was written by the compiler Redblue ran, and the build refuses to ship unless
that file is the fixed point.*

If the ladder is meant to go further, the next rung needs a phase of its own:
a Redblue-to-native backend, or an `rb`-written linker. Neither belongs in a
phase whose `must_touch` is `src/ or bootstrap/` and whose method is "make the
smallest change that turns the test green".

## 4. ~~Non-ASCII source is untested on the S4 path specifically~~ — FIXED in round 3

**Status: fixed.** `edge_the_shipped_compiler_agrees_with_the_frontend_on_non_ascii_source`
in `tests/bootstrap_ship_test.rs` runs the shipped compiler over a source holding
CJK, RTL, two astral-plane emoji, a combining mark, an escaped quote and a
backslash, plus a plain ASCII string beside them, and compares the bytes to
`compile_source`. It asserts `source.len() > source.chars().count()` before
comparing, so it cannot silently decay into another ASCII test.
`edge_the_shipped_compiler_agrees_with_the_frontend_on_an_empty_program` covers
the other end of the same family.

The original reasoning was: `tests/bootstrap_ship_test.rs` uses ASCII
throughout, and the S4 path *is* exercised on non-ASCII source by the
phase-021/022 corpus tests, because S4 runs the same `BytecodeVm` over the same
files. That reasoning is exactly the "it is covered elsewhere" shape that makes a
gap survive two rounds: the corpus tests cover *S3*, this file covers *S4*, and
the argument for skipping it is that they happen to run the same VM today —
which is the thing a future change to `bootstrap.rs` would stop being true of.

## 5. `run_cli`'s argument matching is a hazard for any command with flags

Three of the five functional changes in this phase were argument-matching bugs,
not bootstrap logic:

- `rb bootstrap` did not exist and fell through to the file-runner
  (`src/lib.rs`, the `2` arm);
- `rb bootstrap target/out` read `target/out` as the compiler;
- `rb bootstrap out --rollback` exited 1 **printing the help text to stdout and
  nothing to stderr** — the `4` arm claims every four-argument invocation for
  `format --check`.

A silent non-zero exit is the worst failure mode for a command a CI script
calls. `bootstrap` is now matched on its name (`n if n >= 3 && args[1] ==
"bootstrap"`) rather than on an argument count, but every other command with a
flag is still matched on a count. **`rb format --check` and `rb compile -o` are
the two that exist; any command added later will hit this.** A rewrite of
`run_cli` to a flag table is a refactor and does not belong in a bootstrap
phase — it should be its own phase, and it should come with a test that every
command's usage error goes to stderr.

Round 1 found that the first of those three was only half fixed: the `n >= 3`
arm claimed `rb bootstrap <out-dir>` and the `2` arm still treated the bare
`rb bootstrap` as a file to run, so the documented command with its output
directory defaulted still exited 1 with `Is a directory`. Both forms are now
claimed by name and `edge_bare_rb_bootstrap_is_the_command_and_not_a_file_to_run`
holds the two-argument form in place.

## 6. `sys.argv()` is process-global, so stage 2 is serialized rather than
    scoped

`src/runtime.rs`'s `PROGRAM_ARGS` is one `Mutex<Vec<String>>` for the whole
process, read by `runtime::builtin` when a program asks for `sys.argv()`. Stage 2
publishes its input and output paths there and then runs a VM for minutes, so
two builds in one process could compile each other's input to each other's
output. Round 1 fixed it with a dedicated `STAGE2` mutex rather than a
VM-scoped argument list, because `sys.argv()` is resolved by a free function with
no VM to hang the arguments off: per-VM argv is a change to the stdlib call
path, and it belongs to whoever changes that path.

Round 2 added the other half, which the lock does not cover: the arguments are
now saved before stage 2 publishes them and put back on both the success and the
failure path, through a new `runtime::take_program_args`. Without it a build
left its input and output paths in `sys.argv()` for whatever read them next in
the same process — the lock stops two *builds* interleaving, and does nothing
about a *program* run beside one. The save/restore is inside the `STAGE2` lock
too, so a restore cannot race a publish.

The save/restore being correct depends on nothing between the publish and the
restore returning early, which is why the run is a helper function returning
`(Result, String)` rather than inlined: a `?` on the way out of the body would
skip the restore and the test that checks the failure path would be the only
thing that noticed.

**Round 3 found what the lock does not cover.** It makes a publish-and-restore
pair atomic against another *run*; it does nothing for a **bare read**. A thread
that calls `runtime::take_program_args()` while another is inside its window
takes that run's paths as "what was there before", and the restore then
correctly puts back the process default — so the two disagree and the only way
to tell which is wrong is to lose a race. Finding 9b is that, reached from the
test that verifies this very finding. The fix was in the test, because the
production pair is already correct; but the gap is in the *lock's contract*, and
the doc comment above it should say so: it serialises runs, it does not make the
global safe to read at an arbitrary moment.

**This is a lock, not a fix, and the cost is known:** two self-compilations in
one process now take twice as long as they would in parallel. `cargo test` is
already ~11 minutes for the reason in finding 2, so the cheaper answer — one
build shared by every test that needs one — is still the answer, and it also
removes the need for the lock to be reasoned about at all.

## 7. `Stage2::Bytes` and `build_with` are crate-private, so the build's own
    refusal to a handed stage 2 is only reachable from a unit test

**Status: fixed in round 2, recorded here because it is a shape a future phase
will trip over.**

`Stage2::Bytes` exists so the build can be *caused* to fail its fixed point. Round
1 left it `pub`, so any library caller could hand `build_with` bytes no
self-hosted run produced and get back a `Build` whose `is_fixed_point()` was
true — the one claim S4 exists to make, satisfiable without running the ladder.
Round 2 made both private and put the arm behind `#[cfg(test)]`, which means
`tests/bootstrap_ship_test.rs` can no longer reach them: an integration test is
an outside caller by construction. The tests that need that path are a `mod
tests` in `src/bootstrap.rs`.

The general point: **a private-by-convention path in a public API is not
private.** The doc comment said the only caller was a test, which was true and
was not enforced by anything. If a bypass exists to make a check testable, its
visibility is the check.

## 8. Two matrix legs publish one asset name

**Severity: major. Status: fixed in round 2.**

`release.yml`'s matrix builds on `ubuntu-latest` and `windows-latest`, and both
legs ran the ladder and then uploaded `target/release-bootstrap/compiler.rbc`.
The two legs race to publish one asset name on one release: GitHub either
overwrites the first or fails the second as a duplicate, so a release could ship
a compiler from whichever runner finished second, or not be cut at all. The
binary is per-OS and legitimately different; the compiler is a `.rbc` and is the
same file on both runners. One leg now uploads it, gated on a matrix flag, and
`edge_the_release_workflow_uploads_the_compiler_once` counts the published paths.

The general point: **a matrix multiplied a step that was not idempotent.** Any
future matrix dimension over the release job needs each leg's publish list
checked for overlapping names — the same check as "does this test actually run",
applied to the pipeline.

## 9. Round 3 — the S4 tests could not fail, and one of them failed anyway

Two defects, both in the *verification* of S4 rather than in S4. Neither changes
production behaviour; every hunk of round 3's diff is inside `mod tests`.

### 9a. The fixed-point refusal was pinned by a missing-file refusal

**Severity: major — a test that could not fail. Status: fixed in round 3.**

`a_stage_two_that_is_not_the_fixed_point_ships_nothing` in
`src/bootstrap.rs` handed `build_with` the compiler path `Path::new("unused")`.
`build_with` reads the source *before* it consults the stage 2 it was handed, so
`read_compiler` refused on the missing file, `verify` was never reached, and both
of the test's assertions — `outcome.is_err()` and "nothing was written" — passed
on the missing file rather than on the fixed point.

Proof: with `verify` changed to answer `Ok(())` to every pair, all five tests in
`src/bootstrap.rs` still passed. The test survived the deletion of the only
thing it exists to pin — which is the definition of the failure mode AGENTS.md
§5 calls fake completion, reached by a route nobody took on purpose.

**The general point, and it is not about this test.** The test's own comment
explained the hazard and then did the thing: *"The compiler path is read before
the stage 2 is consulted, so a build handed bytes still has to be handed a real
source."* It passed `Path::new("unused")`, which is not a real source. **A
comment that describes a precondition is not the precondition being met.** The
same shape will recur anywhere a test passes a path it does not create.

Fixed by writing a real compiler to `refused-compiler.rb` and passing that, so
`verify` is reached — and by matching the refusal against both
`"the fixed point does not hold"` and `format!("byte {last}")`, so a refusal for
any *other* reason now fails the test. It also asserts `stage1.rbc` was not
written. The mutant that used to pass now fails, and a second mutant — `verify`'s
message dropping the offset — fails both that test and
`edge_the_fixed_point_comparison_names_the_offset_that_differs`.

### 9b. Two tests shared scratch paths, and one failed on `main` for it

**Severity: major — an intermittent gate failure on the committed tree. Status:
fixed in round 3.**

`cargo test --all-targets` on `daeb73b` failed here:

```
test bootstrap::tests::a_stage_two_run_restores_the_arguments_it_published ... FAILED
assertion `left == right` failed: a stage-2 run left its input and output paths in sys.argv()
  left: []
 right: [".../target/tmp/bootstrap-unit/boom.rb", ".../target/tmp/bootstrap-unit/boom.rbc"]
```

Three things compounded:

- `a_stage_two_run_restores_the_arguments_it_published` sampled `sys.argv()` with
  a bare `runtime::take_program_args()`. `STAGE2` makes a publish-and-restore
  pair atomic against another *run* and does nothing for a bare read, so a
  sample landing inside another run's window took that run's paths as "what was
  there before", and the restore then correctly put back `[]`.
- `a_stage_two_refusal_keeps_its_kind_and_position` and
  `a_stage_two_run_restores_the_arguments_it_published` each used the scratch
  paths `boom.rb`/`boom.rbc`. `run_compiler_on` *deletes* its output path before
  running, so two tests sharing one path delete each other's input while the
  other's VM is mid-run.
- Rust runs a test binary's tests in parallel threads, and all of
  `src/bootstrap.rs`'s tests share one process-global.

How often it fails depends on whether `target/tmp/bootstrap-unit` is cold or
warm, and the warm case is the one that matters. Cold — 2 failures in 40 runs of
the lib binary, because the two tests are racing to *create* `boom.rb`. Warm, so
the files already exist and one test's `fs::remove_file(output_path)` deletes
the other's input mid-run: **60 failures in 60 runs.** A CI machine that has run
any bootstrap test once is the warm case.

Fixed by a helper that takes `STAGE2` around the read, and by giving each test
scratch names no other test uses. 40 consecutive runs after the fix: 0 failures.

**The general point.** `run_compiler_on`'s save/restore (finding 6) and its
`STAGE2` lock were both verified by round 2's tests, and both were correct; what
was missing is that **the tests verifying them shared mutable state with each
other.** A test that mutates a process-global or a file needs its own name, and
anything it reads across a call that another thread can also make needs the same
lock the production code uses. §3.1's determinism row covers "no reliance on
`HashMap` iteration order" and stops there, which is why this shape got through
two rounds: it is deterministic *per thread* and only not across threads.

**Note on how this was found.** It was not found by reading. It was found by
`cargo test --all-targets` failing on a tree I had not touched yet — the first
run of the full suite on the committed checkout. **The full suite, run on
`main` before changing anything, is worth more than any amount of reading the
tests.**
