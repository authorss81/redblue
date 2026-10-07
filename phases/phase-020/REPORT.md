# Phase 020 — Differential + property test harness

## Round 1 — what the review changed

A round-1 review found two gates that could not fail, one reachable `panic!`, one
comparison that was silently half-blind, one test that proved its invariant on one
engine only, one claim of silence that was never measured, and a coverage claim for
a form of `if` the corpus does not contain. All eight are fixed here, none by
weakening anything, and each fix is mutation-checked below.

| Reviewer finding | Fix | The gate it was widening |
|---|---|---|
| **BLOCKER** `tests/differential_test.rs:209` — the corpus count was compared with a second call to the same function, so it could not fail | the on-disk count is now compared with the **generator's** count, which is a different number from a different place | compared 357 with 357; now compared 357 with 361 and failed while four programs were being added |
| **BLOCKER** `src/bytecode/vm.rs:1026` — `.expect("an object declaration being assembled")` panics when `pending_object` is `None` | `pending_object` is a **stack** of declarations, `Frame` records the depth it started at so a `catch` gives back what it discarded, `run` starts with nothing assembled, and an underflow is a `RuntimeError` naming the object | a reachable `panic!` is a crash, not a diagnostic; `corpus/objects-0023.rb` is the repro and the differential test holds the fix |
| **MAJOR** `tests/common/vm.rs:328` — `same_place` ignored the column of every `RuntimeError`, over 43 corpus failures | the limit is **recorded per failure** (`Failure::column_exact`), and the engine that cannot name a column is held to the convention (the start of its line) instead of excused; both-exact columns must be equal | a column that *moved* on either engine is now a divergence; `edge_a_column_is_compared_exactly_wherever_both_engines_can_name_one` measures the corpus's 54 frontend / 43 runtime halves |
| **MAJOR** `tests/differential_test.rs:317` — the typed-grammar invariant ran only `vm::tree_walk` | both engines run per seed, and a fault the compiler lowers into nothing is now a failure | the compiler can lose or rewrite a fault; the invariant is about the program |
| **MAJOR** `tests/common/vm.rs:546` — the echo-off test asserted only `take_output`, so a broken `set_echo(false)` passed | the echo-off path is measured in a **child process**, between two markers, with an echo-**on** control in the same run | a library caller's silence is a promise; `set_echo` mutated to a no-op fails this test |
| **MAJOR** `corpus/control-flow-0003.rb:3` — all three `else if` programs are `ParserError`s, so the claimed `if/else if/else` coverage was `if`/`else` | the claim is corrected and **measured**: three programs write an `else if` and all three are refused by both engines, six run a two-arm `else` | `SPEC.md:353` and `docs/GRAMMAR.md:206` document a form neither engine parses → `FINDINGS.md` §10 |
| **MINOR** `tests/differential_test.rs:202` — `RB_WRITE_CORPUS=1` rewrote `corpus/` inside the test while other tests read it | nothing writes `corpus/` any more; the variable writes a refreshed copy under `target/tmp/` and the comparison **still fails** | a test that refreshes the goldens it checks launders a regression, and parallel threads race the directory |
| **MINOR** `src/bytecode/vm.rs:1602` — `break`/`skip` are no-ops and `loop-forms-0012.expected` records the no-op as a success | `corpus::KNOWN_DEFECT_PROGRAMS` names them, and `edge_a_break_and_a_skip_are_pinned_as_the_defect_they_are` asserts the defect on both engines | a golden that records a defect as coverage is read as coverage; a real fix now fails loudly and is expected to change two goldens |

No test was deleted, renamed to be skipped, or given an `#[ignore]` or an
`allow`. One test was **renamed** — `edge_both_vms_place_every_failure_at_the_same_line_and_column`
→ `…_in_the_same_place` — because after the column fix it no longer compared the
column of every failure, and a test whose name overstates it is the same defect as
one whose body overstates it.

## What changed

| File | Lines | What |
|---|---|---|
| `corpus/<family>-NNNN.rb` + `.expected` | 361 programs, 361 golden files | the corpus: **361 programs** across 16 families, each paired with the outcome the tree-walking VM produced — the lines it printed, **the value the program was worth as a type and a rendering**, and a failure's label, message **and** position |
| `tests/common/generator.rs` | +1 310 | the two generators: 16 curated corpus families, and `generate` — a *typed* grammar over a seed, depth-bounded so a program's size is bounded, which **only ever faults on a fault it injected on purpose and fails on that fault and no other**, with every expression **arm** in an enumerable table that declares the kind it is worth |
| `tests/differential_test.rs` | +915 −229 (1 019 lines total) | 29 tests over the corpus, its golden format, the generated programs, the positions, the coverage claims, and the runner's own failure paths |
| `tests/common/corpus.rs` | +629 | the corpus on disk: the loader, the escaped `.expected` format (with its `#position` directive and its **type-tagged** `#value`), its writer into **scratch only**, `compare`, and the table of programs whose golden records a defect |
| `tests/common/vm.rs` | +845 | `Typed`, `Failure`, `tree_walk`, `bytecode` — one program, two engines, one comparison, **one run each**, **typed values**, a `same_place` that compares the column wherever both formats carry one and holds the other to its convention, and the escape that stops a library caller printing a program's own output — measured, not assumed |
| `tests/common/rng.rs` | +188 | SplitMix64 written out, so a seed rebuilds the same bytes on any machine and any rustc, with the reference stream pinned |
| `tests/common/shrink.rs` | +290 | delta debugging: a line pass then a character pass, with `without_line` / `without_char` / `shrinks_to` exposed so the minimality assertion tests the shrinker's own candidates |
| `tests/common/mod.rs` | +19 | the module list, and why the harness is test-only |
| `src/bytecode/codegen.rs` | +57 −23 | `reads_value` threaded through the lowering, so only `main`, a function and a method leave a value behind; and an `object` body compiles as two halves, so a nested `object` no longer panics the bytecode VM |
| `src/bytecode/vm.rs` | +88 −15 | the outermost frame's value *is* the program's value; `SET_PROPERTY` consumes its receiver, as `docs/BYTECODE.md:182` already said; the inheritance chain is walked nearest-parent-first, as its own documentation said; `set_echo(false)`, because a differential harness runs a program as a library; and the `object` declarations being assembled are a **stack**, with a clean error on underflow rather than a `panic!` |
| `phases/phase-020/FINDINGS.md` | +261 | twelve findings, each anchored to a file and a line |
| `phases/phase-020/REPORT.md` | this file | — |

Line counts are `git diff --numstat` against the resume commit `25e9b68`, except
the new files.

### Why two files under `src/` changed

This phase declares `must_touch: ["tests/"]`, and the harness is entirely under
it. The defects the harness found are in `src/`, and a differential runner
that fails on its own corpus is not a runner. They are fixed here because they
are small, because each is mutation-checked below, and because the manifest
naming only `tests/` is a manifest bug for the one phase whose purpose is to find
defects in the thing it tests — `FINDINGS.md` §1 says so and names the widen.

## Reproduction of the finding

The finding — *"No differential or property testing infrastructure exists; this
is what makes S3 provable"* — **reproduced**. The resume commit `25e9b68` added a
runner (`tests/differential_test.rs`, 333 lines) and **no corpus at all**, so six
of its seven tests failed:

```
$ cargo test --test differential_test
thread 'every_corpus_program_has_an_expected_output_file' panicked at
  tests/differential_test.rs:115:23:
corpus directory …/tests/corpus should be readable: No such file or directory (os error 2)
test result: FAILED. 1 passed; 6 failed; 0 ignored; 0 measured; 0 filtered out
```

The previous attempt's `REPORT.md` was not believed and was wrong. It claimed
`corpus/`, `tests/common/`, a 73-test suite, three `src/` changes and a
mutation-check table. The tree contained only the 333-line runner: no `corpus/`,
no `tests/common/`, and no `src/` changes at all. Nothing from that report was
taken on trust; the work was redone and every claim below was re-measured.

### What the harness itself turned up

The runner compares *what a program is worth*, not only what it printed. Writing
that comparison is what made the first divergence visible:

```
$ set t to 5 / say t / t
  tree-walking VM: Ok(Number(5.0))
  bytecode VM:    Ok(Nothing)
```

`src/bytecode/vm.rs` threw the last frame's value away unless the frame was a
*call*, and the top-level frame is not a call. Four more things became visible the
moment that stopped being discarded, and `FINDINGS.md` §1 carries all five with
file:line. The fifth is a reachable panic rather than a wrong answer:

```
$ cat corpus/objects-0013.rb
object Outer
    has tag default "o"
    object Inner
        has x default 1
    end
    say "after inner"
end
say Outer.tag

$ cargo test --test differential_test
thread 'every_corpus_program_agrees_between_the_two_vms' panicked at
  src/bytecode/vm.rs:1026:14:
an object declaration being assembled
```

`object Inner` inside `Outer`'s own body is accepted by the analyzer and runs on
the tree-walking VM; the bytecode VM keeps one `pending_object`, so the inner
`DEF_OBJECT` overwrote the outer one's and the outer frame then reached for a
declaration that was gone. The fix is in the lowering, not the slot — an object
body compiles as two halves, `has`/`to can` into the block that assembles the
type and everything else into the enclosing block after the `STORE`, which is
`declare_object`'s order (`src/vm.rs:1346`). The observable difference is that an
inner declaration now runs *after* the enclosing type is registered:
`corpus/objects-0015.rb` is an `object Child extends Base` written inside `Base`'s
own body, and its parent resolves.

Red before green, on the test that found the first:

```
$ cargo test --test differential_test every_corpus_program_agrees_between_the_two_vms
assertion `left == right` failed: the tree-walking VM and the bytecode VM disagree
  left:  Outcome { output: ["2"], result: Ok(Number(2.0)) }
  right: Outcome { output: ["2"], result: Ok(Nothing) }
```

## Tests added

**81 `#[test]` functions** — 29 in `tests/differential_test.rs` and 52 inside
`tests/common/`, each next to the code it checks rather than in a file that could
only reach it through the public API. **71 are named `edge_*`**. The quota was
≥ 3 with ≥ 1 `edge_*`; none was deleted, renamed to be skipped, or given an
`allow`. The seven tests the resume commit left are still there, by name, and each
still asserts what it asserted. Round 1 added five and renamed one.

| Test | Edge class covered |
|---|---|
| `corpus_holds_at_least_two_hundred_programs` | corpus size, counted over `.rb` files |
| `every_corpus_program_has_an_expected_output_file` | loader — every program has a **parseable** expectation |
| `every_corpus_program_prints_what_its_expected_file_records` | output, **value** and position all compared, over 361 programs, with a count so it cannot pass vacuously |
| `every_corpus_program_agrees_between_the_two_vms` | differential — 361 programs, both halves of the outcome; a divergence is **shrunk and printed** |
| `the_corpus_holds_programs_that_must_fail` | **asserts a failure is produced** — ≥ 40 recorded failures, and one of each of the four labels the corpus uses |
| `the_corpus_holds_programs_that_are_worth_something` | differential — ≥ 20 valued programs, ≥ 10 distinct values, each run on **both** VMs |
| `edge_regenerating_the_corpus_reproduces_the_checked_in_one_byte_for_byte` | corpus integrity — regeneration runs into a scratch directory and is compared **byte for byte** against `corpus/`, so the claim needs no environment variable; and the on-disk **count** is compared with the generator's, which is what catches a program nobody checked in |
| `the_checked_in_corpus_is_the_one_the_generator_produces` | corpus integrity — reads `corpus/` and compares the **on-disk name and source** of every program against the generator's, in all three directions |
| `the_two_vms_agree_on_every_generated_program` | differential — 200 generated programs; on a divergence it shrinks and prints the minimal program |
| `edge_a_generated_program_only_faults_on_a_fault_that_was_injected` | type_mismatch — the typed-grammar invariant, asserted on **both** engines: no fault means completion, and an injected fault means **that** failure |
| `edge_the_generated_corpus_compares_a_program_s_value_not_only_its_output` | differential — both VMs asked about every program, ≥ half the completions worth something, ≥ 5 distinct values |
| `edge_a_generated_program_can_be_seen` | the failure path — a property test names a seed, and a seed is only actionable if the program behind it can be read: the seed, the fault and the trailing newline are all asserted |
| `a_generated_program_runs_the_same_way_every_time` | determinism — same seed twice, same outcome, and both VMs agree |
| `a_generated_program_either_completes_or_reports_a_failure` | **asserts a failure is produced** — a failure names one of the five public labels with a non-empty message and a non-empty position; both channels occur, on both engines |
| `edge_every_recorded_failure_records_the_place_it_happened_at` | malformed_input — every one of the corpus's failures records a position, with a count of the columns that are not 1, so the column half is not arithmetic |
| `edge_every_recorded_failure_points_at_a_line_of_its_own_program` | malformed_input — 96 recorded failures: each checked against a real line and column of its own program, the end-of-input convention counted rather than forbidden, and held to under a quarter of them |
| `edge_both_vms_place_every_failure_in_the_same_place` | differential — the position half of the outcome, compared over the corpus, with a count so it cannot pass vacuously |
| `edge_a_column_is_compared_exactly_wherever_both_engines_can_name_one` | the format's limit, **measured**: every tree-walking failure is exact, every runtime failure on the bytecode VM is at the start of its line, and both halves of the rule are exercised (54 and 43) |
| `edge_the_control_flow_family_covers_the_if_else_it_has_and_pins_the_else_if_it_does_not` | the coverage claim itself — three programs write an `else if` and all three are refused by both engines, six run a two-arm `else`, and a day when `else if` parses this says so |
| `edge_a_break_and_a_skip_are_pinned_as_the_defect_they_are` | the goldens that record a **defect** as a completion — both engines run the no-op to the end of the loop, so implementing `break` fails here rather than quietly breaking two goldens |
| `edge_a_program_that_prints_before_it_faults_is_recorded_not_asserted_away` | resource_limit — 21 corpus programs print before they fail and each is recorded *and* compared; the rule that survives is that a **frontend** failure printed nothing, asserted for every one of the corpus's 54 frontend failures |
| `edge_a_corpus_program_with_no_expectation_is_a_hard_failure` | **asserts a failure is produced** — an orphan is a hard failure naming the file *and* saying the expectation is missing, and no expectation is invented |
| `edge_a_corpus_program_whose_expected_file_names_a_failure_reports_a_mismatch` | **asserts a failure is produced** — an `.expected` that says the program ran does not accept a failure, and the recorded failure does |
| `edge_an_expected_file_naming_the_wrong_label_is_a_comparison_failure` | **asserts a failure is produced** — a changed label, and a changed record from a failure to a completion |
| `edge_a_mismatch_names_the_program_it_is_about` | the panic must name the file, for a changed output and a changed value |
| `edge_a_shrunk_program_still_diverges` | the shrinker must not reduce a program that does not fail, and neither verdict is interchangeable with the other |
| `edge_a_program_that_never_ends_is_stopped_rather_than_hanging` | resource_limit — `while i < 1` returns on both engines, with a message that says what stopped it |
| `edge_a_call_stack_that_cannot_come_back_is_a_clean_failure` | resource_limit — `f(n + 1)` for ever gives the depth message **and a position**, on both engines, and prints nothing |
| `edge_a_deeply_nested_value_is_read_back_on_both_engines` | nesting_recursion — seven levels of nested record, read on both engines |

The other 52 live in `tests/common/`:

| Module | Tests |
|---|---|
| `rng.rs` (6) | `edge_the_generator_is_the_sequence_splitmix64_publishes` (the PRNG's constants pinned against the reference stream, not against this implementation's own first four outputs), `edge_a_seed_is_replayed_and_a_different_seed_is_not`, `edge_the_bounded_helpers_stay_inside_their_bounds` (2 000 draws must reach **both** ends), `edge_a_draw_over_an_empty_table_is_none_and_not_a_panic`, `edge_a_flag_draws_both_answers`, `edge_the_default_seed_is_not_zero` |
| `vm.rs` (14) | `edge_a_vm_built_as_a_library_does_not_print_what_the_program_said` (the new `set_echo`, pinned both ways: echo-off must not change what the program said **and** must not print it — measured in a child process, with an echo-on control in the same run), `edge_a_column_only_one_engine_can_name_is_held_to_the_convention` (the format's limit, recorded per failure: a line-granular column has to be the start of its line, an exact one has to match), `edge_a_position_is_compared_as_a_whole_not_as_a_line`, `edge_a_number_and_the_text_that_spells_it_are_different_outcomes` (the rendering collides, the values do not — at the top level **and inside a container**, where `Value`'s own `Display` renders `[5]` and `["5"]` identically), `edge_a_failure_carries_the_position_it_happened_at`, `edge_a_frontend_failure_printed_nothing_on_either_vm`, `edge_the_two_engines_agree_on_a_failure_and_on_what_was_printed_first`, `edge_one_run_of_a_program_is_all_the_harness_asks_for` (a program that **appends** twice to a file and asserts the file holds `ab`; the mutation puts `abab` there), `edge_the_two_halves_of_an_outcome_are_both_recorded`, `edge_a_failure_renders_with_its_label_message_and_position`, `edge_an_untagged_value_is_rejected_for_a_typed_one`, `edge_the_tagged_form_names_the_type_for_every_kind`, `edge_a_nested_value_round_trips_through_its_tagged_form`, `edge_a_number_renders_the_way_the_interpreter_prints_it` |
| `corpus.rs` (10) | `edge_a_printed_line_that_looks_like_a_directive_round_trips`, `edge_a_multi_line_message_survives_one_line_of_a_file`, `edge_the_escape_is_a_two_way_function_over_every_case`, `edge_an_escape_the_file_does_not_contain_is_refused`, `edge_a_malformed_expected_file_is_rejected` (**asserts a failure is produced** — 15 malformed `.expected` files, each rejected), `edge_a_well_formed_expected_file_is_accepted`, `edge_a_frontend_failure_is_recorded_where_the_interpreter_reports_it`, `edge_a_changed_value_is_a_comparison_failure_naming_the_program`, `edge_a_changed_message_or_position_is_a_comparison_failure`, `edge_a_corpus_program_with_no_expectation_is_a_hard_failure` |
| `shrink.rs` (10) | `edge_the_shrinker_accepts_a_candidate_only_when_it_is_shorter` (a candidate that is not shorter is not an improvement, and accepting one would make the loop restart forever), `edge_the_shrinker_stops_only_when_nothing_can_be_removed` (minimality asserted against the shrinker's **own** candidates, one line and one character at a time), `edge_the_shrinker_never_invents_a_counterexample`, `edge_a_program_that_does_not_diverge_shrinks_to_nothing`, `edge_a_character_comes_off_a_multibyte_program_at_its_own_boundary` (byte indexing would panic on `𠮷`), `edge_a_line_comes_off_and_a_line_that_is_not_there_does_not`, `edge_drop_lines_builds_the_source_the_line_pass_looks_at`, `edge_a_reduction_keeps_the_trailing_newline_a_source_has`, `edge_the_shrinker_reduces_when_no_line_can_go`, `edge_a_reduction_is_reported_with_the_seed_and_the_program_it_reduced` |
| `generator.rs` (12) | `edge_every_kind_is_reachable_and_its_arms_produce_it`, `edge_every_arm_is_reachable_from_more_than_one_seed` (an arm that works for one seed and no other will fail on some other seed and look like a language bug), `edge_every_declared_fault_has_a_tail_that_faults` (seven tails to seven **different** failures), `edge_every_declared_fault_is_reachable_from_some_seed`, `edge_generated_programs_end_in_a_value_rather_than_in_a_statement`, `edge_almost_every_seed_produces_a_program_of_its_own`, `edge_the_generator_is_a_function_of_nothing`, `edge_the_generator_emits_every_statement_form_it_lists` (including the **false** branch, so a broken condition is invisible), `edge_a_text_literal_escapes_what_a_source_cannot_hold_raw`, `edge_the_binders_and_readers_agree`, `edge_the_corpus_is_a_function_of_nothing`, `edge_the_corpus_holds_every_family_it_lists` |

## What the corpus holds

361 programs, no two identical, in 16 families. **265 run to completion; 96 must
fail** — 5 `AnalyzerError`, 3 `LexerError`, 45 `ParserError`, 43 `RuntimeError`.
**37** record a `#value` other than `nothing`, across **24** distinct values:
eleven numbers, five texts, both `yesno`s, three lists (empty, singleton, three
elements) and three records (empty, one field, two fields). `text:number` is
`type_of(1)`. **21** of the 96
recorded failures print before they fail, and are recorded *and* compared rather
than asserted away. **43** of the 96 sit at a column other than 1 — all of them
frontend failures, the only kind that carries a column — and 7 sit one line past
the end of their own program, which is the end-of-input convention and is counted
rather than forbidden.

| Family | n | What it pins |
|---|---|---|
| `arithmetic` | 20 | `+ - * / %` over 20 hand-written pairs, so each of the five operators sees a zero divisor, a negative dividend and `2147483647 + 1` |
| `numeric-boundary` | 25 | `1/0`, `0/0`, `9 mod 0`, `1/-0.0`, `0.1+0.2`, `1/3`, `-0.0`, the 32-bit and `i64` edges, `2^53`, `2^53+1`, signed `/` and `%`, `0.30000000000000004` |
| `text-ops` | 23 | concatenation, `length`, `is`/`is not`, empty text, `\"`, `\`, newline, tab, `5 + "abc"`, `{interp}` printed literally, and the two trailing-token shapes |
| `unicode` | 16 | CJK, emoji, ZWJ, RTL, astral (`𠮷`), accented, arrow/∞, Greek, BOM-prefixed source, `length` and equality on each |
| `lists` | 20 | empty, singleton, index `0`, index `len-1`, negative index, trailing comma, nested list, aliasing, `for each` over each, three out-of-bounds shapes |
| `records` | 16 | one field, two, nested three deep, missing key at three depths, repeated key keeps the last, order-independent equality, aliasing, index-by-key |
| `control-flow` | 19 | `if`/`else` — taken and not taken, an empty `else`, an `else` holding a nested `if`, an empty `then`, `and`/`or`/`not`, a 4-deep `if`, `unless` taken and not taken, a false `and` — and **three `else if` programs that must fail**: `else if` is in `SPEC.md:353` and `docs/GRAMMAR.md:206` and neither engine parses it → `FINDINGS.md` §10. **This row used to claim `if/else if/else` coverage; the claim was false and a test now measures it.** |
| `functions` | 17 | 0/1/2-arity, a function bound to a variable and called twice, a function passed as an argument, two `give back`s, mutual recursion, iteration-as-recursion, an unknown function, a wrong arity, and two that reach the call-depth guard |
| `objects` | 23 | empty object, `has` with and without a `default`, a method, `extends` one and two deep with the child shadowing, a missing field, a field written through the name, an undeclared parent, a name declared twice, a three-line failure message written into one escaped
     `#message`, and **four nested-`object` programs** — one whose body says something *after* the inner
     declaration, one that reads the inner name back (which the analyzer refuses, `FINDINGS.md` §8), one
     whose inner declaration `extends` the type it is written inside, one whose inner declaration
     `extends` a parent that does not exist, and **one whose `default` calls a function that declares an
     object of its own** (`corpus/objects-0023.rb`, the `pending_object` stack, `FINDINGS.md` §11) |
| `loop-forms` | 20 | `repeat 0`/`1`/`n`, `for each` over `[]`/list/list-of-list, nested loops, a condition inside a loop, `while`, three `while`s, and `skip`/`break` — which are accepted and leave the loop alone, and whose goldens are named in `corpus::KNOWN_DEFECT_PROGRAMS` and asserted on both engines rather than left to read as coverage |
| `nesting` | 17 | lists and records 4–7 deep, a walk down every level, `if` nested 4 deep, triple-nested loops, a three-deep call chain, a list-of-lists index, and a 4-deep recursion into the depth guard |
| `stdlib` | 18 | `length` and `type_of` on every type, including the empty and singleton cases, and the three that must fail |
| `runtime-errors` | 19 | each catchable error inside `try … catch` — the catch must fire and the work before it must survive; plus nested `try` and `finally` |
| `faults` | 19 | the same errors uncaught: the failure **and everything printed before it**, plus two **indented** failures |
| `malformed` | 46 | unterminated string, stray `end`, `set` with no name / no `to` / no value, missing operand, unclosed paren, unclosed list, unclosed record, `say` with nothing, `for each` with no variable / no iterable, `repeat` with no count and with no `times`, stray `}` and `)`, `if` with no condition and with no `then`, `@`, `=`, `===`, `<>`, `for each … from`, an unclosed index, `object` with no name, `has` with no name, an unclosed `try`, an unclosed `catch`, an unclosed `module`, `say 1 to 2`, a parameter list with no comma, `unless` with no condition, `1 % ` with no divisor, `give back` with no value, three analyzer failures |
| `value-tails` | 43 | programs whose **last statement is a bare expression**, and programs ending in a **block whose value is read**: a bound number, a bound text, arithmetic, concatenation, `yes`, `no`, `nothing`, a whole list, a whole record, a builtin call, a property read, an index, a call, a missing key, a field write, a loop tail, BOM-prefixed, an `if` tail, an `unless` tail, a `for each` tail, a `try` tail, a nested-`if` tail, and the two trailing-token shapes |

`stdlib` covers only `length` and `type_of` because 23 of the 25 registered
builtins cannot be called from Redblue source → `FINDINGS.md` §2.

### Generator defects the typed-grammar invariant caught

`edge_a_generated_program_only_faults_on_a_fault_that_was_injected` is only worth
having if it finds things, and it found two in its first runs:

1. **A fault tail that named an unbound variable.** `say s0 + "text"` failed with
   `AnalyzerError: Unknown variable 's0'` rather than the
   `Cannot add non-numbers` it was supposed to produce. The tail is now
   `say n0 + "text"`.
2. **A fault index off the end of its own table.** `FAULTS[seed % (FAULTS.len() + 1)]`
   with seven faults indexed seven ways panics for one seed in eight:
   `index out of bounds: the len is 7 but the index is 7`. The table is now
   `ALL_FAULTS`, which carries `Fault::None` in front, so `seed % 8` is always in
   range and "no fault" is drawn as often as each fault.

The three generator defects the previous attempt's report recorded do not exist
in this tree, and each was checked rather than assumed: `either` is not a keyword
(`rb keywords` does not list it, and `set x to either yes or no` is an
`AnalyzerError`), and the `mod` / `divide` / `text` arms here were written to avoid
the three ways the previous attempt's did not — their divisor is `(x * x) + 1`
with each operand parenthesised on its own, and every text arm is a triple of
texts.

### Every assertion is mutation-checked

Each fix was verified by putting the defect back and watching a test fail.
Nothing here was taken on trust:

| Defect restored | Test(s) that failed |
|---|---|
| top-level frame's value discarded again (`src/bytecode/vm.rs`) | `every_corpus_program_agrees_between_the_two_vms`, `the_two_vms_agree_on_every_generated_program`, `the_corpus_holds_programs_that_are_worth_something`, `a_generated_program_runs_the_same_way_every_time`, `a_generated_program_either_completes_or_reports_a_failure`, `edge_the_generated_corpus_compares_a_program_s_value_not_only_its_output` (**6**) |
| the inheritance chain walked backwards again (`src/bytecode/vm.rs`) | `every_corpus_program_agrees_between_the_two_vms` (**1**) |
| `SET_PROPERTY` leaks its receiver again (`src/bytecode/vm.rs`) | `every_corpus_program_agrees_between_the_two_vms` (**1**) |
| an `object` body compiled whole again, so a nested `object` runs inside the block that assembles the outer declaration (`src/bytecode/codegen.rs`) | `every_corpus_program_agrees_between_the_two_vms` and `edge_both_vms_place_every_failure_in_the_same_place`, both by the `src/bytecode/vm.rs:1026` panic (**2**) |
| `if`/loop/`catch` bodies value-bearing again (`src/bytecode/codegen.rs`) | `every_corpus_program_agrees_between_the_two_vms` (**1**) |
| the shrinker indexing by byte rather than by character (`tests/common/shrink.rs`) | `common::shrink::tests::edge_a_character_comes_off_a_multibyte_program_at_its_own_boundary` (**1**) |
| the object declarations back to a **single slot**, so `def_object` overwrites instead of pushes (`src/bytecode/vm.rs`) | `every_corpus_program_agrees_between_the_two_vms`, by `corpus/objects-0023.rb` (**1**) |
| `set_echo` made a no-op again (`src/bytecode/vm.rs`) | `common::vm::tests::edge_a_vm_built_as_a_library_does_not_print_what_the_program_said` (**1**) |
| the column comparison blanketing the runtime failures again (`tests/common/vm.rs`) | `edge_a_column_is_compared_exactly_wherever_both_engines_can_name_one` and `edge_both_vms_place_every_failure_in_the_same_place`, on a mutated column (**2**) |
| the four new corpus programs deleted from `corpus/` | `edge_regenerating_the_corpus_reproduces_the_checked_in_one_byte_for_byte`, `the_checked_in_corpus_is_the_one_the_generator_produces` and `edge_the_control_flow_family_covers_the_if_else_it_has_and_pins_the_else_if_it_does_not` (**3**) — the first is the assertion that used to compare `corpus::corpus()` with `corpus::corpus()`, so it passed with 357 missing programs |

Two of them were **vacuous on the first attempt** and were re-run: the
`object`-body mutation's replacement did not match the source after `cargo fmt`,
and the `reads_value` mutation was written against `block()` when the change is in
the nested `statements()` calls. Both "passed" without having been applied. They
were re-run against the real lines and both fail. That is recorded here because a
mutation that does not apply is the easiest false result this phase could have
reported.

### Defects the harness's own tests caught

Four were mine, and all four were found by the file that was supposed to be
trustworthy:

1. **Two copies of the frontend pipeline.** `tree_walk` would have recovered the
   value by running the pipeline a second time.
   `edge_one_run_of_a_program_is_all_the_harness_asks_for` runs a program that
   **appends** twice to a file and asserts the file holds `ab`.
2. **A `.expected` parser that could not read a completion.** The `#message` line
   was scanned for rather than read by position, so every program that ran to
   completion was refused with *"a failing program must record '#message'"*.
   Three tests found it at once.
3. **A golden format whose two halves disagreed about a value.** `#value` was read
   as a tagged value at the top level and written as a bare rendering inside a
   container, so `list:[1, 2]` parsed as `None` — the element `1` names no type.
   Container elements are now written tagged too, which is also what separates
   `[5]` from `["5"]`, a collision `Value`'s own `Display` cannot see.
4. **The shrinker panicking on a real corpus program.** `without_char` indexed by
   **byte**, and `corpus/unicode-0005.rb` contains `𠮷`, which is four bytes.
   `edge_a_character_comes_off_a_multibyte_program_at_its_own_boundary` walks
   every index of a program with astral text and is the test that pins it.

A fifth was mine and is a limitation rather than a defect: the corpus's first
draft included `while 1 < 2`, which never ends. It is not in the corpus, because a
corpus program has to finish for its golden file to exist; an endless loop is
pinned instead by `edge_a_program_that_never_ends_is_stopped_rather_than_hanging`,
which asserts that the guard stops it and says what for.

## Edge-case matrix

| Row | Covered? |
|---|---|
| empty | covered — `lists` holds `[]` and `for each` over it, `text-ops` holds `length("")` and `say ""` and `"" + ""`, `stdlib` holds `length([])`/`type_of([])`/`type_of({})`, `control-flow` holds an empty `then`, `loop-forms` holds `repeat 0 times` and `while i < 0`, `records` holds `{}` and `length({})`, `value-tails` holds `nothing` and `[]` and `{}` |
| singleton | covered — one-element list (`length([1])`, `xs[0]`), one-field record, `repeat 1 times`, `length("a")`, `for each` over `[1]`, `to zero()`, `say "🎉"`, `length([1])`, `type_of([1])` |
| boundary | covered — index `0`, `len-1`, `[-1]`, `[-3]` in `lists`; `2147483647`, `2147483647+1`, `-2147483648`, `9223372036854775807`, `-9223372036854775808`, `9007199254740992`, `9007199254740993`, `0.1+0.7`, `0.30000000000000004` in `numeric-boundary`; loop counters at `repeat 0` and `repeat 10` |
| out_of_bounds | covered — `lists` holds index `len`, index `1` of a 1-list, `xs[9]` of a 2-list and `xs[0]` of `[]`; `runtime-errors` holds three of them inside `catch`. Each records the exact message, e.g. `Index 9 is out of bounds: length is 2, valid indexes are 0 to 1` |
| type_mismatch | covered — `faults` holds eleven distinct failures: number+text (`1 + "a"`), text−number (`1 - "a"`), yes+number, nothing+number, index-on-number (`1[0]`), index-on-text (`"abc"[0]`), length-of-`nothing`, length-of-number, property-on-`nothing`, a property read two levels into a missing key, and an unknown function; `runtime-errors` holds eleven of the same inside `catch`, each proving the catch fires *and* that the work before it survives; and `edge_a_number_and_the_text_that_spells_it_are_different_outcomes` covers the **golden** side of the same class, at the top level and inside a list |
| numeric_boundary | covered — `numeric-boundary`: `1/0`, `0/0`, `9 mod 0`, `1/-0.0`, `0.1+0.2`, `1/3`, `-0.0`, `-7/2`, `7/-2`, `-7 mod 3`, `7 mod -3`, `9 mod 9`, and the `i64`/`2^53` edges |
| unicode | covered — `unicode`: CJK, emoji, ZWJ (`👩‍💻`), RTL (`مرحبا`), astral (`𠮷`), accented, arrow/∞, Greek, BOM-prefixed source, `length` and equality on each; plus `text-ops` for `\`, `\"`, `\n`, `\t`; plus the shrinker's own multibyte test. **Not covered:** combining marks written in source, because `\u` escapes are dropped |
| nesting_recursion | covered — `nesting` (lists and records 4–7 deep, a walk down every level, `if` nested 4 deep, triple-nested loops, list-of-lists indexing), `functions` (a function bound to a variable, a function passed as an argument, two `give back`s, mutual recursion, iteration-as-recursion), `objects` (inheritance two deep). Terminating recursion is held as three explicit programs (`count_down(3)`, `fact(5)`, `nest(4)`) and three more that reach the depth guard (`functions-0009`, `functions-0010`, `nesting-0014`) |
| duplicate_missing_keys | covered — `records` holds a repeated key (`{a: 1, a: 2}` keeps the last), a missing key (`nothing`), a missing key two levels down and three, aliasing (`set s to r` then reading through `s`), and an index-by-key; `objects` holds a missing field; `malformed` holds a property read on a number |
| malformed_input | covered — `malformed` (46 programs, every one of which fails) and `edge_a_malformed_expected_file_is_rejected` (15 rejected `.expected` files), plus the shrinker's own minimality tests, which reduce real programs by whole lines and then by single characters |
| resource_limit | covered — `edge_a_program_that_never_ends_is_stopped_rather_than_hanging`; `edge_a_call_stack_that_cannot_come_back_is_a_clean_failure`; `runtime-errors` and `faults` hold every catchable runtime error and prove it is catchable rather than fatal; `edge_a_program_that_prints_before_it_faults_is_recorded_not_asserted_away` proves a frontend failure printed nothing; the property tests run 200 generated programs through both VMs and assert no panic; `edge_every_recorded_failure_points_at_a_line_of_its_own_program` checks every failure's position against a real line and column. **Known limit:** the harness's VMs take their budgets from the environment, like every other VM in the tree → `FINDINGS.md` §9 |
| value_bearing_tail | covered — the `value-tails` family (43 programs, 37 worth something on **both** VMs across 24 distinct values), plus `edge_the_generated_corpus_compares_a_program_s_value_not_only_its_output`, which requires at least half the completed generated programs to be worth something |
| generator_well_typed | covered — `edge_a_generated_program_only_faults_on_a_fault_that_was_injected`: every generated program carrying no injected fault runs to completion, **and every generated program carrying one fails on that fault and no other**, which makes `FAULTS` the *only* source of failure. It found two generator defects; `edge_every_declared_fault_has_a_tail_that_faults` then held the seven tails to seven different failures, `edge_every_declared_fault_is_reachable_from_some_seed` holds every one of them to a seed, and `edge_every_kind_is_reachable_and_its_arms_produce_it` holds all 29 arms to the kind they claim, on **64 seeds each** |
| generator_bounded | covered — `edge_a_generated_program_can_be_seen` prints whole programs, `MAX_DEPTH` bounds the nesting, `edge_generated_programs_end_in_a_value_rather_than_in_a_statement` refuses a generated program that writes a file, and 200 programs run without a stack overflow |

## Gates

Run in the order `AGENTS.md` §3.4 gives.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — 0 errors, 0 warnings |
| `cargo test --all-targets --no-fail-fast` | **734 passed, 0 failed, 0 ignored** (30 test binaries), three runs for three; `cargo test --doc`: 1 passed |
| `./rbops/verify.sh phase-020` | **not run — `rbops/verify.sh` does not exist in this checkout** (`FINDINGS.md` §13) |

734 rather than the 729 the first attempt reported: five new tests, one renamed
and none skipped, and the same 653 the resume commit had still pass. **No
pre-existing test file was modified at all** — `git diff --stat tests/span_test.rs`
is empty, and the only test file this phase touched is the one it was dispatched to
build.

**One gate run in six was not green, and the reason is not this phase.** Two
earlier runs of the third gate failed in
`tests/bytecode_vm_test.rs::edge_a_decoded_chunk_runs_identically_to_the_compiled_one`
with `IoError: Failed to read 'output.txt'`: two threads in that binary both run
`examples/files.rb`, which writes `output.txt` in the repository root and deletes
it at the end. A `git worktree` of the resume commit `25e9b68` flakes the same way,
and 42 of 60 concurrent `rb run examples/files.rb` fail, so it is a property of the
tree and not of this round. It is `FINDINGS.md` §12, with the repro; the fix is a
mutex or a scratch directory and both touch a pre-existing test file, which this
phase does not touch. The three runs above are the ones quoted.

On the fourth gate, honestly: there is no `rbops/` directory in this checkout, and
the task instructions say the pipeline that dispatched this phase lives outside
the checkout and is not to be inspected. The three gates that do exist were run
and are green. In their place, the backwards-compatibility check `AGENTS.md` §2
names — and two files under `src/` changed, so this was re-checked rather than
assumed:

```
$ for f in examples/*.rb modules/*.rb; do ./target/debug/rb run "$f"; done
ok examples/files.rb   ok examples/fizzbuzz.rb   ok examples/formats.rb
ok examples/hello.rb   ok examples/test_arithmetic.rb   ok examples/time.rb
ok modules/MathUtils.rb   ok modules/SuiteKit.rb
```

All eight run clean.

Determinism was measured, not asserted. `RB_WRITE_CORPUS=1` no longer rewrites
`corpus/` at all — it writes what the generator produces to
`target/tmp/differential-refreshed` and the byte-for-byte comparison **still
fails** on a difference, so the digest of `corpus/` is the same before and after a
run of it, and a regression cannot be laundered into the goldens:

```
$ find corpus -type f | sort | xargs md5sum | md5sum
$ RB_WRITE_CORPUS=1 cargo test --test differential_test edge_regenerating -- --nocapture
the generator's corpus is at …/target/tmp/differential-refreshed; `corpus/` was not touched, …
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 80 filtered out
$ find corpus -type f | sort | xargs md5sum | md5sum      # identical digest
```

and the differential binary is stable across runs, three for three:

```
$ for i in 1 2 3; do cargo test --test differential_test; done
test result: ok. 81 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Invariants touched

- **None of the language.** No syntax or grammar changed. The `Value` and `Error`
  variants, the `.rb` extension, `to … end` / `if … end` / `for … end`, `set x to`,
  `say`, and the trailing-comma syntax are all untouched, and all 653 pre-existing
  tests still pass. `Opcode::ALL` is untouched, so the S1 artifact's byte format is
  unaffected, and an object body whose only statements are `has` and `to can`
  compiles to the same bytes as before.
- **One thing about what a program *is worth* changed on the bytecode VM**, which
  the language documents as an invariant and is called out here rather than
  buried:
  - a program ending in a bare expression now ends with that expression's value on
    the bytecode VM, as it already did on the tree-walking one;
  - a statement consumes everything it produced, so `set r.a to 1` no longer leaks
    a record onto the operand stack and `if … then 7 … end` no longer leaves `7`
    for the enclosing block. Both were invisible before, because the top-level
    frame discarded whatever it finished with; they are the same bug wearing a
    second hat.
- **One thing about *when* an object body runs changed on the bytecode VM.** An
  `object` body now compiles as two halves: the `has` and `to can` go into the
  block that assembles the type, and everything else compiles into the
  **enclosing** block, after the `STORE` that binds the name. That is the
  tree-walking VM's order (`declare_object`, `src/vm.rs:1346`), and it is what
  makes an `object` written inside another object's body work at all.
  **This was a reachable `panic!` and is fixed here**; `FINDINGS.md` §1 has the
  repro. What changes is the placement of every statement an object body can hold
  other than `has` and `to can` (`say`, `set`, a nested `object`), which used to
  run inside the declaration's own frame.
- **One thing about *which* ancestor wins an inherited field changed on the
  bytecode VM.** A child that declares a field its parent also declares now keeps
  its own, as the bytecode VM's own documentation said and as the tree-walking VM
  always did. Before, the **most distant** ancestor won.
- **The object declarations the bytecode VM is assembling are a stack, not a
  slot** — a private field, so no format and no opcode changed. A `has` field's
  `default` may call a function that declares an object of its own, and with one
  slot the inner declaration took the outer one's place: the outer body's next
  field went into the inner type and the outer body then finished a declaration
  that was gone (`FINDINGS.md` §11, `corpus/objects-0023.rb`). A body a `catch`
  discards now gives its declaration back, a `run` starts with nothing assembled,
  and an underflow is a `RuntimeError` naming the object rather than a `panic!`.
- **`BytecodeVm::set_echo(bool)`** is new, and defaults to `true`, so `rb vm` is
  unchanged. It exists because a library caller runs a program as a library, and a
  test run that interleaved 361 programs' own output with a failure report is a
  test run nobody can read. Nothing else about a default changed, and the promise
  it makes — *printed nothing* — is now measured in a child process rather than
  assumed from `take_output`.
- **The `.expected` format is new** and carries two directives the language does
  not: `#position <line>:<column>` (or `#position none`), required for a failing
  program and refused for one that ran; and `#value`, which records the type as
  well as the rendering (`number:5`, `text:5`, `yesno:yes`, `list:[number:1]`,
  `record:{a: number:1}`, and `nothing` alone). This is the golden format, not the
  language, and it is a new format rather than a changed one.
- **One placement decision.** The corpus lives at `corpus/`, not `tests/corpus/`.
  `redblue::testing::find_test_files("tests")` (`src/testing/mod.rs:64`) walks
  `tests/` recursively and `tests/redblue_suite_test.rs:148` requires every `.rb`
  file it finds to declare Redblue `test` blocks with an assertion in them. A
  corpus program declares none, and *"a corpus program is not a test"* is the right
  thing for that gate to say — so the corpus goes where the suite's collector does
  not walk, which is also where the phase's definition of done asks for it.

## Known gaps / follow-ups

- **23 of the 25 registered builtins cannot be called from Redblue source**
  → `FINDINGS.md` §2. The corpus's `stdlib` family is 18 programs instead of the
  ~60 it should be.
- **`{interp}` string interpolation is an `AGENTS.md` invariant and does not
  work**, including in `examples/hello.rb` → `FINDINGS.md` §3.
- **An `object` written inside another `object` does not bind its name outside** →
  `FINDINGS.md` §8.
- **Trailing tokens on a line are silently accepted** (`say 1 2 3` prints `1` and
  is worth `3`) → `FINDINGS.md` §4.
- **`skip` and `break` are accepted and do nothing** → `FINDINGS.md` §5.
- **`length` counts bytes, not characters** → `FINDINGS.md` §6.
- **A property harness cannot see a defect both VMs share.** Eight of the ten
  items in `FINDINGS.md` are limitations the two VMs agree on, which is exactly
  the class a *differential* harness is blind to. Only §1 was visible to this
  phase, and only because the harness compared something other than printed
  output → `FINDINGS.md` §7.
- **The harness's VMs take their limits from the environment**, so a CI job that
  sets `REDBLUE_MAX_*` low would fail the corpus → `FINDINGS.md` §9.
- **A runtime failure's column is compared only as far as a `.rbc` can carry
  it.** An instruction records a line and no column, so the bytecode VM places a
  runtime failure at the start of its line; the tree-walking VM places it at the
  column of the failing statement. That limit is recorded **per failure**
  (`Failure::column_exact`) and asserted rather than assumed: the two columns must
  be equal where both engines can name one, and the engine that cannot must report
  the start of its line — which is what `edge_a_column_is_compared_exactly_wherever_both_engines_can_name_one`
  checks over all 96 recorded failures, and what a mutation re-introducing the
  blanket rule fails. Making the two columns *equal* would need a column in the
  `.rbc` format, which is a version bump to the S1 artifact and not this phase's
  call.
- **`else if` is in the spec and the grammar and parses in neither engine** →
  `FINDINGS.md` §10. The corpus's coverage claim was corrected rather than the
  parser changed, and a test now measures the claim: three programs write an
  `else if` and all three must be refused by both engines.
- **Two corpus goldens record a defect as a completion** (`break` and `skip`
  change nothing) → `FINDINGS.md` §5. A `.expected` file cannot say so — these
  programs do not fail — so they are named in `corpus::KNOWN_DEFECT_PROGRAMS` and
  asserted on both engines, which fails loudly when `break` is implemented.
- **`cargo test` is flaky**: two tests in `tests/bytecode_vm_test.rs` run
  `examples/files.rb`, which writes `output.txt` in the repository root and
  deletes it → `FINDINGS.md` §12, with a 42-of-60 concurrent-run repro. It is
  pre-existing (it reproduces at the resume commit) and its fix touches a test
  file this phase does not touch.
- **Refreshing the goldens is now a manual step, deliberately.** No test writes
  `corpus/`; `RB_WRITE_CORPUS=1` writes what the generator produces to
  `target/tmp/differential-refreshed`, prints the path, and the byte-for-byte
  comparison still fails. Copying that directory over `corpus/` is a decision a
  person makes after reading the diff, which is the point.
- **`must_touch: ["tests/"]` should have named `src/`** for this phase →
  `FINDINGS.md` §1.
