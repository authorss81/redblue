# phase-044 — findings

Work that belongs to a later phase, with the evidence for it. Nothing here was
done under phase-044's name; none of it is on-topic for the manifest and
lockfile it was building.

Items §2 and §3 were open after the second run and were closed by the third:
they are written up in `REPORT.md` and kept here only as the record of what they
were, and are marked **closed**.

## 1. ~~The tree-walking loader of `rb run` still resolves against the working directory~~ — **closed**

**Was:** `src/interpreter.rs` (`module_path`, `load_module` →
`crate::runtime::module_program` → `crate::vfs::read_to_string`). The *analyzer*
half and the *bytecode* half were both tested and both took a `root`, but the
production path — `rb run` — never derived one. `rb run subdir/prog.rb`
consulted `./redblue.manifest` while the two rooted APIs the phase added were
tested against roots nobody ran anything through.

**Now:** `run_file` derives the root from the program's own path
(`src/lib.rs::root_of`) and threads it through `run_source_in` →
`analyzer::analyze_in` and `interpreter::run_isolated_in` → `Vm::new_in`, so all
three engines read one manifest. The same root is now in `crate::vfs`
(`vfs::rooted`), which is what made the join possible without the two-rule
problem the note below describes: a search path relative to a root and a path a
filesystem is asked about are now the same string.
`running_a_program_resolves_the_manifest_beside_it` is the finding, run through
`rb run` rather than through a rooted API.

**Why it was not simply threaded through like the other two, originally:** a
module file is read through `crate::vfs::read_to_string(path)`, which on wasm is
the in-memory filesystem. Giving `Vm` a `root` would have made `is_file()`
consult `root.join(path)` and the read still take `path` — finding the right file
by one rule and opening whatever is at the relative path by another. The root in
`vfs` is what answers that: `vfs::rooted(root, relative)` is the one string both
the probe and the read take.

## 2. Any resolve writes the lockfile, including one from the analyzer — **closed**

**Was:** `src/manifest.rs` — `failure_in`, `pinned_module_path_in` and
`resolve_in` all went through the writing path, and `analyze_in` called
`failure_in` once per `import`, so `rb lint` in a directory holding a
`redblue.manifest` created `redblue.lock` beside it — once per import.

**Now:** `manifest::resolve_without_writing(root)` answers the same questions
from reads alone, `resolve_in` is that plus the lockfile, and all three engines
resolve once per program through `manifest::Pins`.
`edge_analysing_a_program_writes_no_lockfile` runs four imports past a manifest
that resolves and asserts no lockfile appeared, then asserts that `resolve_in`
does write one.

## 3. `MAX_RESOLUTION_STEPS` and `MAX_RESOLUTION_DEPTH` are untested — **closed**

**Was:** both caps were reachable only from a registry offering thousands of
modules or a chain deeper than 64, and neither had a test. `MAX_RESOLUTION_DEPTH`
was worse than untested — it was enforced as fan-out width
(`module_manifest.dependencies.len() > MAX_RESOLUTION_DEPTH`), so a seventy-long
chain passed and a module with seventy pins was refused.

**Now:** both constants are `pub`, each queued dependency carries the depth of
the chain that reached it, and each limit has a test on both sides of it
(`edge_a_chain_deeper_than_the_limit_is_refused`,
`edge_a_chain_exactly_at_the_depth_limit_resolves`,
`edge_a_graph_larger_than_the_step_budget_is_refused`,
`edge_a_graph_one_step_under_the_budget_resolves`) plus
`edge_a_module_with_more_pins_than_the_depth_limit_resolves`, which pins that
width is not depth. The `Registry` trait turned out to be exactly the seam the
old note said it was — a module manifest naming four thousand distinct leaves is
four thousand steps, and nothing about it needs a filesystem.

## 4. ~~The lockfile is written, never read~~ — **closed**

**Was:** `write_lockfile` existed and nothing parsed `redblue.lock`, so a resolve
always recomputed from the manifest and overwrote the file. A version published
after the lock was written was handed to the program in silence: `*` resolved to
the new maximum, the program ran different code, and the lockfile still said
what it had said the first time.

**Now:** `manifest::parse_lockfile` and `LockedModule` read it back, and
`check_lockfile` refuses a disagreement — a locked version the resolution no
longer produces, and a locked module the manifest no longer pins — as a
`Runtime` error naming the module, both versions, and the file to delete. The
refused resolve writes nothing, so the lock that could not be honoured is still
there. `edge_a_version_published_after_the_lock_is_refused`,
`edge_deleting_the_lockfile_re_resolves` and
`edge_a_locked_module_the_manifest_no_longer_pins_is_refused` are the three
shapes.

## 5. Requirements are `*` and `=x.y.z`; there is no range and no registry

**Where:** `src/manifest.rs` (`Requirement::parse`).

**What:** `^1.2`, `>=1.2` and `1.x` are refused with a message saying so. A
module version is a directory in the checkout (`DirRegistry`); nothing is
fetched. This is the smallest thing that lets two programs pin two versions of
one module, which is what phase-044 asked for, and nothing more.

**What would fix it:** a range requirement is ~30 lines over the same `Version`
comparison the wildcard already uses. A registry means a network protocol, a
cache directory and a trust decision, and is a phase of its own.

## 6. ~~A directory in a module search path used to be an `Io` error~~ — **closed in the first run**

**Was:** before this phase, `import Fixture` with only `modules/Fixture/`
present answered `Cannot load module './modules/Fixture': Is a directory`. No
test asserted that message. It was changed here rather than left because this
phase is what made `modules/<name>/` mean something — a directory named after a
module is now the normal shape of a published module, and opening one was never
the right answer.

## 7. `rb test` resolves a program's imports against the working directory

**Where:** `src/testing/harness.rs` (`run_file` → `run_source`).

**What:** `rb run <file>` derives its root from the file (§1) and `rb test
<file>` does not, so the two entry points disagree about what `import Fixture`
means for the same file.

**Why it was not changed here:** the harness is not asked to run one program. It
is asked to run a *suite* — `rb test` with no argument discovers every `.rb` file
under `tests/` and runs them together, and each of those files has to resolve
against the same root, the one the command was invoked from. The repository's own
suite depends on it: `tests/test_modules.rb` imports `MathUtils` from
`<repo>/modules/MathUtils.rb`, and deriving a root per file would resolve it
against `<repo>/tests/modules/` and fail every module test. The suite is what
`rb test` means; a root per *file* is what `rb run` means.

**What would fix it:** a `--root` on `rb test`, so a caller that wants one suite
resolved against one directory can say so without the default changing.

## 8. A refused resolve has no flag for the re-resolve that would write it

**Where:** `src/manifest.rs` (`check_lockfile`).

**What:** the only way past a refused lockfile is to delete the file. That is the
right default — a program that has not been run since a module was published
should not change under the reader's feet — but it is one sentence and no
argument, where every other tool in this space offers `update` or `--force`.

**What would fix it:** a `Pins::forcing` beside `Pins::for_running`, which skips
`check_lockfile` on the way to `write_lockfile`, and a `--force-resolve` on `rb
run`. Nothing in this phase needs it, so nothing in this phase has it.