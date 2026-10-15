# Phase 057 — FINDINGS

## Drive-by: a pre-existing race in `tests/repl_color_test.rs`

**Evidence.** `tests/repl_color_test.rs:290` (`edge_banner_is_stable_across_calls`)
and `tests/repl_color_test.rs:313` (`edge_a_narrow_terminal_centres_without_panicking`)
both call `std::env::set_var("COLUMNS", …)` and `remove_var`. The banner reads
the width from the same variable at `src/repl/banner.rs:97`:

```rust
if let Ok(c) = std::env::var("COLUMNS") {
```

Cargo runs the tests of one binary on parallel threads sharing one process
environment, so whichever of the two calls `remove_var` between the first and
second `banner_text` call decides what the other sees. Observed once during this
phase under `cargo test --all-targets`:

```
test edge_banner_is_stable_across_calls ... FAILED
assertion `left == right` failed: the banner must not change between calls
  left:  "                       \u{1b}[1;31m███ …"    (centred at COLUMNS=100)
 right: "\u{1b}[1;31m███ …"                           (no COLUMNS at all)
```

It is not caused by phase-057: the diff touches neither `COLUMNS`, `Style` nor
the banner, and the failing binary is a different one from the tests this phase
added. It passed 12/12 in isolation, and the clean tree passed one full suite
run — i.e. it is a flake, exactly as the test's own comment warns.

**Fixed here**, because it reddens the gate for whatever phase happens to run
next: a `static COLUMNS_LOCK: Mutex<()>` in `tests/repl_color_test.rs`, taken by
both tests. No assertion weakened, no threshold lowered, no test skipped.

**For the auditor, as a separate phase:** the real fix is to stop the library
reading a global. `banner_text` could take the width as an argument, with the
`COLUMNS` read confined to `Repl::run`. Nothing depends on the current shape
outside `tests/repl_color_test.rs`, but that is a public-API change and is
therefore not this phase's to make.

## `.bench print "x"` writes once per run

`.time` and `.bench` drain the VM's buffered output after every run
(`src/repl/mod.rs`, `time_runs`), so `.bench say 1` is silent and the buffer
cannot grow. `print` and `console.log` do not go through that buffer — they
write at the point they are reached, which `Vm::run_quietly` already documents
at `src/interpreter.rs:1354` — so `.bench print "x"` still writes N lines to the
terminal. Capturing or refusing them inside a measurement would mean changing
what a language effect does, which is a language decision rather than a REPL
one. File and line: `src/repl/mod.rs`, `time_runs`.

## `rbops/verify.sh` was not run

`rbops/` is not present in this checkout (`ls rbops` → *No such file or
directory*), so the fourth gate could not be executed here. It was not modified,
inspected or stubbed. The three Rust gates were run: `cargo fmt --all -- --check`,
`cargo clippy --all-targets -- -D warnings`, `cargo test --all-targets`
(1720 passed, 0 failed) and `cargo test --doc` (2 passed).