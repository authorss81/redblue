# AGENTS.md - Redblue Programming Language

This file contains instructions for agentic coding agents working on the Redblue project.

## Project Overview

**Redblue** is a programming language designed to be as readable as plain English while maintaining the power of world-class languages.

- **Language Syntax**: Plain English, e.g., `if count is greater than 10 then say "Hello"`
- **File Extension**: `.rb`
- **Implementation**: Rust
- **Status**: Phase 3-5 (Compiler core done, expanding stdlib)

---

## Build Commands

```bash
# Build the project
cd redblue
cargo build

# Run in release mode
cargo build --release

# Run tests
cargo test

# Run specific test
cargo test test_name

# Run with example
cargo run --example hello

# Run the REPL
cargo run

# Run a Redblue file
cargo run -- test.rb

# Format code
cargo fmt

# Lint code
cargo clippy

# Full check (all of the above)
cargo check --all-targets
```

---

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

4. **Imports**: Group by std, external, local
   ```rust
   use std::collections::HashMap;
   
   use crate::error::{Error, Result};
   use crate::lexer::Lexer;
   ```

5. **Error Handling**:
   - Use `Result<T, Error>` for fallible operations
   - Propagate errors with `?` operator
   - Use descriptive error messages
   - Never silently ignore errors with `_`

6. **Documentation**:
   - Document public functions with `///`
   - Keep docs concise but complete
   - Include examples for complex functions

### Redblue Language Code

1. **Variables**: Use descriptive names
   ```redblue
   // Good
   set user_name to "Alice"
   
   // Bad
   set x to "Alice"
   ```

2. **Functions**: Verb phrases that describe action
   ```redblue
   to calculate_total(items)
   to fetch_user_data(url)
   ```

3. **Control Flow**: Readable conditionals
   ```redblue
   // Good
   if user age is greater than 18
   
   // Acceptable
   if age > 18
   ```

4. **Comments**: Explain why, not what
   ```redblue
   // Using binary search for O(log n) performance
   ```

---

## Architecture

### Compiler Pipeline

```
Source Code (.rb)
    │
    ▼
┌─────────┐
│  Lexer  │  Tokenizes source into tokens
└────┬────┘
     │
     ▼
┌─────────┐
│ Parser  │  Builds AST from tokens
└────┬────┘
     │
     ▼
┌──────────┐
│ Analyzer │  Semantic analysis, scope checking
└────┬─────┘
     │
     ▼
┌─────┴────┐
│    VM    │  Executes AST
└──────────┘
```

### Source Structure

```
redblue/
├── src/
│   ├── lib.rs              # Main library entry
│   ├── main.rs              # CLI entry point
│   ├── error.rs             # Error types
│   ├── value.rs             # Runtime values
│   ├── lexer.rs             # Tokenizer
│   ├── parser.rs            # AST builder
│   ├── analyzer.rs          # Semantic analysis
│   ├── vm.rs                # Virtual machine
│   ├── repl/                # REPL implementation
│   │   ├── mod.rs
│   │   ├── commands.rs
│   │   ├── completer.rs
│   │   └── history.rs
│   └── stdlib/              # Standard library
│       ├── mod.rs
│       ├── text.rs          # String operations
│       ├── math.rs          # Math functions
│       ├── files.rs         # File I/O
│       ├── network.rs       # HTTP/networking
│       ├── formats.rs        # JSON/CSV parsing
│       ├── list.rs          # List operations
│       └── console.rs       # I/O utilities
├── tests/                   # Integration tests
├── examples/                # Example .rb files
├── tooling/                  # IDE/REPL tooling
│   ├── repl/
│   └── vscode/
├── docs/
│   └── GRAMMAR.md          # Language grammar
├── SPEC.md                 # Language specification
├── PHILOSOPHY.md           # Design principles
└── ROADMAP.md              # Project roadmap
```

---

## Key Decisions

### Type System

- **Dynamic typing** with optional type annotations
- **Value types**: `number`, `text`, `yes/no` (boolean), `nothing`
- **Complex types**: `list`, `record`, `object`
- Use `Value` enum for runtime values

### Error Handling

- Use `Result<T, Error>` for fallible operations
- Error enum variants: `Lexer`, `Parser`, `Analyzer`, `Runtime`, `Io`
- Human-friendly error messages with context

### Standard Library Organization

Each module is in `src/stdlib/<name>.rs`:

```rust
// Module function signature
pub fn call(function: &str, args: Vec<Value>) -> Result<Value> {
    match function {
        "function_name" => function_impl(args),
        _ => Err(Error::Runtime(format!("Unknown function '{}'", function))),
    }
}
```

---

## Adding New Features

### Adding a New Keyword

1. Add to `lexer.rs` → `TokenKind` enum
2. Add to `keyword()` method in `Lexer`
3. Handle in `parser.rs` → `parse_statement()` or `parse_expression()`
4. Add tests in `tests/`

### Adding a Standard Library Function

1. Create or edit module in `src/stdlib/`
2. Add `pub fn call()` function
3. Export in `src/stdlib/mod.rs`
4. Update `Vm::call()` in `vm.rs`
5. Add examples in `examples/`

### Adding a New Statement Type

1. Add variant to `parser.rs` → `Statement` enum
2. Implement `parse_*()` method in `Parser`
3. Handle in `vm.rs` → `execute_statement()`
4. Add tests

---

## Testing

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run doc tests
cargo test --doc

# Run specific test
cargo test test_name

# Run integration tests
cargo test --test '*'

# Run with coverage
cargo install cargo-llvm-cov
cargo llvm-cov
```

---

## Git Workflow

1. Create feature branch: `git checkout -b feature/my-feature`
2. Make changes, commit with conventional commits
3. Push: `git push origin feature/my-feature`
4. Create PR on GitHub

### Commit Message Format

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

Types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `chore`

Examples:
```
feat(parser): add pattern matching support
fix(lexer): handle unicode identifiers correctly
docs(readme): update installation instructions
test(vm): add tests for async execution
```

---

## Performance Considerations

- Lexer/Parser: O(n) where n = source length
- VM: Stack-based, no JIT yet
- Memory: Reference counting GC (planned improvements)

---

## Known Limitations

- No async/await yet (planned for Phase 5+)
- No generics yet (planned)
- No module system yet (planned)
- Limited standard library (expanding)
- No bytecode compilation (interpreter only)

---

## Resources

- [Language Specification](SPEC.md)
- [Grammar](docs/GRAMMAR.md)
- [Philosophy](PHILOSOPHY.md)
- [Roadmap](ROADMAP.md)

---

*Last updated: March 2026*
