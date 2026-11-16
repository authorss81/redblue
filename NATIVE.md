# A native backend for Redblue: feasibility study

**Status:** research only. This document answers a question; it does not
implement a backend, add a crate, change the bytecode format, or authorise OS or
GPU work. The diff for this phase adds tests and this file. It does not touch
`Cargo.toml`.

## Recommendation

**Defer it.**

Build the native backend after the bytecode VM has slot-resolved names and some
form of type or shape feedback, and use Cranelift for the first implementation.
Do not build an LLVM AOT backend now, and do not describe the native backend as
the next step merely because it is the only remaining way to improve speed.

The deferral is not a rejection. The existing pipeline has already reached the
point where a backend is technically possible: the tree-walking frontend,
bytecode compiler, stable file format, disassembler, bytecode VM and
self-hosted fixed point all exist. What is missing is not a code generator
framework. It is the semantic layer a code generator needs to produce code that
is actually faster than the interpreter: resolved variables, closure layout,
object shape feedback, and a runtime ABI that preserves Redblue's visible
behaviour. Building machine-code emission before those exists would produce a
faster instruction decoder attached to the same dynamic hash lookups.

The concrete trigger for reversing the deferral is in
[When the recommendation would be wrong](#when-the-recommendation-would-be-wrong).

## What already exists

The native backend would not start from an empty compiler. These are the
components it would consume, with the reason each is or is not reusable.

| Component | Where | Reusable for a native backend? |
|---|---|---|
| Lexer, parser, analyzer | `src/lib.rs:114` (`run_source_in`), `src/bytecode/format.rs:275` (`Chunk::compile`) | **Yes, unchanged.** A native backend can consume the same AST or the same checked program. |
| AST-to-bytecode lowering | `src/bytecode/codegen.rs:32` (`compile`) | **Yes as the first IR**, with a caveat: it deliberately emits names, not slots (`src/bytecode/codegen.rs:7`). A backend needs a later name-resolution pass. |
| Opcode set | `src/bytecode/opcode.rs:60` (`Opcode`), `src/bytecode/opcode.rs:294` (`from_byte`), `src/bytecode/opcode.rs:305` (`name`) | **Yes.** The opcode bytes are stable and documented; a backend can translate the same instruction stream. |
| Chunk, block, instruction and constant types | `src/bytecode/format.rs:244` (`Chunk`), `src/bytecode/format.rs:222` (`Block`), `src/bytecode/format.rs:158` (`Instruction`), `src/bytecode/format.rs:111` (`Constant`) | **Yes.** This is the in-memory IR a backend would translate. |
| Encoder and decoder | `src/bytecode/format.rs:290` (`encode`), `src/bytecode/format.rs:308` (`decode`), version at `src/bytecode/format.rs:81` | **Yes, but should be left alone for the first backend.** The decoder is strict about the version and refuses anything it does not know. |
| Disassembler | `src/bytecode/disasm.rs:102` (`render`), `src/bytecode/disasm.rs:35` (`render_compact`) | **Yes.** It is a pure function of a chunk and is useful for differential testing the backend's input. |
| Bytecode VM | `src/bytecode/vm.rs:685` (`BytecodeVm`), `src/bytecode/vm.rs:1094` (`run`), `src/bytecode/vm.rs:3427` (`call_function`) | **Yes as the semantic oracle, not as the code generator.** The native backend must agree with it instruction for instruction and error for error. |
| Tree-walking VM | `src/interpreter.rs:333` (`Vm`), `src/interpreter.rs:1695` (`run`), `src/interpreter.rs:1786` (captured locals) | **Yes as the second oracle.** It is the language's reference behaviour. |
| Runtime operations | `src/runtime.rs:199` (`binary_op`), `src/runtime.rs:146` (`index`), `src/runtime.rs:87` (`property`), `src/runtime.rs:3251` (`builtin`) | **Partly.** Native code can call these through a runtime ABI, but calling one per operation is the slow path the backend is supposed to remove. |
| Value model | `src/value.rs:387` (`Value`), `src/value.rs:21` (`Fields`), `src/value.rs:37` (`Captured`), `src/value.rs:48` (`FunctionBody`), `src/value.rs:85` (`FunctionValue`), `src/value.rs:470` (`Shared<T> = Arc<T>`) | **Only with a runtime contract.** Dynamic tags, `f64` numbers, shared lists/records, captured scope maps and function bodies all need a native representation or a call into Rust. |
| Standard library | `src/stdlib.rs:520` (`builtins`) | **Yes through the same runtime boundary.** Reimplementing it in native code would duplicate behaviour and drift. |
| Self-hosted compiler and fixed point | `docs/BOOTSTRAP.md:16` (S3), `docs/BOOTSTRAP.md:17` (S4) | **Yes as the acceptance corpus.** A native backend that runs `stage1.rbc` must still produce the same `stage2.rbc` bytes. |
| CLI commands | `src/lib.rs:331` (`compile`), `src/lib.rs:337` (`dis`), `src/lib.rs:343` (`vm`), `src/lib.rs:608` (`compile_command`), `src/lib.rs:622` (`dis_command`), `src/lib.rs:641` (`vm_command`) | **Yes as the dispatch points** for a new backend command. They do not need to change for a prototype. |

The important consequence: the backend does **not** need a new parser, a new
bytecode format, or a new standard library. It needs a lowerer from the existing
stack bytecode to a control-flow graph, a runtime ABI, and enough analysis to
make the generated code cheaper than the interpreter's dispatch plus hash
lookups.

## What a backend would have to add

A realistic first backend has this pipeline:

1. **Read the existing IR.** Decode `.rbc` version 6 (`src/bytecode/format.rs:81`)
   or take a `Chunk` in memory.
2. **Build a control-flow graph.** Bytecode jumps are block-local
   (`docs/BYTECODE.md` block section), but a function is still a block tree and a
   call is still a frame push. The backend must turn forward/backward jumps,
   loop sites, `try`/`catch`/`finally`, and `might fail` guards into basic blocks
   and edges.
3. **Lower the operand stack to SSA or virtual registers.** Every opcode has a
   documented stack effect; the existing VM interprets it with an operand stack
   (`src/bytecode/vm.rs:1094`). A backend needs stack-to-SSA conversion and
   liveness.
4. **Resolve names, or accept hash lookups.** The compiler emits names on
   purpose (`src/bytecode/codegen.rs:7`). The VM resolves them against captured
   scopes and object fields at run time. A backend that does not add slot
   resolution will emit a native `LOAD` that still calls a hash table.
5. **Define a runtime ABI.** Allocation of lists, records and text; `Arc`
   retain/release or an equivalent; function values and captured scopes;
   exceptions/failure unwinding; the step, iteration and call-depth budgets;
   and every builtin in `src/stdlib.rs:520`.
6. **Preserve observable semantics.** Same output, same error kind, same message,
   same line number, same step-budget point, same catchability, same record order,
   same Unicode handling.
7. **Add an execution command and a differential test.** The cheapest proof is
   that the native path and `src/bytecode/vm.rs:1094` produce identical output
   and identical failures over `corpus/` (370 programs as of this study),
   `examples/` (7 programs), `modules/`, and `bootstrap/compiler.rb`.

## Obstacles, stated as costs

These are the real costs of a backend, not concerns to be acknowledged and then
ignored.

### 1. The value model is dynamic and boxed

`Value` is a Rust enum with twelve variants (`src/value.rs:387`), numbers are
`f64`, text is `String`, and lists/records are `Arc`-shared
(`src/value.rs:470`). There is no static type for a local and no unboxed integer
type. Machine code that performs `2 + 3` must still decide at run time whether
the operands are numbers, text, lists, `nothing`, an object, or a builtin.

**Cost:** every arithmetic, comparison, property read and call needs a tag check
and a slow-path call unless the backend also adds type inference, type feedback,
or specialized fast paths. That is a separate analysis project. Without it, the
backend removes interpreter dispatch but keeps most of the dynamic checks, which
bounds the gain.

### 2. Names are not slots, and closures capture whole scopes

The bytecode deliberately carries names (`src/bytecode/codegen.rs:7`). A
function value captures a `Vec<IndexMap<String, Value>>` of whole lexical scopes
(`src/value.rs:37`, `src/value.rs:85`), and the tree-walker clones the live locals when it
declares a function (`src/interpreter.rs:1786`). The VM resolves a `LOAD` by
name against those maps.

**Cost:** a native backend either keeps a hash lookup per variable access, which
is slower than the VM's own map lookup only by a constant, or it adds a
slot-resolution pass and a closure-environment layout. The latter is a new
compiler stage and a `.rbc` change or a side table. It also has to preserve
shadowing, captured-by-value copying and the copy-on-write list semantics the
language documents.

### 3. The IR is stack bytecode, not SSA, and jumps are block-local

The VM is a stack machine with explicit frames, operand stacks, loop sites and
handler stacks (`src/bytecode/vm.rs:685`, `src/bytecode/vm.rs:1094`). Bytecode instructions carry
jump targets that are valid only inside their own block.

**Cost:** the backend needs a real CFG builder, stack-height analysis, SSA
construction, phi nodes at loop headers, and landing pads for `try`, `finally`
and `might fail`. The existing block-local jump rule makes this tractable, but it
is still the largest single piece of compiler work and the place where subtle
control-flow bugs will live.

### 4. The runtime is Rust and the boundary is undefined

Arithmetic, indexing, property access and builtins live in Rust
(`src/runtime.rs:199`, `src/runtime.rs:146`, `src/runtime.rs:87`, `src/runtime.rs:3251`), and the standard library is a
large `HashMap` of builtins (`src/stdlib.rs:520`). A native program cannot call
those functions directly without an ABI: argument passing, ownership, error
representation, panic boundaries, and the thread or stack on which the runtime
runs all have to be specified.

**Cost:** a runtime ABI crate or module, plus either a stable C representation of
`Value` or an opaque handle world. Every crossing has overhead, so the backend
must keep hot values on the native side and cross only for allocation and
builtins. That is where the design effort goes, not into instruction selection.

### 5. Observable semantics are unusually strict

Redblue promises exact error kinds and messages, catchable runtime failures,
`finally` ordering, `might fail`, record insertion order, deterministic
disassembly, and resource limits. The bytecode VM already charges one step per
statement (`src/bytecode/vm.rs:1304`) so both engines stop at the same point.
The self-hosted fixed point is byte-for-byte (`docs/BOOTSTRAP.md:16`).

**Cost:** the native backend must instrument statement boundaries or prove it
has the same limits without instrumenting them. It must preserve failure
unwinding through native frames, and it must not change any message. A backend
that is fast only when it ignores the limits is not a Redblue backend. The
differential corpus has to be large enough to catch the difference.

### 6. The no-shimming rule reaches the backend

`docs/BOOTSTRAP.md:87` says there is no Rust fast path a Redblue program silently
falls back to. The shipped compiler is already produced by the self-hosted
compiler. A native backend written only in Rust and reachable only from Rust
would be a new Rust-only fast path.

**Cost:** either the backend's code generator is eventually written in Redblue
and reached through the same fixed-point discipline, or it is explicitly a
stage-0 host tool whose output is checked against the Redblue path. The first is
another self-hosting project; the second is an honest staging decision that must
be written down and tested, not assumed.

### 7. The `.rbc` format is a compatibility contract

`FORMAT_VERSION` is 6 (`src/bytecode/format.rs:81`). The decoder refuses any
other version (`src/bytecode/format.rs:308`), and the format rules say a byte
value already written never changes while any semantic change bumps the version
(`docs/BYTECODE.md:22`). `rb dis` prints the version in its banner
(`src/bytecode/disasm.rs:107`).

**Cost:** a first backend should consume version 6 unchanged. If it needs slot
annotations, type feedback, a different closure layout, or new opcodes, that is a
new format version, a decoder change, a `docs/BYTECODE.md` change, a
disassembler change, and new tests that old files are still refused or still
read as the rules say. Keeping native-only metadata in a side table avoids that
cost but adds a second source of truth.

### 8. Build and platform cost is not zero

`Cargo.toml:38` already carries a substantial dependency set (`regex`, `chrono`,
`reqwest`, `indexmap`, `rustyline`, `ndarray`, `libc` and others), and builds
are expected to be green on Linux, Windows, macOS and WebAssembly. A code
generator adds either a system toolchain or a large pure-Rust dependency tree.

**Cost:** see the comparison below. The backend also has to decide what it
targets: a JIT inside `rb` (same platforms as the interpreter), an AOT object
file writer (needs linkers and platform object formats), or both. Each target
adds CI coverage and a failure mode the current test suite does not have.

### 9. Performance work is not the same as correctness work

The existing project already has a performance protocol: measure on the same
machine, verify byte-identical bytecode or identical answers, and record the
result in `docs/PERFORMANCE_HISTORY.md`. A native backend inherits that protocol
and adds a new baseline: the interpreter.

**Cost:** every optimization claim needs a differential run over the corpus and a
wall-clock comparison. The project must maintain two execution engines until the
native one is trusted. That is a permanent two-engine test burden, not a
one-time build cost.

## LLVM versus Cranelift

Both are real options. They are not interchangeable, and the difference is
mostly dependency and build cost rather than language semantics.

| Question | LLVM (for example `inkwell` + `llvm-sys`) | Cranelift (`cranelift-codegen`, `cranelift-module`, `cranelift-jit`) |
|---|---|---|
| Language | C++ LLVM with Rust bindings | Pure Rust |
| System dependency | A matching LLVM installation and `llvm-config`; the version has to match the binding exactly | None beyond the Rust toolchain |
| CI cost | Install and cache LLVM in every job that builds the backend; the release and cross-target jobs need it too | Ordinary `cargo` build; no system package step |
| Cold build time | Dominated by building/linking LLVM; commonly minutes, and the first build in a clean CI image is the slowest | Adds a Rust dependency graph; materially less than a system LLVM, but still a noticeable increase in a clean build |
| Binary size | Grows by a large amount of LLVM code, especially in a statically linked release | Smaller increase; still non-trivial because the code generator and its support crates are linked in |
| Optimisation quality | Mature, broad, aggressive; best chance at the highest peak speed | Good baseline code; fewer and less aggressive optimisations than LLVM |
| Targets | Very broad, including many back ends and object formats | x86-64 and AArch64 are the practical first targets |
| JIT | Possible but heavy to embed and manage | Designed for embedding and JIT use |
| AOT object/executable | Natural fit | Possible, but the object/linking story is more manual |
| Risk for Redblue | Toolchain drift, platform packaging, cross-compilation, contributors who cannot build the backend locally | Less mature optimiser, smaller target list, but a much smaller operational surface |
| Best first use | A later AOT backend after the IR and semantics are stable | A baseline JIT or an AOT backend for numeric/loop-heavy code |

**Estimate, not a measurement:** Cranelift is the lower-risk first dependency
because it can be added and built with the existing Rust toolchain. LLVM's
optimisation ceiling is higher, but paying its system-toolchain cost before the
IR has slot resolution and type feedback would be optimising the wrong layer.
No crate is added by this phase, so none of these costs are incurred yet.

## Effect on `.rbc` version 6 and `rb dis`

A first backend should **read version 6 unchanged**. That has three consequences:

1. **No format bump is required for execution.** The backend consumes the same
   `Chunk`, `Block`, `Instruction` and `Constant` values the VM consumes
   (`src/bytecode/format.rs:244`, `src/bytecode/format.rs:222`, `src/bytecode/format.rs:158`, `src/bytecode/format.rs:111`). `FORMAT_VERSION`
   stays 6 (`src/bytecode/format.rs:81`).
2. **`rb dis` is unaffected.** It is a pure function of a chunk and its banner
   prints the format version (`src/bytecode/disasm.rs:102`, `src/bytecode/disasm.rs:107`). If the
   backend adds no opcodes and no operand meanings, the disassembly of an
   existing file stays byte-identical.
3. **The self-hosted fixed point is the backend's acceptance test.** Running
   `stage1.rbc` natively must still write the same `stage2.rbc` bytes as running
   it on the bytecode VM. A backend that changes the compiler's output has
   changed the language or the format, not merely the execution engine.

If a later backend needs slot annotations or type feedback, that is a new
version: bump `FORMAT_VERSION`, update the encoder/decoder, update
`docs/BYTECODE.md`, update the disassembler banner and any golden output, and
keep the rule that an older file is refused rather than half-read. Storing that
metadata outside `.rbc` avoids the bump but creates a cache-invalidation and
reproducibility problem. Neither choice is free.

## Expected speedup

The honest range is **about 1.0x to 8x over the current bytecode interpreter on
the existing benchmark suite, with a realistic middle of roughly 2x to 4x**.
The upper end is not general-purpose; it is what a numeric loop can gain after
the dynamic checks are removed. The lower end is what string-, record-, I/O- and
builtin-heavy code can gain.

Reasoning that bounds the range:

- **Interpreter dispatch is only part of the cost.** The bytecode VM already
  avoids AST walking and uses a Rust runtime. A backend removes instruction
  dispatch and operand-stack traffic, but it still has to allocate values, look
  up names, check tags, call builtins and preserve failures. The dispatch saving
  alone is a constant factor, not an order of magnitude.
- **Dynamic values dominate until specialized.** `Value` is an enum of twelve
  variants and numbers are `f64` (`src/value.rs:387`). Without type feedback,
  unboxing or shape checks, native code still performs dynamic tests. The
  performance history already separates these: the roadmap's target of at least
  15x at phase 178 (`docs/PERFORMANCE_HISTORY.md:28`) is for the full
  monomorphic/JIT stack, not for a baseline code generator.
- **The project's own comparison table bounds the ambition.** It describes
  Redblue as about 15x to 30x slower than native Rust on pure compute loops
  (`docs/FUTURE_ROADMAP_AND_DEFERRED.md:112`), and it places LuaJIT at about
  1.5x to 3x of native Rust (`docs/FUTURE_ROADMAP_AND_DEFERRED.md:115`). A
  competent baseline JIT closing the interpreter gap is therefore a single-digit
  multiple, not a 15x to 30x multiplier, until the unboxing and inline-cache
  work lands.
- **Allocation-heavy code regresses easily.** Lists and records are `Arc`-shared
  (`src/value.rs:470`). Native code that calls into Rust for every allocation and
  every property access can be slower than the VM's tight, cache-friendly loop.
  A backend without an allocation strategy can lose on `concat`-style workloads
  even while winning on `fibonacci` and `primes`.
- **Startup and compilation are costs.** A JIT pays compile time per hot
  function; an AOT backend pays link time and produces a larger artifact. Short
  programs can see no gain or a regression. The benchmark suite includes small
  workloads (`BENCH.md`), so the reported speedup must say which workload it is.
- **I/O and builtins do not move.** `files`, `network`, JSON/CSV, time and the
  other builtins (`src/stdlib.rs:520`) are already Rust. Their time is outside
  the code generator. Native compilation cannot make `network.get` faster.

So: expect a clear win on `fibonacci`, `primes` and tight numeric loops after
specialization; a modest win on mixed control flow; little or no win on
text/collection/IO code; and a possible regression on very short programs. A
claim of 15x or more should be treated as a claim about a future optimizing JIT
with unboxing and shape feedback, not about the first backend.

## What a native backend would unlock

- Faster execution of numeric and loop-heavy Redblue programs, including the
  self-hosted compiler and the CI corpus.
- A path to standalone AOT artifacts that do not need `rb vm` to run.
- Better integration with native libraries and OS interfaces, because the
  program is already machine code at the boundary.
- A stronger feedback loop for the existing optimizer ideas: inline caches,
  hidden classes, unboxing and escape analysis all have a natural target once
  there is a native code generator.
- A credible answer to "can Redblue be fast?" that is measured rather than
  assumed.

## What it would still not fix

- **Readability.** The language's advantage is readability (`LIBRARIES.md:15`);
  native code does not change the source a person writes.
- **Missing libraries.** `LIBRARIES.md:13` through `LIBRARIES.md:29` describes a policy of
  bindings for numeric, native and security-sensitive work. A backend does not
  create a tensor library, an HTTP stack, a regex engine or a package ecosystem.
- **I/O, network and builtin-bound work.** Those costs are already in Rust
  (`src/stdlib.rs:520`) and do not become faster because the caller is native.
- **Correctness, diagnostics, formatting, linting and the LSP.** A backend is an
  execution engine, not a fix for parser, analyzer, tooling or specification
  defects.
- **Memory safety, concurrency and distribution.** Native code does not by
  itself give Redblue a memory model, a scheduler, an actor system or a package
  registry.
- **The bootstrap ladder.** S3 and S4 are already done (`docs/BOOTSTRAP.md:16`,
  `docs/BOOTSTRAP.md:17`). A native backend is a new execution path, not a missing rung that makes
  the existing ladder valid.
- **OS, kernel, GPU or desktop work.** This study does not authorise any of it.
  The deferred roadmap discusses an OS and GPU work, but the finding behind this
  phase asks a performance question, and a positive answer to that question is
  not a mandate for those projects.

## When the recommendation would be wrong

A future reader should check these conditions rather than re-derive the whole
study. The deferral is wrong if any of them becomes true.

1. **The cost of the missing layers collapses.** If a minimal Cranelift lowerer
   for version 6 can be built in a few hundred lines, with a runtime ABI that is
   already stable, and gives at least 3x on the benchmark suite, then waiting for
   slot resolution and type feedback is unnecessary.
2. **CI or self-hosting becomes the bottleneck.** If running the corpus and the
   self-hosted compiler takes long enough to slow the pipeline materially, the
   speed work is no longer a research luxury and should be prioritised ahead of
   the remaining language features.
3. **The bytecode VM stops improving.** If profiling shows the interpreter is
   already close to its practical ceiling and cheap VM optimizations have been
   exhausted, the backend becomes the next real step rather than an optional one.
4. **A system LLVM is already an accepted build dependency.** If every supported
   platform and CI job already ships a compatible LLVM, the main argument for
   preferring Cranelift weakens, and LLVM's optimiser ceiling may justify the
   heavier path.
5. **The target is bare metal, a kernel or a GPU runtime.** If Redblue needs
   machine code because there is no host Rust runtime to call, then the question
   is not "is a JIT faster?" but "is there any other way to run?", and speed is
   not the deciding factor.
6. **A native backend cannot preserve the semantics.** If differential testing
   shows that native code cannot keep the exact failures, limits, ordering and
   catchability without losing the speedup, the correct answer is to decline the
   backend rather than ship a second, subtly different language.
7. **Another engine reaches the same goal more cheaply.** If a threaded
   interpreter, better inline caches or a much faster bytecode VM reaches the
   required speed on the real workloads, a native backend is not justified by
   speed alone.

## Decision summary

- **Decision:** defer.
- **First implementation if the trigger fires:** a Cranelift backend that consumes
  `.rbc` version 6, preserves the existing runtime semantics, and is gated by
  differential tests plus the byte-identical self-hosted fixed point.
- **Not part of this phase:** any code generator, any new dependency, any
  format change, any OS or GPU work.
- **Review note:** the opening line of `docs/BYTECODE.md` still says "Format
  version: 4" while `src/bytecode/format.rs:81` and `docs/BYTECODE.md:56` say
  version 6. This study uses 6. The sentence is a documentation defect and is
  not changed here because it is outside the phase's question.
