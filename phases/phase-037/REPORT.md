# Phase 037 — Seed the `random` builtins and honour their range arguments

## Resume run — what this run did

The previous attempt's implementation was already merged and its three cargo gates
were green, so this run changed no Rust. It re-measured every Definition-of-Done
line against the built binary, verified each accept line against **this run's**
brief rather than the previous report's arithmetic (the manifest was corrected
after that report was written — see below), and then went looking for the same
kind of gap the phase has now produced twice.

It found one, and fixed it: **the `random_shuffle` test passed on the exact defect
the phase names.** Details under "The test that could not fail".

## What changed in this run

| File | Lines | What |
|---|---|---|
| `tests/random_builtin_test.rs` | +98 | New test `edge_random_shuffle_reaches_far_more_permutations_than_it_can` |
| `phases/phase-037/FINDINGS.md` | +55 −27 | New finding #7 (the shuffle test was vacuous); finding #2 rewritten as resolved |
| `phases/phase-037/REPORT.md` | rewritten | This file |

`src/runtime.rs` is byte-identical to the committed one — `git diff --stat src/`
after this run is empty. The implementation below came from the earlier attempts
and is reported, not re-claimed.

## What changed (whole phase, cumulative)

| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +239 −21 | A seeded SplitMix64 generator in a thread-local, the `random_seed` builtin, and `random`/`random_number`/`random_choice`/`random_shuffle` redrawn from it with their range arguments honoured |
| `src/stdlib.rs` | +7 | Registers `random` and `random_seed` as reachable builtins |
| `src/repl/completer.rs` | +1 | Completes `random_seed` |
| `tests/random_builtin_test.rs` | +777 | 18 tests |
| `examples/random.rb` | +27 | A seeded example, now deterministic |
| `tests/bytecode_vm_test.rs` | +16 −8 | `examples/random.rb` leaves `NOT_COMPARABLE` — it is deterministic now, so the comparison says something |
| `tests/bootstrap_selfhost_test.rs` | +4 −4 | Stage-2 corpus count 8 → 9 |
| `SPEC.md` | +24 −2 | Documents the five random builtins, the seeding contract, the whole-member rule |

### The four arms

Every one took `SystemTime::now()`; none has a clock left in it. The only remaining
`SystemTime::now()` in `src/runtime.rs` is `src/runtime.rs:809`, in `time_now`,
which is supposed to read the clock.

| Builtin | Was | Now |
|---|---|---|
| `random(min, max)` | `(now.as_nanos() % 1000) as f64` — **took no arguments at all** | A whole number from `[min, max]`, both ends inclusive |
| `random_number(min, max)` | `min + (now.as_nanos() % 1000000)/1000000 * (max - min)` | A number from `[min, max)`, from the generator |
| `random_choice(list)` | `items[(now.as_nanos() as usize) % items.len()]` | `items[next_bits() % len]`; an empty list is refused instead of answered `nothing` |
| `random_shuffle(list)` | `seed` computed **once**, then `seed % (i + 1)` reused for every swap | Fisher-Yates with a fresh draw at each step |
| `random_seed(n)` | did not exist | Sets the generator, so everything after is reproducible |

The generator is SplitMix64 — a bijection on `u64` with every input bit reaching
every output bit. An LCG would have been shorter, but its low bits are the high
bits of the previous step and the low bits are what `random(0, 100)` selects on,
so it would walk a short cycle instead of spreading.

State is `thread_local`, not global and not a lock: `cargo test` runs these tests
in parallel, and a shared counter would let two tests interleave their draws and
make a pinned sequence unreproducible. A Redblue program is single-threaded, so
thread-local *is* process-local here.

## The test that could not fail

**This run's finding, and a new test because of it.**

`edge_random_shuffle_permutes_its_input_for_every_seed` sweeps seeds 0..=99 and
asserts each answer is a permutation of its input and is not the identity. Those
are the two properties a shuffle must have — and the phase's own defect passes
both. I reinstated the old shuffle, one draw reused as `seed % (i + 1)` for every
swap, and ran the suite:

```
test result: ok. 17 passed; 0 failed; 0 ignored
```

All green on the bug. Reusing one draw across the loop still composes to a
permutation, and it is still rarely the identity: with 840 reachable outcomes the
chance of landing on the identity is about 1/840. Both assertions were satisfiable
by the defect they were written for.

What the defect cannot do is **reach**. With one draw reused, every `j` is
`once % (i + 1)`, so the entire permutation is determined by `once % lcm(2..8)` =
**840** of the `8!` = 40320 permutations. A Fisher-Yates drawing per step has no
such ceiling.

```
$ cargo test --test random_builtin_test edge_random_shuffle_reaches
test result: ok. 1 passed; 0 failed
```

The new test, under the reinstated defect:

```
test edge_random_shuffle_reaches_far_more_permutations_than_it_can ... FAILED
2000 shuffles of an eight-element list reached only 770 distinct
permutations. Reusing one draw for every step caps the reachable set at
lcm(2..=8) = 840, so this is the `seed % (i + 1)` computed once and reused
for every swap — the exact shape of the original defect

test result: FAILED. 17 passed; 1 failed
```

Red for the right reason, green on the shipped code, and the mutation was reverted
(`git diff --stat src/runtime.rs` empty).

| | Real Fisher-Yates | One-draw defect | Ceiling |
|---|---|---|---|
| 2000 shuffles, 8 elements | **1969** distinct | 770 | 840 |
| 500 shuffles, 6 elements | **359** distinct | — | 60 |

Thresholds are deliberately loose (840 against 1969, 60 against 359) so they
survive a change of mixer without retuning, and the test names no particular
permutation and compares against no golden list — that would encode the
generator's internals.

I got this wrong once before getting it right: the first version of the test
asserted that 40 consecutive shuffles differ from one another. That also passes
under the mutation, because reusing the draw across *steps* still leaves each
*call* with a fresh one. A variation check is not a reach check. Both are written
up in FINDINGS.md #7.

## Tests added

All in `tests/random_builtin_test.rs`. Every one seeds before it draws, unless it
is specifically about the default seed.

| Test | Edge class covered |
|---|---|
| `random_honours_its_range_arguments` | singleton/boundary — `random(b, b) == b` for b in {5, 0, −3, 1e6}; 200 draws of `random(0, 100)` name at most 101 distinct values, every one in `[0, 100]` and integral |
| `edge_two_processes_seeded_alike_print_identical_output` | determinism — two `rb` processes, same seed, byte-identical over 50 iterations of all four builtins |
| `edge_two_processes_that_never_seed_still_agree` | determinism — the same with **no** `random_seed` at all; pins the `DEFAULT_SEED` claim |
| `edge_seeding_moves_the_generator` | determinism — unseeded, seed 424242 and 424243 are three different sequences |
| `random_draws_are_reproducible_from_a_seed` | determinism — same seed, same 64-draw sequence, in one process |
| `edge_random_number_spreads_its_draws_across_the_range` | distribution — 1000 draws of `random_number(0, 100)`, every decile in 40–160 |
| `edge_random_shuffle_permutes_its_input_for_every_seed` | seeds 0..=99, 8-element list: a permutation, and never the identity |
| **`edge_random_shuffle_reaches_far_more_permutations_than_it_can`** | **new this run — reachable set of 8! and 6! permutations, which is what separates a real shuffle from the one-draw defect** |
| `edge_random_choice_reaches_every_element_of_the_list` | 1000 draws from a 4-element list reach all four |
| `edge_random_refuses_the_arguments_it_cannot_use` | empty list, single-element list, min > max, negative range, non-number argument |
| `edge_a_range_of_no_width_is_still_a_range` | zero-width ranges — `random(5,5)`, `random(5,5.5)`, `random_number(1,1)` |
| `edge_random_answers_a_whole_number_from_a_range_with_a_fractional_end` | 3000 seeds over four fractional-ended ranges: every answer whole and inside the range; a range with no whole member is refused |
| `edge_random_shuffle_keeps_every_repeated_member` | duplicate keys — `[1, 1, 2, 2, 2]` keeps its multiplicity |
| `edge_random_choice_and_shuffle_carry_nested_values` | nesting — a nested member is chosen and shuffled whole |
| `edge_random_seed_refuses_a_seed_it_cannot_use` | malformed input — no argument, two arguments, text, a list, `1e400` |
| `edge_many_draws_stay_in_range_and_do_not_grow` | resource limit — 20 000 draws of `random(0, 5)` |
| `edge_random_refuses_a_range_no_number_can_measure` | numeric boundary — `random(-1e308, 1e308)` and `random_number(-1e308, 1e308)` |
| `both_engines_agree_on_a_seeded_sequence` | the tree-walker and the bytecode VM draw the same sequence from one seed |

### The tests can fail — three mutations, each reverted

**(a) The fractional-bound defect**, found by the previous attempt and written as a
failing test before `src/runtime.rs` was touched:

```
panicked at tests/random_builtin_test.rs:460:
assertion `left == right` failed: `random` is the whole-number draw, but
random(0, 100.5) answered the fraction 100.5 for seed 581
  left: 0.5
 right: 0.0
```

**(b) The default seed** — the generator was mutated to read `SystemTime::now()` on
first use per thread instead of `DEFAULT_SEED`:

```
test edge_two_processes_that_never_seed_still_agree ... FAILED
test edge_a_range_of_no_width_is_still_a_range ... FAILED
test result: FAILED. 14 passed; 2 failed
  panicked: two processes that never seeded printed different things, so the
            default seed is not fixed
```

**(c) The one-draw shuffle**, this run's — quoted in full above. 17 passed on the
defect before the new test existed; 1 failed after it.

All three reverted. `git diff --stat src/runtime.rs` is empty.

## Edge-case matrix

| Row | Result |
|---|---|
| empty | covered — `random_choice([])` refused, `random_shuffle([])` is `[]`, `random(5,5)` and `random_number(1,1)`, `random(0.2, 0.8)` (no whole member) |
| singleton | covered — one-element list for `random_choice` (4 seeds) and `random_shuffle`; `random(b,b)` for 4 bounds; `random(5, 5.5)`'s single member |
| boundary | covered — both members of `[0,1]` reachable over 65 seeds; zero-width ranges; fractional ends over 3000 seeds each |
| out_of_bounds | covered — 200 draws of `random(0, 100)` stay in `[0, 100]`; 3000 draws of each fractional range stay inside it; a reversed range is refused |
| type_mismatch | covered — 8 non-number arguments across all five builtins, each a clean `Runtime` error |
| numeric_boundary | covered — `1e400` seed, `-1e308..1e308` for both range builtins, exactness at `2^53`, the `2^53` scaling guard, whole-number answers from fractional bounds |
| unicode | N/A — the draws are numbers. The list a draw selects from is held as `Value`, and text handling is a lexer/value concern covered by `lexer_robustness_test.rs` and `comparison_lex_test.rs`. Nothing here was changed to affect text; one element of the 4-element list in the choice test is `"only"` in the singleton test, so text values do travel through the shuffle and choice paths unaltered |
| nesting_recursion | covered — nested list members chosen and shuffled whole |
| duplicate_missing_keys | covered — a list with repeated members keeps its multiplicity after a shuffle. A missing key is N/A: no builtin here reads a record |
| malformed_input | covered — five malformed `random_seed` calls, each a `Runtime` error not a panic |
| resource_limit | covered — 20 000 draws stay in range and evenly spread; the generator is one `u64`, allocates nothing, and no existing guard was raised |

## Definition of done — every line measured

Measured against the built binary on this run, not read off the test suite, and
each against **this run's brief**. The manifest was corrected after the previous
report was written: the 101-distinct bound now attaches to `random()` only,
`random_number` carries the non-monotonicity property, and the bucket band reads
40–160. All of those are met. (The earlier run's complaint that they were
unsatisfiable was correct against the *then-current* manifest and is withdrawn;
FINDINGS.md #2 records that.)

- [x] **`say random(5, 5)` prints exactly `5`** — printed `5`. `random(1, 1)`
      prints `1`; it printed `403` before.

  ```
  $ ./target/debug/rb run target/tmp/dod1.rb   # say random(5,5) / say random(1,1)
  5
  1
  ```

- [x] **200 draws of `random(0, 100)` name at most 101 distinct values** — **85**
      measured, 0 fractional, 0 outside `[0, 100]`. Was 179.

- [x] **200 draws of `random_number(0, 100)` produce 200 distinct values, and are
      neither monotonic nor a fixed arithmetic sequence** — **200 distinct**, 0
      outside `[0, 100)`, and the consecutive differences split 99 rises / 100
      falls. First eight:

  ```
  1.582909387275222 66.39942340488506 16.920437987610846 52.241421811607566
  1.2837748982554853 7.712950416341902 56.5279726411436 31.01882998970825
  ```

  Before, eight consecutive draws were `4.8086, 5.8041, 6.0445, 6.2037, 6.36,
  6.5312, 6.6935, 6.8537` — strictly increasing.

- [x] **1000 draws of `random_number(0, 100)` from the pinned seed put 40–160 in
      each of the ten decile buckets** — `[105, 95, 103, 94, 101, 98, 116, 100,
      89, 99]`, mean 100.0, 0 outside the band.

- [x] **`random_shuffle` is a permutation of its input for every seed 0..=99 and
      is not the identity for any of them** — `non_permutations=0 identity=0`,
      checked in 100 separate `rb` processes.

- [x] **`random_choice` over a four-element list hits all four in 1000 draws** —
      `255 / 246 / 253 / 246`.

- [x] **Two separate processes with an explicit seed print byte-identical output**
      — `diff` empty, both 2465 bytes, same SHA-256 prefix `e4c7d5504604ce0f`.
      Program: `random_seed(987654321)` then 50 iterations of all four builtins.

  ```
  $ ./target/debug/rb run target/tmp/dod4.rb > a
  $ ./target/debug/rb run target/tmp/dod4.rb > b
  $ diff a b ; echo $?
  0
  ```

- [x] **edge_\* tests cover an empty list, a single-element list, min greater than
      max, a negative range, and a non-number argument** — all five in
      `edge_random_refuses_the_arguments_it_cannot_use`, plus six others in that
      file (14 `edge_*` tests total).

- [x] **No `SystemTime` or `Instant` remains in the `random`, `random_number`,
      `random_choice` or `random_shuffle` arms; clippy clean with no
      `allow(dead_code)`** — the only `SystemTime::now()` in `src/runtime.rs` is
      `src/runtime.rs:809`, in `time_now`. `grep -n "dead_code" src/runtime.rs`
      returns nothing; clippy is clean with zero warnings and `-D warnings`.

- [x] **Every `.rb` file under `examples/` and `modules/` still runs** —
      9 files, `pass=9 fail=0`, including the new `examples/random.rb`.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass**, no diff |
| `cargo clippy --all-targets -- -D warnings` | **pass**, zero warnings, no `allow` added |
| `cargo test --all-targets` | **1199 passed, 0 failed, 0 ignored** across 42 targets — the 42 integration test files, the lib's unit tests and the `rb` bin. `cargo test --doc` also passes (2 passed) |
| `./rbops/verify.sh phase-037` | **not run — `rbops/verify.sh` does not exist in this checkout** |

All three cargo gates above were re-run on this run and printed what is recorded
here. Nothing is carried over unverified.

On the fourth gate: `ls rbops` → `No such file or directory`, `ls verify.sh` →
`No such file or directory`, and `git ls-files | grep verify` finds nothing. The
`rbops/` directory is absent from the checkout entirely. I did not create it (hard
rule 1). The fourth gate is **unverified**; whoever runs the gate needs to run it
before this phase is `.done`.

## Invariants touched

- None. The `Value` variants, the `Error` variants, the `.rb` extension, the
  `end` terminator, `set x to …`, `say`, and the trailing-comma / `{interp}` string
  syntax are all unchanged.
- `random` and `random_number` now take their range arguments seriously, and
  `random_seed` is new. These are behavioural changes to builtins, which is what
  the phase asked for; no grammar rule and no type changed.
- `random_choice([])` now raises a `Runtime` error instead of answering
  `nothing`. `nothing` is still a value the language has — this only removes a
  place where a builtin returned it to mean "no answer", which a program could not
  distinguish from a choice that had not happened yet.
- `random` answers whole numbers even when given fractional bounds, and refuses a
  range containing no whole number. The members of a range are `ceil(min)..=floor(max)`.
  Documented in SPEC.md.

## Known gaps / follow-ups

- A seeded generator is not an unpredictable one, so a Redblue program now has no
  source of unpredictable data at all. Real capability gap, out of scope here,
  needs its own phase → FINDINGS.md #3.
- `random` is inclusive at both ends and `random_number` is half-open. Both
  readings are defensible and both are pinned by this phase's requirements, so
  reconciling them is a language decision → FINDINGS.md #4.
- The bytecode compiler still refuses function literals, which is why the
  two-engine test and `examples/random.rb` avoid `map`/`to (x)` → FINDINGS.md #5.
- `rbops/verify.sh` is absent, so gate four is unverified → FINDINGS.md #1.
- FINDINGS.md #7: the shuffle test passed on the defect it was written for. Fixed
  by the new reach test, and the generalisable lesson — for a randomised
  operation, pin the size of the reachable set, not variance — is written up for
  the auditor there.