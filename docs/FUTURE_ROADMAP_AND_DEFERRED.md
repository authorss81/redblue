# Redblue: Future Roadmap, Deferred Features & Real-World Apps

## 1. Context & Purpose

This document records the deferred features, real-world application roadmap, and memory safety phases agreed upon for Redblue, consolidating findings from `DEFERRED-PHASES.md` in `rbops` and recent architectural analysis.

These phases are **deliberately held back from `rbops/phases.json`** to allow the active 40 phases (REPL, Stdlib, and Grammar) to complete unblocked.

---

## 2. The "Full OS in Redblue" Feature (From `DEFERRED-PHASES.md`)

In `rbops/DEFERRED-PHASES.md` (§Tier 3), two distinct OS projects were designed:

### 2.1 The Achievable One: OS Simulator in Pure Redblue
- **Scope**: Write a CPU emulator, a minimal kernel, and a bootloader *entirely in Redblue*, running as a simulated machine on top of Linux.
- **Why it works**: Pure Redblue, every line gate-verified by tests, requiring no root privileges or native compiler. It plays directly to Redblue’s proven strength (it already runs a self-hosted bytecode VM).
- **Prerequisites**: Tier 1 standard library complete (Phases 095–107) and Phase 109 (Native backend feasibility survey).

### 2.2 The Ambitious One: Bare-Metal Redblue OS Booting on Hardware
- **Scope**: Redblue compiling to machine code that boots on bare x86_64/ARM64 hardware without an underlying Linux/Windows host.
- **Blockers**: Needs a native LLVM/Cranelift code generator (`Redblue → machine code`), raw memory access, and interrupt handlers.
- **Status**: **Strictly gated behind Phase 109.** Do not start until native compilation is decided.

---

## 3. Real-World Practical Apps (The Dogfooding Ladder)

To move Redblue from synthetic test cases (`corpus/*.rb`) into a battle-tested language, the following practical applications are queued:

### Phase 128: `rb-httpd` — Micro Web Server in Pure Redblue
- **Goal**: A concurrent HTTP/1.1 web server written in Redblue that routes requests, serves static assets, and parses JSON payloads.
- **What it tests**: Socket stability, string parsing under load, timeout handling, and error recovery.

### Phase 129: `rb-todo` — Production CLI Tool
- **Goal**: A modern, colorful task and project management CLI tool with subcommands (`add`, `list`, `done`), flags, and persistent file storage.
- **What it tests**: CLI argument parsing (`argparse`), terminal colors, and JSON file serialization.

### Phase 130: `rb-markdown` — Pure Redblue Markdown Parser
- **Goal**: A CommonMark-compliant Markdown parser that converts Markdown documents into HTML.
- **What it tests**: String slicing, nested list parsing, and regex engine performance.

---

## 4. Memory Safety & Performance Hardening

### Phase 126: Small-Value Inlining (`SmallVec`)
- **Goal**: Eliminate heap allocations for lists with $\le 4$ items and strings with $\le 24$ bytes ($25\%\text{--}35\%$ speedup in loops).
- **Prerequisite**: Phase 073 (Benchmark harness with automated baseline).

### Phase 127: Structural Cycle-Break Collector
- **Goal**: Eliminate the known reference-cycle memory leak documented in `MEMORY.md` without adding a slow stop-the-world tracing GC.

---

## 5. Promotion Plan (When to Add to `phases.json`)

1. **Now through Phase 076**: Keep the current queue focused on finishing the REPL power features and tooling.
2. **At Phase 076 / Phase 108**: Append Phases 126–130 to `rbops/phases.json`. Because `phases.json` is a FIFO queue, they will safely execute right after Phase 125 without disrupting existing work.
