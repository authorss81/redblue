# phase-040 findings

Work that belongs to a future phase. Recorded here so the auditor can promote
it with file:line evidence, per AGENTS.md § 1.7.

## 1. The phase's own Definition of Done contradicts itself about a condition true on entry

**Severity: minor for the language, blocking for the checklist. Resolved in
favour of the post-test loop, because three other statements in the same
document say post-test.**

`phases.json`'s checklist for phase-040 asks for both of these:

- *"the body runs before the condition is read, so a body that raises is NOT
  rescued by a condition that would have stopped it, and the failure names the
  body's line"* — a statement that is only true of a post-test loop, and
  `edge_a_condition_true_on_entry_leaves_after_exactly_one_turn` plus
  `a_failing_body_is_not_rescued_by_a_condition_that_would_have_stopped_it` pin
  it.
- *"a `repeat ... until` whose condition is already true before the first pass
  runs the body ZERO times and prints nothing"* — a statement that is only true
  of a **pre**-test loop. A post-test loop cannot both run its body before
  reading its condition and skip the body when the condition is already true.

The phase title, the Goal, the finding's own evidence, `SPEC.md:432` §"Repeat
Until" and `docs/GRAMMAR.md:246`
(`repeat_until = 'repeat' { statement } 'until' expression`) all say post-test,
and the body-then-condition production is what the last two of those show. This
phase implements post-test, so a condition true on entry costs exactly one turn
— the minimum a post-test loop can cost — rather than zero. The "condition true
on entry" edge case required by the checklist is covered; what it asserts is one
turn, not zero.

If a zero-turn reading is wanted, it is a different loop (`while` already is
one) and needs its own phase that re-opens the language design.

## 2. `repeat ... until` has no `... end` block of its own, so two tables that assume every block has one are now incomplete

**Severity: minor. One was fixed here, the other is a limit of what a token
scan can see.**

- `open_block_depth` (`src/parser.rs`) is what a REPL reads to decide whether
  the program it has been given is finished. It counted `repeat` as an opener
  and only `end` as a closer, so a post-test loop typed at the prompt asked for
  an `end` the form is never written with. **Fixed in this phase**
  (`src/parser.rs:2312`, with `edge_a_post_test_loop_is_closed_by_its_until_and_not_by_an_end`).
- `Parser::opens_block` (`src/parser.rs:454`) charges one level of the block
  budget per block. `repeat` is still in it, so a post-test loop is charged a
  level — which is right, its body is parsed by recursing back into
  `parse_statement`. No change needed; recorded so a later reader does not
  "fix" it by removing `Repeat` from that table.

## 3. `docs/GRAMMAR.md` and `SPEC.md` are accurate but silent on the semantics this phase chose

**Severity: minor. Documentation gap, not a defect.**

`SPEC.md:432` shows the form and `docs/GRAMMAR.md:246` gives the production,
and neither says what happens to a condition that is true on entry, what
`skip` does inside the loop, or what the iteration cap calls it. This phase
chose, and documented in `REPORT.md`:

- the body runs before the condition is read, so the loop always runs at least
  one turn;
- `skip` goes to the top of the loop, which here is the body, so it starts the
  next turn without reading the condition of the turn it abandoned;
- the per-loop iteration cap names this loop a `'repeat'` loop, on both engines.

A follow-up phase should add those three sentences to `SPEC.md` §"Repeat Until"
so the language's specification-by-example covers them.

## 4. A `repeat ... until` in which every turn is skipped is bounded by the step budget at the published cap

**Severity: minor. Not a hang; a documented limit error either way. Narrowed
and pinned in review round 1.**

Both engines charge a turn at the top of the loop — the tree-walking VM in
`Vm::charge_iteration`, the bytecode VM at the filler its `top` is — and a
`skip` jumps to that same `top`, so a skipped turn is charged like any other
and `edge_a_cap_is_turns_of_a_post_test_loop_on_both_vms` pins that. What a
skip *does* skip is the rest of the turn, the condition included, so a body
that skips on every turn changes nothing and its loop reads no condition at
all.

What stops it depends on which limit is reached first, and the two are
separate numbers:

- at the **published** cap, the step budget is the one that gets there, because
  a skipped turn still costs a statement marker or two plus the turn itself,
  and ten million steps is a million turns or so of a loop with no other work
  in it. The error is `Step budget of N reached before the program finished`,
  and both engines say it — which is the property that matters here.
- under a **lowered** per-loop cap, the loop's own cap is reached first and the
  error is `Maximum of N iterations reached in a 'repeat' loop`, named as a
  `'repeat'` loop on both engines.
  `edge_every_turn_skipped_is_stopped_by_the_iteration_cap_on_both_vms` pins
  that, with caps of four and of five, which is what makes the review round's
  "which limit stops it is untested" gap a measured answer rather than an
  assertion.

Not fixed in this phase because making it cheaper would mean a `skip` not
costing a turn, which is the defect phase-039-era comments at
`src/bytecode/vm.rs:1755` describe fixing for every other loop.

## 5. The one-line spelling of `repeat ... until`, and the diagnostic a `repeat` with no `until` gets

**Severity: minor. A limit of the disambiguation. Resolved in review round 1.**

`repeat` opens two forms, and `Parser::opens_post_test_loop`
(`src/parser.rs:1000`) tells them apart by what follows the keyword. Looking for
the `until` anywhere would not do: it is at the *end* of the body, and a counted
loop whose body holds a post-test loop of its own has one too, at a nesting
level the parser is nowhere near yet.

That leaves a rule that only ever looks at the keyword's own line:

- a `newline` after `repeat` opens a post-test body on the lines below;
- anything else is settled by the first of `until` and `times` written on that
  line, so `repeat 3 times` is the counted form and
  `repeat set n to n + 1 until n is 3` is the post-test one. The counted
  form's count cannot span lines — it never could, because `times` is matched
  immediately after the expression and a newline was never skipped between
  them, so nothing was decided here that was not already true;
- nothing at all after the keyword — a `repeat` at the end of the file — is a
  post-test loop that never got its `until`, and says so:

  ```
  $ printf 'set n to 0\nrepeat' > target/tmp/bare.rb && rb run target/tmp/bare.rb
  Error: ParserError: A `repeat` loop is closed by an `until <condition>`, and this one has not got one
  ```

  Before this it took the counted path and reported an expression error about a
  count that was never written.

`edge_the_one_line_form_runs_and_a_bare_repeat_says_it_has_no_until` pins all
three, and that the formatter's spelling of the one-liner is the multi-line one
the grammar gives.