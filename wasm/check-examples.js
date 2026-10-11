#!/usr/bin/env node
// check-examples.js — the playground's proof that it is the same language.
//
// Runs every example in the corpus twice: once through the native `rb` binary,
// and once through the WebAssembly module loaded from `redblue.wasm`. The two
// byte streams must be identical, byte for byte, or this exits non-zero.
//
//   node wasm/check-examples.js
//
// `examples/time.rb` is not in the corpus: it prints the wall clock, so it has
// no fixed bytes for the two pipelines to agree on.

import { mkdirSync, readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";

import { Redblue } from "./redblue.js";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "..");
const wasmPath = join(
  root,
  "target/wasm32-unknown-unknown/release/redblue.wasm"
);
const rbPath = join(root, "target/release/rb");

// The corpus, and the reason each program is in it. Kept in step with
// `tests/wasm_test.rs`, which fails if `examples/` drifts.
const CORPUS = [
  ["hello.rb", "say, variables and a function call"],
  ["fizzbuzz.rb", "repeat, nested if and for-each"],
  ["formats.rb", "json.parse / json.stringify / records"],
  ["test_arithmetic.rb", "arithmetic and string formatting"],
  ["random.rb", "the seeded random builtins"],
  ["files.rb", "the files module, through the interpreter"],
];

const bytes = readFileSync(wasmPath);
const rb = await Redblue.load(bytes);

assert.equal(rb.version(), "0.2.0", "the module must report the crate version");

let failures = 0;
for (const [name, why] of CORPUS) {
  const source = readFileSync(join(root, "examples", name), "utf8");

  const native = nativeRun(join(root, "examples", name));

  const throughWasm = rb.run(source);
  assert.ok(
    throughWasm.ok,
    `${name}: the module refused the program: ${throughWasm.error}`
  );

  if (throughWasm.output === native) {
    console.log(`ok    ${name}  (${native.length} bytes identical)  — ${why}`);
  } else {
    failures += 1;
    console.error(`FAIL  ${name}`);
    console.error(`  native : ${JSON.stringify(native)}`);
    console.error(`  wasm   : ${JSON.stringify(throughWasm.output)}`);
  }
}

// A failing program is a finding of its own: the playground has to report the
// interpreter's own error, not swallow it or produce a JS exception.
const broken = rb.run('say "unterminated\n');
assert.equal(broken.ok, false, "a malformed program must not report success");
assert.match(broken.error, /Unterminated string/);

// The refusals. On `wasm32-unknown-unknown` these three have no platform to work
// with — a thread for the network client, a second thread to sleep on, a
// terminal to read from — and each one used to abort or hang the whole module
// instance instead of failing. A trap here would be a TypeError, so reaching
// this line at all is half the assertion. These cannot be asserted from
// `cargo test`, which runs natively and where all three genuinely work.
for (const [what, source] of [
  ["network", 'say network.get("https://example.test")'],
  ["time.sleep", 'time.sleep(0.01)\nsay "unreachable"'],
  ["input", 'say input("what is your name? ")'],
]) {
  const result = rb.run(source);
  assert.equal(result.ok, false, `${what} must be refused, not silently allowed`);
  assert.ok(result.error.length > 0, `${what} must say why it was refused`);
  assert.match(result.error, new RegExp(what.split(".")[0]), `${what} must name itself`);
  assert.equal(result.output, "", `${what} must print nothing before refusing`);
}

// The playground has to stop somewhere, and it has to say so. A program past
// the output limit is a failure rather than a truncated "success" — and there
// are two paths to it, so both are checked. `say` buffers its lines until the
// program ends, so that path is charged while the lines are still buffered and
// the program can catch the limit itself; `print` writes at once, so that one is
// refused by the boundary.
const floods = [
  [
    "say",
    'repeat 200000 times\n    say "a line long enough to add up"\nend\n',
    /output past the limit of 4194304 bytes/i,
  ],
  [
    "print",
    'repeat 200000 times\n    print "a line long enough to add up"\nend\n',
    /output past the limit of 4194304 bytes/i,
  ],
];
for (const [what, source, expected] of floods) {
  const flooded = rb.run(source);
  assert.equal(flooded.ok, false, `a ${what} flood must fail`);
  assert.match(flooded.error, expected);
  assert.equal(flooded.output, "", "a refused flood must not leave a partial run behind");
}

// The same limit from inside the program: it is the interpreter's own, so a
// `catch` can take it and the run still succeeds. This is the difference
// between the limit being charged where `say` buffers its lines and it only
// being charged where the bytes are finally written — a program whose lines are
// still waiting would grow without bound.
const FLOOD_LINE = "a line long enough that a million of them add up to a great deal";
const perLine = FLOOD_LINE.length + 1; // the newline `say` adds on the way out
const fills = Math.floor(4194304 / perLine);
const caughtFlood = rb.run(
  "try\n" +
    "    repeat 1000000 times\n" +
    `        say "${FLOOD_LINE}"\n` +
    "    end\n" +
    "catch error\n" +
    "    set caught to 1\n" +
    "end\n"
);
assert.equal(caughtFlood.ok, true, `a caught limit must not fail the run: ${caughtFlood.error}`);
// Exactly the lines that fitted, charged per line as each was buffered. Without
// the charge the loop would finish and this would be 65 MB, not 4.
assert.equal(
  caughtFlood.output.length,
  fills * perLine,
  "the buffered lines must be exactly the ones the limit allowed"
);
// And the charge sticks: a `catch` takes the limit, and then nothing can be
// said at all. Letting a shorter line through into whatever room the last long
// line happened to leave would make what a program can still print depend on
// how its earlier lines divided the limit.
const afterFlood = rb.run(
  "try\n" +
    "    repeat 1000000 times\n" +
    `        say "${FLOOD_LINE}"\n` +
    "    end\n" +
    "catch error\n" +
    "end\n" +
    'say "after"\n'
);
assert.equal(afterFlood.ok, false, "nothing may be said once the limit has stopped a say");
assert.match(afterFlood.error, /4194304/);

// `rb_alloc` refuses a source past its own limit rather than allocating it, and
// the reason comes back through the channel a page already shows failures in.
assert.throws(
  () => rb.run("say \"x\"\n#".padEnd(4194304 + 16, "#")),
  /4194304/,
  "an allocation past the source limit must be refused with the number"
);

// `files.exists` answers for a directory in this build as well as on a disk,
// which is the one place the in-memory filesystem has to model something the
// map does not store.
rb.run('files.write("rb-modules/note.txt", "x")');
assert.equal(rb.run('say files.exists("rb-modules")').output, "yes\n");
assert.equal(rb.run('say files.exists("rb-modules/note.txt")').output, "yes\n");
assert.equal(rb.run('say files.exists("rb-mod")').output, "no\n");

// `time.now()` is the opposite case: the platform has no clock, and it used to
// panic there — aborting the module — but `Redblue.run` hands it the host's, so
// it works. Without that the two builds would not agree on `time.now()` at all.
const clocked = rb.run("say time.now().seconds > 0");
assert.equal(clocked.ok, true, `time.now() must read the host clock: ${clocked.error}`);
assert.equal(clocked.output, "yes\n");

// The module must survive all of the above and still run.
assert.equal(rb.run('say "alive"').output, "alive\n");

if (failures > 0) {
  console.error(`\n${failures} example(s) differ between native rb and the wasm module.`);
  process.exit(1);
}
console.log(`\n${CORPUS.length} examples, byte-identical under node and native rb.`);

/** Runs the native binary and returns stdout as a UTF-8 string. */
function nativeRun(path) {
  // `files.rb` writes into the working directory, so it needs one of its own.
  // Created rather than assumed: a fresh checkout has no `target/tmp` at all.
  const cwd = join(root, "target/tmp/wasm-examples");
  mkdirSync(cwd, { recursive: true });

  return execFileSync(rbPath, ["run", path], {
    encoding: "utf8",
    cwd,
    maxBuffer: 64 * 1024 * 1024,
    stdio: ["ignore", "pipe", "inherit"],
  });
}