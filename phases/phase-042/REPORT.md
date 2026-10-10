# Phase 042 — VS Code extension: snippets, run command and keybindings

## Finding reproduced

`jq '.contributes | keys' tooling/vscode/package.json` on `main`:

```
[ "grammars", "languages" ]
```

No `snippets`, no `commands`, no `keybindings`, no `main`. The TextMate grammar
exists and is pinned to its generator, but the extension did nothing beyond
highlighting. The finding reproduces; it was not stale.

## What changed

| File | Lines | What |
|---|---|---|
| `tooling/vscode/package.json` | +30 −4 | `contributes.snippets`, `contributes.commands` (`redblue.run`), `contributes.keybindings` (`Ctrl+F5`/`Cmd+F5`, scoped to `editorLangId == redblue`), plus `main` and `activationEvents` |
| `tooling/vscode/snippets/redblue.json` | new, 60 lines | 8 snippets: `set`, `say`, `if`, `for`, `test`, `expect`, `try`, `catch` |
| `tooling/vscode/extension.js` | new, 64 lines | extension-host entry point; `registerCommand("redblue.run")` runs `rb run <active file>` in a terminal |
| `tooling/vscode/README.md` | +13 −2 | the "folder of JSON manifests" claim was made false by `extension.js`; file table and a run section corrected |
| `tests/tooling_extension_test.rs` | new, 7 `#[test]` | the assertions below |

`must_touch: ["tooling/"]` is satisfied: four files under `tooling/vscode/`.
The grammar and `language-configuration.json` are untouched.

## Tests added

| Test | Edge class covered |
|---|---|
| `extension_manifest_contributes_snippets_for_the_core_statements` | the phase's eight required completions exist and every contributed snippet path is a real file in the repo |
| `edge_snippet_bodies_are_redblue_once_their_tab_stops_are_filled` | malformed_input — every expanded body is lexed, parsed, analysed and **run** through `run_source`; a snippet that is not Redblue fails here |
| `edge_filling_a_snippet_tab_stop_past_the_end_of_a_list_is_a_clean_error` | out_of_bounds + **asserts a failure**: `expect items[999] to be 2` over a two-element list is `Error::Runtime`, not a panic |
| `extension_manifest_contributes_a_run_command_and_a_keybinding` | `redblue.run` has a title and a keybinding with a key and a Redblue-scoped `when` |
| `edge_a_contributed_command_nothing_registers_is_rejected` | **asserts a failure**: every contributed command id appears in `registerCommand("…")` in `main`, and in `activationEvents`; a command nothing registers fails here |
| `edge_duplicate_command_and_keybinding_entries_are_rejected` | duplicate_missing_keys — the shipped manifest has no duplicate ids, *and* the checker is proved non-vacuous on a synthetic manifest that duplicates both |
| `edge_malformed_manifest_json_is_reported_rather_than_ignored` | malformed_input — all five shipped JSON files parse; **asserts a failure** that a truncated file and a missing file are both rejected, each naming the file |

7 new `#[test]`, 5 of them named `edge_*`, 3 asserting a failure. No test was
skipped, ignored, weakened or deleted.

The phase also asks that the command and keybinding be "verified by `jq`
against the manifest". `jq` is not guaranteed in every environment that runs
`cargo test`, so the automated assertions parse the same JSON through
`serde_json`. Run by hand, against the shipped file:

```
$ jq -e '.contributes.commands[] | select(.command=="redblue.run")' tooling/vscode/package.json
{ "command": "redblue.run", "title": "Run Redblue File", "category": "Redblue" }

$ jq -e '.contributes.keybindings[] | select(.command=="redblue.run") | .key' tooling/vscode/package.json
"ctrl+f5"

$ jq -e '.contributes.snippets[0].path' tooling/vscode/package.json
"./snippets/redblue.json"

$ jq -e 'keys' tooling/vscode/snippets/redblue.json
[ "catch", "expect", "for", "if", "say", "set", "test", "try" ]
```

## Edge-case matrix

| Row | Status |
|---|---|
| empty | covered — a snippet with an empty `body` fails the "expands to nothing" assertion; `edge_a_contributed_command_nothing_registers_is_rejected` fails on an empty `main`; a command contributed twice is caught |
| singleton | covered — a one-line snippet (`set`, `say`, `expect`) is run on its own; `expect 1 + 1 to be 2` is the singleton default |
| boundary | covered — the index snippet is filled with `items[999]` against a **two**-element list, i.e. the last index and one past the end in one case |
| out_of_bounds | covered — `edge_filling_a_snippet_tab_stop_past_the_end_of_a_list_is_a_clean_error` |
| type_mismatch | N/A — this phase adds no expression evaluation. The snippet bodies it does evaluate are `set`, `say`, `if`, `for each`, `test`, `expect`, `try`/`catch`, all run to completion by `edge_snippet_bodies_are_redblue_once_their_tab_stops_are_filled` |
| numeric_boundary | N/A — no arithmetic is added; the only literal is the `${2:1}` default, which `edge_snippet_bodies…` runs |
| unicode | N/A — nothing is transcoded. `shellQuote` in `extension.js` passes bytes through untouched; a path with non-ASCII characters goes to the terminal unchanged |
| nesting_recursion | covered — the `if` snippet nests a `say` inside `then`/`else`, the `for` snippet a body inside its loop, and `catch` is compiled inside an enclosing `try` (three scopes deep in the `try` snippet) |
| duplicate_missing_keys | covered — `edge_duplicate_command_and_keybinding_entries_are_rejected`; the same `snippets()` reader also asserts no snippet name is declared twice across contributed files |
| malformed_input | covered — `edge_malformed_manifest_json_is_reported_rather_than_ignored` (truncated JSON, missing file, all five shipped files) |
| resource_limit | N/A — this phase adds no loop, recursion or output. `redblue.run` starts one terminal per invocation and VS Code owns its lifetime |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — no warnings |
| `cargo test --all-targets` | pass — 46 result blocks, **1230 passed, 0 failed, 0 ignored** |
| `./rbops/verify.sh phase-042` | **not run — the script is not in this checkout** |

`rbops/` does not exist under the project root (`ls rbops` → `No such file or
directory`). The RBOPS harness lives outside this repository and I was told not
to inspect it, so I did not try to reconstruct or substitute for it. The other
three gates were run in full and are green. `examples/hello.rb` was run by hand
(`cargo run --bin rb -- run examples/hello.rb`) and prints as before.

## Invariants touched

- None. No `.rb` source, no lexer keyword, no `Value` variant, no `Error`
  variant, no grammar pattern changed. The grammar file is byte-identical and
  `shipped_grammar_file_matches_the_generator` still passes.

## Known gaps / follow-ups

- `Ctrl+F5` is chosen as the run chord because `F5` belongs to the debugger.
  Nothing checks it against the user's `keybindings.json`; a collision is
  resolved the way VS Code resolves any collision, by the user.
- `redblue.run` requires `rb` on `PATH`; there is no bundled interpreter and no
  "rb not found" diagnostic beyond the terminal showing the shell's error.
- Only the `for each` form is offered; `repeat n times`, `while`,
  `repeat until`, `to`/`give back`, `unless`, `break`, `skip`, `import` and
  `test` blocks with `catch` have no snippets. `examples/` and `tests/*.rb`
  are the source of a follow-up.
- No `.vscodeignore`, no icon, no README on the Marketplace, no `LICENSE`
  entry — a VSIX build (`vsce package`) has never been run here.
- The `jq` check the phase names is run by hand and quoted above; it is not a
  `cargo test`, because `jq` may be absent from the environments CI runs in.
