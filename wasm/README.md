# The Redblue playground

The interpreter, compiled to WebAssembly, behind a page and a Node script.
Nothing about the language changes: the same lexer, parser, analyzer and VM that
`rb` runs natively, compiled to a different target.

## Build

```bash
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown --lib
```

writes `target/wasm32-unknown-unknown/release/redblue.wasm`. The `cdylib` crate
type is what makes this a module rather than an `.rlib`, and it is what exports
the `rb_*` symbols the bindings call.

## Run it

The page, in any browser. Serve the repository root over HTTP and open
`wasm/index.html` — `file://` will not load the module, because `fetch` of a
local file is blocked by the browser.

```bash
python3 -m http.server 8000
# then open http://localhost:8000/wasm/index.html
```

Or drive the module from Node, which is what the check below does:

```js
import { readFileSync } from "node:fs";
import { Redblue } from "./wasm/redblue.js";

const rb = await Redblue.load(
  readFileSync("target/wasm32-unknown-unknown/release/redblue.wasm")
);
console.log(rb.run('say "Hello"\n').output); // Hello\n
```

## Prove it is the same language

`check-examples.js` runs every example in the corpus through the native `rb`
binary and through the module, and requires the two byte streams to be
identical:

```bash
cargo build --release --bin rb
cargo build --release --target wasm32-unknown-unknown --lib
node wasm/check-examples.js
```

```
ok    hello.rb  (43 bytes identical)  — say, variables and a function call
ok    fizzbuzz.rb  (68 bytes identical)  — repeat, nested if and for-each
ok    formats.rb  (151 bytes identical)  — json.parse / json.stringify / records
ok    test_arithmetic.rb  (45 bytes identical)  — arithmetic and string formatting
ok    random.rb  (182 bytes identical)  — the seeded random builtins
ok    files.rb  (160 bytes identical)  — the files module, through the interpreter

6 examples, byte-identical under node and native rb.
```

`examples/time.rb` is deliberately not in the corpus: it prints the wall clock,
so it has no fixed bytes for two pipelines to agree on.

The Rust side of the same contract — the output bytes, the `say`/`print`
ordering, the rendered errors, and the pointer arithmetic of the ABI itself —
is in `tests/wasm_test.rs` and runs in `cargo test` without a `.wasm` file.

## What differs from native `rb`, and why

Everything here is at the I/O boundary. No `#[cfg]` changes how a program is
parsed, analyzed or executed.

| | native `rb` | playground |
|---|---|---|
| **Program output** | written to stdout as the program runs | collected into a buffer the host reads by pointer (`src/wasm.rs`) |
| **`files.*` / `bytes.write`** | `std::fs`, against the disk | an in-memory filesystem (`src/vfs.rs`): flat path to bytes, gone when the page closes |
| **module `import`** | `std::fs`, against the disk | the same in-memory filesystem, so a program can write a module and then import it |
| **`network.get` / `network.post`** | `reqwest`'s blocking client | a clean `Runtime` error naming what is missing |
| **`input` / `ask`** | `std::io::stdin` | a clean `Runtime` error: there is no terminal to read from |
| **`time.sleep`** | `std::thread::sleep` | a clean `Runtime` error: the module has one thread and blocking it would freeze the page |
| **`time.now()`** | `SystemTime::now()` | the host's clock, handed in through `rb_set_clock` before each run |
| **output past 4 MiB in one run** | never reached — the terminal is the limit | refused with a `Limit` message rather than truncated |
| **panic in the interpreter** | the process aborts | caught and reported; the module still runs afterwards |

The refusals are at the transport, not changes to what a program *means*. The
`files` case is a real substitution — the same program produces the same output
against either filesystem, which is what `files.rb` in the corpus demonstrates.

Every path into the filesystem goes through `src/vfs.rs`. A `std::fs` call left
inside a builtin compiles for `wasm32-unknown-unknown`, fails there at runtime,
and passes `cargo test` on the host — so it is a bug that only the one
environment nobody tests in would ever show.

## The stack

`wasm32-unknown-unknown` defaults to a 1 MiB shadow stack, and `MAX_CALL_DEPTH`
is 1000 nested Redblue calls. Natively the interpreter gets the room for that
from `STACK_BYTES_PER_CALL` in a thread it spawns per run; on wasm there is no
thread, so the same depth has to fit in the one stack. At the default it does
not — a Redblue function that called another trapped with "memory access out of
bounds", taking the whole module instance with it.

`build.rs` links the playground with a stack sized from the *same product* the
native thread is sized from: `MAX_CALL_DEPTH * STACK_BYTES_PER_CALL`. Both
numbers live in `build.rs` and `src/interpreter.rs` includes them from there, so
the counter the VM checks and the stack the linker reserves cannot drift apart —
if they did, the playground would trap on a program the native binary reports a
catchable `Limit` for. It is a *link* argument, not a limit anything reads, and
the two builds then accept and refuse the same programs, down to the same
call-depth error.

## The bindings

`redblue.js` is the whole binding layer: no `wasm-bindgen`, no generated glue,
no JavaScript toolchain. The module exports a plain C ABI over linear memory
and the file is 15 exported functions' worth of pointer arithmetic.

| Export | Meaning |
|---|---|
| `rb_alloc(len) -> ptr` | reserve `len` bytes for the host to write source into, or to lend a run its result buffers; null past `MAX_SOURCE_BYTES` |
| `rb_release(ptr) -> i32` | release that buffer by its own address; `0` released, `-1` nothing to release |
| `rb_run_into(ptr, len, out, out_len, err, err_len) -> i32` | run and copy both channels out, under one lock; the two length cells are the capacity in and the bytes written out |
| `rb_run(ptr, len) -> i32` | run without copying; `0` ok, `1` failed, `-1` refused |
| `rb_read_output(dest, cap) -> i32` | copy the output out; `cap < 0` asks the length only |
| `rb_read_error(dest, cap) -> i32` | the same for the rendered error, empty on success |
| `rb_output_ptr()` / `rb_output_len()` | the output's address and length, without the copy |
| `rb_error_ptr()` / `rb_error_len()` | the rendered error's address and length |
| `rb_version_ptr()` / `rb_version_len()` | the crate version |
| `rb_keywords_ptr()` / `rb_keywords_len()` | the keyword list |
| `rb_set_clock(seconds)` | the host's wall clock, so `time.now()` has something to read |

Three rules hold a pointer across this boundary, and each exists because breaking
it is use-after-free or a torn read rather than a wrong answer:

- **The pointer a host holds stays put.** `rb_output_ptr` and `rb_error_ptr`
  point into one allocation per channel, made at the first run and never moved
  or resized — not into a `Vec` behind a `Mutex`, whose address the next writer
  can move and whose lock is released the moment the accessor returns. Same for
  `rb_alloc`: allocations go into an arena rather than one reused buffer, so a
  second, larger program cannot move the first one's buffer out from under it.
- **Every entry point takes the same lock**, so a read never *overlaps* a write.
  Serialising each call is not the same as serialising a protocol: `rb_run` and
  then `rb_read_output` is two acquisitions, and another host's run can land
  between them. That is what `rb_run_into` is for — the run and both copies
  under one lock, with no second moment in it — and it is the form
  `redblue.js` uses. `rb_read_output` and `rb_read_error` stop a read from being
  *torn* for a host that is not running two programs at once.
- **The length is read from the module, never guessed.** `rb_output_len` is the
  `i32` the host should read from the pointer beside it, clamped rather than
  narrowed, so a value can never wrap negative and send a host off the front of
  the buffer. Output past `MAX_OUTPUT_BYTES` is refused with a message rather
  than truncated — and that bound is charged where `say` *buffers* a line as
  well as where the bytes are written, so a program whose lines are still
  waiting cannot outgrow the memory the bound is there to bound. `rb_alloc`
  refuses a source past `MAX_SOURCE_BYTES` for the same reason, before it
  allocates anything.

`rb_alloc(0)` is answered with a pointer that names nothing and
`rb_release` on it is `-1`: a zero-length reservation holds no bytes, and a
`Box` of no bytes has a dangling address that two of them would share.

The module also imports five `wasm-bindgen` shims, from `chrono`'s and
`reqwest`'s own wasm targets. Nothing a Redblue program can reach uses them;
`Redblue.load` stubs them, and the stubs that throw would only fire if the
interpreter had called out to the browser.

## Files

| File | What |
|---|---|
| `../src/wasm.rs` | the output boundary, `run_program`, and the exported ABI |
| `../src/vfs.rs` | `std::fs` natively, an in-memory filesystem on wasm — compiled on every target so `cargo test` can test it |
| `redblue.js` | the bindings |
| `index.html` | the runner page |
| `check-examples.js` | the byte-identical-output proof, and the wasm-only refusals |
| `../tests/wasm_test.rs` | the same contract, asserted in `cargo test` |