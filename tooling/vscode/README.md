# Redblue VS Code Extension

VS Code extension for Redblue programming language.

## Features

### Syntax & Editing
- Syntax highlighting for `.rb` files
- Automatic indentation
- Bracket matching
- Code folding
- Auto-close brackets and quotes

### IntelliSense
- Autocomplete for keywords
- Autocomplete for built-in functions
- Parameter hints for functions
- Quick info on hover

### Navigation
- Go to definition
- Find all references
- Peek definition
- Go to symbol in file

### Debugging
- Breakpoint support
- Variable inspection
- Call stack
- Step through execution

### REPL Integration
- Open REPL panel in VS Code
- Send code to REPL
- View REPL output inline

### Project Support
- Create new Redblue project
- Run scripts from VS Code
- Debug scripts from VS Code

## Installation

1. Open VS Code
2. Press `Ctrl+P` / `Cmd+P`
3. Type `ext install redblue-lang.redblue-vscode`
4. Restart VS Code

## Configuration

```json
{
    "redblue.languageServer": true,
    "redblue.formatter": "rb fmt",
    "redblue.replPath": "/usr/local/bin/rb",
    "redblue.trace.server": "off"
}
```

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+Shift+R` | Open REPL |
| `Ctrl+Shift+B` | Run current file |
| `Ctrl+Shift+D` | Debug current file |
| `F5` | Start debugging |

## Project Structure

```
tooling/vscode/
├── src/
│   ├── extension.ts      # Main extension entry
│   ├── lexer.ts          # Syntax highlighting
│   ├── completer.ts      # Autocomplete
│   ├── hover.ts          # Hover information
│   ├── definition.ts     # Go to definition
│   ├── formatter.ts      # Code formatting
│   ├── debugger/         # Debug adapter
│   └── repl/             # REPL panel
├── syntax/
│   └── redblue.tmLanguage.json
├── package.json
└── README.md
```

## Development

```bash
# Install dependencies
npm install

# Run in development mode
npm run watch

# Package for release
npm run package
```

## Publishing

```bash
# Login to VS Code marketplace
npx vsce login

# Publish
npx vsce publish
```
