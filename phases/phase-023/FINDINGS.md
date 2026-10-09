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

## 4. Non-ASCII source is untested on the S4 path specifically

`tests/bootstrap_ship_test.rs` uses ASCII throughout. The S4 path *is* exercised
on non-ASCII source by the phase-021/022 corpus tests, because S4 runs the same
`BytecodeVm` over the same files — but if a future change gave `bootstrap.rs` its
own VM or its own file writing, nothing in the S4 tests would catch it.

**Cheap fix:** one test that runs the shipped compiler over a source containing
a CJK string, an emoji, an RTL string and a combining mark, and compares the
bytes to `compile_source`. It costs a second, since the build is already made.

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
