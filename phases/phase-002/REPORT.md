# Phase 002 — Fix test discovery: stop feeding Rust source to the Redblue harness

> **Accuracy note (carried forward).** The `REPORT.md` first committed at this
> phase (`fa8b1d0`) described a file `tests/harness_discovery_test.rs` containing
> 15 named tests. No such file exists in the tree, and none of those test names
> exist anywhere. The committed work is `tests/discovery_test.rs` with 13 tests.
> That report also claimed "discovery results sorted for a stable report" — the
> sorting was **not** in the code. See FINDINGS.md F6. This report describes only
> what is in the tree, re-verified gate by gate on the resumed run.

## What changed

Production code is **unchanged by the resumed run**: the fix landed in
`fa8b1d0`/`5ca4867` and was re-verified here rather than rewritten (details
below). The resumed run changed documentation only.

| File | Lines | What |
|---|---|---|
| `src/testing/mod.rs` | +12 −2 | `5ca4867` `find_test_files` sorts its result; the recursive walk moved into a private `collect_test_files` so the sort applies once, globally, to the flattened list rather than per-directory |
| `tests/discovery_test.rs` | +108 | `5ca4867` 3 new tests: sorted/stable discovery order, global sort across nesting, and the real-tree assertion that no `tests/*_test.rs` is ever collected |
| `src/testing/mod.rs` | +10 −9 | `fa8b1d0` discovery collects `.rb` only; `run_all_tests` dropped its `_test.rs` re-filter; `find_test_files` made `pub` |
| `tests/discovery_test.rs` | +289 | `fa8b1d0` 10 tests pinning the discovery contract |
| `phases/phase-002/FINDINGS.md` | +88 −7 | `5ca4867` out-of-scope defects recorded; resumed run corrected F1, which wrongly said `phases/` was absent from the tree |
| `phases/phase-002/REPORT.md` | +228 −94 | `5ca4867` + resumed run: this file — gate table refreshed against output re-run on the resumed run, duplicate "Known gaps" bullet removed, mutation evidence added |

No language surface, no `tests/*.rb` body, no existing test, no assertion
loosened. Nothing outside those files was touched.

### The production change, in full

`src/testing/mod.rs:55-85`. Before, `find_test_files` was itself recursive and
returned `read_dir` order. Now:

```rust
pub fn find_test_files(dir: &str) -> Result<Vec<String>> {
    let mut files = collect_test_files(dir);
    files.sort();
    Ok(files)
}

fn collect_test_files(dir: &str) -> Vec<String> { /* the old body, `?` removed */ }
```

The `?` is removed because the inner function no longer returns `Result`; it
never propagated an error (`read_dir`'s `Err` is swallowed by `if let Ok(..)`,
unchanged — that behaviour is FINDINGS.md F3). Public signature unchanged, so
no caller is affected: `find_test_files` has exactly one caller,
`run_all_tests` at `src/testing/mod.rs:44` — re-confirmed on the resumed run
(`grep -rn 'find_test_files' src/` → one hit).

## Definition of done — verified, not asserted

| Item | Evidence |
|---|---|
| `run_all_tests()` only collects `.rb` | `edge_discovery_never_returns_rust_files`, `edge_repository_rust_suite_is_never_handed_to_the_harness`; no `.rs` extension literal remains in `src/` (`grep -rn '\.rs\b' src/` → one hit, a doc comment at `src/testing/mod.rs:58`) |
| `rb test` reports 0 errors for the Rust suite | `cargo run --bin rb -- test` → `Tests run: 22 / Passed: 21 / Failed: 0`; `edge_project_tests_directory_has_no_failures` |
| no `.rs` path is ever read by the Redblue harness | mutation check below |

## Tests added

`tests/discovery_test.rs`, 13 `#[test]` functions, 12 named `edge_*`.

| Test | Edge class covered |
|---|---|
| `edge_discovery_never_returns_rust_files` | the defect itself — `.rs`/`.txt`/`Makefile` decoys beside one `.rb` |
| `edge_repository_rust_suite_is_never_handed_to_the_harness` | **the defect asserted against the real `tests/` tree**; names all four `*_test.rs` and fails if `tests/` changes shape |
| `edge_project_tests_directory_has_no_failures` | end-to-end: `run_all_tests()` on the real tree, 0 failures |
| `edge_discovery_order_is_sorted_and_stable_across_calls` | determinism — files written `h..a`; asserts sorted **and** two calls identical |
| `edge_nested_discovery_is_globally_sorted` | determinism + nesting — 2 levels, asserts *global* sorted order, not per-level |
| `edge_discovery_collects_a_single_rb_file` | singleton |
| `edge_discovery_of_empty_and_missing_directory_is_empty` | empty + absent path, no panic |
| `find_test_files_walks_nested_directories` | nesting — `.rb` at depth 0 and depth 3, `.rs` decoys at both |
| `edge_non_utf8_rb_file_is_reported_as_an_io_error` | **asserts a failure of a named kind** — invalid UTF-8 → `Error::Io` naming the file, no panic, no silent pass |
| `edge_malformed_rb_test_is_reported_as_a_failure` | **asserts a failure** — unparseable `// test` body → 1 failed / 0 passed |
| `edge_crlf_rb_file_is_discovered_and_executed` | malformed input (CRLF) — discovered *and* its `expect` actually evaluated |
| `edge_unicode_rb_test_is_discovered_and_asserted` | unicode — emoji + CJK + combining acute; the same file is then mutated so the expectation is wrong and must fail |
| `edge_rb_path_with_spaces_is_discovered_and_run` | resource/state — directory and file names both containing spaces |

Quota: 13 ≥ 6 new `#[test]`s · 12 `edge_*` · 3 assert a *failure* is produced
(`..._io_error`, `..._as_a_failure`, and the negative half of the unicode test)
· 0 new `#[ignore]` · 0 `.skip` · 0 `allow(clippy::`.

## Red before green

The two ordering tests failed first, against the unchanged production code
(commit `fa8b1d0`, before the sort):

```
$ cargo test --test discovery_test
---- edge_discovery_order_is_sorted_and_stable_across_calls stdout ----
assertion `left == right` failed: discovery must return paths in sorted order, got
  [".../order/c.rb", ".../order/g.rb", ".../order/d.rb", ".../order/a.rb",
   ".../order/b.rb", ".../order/h.rb", ".../order/f.rb", ".../order/e.rb"]
 right: [".../order/a.rb", ".../order/b.rb", ".../order/c.rb", ".../order/d.rb",
   ".../order/e.rb", ".../order/f.rb", ".../order/g.rb", ".../order/h.rb"]

---- edge_nested_discovery_is_globally_sorted stdout ----
assertion `left == right` failed: nested discovery must return paths in global sorted order, got
  [".../order-nested/inner/z.rb", ".../order-nested/inner/y.rb",
   ".../order-nested/a.rb", ".../order-nested/b.rb"]

failures:
    edge_discovery_order_is_sorted_and_stable_across_calls
    edge_nested_discovery_is_globally_sorted

test result: FAILED. 11 passed; 2 failed
```

### Mutation check on the resumed run — the `.rs` filter itself

The previous report's `.rb`-only filter was inherited already-committed, so no
red-then-green record for it existed. The resumed run re-established one by
reintroducing the pre-fix condition at `src/testing/mod.rs:78` —
`|| path.extension() == Some("rs".as_ref())`, which is exactly the
`ext == "rs" || ext == "rb"` predicate from `git show 6cc1281:src/testing/mod.rs`:

```
$ cargo test --test discovery_test
---- find_test_files_walks_nested_directories stdout ----
assertion `left == right` failed: expected two .rb files, got
  [".../nested/a/b/c/deep.rb", ".../nested/a/b/c/deep_test.rs",
   ".../nested/top.rb", ".../nested/top_test.rs"]
  left: 4
 right: 2

failures:
    edge_discovery_collects_a_single_rb_file
    edge_discovery_never_returns_rust_files
    edge_repository_rust_suite_is_never_handed_to_the_harness
    find_test_files_walks_nested_directories

test result: FAILED. 9 passed; 4 failed
```

4 tests go red for the right reason, including the definition-of-done test
against the real `tests/` tree. The mutation was reverted immediately
(`git diff` empty, 13 passed) — it is **not** part of the diff.

Filesystem evidence for why sorting was needed (`read_dir` returns hash order,
not creation order):

```
$ for n in h g f e d c b a; do ... > orderprobe/$n.rb; done; ls -U orderprobe
c.rb g.rb d.rb a.rb b.rb h.rb f.rb e.rb
```

## Gates

Re-run on the resumed run, this tree, output verbatim:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** — no diff, exit 0 |
| `cargo clippy --all-targets -- -D warnings` | **pass** — `Finished dev profile ... in 7.51s`, zero warnings |
| `cargo test` | **pass** — 59 passed, 0 failed, 0 ignored (lib 5, bin 0, discovery_test 13, expect_test 21, redblue_test 5, span_test 15; doctests 0) |
| `./rbops/verify.sh phase-002` | **NOT RUN — the script does not exist in this checkout** |

Gate 4 verbatim, not paraphrased:

```
$ ./rbops/verify.sh phase-002
/bin/bash: line 1: ./rbops/verify.sh: No such file or directory
EXIT=127

$ ls -d rbops
ls: cannot access 'rbops': No such file or directory
```

`rbops/verify.sh` is **not claimed as passing.** It was not run, so this phase is
unverified against whatever it checks beyond the three cargo commands. The
pipeline lives outside the working directory and was out of scope to inspect, so
I did not go looking for it elsewhere. Full detail and the dispatch-side fix I
believe is needed: FINDINGS.md F1.

Additional check, not a gate — `rb run` over the backwards-compatibility corpus
required by AGENTS.md §2 (re-run on the resumed run):

```
PASS  examples/files.rb          PASS  examples/hello.rb
PASS  examples/fizzbuzz.rb       PASS  examples/test_arithmetic.rb
PASS  examples/formats.rb        PASS  examples/time.rb
FAIL  modules/MathUtils.rb :: Error: ParserError: Expected function name
  --> modules/MathUtils.rb:4:16
4 | constant PI to 3.14159
  |                ^
```

`modules/MathUtils.rb` is a **grammar** defect (`constant … to …` unimplemented),
pre-existing and unrelated to this phase. The previous attempt verified it with
`git stash -u` + rebuild; the resumed run re-confirms it by code path instead:
`src/testing/mod.rs` is reachable only from the `rb test` command
(`src/lib.rs:63-73` → `run_all_tests`) and from `testing::assertions`, which
`vm.rs` uses for `expect` equality. `rb run` of a module touches neither, so this
diff cannot affect it. FINDINGS.md F9.

## Invariants touched

- **None.** No change to `Value`, to `Error`, to the grammar, to `.rb` as the
  source extension, to `say`, to `set … to`, or to `… end` blocks.
- Behavioural change outside the language: the Redblue test harness no longer
  reads `.rs` files (was: `rb test` lexed and executed Rust sources as
  Redblue), and discovery output is now sorted (was: filesystem hash order).
- Public API: `find_test_files` was made `pub` in `fa8b1d0`.

## Test-requirement matrix (phase prompt §"Test requirements")

- **empty** — covered: `edge_discovery_of_empty_and_missing_directory_is_empty`
  (a directory with nothing collectable yields an empty list, not an error).
- **singleton** — covered: `edge_discovery_collects_a_single_rb_file`.
- **boundary** — covered: `edge_discovery_order_is_sorted_and_stable_across_calls`
  covers the index-0 and index-7 ends of the returned list; a `.rb` sharing a
  basename with a `.rs` is collected (index-0 assertion in
  `edge_discovery_never_returns_rust_files`). The `.rb`/`.RB` extension-case
  boundary is **not** covered — I am not adding it here because
  `path.extension() == "rb"` is byte-exact and case-sensitivity is a
  platform-behaviour question that belongs with the marker-grammar phase
  (FINDINGS.md F5).
- **out_of_bounds** — covered: `edge_discovery_of_empty_and_missing_directory_is_empty`
  drives a path three levels deep that was never created; it returns empty
  rather than panicking. No index-based access exists in the changed code.
- **type_mismatch** — covered: `.rs`, `.txt` and an extensionless `Makefile` are
  all rejected while the `.rb` is kept
  (`edge_discovery_never_returns_rust_files`). This is the file-analogue row;
  there are no runtime types in the discovery path.
- **numeric_boundary** — N/A + why: this change adds no arithmetic and reads no
  numbers. The only integers are `Vec::len` in assertions, whose boundary is the
  index-0/index-last case above.
- **unicode** — covered: `edge_unicode_rb_test_is_discovered_and_asserted`
  uses a combining acute, an emoji (U+1F389) and CJK (U+4E16/U+754C), asserts
  the round trip passes, then rewrites the file so the expectation is wrong and
  asserts that it fails — so the text is not passing by accident.
- **nesting_recursion** — covered: `find_test_files_walks_nested_directories`
  descends three levels; `edge_nested_discovery_is_globally_sorted` covers the
  sort across two levels. Recursion depth is one frame per directory level and
  is bounded by the filesystem, not by user input — `rb test` hardcodes
  `tests/`.
- **duplicate_missing_keys** — covered (file analogue): duplicate basenames in
  different directories are both collected
  (`find_test_files_walks_nested_directories` collects `top.rb` and `deep.rb`),
  and a `.rb` that shares its basename with a `.rs` is still collected
  (`edge_discovery_never_returns_rust_files`). There are no records or keys in
  this code path.
- **malformed_input** — covered: `edge_malformed_rb_test_is_reported_as_a_failure`
  (unparseable body), `edge_non_utf8_rb_file_is_reported_as_an_io_error`
  (invalid UTF-8 bytes), `edge_crlf_rb_file_is_discovered_and_executed`
  (CRLF line endings), `edge_discovery_of_empty_and_missing_directory_is_empty`
  (a missing directory). **Not covered:** a `.rb` file that is empty (zero
  bytes). It is collected and yields zero tests, which is indistinguishable from
  a valid file with no tests, and pinning that would assert current behaviour
  rather than desired behaviour → FINDINGS.md F8.
- **resource_limit** — covered: absent directory, three-level recursion, a
  non-decodable file — all terminate cleanly with a recorded outcome and no
  panic. No unbounded loop is introduced.

## Known gaps / follow-ups

All recorded in `phases/phase-002/FINDINGS.md`, none fixed here (out of scope):

- **F1** — `./rbops/verify.sh` cannot be run from the agent's checkout; the
  phase prompt asks for a gate the phase cannot reach. Gate 4 unreported.
- **F2** — the prompt's stated symptom ("reported as a failure") was wrong; the
  file was read and silently produced zero tests.
- **F3** — `run_all_tests()` hardcodes the relative path `"tests/"`, and
  `find_test_files` swallows `read_dir`'s `Err`, so `rb test` from any other
  directory reports `Tests run: 0` — green, having tested nothing.
- **F4** — fixed in `5ca4867`: discovery order was filesystem hash order; it is
  now sorted globally.
- **F5** — the `// test` marker grammar is line-based; an unterminated block
  silently swallows the rest of the file, and `// tests are fun` reads as a test
  marker.
- **F6** — the previous attempt's REPORT.md described a nonexistent test file and
  claimed a sort that was not in the code.
- **F7** — all 21 tests `rb test` discovers are **vacuous**: every `// test`
  block in `tests/suite.rb`, `tests/integration_test.rb` and
  `tests/test_arithmetic.rb` is commented out line by line, so each body is
  empty and asserts nothing. Discovery is now correct; the suite's *contents*
  are not. This is why `rb test` reporting `Failed: 0` is not yet evidence that
  Redblue works. Suite-authoring work, not a discovery-filter phase.
- **F8** — a zero-byte `.rb` file is collected and yields zero tests, so it is
  indistinguishable from a valid empty suite.
- **F9** — `modules/MathUtils.rb` does not parse (`constant … to …` unimplemented;
  pre-existing, confirmed by code path).