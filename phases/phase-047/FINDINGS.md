# phase-047 — findings

Work done by this phase is in `REPORT.md`. This file carries the things that
were discovered and deliberately **not** done, so the auditor can turn them into
real phases.

## 1. `rbops/verify.sh` is not in this checkout

`AGENTS.md` § 3.4 and the phase prompt both name `./rbops/verify.sh phase-047` as
the fourth gate. There is no `rbops/` directory in this working tree — the
pipeline that dispatched the phase lives outside it, and § 1 of the contract
forbids inspecting it. The gate was therefore **not run**, and `REPORT.md` says
so rather than claiming a pass.

What it is said to cover was run by hand:

- every file in `examples/` and `modules/` exits 0 (9 files, 0 failures);
- `cargo test --all-targets` passes, and that includes
  `tests/discovery_test.rs::edge_project_tests_directory_has_no_failures`, which
  is the suite-level form of the same check over `tests/*.rb`.

If a future phase needs the fourth gate run from inside a checkout, the script
has to be vendored or the phase needs to state the equivalent commands.

## 2. `interp_depth`'s limit branch cannot be reached from source — now pinned anyway

`src/parser.rs` caps the recursion into a fresh `Parser` per substitution at
[`MAX_NESTING_DEPTH`], sharing one counter across the chain with an
`Rc<Cell<usize>>`.

**This corrects what this file said the first time round.** The earlier version
claimed nesting was *structurally* impossible, on the grounds that the outer
lexer resolves escapes before the parser sees the literal, so an inner source
containing a string literal cannot re-lex to what it was written as. That is
backwards. The resolution is what makes it work: `\"` becoming `"` gives the
inner source its own delimiters, and those delimiters make it a text token
whose content can hold a brace pair of its own. `"{\"{\"{a}\"}\"}\""`
substitutes at three levels — pinned by
`tests/interpolation_test.rs::edge_substitution_nests_through_a_text_literal_inside_the_braces`
and again from the parser's side by
`src/parser.rs::edge_interpolation_nests_through_an_escaped_text_literal`.

What genuinely cannot be reached is the *limit*. Each level costs an escaped
copy of the level below, so the source for *n* levels grows like 2ⁿ; 64 levels
is 2⁶³ characters and no file holds one. So the branch is driven directly
instead:
`src/parser.rs::edge_the_interpolation_budget_is_shared_and_bounds_the_chain`
puts the shared counter at `MAX_NESTING_DEPTH`, holds the diagnostic's message
and span to account, and holds that a read which came back — read an
expression or refused to find one — hands the budget back. The counter stays:
it costs one `Cell`, and it is the only thing bounding the recursion if the
grammar ever gains a way to nest that does not pay the doubling.

Records still cannot nest, and that part of the first version was right:
`{{a: 1}}` is the deepest a record reaches, because a record's keys are
identifiers and `{a` is not one.

**A real phase, if one is wanted:** the budget is far larger than any source
can spend, which means it is untestable through the language. If that matters,
lower `MAX_NESTING_DEPTH` for interpolation to something a source can reach —
three is the deepest any source can reach today — and let the test drive real
programs instead of the counter.

## 3. Braces cannot be escaped

`"\{x\}"` is `{x}`: `Lexer::read_text`'s catch-all arm
(`Some(c) => text.push(c)`, `src/lexer.rs:296`) turns `\{` into `{`, and `\{`
then reads as a substitution. There is no spelling for a literal brace pair that
would otherwise read as an expression.

This is why `tests/test_text.rb` uses an *unbound* name (`{}`, `{1 + }`,
`{x y}`) for its "not well formed" cases rather than an escape, and why
`examples/formats.rb:4` and `tests/suite.rb:129` keep working: their JSON
documents do not parse as expressions, so they stay text.

**A real phase, if one is wanted:** decide whether `\{` should escape a brace.
`escape_text` in `src/formatter.rs:979` would then need to write it, and the
lexer's catch-all would need a case for it. Both are language-surface changes
and need a phase that says so.

## 4. The stage-2 compiler's `refuse` prints before it stops

`bootstrap/compiler.rb:111` writes its diagnostic and then indexes
`refuse_stop_here[1]` on a one-element list, so a refusal raises
`Index 1 is out of bounds` rather than stopping. `try`/`catch` catches it, which
is what the new `interp_read` relies on, but the user still sees the diagnostic
on stderr and a `0` on stdout.

Consequence for this phase: `rb run bootstrap/compiler.rb some.rb out.rbc` on a
program holding a brace pair whose contents the stage-2 parser refuses prints
one line on stderr and `0` on stdout, then compiles correctly. No example,
module or corpus program does — all of them are silent — and the `.rbc` bytes
are unaffected, which is what the S3 fixed point checks.

**A real phase, if one is wanted:** give `refuse` a way to stop the program it
is in. Every error path in the stage-2 lexer and parser currently reports the
same way, so this is one change with broad reach.

## 6. The stage-2 compiler still scans for a matching `}` the quadratic way

`src/parser.rs`'s `unmatched_opens` settles, in one backward pass, which `{`s
of a literal no `}` closes. `bootstrap/compiler.rb`'s `interp_close`
(`:2070`) answers the same question the slow way: from each `{` it walks
forward counting depth until the count reaches zero or the literal runs out. A
literal that is *all* opens — `set s to "{{{{…"` — therefore costs the rest of
the literal per open, which is the quadratic the stage-1 fix removed.

Left alone on purpose, for two reasons. Stage 1 and stage 2 must agree on every
*answer*, and they still do — the change was to how long stage 1 takes to reach
one, not to what it says, so the S3 fixed point is untouched. And stage 2 is
itself written in Redblue and run by a tree-walking interpreter, so the port is
a reverse loop over a byte list inside the compiler that compiles itself: real
code, on the one path where a mistake is hardest to see, for a program shape no
example, module or corpus file writes.

**A real phase, if one is wanted:** give `interp_close` a sibling
`interp_unmatched(b)` that walks `b` right to left with a depth counter, the
same shape as the Rust one, and have `parse_text_literal` consult it before
falling back to `interp_close`. Check it against
`tests/bootstrap_selfhost_test.rs` — that test is what makes the port safe.

## 7. The corpus generator comment was stale

`tests/common/generator.rs` carried the comment "`{interp}` is documented and
unimplemented: the braces are printed". Corrected here because it was in the
diff's blast radius. The corpus *programs* were not changed, and their two
`.expected` files were updated to the output the programs should have produced
all along (`hi world`, `Hello, World!`) — a golden recorded from a broken
interpreter, not a threshold lowered.

## 8. `phases/phase-003/FINDINGS.md`, `phase-008` and `phase-016` are now stale

All three record this defect as severity `blocker` and all three are now fixed.
They are reports, not code, so this phase did not edit them.