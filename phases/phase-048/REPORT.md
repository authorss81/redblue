# Phase 048 — Make `catch error <name>` bind the message, as SPEC.md and README.md spell it

## What changed

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +39 −0 | `parse_try` reads the `error` word of `catch error <name>` as the handler keyword when a second identifier follows it **on the same line**, and binds that second identifier; new helper `catch_keyword_ahead` makes the decision |
| `bootstrap/compiler.rb` | +16 −3 | the same rule in `parse_try_catch_body`, so stage 2 emits the program stage 1 emits and the S3 fixed point still holds |
| `tests/catch_binding_test.rs` | +327 | new: 13 `#[test]`s, every one run on **both** engines |
| `tests/bootstrap_selfhost_test.rs` | +7 −0 | a `try_catch_error_name` case in the stage-1/stage-2 byte-identity table |
| `SPEC.md` | +74 −17 | "Multiple Catch Blocks" replaced by "Handling Failures by Hand"; "Raising Errors" corrected; Try-Catch section given the binder's scope and the two spellings |
| `docs/GRAMMAR.md` | +11 −3 | `try_statement` production now spells both binder forms and says `catch error of <Type>` does not exist |

### Reproduction, before

```
$ printf 'try\n    set x to 1 / 0\ncatch error e\n    say e\nend\n' > catch_error_name.rb
$ rb run catch_error_name.rb
Error: AnalyzerError: Unknown variable 'e'
Unknown variable 'e'
  --> catch_error_name.rb:3:13
3 | catch error e
  |             ^
EXIT=1
```

`error` is not a keyword (`grep -n '"error"' src/lexer.rs` returns nothing), so
the parser read the bare word `error` as the **binder**, consumed it, and left
`e` as an orphan statement. The other two halves of the finding reproduced on
`main` as described:

```
$ printf 'give back error "boom"\n' > raise.rb && rb run raise.rb
Error: AnalyzerError: Unknown variable 'error'      EXIT=1
$ printf 'try\n ...\ncatch error of FileError\n ...\nend\n' > of.rb && rb run of.rb
Error: AnalyzerError: Unknown variable 'of'; did you mean 'if'?
Error: AnalyzerError: Unknown variable 'FileError'  EXIT=1
```

### After

```
$ rb run catch_error_name.rb
error
EXIT=0
$ rb compile catch_error_name.rb -o cen.rbc && rb vm cen.rbc
error
EXIT=0
```

The three existing spellings are untouched: `catch e` and `catch problem` still
bind the name after `catch`, and `catch error` still binds a name *spelled*
`error` — which is what every example in the language already meant.

### The rule

`error` is a word to the lexer, not a keyword, so which of the two spellings a
line is writing is decided by whether a **second identifier follows `error` on
the same line**:

- `catch error` alone — SPEC.md's own handler, and the spelling every example
  uses — is the word with nothing after it, so it goes on binding `error`;
- `catch error problem` is the word with a name after it, so it binds `problem`;
- `catch error` whose body starts on the **next** line is the first spelling,
  whatever the body's first word is. The body's first token is then never eaten,
  which `edge_the_body_first_word_on_the_next_line_is_not_the_binder` pins.

`src/parser.rs`, the whole of the change:

```rust
if self.catch_keyword_ahead() {
    self.advance(); // consume 'error'
}
if let Some(Token { kind: TokenKind::Identifier(name), .. }) = self.current() {
    catch_var = Some(name.clone());
    self.advance();
}
```

The binding itself was already right and already reached: `catch_var` is bound
by `src/analyzer.rs:468`, declared by `src/linter.rs:321`, compiled by
`src/bytecode/codegen.rs:530` and bound by `src/bytecode/vm.rs:3322` — that
code was not dead, it was just only reachable from a spelling SPEC.md does not
write. The parser change alone puts it on the documented path.

## Tests added

`tests/catch_binding_test.rs` — 13 `#[test]`s. `both_engines` is the only way a
case reaches an engine, and it fails the test when the tree-walking VM and the
bytecode compiler+VM disagree on the printed lines or on a failure's label,
message and line.

| Test | Edge class covered |
|---|---|
| `catch_error_name_binds_the_message` | the phase's spelling, `say e` prints the bound text |
| `every_catch_spelling_handles_the_same_failure` | 4-case table — bare `catch` (README.md:11), `catch error` (README.md:64), `catch <name>` (README.md:51), `catch error <name>` (SPEC.md:800); each asserts the lines it prints |
| `the_binder_is_scoped_to_the_catch_body_alone` | `e` before the `try` and after the `end` are both AnalyzerErrors **naming `e`** |
| `the_binder_shadows_a_program_variable_of_the_same_name` | body sees the binder, the program keeps its own |
| `edge_a_try_that_never_fails_never_binds_the_name` | **failure asserted** — `AnalyzerError` naming `e`, never a silent `nothing` |
| `edge_an_empty_catch_body_still_handles_the_failure` | empty / zero, for both spellings |
| `edge_the_body_first_word_on_the_next_line_is_not_the_binder` | boundary between the two spellings |
| `edge_a_failure_inside_the_catch_body_is_not_swallowed` | **failure asserted** — spanned `RuntimeError` on the catch body's own line |
| `edge_a_try_with_no_catch_reports_its_own_failure` | **failure asserted** — a `try` with no `catch` stops with a spanned `RuntimeError` |
| `edge_the_bound_name_is_ordinary_unicode_text` | unicode — ✓, 🎉, `café` through the handler |
| `edge_the_keyword_and_the_binder_may_be_the_same_word` | `catch error error` binds a name spelled `error` |
| `edge_a_keyword_and_a_binder_on_one_line_bind_and_run` | the binder is usable in an expression in the body |
| `edge_catch_error_of_a_type_is_refused_naming_the_orphan` | **failure asserted** — `catch error of FileError` names the orphan, never passes for a typed handler |

Plus `tests/bootstrap_selfhost_test.rs`: `try_catch_error_name` — stage 2 must
emit byte-for-byte what stage 1 emits for `catch error problem`.

## Decisions the phase owed

**`catch error of <Type>` — SPEC.md corrected, not implemented.** Redblue's
runtime errors are messages, not tagged values: `Error::{Lexer,Parser,Analyzer,
Runtime,Io}` are compile-time kinds and a caught failure binds the text `error`.
There is no error object, so there is nothing for `of FileError` to dispatch on,
and inventing a tagged error type is a language-design change far past this
phase. SPEC.md's "Multiple Catch Blocks" is replaced by "Handling Failures by
Hand", which says so in the spec rather than in a changelog, and
`docs/GRAMMAR.md`'s `try_statement` production no longer claims `of` exists.

**`give back error "..."` — SPEC.md corrected, not implemented.** There is no
`raise` in Redblue: `grep -n 'raise' src/stdlib.rs` returns nothing, and
`give back` hands a value back rather than failing. SPEC.md's "Raising Errors"
now says that, and gives the form that does work — a function signals a failure
by performing an operation that fails, and `might fail` turns it into `nothing`.

Both corrections are in `SPEC.md` and both are named in the changed-files table
above. `tests/catch_binding_test.rs::edge_catch_error_of_a_type_is_refused_naming_the_orphan`
pins the shape that survives, so a future phase that *does* implement typed
handlers has to move that test rather than discover the change later.

## Test-requirement matrix (AGENTS.md § 3)

| Row | Status |
|---|---|
| empty | covered — `edge_an_empty_catch_body_still_handles_the_failure` |
| singleton | covered — a handler is the only member of its clause; the 4-case table is one case per spelling |
| boundary | covered — `edge_the_body_first_word_on_the_next_line_is_not_the_binder` and `edge_the_keyword_and_the_binder_may_be_the_same_word` are the line boundary between the two spellings |
| out_of_bounds | N/A — a binder is a name, not an index; nothing this change touches reads an index |
| type_mismatch | N/A — a binder holds one value (text). The failure channels this change *can* produce, a name read where none is bound and a `catch` of the wrong shape, are both covered and named in the failure rows below |
| numeric_boundary | N/A — no arithmetic is added; `1 / 0` is only the failure the tests provoke, and it is refused the same way before and after |
| unicode | covered — `edge_the_bound_name_is_ordinary_unicode_text` |
| nesting_recursion | covered — `edge_a_failure_inside_the_catch_body_is_not_swallowed` is the catch-in-the-handler nesting, and the pre-existing `shape/nested-tries` and `flow/*` cases in `tests/bytecode_vm_test.rs` still pass |
| duplicate_missing_keys | N/A — a handler binds one name; there are no records or keys in this change |
| malformed_input | covered — `edge_catch_error_of_a_type_is_refused_naming_the_orphan` (the `of` form) and `edge_a_try_that_never_fails_never_binds_the_name` (a binder with no failure behind it), both spanned AnalyzerErrors |
| resource_limit | N/A — the parse adds no recursion and no allocation that grows with input; `parse_try`'s existing block-depth bound is untouched, and `edge_the_self_compilation_still_obeys_the_step_budget` in `tests/bootstrap_selfhost_test.rs` still passes |

### Tests that assert a failure is produced

1. `edge_a_try_with_no_catch_reports_its_own_failure` — `RuntimeError` on the
   line inside the `try`.
2. `edge_a_failure_inside_the_catch_body_is_not_swallowed` — `RuntimeError`
   pointing at the catch body's line, not the first one.
3. `the_binder_is_scoped_to_the_catch_body_alone` — two AnalyzerErrors naming `e`.
4. `edge_a_try_that_never_fails_never_binds_the_name` — AnalyzerError naming `e`.
5. `edge_catch_error_of_a_type_is_refused_naming_the_orphan` — AnalyzerError
   naming `FileError`.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings |
| `cargo test --all-targets` | pass — 0 failed, 0 ignored; `tests/expect_test.rs` 21 passed; the 9 files in `examples/` and `modules/` all `rb run` to exit 0 |
| `./rbops/verify.sh phase-048` | **not run** — `rbops/` is not in this checkout (`ls rbops` → `No such file or directory`), so gate 4 could not be executed here. Nothing is claimed about it. |

Zero new `#[ignore]`, zero `// skip`, zero `allow(clippy::`, zero weakened or
deleted tests.

## Invariants touched

- None. No `Value` variant, no `Error` variant, no keyword added, no `.rb`
  extension change, no `end` closing changed, no `set x to` change. `catch
  error` still binds a name spelled `error`, so the ~60 existing programs and
  tests that write it mean exactly what they meant.
- `src/parser.rs` — the two words after `catch` are now read as keyword-then-
  binder when both are there. `catch error` is unchanged; only a `catch` that
  had two identifiers after it (and therefore already failed to parse into a
  working program) reads differently.

## Known gaps / follow-ups

- `rb format` canonicalises `catch error e` to `catch e`. That is a formatter
  writing the shortest source with the same meaning, which is its job; the round
  trip re-reads to the same program. Verified: `rb format` then `rb run` gives
  the same output. Recorded rather than changed — the formatter emitting the
  longer spelling would make formatted files disagree with the spec's own
  examples.
- `catch error of <Type>` and a user-level `raise` remain **unimplemented
  features**, now correctly described as absent. Both are recorded in
  `FINDINGS.md` for the auditor.
- `rbops/verify.sh` could not be run in this checkout; the pipeline runs it.