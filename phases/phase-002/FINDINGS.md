# phase-002 — FINDINGS

## 1. Gate 4 (`./rbops/verify.sh phase-002`) cannot be run from this checkout

**Tried:** `ls -a`, `ls rbops`, `./rbops/verify.sh phase-002`.

**Exact error:**

```
$ ./rbops/verify.sh phase-002
/bin/bash: line 1: ./rbops/verify.sh: No such file or directory

$ ls rbops
ls: cannot access 'rbops': No such file or directory
```

`rbops/`, `rbops/phases.json` and `phases/` are all absent from the working
tree. Only `.github/`, `src/`, `tests/`, `examples/`, `modules/`, `docs/` and
docs files are present. The dispatching pipeline lives outside this checkout
and was explicitly out of scope to inspect, so the script was not located by
other means.

**Consequence:** gates 1–3 were run and are green (`cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets` =
56 passed / 0 failed / 0 ignored). Gate 4 is reported as **not run**, not as
a pass. `REPORT.md` says the same. If gate 4 encodes requirements beyond the
three cargo commands above, this phase is unverified against them.

**What I believe is required:** dispatch must either mount `rbops/` into the
agent's working directory, or drop the `./rbops/verify.sh` step from the
in-phase gate list and run it in the dispatcher after the agent exits. Today
the phase prompt asks for a gate that the phase cannot reach.

## 2. The phase prompt's stated symptom was wrong; the defect was real

The prompt claims `tests/redblue_test.rs` is "parsed as Redblue code and
reported as a failure". On `main` the file *is* read and handed to the Redblue
parser, but it produces **0 tests and 0 failures** — the three `tests/*.rs`
files contain no `// test ` line, so the line scanner walks straight past their
Rust text. `rb test` printed `Failed: 0` before any change.

The underlying defect (Rust source entering the Redblue parser) was real, and
is now proven by a test that watched it fail. Only the *consequence* was
mis-stated. Flagging so the auditor does not record "reported as a failure" as
the invariant to preserve.

## 3. Deferred: `run_all_tests()` hardcodes the relative path `"tests/"`

`src/testing/mod.rs:44` — `find_test_files("tests/")`. `read_dir` on a missing
relative path returns `Err`, which `find_test_files` swallows into `Ok(vec![])`.
So `rb test` reports `Tests run: 0` — a green run that tested nothing — when the
binary is invoked from any directory other than the crate root. Out of scope
for a discovery-filter fix; would need either `CARGO_MANIFEST_DIR`-style
anchoring or an explicit error when a requested directory is absent.

Evidence: `src/testing/mod.rs:60` (`if let Ok(entries) = std::fs::read_dir(dir)`).

## 4. Deferred: `find_test_files` returns unsorted `read_dir` order

`src/testing/mod.rs:60-72` — results are in filesystem enumeration order, not
deterministic across machines. Harmless today because nothing depends on order
(the new tests assert on membership and counts, never index), but it is a trap
for the next thing that reports a test list. One `files.sort()` would close it.

## 5. Deferred: the `// test ` marker scanner is purely line-based

`src/testing/harness.rs:24-56` — a "test" is any trimmed line starting with
`// test ` or `# test `; the body runs until the first line trimming to `// end`
or exactly `end`. Two consequences:

- A `.rb` file that mentions `// test ` inside a longer comment (`// tests are
  fun here`) has that line's remainder used as the test *name*.
- A file with no `// end` terminator silently consumes the rest of the file as
  one test body rather than reporting a malformed test.

Both are real fragility, but both are the marker grammar rather than discovery,
so they belong to their own phase.