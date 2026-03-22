# REDBLUE PROGRAMMING LANGUAGE
## Portable Handout - Complete Project Transfer

**Version**: 0.1.0  
**Created**: March 2026  
**Status**: Phase 1-6 Complete

---

## QUICK START

### On This Computer (Already Has Files)
```bash
cd redblue
cargo build --release
cargo run
```

### On Another Computer

#### Option 1: Clone from GitHub
```bash
git clone https://github.com/YOUR_USERNAME/redblue.git
cd redblue
cargo build --release
cargo run
```

#### Option 2: Use This Handout
1. Copy the entire `redblue/` folder to the other computer
2. Install Rust: https://rustup.rs
3. Run: `cargo build --release`
4. Run: `cargo run`

---

## WHAT IS REDBLUE?

Redblue is a programming language designed to be as readable as plain English.

**Example Code:**
```redblue
say "Hello, World!"

set name to "World"
if name is "World"
    say "Hello, {name}!"
end

to add(a, b)
    give back a + b
end
```

---

## PROJECT STRUCTURE

```
redblue/
├── src/                    # Rust source code
│   ├── lib.rs             # Main library
│   ├── main.rs            # CLI entry
│   ├── lexer.rs           # Tokenizer
│   ├── parser.rs          # AST builder
│   ├── analyzer.rs        # Semantic analysis
│   ├── vm.rs              # Virtual machine
│   ├── value.rs           # Runtime values
│   ├── error.rs           # Error types
│   ├── repl/              # REPL implementation
│   ├── stdlib/            # Standard library
│   └── testing/           # Testing framework
├── tests/                  # Test files
├── examples/               # Example programs (.rb)
├── docs/                  # Documentation
├── tooling/                # IDE extensions
├── Cargo.toml             # Rust project config
├── AGENTS.md             # Developer guide (THIS FILE)
├── SPEC.md               # Language specification
├── PHILOSOPHY.md         # Design principles
├── ROADMAP.md            # Project roadmap
└── README.md             # Project readme
```

---

## BUILD COMMANDS

```bash
# Build the project
cargo build

# Build in release mode (faster)
cargo build --release

# Run the REPL
cargo run

# Run a Redblue file
cargo run -- examples/hello.rb

# Run all tests
cargo test

# Run specific test
cargo test test_name

# Check code (lint)
cargo check

# Format code
cargo fmt

# Lint with clippy
cargo clippy

# Clean build
cargo clean && cargo build
```

---

## STANDARD LIBRARY MODULES

### text - String Operations
```redblue
set upper to text.uppercase("hello")  // "HELLO"
set lower to text.lowercase("HELLO")  // "hello"
set parts to text.split("a,b,c", by ",")  // ["a", "b", "c"]
```

### math - Math Functions
```redblue
set sqrt2 to math.sqrt(2)
set rand to math.random(1, 100)
set rounded to math.round(3.7)
```

### files - File I/O
```redblue
set content to files.read("data.txt")
files.write("output.txt", "Hello!")
if files.exists("config.txt")
    say "Config found"
end
```

### formats - JSON/CSV
```redblue
set data to formats.parse_json('{"name": "Alice"}')
set json to formats.to_json(data)
```

### list - List Operations
```redblue
set doubled to list.map([1, 2, 3], to (x) give back x * 2)
set evens to list.filter([1, 2, 3, 4], to (x) give back x mod 2 is 0)
```

### console - I/O
```redblue
say "Hello!"           // Print with newline
print "Hello!"         // Print without newline
set input to ask "Your name?"  // Get user input
```

---

## ADDING NEW FEATURES

### Add a New Keyword

1. Edit `src/lexer.rs`:
```rust
// Add to TokenKind enum
pub enum TokenKind {
    // ... existing variants
    MyNewKeyword,
}

// Add to keyword() method
fn keyword(&self, ident: &str) -> TokenKind {
    match ident {
        // ...
        "mynewkeyword" => TokenKind::MyNewKeyword,
        // ...
    }
}
```

2. Edit `src/parser.rs`:
```rust
// Handle in parse_statement()
TokenKind::MyNewKeyword => {
    // Parse the new statement
}
```

### Add a Standard Library Function

1. Edit `src/stdlib/mod.rs`:
```rust
pub fn call_module_function(module: &str, function: &str, args: Vec<Value>) -> Result<Value> {
    match module {
        "mymodule" => mymodule::call(function, args),
        // ...
    }
}
```

2. Create `src/stdlib/mymodule.rs`:
```rust
use crate::error::{Error, Result};
use crate::value::Value;

pub fn call(function: &str, args: Vec<Value>) -> Result<Value> {
    match function {
        "my_function" => my_function(args),
        _ => Err(Error::Runtime(format!("Unknown function '{}'", function))),
    }
}

fn my_function(args: Vec<Value>) -> Result<Value> {
    // Implementation
    Ok(Value::Number(42.0))
}
```

---

## TESTING

### Run Tests
```bash
cargo test
```

### Write Tests
Add tests in `tests/` directory:
```rust
#[test]
fn test_my_feature() {
    let result = run_source("set x to 10");
    assert!(result.is_ok());
}
```

### Redblue Test Syntax
```redblue
// test "my test case"
//   set x to 10
//   if x is 10
//       say "Pass"
//   end
// end
```

---

## GIT COMMANDS

```bash
# Initialize (if not done)
git init

# Add all files
git add .

# Commit
git commit -m "Description of changes"

# Create branch
git checkout -b feature/my-feature

# Switch branch
git checkout main

# Merge branch
git merge feature/my-feature

# Push to GitHub
git push origin main

# Pull from GitHub
git pull origin main
```

---

## INSTALLING RUST

If Rust is not installed:

1. Go to: https://rustup.rs
2. Download and run the installer
3. Restart terminal
4. Verify: `rustc --version`

---

## COMMON ERRORS

### "command not found: cargo"
Rust is not installed. Install from https://rustup.rs

### "could not find Cargo.toml"
Not in the redblue directory. Run: `cd redblue`

### "linker command failed"
Missing build tools. On Windows, install Visual Studio Build Tools.

---

## RESOURCES

- Rust Documentation: https://doc.rust-lang.org/
- Cargo Book: https://doc.rust-lang.org/cargo/
- GitHub Repository: https://github.com/YOUR_USERNAME/redblue

---

## PHASE ROADMAP

| Phase | Description | Status |
|-------|-------------|--------|
| 1-2 | Design & Specification | ✅ Complete |
| 3 | Compiler (Lexer, Parser, VM) | ✅ Complete |
| 4 | Standard Library | ✅ Complete |
| 5 | REPL | ✅ Complete |
| 6 | Testing Framework | ✅ Complete |
| 7 | Documentation | 🔄 In Progress |
| 8 | Performance | 📋 Planned |
| 9 | IDE Extension | 📋 Planned |
| 10 | Package Manager | 📋 Planned |

---

## CONTACT

- GitHub Issues: Report bugs here
- GitHub Discussions: Ask questions
- Discord: Community chat

---

*This handout was auto-generated for Redblue v0.1.0*
