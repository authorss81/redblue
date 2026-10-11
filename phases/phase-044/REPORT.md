# Phase 044 — Package manager: manifest, versions and lockfile

## Finding reproduced

```
$ ls modules/
MathUtils.rb  SuiteKit.rb
$ find . -path ./target -prune -o \( -name "*manifest*" -o -name "*.lock" \) -print
(nothing)
$ grep -rn "pinned\|version" src/interpreter.rs | head
(nothing)
```

No manifest format, no version and no lockfile existed anywhere in the checkout,
and `import Foo` resolved to exactly one file under exactly one name. Two
programs could not depend on two versions of one module, and there was nothing
to resolve. Verified on `main` before any change; the finding stands.

## What changed

Four runs produced this phase. The first built the manifest, the resolver and
the lockfile. The second reported the resolver's failures to the program being
analyzed. The third closed what the review of the second found. This fourth run
closed what the review of the third found: `rb run` never derived a root from the
program it was running, two module readers reached past the filesystem
abstraction to `std::fs`, the lockfile was written and never read, and the
resolver picked a version by trusting the order a registry handed it rather than
by asking for the highest one.

| File | Run | What |
|---|---|---|
| `src/manifest.rs` | 1st | `Version`, `Requirement`, `Manifest`, the `Registry` trait, `Resolution`, `resolve`, `resolve_in`, `DirRegistry`, `pinned_module_path` |
| `src/lib.rs` | 1st | `pub mod manifest;` |
| `src/interpreter.rs` | 1st | the manifest first in the import search; a directory is skipped, not opened |
| `src/bytecode/vm.rs` | 1st | the same search in the bytecode VM, so both engines resolve an import identically |
| `src/manifest.rs` | 2nd | `pinned_module_path_in` and `failure_in` — the same two answers, asked of a named directory rather than of the working directory |
| `src/interpreter.rs` | 2nd | `module_search_paths`, `module_source` and `module_bound_names` take that directory, so the analyzer reads the pins of the program it is asked about |
| `src/analyzer.rs` | 2nd | `analyze_in`; `Analyzer` holds a root; an `import` whose manifest cannot be resolved is the failure the walk stops for |
| `src/analyzer.rs` | 3rd | the walk stops *at every depth*: `analyze_body` is the one way a body is walked, and `analyze_statement` is the one place the stop is checked. The pins are read once per analysis (`pins`, `failure`) instead of once per import |
| `src/manifest.rs` | 3rd | `MAX_RESOLUTION_DEPTH` counts chain depth per queued edge; `MAX_RESOLUTION_STEPS` counts steps; both are `pub` so a test builds the graph that trips each. `resolve_without_writing` is the read-only resolve for passes that only ask; `resolve_in` keeps the lockfile for the loader. The thread-local resolution cache is gone |
| `src/interpreter.rs` | 3rd | `module_search_paths` takes the pin rather than the root: a caller that has resolved the manifest hands the answer over, and `module_path` is the one that has not |
| `src/bytecode/vm.rs` | 3rd | `BytecodeVm` holds a root (`new_in`), so the loader resolves an import against the same manifest the analyzer was given |
| `src/vfs.rs` | 4th | `rooted(root, relative)` — the one string a search path and a filesystem are both asked about — and `is_file`, so a directory implied by the files under it is not opened |
| `src/lib.rs` | 4th | `run_file` derives the root from the program's own path; `run_source_in` / `run_source_value_in` take one, and `run_file`'s diagnostic path does too |
| `src/interpreter.rs` | 4th | `Vm` holds a root and a `manifest::Pins`; `run_isolated_in` threads the root from `run_source_in` into the loader. `module_source` reads through `crate::vfs` |
| `src/bytecode/vm.rs` | 4th | `compile_module` reads through `crate::vfs`; `BytecodeVm` holds the same `Pins` the analyzer and the tree-walker do |
| `src/analyzer.rs` | 4th | `Analyzer` holds a `manifest::Pins` instead of a `Resolution` and a parallel `failure` field — one way to ask, for all three engines |
| `src/manifest.rs` | 4th | `parse_lockfile` / `LockedModule` / `check_lockfile`, so the lockfile is read and a disagreement refused; `Pins`; `*` takes the maximum of what it was offered rather than the last; the manifest's fallback arm says which way a line is wrong |
| `tests/manifest_test.rs` | 1st | 20 `#[test]`s |
| `tests/manifest_test.rs` | 2nd | 5 more, the analyzer half |
| `tests/manifest_test.rs` | 3rd | 15 more, one per finding, plus a `Memory` registry indexed by name |
| `tests/manifest_test.rs` | 4th | 17 more, one per finding, plus an `Unordered` registry that offers what it is given |

### The gap the second run closed

The first run's claim was "the resolution error reaches the program through
`import`". It reached the program only when the program read nothing from the
module. A program that did read from it got this instead:

```
$ cat redblue.manifest
name report
version 1.0.0
dependency Ghost =1.0.0
$ rb run report.rb          # report.rb: `import Fixture` / `say VALUE`
Error: AnalyzerError: Unknown variable 'VALUE'
  --> report.rb:2:1
```

The loader would have said `Cannot resolve dependency 'Ghost' at 1.0.0: no
version of 'Ghost' is published`, but the analyzer runs first and the failed
import binds no names — so every read of a name it *would* have bound is
answered "unknown variable", pointing at the variable, saying nothing about the
manifest. The phase's own definition of done asks for a clean error naming the
conflict; that is what a user with any module-using program actually got.

### The gaps the third run closed

**The stop was only written for the top level.** `analyze_deferred_body` and
every scoped branch kept walking their bodies after the conflict was recorded,
so the page of consequences came back for every `import` below the top level:

```
$ rb run report.rb       # report.rb: `to read_it()` … `import Fixture` / `say VALUE`
Error: AnalyzerError: Cannot resolve dependency 'Ghost' at 1.0.0: no version of 'Ghost' is published
  Unknown variable 'VALUE'
  Unknown variable 'OTHER'
  Unknown variable 'THIRD'
```

One error per unread name, each of them a consequence of the one conflict. Now
`analyze_body` is the only way a body is walked and it stops; `analyze_statement`
checks the stop before it does anything, so no branch can walk past a conflict
recorded inside another statement's body. Eleven shapes are pinned by
`edge_an_unresolvable_pin_inside_a_body_is_the_conflict`: a function, a method
inside an object, `if`, `unless`, `for each`, `repeat`, `repeat until`, `while`,
`try`, a declared `module` and a `test`.

```
$ rb run report.rb
Error: AnalyzerError: Cannot resolve dependency 'Ghost' at 1.0.0: no version of 'Ghost' is published
  --> report.rb:1:1
1 | import Fixture
  | ^
```

**One failure was reported with two labels.** The manifest's error was stored as
`error.to_string()` and re-wrapped as an `Analyzer` error, so the user read
`AnalyzerError: RuntimeError: Cannot resolve …`. The analyzer now takes the
*message*, which is the manifest's own, and the label belongs to whichever error
is reporting it — there is only one. The manifest's file and line stay in the
message, deliberately: a `Span` is drawn as a caret against the program being
run, and a manifest is a second file, so `--> report.rb:3:1` for a manifest line
3 would point at a line of the program that has nothing to do with it.

**The depth limit was counting width.** `MAX_RESOLUTION_DEPTH` is documented as
how deep one module's chain of dependencies may go, and it was enforced as how
many pins one module's manifest held. So a seventy-long chain resolved (seventy
steps, far inside the 4096-step budget) and a module with seventy pins was
refused. Each queued dependency now carries the depth of the chain that reached
it, and that is what the constant is checked against.

**The two engines resolved against different roots.** The analyzer resolved an
import against the `root` it was handed; the bytecode VM resolved it against the
working directory, out of a cache keyed on nothing. `BytecodeVm::new_in(root)`
now carries the same root, and both are pinned by a pair of scratch roots that
each import a different version of one module.

**A static pass wrote a lockfile.** Every resolve went through the writing path,
and the analyzer ran one per import, so `rb lint` in a directory holding a
manifest created `redblue.lock` beside it. `resolve_without_writing` is the
read-only resolve and the analyzer uses it, once, when the analysis starts rather
than once per import. The *loader* keeps the writing resolve — a program that is
about to run is the resolution the lockfile records, and `rb run` still writes it
on the first resolve exactly as it did.

**A resolution was cached per thread, forever.** `pinned_module_path` kept the
first `resolve_in(".")` answer — pins or failure — for the life of the thread,
keyed on nothing, so a REPL's second line or a harness's second file was
answered from the first program's manifest. The cache is gone. What it saved was
a manifest read and a directory scan per import.

### The gaps the fourth run closed

**`rb run` resolved against the working directory.** The two rooted APIs the
third run added — `analyze_in(&program, &root)` and `BytecodeVm::new_in(&root)` —
were both tested, and the production path was not: `run_file` read the file and
then ran it against `.`. So `rb run subdir/prog.rb` consulted
`./redblue.manifest` and opened `./modules/Fixture/1.0.0/Fixture.rb` while the
manifest beside the program said something else, and the two-programs-two-versions
scenario this phase exists for only worked after a `cd`:

```
$ cat subdir/redblue.manifest
name old
version 0.1.0
dependency Fixture =1.0.0
$ cat redblue.manifest                       # the working directory's, which was consulted
name other
version 0.1.0
dependency Fixture =2.0.0
$ rb run subdir/prog.rb                      # `import Fixture` / `say VALUE`
above
$ rb run subdir/prog.rb                      # after the fix
beside
$ ls subdir/redblue.lock                     # written beside the program
module Fixture 1.0.0 modules/Fixture/1.0.0/Fixture.rb
```

`run_file` now derives the root from the program's own path and threads it into
`analyze_in` and `run_isolated_in`, so the analyzer, the tree-walking loader and
the bytecode loader read one manifest between them rather than two.

**A root could not be threaded without the two rules disagreeing.** This is what
the third run's FINDINGS §1 said, and it is why the fix is in `crate::vfs` rather
than only in the three engines. A module file is read through
`vfs::read_to_string(path)`; giving a VM a `root` would have made `is_file()`
consult `root.join(path)` and the read still take `path` — finding the right file
by one rule and opening whatever is at the relative path by another.
`vfs::rooted(root, relative)` is the one string both take, and on wasm, where the
filesystem is a map from path *text*, that is the difference between
`modules/Fixture.rb` and a second entry `./modules/Fixture.rb` the map has never
heard of.

**Two readers reached past the abstraction to `std::fs`.** `compile_module` and
the analyzer's `module_source` read with `std::fs::read_to_string(self.root
.join(path))` while the tree-walking loader reads through `crate::vfs`, which on
`wasm32-unknown-unknown` is the in-memory filesystem `files.write` writes to. So
on the playground a program that wrote a module and imported it worked through one
engine and failed through the other, and the analyzer — whose
`let Ok(...) else { return Vec::new() }` swallows a read that fails — bound zero
names and reported every read of one as an unknown variable. Both read through
`crate::vfs` now. This is invisible to `cargo test` on the host, where `vfs` *is*
`std::fs` and the two are indistinguishable, so
`edge_no_module_loader_reads_a_file_behind_the_filesystem_abstraction` asserts it
from the source and `edge_the_module_search_can_be_answered_from_the_in_memory_filesystem`
exercises the two questions the search asks (`is_file` and `read_to_string`)
against the map that has to answer them on the playground.

**The lockfile was a log, not a lock.** `write_lockfile` existed, nothing parsed
`redblue.lock`, and `resolve_in` recomputed from the manifest and overwrote it
every time. A version published after the lock was written was handed to the
program in silence — `*` resolved to the new maximum, the program ran different
code from the one it was locked against, and the lockfile went on saying what it
had always said:

```
$ rb run prog.rb                      # dependency Fixture *, one version published
one
$ cat redblue.lock
module Fixture 1.0.0 modules/Fixture/1.0.0/Fixture.rb
$ mkdir modules/Fixture/2.0.0         # a module is published
$ rb run prog.rb                      # used to print "two" and say nothing
Error: RuntimeError: Cannot honour 'redblue.lock': module 'Fixture' disagrees with
the resolution — it is locked at 1.0.0 and this manifest resolves it to 2.0.0.
The world moved under the lock; delete 'redblue.lock' to resolve again against what
is published now
$ grep module redblue.lock             # a refused resolve writes nothing
module Fixture 1.0.0 modules/Fixture/1.0.0/Fixture.rb
$ rm redblue.lock && rb run prog.rb    # the way out, named in the message
two
```

A lockfile that disagrees the other way — a module it holds that the manifest no
longer pins — is refused the same way. The refusal is on the *loader* path only:
the analyzer reads the manifest and writes nothing, so `rb lint` in a directory
with a stale lockfile still answers its question rather than failing it.

**A manifest line with too many words said it was missing one.**
`dependency Math.Utils =1.0.0 extra` reported "it carries no value, where one is
required", which is backwards for surplus input — everything the line needs is
written and something else is written beside it. The fallback arm is split, and
the surplus message names the counts.

**`*` took the version a registry listed last.** `available.last()` with the
`Registry` trait documenting that it would be handed versions in ascending order.
Both bundled registries sort, so every test agreed with it; any third-party
registry offering `1.10.0` before `1.9.0` would have had `*` resolve to `1.9.0`
with nothing to say so. `available.iter().max()` asks the question the resolver
should have been asking all along, and the trait's documentation no longer makes
a promise it was not entitled to.

**Every `import` re-read the manifest.** `module_paths` called
`pinned_module_path_in` → a full `resolve_in` — manifest re-read, `modules/*/*`
directory scan, lockfile read-compare — once per `IMPORT` instruction, and the
tree-walker did the same per import. The analyzer had been fixed to read once for
the whole program in the third run; the two loaders had not.
`manifest::Pins` is now the one way all three ask, read on first question rather
than per import, and held by the thing running the program rather than by a
process-wide cache keyed on nothing.

**`failure_in` was dead public API duplicating the analyzer.** It had no callers
after the third run inlined its logic into `Analyzer::analyze`, which is two
implementations of one question. It is gone, and the answer lives in
`manifest::Pins::failure` — which the analyzer, the tree-walker and the bytecode
VM all call, so the three cannot drift.

### The manifest schema

Line-oriented, like the rest of the language, beside the program that owns it —
`redblue.manifest`:

```text
name report
version 1.0.0
dependency MathUtils =1.0.0
dependency SuiteKit *
```

- `name` / `version` — what the program calls itself. Optional in a program
  manifest, refused if written twice.
- `dependency <name> <requirement>` — one pin. `*` is the highest version
  published — the maximum of whatever the registry offers, in whatever order it
  offers them; `=x.y.z` is exactly one version.
- `#` starts a comment; blank lines are ignored; a leading BOM and CRLF endings
  are accepted.
- A module version is `modules/<name>/<version>/<name>.rb`, with its own
  `redblue.manifest` beside it. Those manifests are read too, so a module's own
  dependencies are resolved by the same loop as the program's.

### The lockfile

`redblue.lock`, written beside the manifest on the first successful resolve and
rewritten with identical bytes on every later one. It is built by walking a
`BTreeMap` keyed by module name, so its order does not depend on a directory
listing or on the order the manifest was written in. A resolution with nothing
in it writes nothing, and a lockfile already on disk is never deleted.

It is also read. A lockfile that is only written records what the last resolve
found, which is a log; a program pinned with `*` would be handed a new version
the moment one was published, in silence. So a lockfile that disagrees with the
resolution is refused, naming the module that moved and the file to delete:

```
Cannot honour 'redblue.lock': module 'Fixture' disagrees with the resolution — it is locked
at 1.0.0 and this manifest resolves it to 2.0.0. The world moved under the lock; delete
'redblue.lock' to resolve again against what is published now
```

Both directions are a disagreement: a locked version the resolution no longer
produces, and a locked module the manifest no longer pins. The refusal is on the
*loader* path only — a static pass reads the manifest and writes nothing, so
`rb lint` beside a stale lockfile still answers its question rather than failing
it.

### Failing

Every failure is a `Runtime` error naming the conflict, raised by the resolver
and surfaced by whichever of the analyzer or the loader runs first:

```
Cannot resolve dependency 'Ghost' at 1.0.0: no version of 'Ghost' is published
Cannot resolve dependency 'MathUtils' at 1.5.0: it is not published. Available: 1.0.0, 2.0.0
Module 'MathUtils' is required at two versions: =1.0.0 (required by Report 1.0.0), and =2.0.0 (required by this program). One program cannot depend on two versions of one module
```

## Tests added

| Test | Edge class covered |
|---|---|
| `two_programs_pin_two_versions_of_one_module` | the finding: two pins, two versions |
| `two_programs_load_two_versions_of_one_fixture_module` | both pinned files load through the real lexer and parser |
| `resolutions_of_two_pins_do_not_share_state` | resource/state — no state leaks between resolutions |
| `wildcard_takes_the_highest_version_by_number` | numeric boundary — `1.10.0` above `1.9.0`, not below |
| `a_module_dependencies_are_resolved_too` | nesting — a module's own pins are resolved with the program's |
| `edge_conflicting_pins_name_both_versions` | malformed/duplicate — a transitive two-version conflict |
| `edge_two_pins_of_one_module_in_one_manifest_conflict` | duplicate key — the same module pinned twice |
| `edge_the_same_pin_twice_is_one_dependency` | duplicate key — the same pin twice is one dependency |
| `edge_unresolvable_pin_lists_the_published_versions` | failure — names what was asked for and what is there |
| `edge_pin_on_an_unpublished_module_is_refused` | failure — a different question, said differently |
| `edge_zero_version_resolves_when_it_is_published` | boundary — `0.0.0` pinned exactly |
| `edge_empty_manifest_resolves_to_nothing` | empty |
| `edge_comment_only_manifest_resolves_to_nothing` | empty — BOM/CRLF/whitespace-only |
| `edge_bom_and_crlf_manifest_parses` | malformed input — a Windows-written manifest |
| `edge_malformed_manifest_lines_are_refused_by_line` | malformed input — 5 shapes, each naming its line |
| `edge_a_field_declared_twice_is_refused` | duplicate key — `name`/`version` twice |
| `edge_module_name_that_is_not_an_identifier_is_refused` | unicode/type mismatch — `依赖`, `Math.Utils`, `Math-Utils`, `""` |
| `edge_empty_manifest_file_is_not_an_error` | empty — an empty file resolves to nothing and writes nothing |
| `edge_a_root_with_no_manifest_resolves_to_nothing` | empty — pre-manifest behaviour is unchanged |
| `lockfile_is_written_on_the_first_resolve_and_is_byte_identical_afterwards` | written on first resolve; re-resolve byte-identical; name order |
| **`edge_an_unresolvable_pin_is_an_analyzer_error_naming_the_conflict`** | **failure — the conflict reaches the program, and not as an unknown variable** |
| **`edge_two_versions_in_one_manifest_is_an_analyzer_error`** | **failure — the same for the conflict this phase exists over** |
| **`edge_an_unresolvable_pin_is_reported_even_with_nothing_read`** | **failure — a bare `import` is still an import that failed** |
| **`edge_an_import_binds_the_names_of_the_pinned_version`** | **boundary — the two versions declare different names and each program gets its own** |
| **`edge_a_root_with_no_manifest_analyses_as_it_always_did`** | **regression — adding a root changed nothing for a program that has no manifest** |
| **`edge_an_import_in_a_body_binds_names_when_the_manifest_resolves`** | **regression — the eleven body shapes bind what they bind with a manifest that resolves** |
| **`edge_an_unresolvable_pin_inside_a_body_is_the_conflict`** | **failure — a conflict below the top level is the conflict, not its consequences** |
| **`edge_a_conflict_in_a_body_is_reported_once`** | **failure — one conflict, one error, not one error per unread name** |
| **`edge_a_chain_deeper_than_the_limit_is_refused`** | **resource_limit — depth is the chain, not the width** |
| **`edge_a_chain_exactly_at_the_depth_limit_resolves`** | **boundary — the other side of the depth limit** |
| **`edge_a_module_with_more_pins_than_the_depth_limit_resolves`** | **resource_limit — a wide manifest is width, and width is the step budget's** |
| **`edge_a_graph_larger_than_the_step_budget_is_refused`** | **resource_limit — `MAX_RESOLUTION_STEPS` fires, and names its budget** |
| **`edge_a_graph_one_step_under_the_budget_resolves`** | **boundary — the other side of the step budget** |
| **`two_programs_load_their_pinned_version_on_the_bytecode_vm`** | **cross-engine — each program loads its own pin, twice over** |
| **`edge_both_engines_agree_on_which_version_an_import_binds`** | **cross-engine — the analyzer and the loader, one root, one version** |
| **`edge_the_bytecode_vm_refuses_an_unresolvable_pin_from_its_own_root`** | **failure — the loader refuses a pin too, from its own root** |
| **`edge_analysing_a_program_writes_no_lockfile`** | **resource/state — a static pass reads and does not write** |
| **`edge_a_manifest_failure_is_reported_once_without_a_second_label`** | **failure — one failure, one label, at the import that found it** |
| **`edge_a_malformed_manifest_keeps_its_line_and_the_import_keeps_the_caret`** | **failure — the manifest's line travels in the message; the caret stays on the program** |
| **`edge_two_roots_in_one_process_keep_their_own_pins`** | **resource/state — no resolution leaks from one program to the next** |

57 `#[test]`s (floor is 3), 47 named `edge_*` (floor is 1), 21 of them assert
that a failure is produced. Zero `#[ignore]`, zero `.skip`, zero `allow(clippy::`.

The fifteen marked in bold are the third run's, and the seventeen marked in the
seventeen rows after them are this run's — one per finding. Every one of this
run's was watched fail first, with the fix that made it pass taken back out:

```
$ cargo test --test manifest_test   # with the walk stop taken out
test edge_a_conflict_in_a_body_is_reported_once ... FAILED
test edge_an_unresolvable_pin_inside_a_body_is_the_conflict ... FAILED
assertion `left == right` failed: one conflict is one error, not one error per
unread name: Cannot resolve dependency 'Ghost' at 1.0.0: no version of 'Ghost' is published
Unknown variable 'VALUE'
Unknown variable 'OTHER'
Unknown variable 'THIRD'
  left: 4
 right: 1

$ cargo test --test manifest_test   # with the depth limit back as a width check
test edge_a_chain_deeper_than_the_limit_is_refused ... FAILED
test edge_a_module_with_more_pins_than_the_depth_limit_resolves ... FAILED

$ cargo test --test manifest_test   # with the bytecode VM on the working directory again
test two_programs_load_their_pinned_version_on_the_bytecode_vm ... FAILED
test edge_both_engines_agree_on_which_version_an_import_binds ... FAILED
test edge_the_bytecode_vm_refuses_an_unresolvable_pin_from_its_own_root ... FAILED

$ cargo test --test manifest_test   # with the writing resolve behind the analyzer
test edge_analysing_a_program_writes_no_lockfile ... FAILED

$ cargo test --test manifest_test   # with `error.to_string()` in the analyzer
test edge_a_manifest_failure_is_reported_once_without_a_second_label ... FAILED
the label of the error the message came from must not be printed inside it,
got: RuntimeError: Cannot resolve dependency 'Ghost' at 1.0.0: no version of 'Ghost' is published

$ cargo test --test manifest_test   # with `run_file` back on the working directory
test running_a_program_resolves_the_manifest_beside_it ... FAILED
test edge_a_program_with_no_manifest_of_its_own_still_imports ... FAILED
test edge_every_engine_agrees_about_the_program_a_file_names ... FAILED
test two_programs_run_from_their_own_directories_pin_two_versions ... FAILED
.../subdir/prog.rb should run, failed with AnalyzerError: Unknown variable 'VALUE'
  --> 2:1

$ cargo test --test manifest_test   # with the tree-walking loader back on `.`
test edge_the_tree_walking_vm_resolves_the_manifest_once_per_program ... FAILED
test running_a_program_resolves_the_manifest_beside_it ... FAILED
test two_programs_run_from_their_own_directories_pin_two_versions ... FAILED

$ cargo test --test manifest_test   # with `check_lockfile` removed
test edge_a_version_published_after_the_lock_is_refused ... FAILED
test edge_deleting_the_lockfile_re_resolves ... FAILED
test edge_a_locked_module_the_manifest_no_longer_pins_is_refused ... FAILED

$ cargo test --test manifest_test   # with `available.last()` back
test edge_wildcard_takes_the_highest_version_whichever_order_they_arrive_in ... FAILED
test edge_wildcard_is_numeric_however_the_registry_orders_them ... FAILED
assertion `left == right` failed: offered as ["2.0.0", "1.0.0"], `*` still takes
the highest version
  left: "1.0.0"
 right: "2.0.0"

$ cargo test --test manifest_test   # with the surplus arm folded back into the deficit one
test edge_module_name_that_is_not_an_identifier_is_refused ... FAILED
'Math Utils' should not be a module name, got: redblue.manifest line 1:
'dependency Math Utils =1.0.0' carries no value, where one is required

$ cargo test --test manifest_test   # with the bytecode loader resolving per IMPORT
test edge_the_tree_walking_vm_resolves_the_manifest_once_per_program ... FAILED

$ cargo test --test manifest_test   # with `std::fs` back in a module reader
test edge_no_module_loader_reads_a_file_behind_the_filesystem_abstraction ... FAILED
src/interpreter.rs reaches the filesystem with `std::fs::read_to_string`, which
compiles on every target and fails only on the one with no filesystem
```

## Edge-case matrix — every row

- **empty** — covered: `edge_empty_manifest_resolves_to_nothing`,
  `edge_comment_only_manifest_resolves_to_nothing`,
  `edge_empty_manifest_file_is_not_an_error`, and
  `edge_a_root_with_no_manifest_analyses_as_it_always_did`.
- **singleton** — covered: `edge_the_same_pin_twice_is_one_dependency` is one
  dependency written twice and resolves to one module, and
  `edge_an_import_binds_the_names_of_the_pinned_version` is a program with
  exactly one import against exactly one published module.
- **boundary** — covered: `edge_zero_version_resolves_when_it_is_published`
  (`0.0.0`, the lowest version there is);
  `edge_an_import_binds_the_names_of_the_pinned_version`, where the pin decides
  *which* file an import reads and each pinned version declares a different name;
  and, from this run, all four limits with a test on each side of them —
  `edge_a_chain_exactly_at_the_depth_limit_resolves` and
  `edge_a_graph_one_step_under_the_budget_resolve` against
  `edge_a_chain_deeper_than_the_limit_is_refused` and
  `edge_a_graph_larger_than_the_step_budget_is_refused`.
- **out_of_bounds** — N/A. Nothing in the schema is indexed by position: a
  manifest is a set of keyed lines and a resolution is a map keyed by module
  name. There is no index a caller can make out of bounds. `out_of_bounds` is
  covered for the language itself by `tests/index_bounds_test.rs`.
- **type_mismatch** — covered as `edge_module_name_that_is_not_an_identifier_is_refused`:
  a module name is either an identifier or the manifest is refused, and a
  requirement is either `*` or `=x.y.z` or the manifest is refused. There is no
  coercion to get wrong because there is no coercion.
- **numeric_boundary** — covered: `1.10.0` vs `1.9.0`
  (`wildcard_takes_the_highest_version_by_number`) and `0.0.0`. A version
  component past `u64::MAX` is refused by `Version::parse`, not wrapped.
- **unicode** — covered: `edge_module_name_that_is_not_an_identifier_is_refused`
  refuses `依赖`, and `edge_bom_and_crlf_manifest_parses` covers a byte-order
  mark, which is the one non-ASCII thing that reaches a manifest legitimately.
- **nesting_recursion** — covered at both ends of the range. A module's own
  pins come in through the same queue as the program's
  (`a_module_dependencies_are_resolved_too`); a chain at the limit resolves and
  one link deeper is refused (`edge_a_chain_exactly_at_the_depth_limit_resolves`,
  `edge_a_chain_deeper_than_the_limit_is_refused`); and a cycle ends because a
  module already resolved is skipped. There is no recursion in the resolver — it
  is a work queue — so the depth limit is what stands in for a stack.
- **duplicate_missing_keys** — covered: `name`/`version` twice
  (`edge_a_field_declared_twice_is_refused`), one module pinned twice
  (`edge_two_pins_of_one_module_in_one_manifest_conflict`), the same pin twice
  (`edge_the_same_pin_twice_is_one_dependency`), and a pin on a module that
  publishes nothing (`edge_pin_on_an_unpublished_module_is_refused`).
- **malformed_input** — covered: BOM/CRLF, five malformed line shapes each naming
  its line, an empty file, a missing file, and a version directory that
  publishes no module file (filtered in `DirRegistry::versions`, so `*` cannot
  choose a path that is not there). The lockfile is held to the same standard by
  `edge_a_malformed_lockfile_is_refused_by_line`: four shapes, each naming its
  line, and `edge_a_lockfile_with_comments_and_blank_lines_parses` for what the
  writer itself emits.
- **resource_limit** — covered on both sides of both caps, and this run is what
  added them. `MAX_RESOLUTION_DEPTH` (64) is a chain-depth cap and
  `edge_a_chain_deeper_than_the_limit_is_refused` builds the chain that trips it,
  while `edge_a_module_with_more_pins_than_the_depth_limit_resolves` pins the
  other half: width is not depth, and a wide manifest resolves.
  `MAX_RESOLUTION_STEPS` (4096) is the total-step cap,
  `edge_a_graph_larger_than_the_step_budget_is_refused` builds a module with four
  thousand leaves and `edge_a_graph_one_step_under_the_budget_resolves` builds one
  step under. A cycle ends because an already-settled module is skipped. Both
  constants are `pub` so the tests build the graph the *number* describes rather
  than a copy of the number.
- **state between runs** — covered: `edge_two_roots_in_one_process_keep_their_own_pins`
  and `two_programs_load_their_pinned_version_on_the_bytecode_vm` both ask a
  second program, and then the first again, in one process. The per-thread
  resolution cache that used to answer all three from the first program's
  manifest is gone. `running_a_program_resolves_the_manifest_beside_it` and
  `two_programs_run_from_their_own_directories_pin_two_versions` are the same
  question asked of `rb run <file>` rather than of a rooted API, which is where
  it was still unanswered until this run.
- **side effects of a read** — covered: `edge_analysing_a_program_writes_no_lockfile`
  runs four imports past a manifest that resolves and asserts no lockfile
  appeared, then asserts that the resolving path does write one. The lockfile
  added by this run does not make a reader a writer: `check_lockfile` is on
  `resolve_in` and `Pins::for_reading` is not that, so a stale lock beside a
  manifest that resolves cleanly is a question the analyzer still answers.
- **cross-target** — covered: `edge_the_module_search_can_be_answered_from_the_in_memory_filesystem`
  exercises the two questions a module search asks (`is_file`, and the bytes)
  against the in-memory map that has to answer them on the playground;
  `edge_a_root_is_joined_to_a_search_path_the_way_a_program_names_it` pins the
  two spellings of "no root" that `Path::parent` and `Path::new` produce; and
  `edge_no_module_loader_reads_a_file_behind_the_filesystem_abstraction` asserts
  from the source what no native test can observe, because `crate::vfs` *is*
  `std::fs` on this target and the two are indistinguishable here.
- **resource** — covered: `edge_the_tree_walking_vm_resolves_the_manifest_once_per_program`
  on both loaders, by deleting the program's own manifest between its first and
  second import. A resolver that re-reads finds nothing the second time; one that
  read once for the program answers both imports from the pins it started with.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — zero warnings |
| `cargo test --all-targets` | pass — exit 0, 48 binaries, **1315 passed, 0 failed, 0 ignored** |
| `cargo test --doc` | pass — 2 passed, 0 failed |
| `./rbops/verify.sh phase-044` | **not run** — `rbops/` does not exist in this checkout; the dispatcher invokes it from outside the project tree. The other gates were run here by hand and are quoted verbatim. |

## Verified by hand, end to end

Run with `target/debug/rb` in a scratch directory holding one module published
at two versions:

| Command | Output |
|---|---|
| manifest pins `=1.0.0`, `rb run prog.rb` | `one` |
| lockfile after that resolve | `module Fixture 1.0.0 modules/Fixture/1.0.0/Fixture.rb` |
| same resolve run again | lockfile byte-identical (`diff` clean) |
| manifest pins `=2.0.0` in a *second* directory, `rb run prog.rb` there | `two` |
| `rb run subdir/prog.rb`, manifests in both directories | `beside` — the program's own, and its lockfile lands beside it |
| `rb run /abs/path/prog.rb` with `modules/` beside it | the program's own modules |
| `rb run prog.rb` with no manifest anywhere | unchanged: `modules/MathUtils.rb` from the repo root still loads |
| a version published after the lock was written | `Cannot honour 'redblue.lock': … delete 'redblue.lock' …`, and the lock is left as it was |
| `rm redblue.lock`, run again | takes what is published now |
| `rb lint` / `rb format` over the same program | no `redblue.lock` created — a static pass reads |
| manifest pins a module nothing publishes | `AnalyzerError: Cannot resolve dependency 'Ghost' at 1.0.0: no version of 'Ghost' is published` |
| the same, with the import inside a function body | the same one error, caret on the import at `body.rb:2:5` |
| manifest pins one module at two versions | `AnalyzerError: Module 'Fixture' is required at two versions: =1.0.0 (required by this program), and =2.0.0 (required by this program). One program cannot depend on two versions of one module` |
| manifest with a misspelled field | `AnalyzerError: redblue.manifest line 2: 'depndency' is not a field a manifest has; …` |
| no manifest, `import MathUtils` in this repo | `3.14159` — unchanged |

Each failure is one error, not one error plus a page of consequences, and none
of them is printed with two labels.

## Invariants touched

- None of the language invariants. `import` for a program with no manifest is
  unchanged: the same three search paths in the same order, and a missing module
  is still `Cannot find module 'X'`. `.rb` is still the extension; no grammar,
  lexer, parser, `Value` variant or `Error` variant was added or changed.
- **Behaviour change, deliberate (first run):** `import` now looks at a
  `redblue.manifest` in the working directory first. A program with a manifest
  is a program its author asked to be pinned; a program without one is
  unaffected.
- **Behaviour change, deliberate (first run):** an import path that names a
  directory is skipped rather than opened. `modules/Fixture/` is where versions
  live now, so `Is a directory` would be a confusing way to say "there is no
  module file here". No test asserted the old message.
- **Behaviour change, deliberate (2nd run):** an `import` in a program whose
  manifest cannot be resolved stops the analyzer's walk at that statement. Before
  it, the walk continued and every name the import would have bound answered
  "unknown variable" — consequences of the conflict, none of which the reader
  could act on. The program is rejected either way; only the message changed.
- **Behaviour change, deliberate (this run):** that stop now applies at every
  depth, not only at the top level. A conflict found inside a function, a method,
  an `if`, a loop, a `try`, a declared `module` or a `test` ends the walk
  exactly as one at the top level does. Before, the reader of such a program got
  the conflict *and* one "unknown variable" per unread name around it.
- **Behaviour change, deliberate (this run):** a manifest failure is reported
  once, with the label of the error reporting it rather than the label of the
  error the message came from. `AnalyzerError: RuntimeError: Cannot resolve …`
  was one failure printed twice over.
- **Behaviour change, deliberate (this run):** `MAX_RESOLUTION_DEPTH` counts
  chain depth, which is what it documents. A chain of 65 links is now refused
  and a module holding 65 pins is no longer refused. Both were previously the
  other way round.
- **Behaviour change, deliberate (this run):** `pinned_module_path` is no longer
  cached. It used to keep the first resolve of the process, per thread, for the
  life of that thread, so a second program in one process was answered from the
  first program's manifest. What it cost was a manifest read and a directory scan
  per import.
- **Behaviour change, deliberate (this run):** a program resolves its imports
  against *its own directory*, not the working directory. `rb run
  subdir/prog.rb` consults `subdir/redblue.manifest` and writes
  `subdir/redblue.lock`. A program run from its own directory — `rb run prog.rb`,
  or `rb run /abs/path/prog.rb` with its modules beside it — is unchanged, and a
  program with no manifest still resolves through the same three search paths in
  the same order.
- **Behaviour change, deliberate (this run):** a lockfile that disagrees with the
  resolution is refused rather than overwritten. A module published after the
  lock was written no longer reaches the program silently; the message names the
  module, both versions, and `redblue.lock` as the file to delete. A resolution
  that agrees writes exactly the bytes it wrote before, so a second run of an
  unchanged checkout still touches nothing.
- **Behaviour change, deliberate (this run):** a manifest line carrying too many
  words is reported as carrying too many, not as carrying none.
- **Behaviour change, deliberate (this run):** `*` takes the highest version of
  whatever it is offered, in whatever order it is offered. Both bundled
  registries sort, so this changes nothing they can see.
- **New API, additive:** `redblue::analyzer::analyze_in`,
  `redblue::manifest::{Pins, LockedModule, parse_lockfile}` /
  `pinned_module_path_in` / `resolve_without_writing`,
  `redblue::bytecode::vm::BytecodeVm::new_in`,
  `redblue::vfs::{rooted, is_file}`,
  `redblue::{run_source_in, run_source_value_in, run_isolated_in,
  run_isolated_with_depth_in}`, `redblue::Vm::new_in`, and
  `manifest::{MAX_RESOLUTION_STEPS, MAX_RESOLUTION_DEPTH}` as public constants.
  `analyze`, `pinned_module_path`, `run_source` and `run_isolated` are unchanged
  in behaviour and are what they now call.
- **Removed API:** `redblue::manifest::failure_in`. It had no callers, and the
  question it answered now lives in `manifest::Pins::failure`, which the
  analyzer and both loaders call — one implementation rather than two.

## Known gaps / follow-ups

- `rb test` resolves a program's imports against the working directory while
  `rb run` resolves them against the program's own. That is not an oversight: the
  harness runs a *suite*, and every file in `tests/` has to resolve against the
  one root the command was invoked from — the repository's own
  `tests/test_modules.rb` imports `MathUtils` from `<repo>/modules/`, and a root
  per file would resolve it against `<repo>/tests/modules/` and fail every module
  test. A `--root` flag is what would reconcile them. → FINDINGS.md §7
- A refused resolve has no flag for the re-resolve that would write it; deleting
  `redblue.lock` is the only way past one. That is the right default and it is
  one sentence of a CLI away from the usual answer. → FINDINGS.md §8
- Requirements are `*` and `=x.y.z` only. No range, no registry, no network
  fetch; a module version is a directory in the checkout. That is the smallest
  thing that lets two programs pin two versions, which is what this phase asked
  for. → FINDINGS.md §5
