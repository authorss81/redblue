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
3. **Master Ledger**: Append the finding to [`docs/OUT_OF_SCOPE_FINDINGS_LEDGER.md`](docs/OUT_OF_SCOPE_FINDINGS_LEDGER.md) with status `[OPEN]` so it is automatically queued for remediation in Section 14 (Phases 162–170).

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
2. **Apps Track**: `authorss81/rb-apps` (practical apps incubator) + `authorss81/rb-apps-ops` (autonomous app release runner).
   - **Phase Slicing Strategy**:
     - Standard utility apps (`sysmon`, `todo_cli`, `csv_inspector`, `pass_manager`, `api_tester`): **1 Phase per App** (delivers core logic, TUI/CLI interface, headless tests, and docs in ~90 minutes).
     - Complex flagship apps (`spreadsheet`, `vector_canvas`, `markdown_ide`): Sliced into **2 to 3 staged phases** (Slice 1: Headless Core Engine & Data Model; Slice 2: Interface & Interaction Layer; Slice 3: I/O, File Persistence & End-to-End Verification).
     - **Feature Granularity Rule**: Any individual complex feature within an app may be divided into its own standalone phase if needed to prevent context exhaustion or runner timeouts.
   - **Execution Models**: TUI (terminal box-drawing via Phase 132 `tui`), WebAssembly (zero-install in-browser), and standalone single-file executables via `rb bundle`.
3. **Package Track**: `authorss81/rb-pkg` (package index & registry) + `authorss81/rb-pkg-ops` (package ingestion & crypto verification).
   - **Security & Ergonomics**: Human-friendly imports (`import sqlite`), backed by automated Blake3/SHA-256 cryptographic subresource integrity in `rb.lock`, signed registry provenance, and Levenshtein typosquatting protection.

### Wave 2: Staged Tooling Spin-Up at Dogfooding Milestone (2 Repositories / 1 Paired Track)
4. **Tooling & IDE Track**: `authorss81/rb-ide` + `authorss81/rb-ide-ops`
   - **Scope**: Dedicated Language Server Protocol (`rb lsp` daemon & JSON-RPC transport), VS Code extension backend, Debug Adapter Protocol (DAP), code formatting protocol (`rb format`), linter integration (`rb lint`), syntax highlighting grammars (TextMate / Tree-sitter), and editor integrations (VS Code, Neovim, JetBrains).
   - **Spin-Up Timing**: Initiated at **Milestone 1.5** (midway, once `rb-pkg` establishes the package manifest schema and `rb-apps` ships its first 3 dogfooding apps, or at Core Phase 135), decoupling editor server maintenance completely from compiler core development.

### Orchestrator Operating Standards Across All Paired Tracks:
- **5-Way Parallel Concurrency**: Autonomous workflows operate 5 parallel workers simultaneously using partitioned secrets (`OPENCODE_KEY_1` through `OPENCODE_KEY_5`) with decentralized atomic Git lease locking.
- **Perpetual Self-Dispatching Auditors**: When pending queues drain (`pending == 0`), an autonomous audit workflow scans gaps against industry benchmarks (PyPI, crates.io, POSIX, LSP 3.17) and automatically synthesizes the next batch of phases.
- **Dynamic Write Mandate**: Agents must flush tests, scaffoldings, or implementations to disk at least once every 15–30 minutes to eliminate timeout progress loss.
- **Delta Testing Protocol (Single-Target Gates)**:
  - In `rb-pkg-ops`, phase and PR gates test *only* the modified package (`packages/<pkg>/tests/`) in 1–3 seconds, eliminating wasteful $O(N)$ regression runs.
  - In `rb-apps-ops`, gates test *only* the modified application (`apps/<app>/tests/`).
  - In `rbops` (compiler core), full test suite execution (3,243+ tests) is mandatory because core compiler changes affect the entire grammar and VM.
  - In `rbos`, bottom-up regression tests validate the modified kernel layer and its upper dependencies.
- **Multi-Organization Parallelism Scaling**: Standard GitHub accounts enforce a verified limit of **20 concurrent runner jobs** across the entire account. To scale beyond 20 concurrent workers without queuing, package category tracks are sharded across 10 distinct GitHub Organizations (e.g. 10 orgs × 20 = 200 concurrent runners) backed by 200+ isolated OpenCode API keys via automated org-level secret provisioning.
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
