# Phase 124 — Fix rustdoc warnings, including links to a module that does not exist

## What changed

Re-verified on parent `f55ccc9`: `cargo doc --no-deps` reported **58 lib warnings
and 1 `rb-bench` warning (59 total)**, not 34. The audit undercounted (it counted
distinct broken targets rather than each emitted link), but every warning it named
was present, including the three `crate::vm` links. Separately, `README.md`'s
Project Structure listed a `vm.rs` file that does not exist; that is a prose fix,
not a rustdoc warning, because rustdoc does not lint Markdown files.
After the change `cargo doc --no-deps` and
`RUSTDOCFLAGS="--document-private-items -D warnings" cargo doc --no-deps` both
report zero warnings.

The work was already present in the adopted resume commit `dd5337a`; this run
verified it and wrote the missing report. The round-1 review fixes then isolated
the nested `cargo doc` target directory, restored the `Style::forced(false)`
prose, and replaced assertions on rustdoc's wording with assertions on its stable
lint names. A later round split the positive gate into two passes (with and
without `--document-private-items`) so the private-link lint cannot be silently
suppressed, and made a spawn failure report its missing precondition instead of
panicking in an `.expect`. No production behaviour changed.

| File | Lines | What |
|---|---|---|
| README.md | +1 −1 | Project Structure names `interpreter.rs`, the file that exists, instead of `vm.rs` |
| NATIVE.md | +2 −2 | citation line pins follow the roadmap's shifted comparison table (`112/115` → `119/122`) |
| src/analyzer.rs | +1 −1 | private `Analyzer::declare_in_program_scope` link rewritten as prose code span |
| src/bootstrap.rs | +4 −4 | `bytecode::vm::BytecodeVm` retargeted to `crate::bytecode::vm::BytecodeVm`; `STAGE2` / `Stage2::Bytes` links rewritten as code spans |
| src/bytecode/format.rs | +1 −1 | private link rewritten as code span |
| src/bytecode/format.rs | +2 −2 | `native_study` citation line pins follow the shifted roadmap (`112/115` → `119/122`) |
| src/bytecode/opcode.rs | +1 −1 | private `call_method` link rewritten as code span |
| src/bytecode/vm.rs | +4 −4 | `crate::vm` → `crate::Vm`; `crate::runtime`, `BytecodeVm::call_depth`, `loop_sites` links rewritten as code spans |
| src/debugger.rs | +2 −2 | unresolved `Rc` and private `Frame::capture` links rewritten as code spans |
| src/error.rs | +1 −1 | private `display_width` link rewritten as code span |
| src/failure.rs | +1 −1 | removed redundant explicit link target |
| src/interpreter.rs | +14 −14 | private `Vm::*`, `resolve_limit_from`, `run_without_a_sized_thread`, `Repl::record_definitions` links rewritten as code spans |
| src/lib.rs | +211 −0 | `doc_link_tests` module: the rustdoc gate and its checked-in fixtures |
| src/manifest.rs | +6 −6 | private `check_lockfile`, `Pins::*` links and unresolved `Display` rewritten as code spans |
| src/parser.rs | +2 −2 | private links rewritten as code spans |
| src/path.rs | +1 −1 | private `crate::runtime::builtin` link rewritten as code span |
| src/repl/commands.rs | +3 −3 | unresolved `Run`, `bare_names` and private `COMMAND_TABLE` links rewritten as code spans |
| src/repl/shell.rs | +2 −2 | private `wait_for`, `DIRECTORY_SEPARATOR` links rewritten as code spans |
| src/repl/style.rs | +1 −1 | unresolved `Style::forced(false)` link rewritten as code span |
| src/runtime.rs | +3 −3 | `crate::vm` → `crate::Vm`; `crate::runtime::builtin` link rewritten as code span; private `set_host_clock` link rewritten as code span |
| src/statistics.rs | +1 −1 | private `crate::runtime::builtin` link rewritten as code span |
| src/stdlib.rs | +8 −8 | private `call_method`, `crate::runtime::builtin`, `signatures_cover_the_registry`, `crate::runtime` links rewritten as code spans |
| src/value.rs | +1 −1 | private `finite_number` link rewritten as code span |
| src/vfs.rs | +6 −6 | unresolved `Path::parent`/`Path::new`, private `normalize`, redundant explicit link target rewritten |
| src/wasm.rs | +4 −4 | private `HostBuffer`, `RUN_LOCK`, `run_lock` links rewritten as code spans |
| tests/bench/mod.rs | +1 −1 | unresolved `check_size_supported` link rewritten as code span |
| tests/fixtures/{empty_doc,private_doc_link,private_item_unresolved_doc_link,public_doc_link,unresolved_doc_link}.rs | +14 −0 | checked-in positive/negative rustdoc fixtures |
| tests/unreviewed_work_ack_test.rs | +1 −0 | pins the prior resume commit by SHA |

Every private-item link was handled by **rewriting the prose without a link**
(keeping the wording, turning `[`X`]` into `` `X` ``); no item was made public
just to silence rustdoc. The two `crate::vm` links were retargeted to the public
`crate::Vm`, because the prose names the tree-walker that does exist.

### Resume fix — a docs regression the phase window inherited

The phase base is `f55ccc9`. Between that base and this resumed commit, two
docs commits (`4d1a4f2`, `2403b16`) reorganized
`docs/FUTURE_ROADMAP_AND_DEFERRED.md`, moving the "Rust / C / C++" comparison
row from line 112 to 119 and the LuaJIT row from 115 to 122. `NATIVE.md` still
cited the old lines, and `native_study_records_a_recommendation_and_the_components_it_cites`
(`src/bytecode/format.rs`) pinned the same numbers, so `cargo test` was red on
the adopted tree before any phase-124 edit. The fix updates both the prose
citation and the test's line pins to the rows that actually hold those symbols;
the assertions (the line must contain `Rust` / `LuaJIT`) are unchanged, so this
is a re-pin, not a weakened test. This is the only non-rustdoc change, and it is
what made the full suite green.

## The 59 reproduced warnings, each resolved

Measured on parent `f55ccc9` with `cargo doc --no-deps`; the warning location is
the old `file:line:col`.

| # | Warning | Resolution |
|---|---|---|
| 1 | `with_bound_names` → private `Analyzer::declare_in_program_scope` (src/analyzer.rs:132) | code span |
| 2 | unresolved `bytecode::vm::BytecodeVm` (src/bootstrap.rs:31) | retargeted `crate::bytecode::vm::BytecodeVm` |
| 3 | `run_compiler_on` → private `STAGE2` (src/bootstrap.rs:327) | code span |
| 4 | `CallMethod` → private `crate::bytecode::vm::BytecodeVm::call_method` (src/bytecode/opcode.rs:152) | code span |
| 5 | unresolved `crate::vm` (src/bytecode/vm.rs:3) | retargeted `crate::Vm` |
| 6 | `vm` → private `crate::runtime` (src/bytecode/vm.rs:8) | code span |
| 7 | `vm` → private `BytecodeVm::call_depth` (src/bytecode/vm.rs:18) | code span |
| 8 | `vm` → private `loop_sites` (src/bytecode/vm.rs:35) | code span |
| 9 | unresolved `Rc` (src/debugger.rs:60) | code span |
| 10 | `frame_locals` → private `Frame::capture` (src/debugger.rs:140) | code span |
| 11 | `Span` → private `display_width` (src/error.rs:25) | code span |
| 12 | `resolve_max_iterations_from` → private `resolve_limit_from` (src/interpreter.rs:99) | code span |
| 13 | `resolve_max_steps_from` → private `resolve_limit_from` (src/interpreter.rs:105) | code span |
| 14 | `run_isolated_with_depth_in` → private `run_without_a_sized_thread` (src/interpreter.rs:180) | code span |
| 15 | `ask_questions` → private `Vm::asking_a_question` (src/interpreter.rs:971) | code span |
| 16 | `function_body` → private `Vm::call` (src/interpreter.rs:979) | code span |
| 17 | `function_body` → private `Vm::call_method` (src/interpreter.rs:979) | code span |
| 18 | `function_body` → private `Vm::get_var` (src/interpreter.rs:983) | code span |
| 19 | `function_body` → private `Vm::call_method` (src/interpreter.rs:986) | code span |
| 20 | `module_behind` → private `Vm::declare_module` (src/interpreter.rs:1021) | code span |
| 21 | `user_names` → private `Vm::declare` (src/interpreter.rs:1296) | code span |
| 22 | `user_names` → private `Vm::set_var` (src/interpreter.rs:1297) | code span |
| 23 | `user_names` → private `CallContext::qualified_members` (src/interpreter.rs:1306) | code span |
| 24 | `user_names` → private `Vm::call_method` (src/interpreter.rs:1307) | code span |
| 25 | `published_member` → private `Vm::publish_member` (src/interpreter.rs:1351) | code span |
| 26 | `declares_module` → private `crate::repl::Repl::record_definitions` (src/interpreter.rs:1652) | code span |
| 27 | `manifest` → private `check_lockfile` (src/manifest.rs:48) | code span |
| 28 | unresolved `Display` (src/manifest.rs:122) | code span |
| 29 | `parse_lockfile` → private `check_lockfile` (src/manifest.rs:689) | code span |
| 30 | `check_program_language` → private `Pins::check_program_language_for` (src/manifest.rs:1625) | code span |
| 31 | `check_program_language` → private `Pins::for_analysis` (src/manifest.rs:1627) | code span |
| 32 | `check_program_language` → private `Pins::for_session` (src/manifest.rs:1628) | code span |
| 33 | `builtin` → private `crate::runtime::builtin` (src/path.rs:393) | code span |
| 34 | unresolved `Run` (src/repl/commands.rs:92) | code span |
| 35 | unresolved `bare_names` (src/repl/commands.rs:963) | code span |
| 36 | `help_lines` → private `COMMAND_TABLE` (src/repl/commands.rs:997) | code span |
| 37 | `pipe` → private `wait_for` (src/repl/shell.rs:351) | code span |
| 38 | `listing` → private `DIRECTORY_SEPARATOR` (src/repl/shell.rs:431) | code span |
| 39 | unresolved `Style::forced(false)` (src/repl/style.rs:242) | code span |
| 40 | `statistics` → private `crate::runtime::builtin` (src/statistics.rs:3) | code span |
| 41 | `dotted_call_reaches` → private `crate::interpreter::Vm::call_method` (src/stdlib.rs:324) | code span |
| 42 | `dotted_call_reaches` → private `crate::bytecode::vm::BytecodeVm::call_method` (src/stdlib.rs:325) | code span |
| 43 | `map_function` → private `crate::interpreter::Vm::call_method` (src/stdlib.rs:376) | code span |
| 44 | `map_function` → private `crate::bytecode::vm::BytecodeVm::call_method` (src/stdlib.rs:377) | code span |
| 45 | unresolved `runtime::builtin` (src/stdlib.rs:1003) | code span |
| 46 | `BUILTIN_SIGNATURES` → private `crate::runtime::builtin` (src/stdlib.rs:2408) | code span |
| 47 | unresolved `signatures_cover_the_registry` (src/stdlib.rs:2413) | code span |
| 48 | `EFFECTFUL_BUILTINS` → private `crate::runtime` (src/stdlib.rs:3104) | code span |
| 49 | `expect_repeat_count` → private `finite_number` (src/value.rs:345) | code span |
| 50 | `exists` → private `normalize` (src/vfs.rs:219) | code span |
| 51 | unresolved `Path::parent` (src/vfs.rs:101) | code span |
| 52 | unresolved `Path::new` (src/vfs.rs:102) | code span |
| 53 | `MAX_OUTPUT_BYTES` → private `HostBuffer` (src/wasm.rs:60) | code span |
| 54 | `begin_capture` → private `RUN_LOCK` (src/wasm.rs:171) | code span |
| 55 | `rb_output_ptr` → private `run_lock` (src/wasm.rs:694) | code span |
| 56 | `rb_run_into` → private `run_lock` (src/wasm.rs:777) | code span |
| 57 | redundant explicit link target (src/failure.rs:3) | explicit target removed |
| 58 | redundant explicit link target (src/vfs.rs:288) | explicit target removed |
| 59 | unresolved `check_size_supported` (tests/bench/mod.rs:44, `rb-bench`) | code span |

The `README.md` change is deliberately absent from this table: rustdoc does not
lint Markdown, so the stale `vm.rs` entry was a separate prose fix, not one of
the 59 warnings.

## Tests added

All in `src/lib.rs::doc_link_tests` (the test-only module counts as `src/`).
The positive test runs `cargo doc --no-deps` **twice** with `RUSTDOCFLAGS`
promoting every warning to an error, because no single pass sees every failure:
once without `--document-private-items` (the only pass that reports a public doc
comment linking to a private item) and once with it (the only pass that reports
an unresolved link inside a private item). Together they mean any future private
or unresolved link fails CI. Each child gets its own `CARGO_TARGET_DIR` under
`target/tmp`, so the nested cargo calls cannot contend with the outer build's
lock or reuse each other's stale rustdoc output. A spawn failure is reported with
the missing precondition instead of an `.expect` panic. The `edge_*` tests run
the same `rustdoc` binary over checked-in fixtures, so a broken fixture path
cannot let the positive test pass for the wrong reason. The edge assertions
match the stable rustdoc lint names (`private_intra_doc_links`,
`broken_intra_doc_links`) and the fixture's target, not rustdoc's
human-readable wording, so a toolchain wording change cannot turn them red.

| Test | Edge class covered |
|---|---|
| `doc_links_resolve_to_public_items` | positive crate-wide gate, two passes: zero warnings and no private links without the flag, no unresolved links with it |
| `edge_private_intra_doc_link_is_rejected` | failure: link to a private item is refused |
| `edge_unresolved_intra_doc_link_is_rejected` | failure: link to a missing name is refused |
| `edge_private_item_link_requires_document_private_items` | boundary: proves `--document-private-items` is load-bearing |
| `edge_single_public_intra_doc_link_is_accepted` | singleton: one valid public link is accepted |
| `edge_document_with_no_links_is_accepted` | empty: a document with no links is accepted |

Fixtures: `tests/fixtures/public_doc_link.rs`,
`private_doc_link.rs`, `unresolved_doc_link.rs`,
`private_item_unresolved_doc_link.rs`, `empty_doc.rs`.

### Required edge-case matrix

| Row | Status |
|---|---|
| empty | covered — `edge_document_with_no_links_is_accepted` |
| singleton | covered — `edge_single_public_intra_doc_link_is_accepted` |
| boundary | covered — `edge_private_item_link_requires_document_private_items` |
| out_of_bounds | N/A — no indexing or collection behaviour changes |
| type_mismatch | N/A — no value/type behaviour changes |
| numeric_boundary | N/A — no arithmetic changes |
| unicode | N/A — no text or encoding changes |
| nesting_recursion | N/A — no recursion or scoping changes |
| duplicate_missing_keys | N/A — no record/collection changes |
| malformed_input | covered — `edge_unresolved_intra_doc_link_is_rejected` (a malformed link target) |
| resource_limit | N/A — no runtime resource path changes |

## Gates

Run on the resumed tree after the citation re-pin (base `f55ccc9`).

| Gate | Result |
|---|---|
| `cargo doc --no-deps` | 0 warnings |
| `RUSTDOCFLAGS="--document-private-items -D warnings" cargo doc --no-deps` | 0 warnings |
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 3308 passed, 0 failed, 0 ignored (88 suites) |
| `cargo test --lib doc_link_tests` | 6 passed, 0 failed |
| `../rbops/verify.sh phase-124` (base `f55ccc9`, fixture root) | pass — manifest, report, diff hygiene, must_touch, fmt, clippy, 3308 tests, doc tests, 6 tests / 5 edge / 3 failure asserts, report honesty |

The gate reads `REPORT.md` from the pipeline root and diffs against the phase
base. `dispatch.sh` copies `redblue/phases/phase-124/REPORT.md` into
`$RBOPS_ROOT/phases/phase-124/` after the run and sets `RBOPS_BASE_REF` to
`f55ccc9`; the pass above was reproduced with that base and a fixture root
containing this report, because the report lives in the project checkout while
the gate runs from the pipeline checkout.

## Invariants touched

- None. Only doc comments, README's file list, `NATIVE.md`'s citation line pins,
  a test's citation line pins, and a `#[cfg(test)]` module changed.

## Known gaps / follow-ups

- The audit's "34 warnings" undercounted the parent tree: re-running rustdoc on
  `f55ccc9` emits 59 (58 lib + 1 `rb-bench`). The count is now pinned by the
  positive test, which fails on any new warning rather than on a fixed number.
- The gate lives in `src/lib.rs`, so it runs with the library tests. It shells
  out to `cargo doc` twice with isolated `CARGO_TARGET_DIR`s, so it never shares
  the outer build's directory lock and each pass rebuilds its own rustdoc output.
  `cargo test` already requires a working cargo and a writable target directory,
  so the same preconditions hold here; a spawn failure is reported with the
  missing precondition rather than an `.expect` panic. If a future CI environment
  forbids spawning cargo from a test, the fixtures still cover the rustdoc
  semantics but the crate-wide check would need a dedicated CI step.
