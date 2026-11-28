# Phase 123 — Stop `rb version` and `rb help` naming a version the crate does not have

## Finding re-verification

The finding reproduces on the pre-change tree: `print_help` and `print_version`
in `src/lib.rs` printed the literal `Redblue v0.1.0` while `Cargo.toml` is
`version = "0.2.0"`. The fix routes every surface through one crate-derived
heading, so the current binary answers the crate version:

```bash
$ ./target/debug/rb --version
Redblue v0.2.0
$ ./target/debug/rb version
Redblue v0.2.0
$ ./target/debug/rb help | head -1
Redblue v0.2.0 - A programming language as readable as plain English
```

The REPL greeting says `Welcome to Redblue v0.2.0 - tree-walking interpreter`,
so all four surfaces agree with `env!("CARGO_PKG_VERSION")`.

## What changed
| File | Lines | What |
|---|---|---|
| src/lib.rs | +64 −2 | Added the compile-time `VERSION_HEADING` constant (`concat!("Redblue v", env!("CARGO_PKG_VERSION"))`); `version_heading()` and therefore `print_help`/`print_version` read it instead of the `v0.1.0` literals. Added a `version_tests` module with three unit tests. |
| src/repl/mod.rs | +9 −0 | Kept the existing greeting guard and extended it to assert `version_heading()` equals `Redblue v{CARGO_PKG_VERSION}`. |
| tests/version_surface_test.rs | +217 −0 | Five binary-level tests covering `--version`, `version`, `-v`, the first line of `help`, and the REPL greeting. |
| phases/phase-123/REPORT.md | new | This report. |

## Tests added
| Test | Edge class covered |
|---|---|
| `version_tests::version_heading_names_the_crate_version` | singleton — one heading, compared against `env!("CARGO_PKG_VERSION")`, never a written-out number |
| `version_tests::edge_version_heading_is_non_empty_and_has_no_trailing_whitespace` | empty / boundary — heading non-empty, no trailing whitespace |
| `version_tests::edge_version_heading_rejects_a_stale_literal` | malformed input / failure — proves a stale `v0.1.0` checker returns `Err` (`is_err`) |
| `version_flags_name_the_crate_version` | singleton per surface (`--version`, `version`, `-v`); no empty version, no trailing whitespace |
| `help_first_line_names_the_crate_version` | boundary (first line of `help`); exact prefix `Redblue v<crate> - ` |
| `edge_all_four_version_surfaces_agree` | all four surfaces agree with each other and the crate; none empty or trailing-whitespace |
| `edge_no_surface_prints_a_second_version_spelling` | every `Redblue v...` line in binary output is the crate version (no stale literal) |
| `edge_version_surfaces_reject_extra_arguments` | malformed input: `rb version extra` / `rb help extra` fail instead of printing a banner |

## Edge-case matrix
| Row | Status |
|---|---|
| empty | covered — every surface asserts the version token is non-empty |
| singleton | covered — one version token per surface, checked for `--version`, `version`, `-v`, `help`, greeting |
| boundary | covered — first line of `help` and the greeting line are checked exactly |
| out_of_bounds | N/A — no indexing or collection access is part of version printing |
| type_mismatch | N/A — version surfaces are plain text output, no typed input |
| numeric_boundary | N/A — version is an opaque string; no arithmetic |
| unicode | N/A — the version is ASCII by Cargo's semver rules; no user text involved |
| nesting_recursion | N/A — no recursion or nesting in the code path |
| duplicate_missing_keys | N/A — no maps/records; a missing version is covered as an empty token |
| malformed_input | covered — `rb version extra` and `rb help extra` are rejected (failure test) |
| resource_limit | N/A — output is a fixed handful of lines; no unbounded work |

## Gates
| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 3302 passed, 0 failed |
| ../rbops/verify.sh phase-123 | pass |

## Invariants touched
- None. The language surface, `Value`, error types and `.rb` extension are unchanged; only the printed version string moved to a crate-derived compile-time constant.

## Known gaps / follow-ups
- None for this phase. The `Redblue v[0-9]` literal guard is asserted against the built binary's output, so a future hard-coded version in any of the four surfaces fails `tests/version_surface_test.rs`.
