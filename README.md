# Redblue

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.70+-dea584.svg?logo=rust)](https://www.rust-lang.org)

**Redblue** is a programming language designed to be as readable as plain English. Write code that reads like natural sentences while maintaining the power of modern programming languages.

```redblue
try
    say 1 / 0
catch
    say "recovered"
end

test "division by zero is caught, not fatal"
    set caught to no
    try
        say 1 / 0
    catch
        set caught to yes
    end
    expect caught to be yes
end
```

Errors are values you catch. Tests live beside the code and must assert something that can fail. That is the whole philosophy in fifteen lines.

## What it can do

**Readable control flow.** No braces, no semicolons — blocks close with `end`:

```redblue
set total to 0
for each n in [1, 2, 3]
    if n is 2 then
        skip
    end
    set total to total + n
end
say total
```

**Errors that don't lie.** Every error carries a source span, failures are catchable, and a bare `catch` runs even with no name binding:

```redblue
set empty to []
try
    set x to empty[0]
catch err
    say err
end
```

**Testing built in, not bolted on.** Test blocks use `expect ... to be ...` and every test must be able to fail:

```redblue
test "empty list index is an error"
    set empty to []
    try
        set x to empty[0]
    catch error
        set caught to yes
    end
    expect caught to be yes
end
```

**Values that behave.** Numbers are 64-bit floats and always finite — `1 / 0` is a clean runtime error, never infinity. Text is fully Unicode. Records keep insertion order. Objects support inheritance and method dispatch, closures capture lexically.

**Two engines that must agree.** Programs run on the tree-walking interpreter or compile to versioned bytecode (`.rbc`). A differential harness runs hundreds of programs on both and compares outcomes — if the engines ever disagree, it's a bug, and there's a test proving it.

## Quick Start

```bash
# Run a file
rb run hello.rb

# Start REPL
rb

# Format code
rb format hello.rb

# Lint code
rb lint hello.rb

# Run tests
rb test

# Compile to bytecode, inspect it, run it on the VM
rb compile hello.rb
rb dis hello.rbc
rb vm hello.rbc
```

### Examples

```redblue
// variables.redblue
set name to "Alice"
set age to 30
set fruits to ["apple", "banana", "orange"]

say name
say age
say fruits
```

```redblue
// loops.redblue
for each number from 1 to 5
    say number
end

set items to ["a", "b", "c"]
for each item in items
    say item
end
```

```redblue
// files.redblue
import files

files.write("output.txt", "Hello from Redblue!")
set content to files.read("output.txt")
say content
```

## Language at a glance

| Form | Meaning |
|---|---|
| `set x to <expr>` | assignment |
| `say <expr>` | print |
| `if … then … else … end` | conditional |
| `for each x in <list> … end` | iteration |
| `for each i from A to B … end` | range loop |
| `while … end`, `repeat … until …` | loops |
| `break` / `skip` | leave / skip one iteration |
| `unless … end` | negated conditional |
| `to name(params) … end` | function declaration |
| `to (x) … end` | function literal |
| `object Name … has x … to f() … end` | object with fields and methods |
| `Child extends Parent` | inheritance |
| `test "name" … end` | test block |
| `expect a to be b` | assertion |
| `try … catch [err] … finally … end` | catchable errors |
| `import Mod`, `import a, b as c` | modules |
| `constant PI to 3.14159` | module-level constant |

Full grammar: [`docs/GRAMMAR.md`](docs/GRAMMAR.md). Design rationale: [`PHILOSOPHY.md`](PHILOSOPHY.md). Specification: [`SPEC.md`](SPEC.md).

## Standard Library

### Files
```
files.read(path)      // Read file as text
files.write(path, content)
files.append(path, content)
files.exists(path)   // Check if file exists
files.lines(path)    // Read as list of lines
files.delete(path)
```

### Time
```
time.now()             // Get current timestamp
time.sleep(seconds) // Sleep for N seconds
time.format(timestamp, "format")
time.unix("YYYY-MM-DD HH:MM:SS")
```

### Network
```
network.get(url)     // HTTP GET
network.post(url, body)
```

### Formats
```
json.parse(text)     // Parse JSON to record
json.stringify(value)
csv.parse(text)     // Parse CSV to list of lists
```

### Math
```
PI, E                    // Constants
abs(x), floor(x), ceil(x)
round(x), sqrt(x), pow(x, y)
sin(x), cos(x), tan(x)
log(x), exp(x)
```

### Text
```
uppercase("text"), lowercase("text"), trim("text")
split("text", by), join(list, by)
contains("text", sub)
```

## Project Structure

```
redblue/
├── src/
│   ├── lib.rs           # Main library
│   ├── main.rs          # CLI entry
│   ├── lexer.rs         # Tokenizer
│   ├── parser.rs        # AST builder
│   ├── analyzer.rs      # Semantic analysis
│   ├── vm.rs            # Tree-walking interpreter
│   ├── bytecode/        # Bytecode format, compiler, disassembler, VM
│   ├── stdlib.rs        # Standard library
│   ├── formatter.rs     # Code formatter
│   ├── linter.rs        # Code linter
│   ├── lsp.rs           # Language server
│   ├── repl/            # REPL
│   └── testing/         # Test harness
├── tests/               # Rust + Redblue test suites
├── corpus/              # Differential-test programs
├── examples/            # Example programs
├── modules/             # Importable modules
└── tooling/vscode/      # Editor extension (highlighting)
```

## Development

```bash
# Build
cargo build

# Run tests
cargo test

# Format code
cargo fmt

# Lint
cargo clippy
```

## Status

- [x] Core language (lexer, parser, analyzer, VM)
- [x] Standard library (files, time, network, json, csv, math, text)
- [x] Module system
- [x] Testing framework
- [x] Formatter
- [x] Linter
- [x] Bytecode compiler + VM (`rb compile`, `rb dis`, `rb vm`)
- [ ] IDE extensions (in progress)
- [ ] Package manager (planned)
- [ ] Self-hosting: the compiler rewritten in Redblue, then compiling itself (in progress — see ROADMAP.md)

## License

MIT License - see [LICENSE](LICENSE)

---

*Code should read like English prose.*
