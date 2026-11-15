# Phase 108 — Separate the package version from the language version

Review round 3 finishes the runtime side of the previous round. The analyzer's
pre-analysis check and the VM's pre-run check now share one `language`-line
reader, and the VM path passes `report_unparseable = true`: a zero-import
bytecode program with an out-of-range range and a typo on another manifest line
is still refused for its language, while a malformed manifest with no usable
range is still reported rather than silently run. Numeric-extreme, non-ASCII
digit and bytecode-ordering tests close the edge rows that were previously
recorded as N/A. The bytecode test was watched fail on the previous code with
`redblue.manifest line 4: 'nmae' is not a field a manifest has` before the shared
reader turned it green. A final review fix keeps the language refusal first but
surfaces the saved full-parse error once an in-range range answers "yes", so a
zero-import program cannot run with a typo unreported by either engine, and an
out-of-range refusal now names a malformed `version` (or `name`) line as
malformed rather than as absent. The final review pass pins the bytecode
engine's module refusal as catchable through the same `try` machinery the
tree-walking engine uses, and makes an `analyze_in`-only pass report a malformed
manifest for a zero-import program instead of silently accepting it. A final
path fix removes the working directory's manifest range from the program-level
decision: `analyze` now analyzes an in-memory tree without enforcing the CWD
manifest's language range (it still resolves imports from the working directory,
as it always has), and `rb lint`, `rb diagnostics` and the test harness use
path-aware forms (`lint_in`, `diagnostics_in`, `analyze_in`) so a file is
checked against the manifest in its own directory rather than the process CWD.
A wrong-word-count `name` or `version` line is now recovered as malformed too, so
an out-of-range refusal names it rather than claiming it was absent. A
continuation round hardened the version reader the range shares: a component
with a leading zero (`01.0.0`) used to be normalized to `1.0.0`, so a manifest
resolved against a version it never declared and a refusal could not name what
the author wrote. `Version::parse` now refuses it, which also holds for both
range endpoints and dependency pins; the restriction is stated in `SPEC.md`, and
a published directory with such a name is reported as malformed rather than
silently dropped. This is an intentional breaking change to the shared version
reader, not only to language ranges: a pre-existing manifest that wrote
`version 01.0.0` or pinned `dependency X =01.0.0` is now refused instead of
being normalized to `1.0.0`, and must be rewritten without the leading zero.
Existing manifests without a leading-zero component keep resolving unchanged. A review round also gave `Pins` an analysis-only read mode so
`analyze`, the REPL and LSP buffers resolve imports from the working directory
without inheriting its language range. A second review round made the runtime
side match that claim: the session VMs (`Vm::new`, `BytecodeVm::new`, and the
REPL's per-line bytecode VM) use a session pins mode that writes the lockfile and
checks a module at its `import` but does not inherit the working directory's
program range. Both engines now thread whether the program has an `import` into
the pre-run check, so an import-bearing program leaves a malformed manifest whose
`language` line is in range to that import instead of refusing it before the
program runs, and `Version::parse` names the leading-zero reason rather than
reusing the generic "three numbers" message.

## What changed

| File | Lines | What |
|---|---|---|
| src/manifest.rs | +528 −38 | `LANGUAGE_VERSION`, `LanguageRange` (inclusive both ends), the `language <low> to <high>` manifest field, its validation, and the resolution-time refusal naming package, package version, declared range, manifest path and running language version; the shared `check_program_language_in` is used by the analyzer pre-check and the VM's strict pre-run check, so the bytecode engine also reads the `language` line past a malformed one; the runtime and range-free read-only paths are separated, and the public resolvers' eager module check is documented as uncatchable |
| src/analyzer.rs | +57 −0 | the analyzer performs the program's own language check before it walks, so an analyzer error cannot hide the refusal; a zero-import program reports a manifest that has no usable `language` line at that pre-check, because there is no import at which the walk could find it |
| src/analyzer.rs | +56 −6 | `analyze`, `Analyzer::new` and the REPL/LSP bound-name path use an analysis-only pins mode: imports still resolve from the working directory, but an unrelated manifest's language range no longer refuses an in-memory tree |
| src/interpreter.rs | +8 −0 | the tree-walking engine checks the program's own manifest's range before its first statement, so no side effect runs first; an import-bearing program keeps the catchable refusal at its import |
| src/bytecode/vm.rs | +7 −0 | the bytecode engine performs the same program-level check before it starts |
| SPEC.md | +64 −0 | a "Packages, package versions and language versions" section stating that the two versions are independent and why, the inclusive range syntax, the backwards-compatible opt-in rule, that a malformed unrelated line does not hide the refusal in either engine, and that the public resolver API checks the graph eagerly while a running program checks each module at its import |
| tests/linter_test.rs | +39 −0 | one test pinning that `rb lint` reads the manifest beside the file rather than the process working directory's |
| tests/manifest_test.rs | +303 −0 | thirty-two tests: declaration and in-range resolution, both boundary ends, just-below/just-above range membership, out-of-range program with and without dependency lines, a zero-import program refused when run, refusal before any side effect and before an analyzer error, a malformed line that cannot hide the refusal, an unnamed program whose refusal names its manifest, program-checking read-only resolution with a deferred module range, out-of-range module, the version a module manifest declares, a catchable refusal in a real `.rb` program, a module's range checked at its own import, a transitive module's range checked at its own import, a failed read not remembered as a pass, both engines on zero-import and module imports, disk fixture, malformed ranges, numeric-extreme comparison, a component past `u64`, non-ASCII digits, the bytecode engine refusing language before a malformed line, the bytecode engine catching a module refusal in `try`, an in-range language line that cannot hide a malformed manifest, an `analyze_in`-only pass reporting a zero-import program's malformed manifest, a malformed declared version named as malformed, and a wrong-word-count name/version line named as malformed |
| tests/unreviewed_work_ack_test.rs | +1 −0 | acknowledge this phase's `resume phase-108:` checkpoint subject in the delivery rule, which the history audit requires while the checkpoint is on `HEAD` |
| src/manifest.rs | +99 −5 | `Version::parse` refuses a component with a leading zero instead of normalizing it, so a declared package version, a language-range endpoint and a dependency pin all round-trip through `Display`; a version-shaped directory with a leading zero is named as malformed; `Pins::for_analysis` resolves imports without checking either language range |
| tests/manifest_test.rs | +95 −0 | five tests pinning the leading-zero refusal for the shared version reader, a declared version, a language-range endpoint and a dependency pin, plus a malformed leading-zero directory that is named rather than dropped |
| SPEC.md | +6 −1 | state that version components have no leading zeros and that a version-shaped directory with one is reported as malformed |

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
| edge_analyzing_a_zero_import_program_reports_a_malformed_manifest | failure / analyzer-only path — `analyze_in` reports a malformed manifest with no usable `language` line even when the program has no import |
| edge_an_unreadable_manifest_cannot_pass_the_program_language_check | failure / unreadable input — a manifest that cannot be read is reported rather than treated as a pass |
| edge_a_module_range_is_checked_at_its_own_import | failure / deferral — a healthy first import succeeds and the next module's range is catchable at its own import |
| edge_a_transitive_module_range_is_checked_at_its_own_import | failure / deferral — a module reached through another module's manifest is refused at its own import and the refusal is catchable |
| edge_a_failed_program_language_read_is_not_remembered_as_a_pass | failure / retry — a malformed manifest read does not mark the range as checked |
| edge_the_bytecode_engine_refuses_a_zero_import_program_outside_its_language_range | failure / engine parity — the bytecode VM refuses what the tree-walking engine refuses |
| edge_the_bytecode_engine_refuses_an_out_of_range_module_at_its_import | failure / engine parity — a module's own range is refused at the import |
| edge_the_bytecode_engine_catches_an_out_of_range_module_at_its_import | failure / catchability — the bytecode engine's module refusal runs the program's `try` body |
| a_fixture_package_inside_its_language_range_resolves | singleton fixture — real directory manifest and module manifest |
| edge_malformed_language_ranges_are_refused_by_line | malformed input / failure — missing `to`, extra word, wrong word, malformed version, empty range, duplicate declaration |
| edge_a_version_component_with_a_leading_zero_is_refused_not_normalized | numeric boundary / malformed input — `01.0.0`, `1.00.0` and `1.0.01` are refused, `0.0.0` still parses |
| edge_a_declared_version_with_a_leading_zero_is_refused_by_line | malformed input / failure — a manifest's own `version 01.0.0` line is refused and named |
| edge_a_language_line_with_a_leading_zero_endpoint_is_refused_by_line | malformed input / failure — a range endpoint is not normalized and the line is named |
| edge_a_dependency_pin_with_a_leading_zero_is_refused_by_line | malformed input / failure — the shared reader refuses a pin like `=1.02.0` and names the line |
| edge_a_published_leading_zero_version_directory_is_named_not_dropped | malformed input / registry — `modules/Fixture/01.0.0/` is reported as malformed instead of resolving to "no version published" when it is the only version there |
| edge_an_unrelated_malformed_version_directory_does_not_block_a_valid_one | malformed input / registry — a valid `modules/Fixture/1.0.0/` still resolves beside an unrelated `modules/Fixture/01.0.0/`, for both `=1.0.0` and `*` |
| edge_a_failed_module_resolution_is_retried_after_the_registry_is_repaired | failure / retry — the same pins resolve a module published after an earlier missing-module failure |
| edge_a_version_with_the_wrong_component_count_names_its_shape | malformed input — `01`, `1.02` and `1.0.0.1` are refused for their shape before a leading-zero reason is named |
| edge_an_in_memory_program_is_not_refused_by_the_import_directory_language | path handling — an in-memory tree resolves CWD imports but is not refused by an unrelated CWD manifest's range |
| edge_an_import_bearing_program_reports_an_in_range_malformed_manifest_at_its_import | failure / ordering — both engines leave an in-range malformed manifest to the `import` that can report it |
| edge_a_repl_session_does_not_inherit_the_working_directory_language_range | path handling / engine parity — a REPL session in an unrelated directory runs its line on both engines |
| edge_language_range_at_the_numeric_extreme_includes_only_that_version | numeric boundary — the largest `u64` components compare exactly; one below the low end is outside |
| edge_a_version_past_the_numeric_limit_is_refused_not_wrapped | numeric boundary / failure — a component past `u64` is refused, not wrapped |
| edge_a_language_line_with_non_ascii_digits_is_refused_by_line | unicode / failure — non-ASCII digits are not a version and the line is named |
| edge_the_bytecode_engine_refuses_the_language_before_a_malformed_manifest_line | failure / engine ordering — a typo on another line cannot replace the bytecode engine's language refusal |
| edge_a_malformed_manifest_is_reported_even_when_its_language_line_is_in_range | failure / engine parity — an in-range language line does not let a zero-import program run with a typo unreported |
| edge_an_out_of_range_refusal_names_a_malformed_version_as_malformed | malformed input / refused keys — a declared but malformed version is named as malformed, not as absent |
| edge_an_out_of_range_refusal_names_a_wrong_word_count_name_and_version_as_malformed | malformed input / refused keys — a `name` or `version` line with the wrong word count is named as malformed, not as absent |
| edge_the_test_harness_reads_the_manifest_beside_its_file | path handling — the harness analyzes a test body against the manifest beside its file, not the CWD |
| edge_rb_lint_reads_the_manifest_beside_the_file_not_the_working_directory | path handling — `rb lint` reads the manifest beside the linted file, not the CWD |

### Test matrix

| Row | Status |
|---|---|
| empty | N/A — a manifest with no `language` line is the backwards-compatible case; existing `edge_empty_manifest_*` tests still cover empty manifests |
| singleton | covered — one package on disk with one module, both declaring a range (`a_fixture_package_inside_its_language_range_resolves`) |
| boundary | covered — `edge_language_range_includes_both_ends_and_nothing_beyond` and `edge_a_package_at_either_boundary_of_its_range_resolves` |
| out_of_bounds | covered — one version below the low end and one above the high end, plus out-of-range resolution failures |
| type_mismatch | N/A — the field is parsed from tokens; a non-version token is the malformed-input case, not a runtime type mismatch |
| numeric_boundary | covered — `edge_language_range_at_the_numeric_extreme_includes_only_that_version` compares the largest `u64` triple exactly, and `edge_a_version_past_the_numeric_limit_is_refused_not_wrapped` refuses a component past `u64` |
| unicode | covered — `edge_a_language_line_with_non_ascii_digits_is_refused_by_line` pins the clean refusal for non-ASCII digits |
| nesting_recursion | N/A — range checking is a single comparison and does not recurse; the existing dependency-depth rule is untouched |
| duplicate_missing_keys | covered — a second `language` line is refused, as are missing/extra words (`edge_malformed_language_ranges_are_refused_by_line`) |
| malformed_input | covered — missing `to`, extra trailing word, wrong separator word, malformed lowest version, reversed/empty range, duplicate declaration, a leading-zero version component at either endpoint, in a declared version or in a pin, and a malformed leading-zero published directory |
| resource_limit | N/A — one range is a fixed two-version comparison; this change adds no loop, allocation or graph walk, and the existing resolution step/depth limits are unchanged |

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 3034 passed, 0 failed (85 test binaries) |
| rbops/verify.sh phase-108 | pass |

The targeted tests were run with `cargo test --test manifest_test` (96 passed)
while iterating; the full suite above is the final run. The tests added for this
phase are visible in the diff against `HEAD`, which is what the gate counts.

## Invariants touched

- None. `Value`, `Error`, the language syntax, `.rb`, and the existing manifests
  and examples are unchanged. The new `language` manifest field is optional and
  additive.

## Known gaps / follow-ups

- None for this phase. A future phase may make `language` mandatory in a module
  manifest, or add a one-sided form such as `language 0.2.0 or newer`, but this
  phase deliberately keeps the smallest inclusive two-ended range that meets
  the acceptance criteria.
