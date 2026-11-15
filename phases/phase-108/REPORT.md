# Phase 108 — Separate the package version from the language version

## What changed

| File | Lines | What |
|---|---|---|
| src/manifest.rs | +212 −9 | `LANGUAGE_VERSION`, `LanguageRange` (inclusive both ends), the `language <low> to <high>` manifest field, its validation, and the resolution-time refusal naming package, package version, declared range, manifest path and running language version; the runtime and range-free read-only paths are separated |
| src/interpreter.rs | +116 −0 | a zero-import program checks its own manifest's range before running, and a run whose imports never executed checks at its end; an import-bearing program keeps the catchable refusal at its import |
| SPEC.md | +45 | a "Packages, package versions and language versions" section stating that the two versions are independent and why, the inclusive range syntax, and the backwards-compatible opt-in rule |
| tests/manifest_test.rs | +364 | thirteen tests: declaration and in-range resolution, both boundary ends, just-below/just-above range membership, out-of-range program with and without dependency lines, a zero-import program refused when run, an unnamed program whose refusal names its manifest, runtime vs range-free read-only resolution, out-of-range module, the version a module manifest declares, a catchable refusal in a real `.rb` program, disk fixture, malformed ranges |

The `language` field is optional, so every manifest written before it keeps
resolving. A package that declares a range is checked when `resolve` settles it:
the program's own manifest once at the start, and every module manifest as it is
read. The runtime pins are settled at the `import` that needs them, so the
refusal is a catchable `Runtime` failure there — a `try` around the import
catches it. A program with no `import` has no loader to ask, so the VM checks
the program's own manifest before it runs; a run whose imports were written but
never executed checks at its end. A read-only pass that only asks which names an
import binds (the analyzer) uses `resolve_for_reading`, which resolves without
enforcing the range, so the failure stays at the import the program can guard
instead of becoming an uncatchable analyzer error. `resolve_without_writing` is
the runtime read-only path and still enforces the range.
The error is `Error::Runtime` (never a panic) and reads, for example:

```
Package 'report' 1.0.0 supports Redblue 9.0.0 to 9.9.9, but this is Redblue 0.2.0:
the package cannot run on this language version
```

`LANGUAGE_VERSION` is `0.2.0`, the language this compiler implements. It is a
separate constant from a manifest's own `version` and from the crate release:
package bug fixes change package versions, language semantics change the
language version.

### What previously conflated the two versions

Nothing in tracked production code encoded the language version in a package
version. The gap was the absence of any field or statement at all: a package had
only its own `version`, so the obvious place to record a required language was
that number or a comment. The only manifest-shaped fixtures are the repository
root `redblue.manifest` (`name redblue`, `version 0.2.0`, no language line) and
the module fixtures written by tests; none encoded the opposite assumption, so
none needed changing. `src/manifest.rs:667`'s dependency-depth rule and its
tests (`edge_a_chain_deeper_than_the_limit_is_refused`,
`edge_a_chain_exactly_at_the_depth_limit_resolves`) were not modified and still
pass.

## Tests added

| Test | Edge class covered |
|---|---|
| edge_a_package_declares_the_language_range_it_supports | declaration / in-range resolution |
| edge_language_range_includes_both_ends_and_nothing_beyond | boundary — low end, high end, one below, one above |
| edge_a_package_at_either_boundary_of_its_range_resolves | boundary — running version exactly at low and at high |
| edge_a_package_outside_its_language_range_is_refused_naming_both_versions | failure / out of bounds — names package, package version, range, running version |
| edge_a_program_with_no_dependencies_outside_its_language_range_is_refused | failure / empty — the program-level check fires with no dependency lines |
| edge_a_program_with_no_imports_outside_its_language_range_is_refused_when_run | failure / runtime — a real zero-import `.rb` program is refused by `run_file` |
| edge_an_unnamed_program_refusal_names_its_manifest_and_missing_version | failure / missing keys — the refusal names the manifest path and says no name or version was declared |
| edge_reading_a_manifest_leaves_the_language_range_to_the_runtime | boundary — the static read-only path resolves while the runtime read-only path refuses |
| edge_a_module_outside_its_language_range_is_refused | failure — a module's own range is checked |
| edge_a_module_refusal_names_the_version_its_manifest_declares | failure — the refusal names the declared version, not the directory version |
| edge_an_out_of_range_package_is_a_catchable_runtime_error_at_its_import | failure / catchability — a real `.rb` program catches the refusal with `try` |
| a_fixture_package_inside_its_language_range_resolves | singleton fixture — real directory manifest and module manifest |
| edge_malformed_language_ranges_are_refused_by_line | malformed input / failure — missing `to`, extra word, wrong word, malformed version, empty range, duplicate declaration |

### Test matrix

| Row | Status |
|---|---|
| empty | N/A — a manifest with no `language` line is the backwards-compatible case; existing `edge_empty_manifest_*` tests still cover empty manifests |
| singleton | covered — one package on disk with one module, both declaring a range (`a_fixture_package_inside_its_language_range_resolves`) |
| boundary | covered — `edge_language_range_includes_both_ends_and_nothing_beyond` and `edge_a_package_at_either_boundary_of_its_range_resolves` |
| out_of_bounds | covered — one version below the low end and one above the high end, plus out-of-range resolution failures |
| type_mismatch | N/A — the field is parsed from tokens; a non-version token is the malformed-input case, not a runtime type mismatch |
| numeric_boundary | N/A — versions are parsed as `u64` triples by `Version::parse`; the change adds no arithmetic to that parser and no new numeric operation |
| unicode | N/A — the field accepts only ASCII version digits and the word `to`; non-ASCII input falls under the malformed-version refusal |
| nesting_recursion | N/A — range checking is a single comparison and does not recurse; the existing dependency-depth rule is untouched |
| duplicate_missing_keys | covered — a second `language` line is refused, as are missing/extra words (`edge_malformed_language_ranges_are_refused_by_line`) |
| malformed_input | covered — missing `to`, extra trailing word, wrong separator word, malformed lowest version, reversed/empty range, duplicate declaration |
| resource_limit | N/A — one range is a fixed two-version comparison; this change adds no loop, allocation or graph walk, and the existing resolution step/depth limits are unchanged |

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 3003 passed, 0 failed (85 test binaries) |
| rbops/verify.sh phase-108 | pass |

The targeted tests were run with `cargo test --test manifest_test` (70 passed)
while iterating; the full suite above is the final run. `verify.sh` reported
`test quota met: 13 rust / 0 redblue`, `edge case test present: 12`,
`failure-asserting test present: 9`, and `no newly skipped tests`.

## Invariants touched

- None. `Value`, `Error`, the language syntax, `.rb`, and the existing manifests
  and examples are unchanged. The new `language` manifest field is optional and
  additive.

## Known gaps / follow-ups

- None for this phase. A future phase may make `language` mandatory in a module
  manifest, or add a one-sided form such as `language 0.2.0 or newer`, but this
  phase deliberately keeps the smallest inclusive two-ended range that meets
  the acceptance criteria.
