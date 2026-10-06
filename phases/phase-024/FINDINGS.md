# Phase 024 — FINDINGS

Work outside this phase's concern, recorded for the auditor. Nothing here is
worked around to make a gate pass. §2 was recorded after round 1 and fixed in
this round; the rest is still open.

## 1. `rbops/` is not in this checkout — the baseline entry could not be removed

```
$ ls rbops
ls: cannot access 'rbops': No such file or directory
```

There is no `rbops/verify.sh`, no `rbops/baseline.json` and no
`rbops/phases.json` in the project root. Consequences, stated plainly:

- The fourth gate, `./rbops/verify.sh phase-024`, **could not be run**. It is
  reported in REPORT.md as not run, never as passed.
- The definition-of-done item "the entry is removed from `rbops/baseline.json`"
  **could not be carried out**. Rule 1 of AGENTS.md forbids creating or editing
  anything under `rbops/`, so no file was created. The defect the entry recorded
  is fixed — `rb run modules/MathUtils.rb` exits 0 — but the manifest entry
  itself is still there and needs whoever owns `rbops/` to delete it.
- `rbops/phases.json` could not be read, so this phase's `must_touch`
  (`["src/", "modules/"]`) is taken from the phase prompt. Both were changed.

What the missing gate likely checks, and the equivalent that *was* run here:
`cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets`, every `examples/*.rb` run through `rb run`, and
`rb lint`/`rb format` on `modules/MathUtils.rb`. All recorded in REPORT.md.

## 2. ~~The bytecode compiler cannot enforce that a name is a constant~~ — fixed

This was recorded after round 1 and then fixed: `constant NAME to <expr>` now
compiles to `Opcode::DeclareConst` (`DECLARE_CONST`, byte 46), not to the
`Opcode::Store` a `set` compiles to, and `FORMAT_VERSION` went from 2 to 3
because a version-2 file cannot say a name is read-only.
`docs/BYTECODE.md` carries the instruction, the version row and the rule that
made the bump necessary. `a_constant_compiles_to_its_own_instruction_not_to_a_store`
and `edge_a_version_2_file_is_refused_because_it_cannot_say_a_name_is_read_only`
in `tests/bytecode_test.rs` pin it.

What remains true, and is the reason it was worth doing before S2: nothing runs
a `.rbc` yet, so the *enforcement* is still only in the tree-walking VM. The
format no longer loses the distinction, which is what a bytecode VM would need
to enforce it the same way.

## 3. A module's functions are still unreachable through `import`

`Vm::load_module` (`src/vm.rs`) binds a module's `set` names and, since this
phase, its `constant` names. It still binds no functions, so:

```
$ printf 'import MathUtils\nsay MathUtils.circle_area(5)\n' > target/tmp/m.rb
$ cargo run --bin rb -- run target/tmp/m.rb
Error: RuntimeError: Unknown function 'MathUtils_circle_area'
  --> target/tmp/m.rb:2:1
2 | say MathUtils.circle_area(5)
  | ^
```

The `import` binds a global of the module's name and the loader binds the
module's `set` and `constant` names, and the analyzer now declares all of them
— that is what round 1 fixed, and it is why the failure moved from the analyzer
to the VM. What is still missing is the *functions*: the loader binds none, so
the call has nothing behind it.

Pre-existing and named in the comment above `load_module` (which points at this
file). It is not this phase's finding — the finding was that the file did not
parse at all, and it now parses, runs and exports its constants — but the module
system is still only half a module system, and SPEC.md's
`set area to MathUtils.circle_area(5)` does not work. Candidate for its own
phase; it needs a decision on the member syntax (`Name.function` against a
record of functions, most likely) before any code.

## 4. SPEC.md documents a `module … export all … end` block that does not parse

`SPEC.md` §Modules opens with:

```redblue
module MathUtils
    constant PI to 3.14159
    ...
    export all
end
```

`module` and `export` are in the keyword table (`src/lexer.rs`) but have no
parser arm:

```
$ cargo run --bin rb -- run <(printf 'module Foo\nend\n')
Error: ParserError: Unexpected token Module
```

The
shipped `modules/MathUtils.rb` uses the top-level form, which is the form this
phase implements, so the spec and the shipped module disagree about the same
language feature. Spec drift, pre-existing, and it needs the same decision as
§3 above.

## 5. GRAMMAR §4.1's optional declaration type annotation is not implemented

```
declaration = 'constant' identifier [ ':' type_expression ] 'to' expression
```

The annotation does not parse — for `set` as well as for `constant`:

```
$ cargo run --bin rb -- run <(printf 'set x: number to 1\n')
Error: ParserError: Expected To but got Colon
```

`constant` was given the same shape `set` has, so it introduces no new gap and
fixing it is not this phase's concern. If the type annotation is ever wanted it
has to land for `set` first, or `constant` will be the only declaration that
accepts it.

## 6. The formatter drops a blank line between top-level statements

`rb format --check` reports "File would be reformatted" for
`modules/MathUtils.rb`, for all six `examples/*.rb` files, and for anything else
with a blank line in it. `Formatter::format_statements`
(`src/formatter.rs:169`) writes statements and the comments above them; a blank
line is neither, so it does not survive. `Formatter::needs_reformat`
(`src/formatter.rs:721`) counts that as a difference.

Not caused by `constant` — the two `constant` lines round-trip byte for byte,
which `constant_round_trips_through_the_formatter` asserts. But it means the
formatter is not usable as a check on any of the repository's own `.rb` files,
which is a real hole: the one gate a formatter has is "run it on the corpus and
see it agree". `modules/` is in no format-check corpus today, which is why no
existing test catches it.

## 7. Test-corpus exclusions that were only there because of this defect

Both removed by this phase, recorded because a reader of the diff should know
they were not arbitrary:

- `tests/linter_test.rs` skipped `modules/` with the comment "`modules/MathUtils.rb`
  uses `constant`, which no phase has implemented yet".
- `tests/loop_bounds_test.rs` skipped `modules/` with "`to circle_area(radius)`
  — Expected function name".

Both now walk `modules/`. If a future phase finds another file that cannot be
parsed, the honest response is a phase that makes it parse, not another corpus
exclusion.

## 8. The analyzer's own scope model disagreed with the VM on `set`

Found while round 1 moved `tests/constant_test.rs` onto the whole pipeline, and
fixed here because the test that exposed it is one of the phase's own.

`Analyzer` declared every `set` in the scope the statement was written in, while
`Vm::set_var` writes to the innermost live scope that *already holds* the name
and to a global otherwise. So a name first set inside a loop, an `if` or a `try`
body lands on a global at runtime and survives the scope, and reading it after
the scope ends works — but the analyzer reported it as
`Unknown variable 'from_loop'`, so `rb run` refused a program the interpreter
runs. `Analyzer::declare_assigned` now mirrors `set_var`, and
`declare_constant` mirrors `bind_constant`, which binds a constant to a global
whatever scope the declaration is written in.

Nothing became less strict: the analyzer reports exactly the reads the VM would
fail, which is the whole point of a gate. `edge_constant_is_shadowed_by_a_local`
is the test that failed before the fix.
