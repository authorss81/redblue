# Phase 108 — Separate the package version from the language version

Review round 2 closes the three findings. The pre-analysis check now reads the
`language` line even when another line makes the full manifest unparseable, so
a valid out-of-range range still refuses the program before the analyzer reports
the malformed line; an unreadable manifest is passed on as a read error rather
than treated as a pass. `resolve_for_reading` now enforces the *program's* range
(only a module's range is deferred), so the public helper cannot hand a linter or
language server a silent pass. The transitive case — the program pins `Outer`,
`Outer` pins `Inner`, and `Inner` is out of range — is pinned by a real `.rb`
program that catches `Inner`'s refusal at the import inside `Outer`'s function.

## What changed

| File | Lines | What |
|---|---|---|
| src/manifest.rs | +463 −38 | `LANGUAGE_VERSION`, `LanguageRange` (inclusive both ends), the `language <low> to <high>` manifest field, its validation, and the resolution-time refusal naming package, package version, declared range, manifest path and running language version; the runtime and range-free read-only paths are separated |
| src/analyzer.rs | +5 −0 | the analyzer performs the program's own language check before it walks, so an analyzer error cannot hide the refusal |
| src/interpreter.rs | +8 −0 | the tree-walking engine checks the program's own manifest's range before its first statement, so no side effect runs first; an import-bearing program keeps the catchable refusal at its import |
| src/bytecode/vm.rs | +7 −0 | the bytecode engine performs the same program-level check before it starts |
| SPEC.md | +49 −0 | a "Packages, package versions and language versions" section stating that the two versions are independent and why, the inclusive range syntax, and the backwards-compatible opt-in rule |
| tests/manifest_test.rs | +787 −0 | twenty-two tests: declaration and in-range resolution, both boundary ends, just-below/just-above range membership, out-of-range program with and without dependency lines, a zero-import program refused when run, refusal before any side effect and before an analyzer error, a malformed line that cannot hide the refusal, an unnamed program whose refusal names its manifest, program-checking read-only resolution with a deferred module range, out-of-range module, the version a module manifest declares, a catchable refusal in a real `.rb` program, a module's range checked at its own import, a transitive module's range checked at its own import, a failed read not remembered as a pass, both engines on zero-import and module imports, disk fixture, malformed ranges |
| tests/unreviewed_work_ack_test.rs | +1 −0 | acknowledge this phase's `resume phase-108:` checkpoint subject in the delivery rule, which the history audit requires while the checkpoint is on `HEAD` |

The `language` field is optional, so every manifest written before it keeps
resolving. The program's own manifest is checked before the analyzer walks, so
an analyzer error cannot hide the refusal — including when another manifest line
is malformed, because the `language` line is read on its own before the full
parse failure is deferred to the analyzer. The program is checked again before
the first statement runs, so no side effect happens first. Each module manifest's
range is checked at the `import` that names it, not while the whole graph is
resolved: a healthy first import succeeds even when a later module is out of
range, and a `try` around the out-of-range import catches its `Runtime` failure.
A read-only pass that only asks which names an import binds (the analyzer) uses
`resolve_for_reading`, which still enforces the program's own range but not a
module's, so the module failure stays at the import the program can guard instead
of becoming an uncatchable analyzer error. `resolve_without_writing` is the
runtime read-only path and still enforces both ranges.
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
none needed changing. The dependency-depth rule (`MAX_RESOLUTION_DEPTH`,
`src/manifest.rs:674`) and its tests
(`edge_a_chain_deeper_than_the_limit_is_refused`,
`edge_a_chain_exactly_at_the_depth_limit_resolves`) were not modified and still
pass. The only existing test file this phase changes is the history audit,
which needs the phase-108 checkpoint subject added to its delivery rule; no
manifest test was edited or removed.

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
| edge_reading_a_manifest_checks_the_program_but_leaves_a_module_to_the_runtime | boundary — the static read-only path refuses an out-of-range program but resolves an out-of-range module, while the runtime read-only path refuses both |
| edge_a_module_outside_its_language_range_is_refused | failure — a module's own range is checked |
| edge_a_module_refusal_names_the_version_its_manifest_declares | failure — the refusal names the declared version, not the directory version |
| edge_an_out_of_range_package_is_a_catchable_runtime_error_at_its_import | failure / catchability — a real `.rb` program catches the refusal with `try` |
| edge_a_program_outside_its_language_range_is_refused_before_it_runs | failure / ordering — no statement runs and the program's own `try` cannot swallow the refusal |
| edge_a_program_language_refusal_precedes_an_analyzer_error | failure / ordering — an unknown variable cannot hide the program's language refusal |
| edge_a_malformed_manifest_does_not_hide_the_program_language_refusal | failure / ordering — a malformed non-`language` line cannot hide the program's language refusal |
| edge_an_unreadable_manifest_cannot_pass_the_program_language_check | failure / unreadable input — a manifest that cannot be read is reported rather than treated as a pass |
| edge_a_module_range_is_checked_at_its_own_import | failure / deferral — a healthy first import succeeds and the next module's range is catchable at its own import |
| edge_a_transitive_module_range_is_checked_at_its_own_import | failure / deferral — a module reached through another module's manifest is refused at its own import and the refusal is catchable |
| edge_a_failed_program_language_read_is_not_remembered_as_a_pass | failure / retry — a malformed manifest read does not mark the range as checked |
| edge_the_bytecode_engine_refuses_a_zero_import_program_outside_its_language_range | failure / engine parity — the bytecode VM refuses what the tree-walking engine refuses |
| edge_the_bytecode_engine_refuses_an_out_of_range_module_at_its_import | failure / engine parity — a module's own range is refused at the import |
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
| cargo test --all-targets | 3012 passed, 0 failed (85 test binaries) |
| rbops/verify.sh phase-108 | pass |

The targeted tests were run with `cargo test --test manifest_test` (79 passed)
while iterating; the full suite above is the final run. `verify.sh` reported
`test quota met: 22 rust / 0 redblue`, `edge case test present: 21`,
`failure-asserting test present: 18`, and `no newly skipped tests`.

## Invariants touched

- None. `Value`, `Error`, the language syntax, `.rb`, and the existing manifests
  and examples are unchanged. The new `language` manifest field is optional and
  additive.

## Known gaps / follow-ups

- None for this phase. A future phase may make `language` mandatory in a module
  manifest, or add a one-sided form such as `language 0.2.0 or newer`, but this
  phase deliberately keeps the smallest inclusive two-ended range that meets
  the acceptance criteria.
