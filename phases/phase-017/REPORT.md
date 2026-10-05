# Phase 017 — Language server / editor support scaffolding

## Finding reproduced

```
$ ls tooling/vscode/syntaxes/redblue.tmLanguage.json tooling/vscode/package.json
ls: cannot access 'tooling/vscode/syntaxes/redblue.tmLanguage.json': No such file or directory
ls: cannot access 'tooling/vscode/package.json': No such file or directory

$ grep -c "grammar\|diagnostics" src/lib.rs
0
```

`tooling/vscode/README.md` described an extension (`src/extension.ts`,
`syntax/redblue.tmLanguage.json`, `npm run package`) that does not exist, and
nothing in `src/` produced a grammar or surfaced an error to an editor.

## Red test, before any production change

`tests/tooling_grammar_test.rs` was written first. It failed to compile for the
right reason — the tooling surface did not exist:

```
error[E0432]: unresolved import `redblue::lsp`
error[E0599]: no function or associated item named `keywords` found for struct `Lexer`
error[E0433]: failed to resolve: use of unresolved module or unlinked crate `serde_json`
```

After the change the same file passes, and it fails for the right reason if the
shipped grammar drifts (checked by appending one byte to it):

```
test shipped_grammar_file_matches_the_generator ... FAILED
  tooling/vscode/syntaxes/redblue.tmLanguage.json is stale; regenerate it with `rb grammar > ...`
test shipped_grammar_file_is_valid_json_with_a_redblue_scope ... FAILED
```

## What changed

| File | Lines | What |
|---|---|---|
| `src/lsp.rs` (new) | +261 | `Diagnostic`/`Severity`, `diagnostics()`, `diagnostics_json()`, `diagnostics_to_json()`, `escape_json()`, `keyword_pattern()`, `textmate_grammar()`, `GRAMMAR_PATH` |
| `src/lexer.rs` | +138 −110 | `KEYWORDS` table becomes the single source of truth; `Lexer::keyword()` matches through it; `Lexer::keywords()` publishes the word list |
| `src/lib.rs` | +32 | `pub mod lsp`, re-exports, `rb grammar`, `rb keywords`, `rb diagnostics <file>` |
| `src/error.rs` | +7 −1 | `Error::message()` made public — a diagnostic carries the message and the position separately |
| `tooling/vscode/syntaxes/redblue.tmLanguage.json` (new) | +16 | generated TextMate grammar, checked in, `source.redblue` |
| `tooling/vscode/package.json` (new) | +35 | language + grammar contribution, no JS entry point, no build step |
| `tooling/vscode/language-configuration.json` (new) | +21 | `//` comments, brackets, auto-closing quotes |
| `tooling/vscode/README.md` | +36 −90 | describes what exists and what does not |
| `tests/tooling_grammar_test.rs` (new) | +371 | 13 tests |
| `Cargo.toml` | +1 | `serde_json` as a **dev-dependency** for JSON assertions in tests |

`Cargo.lock` is gitignored and needed no change: `serde_json 1.0.151` is
already in the tree as a `reqwest` dependency.

**Grammar covers the keyword set** by construction: `KEYWORDS`
(`src/lexer.rs:7`) holds the words the lexer matches through *and* the ones the
grammar highlights, so a new keyword cannot be added to one and missed in the
other. `cargo test` fails if the checked-in JSON differs from the generator.

**Diagnostics surface phase-005 spanned errors** through `Error::span()`
(`src/error.rs:48`): the lexer, parser and analyzer error position is copied
into the `Diagnostic`, and linter findings are appended when the program
parses. Positions are 1-based **character** columns, the same convention
`Error::render` draws its caret with, so an editor caret lines up on a line
containing emoji or accented text.

`rb diagnostics` runs the frontend **once**: `diagnostics()` lexes and parses
the file, and the AST that clears the analyzer is the one the linter reads
(`linter::lint_program`), so nothing re-lexes or re-parses it. The command
renders the slice it was handed with `diagnostics_to_json()` rather than
re-running the frontend to format a result it already holds.
`diagnostics_json(source)` keeps the one-shot convenience form by delegating to
the two.

## Tests added

All in `tests/tooling_grammar_test.rs`. 13 tests, 4 of them `edge_*`.

| Test | Edge class covered |
|---|---|
| `grammar_covers_the_lexer_keyword_set` | both directions: no keyword missing from the grammar, no stale word in it |
| `every_listed_keyword_lexes_as_a_keyword_not_an_identifier` | malformed input / drift: a listed word that lexes as an identifier |
| `shipped_grammar_file_matches_the_generator` | resource: the shipped artefact exists and is current |
| `shipped_grammar_file_is_valid_json_with_a_redblue_scope` | malformed input: the grammar is valid JSON declaring `source.redblue` |
| `diagnostics_surface_a_spanned_lexer_error` | **failure asserted**: unterminated string → 1 Error diagnostic at 1:5 |
| `diagnostics_surface_a_spanned_parser_error` | **failure asserted**: `say x +` → parser diagnostic at 2:8 |
| `diagnostics_surface_a_spanned_analyzer_error` | **failure asserted**: undefined name → analyzer diagnostic at 2:1, naming the identifier |
| `diagnostics_report_linter_findings_as_warnings` | **failure asserted**: unused variable → exactly one `Severity::Warning` (asserted `!= Error`, so `rb diagnostics` still exits 0) at `1:1`, the fold of the linter's `0:0`; the JSON entry is `"warning"` too |
| `diagnostics_json_is_a_well_formed_array_of_located_entries` | malformed input: the JSON array parses, severity `error`, position and message match |
| `edge_empty_source_produces_no_diagnostics` | **empty**: `diagnostics("")` is empty and `diagnostics_json("") == "[]"` |
| `edge_diagnostic_columns_count_characters_not_bytes` | **unicode**: emoji + `é` before the literal; column 10, and `assert_ne!(column, 14)` so a byte offset cannot pass |
| `edge_escape_json_escapes_quotes_backslashes_controls_and_unicode` | **unicode/escapes**: `"`, `\`, `\n`, `\t`, `\u{1}`, emoji all round-trip through `escape_json` |
| `edge_diagnostic_json_escapes_a_message_containing_quotes_and_newlines` | **escapes**: a message with quotes, newline and tab stays valid JSON and reads back unchanged |

### Round-2 tests (added by the review fix pass)

| Test | Edge class covered |
|---|---|
| `grammar_highlights_the_comment_marker_the_lexer_accepts` | **drift**: the comment rule matches `//`, not `#`; `//` lexes as nothing and `#` is rejected as an unexpected character |
| `grammar_orders_strings_before_comments_and_comments_before_operators` | malformed input / precedence: string rule before comment rule (a `//` inside a string stays text), comment rule before the operator rule (a trailing `//` is not a division) |
| `shipped_language_configuration_uses_the_lexers_comment_marker` | **resource**: the `Ctrl+/` marker ships as `//` and a line the editor comments out lexes as the line blanked out |
| `unused_variable_warnings_come_out_in_a_stable_name_order` | nondeterminism: 64 diagnostics runs over four unused variables all report `alpha, beta, mid, zeta` |
| `diagnostics_lint_the_ast_they_already_parsed` | state: linting the parsed AST yields exactly the diagnostics the editor sees |

### Mandatory edge-case matrix

| Row | Status |
|---|---|
| empty / zero / "nothing" | covered — `edge_empty_source_produces_no_diagnostics` |
| singleton and boundary | covered — the unterminated-string diagnostic is the only entry (`assert_eq!(entries.len(), 1)`); the `0:0` linter position folding to the first character `1:1` is pinned by `diagnostics_report_linter_findings_as_warnings`, which also asserts the finding is the *only* diagnostic for its program |
| out of bounds | **N/A** — no indexed access exists in the new code; a diagnostic's line/column is copied from a `Span`, never indexed into a buffer |
| type mismatch | **N/A** — `diagnostics()` takes `&str` and returns `Vec<Diagnostic>`; there is no runtime value to mismatch. The nearest analogue, an unplaceable finding, is covered above |
| numeric boundary | **N/A** — no arithmetic. Numeric *literals* in the grammar are a regex string, not numbers |
| unicode and escapes | covered — `edge_diagnostic_columns_count_characters_not_bytes`, `edge_escape_json_...`, `edge_diagnostic_json_escapes_...` |
| nesting and recursion | **N/A** — `textmate_grammar()` and `diagnostics_json()` iterate flat lists; `diagnostics()` recurses through the existing `Lexer → Parser → Analyzer` pipeline, which `tests/span_test.rs` and the VM call-depth tests already cover |
| duplicate and missing keys | **N/A** — no record keys are read or written. The closest keying is the grammar's JSON object keys, asserted by `shipped_grammar_file_is_valid_json_with_a_redblue_scope` |
| malformed input | covered — `diagnostics_surface_a_spanned_lexer_error`, `..._parser_error`, `..._analyzer_error`, `diagnostics_json_is_a_well_formed_array_of_located_entries`, `shipped_grammar_file_is_valid_json_...` |
| resource and state | **N/A** — read-only, allocation-bounded by input size; nothing is written, no file handle is opened, no loop runs unbounded. The one resource guarantee this phase depends on — the lexer refusing a malformed literal instead of looping — is pre-existing and pinned by `tests/lexer_robustness_test.rs` |

## Gates

Run in the project checkout, 2026-10-05 (round 2, after the review fixes).

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test` | **341 passed, 0 failed** (18 in `tooling_grammar_test.rs`, 5 of them added in round 2) |
| `./rbops/verify.sh phase-017` | **NOT RUN** — `rbops/` is still not present in this checkout (`ls: cannot access 'rbops/verify.sh'`). Same as round 1; see FINDINGS.md §1. The dispatcher must run it |
| `examples/*.rb` (backwards compatibility) | 6/6 run clean under `rb` |

Round 2 re-ran the first three gates after fixing the review findings below;
`rbops/verify.sh` remains unavailable, so it is recorded as not run, not as
passed.

Zero new `#[ignore]`, `// skip` or `allow(clippy::` suppressions. Zero
pre-existing tests were modified, skipped or deleted.

## Invariants touched

- None. No grammar, type or file-extension change. `Value` variants, `Error`
  variants, `set … to`, `say`, `to … end` and the `.rb` extension are untouched.
- Additive only: `Lexer::keywords()`, `Error::message()`, the `lsp` module and
  three new `rb` subcommands.
- `src/lexer.rs`: `Lexer::keyword()` was a `match`; it now scans the
  `KEYWORDS` table. The word → token mapping is byte-for-byte the same table in
  the same order, and `every_listed_keyword_lexes_as_a_keyword_not_an_identifier` walks all 53 words through the real lexer to prove it.

## Known gaps / follow-ups

- `rb diagnostics` is a one-shot command, not a JSON-RPC language server →
  FINDINGS.md §5
- `src/linter.rs:46-49` reports lint findings at `0:0`; `lsp::known_position`
  folds that to `1:1` → FINDINGS.md §2
- `tooling/repl/README.md` documents a directory that does not exist; the REPL
  is at `src/repl/` → FINDINGS.md §3
- `Cargo.lock` is gitignored → FINDINGS.md §4
- Grammar covers keywords, constants, strings, numbers, comments, operators and
  call names. No bracket-pair or embedded-language folding, and the extension
  has no JS entry point, so no commands or debugger are contributed.

## Round 2 — review findings fixed

| # | Severity | Finding | Fix |
|---|---|---|---|
| 1 | **BLOCKER** | The shipped grammar and `language-configuration.json` treated `#` as a comment while the language only accepts `//` (`src/lexer.rs:366`, `docs/GRAMMAR.md:25`, `SPEC.md:116`) — the editor would dim `#` lines, `Ctrl+/` would write them, and `rb run` would reject them | The generator's comment rule is now `comment.line.double-slash.redblue` / `//.*$`, the shipped grammar carries it, and `lineComment` is `"//"`. `grammar_highlights_the_comment_marker_the_lexer_accepts` and `shipped_language_configuration_uses_the_lexers_comment_marker` pin the marker against the real lexer |
| 2 | **MAJOR** | Unused-variable warnings came out of a `HashSet`, so a file with two of them produced a different order per process, and that order went straight into the diagnostics JSON | `Linter::lint` collects the findings into a name-sorted vec (`src/linter.rs:43-58`); `unused_variable_warnings_come_out_in_a_stable_name_order` asserts 64 runs agree |
| 3 | MINOR | `diagnostics()` claimed to run the frontend once but called `linter::lint(source)`, which lexed and parsed the file again | `linter::lint_program(&Program)` added; `diagnostics()` lints the AST it already parsed. `diagnostics_lint_the_ast_they_already_parsed` pins that the two paths agree |
| 4 | MINOR | `Diagnostic::to_json` was documented as "the LSP `Diagnostic` shape" while emitting 1-based positions and severity names | Reworded, on `to_json` and on `Severity`, as the editor-facing shape `rb diagnostics` prints; the README already said no LSP is implemented |
| 5 | STYLE | The comment in `textmate_grammar()` said "text comes before comments" while the code pushed comments first | String rule now precedes the comment rule, and the comment explains both precedences (string before comment, comment before operator). `grammar_orders_strings_before_comments_and_comments_before_operators` pins the order |

Each fix was re-checked by reverting it and watching the new test fail.
