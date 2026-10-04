# phase-003 — FINDINGS

Work discovered while writing the Redblue test suite that does **not** belong to
this phase. Each item is file:line anchored and was confirmed by running Redblue
on the current `main` (`ee528e1`). None of these were fixed here: this phase's
concern is the test suite, not the language.

---

## 1. `stdlib::builtin_function` is never called — the whole stdlib is dead

`src/stdlib.rs:193` defines `pub fn builtin_function(name: &str, args: Vec<Value>)
-> Option<Value>` covering `abs`, `floor`, `ceil`, `round`, `sqrt`, `pow`, `log`,
`exp`, `sin`, `cos`, `tan`, `uppercase`, `lowercase`, `trim`, `split`, `join`,
`contains`, `starts_with`, `ends_with`, `replace`, `push`, `pop`, `shift`, `map`,
`filter`, `reduce`, `is_number`, `is_text`, `is_list`, `is_record`, `to_text`,
`to_number`, `to_list`. `src/stdlib.rs:12-188` registers every one of them in
`globals` as `Value::Builtin(name)`.

`src/vm.rs:584` (`fn call`) matches a **hand-copied** subset of that list
(`say`, `length`, `files_*`, `time_*`, `json_*`, `csv_*`, `network_*`, `expect`,
`console_*`, `random_*`, `type_of`) and its final `_ =>` arm at `src/vm.rs:1004`
emits `Unknown function '<name>'`. There is no call to
`stdlib::builtin_function` anywhere in `src/` (`grep -rn "stdlib::" src/vm.rs`
returns only line 22, `let globals = stdlib::builtins();`).

Reproduction:

```
$ cat /tmp/x.rb
say type_of(uppercase)
$ rb run /tmp/x.rb
Error: RuntimeError: Unknown function 'uppercase'
```

**Severity: blocker.** It removes ~35 public stdlib functions from the language.
**Risk: low** — the fix is to consult `stdlib::builtin_function` from the `_ =>`
arm before erroring.
**Suggested acceptance gate:** every name in `stdlib::builtins()` resolves; a
`tests/*.rb` block per family; plus the existing `cargo test` and `rb test` green.

---

## 2. `{interp}` string interpolation is specified but never substituted

`AGENTS.md` §2 lists `{interp}` as a language invariant that must not silently
change, and `SPEC.md:237` specifies it. No VM code performs the substitution —
`src/vm.rs:461` is the whole of text `+`, and no other site formats a `Value`
into a text literal.

Reproduction:

```
$ printf 'set n to 7\nsay "n is {n}"\n' > /tmp/i.rb && rb run /tmp/i.rb
n is {n}
```

`tests/test_text.rb` currently pins the *unsubstituted* behaviour
(`text: braces are ordinary characters in a text literal`) so the gap is visible
in the suite rather than hidden. That test must be inverted once this is fixed.

**Severity: blocker.** `examples/hello.rb:12` depends on it (it only survives
there because user function bodies are never executed — see finding 3).
**Risk: medium** — an invariant is involved; needs its own phase and a grammar/
parser review for the `{expr}` expression grammar.

---

## 3. `Value::Function` stores no body — every user function returns `nothing`

`src/vm.rs:264-270` handles `Statement::Function` by binding the name to
`Value::Function(name.clone(), params.clone())`; `src/parser.rs:167-174` confirms the
variant carries only `name` and `params`. The body is dropped, so
`src/vm.rs:1004`'s user-function branch returns `Value::Nothing`, and
`Statement::GiveBack` (`src/vm.rs:260`) is only ever evaluated in a top-level
position.

Reproduction:

```
$ printf 'to add(a, b)\n  give back a + b\nend\nset s to add(2,3)\nsay s\n' > /tmp/f.rb && rb run /tmp/f.rb
nothing
```

Consequences: no recursion, no closures, no user-defined callbacks
(`map`/`filter`/`reduce` cannot accept one — `src/parser.rs:1030-1043` rejects `to (x)`
as an argument), and `examples/hello.rb:10` prints nothing.

**Severity: blocker.** This is the largest single gap between SPEC and the VM.
**Risk: high** — `Value` is a public exported type (`src/lib.rs:22`); changing
the variant shape is an API change and needs its own phase.

---

## 4. `break` and `skip` are parsed and then ignored by every loop

`Statement::Break` / `Statement::Skip` exist in `src/parser.rs` and are produced,
but `src/vm.rs` runs `for each` / `repeat` / `while` bodies through
`execute_statements` (`src/vm.rs:363`), which discards the control-flow signal.

Reproduction:

```
$ printf 'set n to 0\nfor each x in [1,2,3,4]\n  break\nend\nsay n\n' > /tmp/b.rb && rb run /tmp/b.rb
0
$ printf 'set n to 0\nfor each x in [1,2,3,4]\n  set n to n + 1\n  break\nend\nsay n\n' > /tmp/b2.rb && rb run /tmp/b2.rb
4
```

`tests/test_lists.rb` pins the current iteration count
(`lists: break inside a loop does not truncate it`) so the regression is visible.

**Severity: major.** **Risk: medium** — needs a control-flow signal threaded out
of `execute_statements`.

---

## 5. `has` and `new` are keywords with no parser rule

`src/lexer.rs:266` (`"has" => TokenKind::Has`) and `src/lexer.rs:270`
(`"new" => TokenKind::New`). `parse_statement` (`src/parser.rs:300-395`) has no arm
for either, and `parse_object` (`src/parser.rs:683-719`) only accepts
`extends <identifier>` before the body.

Reproduction:

```
$ printf 'object Point\n  has x\nend\n' > /tmp/o.rb && rb run /tmp/o.rb
Error: ParserError: Unexpected token Has
$ printf 'object Point\nend\nset p to new Point\n' > /tmp/o2.rb && rb run /tmp/o2.rb
Error: ParserError: Unexpected token New
```

So `object X ... end` binds an **empty record** (`src/vm.rs:285-293`) and there
is no way to declare a field or an instance. SPEC.md:453/471 describe both.

**Severity: major.** **Risk: high** — object semantics are unspecified in the
implementation and would need designing.

---

## 6. `constant X to 5` is not parsed, which breaks `modules/MathUtils.rb`

`constant` is not in the lexer's keyword table (`src/lexer.rs:228-275`), so
`modules/MathUtils.rb:3` lexes as three expressions and `parse_function`
(`src/parser.rs:615`) then errors.

Reproduction:

```
$ rb run modules/MathUtils.rb
Error: ParserError: Expected function name
  --> modules/MathUtils.rb:4:16
```

Confirmed pre-existing on `ee528e1` via `git stash`. AGENTS §2 says
`modules/*.rb` are the language's specification-by-example and "the gate runs
them", so this file does not currently run.

**Severity: major.** **Risk: low** — either add `constant` to the lexer/parser
or change the file; needs a decision.

---

## 7. `import` succeeds but module members are unreachable

Three independent holes:

- `src/analyzer.rs` has no `Statement::Import` arm, so any use of the imported
  name is `Unknown variable '<name>'` even after a successful import.
- `src/vm.rs:43-68` (`load_module`) only consumes `Statement::Set` and
  `Statement::Function`, and for a function stores only the *params* into
  `self.functions` — which nothing ever reads.
- `src/vm.rs:338-340` overwrites the imported name with `Value::Nothing` *after*
  loading, discarding anything the loader collected.

Reproduction (with `modules/SuiteKit.rb` added by this phase):

```
$ printf 'import SuiteKit\nset v to SuiteKit.suite_double(2)\nsay v\n' > /tmp/m.rb && rb run /tmp/m.rb
Error: RuntimeError: Unknown function 'SuiteKit_suite_double'
$ printf 'import SuiteKit\nsay SuiteKit\n' > /tmp/m2.rb && rb run /tmp/m2.rb
Error: AnalyzerError: Unknown variable 'SuiteKit'
```

Also note the alias syntax: the parser wants `import Name to Alias`
(`src/parser.rs:342-392`), while SPEC.md:548 documents `import MathUtils as M`.
`as` is not a keyword.

**Severity: major.** **Risk: medium.**

---

## 8. `length` on text counts bytes, SPEC implies characters

`src/vm.rs:601` uses `s.len()`, which is UTF-8 bytes. `length("héllo")` is 6 and
`length("🎉")` is 4. Documented in SPEC.md nowhere; `tests/test_text.rb` pins it
(`edge_text_length_counts_bytes_not_characters`). If the intent is characters,
this is a defect, not a specification.

**Severity: minor.** **Risk: low.** **Needs a decision:** bytes or chars.

---

## 9. Out-of-range and negative indices are silently permissive

`items[999]` and `items[-99]` both yield `Value::Nothing` instead of a clean
runtime error; `items[-1]` resolves to the last element. No negative index
below `-len` is rejected. AGENTS §3.2 requires out-of-bounds access to be "a
clean runtime error, never a panic". It is not a panic today, but it is also not
an error.

**Severity: minor.** **Risk: low.** Pinned in `tests/test_lists.rb`.

---

## 10. `repeat ... until`, `unless` and `when` are keywords with no parser rule

`src/lexer.rs` maps `until`/`unless`/`when`, but `parse_statement` has no arm for
`unless` or `when`, and `repeat` (`src/parser.rs`) only accepts
`repeat <n> times`.

Reproduction:

```
$ printf 'unless no then\n  say "u"\nend\n' > /tmp/u.rb && rb run /tmp/u.rb
Error: ParserError: Unexpected token Unless
$ printf 'repeat\n  set n to 1\nuntil n is 3\n' > /tmp/u2.rb && rb run /tmp/u2.rb
Error: ParserError: Unexpected token Newline
```

SPEC.md:305/350/570 specify all three.

**Severity: minor.** **Risk: medium** — each is a grammar addition.

---

## 11. Redblue has no `<`, `>`, `<=`, `>=`, `==`, `!=`

`src/lexer.rs:334-380` tokenises only `+ - * / % ( ) [ ] { }`. The parser has
`BinaryOp::{Less,LessEqual,Greater,GreaterEqual,Equal,NotEqual}`
(`src/parser.rs:957-1042`) but no lexer token can produce them. Comparison is
therefore `is` / `is not` only, and the word forms in SPEC.md:213-224
(`is greater than`) are unreachable — `src/formatter.rs:402` *emits* them, so
`rb format` produces code the parser cannot read.

Reproduction:

```
$ printf 'say 1 < 2\n' > /tmp/c.rb && rb run /tmp/c.rb
Error: LexerError: Unexpected character '<'
$ printf 'set n to 1\nif n is greater than 0 then\n  say "y"\nend\n' > /tmp/c2.rb && rb run /tmp/c2.rb
Error: ParserError: Expected Then but got Identifier("greater")
```

The formatter round-trip is a live defect: `rb format` on a file using `>`-
family comparisons cannot exist (they cannot be parsed), but a file using
`is greater than` also cannot be parsed, so the formatter emits unparseable
code for any comparison it renders.

**Severity: major.** **Risk: high** — `docs/GRAMMAR.md` and `SPEC.md` would
need reconciling with the implementation choice.

---

## 12. The test harness accepted commented-out tests as passing tests

The pre-existing `TestHarness::run_source` (`src/testing/harness.rs`, before this
phase) discovered only `// test "..."` / `// end` markers and passed the
*commented* body lines to `execute_test_code`, where they lex to an empty
program. That is why `tests/*.rb` reported 22 tests / 21 passed while asserting
nothing. The marker convention is still supported for
`tests/discovery_test.rs` and `tests/expect_test.rs`; it is simply no longer the
only way to write a test.

**Severity: blocker (resolved in this phase).** **Risk: low.**
---

## 13. `expect … to contain …` and `assert.equal` — the two other assertion forms named in AGENTS §3.1 — do not work

`AGENTS.md` §3.1 lists three legal assertion forms. Only the first exists.

`src/parser.rs:868-891` (`parse_expect`) accepts `expect <expr> to [be] <expr>`
and nothing else. `contain` is not a keyword, so after `to` the optional-`be`
check fails, `contain` is parsed as a variable reference, and the run dies with
`Unknown variable 'contain'`. There is no containment comparison anywhere in
`src/vm.rs`'s `BinaryOp` match (`src/parser.rs:7-70`).

`assert.equal(a, b)` lexes `assert` as a `Builtin` registered at
`src/stdlib.rs:157` and `.equal` as a member call; the member-call path is
rewritten to `assert_equal` (`src/vm.rs` `call`) which is not a name
`call_builtin` handles.

Reproduction:

```
$ printf 'test "c"\n  expect "abcdef" to contain "cde"\nend\n' > /tmp/c.rb && rb test /tmp/c.rb
1. contains form: FAILED — RuntimeError: Unknown variable 'contain'
$ printf 'set c to 1\ntest "a"\n  assert.equal(1, 1)\nend\n' > /tmp/a.rb && rb test /tmp/a.rb
1. assert form: FAILED — RuntimeError: Unknown function 'assert_equal'
```

Consequence for this phase: all 187 Redblue blocks use `expect <expr> to be
<expr>`, the only assertion form the language has. Containment is expressed by
comparing the whole value (`expect x to be [1, 2, 3]`) or by an explicit loop.

**Severity: major** — the testing contract in AGENTS §3.1 cannot be satisfied.
**Risk: low** for containment (one more `BinaryOp` plus a `contain` keyword);
**low** for `assert.equal` once finding 1 is fixed, since `stdlib::builtin_function`
already has an `assert` arm to route.

---

## 14. `rb format` corrupts programs — it inserts spaces inside string literals

`Formatter::write` (`src/formatter.rs:430-439`) inserts a single space between
any two adjacent writes unless the output already ends in a space or a newline.
It has no notion of "this write is the inside of a literal", so the three writes
that `format_string_literal` performs (`src/formatter.rs:385-389`) emit
`"` → `" ab` → `" ab "`. Every string literal in every formatted program gains a
leading and a trailing space, which silently changes the program's behaviour.

The same rule produces doubled spacing elsewhere (`write("x")` followed by
`write(" to ")` yields `set x  to ...`), and because `format` re-renders from the
AST (`src/formatter.rs:17-25`) rather than from tokens, comments are discarded
outright.

Reproduction:

```
$ printf 'say "ab"\n' > /tmp/t.rb && rb format /tmp/t.rb
say " ab "

$ rb format examples/hello.rb > /tmp/h.rb && rb run /tmp/h.rb
 Hello, World!          # was: Hello, World!
 Hello, World!
$ echo $?
0
```

Consequence for this phase: `rb format --check` reports
`File would be reformatted` for **every** `.rb` file in the repository — all six
`examples/*.rb`, `modules/MathUtils.rb`, and all ten `tests/*.rb` this phase
wrote. No `.rb` file passes, so the check cannot be used as a gate and the suite's
files were not formatted to "fix" it (doing so would rewrite string literals in
the tests themselves). `rb format` is documented in `AGENTS.md` under
"Formatter (rbfmt)" and its output is unrunnable-equivalent to the input.

**Severity: blocker** — `rb format` is a semantics-changing command, not a
cosmetic one, and it currently destroys every string literal in a program.
**Risk: medium** — the fix is a real-token-aware writer, not a one-line patch:
`write` needs a "no space before/after" signal for literal interiors and
punctuation, and comment preservation likely needs the formatter to work from
tokens rather than the AST.

**Suggested acceptance gate:** `rb format` is idempotent and semantics-preserving
on a corpus: for every file `F` in `examples/*.rb` plus a new corpus,
`rb run F` and `rb run <(rb format F)` produce byte-identical stdout, and
`rb format --check` passes on every file in the repository.
