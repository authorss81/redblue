# Phase 043 — WebAssembly playground: compile the interpreter to WASM

> **Round 2 review: every BLOCKER, MAJOR and MINOR finding is fixed.** The
> findings and what each turned into are in
> [`REVIEW-RESPONSE.md`](REVIEW-RESPONSE.md); the gate results there are the
> current ones. The rest of this file is the original report, updated where
> round 2 changed what it claims, and left otherwise as written so the rounds
> can be read against each other.

## Finding, reproduced

`Cargo.toml` defined `[[bin]] rb` and `[lib] redblue` with no wasm target,
profile or bindings anywhere in the tree, so the only way to run a Redblue
program was a native binary.

```
$ cargo build --target wasm32-unknown-unknown
error: can't find crate for `std`
```

The target was not installed, and after `rustup target add wasm32-unknown-unknown`
the build still failed, which is the part of the finding that mattered:

```
error[E0433]: cannot find `blocking` in `reqwest`
   --> src/runtime.rs:1246:50
1246 |  fn network_client(span: Span) -> Result<reqwest::blocking::Client> {
     |                                                 ^^^^^^^^^^^^^^^ could not find `blocking` in `reqwest`
note: found an item that was configured out
   --> reqwest-0.11.27/src/lib.rs:352:13
352 | pub mod blocking;
    |         ^^^^ the item is gated here
    --> reqwest-0.11.27/src/lib.rs:256:9
256 |     #[cfg(not(target_arch = "wasm32"))]
```

Four further blockers surfaced behind that one, each found by trying to build and
run rather than by reading: the interpreter refuses to run at all without a
thread, `SystemTime::now()` panics, `thread::sleep` panics, `std::fs` is
unsupported, and a Redblue function calling another function overflows the 1 MiB
shadow stack and kills the module instance. All five are I/O- or platform-layer
facts, not language facts, and all five are fixed below.

## What changed

| File | Lines | What |
|---|---|---|
| `Cargo.toml` | +10 −0 | `[lib] crate-type = ["rlib", "cdylib"]` — the crate type that emits a `.wasm` and exports the `rb_*` symbols. `rlib` kept, so the native `rb` and the test suite link exactly as before. |
| `build.rs` | +28 −0 | `-zstack-size` for `wasm32-unknown-unknown`, read from `CARGO_CFG_TARGET_ARCH`. **(Superseded in round 1: the size is now computed from the same `MAX_CALL_DEPTH * STACK_BYTES_PER_CALL` product the native thread uses, and both numbers live here.)** |
| `src/wasm.rs` | +339 | The output boundary (`emit`), `run_program`, and the exported C ABI (`rb_alloc` … `rb_set_clock`). New file. **(Round 2: +568. Three exports added (`rb_read_output`, `rb_read_error`, `rb_run_into`), `rb_release` now takes a pointer, `rb_alloc` is bounded, and every ABI entry point — reads included — takes the run lock.)** |
| `src/vfs.rs` | +134 | `std::fs` re-exported on native; an in-memory filesystem on `wasm32-unknown-unknown`. New file. **(Round 2: +141. `exists` answers for directories; the UTF-8 message is `std`'s sentence exactly.)** |
| `src/runtime.rs` | +161 −33 | `files.*` through `vfs`; `network.*` and `time.sleep` refuse cleanly on wasm; `time.now()` reads a host clock; both network bodies extracted to `network_get`/`network_post` so the `#[cfg]` is at the fn boundary. |
| `src/interpreter.rs` | +19 −13 | `say`/`print` go through `wasm::emit`; `run_isolated` falls back to running inline when no thread can be spawned. **(Round 2: the inline fallback is wasm-only and every other target refuses; `say`'s buffer is charged against `MAX_OUTPUT_BYTES` as lines are buffered.)** |
| `src/bytecode/vm.rs` | +2 −2 | Same output boundary in the bytecode VM, so `rb vm` and `rb run` cannot drift. **(Round 2: and the same charge on the buffer.)** |
| `src/lib.rs` | +2 −0 | `pub mod vfs; pub mod wasm;` **(Round 2: +1 — `run_isolated_with_depth`.)** |
| `tests/wasm_test.rs` | +362 | 14 tests. New file. **(Round 1: 20. Round 2: 28 — eight added, none removed.)** |
| `wasm/redblue.js` | +168 | The bindings. No `wasm-bindgen`, no generated glue. New file. **(Round 2: reads both channels through the copying accessors, and releases by pointer.)** |
| `wasm/index.html` | +185 | The runner page. New file. |
| `wasm/check-examples.js` | +111 | The byte-identical-output proof. New file. **(Round 2: also asserts both output-limit paths, the source-size limit and directory `exists` against the built module.)** |
| `wasm/README.md` | +132 | Build, run, and the list of differences. New file. |

### The documented `wasm` build

```bash
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown --lib
# -> target/wasm32-unknown-unknown/release/redblue.wasm
```

All 15 exports present, verified against the built module (12 before round 2,
which added `rb_read_output`, `rb_read_error` and `rb_run_into`):

```
rb_ exports: rb_alloc rb_error_len rb_error_ptr rb_keywords_len
  rb_keywords_ptr rb_output_len rb_output_ptr rb_read_error rb_read_output
  rb_release rb_run rb_run_into rb_set_clock rb_version_len rb_version_ptr
```

### Byte-identical output

`node wasm/check-examples.js`, native `rb` vs the module under `node`:

```
ok    hello.rb  (43 bytes identical)  — say, variables and a function call
ok    fizzbuzz.rb  (68 bytes identical)  — repeat, nested if and for-each
ok    formats.rb  (151 bytes identical)  — json.parse / json.stringify / records
ok    test_arithmetic.rb  (45 bytes identical)  — arithmetic and string formatting
ok    random.rb  (182 bytes identical)  — the seeded random builtins
ok    files.rb  (160 bytes identical)  — the files module, through the interpreter

6 examples, byte-identical under node and native rb.
```

`examples/time.rb` is deliberately excluded: it prints the wall clock, so it has
no fixed bytes for two pipelines to agree on.

## Tests added

| Test | Edge class covered |
|---|---|
| `wasm_run_reproduces_the_native_output_of_every_example` | the whole contract: all six programs, byte for byte |
| `wasm_example_corpus_is_exactly_the_deterministic_examples` | corpus drift — a new example that is not in `EXAMPLES` fails |
| `edge_print_is_immediate_and_say_flushes_at_the_end` | **empty** (`print` with no trailing newline); singleton (one `say`); ordering |
| `edge_empty_and_whitespace_only_programs_produce_no_output` | **empty** — empty source, blank lines, whitespace only |
| `edge_unicode_survives_the_boundary_intact` | **unicode** — emoji, CJK, RTL, combining mark; asserts byte length, not char length |
| `a_malformed_program_is_reported_and_is_not_output` | **malformed input** — unterminated string, and a failure is `Err` not output |
| `a_runtime_failure_is_reported_rather_than_panicking` | **failure produced** — `1 / 0` returns `Err` with a message |
| `wasm_abi_round_trips_source_and_output` | the ABI itself: alloc → write → run → read back, natively |
| `edge_abi_rejects_bytes_that_are_not_utf8` | **malformed input** at the ABI boundary — invalid UTF-8 is `status != 0`, never UB |
| `edge_abi_refuses_a_negative_length` | **out of bounds** — `len = -1` refused before it becomes a huge `usize` |
| `wasm_module_reports_its_version_and_keywords` | the page's metadata; a zero length would read as an empty string |
| `files_reads_back_what_it_wrote_in_a_temp_dir` | write/append/read/copy/rename/delete/exists round trip, `target/tmp/` |
| `edge_reading_a_file_that_was_never_written_is_a_catchable_error` | **resource/state** — a missing file is a catchable `NotFound`, not an empty file; the edge a `HashMap` lookup gets wrong |
| `edge_recursion_beyond_the_call_depth_is_a_clean_error_not_an_abort` | **resource limit** — `MAX_CALL_DEPTH` is a catchable `Limit` error and the module survives; this is the assertion that fails without the stack argument |
| `edge_a_failure_keeps_the_output_written_before_it` | **failure produced** — the output channel and the error channel are independent |
| `edge_an_allocation_stays_put_however_many_programs_follow_it` | **state** — an arena entry does not move, and every release is asserted |
| `edge_output_past_the_playground_limit_is_reported_not_truncated` | **resource limit** — `print` past `MAX_OUTPUT_BYTES` is refused, not truncated |
| `edge_say_output_is_bounded_while_it_is_still_buffered` | **resource limit** — `say`'s *buffer* is charged per line as it is buffered; exact byte arithmetic, catchable, sticky |
| `edge_an_allocation_past_the_source_limit_is_refused_before_anything_is_allocated` | **out of bounds** — a 2 GiB `rb_alloc` is a null with the number in the error channel |
| `edge_an_allocation_is_released_by_its_own_address_and_only_once` | **state** — release by pointer, out of order, and a double release refused |
| `edge_an_error_too_long_for_its_buffer_is_truncated_and_says_so` | **resource/state** — the one thing that can outgrow its channel, and it says so |
| `edge_the_output_is_copied_out_under_the_lock_rather_than_read_through_a_pointer` | **concurrency** — eight threads running and copying at once, each getting its own bytes |
| `edge_a_run_that_cannot_be_given_its_stack_is_refused_rather_than_run_unprotected` | **resource limit** — no thread for the depth is a catchable `Limit`, not an inline run |
| `edge_the_in_memory_filesystem_answers_for_directories_too` | **state** — `files.exists` answers for a directory, and the separator is what decides |
| `edge_bytes_that_are_not_text_say_the_same_thing_in_both_builds` | **type mismatch at the boundary** — the exact `std::fs` sentence, and the path named once |
| `the_in_memory_filesystem_behaves_like_the_one_it_replaces` | the playground's filesystem, compiled natively so `cargo test` can run it |
| `bytes_write_and_files_read_agree_on_where_the_bytes_went` | `bytes.write` and `files.read` reach one filesystem |

Plus, in `wasm/check-examples.js`: `network.get`, `time.sleep` and `input` are
refused with a message and print nothing; `time.now()` reads the host clock; both
output-limit paths (`say`'s buffer and `print`'s writes) are refused with the
number in the message; a source past `MAX_SOURCE_BYTES` is refused with the
number; `files.exists` answers for a directory; and the module still runs
afterwards.

### Edge-case matrix

| Row | Covered by |
|---|---|
| empty / zero / nothing | `edge_empty_and_whitespace_only_programs_produce_no_output`, `edge_print_is_immediate_and_say_flushes_at_the_end` |
| singleton and boundary | `edge_print_is_immediate_and_say_flushes_at_the_end` (one `say`; `print` with no trailing newline), `wasm_abi_refuses_a_negative_length`, `edge_an_allocation_is_released_by_its_own_address_and_only_once` (`rb_alloc(0)`) |
| out of bounds | `edge_abi_refuses_a_negative_length`, `edge_an_allocation_past_the_source_limit_is_refused_before_anything_is_allocated` |
| type mismatch | `edge_abi_rejects_bytes_that_are_not_utf8` at the ABI (source is bytes); `edge_bytes_that_are_not_text_say_the_same_thing_in_both_builds` in the filesystem (a file that does not decode). No *value* crosses the ABI as a typed value; the existing type-mismatch behaviour is unchanged and already covered by `tests/numeric_edge_test.rs` |
| type coercion boundaries | N/A for this change — no arithmetic crosses the boundary. `examples/test_arithmetic.rb` and `examples/random.rb` are in the byte-identical corpus, so the arithmetic results are pinned to native `rb`'s |
| unicode and escapes | `edge_unicode_survives_the_boundary_intact` (emoji, CJK, RTL, combining mark, byte length) |
| nesting and recursion | `edge_recursion_beyond_the_call_depth_is_a_clean_error_not_an_abort`, `edge_a_run_that_cannot_be_given_its_stack_is_refused_rather_than_run_unprotected`; `examples/fizzbuzz.rb` in the corpus |
| duplicate and missing keys | N/A for this change — no record crosses the boundary as a record; `examples/formats.rb` in the corpus pins record output |
| malformed input | `a_malformed_program_is_reported_and_is_not_output`, `edge_abi_rejects_bytes_that_are_not_utf8`, `edge_an_error_too_long_for_its_buffer_is_truncated_and_says_so` |
| resource and state | `edge_recursion_beyond_the_call_depth_is_a_clean_error_not_an_abort`, `edge_reading_a_file_that_was_never_written_is_a_catchable_error`, `edge_say_output_is_bounded_while_it_is_still_buffered`, `edge_output_past_the_playground_limit_is_reported_not_truncated`, `edge_an_allocation_stays_put_however_many_programs_follow_it`, `edge_the_in_memory_filesystem_answers_for_directories_too` |
| concurrency | `edge_the_output_is_copied_out_under_the_lock_rather_than_read_through_a_pointer` — eight threads running and copying at once, each asserting it reads back its own run |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test --all-targets` | **1258 passed, 0 failed, 0 ignored** |
| `cargo clippy --target wasm32-unknown-unknown -- -D warnings` | pass, zero warnings |
| `cargo build --release --target wasm32-unknown-unknown --lib` | pass, **15** `rb_*` exports |
| `node wasm/check-examples.js` | 6 examples byte-identical; refusals, both output limits, the source limit and directory `exists` asserted |
| `./rbops/verify.sh phase-043` | **NOT RUN — `rbops/` is not present in this checkout** |

`rbops/` does not exist in the project working directory (`ls rbops/` →
`No such file or directory`; a filesystem search for `verify.sh` under any
`rbops` path returns nothing). The RBOPS pipeline that invoked this run lives
outside the checkout, and the brief is explicit that it is not to be inspected.
The gate was therefore not run and is not claimed. The three gates above that
*can* be run were, and each is reported with what it actually printed.

## Invariants touched

- **None.** No `#[cfg]` alters how a program is parsed, analyzed or executed.
  The language surface, the `Value` variants, the `Error` variants and the
  `.rb` extension are untouched. Round 2's one new failure mode — an output
  limit a program can catch — uses the `Error::Limit` variant the step, iteration
  and call-depth limits already used, and adds no variant of its own.

### Every difference, listed

All of them are at the I/O or platform boundary:

| | native `rb` | playground |
|---|---|---|
| program output | written to stdout | collected into a buffer, handed over by `rb_run_into` under one lock (`src/wasm.rs::emit`) |
| `files.*` | `std::fs`, against the disk | in-memory filesystem (`src/vfs.rs`): flat path → content, gone when the page closes. **A directory exists when something is stored in it** (round 2) |
| `network.get` / `network.post` | `reqwest`'s blocking client | clean `Runtime` error: no threads to block on |
| `time.sleep` | `std::thread::sleep` | clean `Runtime` error: one thread, and blocking it would freeze the page |
| `time.now()` | `SystemTime::now()` | the host's clock, via `rb_set_clock` before each run |
| `run_isolated` | runs the program on a thread sized `MAX_CALL_DEPTH × STACK_BYTES_PER_CALL`; a thread that cannot be had is a catchable `Limit` **(round 2)** | runs it inline; `build.rs` links a shadow stack sized from the **same product**, so `MAX_CALL_DEPTH` fits **(round 1)** |
| shadow stack | 8 MiB main thread, plus a `MAX_CALL_DEPTH × STACK_BYTES_PER_CALL` thread per run | the same product, linked via `-zstack-size` in `build.rs` **(round 1)** |

Two of these deserve their reasoning, because each was a bug found by running,
not by reading:

**`run_isolated` had to fall back.** `std::thread::Builder::spawn` fails on
`wasm32-unknown-unknown`, and the old code turned that into
`Error::Io("Cannot start the interpreter thread")` — so the module refused
*every* program. It now runs the same `vm.run` inline when the spawn fails.

**(Round 2.)** Round 1's version ran inline on *any* target whose spawn failed,
which is exactly backwards: the thread's whole purpose is stack size, and the
caller's stack is not `MAX_CALL_DEPTH × STACK_BYTES_PER_CALL`. The inline run is
now scoped to `wasm32-unknown-unknown`, where `build.rs` has linked the one
stack the module has from that same product — so there it is not a fallback but a
run on a stack sized for the whole limit. Every other target refuses the run
with a `Limit` instead, because running there would turn the catchable `Limit`
the depth counter raises back into a stack overflow.

**The stack argument is required and is in `build.rs` for a reason.**
`wasm32-unknown-unknown` defaults to a 1 MiB shadow stack. `MAX_CALL_DEPTH` is
1000, and natively that fits because `run_isolated` sizes a thread for it. On
wasm there is no thread, so the same depth has to fit in the one stack — and at
1 MiB a Redblue function calling another trapped with "memory access out of
bounds" at **depth 1**, which kills the whole module instance rather than
returning an error. `build.rs` links a stack sized from the same
`MAX_CALL_DEPTH * STACK_BYTES_PER_CALL` product `run_isolated` multiplies out.
It is a *link* argument, not a limit anything reads, and after it both builds
produce the *same* call-depth error for the same program.

**(Round 1.)** The size was originally hand-written as 16 MiB against a native
256 MiB, which did *not* let the two builds reach the same depth. Both numbers
now live in `build.rs` and `src/interpreter.rs` includes them from there, so
the counter the VM checks and the stack the linker reserves are the same two
numbers by construction.

`build.rs` rather than `.cargo/config.toml` because CI sets an environment
`RUSTFLAGS` (`-D warnings`), which takes precedence over `target.*.rustflags`
and silently discards it — verified: with the config in place and `RUSTFLAGS`
set, the flag did not reach the linker. `CARGO_CFG_TARGET_ARCH` rather than a
`#[cfg]` in `build.rs` because a build script is compiled for the **host**, so
`cfg!(wasm32)` there is false on the Linux machine cross-compiling to wasm —
which is exactly the case that needs it. Both mistakes were made, caught and
fixed in this run.

## Known gaps / follow-ups

- **`rbops/verify.sh` was not run** — see Gates. Nothing in this phase claims it
  passed.
- **The in-memory filesystem is flat and lives as long as the module.** No
  permissions or symlinks, and **no empty directories** — a directory exists
  when something is stored in it, so a directory no Redblue program created and
  no file was written into is one this cannot answer for. `files.read` of a
  directory-shaped path is `NotFound` in both builds. It is enough for
  `examples/files.rb` to produce byte-identical output and it is not a
  `std::fs` replacement for anything larger.
- **`rb_alloc`/`rb_run` is a two-call protocol.** A host that calls `rb_run`
  with a pointer it did not get from `rb_alloc` breaks the contract its own
  `unsafe` says it must keep. That is what an FFI boundary is; the Rust tests
  walk the protocol so a mistake fails in `cargo test`. **(Round 2: the read
  half is best walked through `rb_run_into`, which is the run and both copies
  under one lock. `rb_run` then `rb_read_output` is two calls and another
  thread's run can land between them — which is what the concurrency test
  caught — so that form is for a host that runs one program at a time.)**
- **The five `wasm-bindgen` imports are stubbed**, not linked. They come from
  `chrono`'s and `reqwest`'s own wasm targets and nothing a Redblue program can
  reach uses them; the stubs that would fire throw. A future dependency bump
  that makes one reachable would surface as that throw.
- **`time.now()` in the playground is the host's clock**, so it is whatever the
  browser reports. That is the same trust a native build places in the system
  clock; the arithmetic around it is the same code.
- **`examples/time.rb` is outside the corpus** and therefore outside the
  byte-identical claim. Nothing about it is excluded from *running* in the
  playground — `time.now()` works there — it simply has no fixed bytes.
- **Pre-existing, out of scope, found while testing:** a Redblue function that
  calls itself reports `Maximum call depth of 1000 reached` on **every**
  recursion, including `f(3)`. Confirmed identical on a stashed, pristine tree,
  so it is not from this phase. Recorded in `FINDINGS.md`.

## Test requirements

- New `#[test]` functions: **14** (floor is 3), in `tests/wasm_test.rs`.
  **(Round 1: 20 — six added, none removed. Round 2: 28 — eight added, none
  removed. See `REVIEW-RESPONSE.md`.)**
- New `edge_*` tests: **7** —
  `edge_print_is_immediate_and_say_flushes_at_the_end`,
  `edge_empty_and_whitespace_only_programs_produce_no_output`,
  `edge_unicode_survives_the_boundary_intact`,
  `edge_reading_a_file_that_was_never_written_is_a_catchable_error`,
  `edge_recursion_beyond_the_call_depth_is_a_clean_error_not_an_abort`,
  `edge_abi_rejects_bytes_that_are_not_utf8`, `edge_abi_refuses_a_negative_length`.
  **(Round 1: 12 — five added:
  `edge_a_failure_keeps_the_output_written_before_it`,
  `edge_an_allocation_stays_put_however_many_programs_follow_it`,
  `edge_output_past_the_playground_limit_is_reported_not_truncated`,
  `edge_the_platform_builtins_still_answer_natively`, and the in-memory
  filesystem test. Round 2: 19 — eight added, all listed in
  `REVIEW-RESPONSE.md`. None removed at any round.)**
- Tests asserting a failure is produced: **15** —
  `a_runtime_failure_is_reported_rather_than_panicking`,
  `a_malformed_program_is_reported_and_is_not_output`,
  `edge_abi_rejects_bytes_that_are_not_utf8`,
  `edge_abi_refuses_a_negative_length`,
  `edge_a_failure_keeps_the_output_written_before_it`,
  `edge_output_past_the_playground_limit_is_reported_not_truncated`,
  `edge_say_output_is_bounded_while_it_is_still_buffered`,
  `edge_an_allocation_past_the_source_limit_is_refused_before_anything_is_allocated`,
  `edge_an_allocation_is_released_by_its_own_address_and_only_once`,
  `edge_an_error_too_long_for_its_buffer_is_truncated_and_says_so`,
  `edge_a_run_that_cannot_be_given_its_stack_is_refused_rather_than_run_unprotected`,
  `edge_the_output_is_copied_out_under_the_lock_rather_than_read_through_a_pointer`
  (a null destination is a refusal, not a crash),
  `edge_reading_a_file_that_was_never_written_is_a_catchable_error`,
  `edge_bytes_that_are_not_text_say_the_same_thing_in_both_builds`,
  `edge_the_platform_builtins_still_answer_natively`.
- New `#[ignore]` / `// skip` / `allow(clippy::`: **0**.
- Newly-failing pre-existing tests: **0** (1258 passed, 0 failed, 0 ignored).

`must_touch: ["src/"]` — satisfied: `src/wasm.rs`, `src/vfs.rs` (new) and
`src/interpreter.rs`, `src/runtime.rs`, `src/bytecode/vm.rs`, `src/lib.rs`
(changed).