# Redblue: Programming Language Roadmap

## Project Overview

**Redblue** — A programming language designed to be as readable as plain English while maintaining the power and capability of world-class languages like Python, Rust, and TypeScript.

**Design Philosophy:**
- Zero cognitive overhead: Code should read like a well-written instruction manual
- Minimum keyword count: ~30 core keywords (vs Python's 35, JavaScript's 50+)
- English-first syntax: `if count is greater than ten then say "Hello"`
- Full capabilities: Compiles to efficient bytecode/JS/native, supports OOP, functional, async/await, generics

---

## Phase 0: Project Setup & GitHub (Before Week 1)

### 0.1 GitHub Repository Setup
- [ ] Create GitHub repository `redblue/lang`
- [ ] Add repository description and topics: `programming-language`, `compiler`, `plain-english`
- [ ] Create initial README.md with project vision
- [ ] Add LICENSE (MIT recommended)
- [ ] Configure branch protection for `main` branch
- [ ] Set up issue templates (bug report, feature request)
- [ ] Enable GitHub Actions for CI/CD

### 0.2 Repository Structure
```
redblue/
├── .github/
│   ├── ISSUE_TEMPLATE/
│   ├── workflows/
│   │   ├── ci.yml          # Build, test, lint
│   │   └── release.yml     # Publish releases
│   └── PULL_REQUEST_TEMPLATE.md
├── src/                    # Compiler source
├── tests/                  # Test suite
├── docs/                   # Documentation
├── examples/               # Example programs
├── tooling/                # IDE extensions, tools
│   └── vscode/            # VS Code extension
│   └── repl/              # REPL implementation
├── SPEC.md                 # Language specification
├── README.md
├── CONTRIBUTING.md
├── CODE_OF_CONDUCT.md
├── LICENSE
└── CHANGELOG.md
```

### 0.3 CI/CD Pipeline Setup
- [ ] **GitHub Actions CI** (`.github/workflows/ci.yml`)
  ```yaml
  # Automated checks on every push/PR
  jobs:
    - lint:         # rustfmt, clippy, security scans
    - test:         # Unit tests, integration tests
    - build:        # Cross-platform builds (Linux, macOS, Windows)
    - coverage:     # Code coverage reports
  ```
- [ ] **GitHub Actions Release** (`.github/workflows/release.yml`)
  ```yaml
  # On tag v*.*.*
  - Build binaries for all platforms
  - Create GitHub Release
  - Publish to package registries
  - Update documentation site
  ```
- [ ] Code coverage tracking with Codecov/Coveralls integration
- [ ] Dependency vulnerability scanning (Dependabot/RustSec)

### 0.4 Community Files
- [ ] **CONTRIBUTING.md**
  - Development setup instructions
  - Coding style guidelines
  - How to submit PRs
  - Commit message conventions
  - Review process
- [ ] **CODE_OF_CONDUCT.md** (Contributor Covenant recommended)
- [ ] **SECURITY.md** - Security vulnerability reporting policy
- [ ] **SUPPORT.md** - How to get help, links to docs

### 0.5 Initial Documentation
- [ ] **README.md** sections:
  - Badges (CI, version, docs, crates.io)
  - One-paragraph description
  - Quick start (3 lines: install, hello world)
  - Core philosophy (plain English)
  - Comparison table (vs Python, JS)
  - Links to docs, playground, community
- [ ] Create `docs/` directory with placeholder pages
- [ ] Set up GitHub Pages deployment

---

## Phase 1: Vision & Language Design (Weeks 1-4)

### 1.1 Core Philosophy Document
- [ ] Define design principles (plain English, minimal complexity, maximum capability)
- [ ] Document anti-patterns (what we'll explicitly NOT support)
- [ ] Create comparison matrix with Python, Ruby, JavaScript, COBOL
- [ ] Write "Hello World" examples in multiple paradigms to validate syntax

### 1.2 Syntax Design
- [ ] **Keywords Selection** (target: 28-32 keywords)
  ```
  Variables:    set, to, is, are, becomes
  Control:      if, then, else, when, unless, for, while, repeat, times
  Functions:    to, return, give back, takes, needs
  Types:       number, text, yes/no, list, record, nothing
  Structure:    module, import, export
  Logic:       and, or, not, either, neither
  Errors:      try, catch, finally, might fail
  OOP:         object, has, can, this, that, each, every
  Async:       wait, async, parallel, until done
  ```
- [ ] Design syntax for each construct with plain English examples
- [ ] Create grammar rules (EBNF notation)
- [ ] Validate readability with non-programmers (user testing)

### 1.3 Type System Design
- [ ] **Primitive Types**
  - `number` (integers, floats unified)
  - `text` (strings, all Unicode)
  - `yes/no` (boolean)
  - `nothing` (null/void)
- [ ] **Complex Types**
  - `list of <type>` (arrays)
  - `record` (objects/dictionaries)
  - `set of <type>`
  - `pair of <type1> and <type2>`
- [ ] **Custom Types**
  - `object <Name> has <properties>`
  - `object <Name> can <methods>`
  - Type inference rules
  - Generic types syntax
  - Union types

### 1.4 Paradigm Support
- [ ] Imperative (core)
- [ ] Object-oriented (prototype-based)
- [ ] Functional (first-class functions, pure functions)
- [ ] Async/await
- [ ] Declarative (list comprehensions, pattern matching)

---

## Phase 2: Language Specification (Weeks 5-8)

### 2.1 Formal Specification Document
- [ ] Complete EBNF grammar
- [ ] Type system formalization
- [ ] Operator precedence table
- [ ] Reserved keywords list
- [ ] Error code definitions (PL001-PL999)
- [ ] Standard library function signatures

### 2.2 Reference Implementation Architecture
```
redblue/
├── src/
│   ├── lexer/           # Tokenizer
│   ├── parser/          # AST builder
│   ├── analyzer/        # Semantic analysis
│   ├── optimizer/       # IR optimization
│   ├── codegen/         # Target code generation
│   │   ├── bytecode/   # VM bytecode
│   │   ├── js/         # JavaScript transpile
│   │   └── native/      # LLVM/Rust backend
│   ├── runtime/         # Standard library
│   └── stdlib/          # Built-in modules
├── docs/                # Language specification
├── tests/               # Test suite
└── tooling/             # IDE plugins, formatter
```

### 2.3 Reference Error Messages
```
// GOOD (human-friendly)
"The file you asked for doesn't exist at 'config.txt'"

// BAD (traditional)
"FileNotFoundError: [Errno 2] No such file: 'config.txt'"
```

### Manual Tasks (Cannot Be Automated)
- [ ] **User research interviews** (5-10 non-programmers)
  - Test "plain English" readability claims
  - Gather feedback on syntax examples
  - Document confusion points and suggestions
- [ ] **Domain expert review** (programming language designers)
  - Review type system design for soundness
  - Evaluate feature completeness
  - Identify missing paradigms or edge cases
- [ ] **Legal review** of language name trademark
- [ ] **Accessibility review** - ensure syntax works for screen readers
- [ ] **Internationalization review** - test keywords in non-English contexts

---

## Phase 3: Reference Compiler Implementation (Weeks 9-20)

### 3.1 Lexer (Weeks 9-10)
- [ ] Token types enumeration
- [ ] Unicode support (identifier names in any language)
- [ ] Multi-line strings (heredoc support)
- [ ] Comment syntax: `// this is a comment`
- [ ] Indentation detection (significant whitespace option)

### 3.2 Parser (Weeks 11-13)
- [ ] Recursive descent parser
- [ ] AST node definitions
- [ ] Operator precedence handling
- [ ] Error recovery (parse until recovered)
- [ ] Source location tracking for errors

### 3.3 Semantic Analyzer (Weeks 14-16)
- [ ] Symbol table management
- [ ] Type inference engine
- [ ] Scope resolution
- [ ] Implicit conversions
- [ ] Dead code detection
- [ ] Unused variable warnings

### 3.4 Code Generation (Weeks 17-20)

#### Bytecode VM (Primary Target)
- [ ] Stack-based VM design
- [ ] Instruction set (50-70 opcodes)
- [ ] Garbage collection (reference counting + mark-sweep)
- [ ] Standard library bindings

#### JavaScript Transpiler (Secondary)
- [ ] ES6+ output
- [ ] Source maps
- [ ] Type annotations stripping

#### Native Backend (Future)
- [ ] LLVM integration
- [ ] Rust codegen backend

### Manual Tasks (Cannot Be Automated)
- [ ] **Performance profiling** - Identify bottlenecks via human analysis
- [ ] **Memory leak investigation** - Manual debugging of GC issues
- [ ] **Security audit** - Manual code review for vulnerabilities
- [ ] **Benchmark interpretation** - Human analysis of performance results
- [ ] **Edge case discovery** - Testing unusual input combinations

---

## Phase 4: Standard Library (Weeks 21-28)

### 4.1 Core Modules
```
text          // String manipulation
math          // Numbers and mathematics
files         // File I/O
network       // HTTP, sockets
time          // Date, sleep, timers
formats       // JSON, CSV, XML parsing
crypto        // Hashing, encryption
random        // Random numbers
console       // Input/output
```

### 4.2 Module System
```redblue
import text
import files
import network

// Usage
set result to text.uppercase("hello")
set data to files.read("data.rb")
set response to network.get("https://api.example.com")
```

### 4.3 Async Standard Library
```redblue
// Synchronous (default)
set data to network.get("url")

// Asynchronous
async set response to wait network.get("url")

// Parallel
parallel
  set users to wait fetch_users()
  set posts to wait fetch_posts()
until done

set combined to users and posts
```

### Manual Tasks (Cannot Be Automated)
- [ ] **API design review** - Human review of stdlib API ergonomics
- [ ] **Documentation writing** - Manual documentation for all functions
- [ ] **Example code creation** - Real-world usage examples for each module
- [ ] **Breaking change decisions** - Human judgment on API compatibility
- [ ] **Performance tuning** - Manual optimization of hot paths

---

## Phase 5: Tooling Ecosystem (Weeks 29-40)

### 5.1 Development Tools

#### Package Manager (`rbm`)
- [ ] Create project: `rbm new my-project`
- [ ] Install packages: `rbm add json-parser`
- [ ] Publish packages: `rbm publish`
- [ ] Dependency resolution
- [ ] Semantic versioning

#### Formatter (`rbfmt`)
```bash
# Before formatting
set x to 1
set name to "World"
if x is 1 then say "Hello"

# After formatting
set x to 1
set name to "World"

if x is 1 then
    say "Hello"
end
```

#### Linter (`rblint`)
- [ ] Unused variables
- [ ] Type mismatches
- [ ] Unreachable code
- [ ] Style violations
- [ ] Security vulnerabilities

### 5.2 IDE Extensions

#### VS Code Extension (`redblue-vscode`)
- [ ] Syntax highlighting
- [ ] IntelliSense (autocomplete)
- [ ] Hover type information
- [ ] Go to definition
- [ ] Refactoring support
- [ ] Debugger integration
- [ ] Inline error highlighting
- [ ] REPL integration panel
- [ ] Project scaffolding commands
- [ ] Code snippets

#### Vim/Neovim Plugin
- [ ] Syntax highlighting
- [ ] Basic completion
- [ ] Indentation rules
- [ ] REPL integration

#### JetBrains Plugin (IntelliJ, PyCharm, etc.)
- [ ] Syntax highlighting
- [ ] IntelliJ IDEA integration
- [ ] Debugger support

### 5.3 Debugger
- [ ] Breakpoints (line, conditional)
- [ ] Variable inspection
- [ ] Call stack navigation
- [ ] Step-through execution
- [ ] Expression evaluation
- [ ] VS Code debugger protocol

### 5.4 REPL (Read-Eval-Print Loop)
```bash
$ rb
Welcome to Redblue v0.1.0
Type 'help' for commands, 'quit' to exit

>>> set name to "World"
>>> say "Hello, {name}!"
Hello, World!
```

#### REPL Features
- [ ] Interactive command input
- [ ] Multi-line statement support
- [ ] Variable/state persistence between inputs
- [ ] Built-in commands (help, quit, clear, history)
- [ ] Tab completion for keywords and functions
- [ ] Error display with source location
- [ ] Load and execute files
- [ ] Debug mode integration
- [ ] Customizable prompt
- [ ] Syntax highlighting in output
- [ ] Auto-indentation

#### REPL Architecture
```
tooling/repl/
├── src/
│   ├── main.rs           # REPL entry point
│   ├── lexer.rs          # REPL-specific lexer
│   ├── evaluator.rs      # Direct evaluation (no compilation)
│   ├── completer.rs      # Tab completion
│   ├── history.rs        # Command history (readline)
│   └── output.rs         # Formatted output
└── tests/
    └── repl_test.rb
```

### 5.5 Web Playground
- [ ] In-browser code editor (Monaco/CodeMirror)
- [ ] Live compilation/output
- [ ] Shareable code snippets
- [ ] Example gallery

### Manual Tasks (Cannot Be Automated)
- [ ] **UX testing** - User testing of IDE extensions
- [ ] **Documentation for tooling** - Manually write docs for rbm, rbfmt, rblint
- [ ] **Error message refinement** - Human-tuned error messages for clarity
- [ ] **Keyboard shortcut design** - Ergonomic keybinding decisions
- [ ] **Release notes writing** - Human-written changelog entries

---

## Phase 6: Testing Framework (Weeks 29-32)

### 6.1 Built-in Testing
```redblue
test "adding numbers"
  set result to math.add(2, 3)
  expect result to be 5
end

test "fetching users might fail"
  set response to might fail network.get("invalid-url")
  expect response to be nothing
end

test "JSON parsing"
  set data to formats.parse_json('{"name": "Alice"}')
  expect data.name to be "Alice"
end
```

### 6.2 Testing Commands
```bash
# Run all tests
rb test

# Run specific test file
rb test tests/auth_test.rb

# Run tests matching pattern
rb test --grep "auth"

# Coverage report
rb test --coverage
```

---

## Phase 7: Documentation (Weeks 33-36)

### 7.1 Documentation Structure
```
docs/
├── getting-started/     # Installation, first program
├── tutorials/           # Step-by-step guides
│   ├── basics/
│   ├── files/
│   ├── network/
│   └── objects/
├── language-guide/     # Complete language reference
│   ├── syntax/
│   ├── types/
│   ├── functions/
│   ├── objects/
│   └── modules/
├── standard-library/   # API documentation
├── best-practices/     # Style guide, patterns
└── faq/                # Common questions
```

### 7.2 Interactive Tutorial
- [ ] In-browser REPL
- [ ] Guided lessons
- [ ] Progress tracking
- [ ] Certificate of completion

### Manual Tasks (Cannot Be Automated)
- [ ] **Technical writing** - Manually write all documentation content
- [ ] **Tutorial creation** - Design learning path for beginners
- [ ] **Example selection** - Human choice of representative examples
- [ ] **Grammar review** - Native speaker review of documentation
- [ ] **Accessibility audit** - Manual accessibility review of docs site

---

## Phase 8: Performance Optimization (Weeks 37-40)

### 8.1 Compiler Optimizations
- [ ] Constant folding
- [ ] Dead code elimination
- [ ] Inline caching
- [ ] Peephole optimization
- [ ] Escape analysis

### 8.2 Runtime Optimizations
- [ ] Generational garbage collection
- [ ] Lazy compilation
- [ ] Native function calls
- [ ] SIMD vectorization (math library)

### 8.3 Benchmarking
- [ ] Standard benchmark suite
- [ ] Comparative analysis (vs Python, JS)
- [ ] Performance regression detection

### Manual Tasks (Cannot Be Automated)
- [ ] **Profiling analysis** - Human interpretation of performance data
- [ ] **Optimization decisions** - Which optimizations to prioritize
- [ ] **Benchmark design** - Define meaningful real-world benchmarks
- [ ] **False positive filtering** - Distinguish real issues from noise
- [ ] **Release timing decisions** - When is "fast enough" good enough?

---

## Phase 9: Ecosystem & Community (Ongoing)

### 9.1 Initial Ecosystem
- [ ] Core libraries (JSON, HTTP, Database drivers)
- [ ] Web framework
- [ ] CLI framework
- [ ] Testing utilities

### 9.2 Community Building
- [ ] Discord/Slack community
- [ ] Reddit community
- [ ] Blog with tutorials
- [ ] YouTube channel
- [ ] Conference talks

### 9.3 Package Registry
- [ ] Central package registry (redblue.dev)
- [ ] Automated testing CI/CD
- [ ] Package discovery search
- [ ] Version compatibility matrix

### Manual Tasks (Cannot Be Automated)
- [ ] **Community moderation** - Human moderation of community spaces
- [ ] **Package review** - Manual review of high-profile packages
- [ ] **Partnership outreach** - Human networking with potential contributors
- [ ] **Conference talks** - Human-presented talks at conferences
- [ ] **Blog content creation** - Human-written tutorials and articles
- [ ] **Support in Discord/forums** - Real-time human support

---

## Phase 10: Production Release (Week 48)

### 10.1 v1.0.0 Release Checklist
- [ ] Language specification finalized
- [ ] Reference implementation stable
- [ ] Standard library complete
- [ ] Tooling (IDE, debugger, formatter, REPL) production-ready
- [ ] Documentation complete
- [ ] Tutorial materials ready
- [ ] Package registry launched
- [ ] Community guidelines established

### Manual Tasks (Cannot Be Automated)
- [ ] **Final release decision** - Human judgment that "it's ready"
- [ ] **Changelog writing** - Human-written release notes
- [ ] **Marketing copy** - Blog post, social media announcements
- [ ] **Press outreach** - Contact journalists, bloggers
- [ ] **Feature freeze decision** - Human judgment on scope
- [ ] **Version numbering** - Semantic versioning decisions
- [ ] **Announcement writing** - Launch announcement content

---

## Timeline Summary

| Phase | Duration | Milestone |
|-------|----------|-----------|
| Phase 0 | Before W1 | GitHub repo, CI/CD setup |
| Phase 1 | 4 weeks | Language design complete |
| Phase 2 | 4 weeks | Formal specification |
| Phase 3 | 12 weeks | Working compiler |
| Phase 4 | 8 weeks | Standard library |
| Phase 5 | 12 weeks | Tooling ecosystem (incl. REPL) |
| Phase 6 | 4 weeks | Testing framework |
| Phase 7 | 4 weeks | Documentation |
| Phase 8 | 4 weeks | Performance tuning |
| Phase 9 | Ongoing | Community building |
| Phase 10 | Week 48 | v1.0.0 release |

**Total estimated time: 48 weeks (approximately 1 year)**

---

## Key Success Metrics

1. **Learnability**: Non-programmer can write working code in < 2 hours
2. **Readability**: Code review accuracy matches Python (studies)
3. **Capability**: Can implement any algorithm that Python/Rust can
4. **Performance**: Within 2x of Python for typical workloads
5. **Tooling**: Full IDE support (VS Code, Vim, JetBrains), REPL
6. **Community**: 1000+ packages in registry at v1.0

---

## Technology Stack for Reference Implementation

**Recommended choices:**

| Component | Option 1 | Option 2 |
|-----------|----------|----------|
| Implementation | Rust | Go |
| VM Bytecode | Custom | Custom |
| Package Registry | Rust (server) | Node.js |
| Website | Astro | Next.js |
| Docs | VitePress | Docusaurus |
| REPL | Rust + rustyline | Rust + reedline |

**Why Rust:**
- Memory safety (no GC pauses)
- Fast compilation
- Excellent tooling (cargo, rustfmt)
- Wasm support for web playground
- Great REPL libraries (rustyline, reedline)

---

## Example Code Comparisons

### Hello World
```redblue
say "Hello, World!"
```

### FizzBuzz
```redblue
for each n from 1 to 100
    if n is divisible by 15
        say "FizzBuzz"
    else if n is divisible by 3
        say "Fizz"
    else if n is divisible by 5
        say "Buzz"
    else
        say n
    end
end
```

### File Processing
```redblue
import files
import text

set contents to files.read("data.csv")
set lines to text.split(contents, by line break)

for each line in lines
    if line is empty
        skip
    end
    
    set parts to text.split(line, by ",")
    set name to parts at 0
    set score to number from (parts at 1)
    
    if score is greater than 80
        say "{name} passed!"
    end
end
```

### Async HTTP Request
```redblue
import network

async set response to wait network.get("https://api.example.com/users")
set users to formats.parse_json(response body of response)

for each user in users
    say "Hello, {user.name}!"
end
```

### Object-Oriented
```redblue
object Animal
    has name
    has age
    
    to introduce()
        say "I'm {this.name}, {this.age} years old"
    end
end

object Dog extends Animal
    has breed
    
    to bark()
        say "Woof!"
    end
end

set my_dog to new Dog
set name of my_dog to "Buddy"
set age of my_dog to 3
set breed of my_dog to "Golden Retriever"

my_dog.introduce()
my_dog.bark()
```

### REPL Session
```bash
$ rb
Welcome to Redblue v0.1.0 - Interactive REPL
Type 'help' for commands, 'quit' to exit

>>> set x to 10
x = 10
>>> set y to 20
y = 20
>>> set z to x + y
z = 30
>>> say "The sum is {z}"
The sum is 30
>>> to greet(name)
..     say "Hello, {name}!"
.. end
Function defined: greet(name)
>>> greet("World")
Hello, World!
>>> :help
Available commands:
  :quit, :exit    - Exit the REPL
  :clear          - Clear screen
  :history        - Show command history
  :load <file>    - Load and run a file
  :save <file>    - Save current session
  :vars           - Show all variables
  :funcs          - Show all functions
  :help           - Show this help
>>> :quit
Goodbye!
```

---

## GitHub Publishing Guide

### Pre-Release Setup

#### Repository Configuration
```bash
# Initialize git repository
git init
git add .
git commit -m "Initial commit: Redblue project structure"

# Add remote
git remote add origin https://github.com/YOUR_USERNAME/redblue.git

# Create development branch
git checkout -b develop
git push -u origin develop
```

#### Required GitHub Settings
- [ ] Enable **Issues** for bug tracking
- [ ] Enable **Discussions** for Q&A and announcements
- [ ] Enable **Projects** for roadmap tracking
- [ ] Configure **Branch protection rules**:
  - Require PR reviews for `main`
  - Require status checks to pass
  - Dismiss stale reviews
- [ ] Add **Topics**: `programming-language`, `compiler`, `plain-english`
- [ ] Set up **GitHub Pages** (if using docs/)
- [ ] Enable **Dependency Graph** and **Dependabot**

### Release Process

#### Version Tagging
```bash
# Semantic versioning: MAJOR.MINOR.PATCH
# v1.0.0, v1.1.0, v1.1.1

# Create annotated tag
git tag -a v0.1.0 -m "Initial alpha release"
git push origin v0.1.0

# GitHub Actions will trigger on tags matching v*
```

#### GitHub Release Creation (Manual)
1. Go to repository → Releases → Draft a new release
2. Select the git tag
3. Use title: `Redblue v1.0.0 - Initial Release`
4. Add release notes:
   - What's new
   - Breaking changes
   - Installation instructions
   - Migration guide (if applicable)
5. Attach binaries (auto-generated via CI)
6. Mark as `Latest` when production-ready
7. Click `Publish release`

#### Automated Release via GitHub Actions
```yaml
# .github/workflows/release.yml
name: Release
on:
  push:
    tags:
      - 'v*'

jobs:
  create-release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Create Release
        uses: softprops/action-gh-release@v1
        with:
          files: |
            dist/redblue-*.tar.gz
            dist/redblue-*.exe
            dist/redblue-*.msi
          draft: false
          prerelease: ${{ contains(github.ref, 'alpha') || contains(github.ref, 'beta') }}
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### Package Registry Publishing

#### Rust Crates (if using Rust)
```bash
# Login to crates.io
cargo login

# Publish package
cargo publish --dry-run  # Test first
cargo publish             # Actual publish

# Update version
# 1. Edit Cargo.toml version
# 2. Commit and tag
# 3. cargo publish
```

#### npm (if using JavaScript tooling)
```bash
# Login to npm
npm login

# Publish package
npm publish --dry-run  # Test first
npm publish             # Actual publish
```

#### Homebrew (macOS/Linux)
```bash
# Create formula in homebrew-core pull request
# Or maintain external tap repository
```

### Cross-Platform Binary Distribution

#### GitHub Releases (Primary)
```
https://github.com/redblue/lang/releases
├── redblue-1.0.0-linux-x86_64.tar.gz
├── redblue-1.0.0-macos-x86_64.tar.gz
├── redblue-1.0.0-windows-x86_64.zip
└── redblue-1.0.0-windows-x86_64.msi
```

#### Package Managers (Secondary)

| Platform | Package Manager | Command |
|----------|-----------------|---------|
| macOS | Homebrew | `brew install redblue/tap/redblue` |
| Linux | apt | `apt install redblue` |
| Linux | yum | `yum install redblue` |
| Windows | winget | `winget install redblue` |
| Windows | Chocolatey | `choco install redblue` |

### CI/CD Pipeline Summary

```
Push to main
    │
    ▼
┌─────────────────────────────────────────────┐
│  CI Pipeline (GitHub Actions)                │
│  ├── lint (rustfmt, clippy)                  │
│  ├── test (cargo test)                      │
│  ├── build (cross-platform)                 │
│  └── coverage (codecov)                     │
└─────────────────────────────────────────────┘
    │ (if all pass)
    ▼
Merge PR
    │
    ▼
┌─────────────────────────────────────────────┐
│  Release Pipeline (on git tag v*)            │
│  ├── build (all platforms)                  │
│  ├── test (integration tests)               │
│  ├── create release (GitHub)                │
│  ├── upload artifacts                       │
│  └── publish (crates.io, npm, etc)          │
└─────────────────────────────────────────────┘
```

### GitHub Workflows Reference

#### Complete CI Pipeline
```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          components: rustfmt, clippy
      - run: rustfmt --check src/**/*.rs
      - run: cargo clippy -- -D warnings

  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - run: cargo test --all-features
      - run: cargo test --doc

  repl-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - run: cargo test --package repl --all-features
      - name: Test REPL commands
        run: |
          echo 'say "Hello from REPL"' | cargo run --package repl

  build:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
      - run: cargo build --release
      - uses: actions/upload-artifact@v4
        with:
          name: redblue-${{ matrix.os }}
          path: target/release/redblue

  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          components: llvm-tools-preview
      - run: cargo install cargo-llvm-cov
      - run: cargo llvm-cov --lcov --output-path lcov.info
      - uses: codecov/codecov-action@v3
        with:
          files: lcov.info
```

### Post-Release Checklist

#### Immediately After Release
- [ ] Verify all assets uploaded to GitHub Release
- [ ] Test installation on clean machine (Windows, macOS, Linux)
- [ ] Verify package registry entries (crates.io, npm)
- [ ] Update website documentation links
- [ ] Post announcement to GitHub Discussions
- [ ] Update issue labels for new version

#### Within 24 Hours
- [ ] Monitor for bug reports in Issues
- [ ] Respond to community questions
- [ ] Monitor CI/CD for any regressions
- [ ] Update Twitter/social media
- [ ] Respond to community feedback

#### Within 1 Week
- [ ] Address critical bugs from release
- [ ] Review user feedback and suggestions
- [ ] Plan hotfix releases if needed
- [ ] Update documentation based on user questions

---

## Next Steps

1. **Week 0**: Set up GitHub repo, CI/CD, community files (README, CONTRIBUTING)
2. **Week 1**: Form team (2-5 people recommended), begin Phase 1
3. **Week 2**: Finalize language design with examples
4. **Week 3**: Get feedback from target users (beginners)
5. **Week 4**: Lock syntax, write formal grammar
6. **Week 5**: Begin compiler implementation
7. **Week 9**: First working "Hello World" compilation
8. **Week 48**: v1.0.0 release

---

*Last updated: March 2026*
*Version: 1.1*
