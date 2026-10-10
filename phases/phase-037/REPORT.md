# Phase 037 — Seed the `random` builtins and honour their range arguments

## Resume run — what this run did

The inherited implementation (SplitMix64 generator + the fractional-bounds fix in
`random_int`) was already merged and its suite was green. This run:

1. Ran all four gates **before** touching anything. The three cargo gates passed;
   `rbops/verify.sh` does not exist in this checkout (see *Gates* and `FINDINGS.md`
   §1).
2. Re-measured **every** Definition-of-Done line against the built binary rather
   than trusting the numbers in the inherited report.
3. Found one real gap and closed it: the corrected `random_number` acceptance
   property — *"consecutive draws are not monotonic and are not a fixed arithmetic
   sequence"* — was **measured but not pinned by any test**. The old REPORT quoted
   figures for it; no assertion in the suite could fail on it. That is exactly the
   "an assertion that cannot fail" finding AGENTS.md §3.1 and the reviewer contract
   exist to catch.
4. **Withdrew the stale "unsatisfiable Definition of Done" complaint** carried in
   the inherited report. It was addressed to criteria that no longer exist in the
   manifest. Verified against the current PROMPT: every line is met. See *The
   withdrawn complaint*, below.

`src/runtime.rs` is byte-identical to the committed file (`git diff --stat
src/runtime.rs` is empty). The only files this run changed are
`tests/random_builtin_test.rs`, this report, and `FINDINGS.md`.

## What changed

| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +239 −21 | *(inherited)* A seeded SplitMix64 generator in a thread-local, the `random_seed` builtin, and `random`/`random_number`/`random_choice`/`random_shuffle` redrawn from it with their range arguments honoured |
| `src/stdlib.rs` | +7 −0 | *(inherited)* Registers `random` and `random_seed` as reachable builtins |
| `src/repl/completer.rs` | +1 −0 | *(inherited)* Completes `random_seed` |
| `tests/random_builtin_test.rs` | +679 *(inherited)*, **+56 this run** | 19 tests. This run adds `edge_random_number_draws_are_not_a_march_and_not_an_arithmetic_sequence` |
| `examples/random.rb` | +27 *(inherited)* | A seeded example, now deterministic |
| `tests/bytecode_vm_test.rs` | +16 −8 *(inherited)* | `examples/random.rb` leaves `NOT_COMPARABLE` — it is deterministic now, so the comparison says something |
| `tests/bootstrap_selfhost_test.rs` | +4 −4 *(inherited)* | Stage-2 corpus count 8 → 9 for the new example |
| `SPEC.md` | +24 −2 *(inherited)* | Documents the five random builtins, the seeding contract, and the whole-member rule for a fractional bound |

### The four arms

Every one took `SystemTime::now()`; none is left with a clock in it. The only
remaining `SystemTime::now()` in `src/runtime.rs` is at **src/runtime.rs:809**, in
`time_now`, which is supposed to read the clock.

| Builtin | Was | Now |
|---|---|---|
| `random(min, max)` | `(now.as_nanos() % 1000) as f64` — **took no arguments at all** | A whole number from `[min, max]`, both ends included |
| `random_number(min, max)` | `min + (now.as_nanos() % 1000000)/1000000 * (max - min)` | A number from `[min, max)`, drawn from the generator |
| `random_choice(list)` | `items[(now.as_nanos() as usize) % items.len()]` | `items[next_bits() % len]`; an empty list is refused instead of answered `nothing` |
| `random_shuffle(list)` | `seed` computed **once**, then `seed % (i + 1)` reused for every swap | Fisher-Yates with a fresh draw at each step |
| `random_seed(n)` | did not exist | Sets the generator, so everything after is reproducible |

The generator is SplitMix64 — a bijection on `u64` with every input bit reaching
every output bit. An LCG would have been shorter but its low bits are the high
bits of the previous step, and the low bits are what `random(0, 100)` selects on,
so it would walk a short cycle instead of spreading.

State is `thread_local`, not global and not a lock: `cargo test` runs these tests
in parallel, and a shared counter would let two tests interleave their draws and
make a pinned sequence unreproducible. A Redblue program is single-threaded, so
thread-local *is* process-local here.

## The gap this run closed

The inherited suite pinned the *spread* of `random_number` over 1000 draws
(`edge_random_number_spreads_its_draws_across_the_range`) but not the *shape of
consecutive draws*, which the corrected Definition of Done requires:

> 200 draws of `random_number(0, 100)` … consecutive draws are not monotonic and
> are not a fixed arithmetic sequence — today eight consecutive draws are strictly
> increasing

That is the symptom the finding reported verbatim — `4.8086, 5.8041, 6.0445,
6.2037, 6.36, 6.5312, 6.6935, 6.8537`, strictly increasing — and no assertion could
fail on it. New test
`edge_random_number_draws_are_not_a_march_and_not_an_arithmetic_sequence`
(tests/random_builtin_test.rs:266) asserts three things on one pinned seed:

- 200 draws name **200 distinct** values (`random_number` is the fractional draw
  over `[0, 100)`, which holds ~`9 × 10^17` representable values; a whole-number
  member bound is not a property it has — `random(0, 100)`, the whole-number draw,
  is the one with 101 members, and that is pinned separately);
- consecutive steps include **both a rise and a fall**;
- the 199 consecutive differences take **≥ 100 distinct** values (an arithmetic
  sequence has exactly one; measured on this seed: **199**).

## The tests can fail — demonstrated, not asserted

Every claim below is a mutation of `src/runtime.rs`, run and reverted. The tree is
clean of them (`git diff --stat src/runtime.rs` → empty).

**(a) This run's new test, against the original defect.** `random_float` was
reinstated on a monotonic counter — the shape the finding reported — and the new
test failed by name, on the monotonic assertion:

```
test edge_random_number_draws_are_not_a_march_and_not_an_arithmetic_sequence ... FAILED
panicked at tests/random_builtin_test.rs:288:
consecutive draws stepped 199 times up and 0 times down over 199 steps; every step
the same way means the draws are marching, which is exactly what the monotonic clock
source produced
```

and, on the same mutation, the bucket test failed with the other half of the
reported symptom:

```
test edge_random_number_spreads_its_draws_across_the_range ... FAILED
panicked at tests/random_builtin_test.rs:249:
decile 0 of random_number(0, 100) took 1000 of 1000 draws; a generator spread over
the range puts about 100 in each
```

A second mutation (a coarse counter, step 7 919 311) tripped the *other*
assertion instead, so all three assertions have teeth rather than one:

```
panicked at tests/random_builtin_test.rs:301:
199 consecutive differences took only 11 distinct values; a fixed arithmetic
sequence has exactly one, and this one is not it
```

**(b) The fractional-bound defect** *(inherited, reproduced here)* — written as a
failing test first, and it failed before `src/runtime.rs` was touched:

```
panicked at tests/random_builtin_test.rs:460:
assertion `left == right` failed: `random` is the whole-number draw, but
random(0, 100.5) answered the fraction 100.5 for seed 581
  left: 0.5
 right: 0.0
```

**(c) The default seed** *(inherited)* — `RANDOM_STATE` was made to read
`SystemTime::now()` on first use per thread instead of `DEFAULT_SEED`:

```
test edge_two_processes_that_never_seed_still_agree ... FAILED
test edge_a_range_of_no_width_is_still_a_range ... FAILED
test result: FAILED. 14 passed; 2 failed
  panicked: two processes that never seeded printed different things, so the
            default seed is not fixed
```

**(d) The shuffle reach ceiling** *(inherited)* — the old one-draw-reused shuffle
passed the permutation and not-the-identity assertions; see `FINDINGS.md` §7.

## Tests added

All in `tests/random_builtin_test.rs` (19 tests, 16 named `edge_*`). Every one
seeds before it draws, unless it is specifically about the default seed.

| Test | Edge class covered |
|---|---|
| `random_honours_its_range_arguments` | singleton/boundary — `random(b, b) == b` for b in {5, 0, −3, 1e6}; 200 draws of `random(0, 100)` name at most 101 distinct values and every one is in `[0, 100]` and integral |
| **`edge_random_number_draws_are_not_a_march_and_not_an_arithmetic_sequence`** *(this run)* | non-monotonicity — 200 distinct draws; rises and falls both present; 199 distinct consecutive differences |
| `edge_two_processes_seeded_alike_print_identical_output` | determinism — two `rb` processes, same seed, byte-identical over 50 iterations of all four builtins |
| `edge_two_processes_that_never_seed_still_agree` | determinism — the same, with **no** `random_seed` call. Pins the `DEFAULT_SEED` claim in `src/runtime.rs` and SPEC.md |
| `edge_seeding_moves_the_generator` | determinism — the unseeded sequence, seed 424242 and seed 424243 are three different sequences |
| `random_draws_are_reproducible_from_a_seed` | determinism — same seed, same 64-draw sequence, in one process |
| `edge_random_number_spreads_its_draws_across_the_range` | distribution — 1000 draws of `random_number(0, 100)`, every decile in 40–160 |
| `edge_random_shuffle_permutes_its_input_for_every_seed` | seeds 0..=99, 8-element list: a permutation of the input, and never the identity |
| `edge_random_shuffle_reaches_far_more_permutations_than_it_can` | reach — 2000 shuffles of 8 elements exceed the `lcm(2..=8)` = 840 ceiling the one-draw defect implies |
| `edge_random_choice_reaches_every_element_of_the_list` | 1000 draws from a 4-element list reach all four |
| `edge_random_refuses_the_arguments_it_cannot_use` | empty list, single-element list, min > max, negative range, non-number arguments |
| `edge_a_range_of_no_width_is_still_a_range` | boundary — `random(5,5)`, `random(5,5.5)`, `random_number(1,1)`, both members of `[0,1]` reachable over 65 seeds |
| `edge_random_answers_a_whole_number_from_a_range_with_a_fractional_end` | boundary / numeric boundary — 3000 seeds over four fractional-ended ranges |
| `edge_random_shuffle_keeps_every_repeated_member` | duplicate keys — `[1, 1, 2, 2, 2]` keeps its multiplicity |
| `edge_random_choice_and_shuffle_carry_nested_values` | nesting — a nested member is chosen and shuffled whole |
| `edge_random_seed_refuses_a_seed_it_cannot_use` | malformed input — no argument, two arguments, text, a list, `1e400` |
| `edge_many_draws_stay_in_range_and_do_not_grow` | resource limit — 20 000 draws of `random(0, 5)`, every member within a factor of two of the mean |
| `edge_random_refuses_a_range_no_number_can_measure` | numeric boundary — `random(-1e308, 1e308)` and `random_number(-1e308, 1e308)` |
| `both_engines_agree_on_a_seeded_sequence` | The tree-walker and the bytecode VM draw the same sequence from one seed |

## Definition of done — every line re-measured against the built binary

Each number below was produced by running `./target/debug/rb` in this checkout on
this run, not read off the test suite.

| Line | Required | Measured | |
|---|---|---|---|
| `say random(5, 5)` | prints exactly `5` | `5` (`random(1, 1)` → `1`; was `403`) | met |
| 200 draws `random(0, 100)`, distinct | ≤ 101 | **91** seeded at 12345, **85** unseeded | met |
| those 200 draws | in range, integral | 0 outside `[0, 100]`, 0 fractional | met |
| 200 draws `random_number(0, 100)`, distinct | 200 | **200** | met |
| consecutive steps | not monotonic | **99 rises, 100 falls** of 199 steps | met |
| consecutive steps | not a fixed arithmetic sequence | **199 distinct** differences of 199 | met |
| 1000 draws `random_number(0, 100)`, deciles | each in 40–160 | `[105, 95, 103, 94, 101, 98, 116, 100, 89, 99]` | met |
| `random_shuffle`, seeds 0..=99 | permutation, never identity | `non_permutations=0 identity=0` over 100 processes | met |
| `random_choice`, 4 elements, 1000 draws | all four reached | a=255 b=246 c=253 d=246 | met |
| two seeded processes | byte-identical | see below | met |
| `SystemTime`/`Instant` in the four arms | none | none; only `src/runtime.rs:809`, `time_now` | met |
| no `allow(dead_code)` added | none | none | met |
| `examples/` + `modules/` | all still run | **pass=9 fail=0** | met |

First eight draws of `random_number(0, 100)` at seed 12345, for the record:
`1.5829, 66.3994, 16.9204, 52.2414, 1.2838, 7.7130, 56.5280, 31.0188`.

### Two separate processes, byte-identical

```
$ ./target/debug/rb run two.rb > two_a.txt
$ ./target/debug/rb run two.rb > two_b.txt
$ diff two_a.txt two_b.txt
BEGIN
END (bytes: 0)
$ md5sum two_a.txt two_b.txt
ff076ddac62824049a2993b6238acc25  two_a.txt
ff076ddac62824049a2993b6238acc25  two_b.txt
```

**The diff is empty** — 0 bytes, 200 lines each.

## The withdrawn complaint

The inherited report argued that two Definition-of-Done lines were arithmetically
unsatisfiable: that the 101-distinct bound was attached to `random_number`, and
that the decile band read 400–600 against a 1000-draw count.

**That complaint is withdrawn as stale.** It was addressed to an earlier manifest.
The current PROMPT attaches the 101-distinct bound to `random()` only — the
whole-number draw, which has 101 members and measures 91 — gives `random_number`
the checkable non-monotonicity property, and reads the band as 40–160, which the
measured deciles `[105, 95, 103, 94, 101, 98, 116, 100, 89, 99]` meet with a mean
of exactly 100.0. No test was bent, loosened or deleted to reach those numbers;
they were measured after the fact.

`FINDINGS.md` §2 records the same correction, with the instruction that a resumed
run re-read the complaint against its own current PROMPT before acting on it.

## Edge-case matrix

| Row | Result |
|---|---|
| empty | covered — `random_choice([])` is refused, `random_shuffle([])` is `[]`, `random(5,5)` and `random_number(1,1)`, `random(0.2, 0.8)` (no whole member) |
| singleton | covered — one-element list for `random_choice` (4 seeds) and `random_shuffle`; `random(b,b)` for 4 bounds; `random(5, 5.5)`'s single member |
| boundary | covered — both members of `[0,1]` reachable over 65 seeds; zero-width ranges; fractional ends over 3000 seeds each |
| out_of_bounds | covered — 200 draws of `random(0, 100)` stay in `[0, 100]`; 3000 draws of each fractional range stay inside it; a reversed range is refused |
| type_mismatch | covered — 8 non-number arguments across all five builtins, each a clean `Runtime` error |
| numeric_boundary | covered — `1e400` seed, `-1e308..1e308` for both range builtins, exactness at `2^53`, the `2^53` scaling guard, whole-number answers from fractional bounds |
| unicode | **N/A** — the draws are numbers. The list a draw selects from is held as a `Value`, and text handling is a lexer/value concern covered by `lexer_robustness_test.rs` and `comparison_lex_test.rs`. Nothing in this phase touches text handling, and no draw can be mis-encoded by it. |
| nesting_recursion | covered — nested list members chosen and shuffled whole |
| duplicate_missing_keys | covered — a list with repeated members keeps its multiplicity after a shuffle. A *missing key* is N/A: no builtin here reads a record |
| malformed_input | covered — five malformed `random_seed` calls, every one a `Runtime` error not a panic |
| resource_limit | covered — 20 000 draws stay in range and evenly spread; the generator is one `u64` and allocates nothing; no existing guard was raised |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** (exit 0, no diff) |
| `cargo clippy --all-targets -- -D warnings` | **pass** (exit 0, zero warnings, no `allow` added) |
| `cargo test --all-targets` | **1200 passed, 0 failed, 0 ignored** across 44 targets |
| `./rbops/verify.sh phase-037` | **not run — `rbops/verify.sh` does not exist in this checkout** |

All three cargo gates were run on this run, before and after the change, and
printed what is recorded here.

**On the fourth gate.** `ls rbops` → `No such file or directory`.
`./rbops/verify.sh phase-037` → `bash: ./rbops/verify.sh: No such file or
directory`. `git ls-files | grep -c rbops` → `0`: no tracked file under `rbops/`
exists in this checkout at any commit, so the directory has never been present
here. I did not create it — hard rule 1 forbids touching `rbops/`. The fourth gate
is **unverified**, and a reviewer must run it before this phase is `.done`.
`FINDINGS.md` §1 carries the same fact.

## Known gaps / follow-ups

All in `FINDINGS.md`, none fixed here (hard rule 7 — one phase, one concern):

- **§3** — a seeded generator is not an unpredictable one. Redblue now has no
  source of unpredictable data at all. A capability gap worth its own phase.
- **§4** — `random` is inclusive at both ends, `random_number` at the low end only.
  Both are individually defensible and both are pinned by existing tests
  (`tests/numeric_edge_test.rs` depends on the half-open reading), so reconciling
  them is a language decision, not a bug fix.
- **§5** — the bytecode compiler still refuses function literals, so no
  seeded-draw test can compare the two engines through `map`.
- **§7** — for any randomised operation, "well-formed" and "varies" are both
  satisfiable by a one-value generator; pin **reach**, not variance.