# Phase 032 — FINDINGS

Work found while reconciling the stdlib module list in SPEC.md with
`src/stdlib.rs` that does **not** belong to this phase. Each entry is
file:line anchored so the auditor can promote it.

## 1. `MODULES` had no entry for `text`, `math`, `list` or `formats` — FIXED here

**Status:** fixed. `src/stdlib.rs:11` now lists all ten, and
`tests/stdlib_module_docs_test.rs` fails if either document and the list stop
agreeing in either direction.

Reproduced before the change:

```
$ printf 'say text.uppercase("hi")\n' > /tmp/a.rb && rb run /tmp/a.rb
Error: AnalyzerError: Unknown variable 'text'
$ printf 'say formats.parse_json("{}")\n' > /tmp/b.rb && rb run /tmp/b.rb
Error: AnalyzerError: Unknown variable 'formats'
```

The root cause was wider than the missing names: `src/stdlib.rs:22-58` registers
`uppercase`, `split`, `abs`, `sqrt` … as `Value::Builtin`, and **nothing
dispatched a `Value::Builtin`**. `Vm::call` only matched `Value::Function`
(`src/vm.rs:1163` before this phase), so `say uppercase("hi")` failed with
`Unknown function 'uppercase'` too — the flat names were dead as well as the
module ones.

## 2. `split` and `join` were registered but never implemented — FIXED here

**Status:** fixed in `src/stdlib.rs` `builtin_function`. `split`, `join`,
`contains`, `starts_with`, `ends_with`, `replace`, `push`, `pop`, `shift`,
`map`, `filter`, `reduce`, `pow`, `sin`, `cos`, `tan`, `log`, `exp`,
`is_number`, `is_text`, `is_list`, `is_record`, `to_text`, `to_number`,
`to_list` are all registered in `builtins()` and answered by **nothing**.

Only `split` and `join` were implemented here, because SPEC.md and README.md
both document `text.split` / `text.join`. The rest are listed below.

`AGENTS.md` §Math Functions and §Text Functions still list the unimplemented
ones — `pow`, `sin`, `cos`, `tan`, `log`, `exp`, `contains`, `starts_with`,
`replace` — as if they were callable, and spell all of them bare. It now says
at the top of its Standard Library section that a module is how each one is
called, which is true of the ones that exist; the rest are the same gap as the
list above, and no document outside AGENTS.md claims them.

## 3. Bare builtin names still say `Unknown function` — NOT fixed, deliberately

**Status:** open, and README.md now says so instead of promising otherwise.
`say uppercase("hi")` and `say split("a,b", ",")` still fail with
`Unknown function`, while `text.uppercase("hi")` works.

`src/vm.rs:1163` routes a `Value::Builtin` **only** when
`stdlib::call_module_function` recognises the name, so a bare builtin behaves
exactly as it did before this phase. Fixing it means dispatching every
registered `Value::Builtin` through `stdlib::builtin_function`, which would
turn `map`, `filter`, `reduce`, `push`, `pow` and 15 others from
`Unknown function` into *either* a working function or a wrong-argument error —
a much larger change to the language surface than reconciling module names
belongs to. It needs its own phase with its own tests.

The review round caught README.md:192 claiming the opposite — that
`uppercase("hi")` and `text.uppercase("hi")` reach one function — which the
shipped code has never done. That sentence is now the truth of §3, and
`tests/stdlib_module_docs_test.rs`
`edge_a_bare_builtin_name_is_not_a_second_spelling_of_a_module_function` pins
both halves of it: a bare `uppercase("hi")` is an `Unknown function`, and
`text.uppercase("hi")` answers `"HI"`. So a future phase that does teach the
bare name fails a test instead of quietly making a stale document true.

## 4. SPEC.md documented calls the parser cannot read — corrected here, three more remain

**Corrected in this phase** (each named in `REPORT.md`):

- SPEC.md:849 `list.map([1, 2, 3], to (x) give back x * 2)` — the function
  literal of phase-034; `to (x) give back x * 2` is a `ParserError`.
- SPEC.md:822 `text.split("a,b,c", by ",")` — `by` is the range-loop step marker
  and is not a named argument; `AnalyzerError: Unknown variable 'by'`.
- SPEC.md:776 `set response to wait network.get(...)` — `wait` is a reserved
  token the parser does not accept in an expression; `ParserError: Unexpected
  token Wait`.
- SPEC.md:829 `set pi to math.PI` — a module has no members but functions, so
  `math.PI` is `AnalyzerError: Unknown variable 'math'` and bare `PI` is
  `Unknown variable 'PI'` (there is no stdlib constant binding in the analyzer;
  see FINDINGS 5).
- SPEC.md:864 `formats.parse_json('{"name": "Alice"}')` — single quotes are not
  a text literal in the lexer; `LexerError: Unexpected character '\''`.
- SPEC.md:855-856, found by the review round:
  `might fail files.write(..)` / `might fail files.append(..)` — `might fail`
  is a reserved word no statement parses (see finding 5). Written
  with `try`/`catch` instead, and the `if` beside them gained the `then` the
  grammar requires.

**Still open, same class, not corrected here:**

- SPEC.md:310-311 in §Function Call shows `text.length("hello")` and
  `math.sqrt(2)` — both work now.
- SPEC.md:634 `give back math.PI * this.radius * this.radius` in the Properties
  example. Same problem as §Standard Library's `math.PI`, corrected there but
  **not** here, because this phase's test only reads §Standard Library. This is
  a leftover the auditor should route to the next spec-drift phase.
- SPEC.md:809-813 §console documents `ask "Your name?"`; `ask` is a reserved
  token (`src/lexer.rs`) that no statement parses, so it is a
  `ParserError: Unexpected token Ask`. `console.log` / `console.error` /
  `console.clear` all work.

## 5. `might fail` is a reserved word that no statement parses — NOT fixed

Found by the review round. `src/lexer.rs:39-40` maps `might` and `fail` to
`TokenKind::MightFail`, and no arm of `src/parser.rs` matches it:

```
$ printf 'might fail files.write("o.txt", "x")\n' > /tmp/a.rb && rb run /tmp/a.rb
Error: ParserError: Unexpected token MightFail
$ printf 'set d to might fail files.read("Cargo.toml")\n' > /tmp/b.rb && rb run /tmp/b.rb
Error: ParserError: Unexpected token MightFail
```

SPEC.md §files is corrected by this phase, because it is inside §Standard
Library and this phase's test reads that section. The same promise is made five
more times outside it — SPEC.md:722, 733, 747, 751, 759 (§Error Handling) and
`docs/GRAMMAR.md:417,514` — and no test reads those sections, so they are left
for the next spec-drift phase. Either the parser grows the statement or the
documents stop promising it; today `try`/`catch` is the only spelling of a
recoverable failure.

## 6. `math.random` and `console.log` answered arguments they did not take

Found by the review round and fixed here.

- `runtime::builtin("random_number")` fell through to `(0.0, 1.0)` for arguments
  that were not numbers, so `math.random("hi", "there")` answered a
  *nondeterministic* number: a wrong argument was indistinguishable from a real
  draw. It now refuses with a named `Runtime` error and a span, and refuses a
  third argument too (`src/runtime.rs`).
- `console.log()` printed nothing and `console.clear(1)` cleared the screen,
  and `files.read("a", "b")` read `"a"`, where SPEC.md and README.md document
  one argument each. `stdlib::arity` states a count for every module function
  whose count is fixed, `runtime::builtin` refuses a call with an argument too
  many, and `console.log`/`console.error` refuse no value to print.
- `time.format`'s count is one *or* two — the format is optional — so it is the
  one module function `arity` states nothing for; the extra argument is refused
  in the arm that reads it.

## 7. Three `bytecode_vm_test.rs` tests raced over `output.txt`

Found while running the gates, not by the review, and fixed without touching
what they assert. `a_corpus_of_programs_runs_identically_on_both_vms`,
`edge_the_two_vms_report_the_same_failure_for_every_corpus_program` and
`edge_a_decoded_chunk_runs_identically_to_the_compiled_one` all walk the whole
corpus, they run in the same process, and `examples/files.rb` writes, renames
and deletes `output.txt` in the working directory. Three walks side by side
interleave over one file, so the walk that reached it second failed to read a
file the first had deleted — `cargo test` failed about one run in three. A
`CORPUS_WALK` mutex (`tests/bytecode_vm_test.rs`) is held for the length of each
walk: no program is skipped, no assertion is loosened, and no test is ignored.

## 8. There is no stdlib constant binding — NOT fixed

`src/stdlib.rs` inserts `PI` and `E` into the globals map, but the analyzer
never learns they are bound, so every read is `Unknown variable 'PI'`
(`tests/constant_test.rs:309` asserts exactly that, for a different reason).
`AGENTS.md` §Math Functions and `src/repl/completer.rs:70` both promise them.
Either the analyzer should know the stdlib globals, or the two constants should
be `constant` declarations SPEC.md teaches. Not this phase's concern.

## 9. `list.map` / `list.filter` / `list.reduce` do not exist

SPEC.md documented three higher-order list functions. They are registered in
`src/stdlib.rs` and implemented nowhere, and they take a function argument the
language cannot yet pass as a value. SPEC.md §Standard Library now documents
`list.length` only, and says why. The functions are phase-034's job.

## 10. `text.length` counts bytes, and SPEC.md now says so

`src/stdlib.rs` answers `s.len()` and `runtime::builtin("length")` does the
same, so `text.length("🎉")` is 4. `tests/test_text.rb` has asserted that since
before this phase (`edge_text_length_counts_bytes_not_characters`), so bytes
are the decision rather than characters. The review round found SPEC.md silent
on the point; §text now says it, with the emoji and the CJK example, and
`edge_unicode_and_escapes_survive_a_module_function` pins the three cases
rather than recomputing the length in Rust.

## 11. `text` and `list` are not reserved words, which is load-bearing

`list` is a type name in the grammar (`take a list`), and `text` is a type name
(`set text to ""`). Neither is in the keyword table (`src/lexer.rs`), so
`list.length([1, 2])` parses as a member call and `set list to []` still works.
This phase relies on that and did not change it. Recorded so a future phase that
reserves those words knows what it breaks.
---

# Round two

A second review, on the phase as it stood after the first round. Seven findings,
two of them BLOCKERs; all seven are fixed in the code, the documents and the
tests. What each one was, and what closed it.

## 12. `time.sleep` aborted the process on any number it could not wait for — FIXED here

**Status:** fixed. `time.sleep(-1)` and `time.sleep(1e20)` reached
`Duration::from_secs_f64`, which *panics* on a negative duration and on a value
past the end of a `u64` of seconds. A panic in the interpreter thread is not a
`Runtime` error: `run_isolated` reported `The interpreter thread stopped
unexpectedly` and no `try` in the program could catch it.

```
$ printf 'say time.sleep(-1)\n' > target/tmp/a.rb && rb run target/tmp/a.rb
thread 'redblue-vm' panicked at library/core/src/time.rs:962:23:
cannot convert float seconds to Duration: value is negative
Error: RuntimeError: The interpreter thread stopped unexpectedly
exit=1
```

`runtime::sleep_duration` (`src/runtime.rs`) now builds the `Duration` from whole
seconds and a nanosecond remainder after refusing a value that is not finite or
not in `[0, MAX_SLEEP_SECS]`, so `from_secs_f64` is never reached.
`MAX_SLEEP_SECS` is one year: inside what a `Duration` holds, outside what a wait
can mean. Covered by `edge_time_sleep_refuses_a_number_it_cannot_wait_for`,
which also asserts the failure is not the interpreter-thread one, so it fails on
the abort rather than passing on it.

## 13. `time.format` panicked on a huge timestamp and answered `1970` for a negative one — FIXED here

**Status:** fixed. `timestamp as u64` saturates, so `time.format(-1)` became `0`
and answered `1970-01-01 00:00:00` as though the program had asked for the epoch,
and `time.format(1e20)` became `u64::MAX`, which overflowed
`UNIX_EPOCH + Duration::from_secs(..)` — a panic, and so the same abort as §12.

```
$ printf 'say time.format(1e20)\n' > target/tmp/a.rb && rb run target/tmp/a.rb
thread 'redblue-vm' panicked at src/runtime.rs:488:28:
overflow when adding duration to `SystemTime`
Error: RuntimeError: The interpreter thread stopped unexpectedly
$ printf 'say time.format(-1)\n' > target/tmp/b.rb && rb run target/tmp/b.rb
1970-01-01 00:00:00
```

`runtime::timestamp_seconds` now requires a whole number of seconds from zero
below `i64::MAX`, and `chrono::DateTime::from_timestamp` refusing what is left
is a `Runtime` error naming `time.format` rather than a `None` unwrapped.
Covered by `edge_a_timestamp_that_is_not_a_whole_number_of_seconds_is_refused` —
negative, fractional, past `i64::MAX`, and past the calendar.

## 14. Every draw was the wall clock, read with an `unwrap()` — FIXED here

**Status:** fixed. `random_number`, `random_choice` and `random_shuffle` each did
`SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` — an abort on a clock
set before 1970 — and then took the *clock reading itself* as the draw. That has
three defects beyond the abort:

- `as_nanos() % 1000000` is 20 bits of a monotonically increasing counter, so
  every draw in one program sits inside one narrow window of `0.0..1.0`.
- There was no seed, so no test could assert a random result and no bug that
  depended on one could be reproduced.
- `random_shuffle` took **one** reading and reused it for every step, so the
  permutation depended on the *length* of the list and not on its contents.

`runtime::with_random` is the one door now, over a per-thread `SplitMix64` state
seeded from the clock on first use — with the clock read checked rather than
unwrapped — and `math.seed(n)` sets the state so a run repeats. `random_shuffle`
is Fisher-Yates with a draw per step. `math.seed` is a new module function, so
`MODULE_FUNCTIONS`, `arity` and `requirement` gained an entry and SPEC.md §math
and README.md §Math document it.

Covered by `edge_a_seed_makes_every_draw_after_it_repeatable`,
`edge_draws_spread_across_their_range_instead_of_riding_the_clock` (200 draws
must cover more than half the range — which a clock cannot) and
`edge_random_choice_and_shuffle_draw_from_the_same_generator`.

`edge_the_runtime_never_unwraps_a_clock_and_never_builds_a_duration_from_a_bare_float`
exists because neither the abort of §12 nor the one of §14 can be *provoked*
from a test: no Redblue program can set a machine's clock, and no clock a test
runs on is before 1970. Both are properties of a call rather than of a value, so
the guarantee is pinned in the source — the same shape as the tests that read
SPEC.md.

**And a third panic, found while writing the generator's own unit tests.**
`Random::below` asked for `1 << (u64::BITS - bound.leading_zeros())` bits, which
is a shift of 64 — and a panic — for any bound with its top bit set. No call site
reaches one today: the bounds are a list's length and an index. It is `low_bits`
now, and `edge_the_mask_a_draw_is_taken_from_reaches_the_whole_range` pins both
ends of it. Recorded because it is the third panic reachable from a Redblue
program in this phase's files, and the reason the source-scanning test exists.

## 15. The two callers of `arity` disagreed about "too few" — FIXED here

**Status:** fixed. `stdlib::call_module_function` compared `given != expected`;
`runtime::builtin` compared `args.len() > expected`. So `text.length()` was
refused on its count — `text.length takes 1 argument(s), given 0` — while a bare
`length()` fell through to the arm inside the function and was refused in
different words entirely. Both are `!=` now.

Three entries in `tests/stdlib_modules_test.rs` pinned the old wording for a call
with an argument too few — `files.write("p")`, `files.copy("a")` and
`network.post("http://example.invalid")` — and now pin the count refusal. No
entry was removed and no assertion was loosened; each still says a wrong call of
that function is refused by name. `edge_a_bare_name_and_a_module_name_refuse_the_same_count`
pins thirteen bare/module pairs, both directions of the count.

## 16. `text.join` stringified whatever it was given — FIXED here

**Status:** fixed. `item.to_string()` on every element meant
`text.join([1, yes, [1]], ",")` answered `"1,yes,[1]"` — the one function of the
set that answered for a list it was not given, while `text.uppercase(1)` and
`list.length(5)` both refuse. A wrong argument that produces plausible output is
the worse of the two failures, because the program goes on running.

`runtime::builtin("join")` now refuses a non-text element by position and type —
`text.join requires a list of text values, but element 2 is number` — and
`stdlib::builtin_function("join")` holds the same rule behind it, so the refusal
and the join cannot disagree. Eight sources are covered by
`edge_join_refuses_a_list_that_is_not_a_list_of_text`, along with the empty text,
empty list and empty separator that must still answer, and a `try`/`catch` that
proves the refusal is catchable.

## 17. README.md stated a fixed arity for two functions with an optional one — FIXED here

**Status:** fixed. `math.random(min, max)` and `time.format(ts, format)` were
written as if two arguments were the whole of each, while
`math.random(5)` draws from zero and `time.format(0)` answers with the default
format. Both documents now show both spellings, SPEC.md has the `### time`
section that was missing altogether, and
`edge_the_documents_state_the_optional_counts_the_code_takes` reads the documents
for each spelling and then runs it.

## 18. AGENTS.md listed twelve names the code does not answer — FIXED here

**Status:** fixed, as §8 asked. §Math Functions and §Text Functions listed `PI`,
`E`, `pow`, `sin`, `cos`, `tan`, `log`, `exp`, `contains`, `starts_with`,
`ends_with` and `replace` as callable and spelled every one of them bare; none is
answered. The two sections now list only what runs, and the prose above them says
what is absent and why. `edge_the_agents_document_does_not_promise_a_name_the_code_does_not_answer`
reads every ```redblue block of that section, refuses any of the twelve by name,
and then checks through `rb run` — the whole pipeline, because a constant is a
disagreement the *analyzer* has with the globals map and a bare tree-walk would
print `3.141592653589793` and pass — that each one really is refused today.

## 19. Still open after round two

- **§8 stands.** `PI` and `E` are in `stdlib::builtins()` and the analyzer never
  learns they are bound, so `say PI` is an `Unknown variable`. Round two
  corrected the documents; it did not decide between an analyzer that knows the
  stdlib globals and two constants that are `constant` declarations.
- **`src/repl/completer.rs` still offers every one of those names** — and about
  twenty bare builtins besides, none of which is a second spelling of a module
  function (§3). A completion hint is a weaker promise than a document, and the
  fix is the same decision §3 is waiting on: dispatch the bare builtins or stop
  suggesting them. Not this phase's to make.
- **`random_choice` and `random_shuffle` are reachable only bare and documented
  in no document.** They are in `stdlib::builtins()` and in `runtime::builtin`,
  and §3 is why `say random_choice([1, 2])` works while
  `say random_shuffle([1, 2])` is the only spelling it has. Round two made them
  draw from the generator and left them where they were; giving them a `random`
  module is the same call as §3.
- **§2 stands** for everything but `split` and `join`: `push`, `pop`, `shift`,
  `map`, `filter`, `reduce`, `pow`, `sin`, `cos`, `tan`, `log`, `exp`, `is_*`,
  `to_*`, `contains`, `starts_with`, `ends_with` and `replace` are still
  registered and answered by nothing.
- **§3, §4, §5, §9 and §11** are unchanged by this round.

---

# Round three

A third review, on the phase as it stood after round two. Eight findings — one
BLOCKER, six MAJOR and one MINOR — and all eight are fixed in the code, the
documents and the tests. §20 to §27 are what each one was.

## 20. `Random::below` spent a stack frame per rejected draw — FIXED here

**Status:** fixed. The rejection retried by calling `self.below(bound)` again. The
rejection rate is under a half, so a draw settles in one or two tries, but a
rejection streak is a streak: a hundred consecutive rejections is a hundred stack
frames on the interpreter thread, and a stack that runs out aborts the process
rather than raising something a `try` can catch. Every draw of `math.random`,
`random_choice` and `random_shuffle` reaches this one function, so it is on the
hot path of all three.

```
$ printf 'say math.random(10)\n' > target/tmp/a.rb && rb run target/tmp/a.rb
7
```

No Redblue program can provoke it — the generator is not reachable from a
program's own arithmetic, and the streak that would overflow a stack is
astronomically unlikely at a rejection rate under a half — so the property is
pinned in the source, the same shape as §12 and §14. It is a `loop` now.

`edge_a_bounded_draw_retries_in_a_loop_and_not_in_a_call_to_itself` reads
`Random::below`'s body out of `src/runtime.rs` with its comments stripped — so
the prose naming the old recursion is not what it reads — counts the braces to
find where the function ends, and fails on a `self.below(` inside it or on the
absence of a `loop`. It then runs 6,000 draws of `random_choice`,
`random_shuffle` and `math.random` through the bytecode VM, which is the volume
a program *can* reach.

## 21. `math.random` computed a width that is not a number — FIXED here

**Status:** fixed. `min + r * (max - min)` forms the *width* first, and the width
of `-1e308` to `1e308` is `infinity`, so every draw from an ordinary range was
refused with `infinity is not a finite number` — a message that names no
function, for a range nothing about which is infinite. A draw of exactly `0.0`
made it `0 * infinity`, which is `NaN`, so no seed escaped it either.

```
$ printf 'say math.random(-1e308, 1e308)\n' > target/tmp/a.rb && rb run target/tmp/a.rb
Error: RuntimeError: infinity is not a finite number
```

The two ends are now scaled into `[0, 1]` and added — `min * (1.0 - r) + max * r`
— so the width is never formed. The addition is checked anyway, and a draw that
is still not finite is refused by the name of the function that was asked
(`math.random` through the module spelling, `random_number` bare), the way the
§6 refusals are.

**One existing test pinned the old behaviour and is now stronger.**
`tests/numeric_edge_test.rs` held `edge_random_number_refuses_a_range_whose_width_overflows`,
which asserted `message.ends_with("is not a finite number")`. That refusal *was*
the defect, so the assertion is replaced by the property that matters: 64 draws
from `-1e308` to `1e308`, every one finite and inside its range, and at least one
draw from `-1e308` to `0` below zero, so the interpolation is not simply
answering one end. The entry is renamed
`edge_a_draw_from_a_range_wider_than_a_double_is_still_a_finite_number` — one
test where there was one test, with a stricter assertion. Nothing was removed and
nothing was loosened. `edge_math_random_draws_from_a_range_too_wide_to_subtract`
covers the same four ranges through `math.random`, checks that SPEC.md §math says
so, and re-checks the four wrong-argument refusals still name `math.random`.

## 22. `type_of()` answered the type of the argument it was not given — FIXED here

**Status:** fixed. `args.first().map(|v| v.type_name()).unwrap_or("nothing")` means
`type_of()` and `type_of(nothing)` return the *same string*. A program that
forgot its argument was answered with a plausible type name and went on running
— the failure mode §16 calls the worse of the two, because the bug does not stop
the program.

```
$ printf 'say type_of()\n' > target/tmp/a.rb && rb run target/tmp/a.rb
nothing
$ printf 'say type_of(nothing)\n' > target/tmp/b.rb && rb run target/tmp/b.rb
nothing
```

`stdlib::arity` now states `type_of`'s count, which is the one table both call
paths read, and `runtime::builtin`'s gate was widened from "a name a module owns"
to "a name with a stated count" — the gate itself was never the point, the count
was. `stdlib::display_name` is new beside `dotted`: `dotted` turns the *first*
underscore into a dot, which is right for `files_read` and wrong for `type_of`,
so `display_name` asks whether a module owns the name and answers `type.of` for
nobody. `runtime::builtin`'s `type_of` arm matches `[value]` and refuses every
other count by name with a span.

Covered by `edge_type_of_takes_exactly_one_argument`: seven values that answer,
three counts that are refused with the message pinned, `arity` and
`display_name` checked on both sides, and a `try`/`catch` that proves the refusal
is catchable. Both VMs are checked on the same four statements by
`edge_both_vms_name_an_unknown_module_member_the_way_the_program_wrote_it`.

## 23. A call through a module that does not exist leaked `module_member` — FIXED here

**Status:** fixed. Both VMs resolve `receiver.member` by looking up the
`module_member` name, and when that name was not bound and the receiver was not a
module either, both fell through to `call` and reported the lookup name:

```
$ printf 'set thing to 1\nsay thing.read("x")\n' > target/tmp/a.rb && rb run target/tmp/a.rb
Error: RuntimeError: Unknown function 'thing_read'
```

`thing_read` is a name no line of Redblue can contain, in a file whose reader
wrote `thing.read`. `src/vm.rs:1230` and `src/bytecode/vm.rs:1728` now refuse
before the call, and name the program rather than the encoding: the
`Module 'text' has no function 'nosuchfunction'` sentence for a receiver that *is*
a module, and `Unknown function 'thing.read'` for one that is not.

The unbound case — `nosuchmodule.read("x")` — is refused by the analyzer before
either VM runs, with `Unknown variable 'nosuchmodule'`, which is accurate and
unchanged: the receiver is the unknown thing. The bound case is the one that
reaches the encoding, which is why the sources above bind the receiver first.

## 24. `edge_an_unknown_module_name_is_a_clean_error` could not fail on the wrong
spelling — FIXED here

**Status:** fixed. The test asserted `message.contains("nosuch")`, which
`nosuchmodule_read` contains as readily as `nosuchmodule.read`, so it pinned
nothing. Its `Error::Analyzer` arm was also unreachable: `eval_err` lexes and
parses and hands the program to the VM without the analyzer, so only the runtime
message was ever compared.

Five sources are pinned on the *exact* message now — three unbound receivers and
two bound ones — each refusing any message containing `_` so the encoding cannot
come back, and each carrying a span. The whole pipeline is checked separately
through `rb run`, where the analyzer refuses first: non-zero exit, and nothing
about the encoding either way. `edge_both_vms_name_an_unknown_module_member_the_way_the_program_wrote_it`
pins the same four refusals on the bytecode VM *and* compares them verbatim with
the tree-walker's.

## 25. SPEC.md §Properties showed `math.PI` — FIXED here

**Status:** fixed, and §4's open item with it. The Properties example computed
`give back math.PI * this.radius * this.radius`. A module has functions and not
members, so `math.PI` is `AnalyzerError: Unknown variable 'math'` — the drift
round one corrected inside §Standard Library and left here, because this phase's
test only read that section.

```
$ printf 'object Circle\n    to area()\n        give back math.PI * this.radius\n    end\nend\n' > target/tmp/a.rb
$ rb run target/tmp/a.rb
Error: AnalyzerError: Unknown variable 'math'
```

The example declares `constant PI to 3.14159` in the object body and reads `PI`,
which is what §math and §Constants already say, and a line under the block says
why. `edge_the_properties_example_is_a_program_that_runs` reads the block out of
SPEC.md and runs it *through `rb run`* — lexer, parser, analyzer, VM — because a
tree-walk that skipped the analyzer would not refuse it. It then sets
`Circle.radius` to 2 and checks the area is `12.56636`, so the constant is the one
the example declares and not a member of `math`.

## 26. Both keyword tables listed `might fail` — FIXED here

**Status:** fixed, and §5's Appendix half with it. SPEC.md §Keywords listed it
under Functions and the Appendix listed it as *Error-prone operation*. `might`
and `fail` both lex to `TokenKind::MightFail`, no arm of `src/parser.rs` matches
it, and so every documented spelling was a `ParserError` — the drift round one
corrected inside §Standard Library and §Files and left in a table no test read.

The row is out of both tables, and the prose under §Keywords now names what the
table no longer does: `might fail`, `when`, `can`, `that`, `new`, `ask`, `wait`,
`async`, `parallel` and `done` are all keywords the lexer produces and no
statement parses, and `takes`/`needs` are not even keywords — a parameter list is
written `(parameters)`. The Appendix marks `takes/needs` reserved rather than
deleting the row, since the row is about words the reader may meet.

Because §Keywords now points at §Error Handling as the spelling of a recoverable
failure, §Error Handling had to be true, so **§5 is closed for SPEC.md as well**:
its four examples are written in `try`/`catch`/`finally` and each of them runs.
`catch error of FileError` is a `ParserError` today — a `try` takes one `catch` —
so that example is now `### One catch` and says so, and `to might fail divide(a,
b)` / `give back error "..."` are replaced by `### Where a failure comes from`,
which says what does raise a failure (`files.read` of a missing file, `1 / 0`,
`xs[9]`) and that a function cannot raise one of its own. `docs/GRAMMAR.md`'s
`### Try/Catch` example is corrected the same way; its grammar *productions* —
§1.3's reserved list and §5.11's `expression = … | 'might fail' expression` —
are left as they are, because that document is the specification of what the
language is *to be*, and a note at §5.11 now names the productions that are not
implemented yet.

`edge_the_spec_keyword_tables_do_not_promise_a_spelling_the_parser_cannot_read`
reads the table rows of both sections — the rows, not the prose, which is where
the correction names the words — and fails on `might fail` in either. It then
asserts the prose still names every one of the ten, parses seven programs that
use one each and checks each is a `ParserError` naming that token, checks nine of
the words lex as keywords rather than identifiers, which is what *reserved* means,
and reads every ```redblue block of SPEC.md **and** `docs/GRAMMAR.md` — every
one, nothing skipped — asserting none of them writes `might fail`. That last half
fails on the three §Error Handling examples and the GRAMMAR.md one as they stood.

## 27. `call_module_function`'s fallback named the lookup — FIXED here

**Status:** fixed. The `requirement` branch above it named the program’s own
spelling; the fallback below it reported `Unknown function '{name}'`, which is the
`module_member` encoding. It is `written` now. No builtin reaches the fallback
today — every one in `MODULE_FUNCTIONS` has a requirement or an arm — so this is
the same fix as §23 applied to the other door, and it is why that door is closed
rather than left waiting for a caller to reach it.

## 28. Still open after round three

- **§8 stands, and §22 makes it slightly worse.** `PI` and `E` are still in the
  globals map with nothing to declare them, so the one way to spell a constant
  today is the `constant` declaration §25's example now uses.
- **§2's unimplemented builtins and §19's five bullets** are unchanged by this
  round. **§3, §9 and §11** likewise. **§4's Properties half** is closed by §25.
  **§5's GRAMMAR.md productions** are left as §26 says, and **§When** in SPEC.md
  is a whole section about `when`/`case`, which §26 records as reserved and
  unimplemented but does not rewrite — that is the next spec-drift phase's.
- **`catch error` binds the text `"error"`, not the failure's message.** A `catch`
  is a *signal* that something failed; §Error Handling now says so, because that
  is what it is. Carrying the message would mean `Error::Runtime`'s payload
  crossing into a scope binding, which is a change to what a caught failure is and
  to every program that catches one. Not this round's, and not asked for.
- **The `math.random` non-finite branch cannot be provoked.** Interpolating
  between two finite numbers cannot overflow — the sum is bounded by the larger
  end — so the check that remains is a backstop for a `Value::Number` a Rust
  caller built out of range, exactly as `low_bits` is a backstop for a bound no
  call site reaches. It is named rather than silent because the arithmetic is
  float arithmetic, and that is all it is for.
