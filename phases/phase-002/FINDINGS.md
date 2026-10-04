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
59 passed / 0 failed / 0 ignored). Gate 4 is reported as **not run**, not as
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

## 4. FIXED in this run: `find_test_files` returned unsorted `read_dir` order

`src/testing/mod.rs:60-72` — results were in filesystem enumeration order. Fixed:
`find_test_files` now delegates the recursive walk to a private
`collect_test_files` and sorts the flattened list, so ordering is global rather
than per-directory. Two tests pin it, and both failed first against the
unsorted code (`edge_discovery_order_is_sorted_and_stable_across_calls`,
`edge_nested_discovery_is_globally_sorted`).

Evidence that this was a real defect, not a hypothetical:

```
$ for n in h g f e d c b a; do ... > orderprobe/$n.rb; done; ls -U orderprobe
c.rb g.rb d.rb a.rb b.rb h.rb f.rb e.rb
```

`read_dir` returns hash order, which is stable on one filesystem and different
on another.

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

## 6. BLOCKER for the auditor: the previous REPORT.md did not describe the tree

The REPORT.md committed in `fa8b1d0` claimed:

- a file `tests/harness_discovery_test.rs` with 15 named tests
  (`discovers_only_rb_files_and_never_rust_sources`,
  `edge_uppercase_extension_is_not_collected`, …). No such file exists; the
  committed file is `tests/discovery_test.rs` with 10 tests, and **none** of
  those 15 names exist anywhere in the repository.
- "discovery results sorted for a stable report". The sort was not in the code.
  It is now (F4 above).

Either the report was written against a tree state that was never committed, or
it was written without reading it. `REPORT.md` has been rewritten to describe
only what is in the tree and to quote gate output verbatim. Per AGENTS.md §8,
this is logged rather than argued about: the audit trail should show that a
report can diverge from the diff, and that the four gates cannot detect it —
three `cargo` commands do not check whether `REPORT.md` is true.

## 7. Deferred: every test `rb test` discovers is vacuous

`rb test` reports `Tests run: 22 / Passed: 21 / Failed: 0`. All 21 non-skipped
tests assert nothing. Every `// test` block in `tests/suite.rb` (12),
`tests/integration_test.rb` (5) and `tests/test_arithmetic.rb` (4) is commented
out line by line, so the scanner finds a `// test "..."` marker, and the body up
to the next `// end` consists entirely of comments:

```
// test "Basic addition"
//     set a to 10
//     set b to 20
//     set sum to a + b
// end
```

This is why AGENTS.md §3 exists and why it matters here: discovery is now
correct, and the green `Failed: 0` it produces is still worthless, because the
suite has no assertions in it. Not fixed in this phase — that is
suite-authoring work, not a discovery filter.

**What I believe is required:** a phase that replaces the commented-out bodies
in those three files with real `expect … to be …` assertions, so that
`rb test` becomes a gate. Until then any `rb test` result, pass or fail, carries
almost no information.

## 8. Deferred: an empty `.rb` file is indistinguishable from a valid empty suite

A zero-byte `bad_empty.rb` is collected by `find_test_files` and yields zero
tests, so it is never reported. Not pinned by a test here, because the only
assertion available would encode the current behaviour as correct. Belongs with
F5's phase, which owns the marker grammar.

## 9. Deferred: `modules/MathUtils.rb` does not parse

`rb run modules/MathUtils.rb` fails:

```
Error: ParserError: Expected function name
  --> modules/MathUtils.rb:4:16
4 | constant PI to 3.14159
```

AGENTS.md §2 says `modules/*.rb` are specification-by-example and the gate runs
them, so this is a backwards-compatibility break that predates this phase.
Verified pre-existing: identical error with this phase's diff stashed
(`git stash -u`, rebuild, rerun). The other 11 files in
`examples/` + `modules/` all pass. `constant … to …` is presumably a keyword
the parser does not implement; needs its own parser phase.