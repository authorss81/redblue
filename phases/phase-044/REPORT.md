# Phase 044 — Package manager: manifest, versions and lockfile

## Finding reproduced

```
$ find . -path ./target -prune -o \( -name "*manifest*" -o -name "*.lock" \) -print
$ grep -n "module_search_paths" -A 6 src/interpreter.rs   # before the change
fn module_search_paths(name: &str) -> Vec<String> {
    vec![
        format!("modules/{name}.rb"),
        format!("./modules/{name}"),
        name.to_string(),
    ]
}
```

No manifest format, no version and no lockfile existed anywhere in the checkout,
and `import Foo` resolved to exactly one file under exactly one name. Two
programs could not depend on two versions of one module, and there was nothing
to resolve. Verified on `main` before any change; the finding stands.

## What changed

| File | Lines | What |
|---|---|---|
| `src/manifest.rs` | +656 | New. `Version`, `Requirement`, `Manifest`, the `Registry` trait, `Resolution`, `resolve`, `resolve_in`, `DirRegistry`, `pinned_module_path` |
| `src/lib.rs` | +1 | `pub mod manifest;` |
| `src/interpreter.rs` | +43 −18 | `module_search_paths` asks the manifest first and returns `Result`; `module_path` skips a directory |
| `src/bytecode/vm.rs` | +28 −11 | `module_paths` mirrors both, so both VMs resolve an import identically |
| `tests/manifest_test.rs` | +525 | 20 new `#[test]`s |

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
  published; `=x.y.z` is exactly one version.
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

### Failing

Every failure is a `Runtime` error naming the conflict:

```
Cannot resolve dependency 'Ghost' at 1.0.0: no version of 'Ghost' is published
Cannot resolve dependency 'MathUtils' at 1.5.0: it is not published. Available: 1.0.0, 2.0.0
Module 'MathUtils' is required at two versions: =1.0.0 (required by Report 1.0.0), and =2.0.0 (required by this program). One program cannot depend on two versions of one module
```

The resolution error reaches the program through `import`, not around it: an
unresolvable pin is never answered with whichever file happens to be at
`modules/<name>.rb`.

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

20 new `#[test]`s (floor is 3), 14 named `edge_*` (floor is 1), 7 assert a
failure is produced. Zero `#[ignore]`, zero `.skip`, zero `allow(clippy::`.

### Edge-case matrix — rows not covered, and why

- **out_of_bounds** — N/A. Nothing in the schema is indexed by position: a
  manifest is a set of keyed lines and a resolution is a map keyed by module
  name. There is no index a caller can make out of bounds. `out_of_bounds` is
  covered for the language itself by `tests/index_bounds_test.rs`.
- **numeric_boundary** — covered in the only form that reaches this phase:
  `1.10.0` vs `1.9.0` (`wildcard_takes_the_highest_version_by_number`) and
  `0.0.0` (`edge_zero_version_resolves_when_it_is_published`). A version
  component past `u64::MAX` is refused by `Version::parse`, not wrapped.
- **type_mismatch** — covered as `edge_module_name_that_is_not_an_identifier_is_refused`:
  a module name is either an identifier or the manifest is refused, and a
  requirement is either `*` or `=x.y.z` or the manifest is refused. There is no
  coercion to get wrong because there is no coercion.
- **resource_limit** — covered in the resolver: `MAX_RESOLUTION_STEPS` (4096)
  and `MAX_RESOLUTION_DEPTH` (64) refuse a graph larger than a package manager
  walks. No test constructs a 4096-node graph because the unit under test is the
  refusal, and building one would cost more than it proves; recorded in
  FINDINGS.md.
- **nesting_recursion** — covered to the depth this phase defines (a module's
  manifest brings its own pins in, one level, through the same loop) by
  `a_module_dependencies_are_resolved_too`. There is no recursion in the
  resolver: it is a work queue, and a cycle ends because a module already
  resolved is skipped.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — zero warnings |
| `cargo test --all-targets` | pass — exit 0, 48 `test result: ok`, 0 failed |
| `cargo test --doc` | pass — 2 passed, 0 failed |
| `./rbops/verify.sh phase-044` | **not run** — `rbops/` does not exist in this checkout; the dispatcher invokes it from outside the project tree. The three gates above were run by hand and are quoted verbatim. |

## Verified by hand, end to end

Run with `target/debug/rb` in a scratch directory holding one module published
at two versions:

| Command | Output |
|---|---|
| manifest pins `=1.0.0`, `rb run old.rb` | `1.0.0` |
| lockfile after that resolve | `module Fixture 1.0.0 modules/Fixture/1.0.0/Fixture.rb` |
| manifest pins `=2.0.0`, `rb run new.rb` | `2.0.0`, lockfile rewritten with the same shape |
| manifest pins a module nothing publishes | `Cannot resolve dependency 'Ghost' at 1.0.0: no version of 'Ghost' is published` |
| manifest pins one module at two versions | `Module 'Fixture' is required at two versions: … One program cannot depend on two versions of one module` |
| manifest with a misspelled field | `redblue.manifest line 1: 'depndency' is not a field a manifest has` |
| no manifest at all, `import MathUtils` | `3.14159` — unchanged |
| no manifest, only a versioned module directory | `Cannot find module 'Fixture'` (was `Is a directory`) |

## Invariants touched

- None of the language invariants. `import` is unchanged for a program with no
  manifest: the same three search paths, in the same order, and a missing
  module is still `Cannot find module 'X'`. `.rb` is still the extension; no
  grammar, lexer, parser, analyzer or `Value` variant was touched.
- **Behaviour change, deliberate:** `import` now looks at a `redblue.manifest`
  in the working directory first. A program with a manifest is a program the
  author asked to be pinned; a program without one is unaffected.
- **Behaviour change, deliberate:** an import path that names a directory is
  skipped rather than opened. `modules/Fixture/` is where versions live now, so
  `Is a directory` would be a confusing way to say "there is no module file
  here". No test asserted the old message.

## Known gaps / follow-ups

- The `import`-side wiring is verified by hand and by the unchanged behaviour of
  every existing import test, not by an automated end-to-end test: an automated
  one would have to change the process's working directory, which `cargo test`
  shares between parallel tests. → FINDINGS.md
- `MAX_RESOLUTION_STEPS` / `MAX_RESOLUTION_DEPTH` have no test. → FINDINGS.md
- Requirements are `*` and `=x.y.z` only. There is no range (`^1.2`), no
  registry, and no network fetch; a module version is a directory in the
  checkout. That is the smallest thing that lets two programs pin two versions,
  which is what this phase asked for. → FINDINGS.md
- A lockfile is never *read back*: a resolve always recomputes from the manifest
  and overwrites. A lockfile that disagreed with the manifest is therefore
  silently corrected rather than refused. → FINDINGS.md