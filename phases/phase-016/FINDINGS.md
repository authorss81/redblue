# phase-016 FINDINGS

## The finding reproduced

`src/linter.rs` had no tests and two reproducible false positives. Both are now
covered by `tests/linter_test.rs`. Recorded here so the auditor can see what was
wrong before the fix.

1. **An unread function parameter was reported as an unused variable.**
   `to greet(name) / say "hello" / end` printed
   `Warning: Unused variable: 'name'`. A parameter is part of a signature, not a
   leftover binding.
2. **A field write was reported as an unused variable.**
   `set Box.label to "crate"` printed `Warning: Unused variable: 'Box.label'`,
   because `set record.field to ...` was entered into the variable table as
   though `field` were a name of its own.
3. **The `errors` list was never written to.** `lint()` returned
   `(vec![], vec![])` for any source that failed to lex or parse, so
   `rb lint` exited 0 on a file that cannot run — a missing `end` included.
4. **An object named by `extends` was reported as an unused variable.**
   `Statement::Object` destructured the parent as `extends: _` and dropped it,
   but `object Child extends Parent` reads `Parent`: the VM walks
   `self.objects` when the declaration runs (`src/vm.rs:1419`) and the analyser
   requires the parent to be declared (`src/analyzer.rs:188`). Found by the
   resumed run, which re-walked the corpus the previous attempt had declared
   clean. Five warnings in `tests/test_objects.rb` and
   `tests/test_object_model.rb` were this one false positive.
5. **The corpus cross-check was blind to it.** `is_binding_mention` in
   `tests/linter_test.rs` listed `(Some("extends"), _)`, classifying the parent
   name as a binding. `extends` binds nothing, so the checker that was supposed
   to catch the previous item could not. Reverting the fix while the checker was
   corrected makes
   `edge_no_unused_variable_warning_in_the_corpus_names_a_read_variable` fail
   with `tests/test_object_model.rb:74 claims 'One' is unused but it is read on
   [80]`.

## Findings that belong to other phases

### F1 — `constant` is not in the grammar, so `modules/MathUtils.rb` does not parse

`modules/MathUtils.rb:4` is `constant PI to 3.14159`. `parse_callable`
(`src/parser.rs:815-830`) expects an identifier after `to`, so the whole file
fails with `Syntax error: Expected function name`. The module loader skips
files it cannot parse, so nothing in the gate notices; `rb lint` now reports it
as an error, which is correct but means a module file cannot be linted as a
program. `modules/SuiteKit.rb` uses only `set` and functions and does parse.

Not this phase's work: adding `constant` is a grammar change and needs its own
phase with a decision in SPEC.md. Until then `tests/linter_test.rs` keeps
`modules/` out of the corpus walk and says why.

### F2 — a defined and never called function is not reported

`Linter::defined_functions` (`src/linter.rs:14`) is populated and never read.
A "defined but never called" rule cannot be written on the syntax tree alone:
Redblue functions are first class (`set double to to double(v) ... end`, see
`tests/test_closures.rb`), so a call site can be a value rather than a call.
Until the linter tracks values it would fire false positives, so the field stays
inert and is commented rather than turned into a half-rule.

### F3 — the test corpus carries true-positive warnings

`rb lint tests/*.rb` reports 17 unused variables and one shadowed parameter
across the `.rb` suite. Each one was checked by
`edge_no_unused_variable_warning_in_the_corpus_names_a_read_variable` and
`edge_every_shadow_warning_in_the_corpus_hides_a_real_outer_binding`, which read
the source independently of the linter: the names really are never read. They
fall into four groups:

- `set <name> to <expr>` inside a `try` whose right-hand side is written to
  fault (`1 / 0`, `1 + "x"`, `items[999]`, a call to a function that recurses
  until the depth guard trips) — the binding target is never read because the
  assignment never completes. 13 of the 17.
- `tests/test_control_flow.rb:175` `only_inside` and
  `tests/test_functions.rb:195` `marker` — the test is about the branch or the
  `catch` being taken, not about the name written there.
- `tests/test_text.rb:40` `name` and `:139` `word` — `{name}` inside a text
  literal. Interpolation is in SPEC.md but unimplemented, so the braces are
  literal characters and the name is text, not a read.
  `tests/test_text.rb:38` says so in a comment.
- `tests/test_closures.rb:52` `Parameter 'v' shadows an outer variable` — the
  test is named "a parameter shadows what the declaration captured", so the
  shadowing is the behaviour under test.

A fourth group was here and is now fixed: `object Two extends One` used to
report the parent `One` as an unused variable. See item 4 under "The finding
reproduced".

If a later gate wants `rb lint` silent over `tests/`, those tests must first
read their values, which would be a change to the language's test corpus and
should be its own phase.

### F4 — `import X as Y` is the documented alias syntax and the parser does not read it

`SPEC.md:645` writes `import MathUtils as M`, `docs/GRAMMAR.md:152` gives
`import_statement = 'import' identifier { 'as' identifier }`, and `AGENTS.md:182`
writes `import files, network as net`. `parse_import`
(`src/parser.rs:479-497`) accepts `to` instead of `as`, and nothing else. So the
documented form parses the import, drops the alias, and leaves `as M` to be
re-read as two expression statements.

Reproduced on the built binary:

```
$ cat target/tmp/lint/alias.rb
import MathUtils as M

set area to M.circle_area(5)
say area

$ ./target/debug/rb run target/tmp/lint/alias.rb
Error: AnalyzerError: Unknown variable 'as'
Unknown variable 'M'
Unknown variable 'M'
  --> target/tmp/lint/alias.rb:1:18
exit=1

$ ./target/debug/rb lint target/tmp/lint/alias.rb
Warning: line 1: Unused import: 'MathUtils'
exit=0
```

The linter's warning is true of the tree it was handed — nothing in that tree
mentions `MathUtils` — so it is not a linter false positive. It is the linter
being honest about a parse that lost the alias. `tests/test_modules.rb:46` and
`:80` use `import SuiteKit to K` / `import NopeNotHere to N`, so the working
form is `to` and the documented form is `as`; both cannot be right.

Not this phase's work: it is a change to the parser and a decision about which
of the two spellings the language keeps, and the linter cannot be made correct
about `as` without it. Recording it so a grammar phase can settle it.

### F5 — the linter gives a `finally` body a scope the analyser does not

`src/linter.rs:304` walks the `finally` body through `analyze_body`, which pushes
a scope. `src/analyzer.rs:216-218` walks the same body with no scope pushed, so
a name written there belongs to the enclosing scope in the analyser.

Nothing observable depends on it today, and `edge_a_finally_binding_does_not_shadow_the_catch_binding`
pins the behaviour that agrees with the analyser: the `catch` binding's scope is
closed before `finally` runs, so reusing that name in `finally` is a new binding
and not a shadow. The divergence would matter only if a rule were ever written
that reads through a block boundary. Noted so the next phase that touches
`finally` knows the two walkers disagree.

