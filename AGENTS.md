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
├── vm.rs            # Virtual machine
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
2. Implement in `vm.rs` → `call_builtin()`
3. Add tests

### New Statement Type
1. Add variant to `parser.rs` → `Statement` enum
2. Implement `parse_*()` method
3. Handle in `vm.rs` → `execute_statement()`

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

## Resources

- [Language Specification](SPEC.md)
- [Grammar](docs/GRAMMAR.md)
- [Philosophy](PHILOSOPHY.md)
- [Roadmap](ROADMAP.md)
