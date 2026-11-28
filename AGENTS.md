# AGENTS.md - Redblue Programming Language

Instructions for agentic coding agents working on the Redblue project.

## Project Overview

**Redblue** is a programming language designed to be as readable as plain English.

- **Language Syntax**: Plain English, e.g., `if count is greater than 10 then say "Hello"`
- **File Extension**: `.rb`
- **Implementation**: Rust
- **Binary**: `rb` (run with `cargo run --bin rb`)

## Build Commands

```bash
# Build and run tests
cargo build && cargo test

# Run in release mode
cargo build --release

# Run the CLI (binary named 'rb')
cargo run --bin rb -- [args]

# Run a Redblue file
cargo run --bin rb -- run hello.rb

# Run tests
cargo test

# Run specific test by name
cargo test test_name

# Run with output
cargo test -- --nocapture

# Run doc tests
cargo test --doc

# Format and lint
cargo fmt && cargo clippy

# Full check
cargo check --all-targets
```

## Code Style Guidelines

### Rust Code

1. **Indentation**: 4 spaces (no tabs)
2. **Line Length**: Max 100 characters
3. **Naming Conventions**:
   - Types/Structs: `PascalCase` (e.g., `TokenKind`, `Parser`)
   - Functions/Methods: `snake_case` (e.g., `parse_expression`)
   - Variables: `snake_case` (e.g., `current_token`)
   - Constants: `SCREAMING_SNAKE_CASE` (e.g., `MAX_BUFFER_SIZE`)
   - Modules: `snake_case` (e.g., `lexer.rs`, `parser.rs`)

4. **Imports**: Group by std, external, local (blank line between):
   ```rust
   use std::collections::HashMap;
   
   use crate::error::{Error, Result};
   use crate::lexer::Lexer;
   ```

5. **Error Handling**:
   - Use `Result<T, Error>` for fallible operations
   - Propagate errors with `?` operator
   - Never silently ignore errors with `_`

6. **Documentation**: Document public functions with `///`

## Architecture

### Compiler Pipeline
```
Source (.rb) → Lexer → Parser → Analyzer → VM → Output
```

### Source Structure

```
src/
├── lib.rs           # Main library entry
├── main.rs          # CLI entry point
├── error.rs         # Error types (Lexer, Parser, Analyzer, Runtime, Io)
├── value.rs         # Runtime values (Value enum)
├── lexer.rs         # Tokenizer (TokenKind enum)
├── parser.rs        # AST builder
├── analyzer.rs      # Semantic analysis
├── interpreter.rs  # Tree-walking interpreter (the bytecode VM lives in bytecode/)
├── stdlib.rs        # Standard library (builtins function)
├── formatter.rs     # Code formatter
├── linter.rs        # Code linter
├── repl/            # REPL implementation
└── testing/         # Test harness (harness, runner, assertions, reporter)
```

## Adding New Features

### New Keyword
1. Add to `lexer.rs` → `TokenKind` enum
2. Handle in `parser.rs` → `parse_statement()` or `parse_expression()`
3. Add tests

### Standard Library Function
1. Add to `src/stdlib.rs` → `builtins()` function
2. Implement in `interpreter.rs` → `call_builtin()`
3. Add tests

### New Statement Type
1. Add variant to `parser.rs` → `Statement` enum
2. Implement `parse_*()` method
3. Handle in `interpreter.rs` → `execute_statement()`

## Testing

Custom test harness in `src/testing/`. Test files: `.rs` (ending in `_test.rs`) or `.rb`.

### Built-in Test Syntax
```redblue
test "my test"
    set result to 2 + 3
    expect result to be 5
end
```

### Formatter (rbfmt)
```bash
rb format <file>           # Format a file
rb format --check <file>   # Check if file needs formatting
```

### Linter (rblint)
```bash
rb lint <file>            # Lint a file
```

Detects: unused variables, style issues

### Standard Library (Implemented)

#### Files Module
```redblue
files.read("path")     // Read entire file as text
files.write("path", "content")  // Write content to file
files.append("path", "content") // Append to file
files.exists("path")    // Check if file exists
files.lines("path")    // Read file as list of lines
files.delete("path")   // Delete a file
files.copy("from", "to")   // Copy file
files.rename("from", "to") // Rename/move file
```

#### Time Module
```redblue
time.now()             // Get current time as record
time.sleep(seconds)    // Sleep for given seconds
time.format(timestamp, "format")  // Format Unix timestamp
time.unix("YYYY-MM-DD HH:MM:SS")  // Parse date string to Unix timestamp
```

#### Formats Module (JSON/CSV)
```redblue
json.parse("{\"key\": \"value\"}")  // Parse JSON string to record
json.stringify(value)               // Convert value to JSON string
csv.parse("a,b,c\n1,2,3")          // Parse CSV to list of lists
```

#### Network Module
```redblue
network.get("url")     // HTTP GET request
network.post("url", "body")  // HTTP POST request
```

#### Module System
```redblue
import MathUtils                    // Import module
import files, network as net         // Multiple imports with alias

MathUtils.circle_area(5)             // Use module functions
net.get("https://api.example.com")   // Using alias
```

#### Math Functions
```redblue
PI, E                    // Constants
abs(x), floor(x), ceil(x)
round(x), sqrt(x), pow(x, y)
sin(x), cos(x), tan(x)
log(x), exp(x)
```

#### Text Functions
```redblue
uppercase("text"), lowercase("text"), trim("text")
split("text", by), join(list, by)
contains("text", sub), starts_with("text", prefix)
replace("text", from, to)
```

## Performance & Optimization Protocol

Whenever a phase implements a compiler optimization, VM enhancement, or execution speedup:
1. **Back-to-Back Same-Machine Control**: Measure wall-clock execution before and after the change on the exact same machine to eliminate runner hardware variance.
2. **Bytecode / Mathematical Fixed Point**: Verify that compiler optimizations produce byte-identical bytecode (`cmp stage1.rbc stage2.rbc == identical`) and runtime optimizations preserve exact mathematical answers.
3. **Mandatory Documentation**: Append the verified numbers, hardware profile, and speedup multiplier to [`docs/PERFORMANCE_HISTORY.md`](docs/PERFORMANCE_HISTORY.md).

## Out-of-Scope Findings & Defect Protocol

Whenever an agent identifies an engine bug or spec-drift defect outside its assigned phase scope:
1. **Never Hack Unprompted Fixes**: Do not attempt an out-of-scope fix that violates the phase's single responsibility.
2. **Phase Report**: Document the finding with an executable reproduction snippet in `phases/phase-XXX/FINDINGS.md`.
3. **Master Ledger**: Append the finding to [`docs/OUT_OF_SCOPE_FINDINGS_LEDGER.md`](docs/OUT_OF_SCOPE_FINDINGS_LEDGER.md) with status `[OPEN]` so it is automatically queued for remediation in Section 14 (Phases 167–175).

## Ecosystem Repositories Standing Directive (Phase 126 & Staged Spin-Up)

When Phase 126 lands on `main`, the ecosystem expands beyond the core compiler into **paired repositories (Code Targets + Dedicated Ops Orchestrators)** to maintain gate security, 24/7 CI concurrency, and clean git histories:

### Wave 1: Immediate Spin-Up at Phase 126 Landing (6 Repositories / 3 Paired Tracks)
1. **OS Track**: `authorss81/rbos` (kernel & desktop) + `authorss81/rbos-ops` (autonomous pipeline & QEMU runner).
   - **Delivery Architecture**: Delivered via a strict 6-layer bottom-up dependency pipeline:
     1. Bare-metal multiboot stub, GDT, IDT & serial console.
     2. Physical frame allocator, 4KB virtual paging & per-fiber bump arenas.
     3. Preemptive fiber scheduler, Task Control Blocks (TCB) & timer interrupts.
     4. VirtIO hardware drivers (Block disk, Net Ethernet, Input keyboard/mouse, GPU display).
     5. Virtual File System (VFS), RAMFS, TarFS & SimpleFS.
     6. Userland shell (`init`, `sh`) & framebuffer desktop window manager.
   - **Verification**: Fully verified in headless CI via `qemu-system-x86_64` with automated serial console assertions.
2. **Apps & Production Backends Track (10 Domain Sectors)**: `rb-apps-org/apps` + `rb-apps-org/ops` (scalable across 10 specialized domain application sectors). Focuses on full-scale, working open-source applications and production servers ported natively into pure Redblue:
   - **Sectors 1–4 (Backends & Infrastructure)**:
     - `apps-web`: Production REST API frameworks & backends (Express / FastAPI ports).
     - `apps-db`: In-memory caching & storage engines (Redis-compatible RESP server, embedded KV/SQLite ports).
     - `apps-net`: Network infrastructure (HTTP/WebSocket reverse proxy, load balancer, API gateway).
     - `apps-auth`: Identity & security microservices (JWT/OAuth2 auth daemon, encrypted vault).
   - **Sectors 5–7 (Terminal & Desktop Power Tools)**:
     - `apps-tui`: Terminal productivity tools (Terminal Git client like `lazygit`, terminal file manager like `yazi`).
     - `apps-sys`: Real-time system & resource monitors (`htop` / `btop` ports).
     - `apps-office`: Practical computation engines (Terminal/WASM spreadsheet with formula engine, vector canvas).
   - **Sectors 8–10 (Developer, Media & Local AI)**:
     - `apps-dev`: Developer tooling & generators (Static site generator like Hugo/Zola, markdown documentation IDE).
     - `apps-media`: Creative workstations (Audio synthesizer/tracker, 2D canvas & pixel art editor).
     - `apps-ai`: Local inference & workflow tools (Prompt studio, local vector search & RAG engine).
   - **Phase Slicing Strategy**:
     - Standard utility apps: **1 Phase per App** (~90 minutes).
     - Complex production backends & flagship apps: **3 staged phases** (Slice 1: Headless Protocol & Data Model; Slice 2: Network & Concurrent I/O; Slice 3: CLI, Configuration & End-to-End Verification).
   - **Execution Models**: TUI (terminal box-drawing via Phase 137 `tui`), WebAssembly (zero-install in-browser), and standalone single-file executables via `rb bundle`.
3. **Package Track**: `authorss81/rb-pkg` (package index & registry) + `authorss81/rb-pkg-ops` (package ingestion & crypto verification).
   - **Bootstrap & Velocity Philosophy**: Go, Rust, and Zig bootstrapped their entire ecosystems by porting battle-tested C, Python, and JavaScript/TypeScript libraries into pure native code. Autonomous package workers must **strictly prioritize porting proven open-source packages** (MIT, Apache-2.0, BSD, ISC) over inventing algorithms from scratch. This guarantees 10× faster package generation, rock-solid production reliability, immediate API familiarity, and verified test parity.
   - **Security & Ergonomics**: Human-friendly imports (`import sqlite`), backed by automated Blake3/SHA-256 cryptographic subresource integrity in `rb.lock`, signed registry provenance, and Levenshtein typosquatting protection.
4. **Formal Math & Proof Track**: `rb-lean/rean` (Lean 4 $\leftrightarrow$ Redblue Plain-English Mathematical Bridge) + `rb-lean/rean-ops` (autonomous pipeline cloned from `rb-pkg-ops`).
   - **Scope & Architecture**: Preserves 100% mathematical soundness by using Lean 4's verified kernel as the source of truth while providing a deterministic AST & InfoTree-to-Plain-English Redblue translator (`Rean`). Allows non-technical mathematicians to read theorem statements and goal-state deductions in unambiguous natural English, and audit formalization fidelity.
   - **Spin-Up Action at Phase 126**: Push initial repository scaffolds to the `rb-lean` organization (`rb-lean/rean` + `rb-lean/rean-ops`) and replicate the full `ops-core` engine, workflows, and 18-worker concurrency setup from `rb-pkg-ops`.

### Wave 2: Staged Tooling Spin-Up at Dogfooding Milestone (2 Repositories / 1 Paired Track)
5. **Tooling & IDE Track**: `authorss81/rb-ide` + `authorss81/rb-ide-ops`
   - **Scope**: Dedicated Language Server Protocol (`rb lsp` daemon & JSON-RPC transport), VS Code extension backend, Debug Adapter Protocol (DAP), code formatting protocol (`rb format`), linter integration (`rb lint`), syntax highlighting grammars (TextMate / Tree-sitter), and editor integrations (VS Code, Neovim, JetBrains).
   - **Spin-Up Timing**: Initiated at **Milestone 1.5** (midway, once `rb-pkg` establishes the package manifest schema and `rb-apps` ships its first 3 dogfooding apps, or at Core Phase 140), decoupling editor server maintenance completely from compiler core development.

### Orchestrator Operating Standards Across All Paired Tracks:
- **5-Way Parallel Concurrency (Strict Cap at 18 AI Workers per Org / 2 Reserved CI Slots)**: Autonomous workflows operate 5 parallel workers simultaneously (strictly capped at 18 AI workers per org) using partitioned secrets (`OPENCODE_KEY_1` through `OPENCODE_KEY_5`) with decentralized atomic Git lease locking in `authorss81/rb-pkg/leases/`. Exactly 2 runner slots per organization are permanently reserved for nightly test sweeps, PR validation, and operational CI, preventing critical test suites from being blocked in long queues. A dedicated `smoke_parallel.yml` workflow verifies race-free concurrent execution.
- **Zero-Token 100k Scaling & Capability Concept Locks**: Autonomous auditors never ingest 100,000 package descriptions into LLM prompts. Auditors check client-side static taxonomies against $O(1)$ HTTP lookups of `authorss81/rb-pkg/capabilities/<concept_id>.json`. The LLM is invoked only for single candidate concept synthesis.
- **Package Coexistence & Disambiguation Appeals**: Competing packages providing distinct trade-offs for the same capability (e.g. `fast_json` vs `simple_json`) are unconditionally welcomed. Permutation folding (`foo_bar` vs `bar_foo`) serves as an auditor guardrail against bot duplication, but never blocks needed packages: authors bypass false collisions via `allow_permutation_collision = true` and `disambiguation = "..."` in `rb.toml` or via scoped names.
- **Ops Engine Inheritance & 3-Round Review/Fix**: All ecosystem orchestrators (`rb-pkg-*/ops`, `rb-apps-org/ops`, `authorss81/rbos-ops`) inherit the proven core engine from `rbops` (`dispatch.sh`, `worker.sh`, `verify.sh`, 3 review rounds, 3 fix rounds), tailored for rapid single-target delta testing.
- **Perpetual Self-Dispatching Auditors**: When pending queues drain (`pending == 0`), an autonomous audit workflow scans gaps against industry benchmarks (PyPI, crates.io, POSIX, LSP 3.17) and automatically synthesizes the next batch of phases.
- **Dynamic Write Mandate**: Agents must flush tests, scaffoldings, or implementations to disk at least once every 15–30 minutes to eliminate timeout progress loss.
- **Delta Testing Protocol (Single-Target Gates)**:
  - In `rb-pkg-ops`, phase and PR gates test *only* the modified package (`packages/<pkg>/tests/`) in 1–3 seconds, eliminating wasteful $O(N)$ regression runs.
  - In `rb-apps-ops`, gates test *only* the modified application (`apps/<app>/tests/`).
  - In `rbops` (compiler core), full test suite execution (3,243+ tests) is mandatory because core compiler changes affect the entire grammar and VM.
  - In `rbos`, bottom-up regression tests validate the modified kernel layer and its upper dependencies.
- **Post-126 Versioning Inauguration (Phase 127)**: Immediate strict language versioning is officially inaugurated in Phase 127 with the `v0.2.0` baseline. From Phase 127 onward, every compiler fix, patch, or feature increments the version (`v0.2.1`, `v0.2.2`, etc.) and produces an immutable Git commit snapshot, guaranteeing instant snapshot reversibility if any breaking change occurs.
- **Production Backends & Real-World Apps Engine**: Rather than simple code snippets, the apps track (`rb-apps-org`) focuses on full-scale, working open-source applications and production servers ported natively into pure Redblue (Redis-compatible in-memory caching server, Express/FastAPI-grade REST backends, reverse proxies, embedded KV stores, terminal Git clients like lazygit, and system monitors like htop) powered by 200+ pooled OpenCode API keys.
- **Multi-Organization Parallelism Scaling & MIT Porting Engine (Strict 18 AI Runner Cap / 180 AI Workers + 20 Reserved CI Slots)**: Standard GitHub accounts enforce a verified limit of 20 concurrent runner jobs across an account/organization. AI worker concurrency is strictly capped at **18 concurrent runners per organization** (scaling to 180 concurrent AI workers across the 10 domain organizations, elastic to 360 with secondary key pools). This strictly guarantees that **2 runner slots in every organization (20 slots total across the ecosystem) are permanently reserved for nightly regression sweeps, automated PR gates, and compiler test runs**, ensuring continuous test verification is never starved or stuck in queues. Autonomous workers accelerate package generation by porting battle-tested open-source MIT/Apache-2.0 libraries (e.g. NumPy algorithms in `rb-pkg-math`, Express routers in `rb-pkg-web`, Lodash collections in `rb-pkg-data`) with strict licensing attribution in `rb.toml`.
- **Hybrid Gradual Typing Roadmap**: To combine the readability of dynamic typing with the robustness of static type systems, a hybrid gradual typing model (optional type annotations) is slated for Milestone 2.0 (Phase 155+), ensuring core VM stability remains unblocked through Phase 126.
- **CodeRabbit AI Review Integration**: All ecosystem repositories integrate the CodeRabbit GitHub App (`.coderabbit.yaml`) to perform automated AI-assisted code reviews on pull requests, enforcing Redblue idioms, zero-shim policy, test adequacy, and gate criteria before merging.
- **Resumable 6-Hour Nightly Regression Sweeps & Adaptive Cadence**:
  - A sweep tests all packages and applications against the latest `redblue` compiler release.
  - To respect GitHub's 6-hour job ceiling, execution checkpoints to `state/sweep_status.json` at the 5h 30m mark.
  - If tests remain, the workflow self-dispatches a fresh runner via `repository_dispatch` to resume from the cursor, automatically isolating compiler degradation from individual package bugs.
  - **Adaptive Cadence & Scaling**: Scaled for an initial baseline of **100,000 packages** (with elastic expansion to 1–10M). Sweep cadence runs daily for $\le 10,000$ packages, and adaptively shifts to bi-daily (every 48h) or weekly per category once registry size exceeds 50,000+ packages to prevent CI exhaustion.
- **Cross-Repo Out-of-Scope Feedback Protocol (`[BLOCKED_BY_CORE]`)**:
  - If a package or application requires syntax, built-ins, or compiler features not yet in Redblue, workers must NEVER hack the compiler in the package repo.
  - Workers log the requirement in `docs/OUT_OF_SCOPE_FINDINGS_LEDGER.md` with status `[BLOCKED_BY_CORE]`, mark the package phase as deferred upstream, and immediately proceed to the next package.
  - `rbops` synthesizes and lands the compiler phase; once resolved on `main`, a dispatch unblocks and resumes the package.
- **Automated Fault Attribution & Deep Fleet Audit**:
  - When sweep tests fail, an automated evaluator bisects against prior compiler releases ($C_{prior}$ vs $C_{new}$) and classifies the failure.
  - If intentional compiler safety/hardening broke unsound package code, it attributes the failure as `[PACKAGE_OBSOLESCENCE]`, dispatches an autonomous `package-fix` (or `app-fix`, `kernel-fix`) worker, and triggers a deep fleet audit scanning the entire registry for identical antipatterns to queue proactive batch migrations.
  - If valid syntax or execution regressed unexpectedly, it attributes the failure as `[COMPILER_REGRESSION]` and logs an isolated reproduction to `docs/OUT_OF_SCOPE_FINDINGS_LEDGER.md` for `rbops`.
- **Pre-126 Staging vs. Post-126 Dispatch Playbook**:
  - Organizations, repository shells, CI workflow files, and organization-level secrets (`OPENCODE_KEY_1..5`) are staged and verified in advance via dry-run smoke tests.
  - Production phase workers remain locked (`pending == 0`) until Phase 126 lands on `main` to ensure code is synthesized against a frozen, verified language core.


## Resources

- [Library policy](LIBRARIES.md) — **read this before adding a stdlib module**:
  what is written in Redblue, what is a binding to a Rust crate, and what is not
  built at all
- [Language Specification](SPEC.md)
- [Grammar](docs/GRAMMAR.md)
- [Philosophy](PHILOSOPHY.md)
- [Roadmap](ROADMAP.md)
