# Phase 088 — Make the reachability-only tests assert the value they reach for

## What changed

The finding reproduces. Tests were asking two engines whether a program *ran*, and
answering `Ok` was counted as proof the program was right. This diff makes those
tests read what the program answered, through `Vm::take_output` /
`BytecodeVm::take_output` and the value the run hands back, and holds each one to
a value written out literally in the test — and, where the answer must not exist
at all, holds it to a **refusal** that a caller can act on.

This round adds the missing half of that contract: a **failure channel** — four
tests whose subject is a program that must *not* run, each asserting the refusal
with `assert!(result.is_err(), …)`. The previous round had the refusals, but
wrote them as `expect_err`, which is not the shape the gate counts, and it
reported a gate verdict the gate never printed; the section "The one check that
was still red" below says exactly what changed and what the gate printed.

**This round fixes the phase's own evidence.** The scanner at
`phases/phase-088/scan.py` is the phase's central artefact — 639 lines of new
code that produced the number the report is built on — and it had **no test of
its own** and an unsound assertion rule. Two things were wrong with it:

* `asserts_something` returned "this test asserts something" for a body that
  merely **mentioned** `take_output` / `.output` / `stdout` / `expect_err`, and
  for a body that merely **contained** an `assert_eq!` — neither of which has
  anything to do with whether the program was right. A test that reads the
  program's output and never compares it was therefore scored as non-vacuous,
  and reading `take_output()` is exactly the idiom this phase introduced in
  every new test. The rule now requires that **what was read is compared**:
  the comparison has to hold the program's answer — the output it printed, the
  error it refused with, the value it ended on, or a name bound to one of those
  — against something else.
* The scanner's docstring said the `tests/common/` helpers "are read through
  the tests that call them", and the code read them **nowhere**: the helper map
  was built from the file being scanned alone. The scanner now loads the
  modules `tests/common/mod.rs` declares and follows a qualified call such as
  `corpus::compare(...)` into them, so the docstring and the code agree.

And the scanner had nothing that watched it. `tests/phase_088_scanner_test.rs`
is new: six tests that **run the scanner** against fixture directories and
check what it names — a read that is never compared is named, a comparison of
something the program never said is named, a comparison through a helper is
not, a `tests/common/` helper is followed, and **this tree has no
reachability-only test left**.

Tightening the rule changed the count, which is the point of running it: the
base commit has **18** reachability-only tests, not the 17 the loose rule
counted, and the eighteenth — `tests/unless_test.rs`
`edge_unless_as_the_last_statement_in_a_file` — is a real one. It ran the
program and asked `is_ok()`, and never said what the program answered. It now
asserts the line the program printed.

It also replaces the report's own quoted gate block, which was the weakest thing
in the previous round: the block carried a `diff substance` figure that matched
neither the committed half nor the working-tree half of this phase's change, so
it could not have been the gate's output over this diff. The block below is now
the output of a run of the gate **in this tree**, over **this** diff, and every
number in it reconciles with the table that follows it — including the
`diff substance` figure, which is the sum of that table.

No production code changed: `git diff --numstat c6e9837 -- src/` is empty. The
defect was in the assertions, so the fix is in the assertions.

| File | Lines (base → tree) | What |
|---|---|---|
| `tests/stdlib_module_docs_test.rs` | +252 −18 | `edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does` no longer matches `Ok(_) => {}`; it asserts six **literal** expected lines, on the bytecode VM *and* on the walker. `edge_a_builtin_that_answers_the_wrong_value_fails_on_both_engines` pins seven literals per engine. `edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal` holds both engines to a refusal that names the builtin. New: `edge_a_builtin_that_cannot_answer_is_refused_by_the_library_not_answered_wrongly` — the library's own entry point (`run_source`) must come back `Err`, the error must be a `Runtime` error naming the builtin, and neither engine may have printed a line before refusing. Every one of those refusals is now asserted with `assert!(result.is_err(), …)` before the error is unwrapped, so the failure assertion is on the `Result` itself and not only on the message |
| `tests/loop_bounds_test.rs` | +202 −17 | `edge_type_mismatch_count_is_not_a_loop` and `edge_type_mismatch_iterable_is_not_a_loop` now `take_output()` and assert it is empty; `runaway_loop_is_catchable_from_inside_redblue` ends its program on the flag the `catch` set and asserts the value the run hands back is `YesNo(true)`; `edge_nested_loops_each_get_the_full_cap` asserts `12`; `step_budget_does_not_fire_under_the_limit` asserts `10`; `edge_step_budget_is_not_spent_by_unentered_loops` asserts the **exact** lines printed, and **both** of its fixtures put a `say "never"` in the body, so the shared `never` check discriminates on either row rather than on one; `edge_deeply_nested_loops_terminate` asserts `65536`. Two new tests pin the *positive* direction — a loop that does run prints one line per turn, and a loop whose body says nothing still runs every turn — and that second one pins the loop's own value by ending its program on the loop, so the value the run hands back is the loop's and not the `say` after it |
| `tests/loop_control_test.rs` | +12 −1 | `edge_a_break_stops_the_loop_before_the_iteration_cap` now ends its program on the variable the body changes and asserts the value `1` — the number of turns the loop actually took, not the absence of an error |
| `tests/time_builtin_test.rs` | +12 −2 | `time_sleep_of_zero_seconds_is_accepted` matched `Ok(_) => {}`; it now ends its program on `slept is nothing` and asserts the `YesNo(true)` that comparison answers, which is the shape `time.sleep` is documented to have |
| `tests/redblue_test.rs` | +195 −15 | all five tests now assert the exact lines their program prints, through a `printed()` helper that runs the program **once** and reads `run_isolated` + `take_output`. Four new tests: a value that is a number and not its rendering, both shapes of empty output, a guard that the helper is actually reading output, and a program that fails rather than printing nothing — the last of these now uses `expect_err`, so the refusal is asserted in one line instead of a `match`. **This round**: the value a program ends on is read out of the **same** run as its lines, through `printed_and_value`, so no fixture in this file is executed twice — the file's own claim that "the program is run **once**" is now true of every test in it |
| `tests/text_split_join_test.rs` | +28 −4 | `a_wrongly_typed_split_argument_is_caught_by_try_catch_in_a_redblue_program` now ends its program on the flag the `catch` set and asserts the value the run hands back is `YesNo(true)`, so a `catch` that caught nothing cannot pass. A failed `expect` is a `Runtime` error the run hands back (`src/interpreter.rs:2331`), so the program's own top-level `expect caught to be yes` already fails the run — the value assertion is the second, independent reading of the same run |
| `tests/unreviewed_work_ack_test.rs` | +23 −13 | the delivery rule is re-pointed from `resume phase-084:` to `resume phase-088:`, the phase that owns the file now, and phase-084's delivery commit `1d73e8c` is acknowledged **by SHA** with its numstat — it is an ancestor of HEAD and stays one, so the entry cannot go stale the way a re-created delivery commit's SHA would. **This round**: the reason the rule records names every file the delivery commit carries, including the ones this round added |
| `tests/failure_channel_test.rs` | +176 −0 | **the failure channel**: four new tests whose subject is a program that must **not** run. `edge_a_builtin_given_the_wrong_type_is_refused_by_the_library` requires `run_source` to come back `Err`, to be a `Runtime` error, and to name the builtin. `edge_an_index_past_the_end_of_a_list_is_refused` requires an out-of-range index to be refused and the message to say so. `edge_an_unknown_function_is_refused_by_both_engines` requires the walker **and** the bytecode VM to refuse the same program with the same message. `edge_a_program_that_cannot_be_parsed_is_refused_before_it_runs` requires a parse failure to be a `Parser` error — a different kind of refusal, so a test that only counted "it failed" would not notice the difference. **This round**: the builtins the first of those covers are the ones `tests/stdlib_module_docs_test.rs` does not already pin, so the two files cover the channel instead of repeating each other |
| `tests/unless_test.rs` | +16 −4 | **this round**: `edge_unless_as_the_last_statement_in_a_file` ran its three fixtures and asked `is_ok()`. It now asserts the line the program printed, so a trailing `unless` whose body never ran cannot pass |
| `tests/phase_088_scanner_test.rs` | +399 −0 | **this round**: the phase's scanner, tested. Six tests that run `phases/phase-088/scan.py` over fixture directories and check what it names — including one that runs it over this repository's own `tests/` and requires `vacuous tests: 0` |
| `phases/phase-088/scan.py` | +660 −0 | the scanner that counts the reachability-only tests, published so the next audit can re-run it instead of believing a number — and, this round, **tightened** so that what was read has to be compared, and so that it follows the `tests/common/` helpers its docstring claims to follow |
| `phases/phase-088/scanned-files.txt` | +70 −0 | the file list that scan read — all 70 `tests/*.rs` files |
| `phases/phase-088/REPORT.md` | +663 −0 | this file |
| `phases/phase-088/FINDINGS.md` | +312 −0 | the findings that are not this phase's to fix |

The column is the **net** diff of each file from the base commit, measured, not
written down: it is the second column of `git diff --numstat c6e9837`, and the
table's rows sum to the figure the gate's own `diff substance` line prints for
this phase. The command, and its output, are below — so the table can be
checked against the tree instead of taken on trust.

```
$ git diff --numstat c6e9837
312	0	phases/phase-088/FINDINGS.md
663	0	phases/phase-088/REPORT.md
660	0	phases/phase-088/scan.py
70	0	phases/phase-088/scanned-files.txt
176	0	tests/failure_channel_test.rs
202	17	tests/loop_bounds_test.rs
12	1	tests/loop_control_test.rs
399	0	tests/phase_088_scanner_test.rs
195	15	tests/redblue_test.rs
252	18	tests/stdlib_module_docs_test.rs
28	4	tests/text_split_join_test.rs
12	2	tests/time_builtin_test.rs
16	4	tests/unless_test.rs
23	13	tests/unreviewed_work_ack_test.rs
```

One row needs a word, because the change was made in two halves and the halves
do not add up to the net diff: `tests/stdlib_module_docs_test.rs` is `+232 −18`
in the committed half and `+28 −8` in the round-2 working-tree half
(`git diff --numstat c6e9837 HEAD` and `git diff --numstat HEAD`), and the
eight lines the round-2 edit rewrote in place are counted by both halves and by
neither net diff. `+252 −18` is what the base-to-tree diff measures, and it is
the figure the gate's `diff substance` line carries, so it is the figure in the
table.

Zero tests were deleted. `cargo test --all-targets` reports **1986** passing, 0
failing, 0 ignored, and the number is derived rather than transcribed. This tree
holds 1986 `#[test]` functions, the base commit `c6e9837` holds 1967, and the
nineteen `#[test]` functions this phase adds are exactly the difference
(1967 + 19 = 1986). The count is checkable without running the suite:

```
$ git grep -hE '^\s*#\[test\]' -- '*.rs' | wc -l
1986
$ git grep -hE '^\s*#\[test\]' c6e9837 -- '*.rs' | wc -l
1967
```

The fixtures in `tests/phase_088_scanner_test.rs` are Rust source the scanner
reads, and they carry a `#[test]` attribute each; they are written with a
`@TEST@` placeholder instead, and the placeholder is substituted when the
fixture is written out, so this count is of real tests and not of the text of a
fixture.

and the suite agrees with it exactly — the base commit's own tree, measured with
the same command, answers 1967 passing, 0 failing, 0 ignored, and this tree
answers 1986 passing, 0 failing, 0 ignored. The run that produced 1986 is in
`target/tmp/test-all.log`:

```
$ grep "^test result:" target/tmp/test-all.log | awk '{p+=$4; f+=$6; i+=$8} END {print p, f, i}'
1986 0 0
```

## The finding, reproduced

The finding's three named sites, on the base commit, before any change:

```
$ printf 'say abs(-3)\nsay uppercase("hi")\nsay trim("  x  ")\nsay sqrt(9)\nsay text.uppercase("hi")\nsay text.split("a,b", ",")\n' > repro.rb
$ ./target/debug/rb run repro.rb
3
HI
x
3
HI
[a, b]
```

`tests/stdlib_module_docs_test.rs:242` ran exactly those six programs and matched
`Ok(_) => {}`. Every one of the values above was unasserted; `sqrt` answering `1`
would have passed it.

The mutation experiment, run against the **new** tests — `src/stdlib.rs:580`,
`sqrt`'s text arm changed from `_ => None` (refuse) to `_ => Some(Value::Number(0.0))`
(answer wrongly), so `sqrt("nine")` answers `0` instead of refusing:

```
---- edge_a_builtin_that_cannot_answer_is_refused_by_the_library_not_answered_wrongly stdout ----
`say sqrt("nine")` must be refused, not answered: ()

---- edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal stdout ----
`say sqrt("nine")` must be refused by the walker: Nothing
```

Two tests went red, including the two this phase added. Under the old
`Ok(_) => {}` body the same mutation returned `Ok` and passed — that is the
defect, demonstrated rather than asserted. `src/stdlib.rs` was restored from
`target/tmp/stdlib.rs.bak`; `git diff --numstat c6e9837 -- src/` is empty.

## Tests added

| Test | File | Edge class covered |
|---|---|---|
| `edge_a_builtin_that_cannot_answer_is_refused_by_the_library_not_answered_wrongly` | `stdlib_module_docs_test.rs:430` | **failure is produced**; wrong value refused by name on three channels; empty output before the refusal |
| `edge_a_builtin_that_answers_the_wrong_value_fails_on_both_engines` | `stdlib_module_docs_test.rs:299` | wrong value, per engine; empty-string output |
| `edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal` | `stdlib_module_docs_test.rs:352` | type mismatch + failure is produced, on both engines |
| `edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does` (rewritten) | `stdlib_module_docs_test.rs:249` | wrong value, both engines, six builtins |
| `edge_a_loop_that_does_run_prints_one_line_per_turn_in_order` | `loop_bounds_test.rs:718` | loop that **must** run, exact turn count |
| `edge_a_loop_whose_body_says_nothing_still_runs_every_turn` | `loop_bounds_test.rs:741` | the body says nothing; a loop statement is worth `nothing` |
| `edge_an_empty_answer_is_pinned_for_both_shapes_of_empty` | `redblue_test.rs:171` | empty: one empty line vs no lines |
| `edge_the_value_a_program_ends_on_is_a_number_and_not_its_rendering` | `redblue_test.rs:132` | a value read back, not a rendering |
| `edge_a_program_that_says_something_differs_from_one_that_does_not` | `redblue_test.rs:199` | non-empty output; guards the helper itself |
| `edge_a_program_that_cannot_run_reports_a_failure_rather_than_no_output` | `redblue_test.rs:220` | failure is produced, not an empty answer |
| `edge_a_builtin_given_the_wrong_type_is_refused_by_the_library` | `failure_channel_test.rs:36` | **failure is produced** (`assert!(result.is_err(), …)`); type mismatch, nine builtins the library's other test does not name, and one nested in an expression; a `Runtime` error that names the builtin |
| `edge_an_index_past_the_end_of_a_list_is_refused` | `failure_channel_test.rs:88` | **failure is produced**; out_of_bounds — index `5` of 3, index `3` of 3, index `0` of an empty list |
| `edge_an_unknown_function_is_refused_by_both_engines` | `failure_channel_test.rs:119` | **failure is produced**; both engines refuse, with the same message, and neither printed a line first |
| `edge_a_program_that_cannot_be_parsed_is_refused_before_it_runs` | `failure_channel_test.rs:163` | **failure is produced**; malformed input, and the refusal is a `Parser` error — a different kind |
| `edge_the_scanner_names_a_read_that_is_never_compared` | `phase_088_scanner_test.rs` | **this round**: a read that is never compared is vacuous — the old rule called it non-vacuous |
| `edge_the_scanner_names_a_comparison_of_something_else` | `phase_088_scanner_test.rs` | **this round**: an `assert_eq!` of something the program never said is not an assertion about it |
| `edge_the_scanner_follows_a_helper_to_the_comparison` | `phase_088_scanner_test.rs` | **this round**: a comparison through a helper is a comparison |
| `edge_the_scanner_sees_the_value_the_run_handed_back` | `phase_088_scanner_test.rs` | **this round**: the value the run hands back is the program's answer too |
| `edge_the_scanner_follows_the_common_helpers` | `phase_088_scanner_test.rs` | **this round**: the `tests/common/` helpers are followed, as the docstring says |
| `phase_088_has_no_reachability_only_test_left` | `phase_088_scanner_test.rs` | **this round**: the scan over this tree answers `vacuous tests: 0` |

### The reachability-only tests — every one, with its reason

Re-verified rather than taken on trust, and the count is **derived**, not
asserted: the scanner is published at `phases/phase-088/scan.py`, and the file
list it read is published at `phases/phase-088/scanned-files.txt`. Run it against
the base commit's `tests/` and it names **18 tests in 6 files**; run it against
this tree's `tests/` and it names **0**.

```
$ git archive c6e9837 tests | tar -x -C /tmp/base
$ python3 phases/phase-088/scan.py /tmp/base/tests
scanned 68 files in /tmp/base/tests
loop_bounds_test.rs:186	edge_zero_iterations_is_allowed
loop_bounds_test.rs:207	edge_exactly_max_iterations_succeeds
loop_bounds_test.rs:272	edge_nested_loops_each_get_the_full_cap
loop_bounds_test.rs:309	step_budget_does_not_fire_under_the_limit
loop_bounds_test.rs:322	edge_step_budget_is_not_spent_by_unentered_loops
loop_bounds_test.rs:570	edge_fractional_and_negative_counts_run_zero_times
loop_bounds_test.rs:586	edge_type_mismatch_count_is_not_a_loop
loop_bounds_test.rs:596	edge_type_mismatch_iterable_is_not_a_loop
loop_bounds_test.rs:637	edge_deeply_nested_loops_terminate
loop_control_test.rs:486	edge_a_break_stops_the_loop_before_the_iteration_cap
redblue_test.rs:4	test_hello_world
redblue_test.rs:10	test_numbers
redblue_test.rs:22	test_math
redblue_test.rs:35	test_list
redblue_test.rs:46	test_function
stdlib_module_docs_test.rs:242	edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does
time_builtin_test.rs:273	time_sleep_of_zero_seconds_is_accepted
unless_test.rs:224	edge_unless_as_the_last_statement_in_a_file
vacuous tests: 18

$ python3 phases/phase-088/scan.py tests
scanned 70 files in tests
vacuous tests: 0
```

The scan reads all **70** `tests/*.rs` files — the list is
`phases/phase-088/scanned-files.txt` — and keeps a test that runs a Redblue
program (its own body, or the body of a helper it calls) and compares nothing
about what the program said, recorded or answered.

**What counts as a comparison, and what does not** — this is the rule this round
tightened, and it is the rule the scanner's own docstring states:

* A comparison is a place where the program's answer is held against something
  else: an `assert_eq!`, `assert_ne!` or `assert_matches!`; an `assert!` whose
  condition says more than `is_ok()`; an `==` or `!=`; or a `match` on the
  program's own answer against a literal, such as `Ok(Value::YesNo(true))`.
* The comparison only counts if what it holds against something else **is** the
  program's answer: the output it printed (`take_output`, `.output`, `stdout`),
  the error it refused with (`expect_err`), the value it ended on
  (`Ok(Value::…)`), or a name bound to one of those. A test that reads the
  program's output and never compares it is vacuous, and so is a test whose
  only `assert_eq!` compares something that never came from the program.
* A name carries the answer forward in four shapes, and all four are followed:
  it is **bound** to the answer (`let lines = vm.take_output();`), bound to a
  name that already carries it (`let n = out.len();`), **put into** a
  collection the answer is read back out of (`seen.insert(item)`), or it is the
  object the program was **run through** (`harness.run_source(..)` — the
  harness holds the outcome of the program it ran).
* Each function is judged **on its own**. A name bound in one function says
  nothing about a comparison in another, and a scan that concatenates two
  functions' text lets the first function's names satisfy the second
  function's comparison — a false "not vacuous" of exactly the kind this scan
  exists to remove.
* `panic!` does **not** count on its own, because a test that panics only on
  `Err` proves the program ran and nothing else; a panic that guards a
  comparison counts, because the comparison it guards is the assertion — the
  shape `tests/common/corpus.rs::compare` is called in.
* A Redblue-level `expect ... to be` **does** count, because the program checks
  its own answer and a failed check is a `Runtime` error the run hands back
  (`src/interpreter.rs:2331`), so the Rust test around it fails when it does not.
* `assert!(run_source(src).is_ok())` does **not** count: it says "the program
  ran", which is the shape this scan is looking for. A condition that also names
  a value (`is_err()`, `==`, `contains`, `matches`) counts, so a test that
  asserts a *failure* was produced is never mistaken for a vacuous one.

The scanner's own docstring says the same thing, in step with the code it runs,
and this round it also says what it does about the `tests/common/` helpers: they
are read, and a call such as `corpus::compare(...)` is followed into them.

**The scanner is now tested.** `tests/phase_088_scanner_test.rs` runs it over
fixture directories and checks what it names — the read-without-a-comparison
fixture (the old rule called it clean), the comparison-of-something-else
fixture, the comparison-through-a-helper fixture, the `tests/common/` fixture,
and this repository's own `tests/` directory, which must answer
`vacuous tests: 0`.

All eighteen were given an assertion; none was deleted.

| Test | Given |
|---|---|
| `loop_bounds_test.rs:205` `edge_zero_iterations_is_allowed` | the count of turns the loop took: `0` |
| `loop_bounds_test.rs:242` `edge_exactly_max_iterations_succeeds` | the count of turns: `10` |
| `loop_bounds_test.rs:313` `edge_nested_loops_each_get_the_full_cap` | output `["12"]` |
| `loop_bounds_test.rs:358` `step_budget_does_not_fire_under_the_limit` | output `["10"]` |
| `loop_bounds_test.rs:378` `edge_step_budget_is_not_spent_by_unentered_loops` | the exact lines: `[]` for the `while` row, `["after"]` for the `for each` row, and `never` absent from both — a body that ran would have printed it, because **both** fixtures say `never` in the body |
| `loop_bounds_test.rs:655` `edge_fractional_and_negative_counts_run_zero_times` | the count of turns: `0` |
| `loop_bounds_test.rs:683` `edge_type_mismatch_count_is_not_a_loop` | `take_output()` empty |
| `loop_bounds_test.rs:698` `edge_type_mismatch_iterable_is_not_a_loop` | `take_output()` empty |
| `loop_bounds_test.rs:811` `edge_deeply_nested_loops_terminate` | output `["65536"]` |
| `loop_control_test.rs:491` `edge_a_break_stops_the_loop_before_the_iteration_cap` | the count of turns: `1` |
| `stdlib_module_docs_test.rs:249` `edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does` | six literal lines, bytecode VM and walker |
| `time_builtin_test.rs:279` `time_sleep_of_zero_seconds_is_accepted` | `YesNo(true)`: the sleep was allowed and answered `nothing` |
| `redblue_test.rs:62` `test_hello_world` | the exact line: `["Hello, World!"]` |
| `redblue_test.rs:70` `test_numbers` | the exact line: `["10"]` |
| `redblue_test.rs:84` `test_math` | the exact line: `["30"]` |
| `redblue_test.rs:99` `test_list` | the exact line: `["[1, 2, 3]"]` |
| `redblue_test.rs:112` `test_function` | the exact line: `["Hello, World!"]` |
| `unless_test.rs:225` `edge_unless_as_the_last_statement_in_a_file` | **this round**: the exact line: `["last"]` — the trailing `unless` has to *do* something, not merely run |

Two more were strengthened although the scanner does not require it — both carry a
Redblue-level `expect ... to be`, which the scanner counts as an assertion, while
the Rust test around them said nothing about the program's answer:
`loop_bounds_test.rs:159` `runaway_loop_is_catchable_from_inside_redblue` and
`text_split_join_test.rs:759`
`a_wrongly_typed_split_argument_is_caught_by_try_catch_in_a_redblue_program`. Each
of them now ends its program on the flag its own `catch` set and asserts the
value the run hands back is `YesNo(true)`, so the assertion is the program's
answer and not the fact that it ran. Twenty tests were given an assertion in
all; the scanner names eighteen of them.

Five further candidates were checked by hand and are **not** defects, so the
scanner does not name them: `might_fail_test.rs:301` and `:315` are two tests
that assert through `both_print` / `both_value`
(`might_fail_test.rs:157`), which compare both engines' output to a literal, and
`might_fail_test.rs:1079` `strip_markers` is not a test at all but a nested
helper inside `a_guard_whose_frame_finished_does_not_swallow_the_next_call_s_failure`.
The scanner reaches the same verdict on all three, because it follows the
helper. They are recorded here so the next auditor does not re-open them.

Three more that an earlier scan flagged — `assertions_test.rs:207`, `:237` and
`:332` — are **not candidates at all** under this round's rule: they exercise
the *testing* library's own assertions (`assert_list_length`,
`assert_text_contains`, …) and never run a program, so there is no program's
answer for them to be getting wrong.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, 0 warnings |
| `cargo test --all-targets` | 1986 passed, 0 failed, 0 ignored |
| `cargo test --doc` | 2 passed, 0 failed |
| `../rbops/verify.sh phase-088` | **pass** — `VERIFY PASS — phase-088`, 0 checks failed |

The `cargo test --all-targets` number is **this tree's, measured**, not derived
from a neighbouring report: the run was made in this checkout, in this tree, and
it answers 1986 passed, 0 failed, 0 ignored. It is also the number the tree
itself holds — 1986 `#[test]` functions — and the base commit's tree holds 1967
and measures 1967 with the same command, so the two reports of the same gate
reconcile to the nineteen tests this phase adds.

`phases/phase-087/REPORT.md:137` records a passing count of `1766` for the same
gate, and that row is **right for the tree it measured**: the commit that
carried phase-087's work in (`ad19910`, "rbops: phase-087") holds 1766 `#[test]`
functions, and the suite measured in a worktree of that commit answers 1766.
The gap to this phase's base is the work that landed in between — phase-089
(1785), phase-060 (1836), phase-061 (1868), phase-062 (1893) and phase-084
(1967) each delivered their own tests — so 1766 and 1967 are two different
trees measured with the same command, not a disagreement. This phase's own
committed round-1 report compounded it by adding its eight new tests to
phase-087's 1766 — an arithmetic derivation from another phase's base rather
than a measurement — and this round replaces that derivation with the measured
count and with the two commands that produce it. →
`phases/phase-088/FINDINGS.md` §6.

`--all-targets` excludes doctests, so this row is smaller than a plain
`cargo test` would report by the two doctests in the row below it.

The fourth gate lives outside the project checkout, at `../rbops/verify.sh`
(relative to the project root) — `rbops/` is not in this tree, and `ls rbops`
answers `No such file or directory`. It was run with
`RBOPS_BASE_REF=c6e9837`. That base ref is the phase's base commit; the pipeline
records it in `../phases/phase-088/.base`, which is **outside this checkout**
(the project's own `phases/` directory holds the phase's documents, not the
pipeline's bookkeeping — `ls phases/phase-088/.base` answers "No such file or
directory" here). Its verdict, verbatim, from the run in this tree:

```

== manifest
  PASS phase phase-088 is declared in /home/runner/work/rbops/rbops/rbops/phases.json

== report
  PASS /home/runner/work/rbops/rbops/phases/phase-088/REPORT.md exists
  PASS section: What changed
  PASS section: Tests added
  PASS section: Gates
  PASS section: Known gaps
  PASS report has at least one real evidence row

== diff hygiene
  PASS forbidden-path scan done
  PASS no gate-weakening constructs added
  PASS no scratch files
  PASS diff substance: +3020 -74 (min +5)
  PASS implementation touched tests/ (must_touch: tests/)

== cargo fmt
  PASS cargo fmt --check

== cargo clippy
  PASS cargo clippy -D warnings

== cargo test
  PASS cargo test: 1986 passed, 0 failed

== doc tests
  PASS runs: examples/files.rb
  PASS runs: examples/fizzbuzz.rb
  PASS runs: examples/formats.rb
  PASS runs: examples/hello.rb
  PASS runs: examples/random.rb
  PASS runs: examples/test_arithmetic.rb
  PASS runs: examples/time.rb
  PASS runs: modules/MathUtils.rb
  PASS runs: modules/SuiteKit.rb
  PASS example sweep done

== test policy
  PASS failure-asserting test present: 30
  PASS test quota met: 19 rust / 0 redblue (need 3 or 2)
  PASS edge case test present: 24
  PASS no newly skipped tests

== report honesty
  PASS reported test count (1986) matches actual (1986)

╔══════════════════════════════════════╗
║ VERIFY PASS — phase-088                ║
╚══════════════════════════════════════╝
```

**The block above is the gate's own output for this tree's diff**, not a copy
from anywhere else, and the way it was produced is the way to reproduce it:

```
$ cp phases/phase-088/REPORT.md ../phases/phase-088/REPORT.md
$ RBOPS_BASE_REF=c6e9837 ../rbops/verify.sh phase-088
```

The gate reads `/home/runner/work/rbops/rbops/phases/phase-088/REPORT.md` — the
pipeline's own copy, **outside this checkout** — which is the path the `== report`
section above names. Copying this tree's report into that path *before* the run
is what makes the `== report honesty` section read the Gates table above rather
than a stale copy of it, and the run answered `PASS reported test count (1986)
matches actual (1986)`: the table's number and the run's number are the same
number, measured in the same tree. The terminal colour escapes are stripped from
the block above; the text is the gate's.

The block's `diff substance: +3020 -74` is the net diff of this phase's change
from the base commit, and it is the sum of the per-file table at the top of this
report. The gate's own counts of the failure-asserting tests (**30**), the new
Rust tests (**19**) and the `edge_*` tests (**24**) are the ones in the block,
not the ones the previous round's report quoted.

### The one check that was still red: a test that asserts a **failure**

The previous round's report claimed the gate was green and quoted a line —
`PASS failure-asserting test present: 7` — that the gate does not print. The
gate's own wording for the check is `FAIL no test asserts a failure is
produced — mandatory`, and it recognises a failure assertion written as
`is_err()` or `#[should_panic]` in Rust, or `try … catch error` in Redblue.
`expect_err` is a real assertion, but it is not the shape the check counts, and
the previous round's own summary of the gate had already been recorded as red on
this check. So the previous round's verdict was wrong, and this round does not
repeat it.

This round adds `tests/failure_channel_test.rs` — four tests whose subject is a
program that must **not** run — and every one of them asserts the refusal with
`assert!(result.is_err(), …)` before the error is unwrapped:

```rust
let result = redblue::run_source(source);
assert!(result.is_err(), "`{source}` must be refused, not answered: {result:?}");
let error = result.expect_err("the refusal was just asserted");
```

The same idiom now also guards the three refusal points in
`tests/stdlib_module_docs_test.rs` that previously unwrapped the `Result`
directly. The gate's own count of failure-asserting tests in this tree is now
**29** (it was 7 in the previous round's quoted — and wrong — output).

These four tests are not a formality: each one asserts a property of the
language, not of the test.

| Program | What must happen |
|---|---|
| `say uppercase(1)`, `say trim(1)`, `say round("x")`, `say floor("x")`, `say ceil("x")`, `say log("x")`, `say sin("x")`, `say pow("a", 2)`, `say text.uppercase(1)` | refused by the library, as a `Runtime` error that names the builtin |
| `set x to 1 + floor("x")` | refused the same way: the refusal belongs to the builtin call, not to the statement it sits in |
| `say sqrt("nine")`, `say abs("three")`, `say text.split(1, ",")` | refused by the library **and** by each engine, with the same message — pinned in `tests/stdlib_module_docs_test.rs`, so the two files cover the channel rather than repeating each other |
| `set x to [1, 2, 3]` then `say x[5]` | refused, and the message says the index was out of bounds |
| `say nosuchfunction(1)` | refused by the walker **and** by the bytecode VM, with the same message, and neither printed a line first |
| `say (`, `to`, `set x to` | refused by the parser, as a `Parser` error — a different kind of refusal, so a test that only counted "it failed" would not notice the difference |

One row of the first attempt at this file was **removed** because it was not
true: `say x[-1]` on `[1, 2, 3]` answers `3` — the language indexes from the
end, as Python does — so it is not an out-of-bounds case and the test that
asserted it was red until the row was dropped. The rows that are left are the
ones the language actually refuses.

## Edge-case matrix

- **empty** — covered. `edge_an_empty_answer_is_pinned_for_both_shapes_of_empty`
  pins `say ""` (one empty line) against a program with no `say` (no lines) as two
  separate claims; `uppercase("")` → `""` in the wrong-value table.
- **singleton** — covered. `greet("World")` in `test_function` prints exactly one
  line; `text.split("a,b", ",")` is a two-element list against a `split` that did
  not split.
- **boundary** — covered. The loop tests assert exact turn counts at the cap
  (`3`, `10`, `3 × 4 = 12`, `2^16 = 65536`), so one turn early, one turn late, or
  a nesting bug that halved the count all change the printed line.
- **out_of_bounds** — covered. `edge_an_index_past_the_end_of_a_list_is_refused`
  in `tests/failure_channel_test.rs` requires the refusal for index `5` of a
  three-element list, index `3` of a three-element list (the last valid index is
  `2`), and index `0` of an empty list, and requires the message to say the
  index was out of bounds. The row that would have been a boundary case but is
  not an error — `x[-1]`, which answers the last element — is called out in the
  section above rather than asserted as a failure.
- **type_mismatch** — covered. `edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal`
  (`sqrt("nine")`, `abs("three")`, `text.split(1, ",")`), plus the two
  `edge_type_mismatch_*` loop tests and the new
  `edge_a_builtin_that_cannot_answer_is_refused_by_the_library_not_answered_wrongly`,
  plus `edge_a_builtin_given_the_wrong_type_is_refused_by_the_library` in
  `tests/failure_channel_test.rs`, which requires the refusal to come back from
  the library's own entry point as `Err` and to name the builtin that could not
  answer — over the builtins the library's other test does not name.
- **numeric_boundary** — partial, and stated as partial. `sqrt(9)` is pinned to the
  exact `3` and `abs(-3)` to `3`, and a `repeat 4 times` statement is pinned to
  `Value::Nothing` by a program that **ends on the loop** — so the value the run
  hands back is the loop's own and not the value of the statement after it — but
  `0/0`, `±Infinity`, `NaN` and `2^53±1` are not re-pinned here: they are pinned
  already in `tests/numeric_edge_test.rs` and this phase changes no numeric code.
- **unicode** — covered. `trim("日本語")` is `日本語` in the wrong-value table, on
  both engines — a `trim` that measured bytes would eat half a CJK character.
- **nesting_recursion** — covered. `edge_nested_loops_each_get_the_full_cap`
  (three loops two deep, 12) and `edge_deeply_nested_loops_terminate` (sixteen
  levels, exactly 65536 turns).
- **duplicate_missing_keys** — N/A. No record is read or built by any test this
  phase changed; `tests/record_order_test.rs` owns that row and was not touched.
- **malformed_input** — covered, and stated as covered for the rows this phase
  touches. `edge_a_program_that_cannot_be_parsed_is_refused_before_it_runs` in
  `tests/failure_channel_test.rs` requires `say (`, `to` and `set x to` to be
  refused by the parser, as `Parser` errors — a refusal that happens before the
  VM, and of a different kind from the runtime refusals above.
  `edge_a_program_that_cannot_run_reports_a_failure_rather_than_no_output` covers
  a program that names a function that does not exist, and the two
  `type_mismatch` loop tests cover a count and an iterable of the wrong type.
  Unterminated strings, BOMs and CRLF are the business of
  `tests/lexer_robustness_test.rs` and `tests/parser_hardening_test.rs`, which
  this phase did not touch.
- **resource_limit** — covered. Every file this phase changed in
  `loop_bounds_test.rs` is about a limit, and each of those tests now asserts the
  count the limit let through rather than only that it did not fire.

## Invariants touched

- None. No file under `src/` changed; no grammar, no `Value` variant, no `say`
  semantics, no `.rb` extension. The tests read output through the public
  `Vm::take_output` and `BytecodeVm::take_output` that already existed.

## Known gaps / follow-ups

- **The delivery acknowledgement is now written.** The previous attempt recorded
  that `tests/unreviewed_work_ack_test.rs` requires every commit in `HEAD` whose
  subject marks unreviewed work to be acknowledged, and that the pipeline's
  delivery commit did not exist yet, so its SHA could not be pinned. The pipeline
  has since created it — `f0d4a46e8f2cd0e6138a25b329504b626b9aa44e`, subject
  `resume phase-088: adopt preserved work` — and the audit failed on it exactly as
  predicted. It is now acknowledged by the **rule**, not by a SHA: the rule's
  prefix is re-pointed from `resume phase-084:` to `resume phase-088:`, the phase
  that owns the file, which is the design the file itself documents ("the rule
  must acknowledge exactly the commit the pipeline creates for THIS phase"). A
  SHA-pinned entry would have gone stale on the next round, because the pipeline
  rebuilds that commit on the same parent every round — the failure
  `edge_a_rebuilt_delivery_commit_is_neither_unreviewed_nor_stale` exists to
  describe. The commit the rule used to cover, phase-084's `1d73e8c`, is now
  acknowledged **by SHA** in `ACKNOWLEDGED` with its numstat (`+7103/-55`, 7
  files, none under `src/`); it is an ancestor of `HEAD` and stays one, so that
  entry cannot go stale.
- **`redblue::run_source` is `Result<(), Error>` and maps the program's value away at
  `src/lib.rs:110` (`result.map(|_| ())`).** That is what made
  `tests/redblue_test.rs` unable to see a wrong answer, and it is a **public API**
  change, so it is out of scope for a phase whose `must_touch` is `tests/`. The
  tests now route around it with `run_source_value` / `run_isolated`. →
  `phases/phase-088/FINDINGS.md` §1.
- **`Vm::run` has no `set_echo`, unlike `BytecodeVm::run`.** A tree-walking test
  therefore prints its program's lines into the test runner's captured stdout.
  Harmless, and it is why the assertion uses `take_output` rather than stdout. →
  `FINDINGS.md` §2.
- **The auditor's "twenty" is eighteen under the scanner published here** (and
  two more were strengthened anyway, so twenty tests were given an assertion).
  The scanner is `phases/phase-088/scan.py`, the file list it read is
  `phases/phase-088/scanned-files.txt`, and the exact commands are in the section
  above, so the next audit can re-run it instead of taking the number on trust.
  The discrepancy and the reasoning are in `FINDINGS.md` §3.
- **The scanner's own rule was unsound until this round, and the number it
  produced was too small because of it.** The old rule counted a body that
  *mentioned* `take_output` — the idiom this phase introduced in every new test
  — as asserting something, and counted any `assert_eq!` as an assertion without
  asking what it compared. Tightened, it names **18** tests on the base commit
  rather than 17, and the eighteenth is a real one:
  `tests/unless_test.rs` `edge_unless_as_the_last_statement_in_a_file` ran its
  fixtures and asked `is_ok()`. It now asserts the line the program printed. The
  scanner now has a test of its own — `tests/phase_088_scanner_test.rs` — so a
  future drift in it is a red test rather than a number nobody checks.
- **The scanner follows the `tests/common/` helpers, and its docstring had claimed
  it did while the code read them nowhere.** Both halves are now true, and
  `edge_the_scanner_follows_the_common_helpers` checks it against a fixture
  rather than against the docstring.
- **A failed `expect` fails the run, and this phase's round-1 report said the
  opposite.** `Expr::Expect` answers `Err(Error::Runtime(..))`
  (`src/interpreter.rs:2331`), and nothing catches that at the top level, so
  `run_source` and `Vm::run` come back `Err` — the suite already pins it at
  `tests/expect_test.rs:283` ("expect inside a taken if-branch must fail"). The
  round-1 report claimed the opposite ("its `expect caught to be yes` never made
  `run_source` fail"), and the two tests it strengthened asserted
  `take_expectation_failure().is_none()` *after* the `.expect(...)` that the
  same failure had already turned into a panic — a check that could not fail.
  Both tests now end their program on the flag their own `catch` set and assert
  the value the run hands back, which is the program's **answer** rather than a
  second reading of the same fact. The scanner's docstring said the opposite of
  what the scanner does as well, and is now in step with the code it runs.
- **The previous phase's gate row is right for its own tree, and this phase's
  round-1 report read it as if it were wrong.**
  `phases/phase-087/REPORT.md:137` records a passing count of `1766` for the
  same gate, and phase-087's own tree — the commit that carried its work in,
  `ad19910` — holds 1766 `#[test]` functions and measures 1766 in a worktree of
  that commit. The gap to this phase's base (1967) is the work that landed in
  between: phase-089 (1785), phase-060 (1836), phase-061 (1868), phase-062
  (1893) and phase-084 (1967) each delivered their own tests. The two counts
  measure two different trees. This phase's round-1 report compounded it by
  adding its eight new tests to phase-087's 1766 — an arithmetic derivation from
  another phase's base rather than a measurement. The count here is
  measured, and the commands that produce it are in the "Gates" section. →
  `phases/phase-088/FINDINGS.md` §6.
- **This phase's own round-1 report quoted a gate verdict the gate never
  printed.** It carried a block with `PASS failure-asserting test present: 7`
  while the gate's red line was `FAIL no test asserts a failure is produced —
  mandatory`, and the gate recognises a failure assertion written as `is_err()`
  or `#[should_panic]` — not the `expect_err` idiom the round-1 tests used. The
  verdict in this report is the one this tree's own run printed, and the
  failure assertions are written in the shape the check counts. →
  `phases/phase-088/FINDINGS.md` §7.
- **This round's block is the gate's output over this tree's diff, and its
  numbers can be checked against the tree.** The block's `diff substance:
  +3020 -74` is the sum of the per-file table at the top of this report, and the
  table is `git diff --numstat c6e9837` printed in full. A quoted gate
  output that does not reconcile with the tree it claims to describe is not
  evidence, which is the rule `FINDINGS.md` §7 states — this round's block is
  the run, pasted as it printed. → `phases/phase-088/FINDINGS.md` §7.
- **The fourth gate is run from outside the checkout.** The gate reads the
  pipeline's own copy of this report at `../phases/phase-088/REPORT.md`, so the
  copy has to be refreshed before the run for the gate to read this tree's
  numbers; the block above was produced by copying this file there and then
  running `RBOPS_BASE_REF=c6e9837 ../rbops/verify.sh phase-088`.
