# Phase 028 — findings

Work outside this phase's scope, with file:line evidence. Not fixed here; the
auditor should promote these to phases.

## 1. MAJOR — `break` and `skip` do nothing in any loop

`src/vm.rs` has no arm that makes `break` or `skip` leave a loop, and neither
statement carries any state out of the body. Verified on `main` semantics after
this phase landed, across **every** loop form — so it is not a range-loop
defect and this phase did not introduce it:

```
$ printf 'for each v in [1,2,3,4]\n    if v is 3 then\n        break\n    end\n    say v\nend\n' > target/tmp/t.rb
$ ./target/debug/rb run target/tmp/t.rb
1
2
3
4
exit=0
```

Same silent no-op for `skip`, and for `while`, `until` and `repeat`. The
statements parse (`src/parser.rs` builds `Statement::Break` /
`Statement::Skip`), the analyzer treats them as leaves (`src/linter.rs:264`
`Statement::Break | Statement::Skip => {}`), the formatter emits them, and the
runtime drops them. `tests/bytecode_vm_test.rs:449` carries a corpus entry
named `shape/break-and-skip-inside-a-loop` that compares the two VMs — they
agree, because both ignore them, so the differential test cannot catch it.

Worse, `break` inside a `while` whose body increments the counter is an infinite
loop stopped only by the iteration guard: a program that looks bounded is not.

Needs a signal from `execute_statement` to the loop driver (a
`Result<ControlFlow>`-shaped return, or a loop-depth stack in `Vm`), in both
VMs. Suggested phase: "Make `break` and `skip` leave a loop" — `must_touch:
["src/"]`, severity major.

## 2. MINOR — `by` is not highlighted, because it is not a keyword

`src/lsp.rs:195` `keyword_pattern()` and
`tooling/vscode/syntaxes/redblue.tmLanguage.json:11` both build their keyword
list from `KEYWORDS` (`src/lexer.rs:14-69`). `by` is deliberately **not** in
that table — see `Parser::at_range_step_marker` (`src/parser.rs:759`) — because
reserving it would break `to can grow(by)` (`tests/bytecode_test.rs:602`). The
consequence is that an editor dims neither `from 0 to 10 by 5` nor a variable
named `by`, so the step marker reads as an ordinary identifier.

Fixing it properly needs a distinction the single keyword table cannot express:
either a contextual keyword, or a TextMate rule scoped to the range-loop
production. Suggested phase: "Highlight contextual keywords" — `must_touch:
["src/lsp.rs", "tooling/"]`, severity minor.

## 3. MINOR — the analyzer reports an out-of-scope read on the wrong line

`for each i from 1 to 3` / `say i` / `end` / `say i` fails with
`AnalyzerError: Unknown variable 'i'` at **line 4** for a read on line 3:

```
$ printf 'for each i from 1 to 3\n    say i\nend\nsay i\n' > target/tmp/t.rb
$ ./target/debug/rb run target/tmp/t.rb
Error: AnalyzerError: Unknown variable 'i'
  --> target/tmp/t.rb:4:1
```

The span is still a real position, so the message is usable; it is just the
start of the next statement rather than the read. `tests/for_range_test.rs`
`edge_the_loop_variable_is_out_of_scope_after_end` asserts the message and that
a position exists, not the line, precisely so it does not pin this. Fix belongs
with whatever makes analyzer spans per-expression rather than per-statement.