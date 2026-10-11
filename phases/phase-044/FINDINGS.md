# phase-044 — findings

Work that belongs to a later phase, with the evidence for it. Nothing here was
done under phase-044's name; none of it is on-topic for the manifest and
lockfile it was building.

## 1. An `import` that resolves a pinned version has no automated test

**Where:** `src/interpreter.rs:413` (`module_search_paths`),
`src/bytecode/vm.rs:580` (`module_paths`), both of which now consult
`crate::manifest::pinned_module_path` before the three existing search paths.

**What:** the behaviour was verified by hand (`rb run` in a scratch directory,
output quoted in `REPORT.md`) and indirectly, by the existing import tests
still passing with the search-path function now returning `Result`. It has no
automated end-to-end test, because `pinned_module_path` resolves against
`Path::new(".")`: an automated test would have to call `set_current_dir`, and
`cargo test` runs test binaries in parallel threads sharing one process
working directory. One such test would make every other test in the binary
depend on when it ran.

**What would fix it:** a root parameter threaded from the VM through to
`pinned_module_path`, or a `Vfs`-level root so that the working directory is
data rather than process state. `src/vfs.rs` already exists for exactly this
reason (it is why a module load on wasm reads the in-memory filesystem) and is
the obvious place. Until then, a regression in the `import` wiring would not be
caught by `cargo test`.

## 2. `MAX_RESOLUTION_STEPS` and `MAX_RESOLUTION_DEPTH` are untested

**Where:** `src/manifest.rs:68` and `src/manifest.rs:72`.

**What:** both caps are reachable only from a registry offering thousands of
modules or a chain deeper than 64, and neither has a test. Constructing either
fixture costs more than the refusal proves, and a registry that offered fresh
names forever is not something the filesystem can do.

**What would fix it:** a `Registry` implementation in the test file that counts
its `versions()` calls and stops answering after 4096, asserting the message.
The `Registry` trait exists precisely so a test can be that registry — it was
not written here because a 4096-iteration test to prove a counter fires is
self-justifying.

## 3. The lockfile is written, never read

**Where:** `src/manifest.rs:611` (`write_lockfile`) — there is no
`parse_lockfile`.

**What:** a resolve always recomputes from the manifest and overwrites the
lockfile. A lockfile that disagreed with the manifest — a module directory
deleted after the lock was written, a manifest edited without re-running — is
corrected rather than refused. That is the safe direction, but it means the
lockfile records what the last resolve found rather than being able to detect
that the world moved underneath it.

**What would fix it:** read the lockfile, compare it to the resolution, and
refuse a disagreement as a `Runtime` error naming the module whose version
moved — unless a flag asked for the re-resolve that would write it.

## 4. Requirements are `*` and `=x.y.z`; there is no range and no registry

**Where:** `src/manifest.rs:136` (`Requirement::parse`).

**What:** `^1.2`, `>=1.2` and `1.x` are refused with a message saying so. A
module version is a directory in the checkout (`src/manifest.rs`, `DirRegistry`);
nothing is fetched. This is the smallest thing that lets two programs pin two
versions of one module, which is what phase-044 asked for, and nothing more.

**What would fix it:** a range requirement is ~30 lines over the same `Version`
comparison the wildcard already uses. A registry means a network protocol, a
cache directory and a trust decision, and is a phase of its own.

## 5. A directory in a module search path used to be an `Io` error

**Where:** the finder in `src/interpreter.rs:437` and `src/bytecode/vm.rs:3393`
now requires `is_file()` rather than `exists()`.

**What:** before this phase, `import Fixture` with only `modules/Fixture/`
present answered `Cannot load module './modules/Fixture': Is a directory`. No
test asserted that message. It was changed here rather than left because this
phase is what made `modules/<name>/` mean something — a directory named after a
module is now the normal shape of a published module, and opening one was never
the right answer.