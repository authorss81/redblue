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
  (`src/interpreter.rs:1772-1774`, `src/bytecode/vm.rs:3326`). There is nothing for
  `of <Type>` to match against without first inventing a tagged error value —
  a language-design change with its own phase, touching `Value`, the two VMs,
  `Error`, the `.rbc` format and every runtime failure site.
- **What this phase did:** corrected SPEC.md and GRAMMAR.md to the untyped form
  the code supports, and pinned the surviving shape with
  `tests/catch_binding_test.rs::edge_catch_error_of_a_type_is_refused_naming_the_orphan`.
- **Deferred to phase-089**, which owns tagged error values. The decision is
  settled there, so nothing in this section is open work for this phase.

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

## 4. The same-line rule has one losing case, and it is a clean error

`catch error <name>` is told apart from `catch error` by whether a second
identifier follows `error` **on the same line** (`src/parser.rs::catch_keyword_ahead`).
So a handler written on one line whose body starts with a *bare name* —
`catch error n is 5` — reads `n` as the binder, and the leftover `is 5` does not
parse: `ParserError: Unexpected token Is`, spanned at the `catch` line. It fails
cleanly rather than mis-binding, it is the one shape the docs never write, and
narrowing it further would need a statement-start test in the parser — more
machinery than the ambiguity is worth. Pinned by
`tests/catch_binding_test.rs::edge_a_body_starting_with_a_bare_name_on_the_catch_line_is_refused`
so a future change to the rule has to move that test rather than discover the
case later.

## 5. `if <cond>` with no `then` will not take `might fail` as its body

Found on this attempt while running every SPEC.md example that mentions `try`.
`SPEC.md:870` § "Finally" writes:

```redblue
try
    set file to might fail files.open("data.rb")
    process file
finally
    if file is not nothing
        might fail files.close(file)
    end
end
```

which fails on both engines with `ParserError: Expected 'then' but got 'might'`,
spanned at the `might fail` line. The same `if` with `skip` as its body parses
(the spec's own `skip`-in-`finally` example, `SPEC.md:911`, runs
to exit 0), so the inconsistency is specific to `might fail` not being accepted
as the first statement of a block opened without `then`.

- **Not fixed here.** This is `if`, not `catch`: `might fail` as a block-leading
  statement is a different phase's concern, and AGENTS.md § 1.7 says to record it
  rather than do it.
- **Reproduction:** `printf 'if 1 is 1\n    might fail files.read("x")\nend\n' > t.rb && rb run t.rb`
- **Why it matters:** `might fail` is the form SPEC.md's corrected "Raising
  Errors" section tells a reader to use instead of a `raise`, so the documented
  replacement for a raise cannot be the first statement of an `if` body.

## 6. Gate 4 was not run

`rbops/` is not present in this checkout (`ls rbops` → `No such file or
directory`), so `./rbops/verify.sh phase-048` could not be executed and nothing
is claimed about its result. Gates 1-3 were run from the project checkout and
pass; `examples/` and `modules/` were each run through `rb run` and all exit 0.