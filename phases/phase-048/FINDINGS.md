# phase-048 — findings not fixed in this phase

Recorded per AGENTS.md § 1.7. Nothing here was done, because none of it is this
phase's concern.

## 1. `catch error of <Type>` is a spec feature the language does not have

- `SPEC.md` "Multiple Catch Blocks" (was :807-814) wrote three `catch` clauses
  dispatched on `FileError` and `NetworkError`.
- `docs/GRAMMAR.md:304` had it in the production:
  `{ 'catch' [ identifier ] [ 'of' identifier ] { statement } }`.
- `of` is not a word: `grep -c '"of"' src/lexer.rs` is 0. It lexes to an
  identifier, so `catch error of FileError` bound a name and left `FileError` as
  an orphan statement → `AnalyzerError: Unknown variable 'FileError'`.
- **The blocker for implementing it:** Redblue's runtime errors carry no type.
  `src/error.rs:78` `Error::{Lexer,Parser,Analyzer,Runtime,Io}` are the
  *compile-time* kinds; a caught failure binds the text `error`
  (`src/interpreter.rs:1772`, `src/bytecode/vm.rs:3326`). There is nothing for
  `of <Type>` to match against without first inventing a tagged error value —
  a language-design change with its own phase, touching `Value`, the two VMs,
  `Error`, the `.rbc` format and every runtime failure site.
- **What this phase did:** corrected SPEC.md and GRAMMAR.md to the untyped form
  the code supports, and pinned the surviving shape with
  `tests/catch_binding_test.rs::edge_catch_error_of_a_type_is_refused_naming_the_orphan`.

## 2. `give back error "..."` — a raise statement the language does not have

- `SPEC.md` "Raising Errors" (was :951-960) wrote:
  `if b is equal to 0 then give back error "Cannot divide by zero" end`.
- `give back` parses an expression (`src/parser.rs:215` `GiveBack(Option<Expr>)`)
  and `error` lexes to an identifier, so it reads a variable: `AnalyzerError:
  Unknown variable 'error'`. There is no `raise` builtin either —
  `grep -n 'raise' src/stdlib.rs` is empty.
- **What this phase did:** corrected SPEC.md's section to state that no such
  statement exists and to give the form that does work (a function signals a
  failure by performing an operation that fails; `might fail` turns it into
  `nothing`). No code was added — a raise statement is a new language feature,
  not a bug fix, and inventing one here would be the unbounded speculation
  AGENTS.md § 1.8 forbids.

## 3. `rb format` canonicalises `catch error e` to `catch e`

`src/formatter.rs:512-524` writes `catch ` followed by `catch_var`, so the
keyword word of the spelled-out form is dropped on format. The result reads back
to the same program and runs the same (verified), and the shorter form is what
every example in the language writes — but a user who wrote SPEC.md's spelling
does not get it back from `rb format`. Deliberate here: writing the longer form
would make formatted files disagree with the spec's own examples. Not changed.

## 4. Gate 4 was not run

`rbops/` is not present in this checkout (`ls rbops` → `No such file or
directory`), so `./rbops/verify.sh phase-048` could not be executed and nothing
is claimed about its result. Gates 1-3 were run and pass.