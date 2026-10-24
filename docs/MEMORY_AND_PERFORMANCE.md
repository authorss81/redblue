# Redblue: Memory Safety, Performance Architecture & Evolution Roadmap

## 1. Executive Summary

This document specifies how Redblue manages memory safety today, evaluates performance trade-offs, and maps out the architectural phases to scale Redblue from an intermediate language to a production-grade, self-hosting runtime.

---

## 2. Current Memory Safety Architecture

Redblue is implemented in Rust and guarantees **100% host-level memory safety** (no undefined behaviour, no buffer overruns, no use-after-free, no data races).

### 2.1 Pure Value Semantics (No Aliasing)
- **Assignment Copies**: In Redblue, `set a to b` creates a copy rather than an alias. Mutating `a` cannot mutate `b`.
- **No Raw Pointers**: The language syntax provides no pointer, reference, or address-of operators, making accidental aliasing or pointer corruption structurally impossible.

### 2.2 Copy-on-Write Storage (`Arc<T>`)
Composite structures (`List`, `Record`, `Object`) store their heap data in atomic reference-counted pointers (`Shared<T> = std::sync::Arc<T>`):
- **Reads / Passing Arguments**: $O(1)$ shallow pointer clone.
- **Mutations (`append`, field write)**: Handled via `Arc::make_mut`.
  - **Unique handle ($refcount = 1$)**: Mutates in-place with **zero heap allocations**.
  - **Shared handle ($refcount > 1$)**: Clones the inner storage on demand, preserving caller immutability.

### 2.3 Deterministic Reclamation (No Tracing GC)
Memory is freed predictably the moment a variable drops to zero references. There are no stop-the-world pauses or background collector threads.

### 2.4 Recursion and Stack Safety
Stack overflows are prevented by hard limits:
- `MAX_CALL_DEPTH = 1000` (shared between interpreter and WebAssembly target).
- `MAX_ITERATIONS = 1_000_000`
- `MAX_STEPS = 32_000_000`
Exceeding recursion limits raises a catchable Redblue `Error::Limit` rather than crashing the operating system thread.

---

## 3. The Known Memory Limitation: Reference Cycle Leaks

As documented in `MEMORY.md`:
Because Redblue uses `Arc<T>` without `Weak<T>` or a cycle detector, circular references leak:
```
list A contains list B
list B contains list A
      ↓
Neither refcount reaches zero → memory is held until process exit.
```
While circular structures are rare in standard scripts, long-running servers or graphs require cycle mitigation.

---

## 4. Performance Models & Data Safety Evaluation

To achieve maximum throughput while maintaining zero cognitive overhead for the programmer, three architectural upgrades are planned:

| Architecture | Mechanism | Target Speedup | Best Use Case |
| :--- | :--- | :---: | :--- |
| **Small-Value Inlining** | Store small strings ($\le 24$ bytes) and lists ($\le 4$ items) inline on the stack without heap allocation (`SmallVec`). | **25%–35% faster** | Scripting, loops, arithmetic |
| **Thread-Local `Rc` / Arena** | Replace cross-thread atomic operations (`Arc`) with thread-local `Rc` or generational arenas for single-threaded sessions. | **15%–20% faster** | REPL, CLI commands |
| **Persistent Structural Sharing (RRB-Trees)** | Shared lists use Relaxed Radix Balanced Trees (Clojure/Scala style) so mutations copy only an $O(\log n)$ tree spine instead of the full buffer. | **$10\times$ faster** on large lists | Data processing, large collections |
| **Cycle-Breaking Drop Pass** | A lightweight heuristic cycle breaker on container drops to reclaim circular graphs without a full tracing GC. | Zero cycle leaks | Long-running daemons |

---

## 5. Self-Hosting: Redblue Compiling Redblue

### 5.1 Is it possible?
**Yes. It is already partially implemented.**
In `bootstrap/compiler.rb`, Redblue includes a **140 KB full bytecode compiler written entirely in Redblue**.

### 5.2 The 5-Stage Bootstrap Ladder (`docs/BOOTSTRAP.md`)
1. **Stage 0 (S0)**: Rust tree-walking interpreter runs Redblue scripts.
2. **Stage 1 (S1)**: Rust bytecode compiler (`rb compile`) compiles Redblue source into `.rbc` bytecode.
3. **Stage 2 (S2)**: `bootstrap/compiler.rb` (written in Redblue) runs on the VM to compile input programs into `.rbc`.
4. **Stage 3 (S3 - Fixed Point)**: `bootstrap/compiler.rb` compiles itself. Stage 1 `.rbc` and Stage 2 `.rbc` must be **100% byte-for-byte identical**.
5. **Stage 4 (S4 - Production Self-Hosting)**: The compiler binary shipped in releases is produced directly by the self-hosted compiler.

---

## 6. Strategic Real-World Roadmap

To bridge the gap between "basic educational language" and "production real-world language", the roadmap follows four distinct evolutionary horizons:

```
┌────────────────────────┐     ┌────────────────────────┐     ┌────────────────────────┐     ┌────────────────────────┐
│  Horizon 1 (Current)   │ ──▶ │  Horizon 2 (Tooling)   │ ──▶ │  Horizon 3 (Stdlib)    │ ──▶ │  Horizon 4 (Self-Host) │
│  Phases 067–072        │     │  Phases 073–076        │     │  Phases 095–107        │     │  Post-Phase 126        │
│  REPL Power & State    │     │  Benchmarking & LSP    │     │  Regex, Datetime, HTTP │     │  S3 Bootstrap, Native  │
└────────────────────────┘     └────────────────────────┘     └────────────────────────┘     └────────────────────────┘
```

### Horizon 1: Developer Interaction & State (Phases 067–072)
- Session memory, checkpoints, undo/redo (`phase-067`).
- Shell integration (`!cmd`, `.pipe`) and live VM engine toggling (`.vm`).

### Horizon 2: Rigorous Measurement & IDE Integration (Phases 073–076)
- **`phase-073`**: Benchmark harness with checked-in automated baselines (prerequisite for any memory/engine optimization).
- **`phase-075`**: **Language Server Protocol (LSP)** for VS Code (Go-to-definition, code completion, error diagnostics).
- **`phase-076`**: Project scaffolding CLI (`rb new`, project-aware test runner).

### Horizon 3: Production Standard Library (Phases 095–107)
*Real-world capability upgrades ranked by the Phase-090 Survey:*
- **`phase-095`**: Full regular expressions (`regex` crate integration).
- **`phase-096`–`097`**: Date/time arithmetic, duration parsing, timezone handling.
- **`phase-098`–`099`**: High-performance iteration helpers (`itertools`) and collections (`Counter`, `defaultdict`, `deque`).
- **`phase-100`**: Pathlib filesystem paths without manual string concatenation.
- **`phase-102`–`105`**: CLI argument parsing (`argparse`), structured logging, and formats (CSV writer, JSON-lines, TOML).

### Horizon 4: Engine Hardening & Full Bootstrap (Post Phase 126)
- **Memory Phase A**: Implement `SmallVec` and `TinyString` inlining for zero-allocation loops.
- **Memory Phase B**: Cycle-detection pass to eliminate circular reference leaks.
- **Bootstrap S3/S4**: Close the bootstrap loop so `bootstrap/compiler.rb` produces identical bytecode to the Rust compiler.
- **Native Backend (Phase 109)**: LLVM / Cranelift code generation for C-speed native machine code binaries.
