# Redblue

[![CI](https://github.com/redblue/lang/actions/workflows/ci.yml/badge.svg)](https://github.com/redblue/lang/actions/workflows/ci.yml)
[![Release](https://github.com/redblue/lang/actions/workflows/release.yml/badge.svg)](https://github.com/redblue/lang/releases)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Chat](https://img.shields.io/badge/Discord-Join-blue)](https://discord.gg/redblue)

**Redblue** is a programming language designed to be as readable as plain English while maintaining the power and capability of world-class languages.

```redblue
// Hello World
say "Hello, World!"

// Functions
to greet(name)
    say "Hello, {name}!"
end

// Objects
object Person
    has name
    has age
    
    to introduce()
        say "I'm {this.name}"
    end
end
```

## Philosophy

Redblue is built on four core principles:

1. **Plain English First** - Code reads like natural English sentences
2. **Zero Cognitive Overhead** - Minimal keywords, maximum clarity
3. **Minimal Keywords** - ~32 keywords (vs Python's 35, JavaScript's 50+)
4. **Maximum Capability** - Full support for OOP, functional, async, and more

## Installation

### Pre-built Binaries

Download from the [latest release](https://github.com/redblue/lang/releases/latest):

```bash
# Linux/macOS
curl -L https://github.com/redblue/lang/releases/latest/redblue-linux-x86_64 -o redblue
chmod +x redblue
sudo mv redblue /usr/local/bin/

# Windows
# Download redblue-windows-x86_64.exe from releases
```

### Build from Source

```bash
# Requires Rust 1.70+
git clone https://github.com/redblue/lang.git
cd lang
cargo build --release
./target/release/redblue --version
```

### Package Managers

```bash
# Homebrew (macOS/Linux)
brew install redblue/tap/redblue

# Cargo (all platforms)
cargo install redblue
```

## Quick Start

### Your First Program

Create a file `hello.rb`:

```redblue
say "Hello, World!"
```

Run it:

```bash
redblue run hello.rb
```

### Variables

```redblue
set name to "Alice"
set age to 30
set is_member to yes
set items to [1, 2, 3]
```

### Control Flow

```redblue
if age is greater than 18
    say "Welcome!"
else
    say "Sorry, too young."
end
```

### Functions

```redblue
to add(a, b)
    give back a + b
end

set result to add(5, 3)
say result  // 8
```

### Objects

```redblue
object Dog
    has name
    has breed
    
    to bark()
        say "Woof!"
    end
end

set dog to new Dog
set name of dog to "Buddy"
dog.bark()
```

## Documentation

- [Getting Started](docs/getting-started.md)
- [Language Guide](docs/language-guide/)
- [Standard Library](docs/stdlib/)
- [Examples](examples/)
- [FAQ](docs/faq.md)

## Comparison

| Feature | Redblue | Python | JavaScript |
|---------|-----------|--------|------------|
| Keywords | 32 | 35 | 50+ |
| Readability | ★★★★★ | ★★★★☆ | ★★★☆☆ |
| OOP | Yes | Yes | Yes |
| Functional | Yes | Yes | Yes |
| Async | Yes | Yes | Yes |
| Type Safety | Optional | Dynamic | Dynamic |

## Development

### Prerequisites

- Rust 1.70+
- Git

### Setup

```bash
git clone https://github.com/redblue/lang.git
cd lang
cargo build
cargo test
```

### Project Structure

```
redblue/
├── src/              # Compiler source code
├── tests/            # Test suite
├── docs/             # Documentation
├── examples/         # Example programs
└── tooling/          # IDE extensions, tools
```

### Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Status

Redblue is in **early development**. The following milestones have been reached:

- [x] Language design complete
- [x] Formal grammar specification
- [x] Reference compiler (basic)
- [ ] Standard library (in progress)
- [ ] Production tooling

See the [roadmap](ROADMAP.md) for full details.

## Community

- [Discord](https://discord.gg/redblue) - Chat with the community
- [GitHub Discussions](https://github.com/redblue/lang/discussions) - Q&A and ideas
- [Twitter](https://twitter.com/redblue) - News and updates

## License

Redblue is open source software released under the [MIT License](LICENSE).

---

*Making programming accessible to everyone, one plain English sentence at a time.*
