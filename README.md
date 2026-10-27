# Redblue

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.70+-dea584.svg?logo=rust)](https://www.rust-lang.org)
[![CI](https://github.com/authorss81/redblue/actions/workflows/ci.yml/badge.svg)](https://github.com/authorss81/redblue/actions/workflows/ci.yml)

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

**Two engines that must agree.** Programs run on the tree-walking interpreter or compile to versioned bytecode (`.rbc`). A differential harness runs 370 programs on both and compares outcomes — if the engines ever disagree, it's a bug, and there's a test proving it.

**Errors you can read.** Every error names its kind and points at the line:

```
Error: AnalyzerError: Unknown variable 'y'
  --> err.rb:2:1
2 | say y
  | ^
```

## Quick Start

### Build

```bash
git clone https://github.com/authorss81/redblue.git
cd redblue
cargo build --release
# the binary is at ./target/release/rb
```

**The binary is not on your PATH after building.** `cargo build` puts it in
`target/release/`, and `rb` alone will not be found. Either invoke it by path:

```bash
./target/release/rb          # Linux / macOS
.\target\release\rb.exe      # Windows
```

or add the directory to your PATH once:

```bash
# Linux / macOS — quote the path, it contains spaces in a default checkout
export PATH="$HOME/3D Objects/a/rb/redblue/target/release:$PATH"
```

```bash
# Windows PowerShell
$env:PATH += ";$env:USERPROFILE\3D Objects\a\rb\redblue\target\release"
```

Then `rb`, `rb help` and so on work from any directory.

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
// variables.rb
set name to "Alice"
set age to 30
set fruits to ["apple", "banana", "orange"]

say name
say age
say fruits
```

```redblue
// loops.rb
for each number from 1 to 5
    say number
end

set items to ["a", "b", "c"]
for each item in items
    say item
end
```

```redblue
// files.rb
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
| `x is 5`, `x is not 5` | equality (also `==`, `!=`) |
| `x is greater than 4` | comparison (also `<`, `<=`, `>`, `>=`) |
| `a and b`, `a or b`, `not x` | logic |
| `list[0]` | indexing (out of bounds is a clean error) |
| `7 mod 3` | remainder |
| `to name(params) … end` | function declaration |
| `to (x) … end` | function literal |
| `object Name … has x … to f() … end` | object with fields and methods |
| `Child extends Parent` | inheritance |
| `test "name" … end` | test block |
| `expect a to be b` | assertion |
| `try … catch [err] … finally … end` | catchable errors |
| `import Mod`, `import a, b as c` | modules |
| `constant PI to 3.14159` | module-level constant |

Full grammar: [`docs/GRAMMAR.md`](docs/GRAMMAR.md). Design rationale: [`PHILOSOPHY.md`](PHILOSOPHY.md). Specification: [`SPEC.md`](SPEC.md). Library policy — what is written in Redblue, what is a Rust binding, and what is not built: [`LIBRARIES.md`](LIBRARIES.md).

## Standard Library

Before adding a module or builtin, read [`LIBRARIES.md`](LIBRARIES.md): it is the
standing decision on whether a capability is written in Redblue, bound to a Rust
crate, or not built at all.

### Files
```
files.read(path)      // Read file as text
files.write(path, content)
files.append(path, content)
files.exists(path)   // Check if file exists
files.lines(path)    // Read as list of lines
files.delete(path)
files.copy(from, to)     // Copy a file
files.rename(from, to)   // Rename it, or move it
```

### Bytes
```
bytes.from_text(text)   // Text to a list of byte values (0 to 255)
bytes.text(bytes)       // A byte list back to text
bytes.write(path, bytes) // Write a byte list to a file
```

### Time
```
time.now()             // Get current timestamp
time.sleep(seconds) // Sleep for N seconds
time.format(timestamp_or_date, "format")
time.parse("YYYY-MM-DDTHH:MM:SS+HH:MM")         // ISO-8601 text to a date
time.parse("29/02/2024", "%d/%m/%Y")            // text in a chosen layout
time.unix("YYYY-MM-DD HH:MM:SS")
time.from_unix(seconds)                        // Seconds to a date, read as UTC
time.date(year, month, day)                    // A date at midnight
time.datetime(year, month, day, hour, minute, second)
time.duration(seconds)                         // A signed span of time
time.add_years(date, years)                    // Calendar arithmetic
time.add_months(date, months)                  // Clamps at month end
time.add_days(date, days)                      // Fixed 24-hour days
time.add_hours(date, hours)
time.add_minutes(date, minutes)
time.add_seconds(date, seconds)
// dates compare with < <= > >=, subtract to a duration, and add a duration
```

### Sys
```
sys.argv()             // Arguments after the program path (empty outside `rb run`)
```

### Network
```
network.get(url)     // HTTP GET
network.post(url, body)
```

### JSON and CSV
```
json.parse(text)     // Parse JSON to record
json.stringify(value)
csv.parse(text)     // Parse CSV to list of lists
```

### Math
```
PI, E                          // Constants, not module members: no math.PI
math.abs(x), math.floor(x), math.ceil(x)
math.round(x), math.sqrt(x), math.pow(x, y)
math.sin(x), math.cos(x), math.tan(x)
math.log(x), math.exp(x)
// every one of them is also spelled flat: abs(x), sqrt(x), ...
// math.random is not implemented, and there is no math.random
```

### Text
```
text.uppercase("text"), text.lowercase("text"), text.trim("text")
text.split("text", by), text.join(list, by)
text.contains("text", sub)
text.starts_with("text", prefix), text.ends_with("text", suffix)
text.replace("text", from, to)
text.length("text")      // characters, not bytes: text.length("héllo") is 5
// every one of them is also spelled flat: uppercase("text"), split("text", by), ...
```

### Console
```
say "Hello"                      // print with a newline
print "Hello"                    // print without one
console.log("Hello")             // `say` under its module spelling
console.error("Could not read")  // diagnostics go to stderr
console.clear()                  // clear the screen
set name to input("Your name? ")  // read one line from the terminal
```

### List
```
list.map(xs, to (x) give back x * 2)  // every member, through a function
list.length([1, 2, 3])                 // 3
length(xs), push(xs, x)                // flat list builtins
```

### Regex
```
regex.match(text, pattern)             // a match record, or nothing
regex.is_match(text, pattern)          // yes/no, searching anywhere
regex.capture(text, pattern, group)    // a captured text, or nothing
regex.replace(text, pattern, "user-$1")// every match, with capture references
regex.split(text, pattern)             // the text between matches
// text.split keeps its literal separator; regex.split is the pattern form.
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
