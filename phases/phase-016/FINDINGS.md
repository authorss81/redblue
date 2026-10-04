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

### F3 — the test corpus now carries true-positive warnings

`rb lint tests/*.rb` reports 23 unused variables and one shadowed parameter
across the `.rb` suite. Each one was checked by
`edge_no_unused_variable_warning_in_the_corpus_names_a_read_variable` and
`edge_every_shadow_warning_in_the_corpus_hides_a_real_outer_binding`, which read
the source independently of the linter: the names really are never read. Two
that look wrong at first glance and are not:

- `tests/test_text.rb:40` `name` — `{name}` inside a text literal. Interpolation
  is in SPEC.md but unimplemented, so the braces are literal characters and the
  name is text, not a read. `tests/test_text.rb:38` says so in a comment.
- `tests/test_objects.rb` `One`, `Two` — `object Two extends One` names the
  parent at declaration time; neither record is read afterwards.

If a later gate wants `rb lint` silent over `tests/`, those tests must first
read their values, which would be a change to the language's test corpus and
should be its own phase.
