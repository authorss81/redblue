# Redblue: Future Roadmap, Deferred Features & Real-World Apps

## 1. Context & Purpose

This document records the deferred features, real-world application roadmap, memory safety phases, and visual/UX enhancements agreed upon for Redblue, consolidating findings from `DEFERRED-PHASES.md` in `rbops`, architectural analysis, and benchmarks from the sibling `Ipp` and `InScript` language REPLs.

These phases are **deliberately held back from `rbops/phases.json`** to allow the active pipeline (Phases 070–126) to complete smoothly without moving the completion horizon.

---

## 2. REPL Visual Polish & Rich Layout Subsystem (The `Ipp` & `InScript` Standards)

A comparison against [`authorss81/Ipp`](https://github.com/authorss81/Ipp) and [`authorss81/inscript`](https://github.com/authorss81/inscript) reveals that Redblue's current REPL is functional but visually plain (unbordered plaintext, single hardcoded palette, manual-only timing). The following visual enhancements are planned:

### Phase 127: Rich Box-Drawing Cards & Frame Layouts
- **Goal**: Frame `.help` menus, `.vars`/`.inspect` listings, command tables, and error diagnostics inside clean Unicode box-drawing cards (`╔════╗`, `║`, `╚════╝`, `────`), with clear column alignment and ASCII fallbacks.
- **Why it matters**: Replaces wall-of-text dumps with readable visual panels matching `InScript`'s terminal aesthetics.
- **Acceptance Criteria**:
  - Help panels and inspection views render inside bordered cards.
  - Auto-adapts to terminal width without line wrapping.
  - Degrades gracefully to plain ASCII (`+---`, `|`) on non-Unicode terminals.
  - Output remains byte-identical when `NO_COLOR=1` or when piped to non-TTY.

### Phase 128: Automatic Inline Execution Timing (`[1.2ms]`)
- **Goal**: Automatically calculate and display execution time for every evaluated expression directly beside or below the result (e.g. `[1.4ms]`).
- **Features**:
  - Sub-millisecond precision for fast expressions (`0.4ms`), second precision for long runs (`1.24s`).
  - `.timing on` and `.timing off` commands to toggle automatic timing.
  - Color-coded: dim grey for fast runs, subtle yellow for runs $> 500\text{ms}$.

### Phase 129: Multi-line Line Numbers & Dynamic Prompts
- **Goal**: Upgrade the continuation prompt from static `... ` to line-numbered prompts (`... 2 `, `... 3 `) and add contextual prompt status glyphs (borrowed from `InScript`).
- **Features**:
  - Explicit line numbering during block entry (`if`, `repeat`, `function`).
  - Visual distinction between primary prompt (`rb> `) and continuation prompt.

### Phase 130: Extended Theme Engine & Live Palette Swatches
- **Goal**: Expand Phase 117's theme system with a full suite of vibrant terminal themes:
  - **Themes**: Monokai, Dracula, Nord, Solarized Dark, Gruvbox, and Cyberpunk/Sunset.
  - **Palette Viewer**: `.themes` renders a visual swatch card showing actual background/foreground color samples for keywords, strings, builtins, results, and errors.

### Phase 131: History Ghost Suggestions & Bracket Highlights
- **Goal**: Faint inline auto-suggestions from command history as the user types (similar to `prompt_toolkit` in `Ipp`), with real-time matching bracket/parenthesis highlights.

---

## 3. Real-World Practical Apps (The Dogfooding Ladder)

To move Redblue from synthetic test cases (`corpus/*.rb`) into a battle-tested language, the following practical applications are queued:

### Phase 132: `rb-httpd` — Micro Web Server in Pure Redblue
- **Goal**: A concurrent HTTP/1.1 web server written in Redblue that routes requests, serves static assets, and parses JSON payloads.
- **What it tests**: Socket stability, string parsing under load, timeout handling, and error recovery.

### Phase 133: `rb-todo` — Production CLI Tool
- **Goal**: A modern, colorful task and project management CLI tool with subcommands (`add`, `list`, `done`), flags, and persistent file storage.
- **What it tests**: CLI argument parsing (`argparse`), terminal colors, and JSON file serialization.

### Phase 134: `rb-markdown` — Pure Redblue Markdown Parser
- **Goal**: A CommonMark-compliant Markdown parser that converts Markdown documents into HTML.
- **What it tests**: String slicing, nested list parsing, and regex engine performance.

### Phase 135: `rb-kv` — Embedded Persistent Key-Value Store
- **Goal**: An embedded ACID key-value database written entirely in Redblue using an append-only write-ahead log (WAL) and memory-mapped index.
- **What it tests**: File I/O reliability, binary encoding/decoding, crash recovery, and transaction rollback.

### Phase 136: `rb-ssg` — Static Site Generator
- **Goal**: A fast documentation and blog generator that parses markdown files with YAML/JSON frontmatter, applies layout templates, and emits clean static HTML.
- **What it tests**: Recursive directory traversal, template interpolation, and string formatting at scale.

### Phase 137: `rb-git` — Content-Addressable Version Control
- **Goal**: A minimal Git implementation in Redblue that computes SHA object hashes, creates blob/tree/commit objects, and checks out branches.
- **What it tests**: Cryptographic hashing, raw byte manipulation, tree recursion, and filesystem state tracking.

### Phase 138: `rb-lsp` — Language Server Protocol Daemon
- **Goal**: A stdio JSON-RPC daemon that speaks LSP to VS Code and Neovim, providing live syntax diagnostics, symbol outlines, hover docstrings, and code formatting.
- **What it tests**: Inter-process communication, JSON-RPC streaming, parser error tolerance, and non-blocking I/O.

---

## 4. Performance Comparison & Engine Optimization Roadmap

### 4.1 How Redblue Compares to Other Languages Today

| Language | Execution Model | Typical Speed Relative to C/Rust | Redblue Comparison |
|---|---|---|---|
| **Rust / C / C++** | AOT Compiled Machine Code | $1.0\times$ (Baseline) | Rust is compiled to raw machine instructions with zero runtime overhead. Redblue is currently an interpreted/bytecode language, running $\approx 15\times\text{--}30\times$ slower than native Rust on pure compute loops. |
| **Python (CPython 3.12+)** | Stack Bytecode VM + Tier-2 Adaptive JIT | $10\times\text{--}40\times$ slower than Rust | **Redblue matches or beats CPython** on raw startup time, memory footprint, and native built-ins because Redblue's host VM is written in Rust with zero GIL, tight memory layouts, and no Python object boxing overhead. |
| **JavaScript (V8 / Node)** | Multi-Tier JIT (Ignition → Turbofan) | $2\times\text{--}4\times$ slower than Rust | V8 has JIT compilers that optimize hot loops into native assembly. Redblue currently uses a bytecode interpreter, so V8 is significantly faster on long numerical loops. |
| **Lua 5.4 / LuaJIT** | Register Bytecode VM / Tracing JIT | Lua 5.4: $10\times\text{--}15\times$; LuaJIT: $1.5\times\text{--}3\times$ | Lua 5.4 is the gold standard for clean scripting VM design. With the optimizations below, Redblue can match Lua 5.4 speed. |

### 4.2 Engine Optimization Phases

The following VM and memory optimizations are queued to close the performance gap:

### Phase 139: NaN-Boxing Value Representation
- **Goal**: Compress Redblue's runtime `Value` enum from 24 bytes down to an 8-byte IEEE-754 floating-point `u64`.
- **How it works**: Uses unallocated quiet NaN bit patterns to pack integers, booleans, nil, symbols, and heap pointers into a single CPU register.
- **Impact**: Doubles CPU cache density, eliminates pointer dereferences for primitives, and speeds up stack operations by $25\%\text{--}40\%$.

### Phase 140: Bytecode Peephole Optimizer & Constant Folding
- **Goal**: Add a multi-pass optimization pipeline to the bytecode compiler.
- **Techniques**:
  - Constant folding (`set x to 2 + 3` emits `LOAD_CONST 5`, not runtime addition).
  - Dead code elimination (skips unreachable `else` branches).
  - Super-instructions (e.g., merging `LOAD_VAR` and `BINARY_ADD` into `ADD_VAR`).

### Phase 141: Inline Caching (IC) for Member & Record Lookups
- **Goal**: Convert dynamic property access (`user.name` or `record.field`) from expensive dictionary hash lookups into single array index lookups by caching the field offset at the bytecode call-site.
- **Impact**: Accelerates record and module member lookups to near-native $O(1)$ speeds.

### Phase 142: Small-Value Inlining (`SmallVec`)
- **Goal**: Inline small strings ($\le 24$ bytes) and small lists ($\le 4$ elements) directly into the value stack rather than allocating on the system heap.
- **Impact**: Eliminates heap allocator locks in tight loops ($20\%\text{--}35\%$ speedup).

### Phase 143: Structural Cycle-Break Collector
- **Goal**: Implement a trial-deletion cycle collector for reference-counted cyclic structures (records referencing records), guaranteeing deterministic memory release without stop-the-world GC pauses.

### 4.3 Why Performance Optimizations Must Precede Bootstrapping (Self-Hosting)

The project includes a 140 KB compiler written in pure Redblue (`bootstrap/compiler.rb`). The ultimate self-hosting milestone requires the S3 fixed-point bootstrap ladder (`docs/BOOTSTRAP.md`):
$$\text{Rust Compiler} \to \text{Stage 1 Compiler} \to \text{Stage 2 Compiler} \quad (\text{assert } \text{stage1.rbc} == \text{stage2.rbc})$$

**Why optimization is prerequisite to Stage 3:**
1. **Compiling the Compiler in Redblue**: Stage 2 and Stage 3 require the Redblue compiler to run *inside* Redblue itself. Compiling 140 KB of complex AST parser logic on an unoptimized interpreter would take 15–25 minutes per pass, risking CI timeouts.
2. **Memory Churn**: Without NaN-boxing and small-value inlining, generating hundreds of thousands of AST nodes and token strings creates heavy heap allocator pressure.
3. **Sequence**: Implementing Phases 139–143 (NaN-boxing, constant folding, and inlining) **before** the final bootstrap ladder ensures that self-compilation executes in seconds, making fixed-point verification fast and reproducible.

### 4.4 Execution Engine Strategy: Dual-Engine Testing vs. VM Consolidation

Currently, Redblue maintains two distinct execution engines: the **AST Tree-Walking Interpreter** (`src/interpreter.rs`) and the **Bytecode VM** (`src/bytecode/`).

#### Current Phase (Development & Correctness): Dual-Engine Superpower
* **Differential Testing**: Redblue's greatest test asset is differential testing (`.compare` and `tests/differential_test.rs`). Every program and expression is executed across both engines and asserted to produce identical values. If the bytecode compiler emits an invalid stack offset or wrong jump target, the AST interpreter immediately exposes the discrepancy.

#### Future Phase (Production & v1.0): Bytecode VM Consolidation
* **Double Maintenance Cost**: Maintaining both engines permanently means every single new keyword, syntax form, or standard library type must be implemented and maintained twice (`src/interpreter.rs` and `src/bytecode/vm.rs`).
* **Optimization Asymmetry**: Key performance optimizations (NaN-boxing, register allocation, inline caches, and JIT compilation) can only be built into the Bytecode VM. The AST tree-walker cannot benefit from them.
* **Roadmap**:
  1. Maintain both engines throughout the 126 base phases and initial bootstrap testing.
  2. Once the Bytecode VM optimizations (Phases 139–143) land and pass all test suites, consolidate on the **Bytecode VM as the primary and default engine**.
  3. Demote the AST interpreter to an optional reference specification or retire it to reduce technical debt.

---

## 5. The Redblue OS Architecture: Decoupled Dedicated Repository

In `rbops/DEFERRED-PHASES.md` (§Tier 3), two distinct OS projects were designed: an OS simulator (CPU emulator + minimal kernel in Redblue) and bare-metal hardware boot.

### 5.1 Repository Separation: Dedicated Sibling Repo (`authorss81/rbos`)
Rather than building the Operating System inside the `authorss81/redblue` language repository, **the OS project will be established in its own dedicated repository (e.g. `authorss81/rbos`)**:

* **Why Separation is Superior**:
  1. **Separation of Concerns**: `redblue` is a programming language toolchain. An Operating System is a large, standalone application built *with* Redblue.
  2. **Pipeline Unblocking**: Kernel development, CPU emulation, and hardware mock tests require extensive compute and run time. Embedding this in `redblue` would stall language toolchain phases.
  3. **Parallel Autonomous Execution**: A separate repository can run its own independent GitHub Actions pipeline using the same self-dispatching OpenCode logic (powered by free DeepSeek/Nemotron models) 24/7 without competing for job concurrency or tokens with language development.
  4. **True Dogfooding**: The OS repository consumes the official `rb` binary release as an external toolchain (analogous to how Linux consumes GCC), validating real-world developer workflows.

### 5.2 The Two OS Tiers

#### Tier 1: OS Simulator & CPU Emulator in Pure Redblue (In `authorss81/rbos`)
- **Scope**: Write a CPU emulator (RISC-V/custom), a minimal multitasking kernel, and a bootloader *entirely in Redblue*, running as a simulated machine on host OS.
- **Why it works**: Pure Redblue, gate-verified by tests, requiring no root privileges or native compiler.

#### Tier 2: Bare-Metal Redblue OS Booting on Hardware
- **Scope**: Redblue compiling to machine code that boots on bare x86_64/ARM64 hardware without an underlying Linux/Windows host.
- **Blockers**: Needs a native LLVM/Cranelift code generator (`Redblue → machine code`), raw memory access, and interrupt handlers.
- **Status**: **Strictly gated behind Phase 109 (Native backend feasibility survey).** Do not start until native compilation is decided.

---

## 6. Promotion Plan (When to Deploy to `phases.json`)

1. **Now through Phase 126**:
   - **Do not modify `rbops/phases.json` now.**
   - Keeping the active queue fixed at 126 phases ensures the current CI pipeline runs uninterrupted without merge conflicts or moving target completion dates (finishing ~October 30–31).
2. **At Phase 126 Completion**:
   - Append Phases 127 through 143 (Visual Polish, Real-World Apps, and VM Optimizations) to the end of `rbops/phases.json`.
   - Initialize `authorss81/rbos` as a standalone repo for OS development.
   - Because `phases.json` operates strictly as a FIFO queue, they will seamlessly trigger one by one.
