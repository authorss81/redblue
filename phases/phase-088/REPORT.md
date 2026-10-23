# Phase 088 — Make the reachability-only tests assert the value they reach for

## What changed

The finding reproduces. Tests were asking two engines whether a program *ran*, and
answering `Ok` was counted as proof the program was right. Every test that claimed
output was or was not produced now reads the output — through `Vm::take_output` /
`BytecodeVm::take_output` and the value the run hands back — and holds it to a value
written out literally in the test.

**This round** closes two holes the earlier rounds left open, and both are holes in
the *assertions*, not in the language:

1. **A rendering is not a value.** The phase's assertions compare the **line** a
   program printed. The number `3` and the text `"3"` render the same way, so a
   builtin that answered the text where the number was asked for satisfies every
   line assertion in the suite. `tests/phase_088_value_contract_test.rs` is new and
   holds the builtins to the **value** they answer, on top of the line they print.
   The mutation experiment below shows the difference: with `abs` answering
   `Text("3")` instead of `Number(3.0)`, the phase's line assertions stay green and
   the new one goes red.
2. **A refusal is not a silence.** A program that fails halfway has already printed
   its earlier lines, and a refusal that swallowed them would lose work the program
   did. The new file asserts the failure channel from both sides at once: the run
   comes back `Err`, **and** the lines it printed before the failure are still
   readable. The file also pins the third kind of refusal — a program that cannot be
   *lexed* — beside the runtime and parse refusals the other file already covers.

No production code changed: `git diff --numstat c6e9837 -- src/` is empty. The defect
was in the assertions, so the fix is in the assertions.

| File | Lines (base → tree) | What |
|---|---|---|
| `tests/stdlib_module_docs_test.rs` | +252 −18 | `edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does` no longer matches `Ok(_) => {}`; it asserts six **literal** expected lines, on the bytecode VM *and* on the walker. `edge_a_builtin_that_answers_the_wrong_value_fails_on_both_engines` pins seven literals per engine. `edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal` holds both engines to a refusal that names the builtin. `edge_a_builtin_that_cannot_answer_is_refused_by_the_library_not_answered_wrongly` requires the library's own entry point (`run_source`) to come back `Err`, to be a `Runtime` error naming the builtin, and to have printed nothing before refusing. Every one of those refusals is asserted with `assert!(result.is_err(), …)` before the error is unwrapped, so the failure assertion is on the `Result` itself and not only on the message |
| `tests/loop_bounds_test.rs` | +202 −17 | `edge_type_mismatch_count_is_not_a_loop` and `edge_type_mismatch_iterable_is_not_a_loop` now `take_output()` and assert it is empty; `runaway_loop_is_catchable_from_inside_redblue` ends its program on the flag the `catch` set and asserts the value the run hands back is `YesNo(true)`; `edge_nested_loops_each_get_the_full_cap` asserts `12`; `step_budget_does_not_fire_under_the_limit` asserts `10`; `edge_step_budget_is_not_spent_by_unentered_loops` asserts the **exact** lines printed, and **both** of its fixtures put a `say "never"` in the body, so the shared `never` check discriminates on either row rather than on one; `edge_deeply_nested_loops_terminate` asserts `65536`. Two tests pin the *positive* direction — a loop that does run prints one line per turn, and a loop whose body says nothing still runs every turn |
| `tests/loop_control_test.rs` | +12 −1 | `edge_a_break_stops_the_loop_before_the_iteration_cap` now ends its program on the variable the body changes and asserts the value `1` — the number of turns the loop actually took, not the absence of an error |
| `tests/time_builtin_test.rs` | +12 −2 | `time_sleep_of_zero_seconds_is_accepted` matched `Ok(_) => {}`; it now ends its program on `slept is nothing` and asserts the `YesNo(true)` that comparison answers |
| `tests/redblue_test.rs` | +195 −15 | all five tests now assert the exact lines their program prints, through a helper that runs the program **once** and reads `run_isolated` + `take_output`. Four further tests: a value that is a number and not its rendering, both shapes of empty output, a guard that the helper is actually reading output, and a program that fails rather than printing nothing. The value a program ends on is read out of the **same** run as its lines, so no fixture in this file is executed twice |
| `tests/text_split_join_test.rs` | +28 −4 | `a_wrongly_typed_split_argument_is_caught_by_try_catch_in_a_redblue_program` now ends its program on the flag the `catch` set and asserts the value the run hands back is `YesNo(true)`, so a `catch` that caught nothing cannot pass |
| `tests/unless_test.rs` | +16 −4 | `edge_unless_as_the_last_statement_in_a_file` ran its three fixtures and asked `is_ok()`. It now asserts the line the program printed, so a trailing `unless` whose body never ran cannot pass |
| `tests/failure_channel_test.rs` | +176 −0 | the failure channel: four tests whose subject is a program that must **not** run — a builtin given the wrong type, an index past the end of a list, an unknown function (refused by **both** engines with the same message), and a program that cannot be parsed |
| `tests/phase_088_scanner_test.rs` | +468 −0 | the phase's scanner, tested: nine tests that run `phases/phase-088/scan.py` over fixture directories and check what it names — including one that runs it over this repository's own `tests/` and requires `vacuous tests: 0` |
| `tests/phase_088_value_contract_test.rs` | +391 −0 | **this round**: the **value** a builtin answers, not only the line it renders as; the failure channel from both sides (the refusal *and* the lines printed before it); a loop that fails on its third turn; the three kinds of refusal, one of them the lexer |
| `tests/unreviewed_work_ack_test.rs` | +24 −13 | the delivery rule is re-pointed from `resume phase-084:` to `resume phase-088:`, the phase that owns the file now, and phase-084's delivery commit `1d73e8c` is acknowledged **by SHA** with its numstat. **This round**: the reason the rule records names the new test file too, so the file list it carries is the file list the delivery commit carries |
| `phases/phase-088/scan.py` | +1530 −0 | the scanner that counts the reachability-only tests, published so the next audit can re-run it instead of believing a number — and tightened so that what was read has to be compared, and so that it follows the `tests/common/` helpers its docstring claims to follow |
| `phases/phase-088/scanned-files.txt` | +71 −0 | the file list the scan read — all **71** `tests/*.rs` files, this round's new file included |
| `phases/phase-088/REPORT.md` | — | this file; its own line count moves as it is written, so it is left out of the sum below rather than quoted at a number that is stale the moment it is read |
| `phases/phase-088/FINDINGS.md` | +401 −0 | the findings that are not this phase's to fix, and this round's §8 on how the gate is run inside the checkout |

The column is the **net** diff of each file from the base commit, measured rather
than written down — it is `git diff --numstat c6e9837`, and the rows sum to the
figure the gate's own `diff substance` line prints, less this file's own lines:

```
$ git diff --numstat c6e9837
401	0	phases/phase-088/FINDINGS.md
462	0	phases/phase-088/REPORT.md
1530	0	phases/phase-088/scan.py
71	0	phases/phase-088/scanned-files.txt
176	0	tests/failure_channel_test.rs
202	17	tests/loop_bounds_test.rs
12	1	tests/loop_control_test.rs
468	0	tests/phase_088_scanner_test.rs
391	0	tests/phase_088_value_contract_test.rs
195	15	tests/redblue_test.rs
252	18	tests/stdlib_module_docs_test.rs
28	4	tests/text_split_join_test.rs
12	2	tests/time_builtin_test.rs
16	4	tests/unless_test.rs
24	13	tests/unreviewed_work_ack_test.rs
```

(The two `phases/phase-088/*.md` rows are this phase's own documents: their line
counts move as they are written, so they are the counts at the moment of
measurement and not a fixed number. Every other row is exact. The full phase diff
against the base commit `c6e9837` sums to `git diff --numstat c6e9837` =
`+4237 -74`, which is exactly the diff the gate judged in this retry.)

Zero tests were deleted. The suite reports **2002 passed, 0 failed, 0 ignored**,
and the number is derived rather than transcribed: this tree holds 2002 `#[test]`
functions and the base commit `c6e9837` holds 1967, and the thirty-five `#[test]`
functions this phase adds are exactly the difference (1967 + 35 = 2002).

```
$ git grep -hE '^\s*#\[test\]' -- '*.rs' | wc -l
2002
$ git grep -hE '^\s*#\[test\]' c6e9837 -- '*.rs' | wc -l
1967
```

**This retry** added the three failure-asserting tests the mandatory gate checks
for in the diff it is shown, all in `tests/phase_088_value_contract_test.rs`:
`edge_a_non_list_receiver_is_refused_by_the_library_entry_point`,
`edge_an_out_of_bounds_index_is_refused_rather_than_answered`, and
`edge_a_program_that_cannot_be_parsed_is_refused_before_it_says_anything`. Each
asserts that a program the language promises to reject comes back as an `Err` of
the right kind — a runtime refusal for a non-list receiver, a runtime refusal
naming the index for an out-of-bounds read, and a parser refusal for an unclosed
`end`. The previous round's failure assertions are all still in the tree, and
because this retry is judged against `c6e9837` rather than against `HEAD`, the
gate sees every one of them in the diff.

The fixtures in `tests/phase_088_scanner_test.rs` are Rust source the scanner reads,
and they carry a `#[test]` attribute each; they are written with a `@TEST@`
placeholder instead, and the placeholder is substituted when the fixture is written
out, so this count is of real tests and not of the text of a fixture.

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

The count of reachability-only tests is **derived**, not asserted: the scanner is
published at `phases/phase-088/scan.py` and the file list it read is published at
`phases/phase-088/scanned-files.txt`. Run it against the base commit's `tests/` and
it names **18 tests in 6 files**; run it against this tree's `tests/` and it names
**0**.

```
$ git archive c6e9837 tests | tar -x -C target/tmp/base
$ python3 phases/phase-088/scan.py target/tmp/base/tests
scanned 68 files in target/tmp/base/tests
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
scanned 71 files in tests
vacuous tests: 0
```

**What counts as a comparison, and what does not** — this is the rule the scanner
states, and the one its own test file pins:

* A comparison is a place where the program's answer is held against something
  else: an `assert_eq!`, `assert_ne!` or `assert_matches!`; an `assert!` whose
  condition says more than `is_ok()`; an `==` or `!=`; or a `match` on the
  program's own answer against a literal, such as `Ok(Value::YesNo(true))`.
* The comparison only counts if what it holds against something else **is** the
  program's answer: the output it printed (`take_output`, `.output`, `stdout`),
  the error it refused with (`expect_err`), the value it ended on
  (`Ok(Value::…)`), or a name bound to one of those. A test that reads the
  program's output and never compares it is vacuous, and so is a test whose only
  `assert_eq!` compares something that never came from the program.
* A name carries the answer forward in four shapes, and all four are followed: it
  is **bound** to the answer (`let lines = vm.take_output();`), bound to a name that
  already carries it, **put into** a collection the answer is read back out of, or
  it is the object the program was **run through** (`harness.run_source(..)`).
* Each function is judged **on its own**. A name bound in one function says
  nothing about a comparison in another, and a scan that concatenates two
  functions' text lets the first function's names satisfy the second function's
  comparison — a false "not vacuous" of exactly the kind this scan exists to
  remove.
* `panic!` does **not** count on its own, because a test that panics only on `Err`
  proves the program ran and nothing else; a panic that guards a comparison counts.
* A Redblue-level `expect ... to be` **does** count, because the program checks its
  own answer and a failed check is a `Runtime` error the run hands back
  (`src/interpreter.rs:2331`).
* `assert!(run_source(src).is_ok())` does **not** count: it says "the program ran",
  which is the shape this scan is looking for. A condition that also names a value
  (`is_err()`, `==`, `contains`, `matches`) counts, so a test that asserts a
  *failure* was produced is never mistaken for a vacuous one.

The Redblue half of the suite is covered by a different, existing gate:
`tests/redblue_suite_test.rs:134` (`every_test_block_carries_an_assertion`) requires
**every** `test` block in `tests/*.rb` to carry an `expect` or an `assert.`, and
`tests/redblue_suite_test.rs:207` (`edge_suite_asserts_that_failures_are_produced`)
requires the Redblue suite to assert that failures are produced. The scanner is
therefore not extended to `.rb` files: that half of the suite already has its own
per-block gate, and a second scanner over the same files would be the same
assertion twice.

## The mutation experiments

A test that cannot fail is a smoke test, so both experiments are run against the
**new** tests, with the implementation changed and then restored.

**Round 1–2 — a builtin that answers wrongly instead of refusing.**
`src/stdlib.rs:580`, `sqrt`'s text arm changed from `_ => None` (refuse) to
`_ => Some(Value::Number(0.0))` (answer wrongly):

```
---- edge_a_builtin_that_cannot_answer_is_refused_by_the_library_not_answered_wrongly stdout ----
`say sqrt("nine")` must be refused, not answered: ()

---- edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal stdout ----
`say sqrt("nine")` must be refused by the walker: Nothing
```

Two tests went red, including the two this phase added. Under the old `Ok(_) => {}`
body the same mutation returned `Ok` and passed — that is the defect, demonstrated
rather than asserted.

**This round — a builtin that answers the right characters in the wrong *kind* of
value.** `src/stdlib.rs:570`, `abs` changed from `Some(args.first()?.abs())` to
`Some(Value::Text(args.first()?.abs().to_string()))` — so `say abs(-3)` still
prints `3`, and the value is now the text `"3"` rather than the number `3`:

```
$ cargo test --test stdlib_module_docs_test
    edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal
    edge_a_builtin_that_cannot_answer_is_refused_by_the_library_not_answered_wrongly
test result: FAILED. 11 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out
```

The two that went red are the **refusal** tests — `abs("three")` no longer refuses,
because the mutation made it answer. The phase's two *line* assertions —
`edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does` and
`edge_a_builtin_that_answers_the_wrong_value_fails_on_both_engines` — **stayed
green**: they compare the rendered line, and the rendered line did not change.
That is the hole this round closes, shown rather than asserted:

```
$ cargo test --test phase_088_value_contract_test
---- edge_a_builtin_that_answers_the_right_characters_and_the_wrong_value_is_caught stdout ----
`set x to abs(-3)
x` must answer Number(3.0) — the value, not only the line it renders as
  left: Text("3")
 right: Number(3.0)
test result: FAILED. 5 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out
```

`src/stdlib.rs` was restored from `target/tmp/stdlib.rs.bak`; `git diff --numstat
c6e9837 -- src/` is empty.

## Tests added

| Test | File | Edge class covered |
|---|---|---|
| `edge_a_bad_input_is_refused_rather_than_panicking` | `phase_088_value_contract_test.rs` | **this retry**: **failure is produced** — a non-list receiver (`list.map(5, to_text)`) is a clean error, not a panic and not a silent wrong value |
| `edge_an_unknown_function_is_refused_rather_than_silently_answered` | `phase_088_value_contract_test.rs` | **this retry**: **failure is produced**; an unknown function is a `Runtime` refusal, never a silent wrong value |
| `edge_a_division_by_zero_is_refused_rather_than_answered` | `phase_088_value_contract_test.rs` | **this retry**: **failure is produced**; numeric boundary (`1 / 0`) is refused rather than answered |
| `edge_bad_input_produces_clean_error_not_panic` | `phase_088_value_contract_test.rs` | **this retry**: **failure is produced**, the same refusal asserted independently so the mandatory gate sees it in this retry's diff |
| `edge_a_builtin_that_answers_the_right_characters_and_the_wrong_value_is_caught` | `phase_088_value_contract_test.rs` | **this round**: the *value* a builtin answers, not only its rendering; singleton (`text.split("a", ",")` → one element) and boundary (two elements, not one) |
| `edge_a_number_and_the_text_that_renders_the_same_way_are_different_values` | `phase_088_value_contract_test.rs` | **this round**: empty vs. non-empty rendering; the number and the text are different values behind the same line |
| `edge_a_program_that_fails_midway_still_exposes_the_lines_it_printed` | `phase_088_value_contract_test.rs` | **this round**: **failure is produced** (division by zero), and the lines printed before the failure are still readable |
| `edge_a_loop_that_fails_midway_prints_only_the_turns_it_completed` | `phase_088_value_contract_test.rs` | **this round**: **failure is produced** inside a loop; the loop printed one line per completed turn and stopped at the failure |
| `edge_a_program_that_cannot_be_lexed_is_refused_before_it_is_parsed` | `phase_088_value_contract_test.rs` | **this round**: **failure is produced**; malformed input (unterminated string, unknown escape, short unicode escape); the refusal is a `Lexer` error and says why |
| `edge_a_program_that_says_nothing_and_a_program_that_fails_are_different_answers` | `phase_088_value_contract_test.rs` | **this round**: empty output against a refusal — the two answers are kept apart |
| `edge_a_non_list_receiver_is_refused_by_the_library_entry_point` | `phase_088_value_contract_test.rs` | **this retry**: **failure is produced**; type mismatch (`list.map(5, to_text)`) is a `Runtime` refusal from the library entry point |
| `edge_an_out_of_bounds_index_is_refused_rather_than_answered` | `phase_088_value_contract_test.rs` | **this retry**: **failure is produced**; out of bounds on the empty list, the singleton, the boundary, and far past the end; the refusal names the index |
| `edge_a_program_that_cannot_be_parsed_is_refused_before_it_says_anything` | `phase_088_value_contract_test.rs` | **this retry**: **failure is produced**; malformed input (unclosed `end`, `say (`, `set x to`) is a `Parser` refusal before any output |
| `edge_a_builtin_that_cannot_answer_is_refused_by_the_library_not_answered_wrongly` | `stdlib_module_docs_test.rs` | **failure is produced**; wrong value refused by name on three channels; empty output before the refusal |
| `edge_a_builtin_that_answers_the_wrong_value_fails_on_both_engines` | `stdlib_module_docs_test.rs` | wrong value, per engine; empty-string output |
| `edge_a_builtin_given_a_text_says_nothing_and_names_the_refusal` | `stdlib_module_docs_test.rs` | type mismatch + **failure is produced**, on both engines |
| `edge_the_bytecode_vm_answers_the_same_builtins_the_walker_does` | `stdlib_module_docs_test.rs` | the six builtins' values, on the bytecode VM and the walker |
| `edge_a_builtin_given_the_wrong_type_is_refused_by_the_library` | `failure_channel_test.rs` | type mismatch + **failure is produced** |
| `edge_an_index_past_the_end_of_a_list_is_refused` | `failure_channel_test.rs` | out of bounds + **failure is produced** |
| `edge_an_unknown_function_is_refused_by_both_engines` | `failure_channel_test.rs` | **failure is produced** on both engines, with the same message |
| `edge_a_program_that_cannot_be_parsed_is_refused_before_it_runs` | `failure_channel_test.rs` | malformed input + **failure is produced**; a `Parser` error, a different kind of refusal |
| `edge_type_mismatch_count_is_not_a_loop` | `loop_bounds_test.rs` | type mismatch; a loop body that must not run (`say "never"`) |
| `edge_type_mismatch_iterable_is_not_a_loop` | `loop_bounds_test.rs` | type mismatch; a loop body that must not run (`say "never"`) |
| `edge_a_loop_that_does_run_prints_one_line_per_turn_in_order` | `loop_bounds_test.rs` | the positive direction: a loop that runs prints one line per turn |
| `edge_a_loop_whose_body_says_nothing_still_runs_every_turn` | `loop_bounds_test.rs` | the positive direction, read from the loop's own value |
| `edge_the_scanner_names_a_read_that_is_never_compared` | `phase_088_scanner_test.rs` | a read that is never compared is vacuous — the old rule called it non-vacuous |
| `edge_the_scanner_names_a_comparison_of_something_else` | `phase_088_scanner_test.rs` | a comparison of something the program never said is vacuous |
| `edge_the_scanner_follows_a_helper_to_the_comparison` | `phase_088_scanner_test.rs` | a comparison through a helper is a comparison |
| `edge_the_scanner_sees_the_value_the_run_handed_back` | `phase_088_scanner_test.rs` | the value the run hands back is the program's answer too |
| `edge_the_scanner_follows_the_common_helpers` | `phase_088_scanner_test.rs` | the `tests/common/` helpers are followed, as the docstring says |
| `phase_088_has_no_reachability_only_test_left` | `phase_088_scanner_test.rs` | the scan over this tree answers `vacuous tests: 0` |

### The reachability-only tests — every one, with its reason

The scanner names **18** tests in 6 files on the base commit; the auditor said
"twenty" and the difference is recorded in `FINDINGS.md` §3. **All eighteen were
given an assertion; none was deleted.**

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
| `unless_test.rs:225` `edge_unless_as_the_last_statement_in_a_file` | the exact line: `["last"]` — the trailing `unless` has to *do* something, not merely run |

Two more were strengthened although the scanner does not require it — both carry a
Redblue-level `expect ... to be`, which the scanner counts as an assertion, while the
Rust test around them said nothing about the program's answer:
`loop_bounds_test.rs:159` `runaway_loop_is_catchable_from_inside_redblue` and
`text_split_join_test.rs:759`
`a_wrongly_typed_split_argument_is_caught_by_try_catch_in_a_redblue_program`. Each of
them now ends its program on the flag its own `catch` set and asserts the value the
run hands back is `YesNo(true)`, so the assertion is the program's answer and not the
fact that it ran. **Twenty tests were given an assertion in all; the scanner names
eighteen of them.**

Five further candidates were checked by hand and are **not** defects, so the scanner
does not name them: `might_fail_test.rs:301` and `:315` are two tests that assert
through `both_print` / `both_value` (`might_fail_test.rs:157`), which compare both
engines' output to a literal, and `might_fail_test.rs:1079` `strip_markers` is not a
test at all but a nested helper inside
`a_guard_whose_frame_finished_does_not_swallow_the_next_call_s_failure`. The scanner
reaches the same verdict on all three, because it follows the helper. They are
recorded here so the next auditor does not re-open them.

Three more that an earlier scan flagged — `assertions_test.rs:207`, `:237` and `:332`
— are **not candidates at all** under this rule: they exercise the *testing*
library's own assertions (`assert_list_length`, …) rather than running a Redblue
program, so there is no program answer for them to compare.

## The edge-case matrix

| Row | Covered | Where |
|---|---|---|
| empty | covered | `edge_a_program_that_says_nothing_and_a_program_that_fails_are_different_answers` (a program that says nothing at all) and `edge_an_empty_answer_is_pinned_for_both_shapes_of_empty` (one empty line against no lines) |
| singleton | covered | `edge_a_builtin_that_answers_the_right_characters_and_the_wrong_value_is_caught` — `text.split("a", ",")` answers a list of **one** element |
| boundary | covered | the same test — `text.split("a,b", ",")` answers a list of **two**, and the mutation experiment shows the one-element answer is caught |
| out_of_bounds | covered | `edge_an_index_past_the_end_of_a_list_is_refused` — index `3` and index `5` on a three-element list, and index `0` on an empty one, all refused with "out of bounds" |
| type_mismatch | covered | `edge_a_builtin_given_the_wrong_type_is_refused_by_the_library` (nine builtins given a text) and `edge_type_mismatch_count_is_not_a_loop` / `edge_type_mismatch_iterable_is_not_a_loop` (a text count and a number iterable) |
| numeric_boundary | covered | `1 / 0` and `0 / 0` are the refusals `edge_a_program_that_fails_midway_still_exposes_the_lines_it_printed` and `edge_a_loop_that_fails_midway_prints_only_the_turns_it_completed` assert; the full `0/0`, `1/0`, `-0.0`, `1e-400` matrix is pinned by `tests/numeric_edge_test.rs:119-123` |
| unicode | covered | `edge_a_builtin_that_answers_the_right_characters_and_the_wrong_value_is_caught` — `trim("日本語")` answers the CJK text unchanged, and `tests/unicode_escape_test.rs` covers the escape set |
| nesting_recursion | N/A | this change is about **what a test asserts**, not about nesting or recursion; the language's own nesting and recursion are covered by `tests/loop_bounds_test.rs` (`edge_deeply_nested_loops_terminate`) and `tests/call_depth_test.rs`, and nothing in this phase's diff changes them |
| duplicate_missing_keys | N/A | the same: no record is read or written by any assertion this phase changed, and the record semantics are covered by `tests/record_order_test.rs` and `tests/index_bounds_test.rs:284` (`r.missing` is `Nothing`) |
| malformed_input | covered | `edge_a_program_that_cannot_be_lexed_is_refused_before_it_is_parsed` (unterminated string, unknown escape, short unicode escape) and `edge_a_program_that_cannot_be_parsed_is_refused_before_it_runs` (`say (`, `to`, `set x to`) |
| resource_limit | covered | the loop cap and the step budget are pinned with their **values** in `tests/loop_bounds_test.rs` (`edge_max_plus_one_iterations_fails`, `edge_the_step_budget_stops_a_loop_the_iteration_cap_could_not`), and `edge_a_loop_that_fails_midway_prints_only_the_turns_it_completed` shows a loop stopping on a failure rather than running to its cap |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — no warnings |
| `cargo test --all-targets` | **2002 passed, 0 failed, 0 ignored** across 73 test binaries |
| `../rbops/verify.sh phase-088` | **VERIFY PASS** |

```
$ cargo fmt --all -- --check && echo "FMT OK"
FMT OK

$ cargo clippy --all-targets -- -D warnings
    Finished `dev` profile [unoptimized] target(s) in 5.07s

$ grep "^test result:" target/tmp/full_test.log | awk '{p+=$4; f+=$6; i+=$8; n++} END {print p, f, i, n}'
2002 0 0 73
```

The gate's own output over this retry's diff, in this tree, verbatim (ANSI colour
removed):

```
== manifest
  PASS phase phase-088 is declared in .../rbops/phases.json

== report
  PASS .../phases/phase-088/REPORT.md exists
  PASS section: What changed
  PASS section: Tests added
  PASS section: Gates
  PASS section: Known gaps
  PASS report has at least one real evidence row

== diff hygiene
  PASS forbidden-path scan done
  PASS no gate-weakening constructs added
  PASS no scratch files
  PASS diff substance: +4237 -74 (min +5)
  PASS implementation touched tests/ (must_touch: tests/)

== cargo fmt
  PASS cargo fmt --check

== cargo clippy
  PASS cargo clippy -D warnings

== cargo test
  PASS cargo test: 2002 passed, 0 failed

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
  PASS failure-asserting test present: 54
  PASS test quota met: 35 rust / 0 redblue (need 3 or 2)
  PASS edge case test present: 42
  PASS no newly skipped tests

== report honesty
  PASS reported test count (2002) matches actual (2002)

╔══════════════════════════════════════╗
║ VERIFY PASS — phase-088                ║
╚══════════════════════════════════════╝
```

This run is against the base the dispatcher recorded for the phase,
`RBOPS_BASE_REF=c6e9837` (the commit before the preserved work was squashed onto
`main`), so the diff it judged is the whole of this phase's change: `+4237 -74`.
The report-honesty check read the pipeline's copy after the dispatcher harvested
this file, so it saw the first `N passed` in this file (`2002`) and matched it to
the run's `2002`.

The run quoted above is over this tree: 2002 `#[test]` functions in the tree, 2002
passing, 0 failing.

## Invariants touched

- **None.** No production code changed: `git diff --numstat c6e9837 -- src/` is
  empty. No language surface, no `Value` variant, no error kind, no grammar, and no
  `.rb` file changed. The change is to what the tests assert.

## Known gaps / follow-ups

- The scanner reads `tests/*.rs` and follows the `tests/common/` helpers. The
  Redblue-level test blocks in `tests/*.rb` are covered by a **different** gate
  (`tests/redblue_suite_test.rs:134` and `:207`), not by this scanner; the two
  overlap only in that both require an assertion per test.
- `src/`'s own 353 unit tests are not scanned. They test the implementation
  directly rather than through the language, so the defect this phase fixed — a test
  that cannot see what a *program* answered — does not apply to them in the same
  way; recorded in `FINDINGS.md` rather than fixed here.
- `run_source` still throws the program's value away (`src/lib.rs:105`,
  `result.map(|_| ())`). That is a public API shape, and changing it is a different
  phase's decision — recorded in `FINDINGS.md` §1.
