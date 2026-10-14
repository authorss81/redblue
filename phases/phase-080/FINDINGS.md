# phase-080 FINDINGS

Work found while doing phase-080 that does **not** belong to this phase. Recorded
here for the auditor, per AGENTS.md § 1.7. Nothing below was fixed.

## 1. `rb lint` cannot report a failure that only happens when the program runs

`src/linter.rs` now always asks the analyzer whether `rb` could run a file, and
**appends** the refusal to the linter's own findings — it never substitutes it,
so no file that previously produced a finding produces fewer. (The round-1 cut
asked only when the report was otherwise empty, and that made the pairing
conditional on the file being clean; see REPORT.md § Round 1 fixes. Round 2 moved
that append into `lint_program`, so `rb diagnostics` and `rb lint` cannot
disagree; see REPORT.md § Round 2 fixes.) That covers the three static stages —
lexer, parser, analyzer — and nothing beyond them.

A failure that appears only during execution, such as calling a function that
does not exist (`RuntimeError: Unknown function 'not_a_function'`) or indexing
past the end of a list, is invisible to a linter: it reads the syntax tree and
does not run the program. Reporting those would mean running the program, which
is a different tool with different costs and different failure modes.

**Why it matters.** `rb lint` reads as "this file is fine", and a program whose
failure is deferred to run time is not fine. There is no cheap fix that does not
change what a linter is.

**Suggested phase.** Either document the boundary where `rb lint` is documented,
or add a separate `rb check` that runs the analyzer *and* compiles — refusing the
file statically for the classes that can be refused statically — and leave
`rb lint` about the tree.

## 2. `SPEC.md` § Function Call was checked and is fine — recorded so it is not re-raised

`SPEC.md` § Function Call sits directly above the § List Operations block this
phase fixed, and reads:

```redblue
set len to text.length("hello")
set result to math.sqrt(2)
```

The same audit that found `items at 0` and `length of items` were dead syntax
suggested these might be too. They are not: both lines run and print the
documented value (`5`, and `1.4142135623730951`). `text` and `math` are builtin
module records, not variables, so the analyzer resolves them.

**Why it is written down.** It was checked rather than assumed, and the check is
cheap to repeat. Recorded so a later phase does not re-investigate the two lines
next to the ones that were broken.

## 3. A chaining limitation of `items at N`

`items at N` reads the arithmetic after `at` as the index, so a second `at`
belongs to that index: `xs at 1 at 0` is `xs at (1 at 0)` and fails
`RuntimeError: Cannot index non-list`. Stepping twice means binding the
intermediate value (`set row to xs at 1`, then `row at 0`) or writing `xs[1][0]`.

This is documented in SPEC.md § List Operations and `docs/GRAMMAR.md` § 5.2, and
pinned by `tests/index_words_test.rs`
(`edge_the_worded_index_operand_is_read_as_a_whole_expression`), so it is a
decided behaviour rather than a defect. It is listed because a reader who has
just learned `xs[N]` chaining left may reasonably expect `xs at N at M` to chain
too, and the only way to know the rule is to read the paragraph.

The round-2 fix narrowed what the index swallows — `is`, `and` and `or` now stay
outside it, which is what `xs at 0 is 10` needs — and did **not** narrow this.
The two spellings of one operator still differ on associativity, and
`docs/GRAMMAR.md`'s precedence table now gives them separate rows rather than
claiming Left for both.

**Suggested phase.** None needed. Recorded so the auditor does not re-raise it as
an undiscovered bug.

## 4. Stage 2 still cannot compile `might fail <call>`

`bootstrap/compiler.rb`'s `parse_primary` refuses the `might fail` guard. Stage 1
compiles such a program; stage 2 does not. This is pre-existing and unrelated to
`at`/`length of` — it was true before this phase and is unchanged by it.

**What round 2 changed here, and why.** The round-1 fix added a `mightfail` entry
to stage 2's `is_expression_start` so the table would equal
`Parser::kind_starts_expression` entry for entry, and the comment admitted the
entry names a primary stage 2 does not have. Left alone that is a latent trap: the
review showed it firing. `xs at might fail f()` read the word-index in **both**
engines — stage 1 built `index(might fail f())`, stage 2 fell off the end of its
primary chain and refused — so code added by this phase was deciding, from a table
that had no arm behind it, whether a program was well formed.

Three things changed, and only the first is in the language:

1. **A worded operand is read at the additive level** (`Parser::parse_worded_operand`,
   `parse_expression(…, 3, …)` in stage 2), so the new word-index code asks a
   *different* table — `is_operand_start`, which is `is_expression_start` less
   `mightfail` — and that table names only what level 3 can actually read. Stage 1
   and stage 2 agree that `at` in `xs at might fail f()` is the variable it has
   always been, and stage 1 refuses the program for the ordinary reason: `at` is
   unbound.
2. **`parse_primary` refuses `might fail` in a sentence that says which runner
   does compile it**, exactly as it already refuses a function literal. Before
   this, the guard fell through to `unexpected token mightfail`, which names the
   symptom rather than the gap. The refusal is now deliberate, and it is a
   *refusal* — stage 2 exits non-zero, names the construct and writes no `.rbc`.
3. **The disagreement is pinned by a test**
   (`bootstrap_selfhost_test::edge_a_construct_stage_two_cannot_compile_is_refused_rather_than_mistranslated`),
   so a future stage 2 that meets a token it cannot read and carries on — writing
   a file that differs from stage 1's in bytes nobody looks at — fails.

**Why it is still a finding.** Stage 1 compiles the guard and stage 2 refuses it,
and that gap is invisible to every byte-identity test, because those compare
programs both engines accept. There is still no `might fail` program in `corpus/`
and no S2/S3 test can catch the *absence* without one.

**Suggested phase.** Teach `parse_primary` the guard — it needs the `MIGHT_FAIL`
jump, the patched recovery and the `Nothing` constant that
`src/bytecode/codegen.rs` emits — and add one corpus program that uses it, so the
gap becomes a byte-identity case instead of an absence.
