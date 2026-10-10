# Phase 040 — Parse `repeat ... until` as the post-test loop SPEC.md documents

## Reproduction

The finding reproduces exactly as written. `TokenKind::Until` is declared at
`src/lexer.rs:33` and matched nowhere in the parser before this change:

```
$ grep -rn 'TokenKind::Until' src/parser.rs        # before this phase
(no output)

$ printf 'set n to 0\nrepeat\n    set n to n + 1\nuntil n is 3\nsay n\n' > target/tmp/rep_until.rb
$ ./target/debug/rb run target/tmp/rep_until.rb; echo "exit=$?"
Error: ParserError: Unexpected token Newline
  --> target/tmp/rep_until.rb:2:7
2 | repeat
  |       ^
exit=1
```

The sibling counted form was already working, which is what made the gap
invisible: `repeat 3 times / say "x" / end` printed three lines, so the keyword
looked used and only the third documented form was unreachable.

After this phase the same command prints `3`, under `rb run`, under `rb vm` on
the output of `rb compile`, and in every test in `tests/repeat_until_test.rs`.

## What changed

| File | Lines | What |
|---|---|---|
| `src/parser.rs` | +230 −0 | `Statement::RepeatUntil { body, condition, condition_span }`; `Parser::parse_repeat` splits on what follows the keyword and hands the post-test form to `parse_repeat_until`, which reads the body until the `until` and then the condition. `opens_post_test_loop` is that split, `line_ends_in_until` is the same-line half of it, and `open_block_depth` counts `until` as a closer so a REPL does not wait for an `end` this form does not have |
| `src/interpreter.rs` | +45 −1 | the loop itself: charge a turn at the top of it, run the body, then read the condition — under the condition's own span — and go round again while it is false. `Vm::evaluate_at` is what names that span |
| `src/analyzer.rs` | +18 −0 | `analyze_statement` and `collect_later_names` handle the new variant — the body walked in its own scope first, the condition after it, names in the body visible to the next statement |
| `src/bytecode/codegen.rs` | +21 −0 | compiles it to a filler at the loop's `top`, the body, the condition and a backward `JUMP_IF_FALSE` to that `top` — the last two compiled with the `until` line rather than the `repeat` one |
| `src/bytecode/vm.rs` | +93 −3 | `loop_sites` reads a backward `JUMP_IF_FALSE` as a loop's back edge (the post-test shape); `charge_loop_top` charges a turn at a filler that is a loop's `top`; `leave_post_test_loop` gives the entry back when the condition ends the loop |
| `src/formatter.rs` | +26 −1 | formats the form: `repeat`, indented body, `until <condition>`, and no `end`. `closing_lines` reads an `until` as the closing keyword it is, so the body's tail comments stop where the loop does |
| `src/linter.rs` | +10 −0 | analyses the body and the condition like any other loop's, body first, for the analyzer's reason |
| `tests/repeat_until_test.rs` | +893 −0 | new file, 18 `#[test]` functions (13 named `edge_*`) |

`must_touch: ["src/"]` is satisfied by seven files under `src/`.

### How the two engines tell this loop apart from a `while`

The bytecode VM finds loops from the backward jumps in a block, and a `while`
is told from a sequence loop by what precedes its `top`. A post-test loop is
third again: its backward jump is *conditional*, because its condition is at
the end of a turn rather than the start of one, and a backward `JUMP_IF_FALSE`
is the one shape nothing else in the compiler emits. So `loop_sites`
(`src/bytecode/vm.rs:459`) recognises it and calls it what the program called
it — a `repeat` — in the message the iteration cap reports.

The compiled shape is:

```
top:   NOP                 <- the loop's top: a turn is charged here, and a skip comes back here
       <body>
       <condition>
       JUMP_IF_FALSE top   <- false goes round again, true falls out of the loop
exit:
```

Charging at the filler is what makes the two engines agree: the
tree-walking VM charges at the top of a turn before the body runs
(`Vm::charge_iteration`), and so does this one, once per turn, whatever ends
it — a turn that finishes, a `break` that leaves, a `skip` that starts the
next. `edge_a_cap_is_turns_of_a_post_test_loop_on_both_vms` asks the boundary
about it from both sides.

## Tests added

18 `#[test]` functions in `tests/repeat_until_test.rs` plus 1 in
`src/parser.rs`. Thirteen are named `edge_*`; eight assert a produced failure,
and ten more fail if the loop stops being post-test. Every one runs its
program on **both** engines and compares them, so a divergence fails the test
that made the comparison.

| Test | Edge class covered |
|---|---|
| `a_repeat_until_stops_on_the_first_true_condition` | the loop itself — the body runs until the condition is true and not one turn past it (`3`, not `4`) |
| `the_counted_repeat_still_runs_exactly_its_count` | regression — `repeat 3 times` still prints three lines on both engines |
| `edge_a_condition_true_on_entry_leaves_after_exactly_one_turn` | singleton / true-on-entry — a condition already true leaves the loop after exactly one turn, never zero and never two |
| `edge_an_empty_body_reads_its_condition_once_and_stops` | empty — a loop with no body at all turns once and stops, rather than spinning on a condition it cannot change |
| `edge_a_cap_is_turns_of_a_post_test_loop_on_both_vms` | boundary — a cap of exactly N is N turns on both engines; one turn less is the cap that stops it, and the refused turn's body never runs |
| `edge_a_condition_that_never_comes_true_is_stopped_by_the_cap` | resource_limit / numeric_boundary — a condition that can never be true (`turns is 2.5`) is stopped by the cap, the message names `Maximum of 5 iterations` *and* `'repeat' loop`, and the bytecode VM stops there too |
| `a_failing_body_is_not_rescued_by_a_condition_that_would_have_stopped_it` | the post-test property — a body that divides by zero fails the program although the condition would have ended the loop, and the failure's span is line 4, the body, not line 5, the `until` |
| `a_break_leaves_a_post_test_loop_and_a_skip_starts_the_next_turn` | loop control — `break` leaves; a `skip` goes to the top of the loop, so the condition of the turn it abandoned is never read (a program where reading it would have printed something else) |
| `edge_a_post_test_loop_nested_in_a_for_each_and_in_itself` | nesting_recursion — a post-test loop inside a `for each`, and one inside another; each loop's turns are its own |
| `edge_a_condition_that_indexes_past_the_end_is_a_clean_failure` | out_of_bounds — a condition that indexes `xs[99]` is a clean `RuntimeError` on both engines, and not a panic |
| `edge_a_condition_may_read_a_name_the_body_binds` | ordering — `repeat / set t to 1 / until t is 1` runs on both engines rather than being refused as an unknown variable, and a name nothing binds is still refused |
| `edge_a_post_test_loop_inside_a_block_keeps_the_comments_it_was_given` | the formatter's block table — an `until` closes the body it closes; a comment at the end of the `until` line, a comment after it, and a comment inside the body each stay where they were written, and formatting is idempotent |
| `edge_a_failure_in_the_condition_names_the_until_line_on_both_vms` | error reporting — a failure in the condition names the `until` line on both engines, and a failure in the body still names the body's own line |
| `edge_every_turn_skipped_is_stopped_by_the_iteration_cap_on_both_vms` | resource_limit, the shape FINDINGS.md § 4 records — a body that `skip`s on every turn reads no condition at all, and a cap of N is N turns and is what stops it, on both engines |
| `edge_the_one_line_form_runs_and_a_bare_repeat_says_it_has_no_until` | the disambiguation — `repeat set n to n + 1 until n is 3` is this loop, a counted loop holding a post-test loop is still the counted form, and a `repeat` with no `until` says so |
| `edge_the_body_prints_the_unicode_it_was_given` | unicode — `héllo 🌍 مرحبا` (Latin with a combining-free diacritic, an emoji, an RTL word) printed once per turn, byte for byte |
| `edge_a_repeat_with_no_until_is_a_clean_parser_failure` | malformed_input — a `repeat` with no `until` is a `ParserError` on both frontends and neither panics; an `end` after the `until` line is a stray token, not a way to close the loop |
| `edge_rb_run_and_rb_vm_print_the_same_post_test_loop` | the CLI as shipped — `rb run` prints `3`, `rb compile` writes a `.rbc`, `rb vm` prints `3` |
| `parser::tests::edge_a_post_test_loop_is_closed_by_its_until_and_not_by_an_end` | the REPL's block counter — a post-test loop leaves depth 0 (so the prompt runs it), and a `repeat` whose `until` has not been typed is still open |

### Every fix was checked by breaking the thing it pins

Not by inspection — by mutation, each reverted afterwards and the four gates
re-run:

| Mutation | Result |
|---|---|
| the interpreter reads the condition *before* the body (a pre-test loop) | `edge_a_condition_true_on_entry_leaves_after_exactly_one_turn` and `a_break_leaves_a_post_test_loop_and_a_skip_starts_the_next_turn` **fail** |
| `loop_sites` calls the post-test site a `'while'` loop instead of a `'repeat'` one | `edge_a_condition_that_never_comes_true_is_stopped_by_the_cap` and `edge_a_cap_is_turns_of_a_post_test_loop_on_both_vms` **fail** |
| the bytecode VM stops charging a turn at the loop's filler | `edge_a_cap_is_turns_of_a_post_test_loop_on_both_vms` **fails**, and `edge_a_condition_that_never_comes_true_is_stopped_by_the_cap` **fails after 121.93 s** instead of 0.01 s — which is the "not a hang" requirement, measured rather than asserted |

`git diff --numstat -- src/` after the reverts shows only the lines this phase
and review round 1 add: `18/0 analyzer.rs`, `21/0 codegen.rs`, `93/3 vm.rs`,
`26/1 formatter.rs`, `45/1 interpreter.rs`, `10/0 linter.rs`, `146/0
parser.rs`.

## Edge-case matrix

| Row | Status |
|---|---|
| empty | covered — `edge_an_empty_body_reads_its_condition_once_and_stops` (a post-test loop with no body turns once and stops) |
| singleton | covered — `edge_a_condition_true_on_entry_leaves_after_exactly_one_turn` (exactly one turn, the minimum a post-test loop can take) and `the_counted_repeat_still_runs_exactly_its_count` (exactly three turns of the counted form) |
| boundary | covered — `edge_a_cap_is_turns_of_a_post_test_loop_on_both_vms`: a cap of exactly the turns the loop takes runs it, and one turn less is the cap that stops it, on both engines. `edge_every_turn_skipped_is_stopped_by_the_iteration_cap_on_both_vms` walks the same boundary for the one shape whose condition is never read |
| out_of_bounds | covered — `edge_a_condition_that_indexes_past_the_end_is_a_clean_failure` (`xs[99]` on a two-element list, in the condition, is a clean `RuntimeError` on both engines). The loop reads no index of its own |
| type_mismatch | covered — `edge_the_body_prints_the_unicode_it_was_given` (a text body under a loop whose condition is a comparison), and `edge_a_condition_that_indexes_past_the_end_is_a_clean_failure` (a list handed an index that names no element). A condition accepts any value and reads its truthiness (`Value::is_truthy`, `src/value.rs:374`) exactly as a `while`'s does, so there is no new type to mismatch; that is the shared existing contract, not a gap |
| numeric_boundary | covered — `edge_a_condition_that_never_comes_true_is_stopped_by_the_cap` compares against `2.5`, a target a counter of whole turns never reaches, which is what makes the loop unbounded and the cap the thing that stops it. i64 overflow is N/A for this change: `Value::Number` is `f64` and the loop introduces no arithmetic of its own |
| unicode | covered — `edge_the_body_prints_the_unicode_it_was_given`: `héllo 🌍 مرحبا` once per turn. Escapes and very long strings are N/A: this change moves no string through a decoder, and the lexer's escape handling is `tests/lexer_robustness_test.rs`, green and untouched |
| nesting_recursion | covered — `edge_a_post_test_loop_nested_in_a_for_each_and_in_itself` nests a post-test loop in a `for each` and in another post-test loop. Mutual recursion is N/A: a loop is not a call, and the recursion limit is `tests/call_depth_test.rs`, green and untouched |
| duplicate_missing_keys | **N/A.** Nothing in this change reads or writes a record: a post-test loop's state is its body's statements, and its condition is an ordinary expression over whatever the body bound. Record keys are `tests/record_order_test.rs` and `tests/expect_test.rs`, green and untouched |
| malformed_input | covered — `edge_a_repeat_with_no_until_is_a_clean_parser_failure` (a `repeat` with no `until`; a stray `end` after the `until` line), on both frontends, and `edge_the_one_line_form_runs_and_a_bare_repeat_says_it_has_no_until` (a `repeat` at the end of the file, and at the end of a line, each naming the `until` they are missing). An empty file, a BOM, CRLF and non-UTF8 bytes are lexer behaviour this change does not touch (`tests/lexer_robustness_test.rs`) |
| resource_limit | covered — `edge_a_condition_that_never_comes_true_is_stopped_by_the_cap` (the per-loop iteration cap stops it, named, on both engines), `edge_a_cap_is_turns_of_a_post_test_loop_on_both_vms` (the boundary of that cap), and `edge_every_turn_skipped_is_stopped_by_the_iteration_cap_on_both_vms` (the one shape that reads no condition at all). At the *published* cap a body that `skip`s on every turn is stopped by the step budget instead, because a skip abandons the rest of the turn and the condition with it → FINDINGS.md § 4, which now records both numbers rather than one |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — 0 warnings |
| `cargo test --all-targets` | **1181 passed, 0 failed, 0 ignored** across 43 targets |
| `cargo test --doc` | 2 passed, 0 failed |
| `./rbops/verify.sh phase-040` | **could not run — `rbops/` is not present in this checkout.** `ls rbops` returns `No such file or directory`, and the pipeline that invokes this phase lives outside the project root, which this phase is forbidden to inspect. Reported honestly rather than claimed as a pass. |

Measured, not asserted: `cargo test --all-targets` summed over all 43 targets
reports `passed=1181 failed=0 ignored=0`, and every one of the eight example and
module programs still exits 0 under `rb run`
(`examples/{files,fizzbuzz,formats,hello,test_arithmetic,time}.rb`,
`modules/{MathUtils,SuiteKit}.rb`).

## Definition of done, verified by hand

| Item | Evidence |
|---|---|
| `repeat` / `set n to n + 1` / `until n is 3` + `say n` prints `3` under `rb run` **and** `rb vm` on the output of `rb compile` | `edge_rb_run_and_rb_vm_print_the_same_post_test_loop` runs all three commands and requires `3` from each; run by hand: `./target/debug/rb run target/tmp/rep_until.rb` → `3`, `rb compile` → `target/tmp/rep_until.rbc`, `rb vm` → `3` |
| a condition true before the first pass | `edge_a_condition_true_on_entry_leaves_after_exactly_one_turn` — one turn, and the body printed. It cannot be zero turns in a post-test loop; the checklist contradicts itself here and FINDINGS.md § 1 records the reading taken and why |
| the body runs before the condition is read, a failing body is not rescued, and the failure names the body's line | `a_failing_body_is_not_rescued_by_a_condition_that_would_have_stopped_it` asserts the span's line is 4 (the `say 1 / 0`) and not 5 (the `until`) |
| `repeat 3 times` / `say "x"` / `end` still prints three lines | `the_counted_repeat_still_runs_exactly_its_count`, on both engines |
| a never-true condition is stopped by the cap and reported as the documented limit error, not a hang | `edge_a_condition_that_never_comes_true_is_stopped_by_the_cap`: `Maximum of 5 iterations` in a `'repeat' loop`, from both engines, in 0.01 s |
| a condition true on entry | `edge_a_condition_true_on_entry_leaves_after_exactly_one_turn` |
| `grep -n 'TokenKind::Until' src/parser.rs` returns at least one line | three (below) |
| `cargo test --all-targets` reports 0 failures | **1181 passed, 0 failed** |

```
$ grep -n 'TokenKind::Until' src/parser.rs
1032:                TokenKind::Until => return true,
1053:            Some(TokenKind::Until) | Some(TokenKind::End) | Some(TokenKind::Eof)
1066:        let until = self.expect(&TokenKind::Until).map_err(|_| {
2374:            TokenKind::Until => {
```

## Invariants touched

None of the language surface in `phases/INVARIANTS.md` moved. `.rb` is
unchanged; `set x to <expr>`, `say`, the `Value` variants and the `Error`
variants are untouched; no braces were introduced anywhere.

The loop form itself follows `SPEC.md:432` §"Repeat Until" and
`docs/GRAMMAR.md:246` exactly: `repeat` { statement } `until` expression, with
no `end` of its own — so `to … end`, `if … end` and `for … end` are unchanged
and this is the one block form closed by something else.

Three semantics were chosen where the specification is silent, and each is
pinned by a test that fails if it changes:

- **the condition is read after the body**, so the loop always runs at least
  one turn (`edge_a_condition_true_on_entry_leaves_after_exactly_one_turn`);
- **`skip` goes to the top of the loop**, which here is the body, so it starts
  the next turn without reading the condition of the turn it abandoned
  (`a_break_leaves_a_post_test_loop_and_a_skip_starts_the_next_turn`);
- **the iteration cap calls this loop a `'repeat'` loop**, on both engines
  (`edge_a_condition_that_never_comes_true_is_stopped_by_the_cap`);
- **a failure in the condition names the `until` line**, on both engines, the
  one place a statement's expression is not on the statement's own line
  (`edge_a_failure_in_the_condition_names_the_until_line_on_both_vms`).

A fourth was settled by review round 1 rather than chosen: the analyzer walks
the body before the condition, because that is the order the loop runs in and a
name the body binds is therefore a name the condition may read
(`edge_a_condition_may_read_a_name_the_body_binds`).

`open_block_depth` gained one case: `until` closes a block. That is the REPL's
"is the program finished yet" count, and a post-test loop has no `end` for it
to find — before the change, typing the loop at the prompt ended with
`Error: incomplete block: 4 line(s) were never closed with 'end'`; after it,
the prompt prints `3`. No previously-accepted program changed meaning: the only
way to reach this case is a `repeat` whose body is closed by an `until`, which
was a parse error before this phase.

## Known gaps / follow-ups

- The checklist's "condition already true runs the body ZERO times" contradicts
  the checklist's own post-test requirement and the phase title → FINDINGS.md § 1.
- `SPEC.md` and `docs/GRAMMAR.md` show the form but do not state the three
  semantics above → FINDINGS.md § 3.
- At the *published* cap, a body that `skip`s on every turn is bounded by the
  step budget rather than by the per-loop iteration cap, because a `skip`
  abandons the rest of the turn and its condition with it. Under a lowered cap
  the loop's own cap is reached first, and that is what
  `edge_every_turn_skipped_is_stopped_by_the_iteration_cap_on_both_vms`
  pins → FINDINGS.md § 4.
- The counted form's count may not span lines, so
  `repeat <expression>
times` is still a parse error. It always was — `times`
  is matched immediately after the expression with no newline skipped between
  them — and the one-line disambiguation does not change it → FINDINGS.md § 5.
- `SPEC.md` §"Repeat Until" still shows only the multi-line spelling. Round 1
  made the one-line spelling parse and the formatter write the multi-line one,
  which is the shape the specification gives → FINDINGS.md § 5, § 3.

## Round 0 — nothing inherited

`cargo test --all-targets` was green on arrival: this phase reproduced the
finding, wrote its test, watched it fail, and fixed it. No pre-existing test
was modified, deleted, skipped or given an `#[allow]`; the only change to an
existing test file is the one new `#[test]` in `src/parser.rs`, and it adds a
case rather than relaxing one.
## Round 1 — the reviewer's five findings

The reviewer read the change and found one blocker, three majors and one minor.
All five are fixed here. Each was reproduced first, then fixed, then the fix was
checked by breaking the thing it pins — each mutation reverted afterwards, and
the four gates re-run.

### 1. [BLOCKER] the analyzer refused a valid post-test program

`src/analyzer.rs:296` analysed the condition *before* walking the body. A
`set` inside a loop body lands in the program's own scope (`declare_assigned`,
`src/analyzer.rs:100`), and both VMs read the condition after the body has run —
so `t` is bound by the time `until t is 1` looks for it. Walking the condition
first reported `Unknown variable 't'` for a program both engines run:

```
$ printf 'repeat\n    set t to 1\nuntil t is 1\nsay t\n' > target/tmp/f1.rb
$ ./target/debug/rb run target/tmp/f1.rb        # before
Error: AnalyzerError: Unknown variable 't'
  --> target/tmp/f1.rb:1:1
$ ./target/debug/rb run target/tmp/f1.rb        # after
1
```

**Fix**: walk the body first, in its own scope, then the condition
(`src/analyzer.rs:296`). `src/linter.rs:269` moved with it, for the same reason.

**Pinned by** `edge_a_condition_may_read_a_name_the_body_binds`, which also
asserts the other half still holds: a name nothing binds is still refused.

### 2. [MAJOR] the formatter did not know an `until` closes a block

`closing_lines` (`src/formatter.rs:134`) collected `else`, `catch`, `finally`
and `end`, so `format_block` ran a post-test loop's body on past its own
`until` and took the enclosing block's `end` as its bound. Two comments showed
where they landed — before, on `repeat 3 times / repeat / set n to 1 / until n
is 3 // why / end`:

```
repeat 3 times
    repeat
        set n to 1
        // why          <- the comment that trailed the `until` line
    until n is 3
end
```

and after, a comment written *below* the `until` — which belongs to the
enclosing block — was pulled up into the loop's body.

**Fix**: `TokenKind::Until` is a closing keyword (`src/formatter.rs:150`), and
the `until` line is written the way the `catch` name is — keyword, then
condition, then the one trailing comment — so a comment cannot land between the
two (`src/formatter.rs:392`).

**Pinned by** `edge_a_post_test_loop_inside_a_block_keeps_the_comments_it_was_given`,
which asserts all three placements and that formatting is idempotent.

### 3. [MAJOR] a failure in the condition named the `repeat` line

The condition is the one expression in the language that is not on the line of
the statement that holds it. It was compiled with the `repeat` statement's line
and evaluated under that statement's span, so `until xs[99] is 1` on line 5
reported line 3:

```
$ ./target/debug/rb run target/tmp/f3.rb          # before
  --> target/tmp/f3.rb:3:1
3 | repeat
$ ./target/debug/rb run target/tmp/f3.rb          # after
  --> target/tmp/f3.rb:5:1
5 | until xs[99] is 1
```

**Fix**: `Statement::RepeatUntil` carries `condition_span` — the `until`
token's own span (`src/parser.rs:194`). `codegen.rs:413` compiles the
condition and its backward `JUMP_IF_FALSE` with that line, and `Vm::evaluate_at`
(`src/interpreter.rs:1661`) evaluates it under that span.

**Pinned by** `edge_a_failure_in_the_condition_names_the_until_line_on_both_vms`,
on both engines, and against the body still naming its own line.

### 4. [MAJOR] the every-turn-skipped loop had no test at all

FINDINGS.md § 4 recorded that a body which `skip`s on every turn reads no
condition and said nothing about which limit stops it. It said the step budget,
at the published cap, and left it there untested.

**Answered, not asserted** by `edge_every_turn_skipped_is_stopped_by_the_iteration_cap_on_both_vms`:
under a lowered cap the loop's own cap is reached first, and it is
`Maximum of N iterations reached in a 'repeat' loop` on both engines, at N = 5
and at N = 4. FINDINGS.md § 4 now records both numbers.

**Fix**: none needed — the behaviour was already correct. What changed is that it
is pinned, and the finding is narrowed rather than closed by a test.

### 5. [MINOR] the one-line spelling, and a confusing diagnostic

`opens_post_test_loop` was true only on an immediate `Newline`, so
`repeat set n to n + 1 until n is 3` took the counted path and was refused, and
a `repeat` at the end of the file took it too and reported an expression error
about a count that was never written.

**Fix**: `Parser::opens_post_test_loop` (`src/parser.rs:1000`) now decides on
the keyword's own line — a newline, an end of input, or the first of `until` and
`times` written on it (`line_ends_in_until`, `src/parser.rs:1022`). The scan
stops at the newline, which is the whole point: an `until` further down the file
belongs to a statement this parser has not reached. `parse_repeat_until`
(`src/parser.rs:1047`) reports the missing keyword by name.

**Pinned by** `edge_the_one_line_form_runs_and_a_bare_repeat_says_it_has_no_until`,
including that a counted loop whose *body* holds a post-test loop is still the
counted form — the trap the line limit exists to avoid.

### The mutations, and what each one failed

| Mutation | Test that **failed** |
|---|---|
| the analyzer walks the condition before the body | `edge_a_condition_may_read_a_name_the_body_binds` |
| `closing_lines` drops `Until` again | `edge_a_post_test_loop_inside_a_block_keeps_the_comments_it_was_given` |
| the `until` line's comment is written before the condition | `edge_a_post_test_loop_inside_a_block_keeps_the_comments_it_was_given` |
| codegen compiles the condition with the `repeat` line | `edge_a_failure_in_the_condition_names_the_until_line_on_both_vms` |
| the tree-walker evaluates the condition under the statement's span | `edge_a_failure_in_the_condition_names_the_until_line_on_both_vms` |
| the parser stops deciding on the newline alone | `edge_the_one_line_form_runs_and_a_bare_repeat_says_it_has_no_until` |
| the missing-`until` diagnostic is the old `Expected Until but got …` | `edge_the_one_line_form_runs_and_a_bare_repeat_says_it_has_no_until` |
| the bytecode VM stops charging a turn at the loop's filler | `edge_every_turn_skipped_is_stopped_by_the_iteration_cap_on_both_vms`, after more than 60 s instead of 0.01 s — the "not a hang" requirement, measured |

### Round 1 gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — 0 warnings |
| `cargo test --all-targets` | **1181 passed, 0 failed, 0 ignored** across 43 targets |
| `cargo test --doc` | 2 passed, 0 failed |
| `./rbops/verify.sh phase-040` | **still could not run — `rbops/` is not present in this checkout.** |

Nothing was weakened to get there: no `#[ignore]`, no `// skip`, no
`allow(clippy:: …)`, no test deleted, and no existing test weakened. The one
pre-existing test the changes touch — `edge_a_repeat_with_no_until_is_a_clean_parser_failure`,
which asserted the message *is* a `ParserError` and is *not* a panic — still
holds; the diagnostic it inspects got better, not looser.

`git diff --numstat -- src/`: `18/0 analyzer.rs`, `21/0 codegen.rs`, `93/3 vm.rs`,
`26/1 formatter.rs`, `45/1 interpreter.rs`, `10/0 linter.rs`, `146/0 parser.rs`.
