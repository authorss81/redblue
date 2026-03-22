# Redblue REPL

Interactive Read-Eval-Print Loop for Redblue.

## Features

- Interactive command input with multi-line support
- Variable/state persistence between inputs
- Tab completion for keywords and built-in functions
- Command history (readline)
- Built-in commands (help, quit, clear, history)
- Syntax highlighting in output
- Load and execute files
- Debug mode integration

## Commands

| Command | Description |
|---------|-------------|
| `:quit` | Exit the REPL |
| `:exit` | Exit the REPL |
| `:clear` | Clear the screen |
| `:history` | Show command history |
| `:load <file>` | Load and run a file |
| `:save <file>` | Save current session |
| `:vars` | Show all defined variables |
| `:funcs` | Show all defined functions |
| `:help` | Show help information |

## Usage

```bash
# Run REPL
rb

# Run script
rb run script.rb

# Evaluate expression
rb eval "say 'Hello, World!'"
```

## Architecture

```
tooling/repl/
├── src/
│   ├── main.rs           # REPL entry point
│   ├── lexer.rs          # REPL-specific lexer
│   ├── evaluator.rs      # Direct evaluation (no compilation)
│   ├── completer.rs      # Tab completion
│   ├── history.rs        # Command history (readline)
│   ├── output.rs         # Formatted output
│   └── commands.rs       # Built-in commands
└── tests/
    └── repl_test.rb
```

## Example Session

```
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
>>> :vars
x = 10
y = 20
z = 30
>>> :quit
Goodbye!
```

## Implementation Notes

- Uses `rustyline` or `reedline` for input handling
- Integrates with core compiler for parsing
- Direct AST evaluation for REPL (no full compilation)
- Maintains session state between inputs
