# Phase 076 — Scaffold projects: `rb new`, root discovery, and a project-aware `rb test`

## Finding reproduced

```
$ mkdir -p target/tmp/repro && cd target/tmp/repro
$ ../../../target/debug/rb new hello
Redblue v0.1.0 - A programming language as readable as plain English
...
  rb help        Show this help
exit=1
$ ls -A
(nothing)
```

`rb new hello` printed the generic help text and exited 1 without creating
anything. Bare `rb test` had no notion of a project either: `run_test(None)`
called `testing::run_all_tests()`, which always walked the literal `tests/`
directory of the working directory (`src/testing/mod.rs`). Nothing walked up to
a manifest, so a command run from `src/` could not find the project's tests.

## What changed

| File | Lines | What |
|---|---|---|
| `src/project.rs` | +191 (new) | `find_project_root` walks up to the nearest `redblue.manifest`; `create_project` writes `src/`, `tests/`, `src/main.rb`, `tests/main_test.rb` and a manifest in phase-044's schema; `check_manifest` parses it with `Manifest::parse` and refuses a directory with no manifest; `tests_dir` names the project's suite |
| `src/lib.rs` | +51 −5 | `rb new <name>` command (bare `rb new` is a usage error); bare `rb test` discovers the project root, validates its manifest and runs the root's `tests/`; a failing project suite now finishes with the same failure detail (`test name`, `FAILED`, location, message) the explicit-path form prints instead of only a count; `rb test <path>` still runs exactly the named file, and its failure location now names that file instead of `inline`; help text lists `rb new` |
| `src/testing/mod.rs` | +69 −? | `run_tests_in(dir)` is the directory-scoped runner; `run_all_tests()` now calls it with `tests/`, so the existing suite is byte-identical, and an empty directory keeps returning a zero-count result; the command layer owns the "No Redblue test files" rejection; `collect_test_files` propagates a real `read_dir` error instead of silently reporting success, and skips directory symlinks so a link cycle cannot recurse |
| `src/testing/harness.rs` | +13 −4 | `TestHarness::run_file` accepts any path-like value, so a discovered `PathBuf` is run without a lossy string conversion, and records that path as the failure location instead of labelling every file `inline` |
| `src/manifest.rs` | +20 −18 | `is_module_name` is the single name rule shared by the manifest parser and `rb new`, so the two validators cannot drift |
| `tests/discovery_test.rs` | +595 | project tests driving the `rb` binary in `target/tmp/`, beside the existing discovery tests, including the symlink-cycle, empty-library-suite, missing-manifest and explicit-path-failure contracts |
| `tests/unreviewed_work_ack_test.rs` | +1 | `resume phase-076:` joins the in-flight delivery-subject rule, so the pipeline's re-created delivery commit is acknowledged by subject rather than by a SHA that goes stale |
| `redblue.manifest` | +2 (new) | the checkout is now a project, so bare `rb test` from the repository root runs the shipped `tests/` suite instead of failing with "No Redblue project found here" |

The manifest `rb new` writes is exactly phase-044's schema and nothing more:

```text
name hello
version 0.1.0
```

No dependency, lockfile or second schema is invented. `rb new` writes the
manifest last, so a half-written layout is not discoverable as a project.

## Round 2 review fixes

| Finding | Fix |
|---|---|
| bare `rb test` exited 0 after running zero tests | `run_test(None)` now rejects a zero-count result with `No tests were defined in '<dir>'`, after the existing no-`.rb`-files guard |
| whole-file lex/parse failure reported `Location: unknown` | `TestHarness` records the parse error against `source_name` and preserves the error's span line |
| `collect_test_files` recursed without a depth bound | the walk carries a depth counter and refuses a tree deeper than 64 levels instead of overflowing the stack |
| a failed scaffold write left directories that blocked a retry | `create_project` removes the partial layout on error, keeping a pre-existing empty root while removing a root the attempt created |

## Round 3 review fixes

| Finding | Fix |
|---|---|
| `rb test <file>` still exited 0 when the file defined no tests | both forms now call `reject_zero_test_run`, so `rb test empty.rb` fails with `No tests were defined`; `edge_rb_test_with_a_path_that_defines_no_tests_fails` pins it |
| a per-file error in `run_tests_in` was labelled `Location: unknown` | `run_tests_in` calls `TestHarness::add_source_error_at(path, error)`, which records the path and the error's span; `edge_run_tests_in_names_an_unreadable_file` pins it |
| the depth bound allowed one more level than documented | the walk refuses `depth >= MAX_TEST_DIR_DEPTH` before opening that directory, and the doc/message now state the bound exactly |
| the scaffold test existed but never ran the generated entry point | `rb_new_scaffolds_a_project_that_tests_green` now runs `rb run src/main.rb` and asserts it prints `Hello from hello` |

## Tests added

| Test | Edge class covered |
|---|---|
| `rb_new_scaffolds_a_project_that_tests_green` | happy path + end-to-end: layout exists, manifest parses through 044's `parse_manifest`, bare `rb test` reports `Passed: 1` |
| `edge_rb_new_refuses_an_existing_non_empty_directory` | duplicate/occupied ground: refuses, names the directory, keeps the existing file, writes no manifest |
| `edge_rb_new_without_a_name_is_a_usage_error` | empty/missing name: non-zero exit, names the missing project name, creates nothing |
| `edge_rb_new_with_a_path_separator_is_refused_in_place` | boundary/traversal: `nested/hello` is refused, no directory outside the working directory, no leaf in it |
| `edge_rb_test_outside_a_project_names_rb_new` | empty project context: clean non-zero error naming `rb new`, never a silent success |
| `edge_rb_test_walks_up_from_a_nested_directory` | nesting/recursion: from `src/a/b` discovery finds the root and runs its `tests/` |
| `edge_rb_test_with_a_malformed_manifest_names_the_file_and_line` | malformed input: names `redblue.manifest` and `line 2` before running anything |
| `edge_rb_test_with_a_path_runs_only_that_file` | explicit path compatibility: a failing project suite is not run when a passing file is named |
| `edge_rb_test_reports_the_failing_test_name_and_assertion` | failure output: a failing project suite names the test, marks it `FAILED`, and prints a location naming the test file plus the assertion message |
| `edge_rb_test_reports_every_failing_test_not_just_the_first` | boundary/error collection: both failing tests are reported, not only the first |
| `edge_create_project_refuses_an_empty_name` | empty/missing name at the library boundary: `create_project` returns `Err` and writes nothing |
| `edge_rb_new_refuses_a_file_named_after_the_project` | boundary/occupied ground: a file in the project's place is refused as "not a directory" and survives |
| `edge_rb_new_accepts_an_existing_empty_directory` | boundary/empty: an existing empty directory is accepted and tests green |
| `edge_rb_test_in_a_project_with_no_test_files_fails` | empty/missing: both a missing and an empty `tests/` fail with "No Redblue test files" rather than exiting 0 |
| `edge_project_root_canonicalises_parent_components` | boundary/path: a start path containing `..` answers the canonical project root |
| `edge_discovery_does_not_follow_symlinked_directories` | resource/recursion: a `loop -> .` directory symlink is skipped instead of recursing forever, and the real `.rb` file is still found |
| `edge_run_tests_in_empty_directory_is_a_zero_count_result` | empty/library contract: `run_tests_in` keeps returning a zero-count result, so the command layer owns the "no tests" rejection |
| `edge_check_manifest_refuses_a_directory_without_a_manifest` | missing key: a non-project directory is refused rather than reported as a valid project |
| `edge_rb_test_with_a_path_names_the_file_in_failure_output` | explicit path: `rb test <file>` still runs only that file and its failure location names it |
| `edge_rb_test_that_defines_no_tests_fails` | empty/zero-execution: a comment-only and an empty `.rb` file both fail with "No tests were defined" rather than exiting 0 |
| `edge_rb_test_with_a_path_reports_a_parse_failure_file_and_line` | malformed input: a whole-file parse failure names `broken.rb:2` instead of `Location: unknown` |
| `edge_discovery_refuses_a_too_deep_directory_tree` | resource/recursion: a real tree deeper than the bound is refused with a reported error |
| `cleanup_scaffold_removes_a_root_created_by_the_failed_attempt` | failed scaffold: a root created by the attempt is removed entirely |
| `cleanup_scaffold_leaves_a_pre_existing_directory_empty` | failed scaffold: a pre-existing empty directory survives cleanup and is left empty for a retry |
| `edge_rb_test_with_a_path_that_defines_no_tests_fails` | empty/zero-execution: `rb test <file>` rejects a comment-only and an empty file with `No tests were defined` |
| `edge_run_tests_in_names_an_unreadable_file` | malformed input: a non-UTF-8 file in a project suite is a failure that names `bad.rb` instead of `unknown` |

### Mandatory edge-case matrix

| Row | Covered? |
|---|---|
| empty | covered: `edge_rb_new_without_a_name_is_a_usage_error` (no name) and `edge_rb_test_outside_a_project_names_rb_new` (empty directory outside any project) |
| singleton | covered: the scaffold writes exactly one test and bare `rb test` reports `Passed: 1` |
| boundary | covered: `edge_rb_new_with_a_path_separator_is_refused_in_place` (name boundary), `edge_rb_new_refuses_a_file_named_after_the_project` and `edge_rb_new_accepts_an_existing_empty_directory` (path-shape boundary), `edge_rb_test_with_a_path_runs_only_that_file` (explicit path boundary), and `edge_project_root_canonicalises_parent_components` |
| out_of_bounds | N/A: no index or collection is addressed; the closest path concern is traversal, covered by the path-separator refusal |
| type_mismatch | N/A: command arguments are text, and the only typed value is the project name, which the identifier validator refuses (`edge_rb_new_with_a_path_separator_is_refused_in_place`) |
| numeric_boundary | N/A: no arithmetic or numeric values are evaluated; the version is the fixed literal `0.1.0` parsed by phase-044's `Version` |
| unicode | N/A: project names follow phase-044's ASCII identifier schema; a non-ASCII name is refused by the same validator that refuses a path separator, and no path is written |
| nesting_recursion | covered: `edge_rb_test_walks_up_from_a_nested_directory` walks up three directories (`src/a/b`) |
| duplicate_missing_keys | covered: `edge_rb_new_refuses_an_existing_non_empty_directory` (occupied directory), `edge_rb_new_refuses_a_file_named_after_the_project`, `edge_rb_test_in_a_project_with_no_test_files_fails` (missing/empty tests), and `edge_rb_test_with_a_malformed_manifest_names_the_file_and_line` (unknown manifest field) |
| malformed_input | covered: malformed manifest names the file and line; invalid names and occupied directories are refused cleanly |
| resource_limit | covered: `edge_discovery_does_not_follow_symlinked_directories` proves a `loop -> .` link cannot recurse, and `edge_discovery_refuses_a_too_deep_directory_tree` bounds a genuinely deep real tree; the walk descends only into real directories, never a directory symlink, and sort/output stay bounded by the real files |

The suite adds 26 `#[test]` functions, 23 named `edge_*`. Zero new `#[ignore]`,
`// skip` or `allow(clippy::` suppressions.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | 2945 passed, 0 failed, 0 ignored |
| ../rbops/verify.sh "phase-076" | pass |

## Invariants touched

- None to the language surface: no lexer, parser, grammar, `Value` or `Error`
  variant changed. `.rb` is still the extension.
- Behaviour change, deliberate: bare `rb test` now means "the current
  project's tests", not "the working directory's `tests/`". Outside any project
  it is a non-zero error naming `rb new`.
- Behaviour change, deliberate: `rb new <name>` exists; bare `rb new` is a usage
  error.
- `rb test <path>` still runs exactly the named file through the same
  `TestHarness`/`PrettyReporter` path, so the existing test-runner tests pass
  unmodified. The one observable change is the failure `Location:` label: it
  now names the file that ran (`failing.rb:2`) instead of the generic
  `inline:2`, and `edge_rb_test_with_a_path_names_the_file_in_failure_output`
  pins it.
- Additive public API: `redblue::project::{create_project, find_project_root,
  check_manifest, tests_dir}` and `redblue::testing::run_tests_in`.

## Known gaps / follow-ups

- Root discovery is exposed as `project::find_project_root` and used by bare
  `rb test`. The explicit-path commands (`rb run`, `rb lint`, `rb format`,
  `rb compile`) keep their phase-044 semantics: a program resolves imports
  against its own directory; `rb test <path>` keeps its file-scoped semantics
  (only the failure location label changed, as above).
  Teaching those commands to fall back to a discovered root is a separate
  behaviour change, not smuggled into this phase.
- An existing *empty* directory is accepted by `rb new`; only occupied ground
  is refused. This is intentional: an empty directory is not a merge and has
  nothing to overwrite. Both branches are pinned by the tests.
- The checkout now ships `redblue.manifest` so bare `rb test` at the repository
  root runs `tests/`; a project with a discovered manifest but no test files is
  a non-zero error, not a zero-test success.
