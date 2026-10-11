# phase-043 — REVIEW-RESPONSE

Round 1 and round 2. What each finding turned into, and the gates re-run
after. Round 2 is first because it is the current state; round 1 is below it,
left as written so the two can be read against each other.

## Round 2

### BLOCKER 1. `Vm.output` was unbounded, so `MAX_OUTPUT_BYTES` bounded nothing a `say` could reach

`Statement::Say` pushed a line onto a `Vec<String>` and `Vm::run` wrote them all
at the end. `MAX_OUTPUT_BYTES` is charged in [`emit`](../../src/wasm.rs), which
only runs at that flush — so a program whose lines never reached the boundary,
because it failed first or never finished, grew without limit while it did. A
`repeat` of `MAX_ITERATIONS` long lines was a process-killer, and the 4 MiB the
module claimed to hold was not holding it. `BytecodeVm::output` had the same
shape.

**Fixed.** Both VMs keep an `output_bytes` counter, charged by `charge_output`
as each line is *buffered* — its own bytes plus the newline the flush appends —
against the same `crate::wasm::MAX_OUTPUT_BYTES` constant `emit` reads, so the
two engines and the two paths cannot drift. Past the bound it is an
`Error::Limit`, the same channel `MAX_CALL_DEPTH` and `MAX_ITERATIONS` use, so a
Redblue program can `catch` it like any other limit.

One byte per line leaves a ragged remainder — 4 MiB is not a whole number of
65-byte lines — and letting a later *shorter* line through into that remainder
would make what a program can still say depend on how its earlier lines happened
to divide the limit. So the charge sticks (`output_full`): once the bound has
stopped one `say`, every later `say` in that run is stopped too.

`edge_say_output_is_bounded_while_it_is_still_buffered` pins the byte
arithmetic exactly (`fills * per_line`, not "about 4 MiB"), that the run
*succeeds* when the limit is caught, and that the charge sticks. The same two
checks are asserted against the built module in `wasm/check-examples.js`, which
is the only place they can be proven true of the wasm build.

### MAJOR 2. One `rb_alloc` could ask for two gigabytes

`HostInput::reserve` did `vec![0u8; len]` for any `len` up to `i32::MAX`. One
host call could exhaust the module, and on a native build the host process too.

**Fixed.** `MAX_SOURCE_BYTES`, checked *before* anything is allocated, with the
refusal left in the error channel where a page already shows failures — so the
host prints the module's own sentence, including the number, rather than a
JavaScript guess. The bound is `MAX_OUTPUT_BYTES` for a reason and not by
accident: a host reading a full output channel into a buffer it asked the module
for needs room for exactly that, and a smaller bound would make the largest
legal run unreadable through the sanctioned path.

`rb_alloc(0)` is served, not refused — an empty program is a program — but it is
given no arena slot, because a `Box<[u8]>` of no bytes has a dangling address
and two of them would share it.

### MAJOR 3. `rb_release` released "the most recent" and never cleared it

`HostInput::release` popped a `last` index and pushed it onto `free` without
clearing it, so a second `rb_release` re-released an already-free index and
answered `-1`, and an allocation that was not the most recent could never be
freed at all. `tests/wasm_test.rs` released twice and ignored both returns, so
the leak passed vacuously.

**Fixed** by changing the ABI rather than the contract around it: **`rb_release`
now takes the pointer**, `rb_release(ptr: *const u8) -> i32`. Matching on the
address the host was handed makes an out-of-order release, an in-order release
and a double release three ordinary cases rather than one case and two errors.
The ignored returns are now asserted, and `edge_an_allocation_is_released_by_its_
own_address_and_only_once` releases three allocations out of order and refuses
each a second time. `wasm/redblue.js` was updated with it.

### MAJOR 4. A status branch no run could reach

`rb_run` treated a `host_output().set` that did not fit as an overflow and
answered `-1` with a second wording for it. It could not happen: the channel is
`MAX_OUTPUT_BYTES` and `emit` refuses past `MAX_OUTPUT_BYTES`, so `printed`
always fits — and an overflowing run had already returned from `run_outcome`
with a different message. A guard nobody can reach is a guard nobody can rely
on.

**Fixed** by deleting the branch. The invariant it rested on is now asserted
rather than branched on (`debug_assert!`, live in every `cargo test` run), and
the truncation `HostBuffer::set` *does* perform is made reachable and tested:
a rendered error is the one thing that can outgrow its buffer, because the
source line it quotes is any length a file can hold, and
`edge_an_error_too_long_for_its_buffer_is_truncated_and_says_so` asserts the
channel fills, keeps the head that names the failure, and ends with the marker
that says it is a fragment.

### MAJOR 5. The read half of the ABI held no lock

`rb_run` held `RUN_LOCK` while it wrote both channels; `rb_output_ptr`,
`rb_output_len`, `rb_error_ptr` and `rb_error_len` held nothing. Two native
tests — `cargo test` runs tests in parallel threads for exactly this reason —
could have one thread's `rb_run` rewrite the bytes another thread was part-way
through reading. The "serialized by `RUN_LOCK`" comment was true of the write
half and false of the read half.

**Fixed twice**, because serialising each call is not serialising a protocol:

- Every entry point now takes the lock, so a read never *overlaps* a write.
- `rb_read_output` and `rb_read_error` are new exports that **copy** the bytes
  out from inside the module with the lock held across the whole copy. The
  address, the length and the bytes were three moments and could be another
  run's by the third; a copy cannot be *torn*.

That was not enough, and the test is what proved it.
`edge_the_output_is_copied_out_under_the_lock_rather_than_read_through_a_pointer`
runs eight threads × twenty-five `rb_run` → `rb_read_output` cycles each, and it
**failed**: `rb_run` and `rb_read_output` are two acquisitions of the lock, and
another thread's run lands between them often enough to be seen in one run of
the suite. Copying under the lock stops a torn read; it does not stop a *stale*
one, because the host asked a moment after it ran.

- So `rb_run_into(ptr, len, out, out_len, err, err_len)` is the third export:
  the run **and** both copies under one lock, with no second moment in it for
  anything to land in. The length arguments are in/out cells — capacity in,
  bytes written out — so a buffer that is too small gets a short copy and the
  full length rather than a silent truncation. `wasm/redblue.js` runs every
  program this way now, and so does the concurrency test.

`edge_the_output_is_copied_out_under_the_lock_rather_than_read_through_a_pointer`
runs those eight threads through `rb_run_into` and asserts each gets back
exactly its own output: 200 reads, each checked against the run that thread just
made. The pointer accessors are still exported — they are the zero-copy path for
a host that is not running two programs at once — and are documented as such
rather than as generally safe.

### MAJOR 6. `run_isolated` ran the program inline when the thread could not be had

The fallback ran `vm.run` on the caller's thread. That thread is not the
`MAX_CALL_DEPTH * STACK_BYTES_PER_CALL` stack the depth counter assumes is
reachable, so exactly when resources were scarce the counter stopped being
backed by stack — turning a catchable `Limit` back into a Rust stack overflow,
which is an abort.

**Fixed** by scoping the inline run to `wasm32-unknown-unknown` and refusing
everywhere else. On wasm there is no thread, `spawn` always fails, and
`build.rs` links the module's single shadow stack from the very same product —
so an inline run there *is* on a stack sized for the whole limit, and the two
builds stop at the same depth. Anywhere else a failed spawn means the stack
could not be had, and the run is refused with an `Error::Limit` naming it.

`run_isolated` is split into `run_isolated_with_depth(program, depth)` so a test
can ask for a depth no thread can carry without mutating the process
environment every other test shares — the same reason
`resolve_max_iterations_from` already existed.
`edge_a_run_that_cannot_be_given_its_stack_is_refused_rather_than_run_
unprotected` asks for 2^40 calls (2^58 bytes of stack), asserts the refusal is
a resource limit, that no output was produced, and that an ordinary depth still
runs.

### MAJOR 7. `files.exists` answered differently for a directory

The in-memory filesystem's `exists` was `contains_key`, so `files.exists(
"modules")` was `no` in the playground and `yes` in the binary — the native
backend is `Path::exists`, which is `true` for a directory — for the same
program, against the byte-identical promise.

**Fixed** by modelling the directory the only way a flat map can: a directory
exists when something is stored underneath it, which is what
`memory::directory_prefix` checks and what the trailing separator is for. So
`/playground` is a directory holding `/playground/note.txt` while
`/playground-note.txt` is a different path that was never written — the edge a
prefix test without the separator gets wrong. Reading a directory is still the
`NotFound` it is on a disk. The one case this cannot reproduce — an *empty*
directory, which nothing in the language creates — is written down at
`memory::exists` rather than left to be found.
`edge_the_in_memory_filesystem_answers_for_directories_too` pins all of it, and
`wasm/check-examples.js` asserts the same three answers against the built
module.

### MINOR 8. The in-memory UTF-8 message had the path in it twice

`std::fs` says `stream did not contain valid UTF-8` and nothing else; the memory
backend appended `: {path}`, and `runtime.rs` prefixes the path itself — so the
same program read the path twice in the playground and once natively.

**Fixed** by emitting exactly `std`'s sentence.
`edge_bytes_that_are_not_text_say_the_same_thing_in_both_builds` asserts the
equality, not a substring, and then asserts the *rendered* native message names
the path exactly once.

### What round 2 cost, and what it did not

- The ABI gained three exports (`rb_read_output`, `rb_read_error`,
  `rb_run_into`) and one changed shape (`rb_release(ptr)`). 15 `rb_*` exports,
  up from 12.
- `run_isolated` gained a sibling that takes the depth explicitly.
- No language behaviour changed. `src/runtime.rs`, the parser, the analyzer and
  the `Value` variants are untouched, and the 6 examples are still byte-identical
  between `rb` and the module.

## Round 1

### BLOCKER

#### 1. `rb_output_ptr` handed out a pointer borrowed from a dropped guard

`output_buffer().as_ptr()` took the address out of a `MutexGuard` and let the
guard drop before returning. The allocation is owned by the `static`, so the
bytes survive — but the lock is released the instant the accessor returns, and
the next writer can move or grow that `Vec`. A host reading through the pointer
is reading memory nothing holds still, which is use-after-free with extra steps.

**Fixed.** Each channel owns one allocation, made at the first run and never
moved or resized: `HostBuffer` holds a `Box<[u8]>` behind a `&'static`, so every
pointer it hands out is derived from that allocation rather than from a
temporary. `set` is the only writer and every caller holds [`run_lock`](../../src/wasm.rs).
*(Round 2 extended this: every reader takes it too.)*

#### 2. `rb_error_ptr`, the same mistake on the other channel

`ERROR_TEXT.lock()...as_ptr()` had exactly the shape finding 1 describes.
**Fixed** the same way — `HostBuffer` is now the only way a channel's bytes are
addressed, and both channels use it.

### MAJOR

#### 3. `bytes.write` bypassed the filesystem shim

`std::fs::write` at `src/runtime.rs` compiles for `wasm32-unknown-unknown`,
fails there at runtime, and passes `cargo test` on the host — so `files.write`
and `bytes.write` pointed at two different filesystems in the one build that has
both. **Fixed:** `vfs::write_bytes` on both backends, `bytes.write` routed
through it, and the memory store now holds `Vec<u8>` rather than `String` so
bytes that are not text round-trip as themselves. `read_to_string` over
non-text bytes answers `InvalidData`, which is what `std::fs` answers.

#### 4. `say("hi")` and `say "hi"` printed to different sinks

The `say`, `console_log` and `console_clear` builtins used `println!`/`print!`
while the statements went through `wasm::emit`, so the byte-identical claim did
not hold for the builtin spellings. **Fixed:** all three go through
`wasm::emit`. `console_error` deliberately still writes to stderr — folding a
diagnostic stream into the output stream would change what native `rb` writes on
each, which is the thing this phase promised not to do.

#### 5. `input`/`ask` blocked on wasm with no refusal

`std::io::stdin().read_line` on `wasm32-unknown-unknown` cannot return; it
blocks the module's only thread, which is a frozen page rather than a failure a
page can show. **Fixed** the same way `network` and `time.sleep` are: a
`read_line` split at the `#[cfg]` boundary, the wasm arm a clean `Runtime`
error. The prompt goes through `wasm::emit` on the native side, so it is
collected with the rest of the output.

#### 6. `import` read modules with `std::fs`

`module_program` could never see the in-memory filesystem on wasm, so a program
that wrote a module and then imported it could not. **Fixed:** through
`crate::vfs::read_to_string`.

#### 7. `rb_alloc` invalidated outstanding pointers

One reused `Vec`; a second `rb_alloc` that needed more room moved the
allocation, and the next `rb_release` could clear a buffer a host was still
holding. `RUN_LOCK` was not held across alloc→run, so two native tests could
interleave. **Fixed:** `HostInput` is an arena. A new allocation appends and
moves nothing, so a pointer stays valid until the host releases that allocation
itself; released buffers are reused, so a host that runs a thousand programs
does not grow the arena a thousand times. *(Round 2: release is now by pointer,
and `rb_alloc` is bounded.)*

#### 8. Partial output was discarded on failure; a panic left capture on

`run_captured` threw away everything it captured when the run failed, so
`rb_run` left the output buffer empty beside a message — a host would have read
that as "this program printed nothing". And with no `catch_unwind`, an unwind
out of the interpreter left `CAPTURING` set, and every later run in the process
would have appended to a capture nobody was reading.

**Fixed.** `run_outcome` returns the bytes and the failure *independently*, the
run is wrapped in `catch_unwind` with the panic payload rendered into the error
channel, and a `CaptureGuard` resets the capture flags however the run ends.
Both channels are written on every path.

What survives a failure is what reached the boundary — the bytes `print` writes
at once. `say` is buffered until the program ends and is lost with the failure,
in the playground and in native `rb` alike; that is the behaviour both builds
have always had and matching it is the point.

#### 9. The recursion test could not pin `MAX_CALL_DEPTH`

`f(100000)` passed for any limit, because `return` inside an `if` does not
return from the function in this language — so `f(3)` already exhausted the
counter. **Fixed without touching the language:** the test now uses the shape
`tests/call_depth_test.rs` already uses (the recursive call is *assigned*, not
returned) and asserts from both sides — `countdown(MAX_CALL_DEPTH - 1)` runs and
prints its value, `countdown(MAX_CALL_DEPTH)` is caught, and the runaway shape
is caught too. A test that cannot fail for the wrong reason is now the same
test that pins the boundary.

#### 10. The wasm-only code paths were never executed by `cargo test`

The in-memory filesystem, `network_refused`, the wasm `sleep_for` and the wasm
`now` were all behind `#[cfg(target_arch = "wasm32")]`, so on the host they were
not compiled at all and no test could reach them.

**Fixed where it can be.** The memory filesystem is now compiled on **every**
target as `vfs::memory`, precisely so `cargo test` can run it:
`the_in_memory_filesystem_behaves_like_the_one_it_replaces` pins the behaviours
a `BTreeMap` gets wrong — a missing file is `NotFound`, not an empty read,
`append` creates the way the disk does, `bytes.write` and `files.read` meet, and
the `NotFound` wording is the sentence `std::fs` uses. The three refusals are
wasm-only and are asserted where they are true, in
`wasm/check-examples.js`, against the built module; `edge_the_platform_builtins_still_answer_natively`
asserts from `cargo test` that the *native* answers are unchanged, which is the
half that is checkable here.

#### 11. The two builds did not size the stack the same way

Native spawned a thread of `limit * STACK_BYTES_PER_CALL` = 256 MiB; wasm
linked a hand-written 16 MiB against a per-frame cost of ~58 KiB. The two
builds could accept different programs, which is the one thing the playground
must not do. **Fixed:** both numbers moved into `build.rs`, which writes them to
`$OUT_DIR/stack_budget.rs` for `src/interpreter.rs` to `include!`, and the
`-zstack-size` is computed from that same product. One copy, so the counter the
VM checks and the stack the linker reserves cannot drift. Verified end to end:
1000-deep recursion is a catchable error under `node`, and the module still runs
afterwards.

#### 12. `len as i32` narrowed, with no bound on output

**Fixed twice over.** Every length crossing the ABI is `i32::try_from(...).unwrap_or(i32::MAX)`,
so it cannot wrap negative and send a host off the front of the buffer; and
`MAX_OUTPUT_BYTES` (4 MiB) is a real bound on what one run may produce, enforced
in `emit`. A program past it is *refused* with a message, not truncated — half an
endless loop is not the program's output, and a host that got it would show a
success it did not get. *(Round 2: the bound is now also charged where `say`
buffers its lines, which is where round 1's version did not reach.)*

## Not changed

The pre-existing `return`-inside-an-`if` behaviour is a language semantics
question, not a wasm one; `tests/call_depth_test.rs` documents it and works
within it. See [`FINDINGS.md`](FINDINGS.md).

## Gates

| Gate | Round 1 | Round 2 |
|---|---|---|
| `cargo fmt --all -- --check` | pass | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings | pass, zero warnings |
| `cargo test --all-targets` | 1250 passed, 0 failed, 0 ignored | **1258 passed, 0 failed, 0 ignored** |
| `cargo clippy --target wasm32-unknown-unknown -- -D warnings` | pass, zero warnings | pass, zero warnings |
| `cargo build --release --target wasm32-unknown-unknown --lib` | pass | pass, **15** `rb_*` exports |
| `node wasm/check-examples.js` | 6 examples byte-identical; refusals and the output bound asserted | 6 examples byte-identical; refusals, both output-limit paths, the source limit and directory `exists` asserted |
| `./rbops/verify.sh phase-043` | **NOT RUN — `rbops/` is not present in this checkout** | **NOT RUN — unchanged** |

Round 1 added 6 tests; round 2 added 8. Round 2's eight are all `edge_*`:
`edge_say_output_is_bounded_while_it_is_still_buffered`,
`edge_an_allocation_past_the_source_limit_is_refused_before_anything_is_allocated`,
`edge_an_allocation_is_released_by_its_own_address_and_only_once`,
`edge_an_error_too_long_for_its_buffer_is_truncated_and_says_so`,
`edge_the_output_is_copied_out_under_the_lock_rather_than_read_through_a_pointer`,
`edge_a_run_that_cannot_be_given_its_stack_is_refused_rather_than_run_unprotected`,
`edge_the_in_memory_filesystem_answers_for_directories_too`,
`edge_bytes_that_are_not_text_say_the_same_thing_in_both_builds`.

`edge_output_past_the_playground_limit_is_reported_not_truncated` was
**rewritten, not deleted**: it now drives `print` so it keeps testing the
boundary `emit` enforces, and its assertion names the limit rather than the word
"playground" — the wording changed because the two output-limit paths were given
one sentence instead of two.

New `#[ignore]` / `// skip` / `allow(clippy::`: 0. Tests deleted: 0. Newly-failing
pre-existing tests: 0.