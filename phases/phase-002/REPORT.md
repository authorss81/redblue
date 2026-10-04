# Phase 002 — Fix test discovery: stop feeding Rust source to the Redblue harness

## What changed
| File | Lines | What |
|---|---|---|
| `src/testing/mod.rs` | +10 −9 | `find_test_files` now collects only `.rb`; the `_test.rs` branch in `run_all_tests` is removed so the filter lives in exactly one place. `find_test_files` made `pub` (additive, one keyword) so the discovery contract is assertable from `tests/`. |
| `tests/discovery_test.rs` | +283 (new) | 10 new `#[test]` functions pinning the discovery contract. |

Production change is 4 effective lines. No public type renamed, no language
surface touched.

## Reproduction (before the change)

The finding's premise reproduced exactly; its *consequence* did not.

```
$ cargo run --bin rb -- test
.....................SKIP: // skip "Awaiting full test harness implementation" ...
Tests run: 22
Passed: 21
Failed: 0
```

`run_all_tests()` → `find_test_files("tests/")` collected both extensions, so
`tests/redblue_test.rs`, `tests/expect_test.rs` and `tests/span_test.rs` were
each read with `std::fs::read_to_string` and handed to
`TestHarness::run_source` — a Redblue parser. `Failed: 0` was luck, not
design: those three files happen to contain no `// test ` line, so the scanner
walked past their Rust text without raising. Direct proof that `.rs` reached
the Redblue path:

```
$ cargo run --bin rb -- test tests/redblue_test.rs
Test Results: 0 total, 0 passed, 0 failed, 0 skipped (0.0% success)
```

`edge_discovery_never_returns_rust_files` watched this fail before the fix:

```
discovery handed Rust source to the Redblue harness:
[".../rust-filter/redblue_test.rs", ".../rust-filter/expect_test.rs", ".../rust-filter/red_suite.rb"]
```

3 of 7 tests red for the right reason; green after the change.

## Definition of done
- [x] `run_all_tests()` only collects `.rb` — asserted by
      `edge_discovery_never_returns_rust_files`
- [x] `rb test` reports 0 errors for the Rust suite — asserted by
      `edge_project_tests_directory_has_no_failures`; CLI still prints
      `Tests run: 22 / Passed: 21 / Failed: 0`
- [x] no `.rs` path is ever read by the Redblue harness — the filter is in
      `find_test_files`, the single chokepoint every caller passes through,
      and three separate tests assert it at depth 0, depth 1 and depth 3.

## Tests added
| Test | Edge class covered |
|---|---|
| `edge_discovery_never_returns_rust_files` | core defect: `.rs`, `.txt` and extensionless files all rejected |
| `edge_discovery_collects_a_single_rb_file` | singleton / boundary |
| `find_test_files_walks_nested_directories` | nesting_recursion (3 levels deep, mixed `.rb`/`.rs`) |
| `edge_discovery_of_empty_and_missing_directory_is_empty` | empty + resource_limit (unreadable dir is `Ok([])`, not an error) |
| `edge_non_utf8_rb_file_is_reported_as_an_io_error` | malformed_input: non-UTF-8 `.rb` → clean `Error::Io` naming the file, no panic |
| `edge_malformed_rb_test_is_reported_as_a_failure` | asserts a **failure** is produced; proves the filter does not blanket-suppress real failures |
| `edge_project_tests_directory_has_no_failures` | end-to-end DoD on the real `tests/` tree |
| `edge_crlf_rb_file_is_discovered_and_executed` | malformed_input: CRLF line endings still discovered *and* executed |
| `edge_unicode_rb_test_is_discovered_and_asserted` | unicode: `héllo 🎉 世界` passes; a truncated expectation on the same text still fails |
| `edge_rb_path_with_spaces_is_discovered_and_run` | resource_limit: `.rb` path containing spaces in a spaced directory |

Scratch files are written under `target/tmp/rb-discovery-test/<case>/` inside
the checkout, wiped per test — nothing outside the project is touched, no
wall-clock, no network, no ordering assumptions (all assertions are on
membership and counts, never on `read_dir` order).

### Edge rows deliberately not covered
- **out_of_bounds** — N/A: this change touches file selection, not indexing.
  No index, slice or list element is read by the modified code.
- **type_mismatch** — N/A: discovery yields `Vec<String>` paths; the only
  "type" boundary is file extension, and a non-`.rb` extension (including no
  extension at all) is asserted to be dropped.
- **numeric_boundary** — N/A: no arithmetic in the changed path. Numeric
  boundaries are `expect_test.rs`'s existing subject, untouched here.
- **duplicate_missing_keys** — N/A: records are not involved in discovery.

## Gates
| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | pass — 56 passed, 0 failed, 0 ignored (5 + 10 + 21 + 5 + 15 across 6 binaries) |
| `./rbops/verify.sh phase-002` | **NOT RUN — script absent from this checkout** |

### Gate 4 could not be executed

`./rbops/verify.sh` does not exist here, and neither does `phases/` or
`rbops/phases.json`:

```
$ ls -a
.git  .github  .gitignore  AGENTS.md  ...  src  target  tests  tooling
$ ls rbops
ls: cannot access 'rbops': No such file or directory
```

The invoking pipeline lives outside this checkout, and I was instructed not to
inspect or audit it, so I did not go looking for the script elsewhere. The
first three gates were run and are green. Gate 4 is reported as not-run rather
than claimed as a pass. See `FINDINGS.md`.

## Invariants touched
- None. No change to the `Value` variants, the `Error` enum, `.rb` as the
  source extension, `set x to <expr>`, `say`, or `to … end`.
- Public API: `redblue::testing::find_test_files` becomes `pub`. Additive —
  nothing previously callable was removed or re-signatured.
- `rb test`'s output for the current `tests/` tree is byte-identical before and
  after (`Tests run: 22 / Passed: 21 / Failed: 0`): the `.rs` files were
  contributing zero tests, so no previously-passing test was lost.

## Known gaps / follow-ups
- The `// test ` marker scanner is still line-based and comment-prefix-matched.
  A `.rb` file whose `// test` marker is indented, or written with `# test`,
  works (`run_source` accepts both prefixes), but a test body nested inside
  any other block structure is not supported. Out of scope here → FINDINGS.md.
- `find_test_files` uses `read_dir` order, which is filesystem-dependent.
  Tests therefore assert on membership and counts, never on index order. A
  sorted result would be more deterministic for future reporters → FINDINGS.md.
- `run_all_tests()` hardcodes the relative path `"tests/"`, so it silently
  discovers nothing when the process cwd is not the crate root (e.g. invoking
  the built binary from elsewhere). Pre-existing; not touched here →
  FINDINGS.md.