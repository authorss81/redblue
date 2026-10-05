# Redblue VS Code Extension

Editor support for `.rb` files. There is **no build step**: the extension is a
folder of JSON manifests, and `cargo test` is what keeps them honest.

## What is here

| File | What |
|---|---|
| `package.json` | Extension manifest: registers the `redblue` language and the grammar |
| `language-configuration.json` | Comment style, brackets, auto-closing pairs |
| `syntaxes/redblue.tmLanguage.json` | TextMate grammar — keywords, constants, strings, numbers, comments, operators |

Open this folder in VS Code (**File → Open Folder**) and `.rb` files highlight.
Nothing is compiled, transpiled or fetched.

## The grammar is generated, not hand-written

`syntaxes/redblue.tmLanguage.json` is rendered by the compiler from
`KEYWORDS` in `src/lexer.rs` — the same table the lexer matches words through,
so highlighting cannot drift from the language. Regenerate it with:

```bash
rb grammar > tooling/vscode/syntaxes/redblue.tmLanguage.json
```

Two tests in `tests/tooling_grammar_test.rs` enforce this: one fails if the
checked-in file differs from the generator, the other fails if the file stops
being valid JSON.

Comments start with `//` — the marker the lexer skips, and the one
`language-configuration.json` hands `Ctrl+/`. `#` is not a comment in Redblue;
`rb run` rejects it as an unexpected character. The grammar's pattern order is
part of that: the string rule comes before the comment rule so a `//` inside a
string stays string text, and the comment rule comes before the operator rule so
a trailing `//` is not highlighted as a division.

## Diagnostics

Errors reach an editor through the command line, which needs no Node
toolchain:

```bash
rb diagnostics path/to/file.rb
```

It prints a JSON array of `{line, column, severity, message}` objects — the
spanned lexer, parser and analyzer errors plus the linter's findings — and
exits non-zero when any of them is an error, so it doubles as a CI check.
Lines and columns are 1-based and count **characters**, so a caret drawn under
the column lines up on a line containing emoji or accented text.

This is the transport a language server would speak; a full LSP (stdin/stdout
JSON-RPC) is not implemented.

## Not implemented

IntelliSense, go-to-definition, the debugger, and the REPL panel are described
in the language roadmap but do not exist yet. The REPL itself lives in
`src/repl/` and is reached with `rb`.