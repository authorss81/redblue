# Redblue

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.70+-dea584.svg?logo=rust)](https://www.rust-lang.org)

**Redblue** is a programming language designed to be as readable as plain English. Write code that reads like natural sentences while maintaining the power of modern programming languages.

```redblue
// Hello World
say "Hello, World!"

// Functions - readable and clear
to greet(name)
    say "Hello, {name}!"
end

// Control flow - no cryptic symbols
if count is greater than 10
    say "Large count!"
else
    say "Small count!"
end

// Objects - simple and intuitive
object Person
    has name
    has age
    
    to introduce()
        say "I'm {this.name}"
    end
end
```

## Why Redblue?

| Feature | Redblue | Python | JavaScript |
|---------|---------|--------|------------|
| Keywords | 32 | 35 | 50+ |
| Readability | ★★★★★ | ★★★★☆ | ★★★☆☆ |
| Plain English syntax | Yes | No | No |

- **32 keywords** vs Python's 35 and JavaScript's 50+
- Code reads like instructions you'd give a person
- No cryptic symbols: `end` not `}`, `each` not `for...of`

## Installation

### Windows
1. Download Rust: https://win.rustup.rs-x86_64.exe
2. Run the installer
3. Restart terminal

### macOS / Linux
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Build from Source
```bash
git clone https://github.com/redblue-lang/redblue.git
cd redblue
cargo build --release
./target/release/rb --version
```

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
```

### Examples

```redblue
// variables.redblue
set name to "Alice"
set age to 30
set fruits to ["apple", "banana", "orange"]

say "Hello, {name}!"
say "Age: {age}"
say "First fruit: {fruits at 0}"
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

## Standard Library

The full list, with what each function does, is [SPEC.md](SPEC.md) §Standard
Library. Every module name below is callable as `module.function(..)`.

### Text
```redblue
text.uppercase(text)     // Text as uppercase
text.lowercase(text)     // Text as lowercase
text.trim(text)          // Text without leading/trailing space
text.split(text, sep)    // Split text into a list by a separator
text.join(list, sep)     // Join a list into text with a separator
text.length(text)        // Length of text
```

### Math
```redblue
math.abs(n)             // Absolute value
math.floor(n)           // Round down
math.ceil(n)            // Round up
math.round(n)           // Round to the nearest whole number
math.sqrt(n)            // Square root
math.random(min, max)   // A number in a range
```

### Files
```redblue
files.read(path)      // Read file as text
files.write(path, content)
files.append(path, content)
files.exists(path)   // Check if file exists
files.lines(path)    // Read as list of lines
files.delete(path)
```

### List
```redblue
list.length(list)     // Number of elements
```

### Time
```redblue
time.now()           // Get current timestamp
time.sleep(seconds) // Sleep for N seconds
time.format(ts, format)
time.unix("YYYY-MM-DD HH:MM:SS")
```

### Network
```redblue
network.get(url)     // HTTP GET
network.post(url, body)
```

### Formats
```redblue
formats.parse_json(text)     // Parse JSON to record
formats.to_json(value)       // Convert value to JSON text
formats.parse_csv(text)      // Parse CSV to list of lists

json.parse(text)     // Parse JSON to record — the same function
json.stringify(value)
csv.parse(text)     // Parse CSV to list of lists — the same function
```

### Console
```redblue
console.log(text)    // Print with newline
console.error(text)  // Print an error with newline
console.clear()      // Clear the screen
```

Every text, math and list function above is also a bare builtin of the same
name, so `uppercase("hi")` and `text.uppercase("hi")` reach one function.

## Project Structure

```
redblue/
├── src/
│   ├── lib.rs           # Main library
│   ├── main.rs          # CLI entry
│   ├── lexer.rs         # Tokenizer
│   ├── parser.rs        # AST builder
│   ├── analyzer.rs      # Semantic analysis
│   ├── vm.rs            # Virtual machine
│   ├── stdlib.rs        # Standard library
│   ├── formatter.rs     # Code formatter
│   ├── linter.rs        # Code linter
│   ├── repl/            # REPL
│   └── testing/         # Test harness
├── examples/            # Example programs
└── modules/             # Importable modules
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
- [ ] IDE extensions (in progress)
- [ ] Package manager (planned)

## License

MIT License - see [LICENSE](LICENSE)

---

*Code should read like English prose.*