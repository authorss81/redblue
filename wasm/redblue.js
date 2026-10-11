// redblue.js — the WebAssembly playground's bindings.
//
// The module is the library built with:
//
//   rustup target add wasm32-unknown-unknown
//   cargo build --release --target wasm32-unknown-unknown --lib
//
// which writes `target/wasm32-unknown-unknown/release/redblue.wasm`. It exports
// the `rb_*` C ABI declared in `src/wasm.rs` — pointers into the module's own
// linear memory — and nothing else. There is no `wasm-bindgen`, no generated
// glue and no JavaScript toolchain: this file is the whole binding layer, and
// it runs identically under Node and in a browser.
//
// The contract, in four steps:
//
//   const rb = await Redblue.load(wasmBytes);
//   const result = rb.run('say "Hello"\n');
//   result.ok          // true
//   result.output      // 'Hello\n'  — byte-for-byte what native `rb` prints
//   result.error       // '' when it succeeded
//
// `load` is the only async part. After it resolves, `run` is synchronous: the
// module has no imports and does no I/O of its own.

const WASM_PATH = "../target/wasm32-unknown-unknown/release/redblue.wasm";

export class Redblue {
  /**
   * Instantiates the module and returns the wrapper. `bytes` is the `.wasm`
   * file's contents.
   *
   * `importObject` is optional and only needed if the host wants to override
   * the stubs below — the module's own imports are `wasm-bindgen` shims that
   * `chrono`'s and `reqwest`'s wasm targets pull in, and none of them is
   * reachable from a Redblue program. See `stubImports` for what they are and
   * why they are safe to stub.
   */
  static async load(bytes, importObject = null) {
    const module = await WebAssembly.compile(bytes);
    return new Redblue(
      await WebAssembly.instantiate(module, importObject ?? (await stubImports(module)))
    );
  }

  /** Reads the `.wasm` over HTTP and instantiates it. Browser convenience. */
  static async loadFrom(url = WASM_PATH) {
    const response = await fetch(url);
    if (!response.ok) {
      throw new Error(
        `Cannot load the Redblue module from ${url}: HTTP ${response.status}. ` +
          `Build it with: cargo build --release --target wasm32-unknown-unknown --lib`
      );
    }
    return Redblue.load(await response.arrayBuffer());
  }

  constructor(instance) {
    this.exports = instance.exports;
    this.memory = instance.exports.memory;
  }
}

// The two channel sizes, from `src/wasm.rs`. `rb_run_into` takes a destination
// and the number of bytes it has, and writes back how many it used, so the
// buffers have to be this big before the run rather than after it — and a host
// that had to guess would be guessing at where the run's output ends.
const MAX_OUTPUT_BYTES = 4 * 1024 * 1024; // wasm::MAX_OUTPUT_BYTES
const MAX_ERROR_BYTES = 64 * 1024; // wasm::MAX_ERROR_BYTES

/**
 * Copies `text` into the module's linear memory, runs it, and copies the
 * result back out.
 *
 * The result comes back through `rb_run_into`, which runs the program and copies
 * both channels into buffers this host lent it — in one call, with the module's
 * lock held across all of it. That is the form to use: `rb_run` followed by
 * `rb_read_output` is two calls, and another thread's run landing between them
 * would hand this host another program's bytes. The address accessors
 * (`rb_output_ptr` and friends) are still exported for a host that wants them,
 * but an address taken before the next `run` aims at the *next* run's bytes.
 *
 * Every pointer here points into the module's own memory, which wasm may grow at
 * any allocation — so the bytes are copied into a JavaScript string before this
 * function returns. Holding a pointer instead would be a use-after-free the
 * moment the next `run` allocated.
 */
Redblue.prototype.run = function run(text) {
  const bytes = new TextEncoder().encode(text);
  const len = bytes.length;

  // `wasm32-unknown-unknown` has no clock for a program to read, and
  // `SystemTime::now()` panics there — which would abort the module rather
  // than fail one run. Handing it the host's time before each run is what makes
  // `time.now()` work here. Under Node that is `Date.now()`; in a browser it
  // is the same function on `window`.
  this.exports.rb_set_clock(Date.now() / 1000);

  const input = this.exports.rb_alloc(len);
  if (len > 0 && input === 0) {
    // A zero-length request is answered with a pointer that names nothing, so
    // only a non-empty one that came back null is a refusal. `rb_alloc` leaves
    // its reason in the error channel — the same one a failed run uses — so the
    // page shows the module's own sentence rather than a JavaScript guess.
    throw new Error(readChannel(this, this.exports.rb_read_error));
  }
  if (len > 0) {
    new Uint8Array(this.memory.buffer, input, len).set(bytes);
  }

  // Four buffers and two length cells, all lent by the module and all released
  // below whatever happens.
  const outBuffer = this.exports.rb_alloc(MAX_OUTPUT_BYTES);
  const errBuffer = this.exports.rb_alloc(MAX_ERROR_BYTES);
  const cells = this.exports.rb_alloc(8);
  if (outBuffer === 0 || errBuffer === 0 || cells === 0) {
    throw new Error("The Redblue module could not lend buffers for a run.");
  }
  // The cells are two `i32`s: the capacity in, the bytes written out.
  new Int32Array(this.memory.buffer, cells, 2).set([MAX_OUTPUT_BYTES, MAX_ERROR_BYTES]);

  const status = this.exports.rb_run_into(
    input,
    len,
    outBuffer,
    cells,
    errBuffer,
    cells + 4
  );

  // Both views are taken *after* the run, and that is not tidiness. Running a
  // program grows the module's memory, and wasm growth replaces `memory.buffer`
  // and detaches every view of the old one — a view made before the call reads
  // back `undefined` from an address the run has since moved past. The bytes in
  // linear memory keep their addresses across a grow, so a view taken now is
  // both valid and correct.
  const buffer = this.memory.buffer;
  const lengths = new Int32Array(buffer, cells, 2);
  const decoder = new TextDecoder("utf-8");
  const output = decoder.decode(new Uint8Array(buffer, outBuffer, lengths[0]));
  const error = decoder.decode(new Uint8Array(buffer, errBuffer, lengths[1]));

  for (const pointer of [input, outBuffer, errBuffer, cells]) {
    this.exports.rb_release(pointer);
  }

  return { ok: status === 0, output, error };
};

/** The module's version string, as `rb version` prints it. */
Redblue.prototype.version = function version() {
  return readByAddress(this, this.exports.rb_version_ptr, this.exports.rb_version_len);
};

/** The keyword list, newline-separated, as `rb keywords` prints it. */
Redblue.prototype.keywords = function keywords() {
  return readByAddress(this, this.exports.rb_keywords_ptr, this.exports.rb_keywords_len);
};

/**
 * Builds the import object the module needs.
 *
 * The interpreter itself imports nothing — it reads files, prints and parses in
 * Rust. What does import something is `chrono`, which reaches for `js-sys` on
 * this target, and `reqwest`, which reaches for `wasm-bindgen-futures` and
 * `web-sys`; those crates compile their JS-facing shims in whether or not the
 * program ever calls them. So the module has five imports named
 * `__wbindgen_placeholder__` / `__wbindgen_externref_xform__` and nothing a
 * Redblue program can reach.
 *
 * They are stubbed rather than linked: a stub that throws is the honest answer,
 * because reaching one would mean the interpreter had called out to the browser,
 * and `network.get` is refused at the language boundary long before that. The
 * two table helpers are the exception — `wasm-bindgen` calls them while
 * building an externref table it never then uses, so they are given the
 * behaviour they are named for instead of a throw.
 */
async function stubImports(module) {
  const imports = {};
  for (const { module: moduleName, name, kind } of WebAssembly.Module.imports(module)) {
    imports[moduleName] ??= {};
    imports[moduleName][name] = stubFor(name);
  }
  return imports;
}

const NEVER_CALLED = "Redblue's WebAssembly module called a wasm-bindgen shim";

function stubFor(name) {
  if (name === "__wbindgen_describe") {
    // Reached only by `wasm-bindgen`'s own generated glue, which this module
    // has none of. Answering is not a throw, because it is a pure query.
    return () => undefined;
  }
  if (name.endsWith("_table_grow") || name.endsWith("_table_set_null")) {
    return stubTable;
  }
  return () => {
    throw new Error(NEVER_CALLED);
  };
}

/** Grows an externref table by `delta`, which is 0 for the only caller. */
function stubTable(table, delta) {
  if (table === undefined || delta === 0) {
    return 0;
  }
  return table.grow(delta);
}
/**
 * Reads a channel the module copies out: `readFn(0, -1)` asks how many bytes
 * there are, `rb_alloc` gets a buffer of exactly that many, and `readFn(dest,
 * len)` fills it from inside the module while the module's own lock is held.
 *
 * Used by `run` only for the one case `rb_run_into` cannot reach — reading the
 * reason behind a refused `rb_alloc`, which happens before there is a run to
 * read the result of. A run's own output and error go through `rb_run_into`,
 * which does not have the second call this does.
 *
 * The destination has to be module memory: JavaScript cannot hand the module an
 * address in its own heap, and an address in the *JavaScript* heap is a
 * different thing entirely. So the module lends the host a buffer, which is
 * what `rb_alloc` is for.
 */
function readChannel(rb, readFn) {
  const len = readFn.call(rb.exports, 0, -1);
  if (len <= 0) {
    return "";
  }
  const scratch = rb.exports.rb_alloc(len);
  if (scratch === 0) {
    throw new Error("The Redblue module could not lend a buffer to read a message into.");
  }
  const copied = readFn.call(rb.exports, scratch, len);
  rb.exports.rb_release(scratch);
  // The view is taken after the allocation, which may have grown the memory and
  // detached any earlier `ArrayBuffer` view of it.
  const into = new Uint8Array(rb.memory.buffer, scratch, copied);
  return new TextDecoder("utf-8").decode(into);
}

/**
 * Reads a channel the module only ever exposes as an address — the version and
 * the keyword list, which are built once and never written again, so there is
 * nothing a later run could do to them.
 */
function readByAddress(rb, ptrFn, lenFn) {
  const len = lenFn.call(rb.exports);
  if (len <= 0) {
    return "";
  }
  const ptr = ptrFn.call(rb.exports);
  return new TextDecoder("utf-8").decode(new Uint8Array(rb.memory.buffer, ptr, len));
}

export default Redblue;