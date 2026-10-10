# Phase 037 — Seed the `random` builtins and honour their range arguments

## Resume run — what this run did

The previous attempt's implementation was already merged and its suite was
green, so this run changed no Rust. It re-ran all four gates from scratch,
re-measured every line of the Definition of Done against the built binary
rather than trusting the numbers below, and corrected the two that had drifted:

- `cargo test --all-targets` prints **1179 passed, 0 failed, 0 ignored**, not
  the 1121 the earlier run recorded. Same tree; the earlier figure was simply
  not what the command prints.
- The 200-draw distinct count was ambiguous (91 with a seed, unspecified
  without). Both are now stated and both were measured: 85 unseeded, 91 at
  `random_seed(12345)`.

Nothing the previous run claimed is contradicted here, and nothing was weakened
to make it true. `src/runtime.rs` is byte-identical to the committed one — the
only files this run touched are this report and `FINDINGS.md`.

## What changed

| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +239 −21 | A seeded SplitMix64 generator in a thread-local, the `random_seed` builtin, and `random`/`random_number`/`random_choice`/`random_shuffle` redrawn from it with their range arguments honoured |
| `src/stdlib.rs` | +7 −0 | Registers `random` and `random_seed` as reachable builtins |
| `src/repl/completer.rs` | +1 −0 | Completes `random_seed` |
| `tests/random_builtin_test.rs` | +679 | New file: 17 tests |
| `examples/random.rb` | +27 | New file: a seeded example, now deterministic |
| `tests/bytecode_vm_test.rs` | +16 −8 | `examples/random.rb` leaves `NOT_COMPARABLE` — it is deterministic now, so the comparison says something |
| `tests/bootstrap_selfhost_test.rs` | +4 −4 | The stage-2 corpus count 8 → 9, for the new example. The byte-identical comparison itself passed unchanged |
| `SPEC.md` | +24 −2 | Documents the five random builtins, the seeding contract, and the whole-member rule for a fractional bound |

### The four arms

Every one took `SystemTime::now()`; none is left with a clock in it. The only
remaining `SystemTime::now()` in `src/runtime.rs` is at line 786, in `time_now`,
which is supposed to read the clock.

| Builtin | Was | Now |
|---|---|---|
| `random(min, max)` | `(now.as_nanos() % 1000) as f64` — **took no arguments at all** | A whole number from `[min, max]`, both ends included |
| `random_number(min, max)` | `min + (now.as_nanos() % 1000000)/1000000 * (max - min)` | A number from `[min, max)`, drawn from the generator |
| `random_choice(list)` | `items[(now.as_nanos() as usize) % items.len()]` | `items[next_bits() % len]`; an empty list is refused instead of answered `nothing` |
| `random_shuffle(list)` | `seed` computed **once**, then `seed % (i + 1)` reused for every swap | Fisher-Yates with a fresh draw at each step |
| `random_seed(n)` | did not exist | Sets the generator, so everything after is reproducible |

The generator is SplitMix64 — a bijection on `u64` with every input bit reaching
every output bit. An LCG would have been shorter but its low bits are the high
bits of the previous step, and the low bits are what `random(0, 100)` selects
on, so it would walk a short cycle instead of spreading.

State is `thread_local`, not global and not a lock: `cargo test` runs these
tests in parallel, and a shared counter would let two tests interleave their
draws and make a pinned sequence unreproducible. A Redblue program is
single-threaded, so thread-local *is* process-local here.

### The defect this run found and fixed

The first attempt at this phase got the seeded generator right but left a real
hole in `random_int`: when a bound was a fraction it scaled the raw width
instead of counting members, so

```redblue
random(0, 100.5)   // could answer 100.5, or 101 — outside the range
```

Measured on the pre-fix binary, seed 581 answered `100.5`, and a scan of 3000
seeds found 26 such draws. `random` is the whole-number draw —
`random_honours_its_range_arguments` already asserts `value.fract() == 0.0` — so
those answers broke the contract the phase's own test pinned, and `101` was
outside the range entirely. `random(0.2, 0.8)`, a range holding no integer at
all, answered `0.2`.

The members of a range are its whole numbers, `ceil(min)..=floor(max)`, and the
fix counts those. `[0, 100.5]` has members `0..=100`, so the draw answers `100`
and never `100.5` or `101`; `[0.2, 0.8]` has none, so it is refused by name.
The modulo path and the `2^53` bias guard are unchanged in shape; `width.fract()
== 0.0` went away because the member count is a whole number by construction.

## Reproduction, before

```
$ printf 'say random(1, 1)\n' > target/tmp/repro.rb
$ ./target/debug/rb run target/tmp/repro.rb
403
```

## Tests added

All in `tests/random_builtin_test.rs`. Every one seeds before it draws, unless
it is specifically about the default seed.

| Test | Edge class covered |
|---|---|
| `random_honours_its_range_arguments` | singleton/boundary — `random(b, b) == b` for b in {5, 0, −3, 1e6}; 200 draws of `random(0, 100)` name at most 101 distinct values and every one is in `[0, 100]` and integral |
| `edge_two_processes_seeded_alike_print_identical_output` | determinism — two `rb` processes, same seed, byte-identical output over 50 iterations of all four builtins |
| `edge_two_processes_that_never_seed_still_agree` | determinism — the same, with **no** `random_seed` call at all. Pins the `DEFAULT_SEED` claim in `src/runtime.rs` and SPEC.md |
| `edge_seeding_moves_the_generator` | determinism — the unseeded sequence, seed 424242 and seed 424243 are three different sequences, so `random_seed` reaches the generator rather than being a no-op |
| `random_draws_are_reproducible_from_a_seed` | determinism — same seed, same 64-draw sequence, in one process |
| `edge_random_number_spreads_its_draws_across_the_range` | distribution — 1000 draws of `random_number(0, 100)` put 40–160 in each of ten decile buckets. The old source was monotonic, so all 1000 landed in the first decile |
| `edge_random_answers_a_whole_number_from_a_range_with_a_fractional_end` | boundary / numeric boundary — 3000 seeds over four fractional-ended ranges: every answer is a whole number inside the range, every whole member is reachable, and a range with no whole member is refused |
| `edge_random_shuffle_permutes_its_input_for_every_seed` | seeds 0..=99, 8-element list: a permutation of the input, and never the identity |
| `edge_random_choice_reaches_every_element_of_the_list` | 1000 draws from a 4-element list reach all four |
| `edge_random_refuses_the_arguments_it_cannot_use` | empty list, single-element list, min > max, negative range, non-number arguments |
| `edge_a_range_of_no_width_is_still_a_range` | boundary — `random(5,5)`, `random(5,5.5)`, `random_number(1,1)`, and both members of `[0,1]` reachable over 65 seeds |
| `edge_random_shuffle_keeps_every_repeated_member` | duplicate keys — `[1, 1, 2, 2, 2]` still has two 1s and three 2s after a shuffle |
| `edge_random_choice_and_shuffle_carry_nested_values` | nesting — a nested member is chosen and shuffled whole, not its parts |
| `edge_random_seed_refuses_a_seed_it_cannot_use` | malformed input — no argument, two arguments, text, a list, `1e400` |
| `edge_many_draws_stay_in_range_and_do_not_grow` | resource limit — 20 000 draws of `random(0, 5)`, every member within a factor of two of the mean |
| `edge_random_refuses_a_range_no_number_can_measure` | numeric boundary — `random(-1e308, 1e308)` and `random_number(-1e308, 1e308)` |
| `both_engines_agree_on_a_seeded_sequence` | The tree-walker and the bytecode VM draw the same sequence from one seed |

### The tests can fail — demonstrated, not asserted

Two mutations, each reverted.

**(a) The fractional-bound defect.** Written as a failing test first, and it
failed for the right reason before `src/runtime.rs` was touched:

```
panicked at tests/random_builtin_test.rs:460:
assertion `left == right` failed: `random` is the whole-number draw, but
random(0, 100.5) answered the fraction 100.5 for seed 581
  left: 0.5
 right: 0.0
```

**(b) The default seed.** I mutated `src/runtime.rs` so the generator reads
`SystemTime::now()` on first use per thread instead of `DEFAULT_SEED`, and
re-ran the suite:

```
test edge_two_processes_that_never_seed_still_agree ... FAILED
test edge_a_range_of_no_width_is_still_a_range ... FAILED
test result: FAILED. 14 passed; 2 failed
  panicked: two processes that never seeded printed different things, so the
            default seed is not fixed
```

Both mutations were reverted; `git diff --stat src/runtime.rs` now shows only the
intended fix, and the shipped file is byte-identical to the committed one.

## Edge-case matrix

| Row | Result |
|---|---|
| empty | covered — `random_choice([])` is refused, `random_shuffle([])` is `[]`, `random(5,5)` and `random_number(1,1)`, `random(0.2, 0.8)` (no whole member) |
| singleton | covered — one-element list for `random_choice` (4 seeds) and `random_shuffle`; `random(b,b)` for 4 bounds; `random(5, 5.5)`'s single member |
| boundary | covered — both members of `[0,1]` reachable over 65 seeds; zero-width ranges; fractional ends over 3000 seeds each |
| out_of_bounds | covered — 200 draws of `random(0, 100)` stay in `[0, 100]`; 3000 draws of each fractional range stay inside it; a reversed range is refused |
| type_mismatch | covered — 8 non-number arguments across all five builtins, each a clean `Runtime` error |
| numeric_boundary | covered — `1e400` seed, `-1e308..1e308` for both range builtins, exactness at `2^53`, the `2^53` scaling guard, whole-number answers from fractional bounds |
| unicode | N/A — the draws are numbers; the list a draw selects from is held as `Value`, and unicode is a lexer/value concern covered by `lexer_robustness_test.rs` and `comparison_lex_test.rs`. Nothing here was changed to affect text handling |
| nesting_recursion | covered — nested list members chosen and shuffled whole |
| duplicate_missing_keys | covered — a list with repeated members keeps its multiplicity after a shuffle. A missing key is N/A: no builtin here reads a record |
| malformed_input | covered — five malformed `random_seed` calls, every one a `Runtime` error not a panic |
| resource_limit | covered — 20 000 draws stay in range and evenly spread; the generator is one `u64` and allocates nothing, and no existing guard was raised |

## Definition of done, measured

Every item below was measured against the built binary, not read off the test
suite. Re-measured from scratch on the resume run; every number here is one I
reproduced in this checkout.

- `say random(5, 5)` prints `5`. **Met** — printed `5`. `random(1, 1)` prints
  `1`; it printed `403` before.
- 200 draws of `random(0, 100)` in one program: **85 distinct unseeded, 91
  seeded with `random_seed(12345)`** — both under the 101 the range holds. Every
  one integral and inside `[0, 100]` (`awk` over the output: 0 violations). Was
  179. **Met.**
- `random_shuffle` is a permutation of its input for every seed 0..=99 and is
  never the identity. **Met** — checked in 100 separate `rb` processes:
  `non_permutations=0 identity=0`.
- `random_choice` over four elements reaches all four in 1000 draws. **Met** —
  255/246/253/246.
- 1000 draws of `random_number(0, 100)` from seed 12345 land in the ten deciles
  as `[105, 95, 103, 94, 101, 98, 116, 100, 89, 99]` — every one inside the
  40–160 the line's own arithmetic implies. **Met** (see the caveat below on the
  line's 400–600 wording).
- `random_number(0, 100)` draws are neither monotonic nor an arithmetic
  sequence. **Met** — 200 draws, 200 distinct, 0 outside `[0, 100)`; the first
  eight are `43.15, 27.62, 50.26, 77.82, 94.78, 70.17, 95.96, 90.40`, and the
  consecutive differences alternate sign. Before, eight consecutive draws were
  strictly increasing.
- Two separate processes with an explicit seed print byte-identical output.
  **Met** — the diff below is empty.
- Every draw is whole and inside its range even with a fractional bound. **Met**
  — this run's fix; was broken at seed 581.
- All 9 `.rb` files under `examples/` and `modules/` still run: `pass=9 fail=0`.

### Two Definition-of-Done lines that cannot be met as written

I am reporting these rather than bending a test to make them look satisfied.

**"200 draws of `random_number(0, 100)` produce at most 101 distinct values".**
`random_number` is the fractional draw over `[0, 100)`, which holds about
`100 × 2^53 ≈ 9.0 × 10^17` representable values. Two draws of it collide with
probability about `1.1 × 10^-16`, so 200 draws yielding at most 101 distinct
values would require ~99 collisions — the expected number is `2 × 10^-14`. I
measured **200 distinct of 200**. The 101-member bound belongs to `random(0, 100)`,
the whole-number draw, which does meet it (91 ≤ 101). As written, this line
would require `random_number` to discard its fractional part, which would
contradict the same section's requirement that `random_number(0, 100)` "put
between 400 and 600 draws in each of the ten decile buckets" of a `[0, 100]`
range. The two halves of the Definition of Done cannot both hold.

**"1000 draws of `random_number(0, 100)` put between 400 and 600 draws in each
of the ten decile buckets".** 1000 draws over ten buckets is a mean of 100 a
bucket; a band of 400–600 per bucket needs 4000–6000 total draws. The line is
off by a factor of five against its own draw count. The test asserts the
1000-draw version of it: **every decile in 40–160, measured
`[105, 95, 103, 94, 101, 98, 116, 100, 89, 99]`** — a mean of 100.0 against an
expected 100. At 5000 and 20000 draws the same distribution gives
`[503, 482, 522, 498, 495, 504, 490, 486, 494, 526]` and
`[2031, 1973, 1988, 2000, 2047, 2003, 1976, 1968, 2006, 2008]`, i.e. the band
holds at the rate the line's own arithmetic implies. The behaviour the line is
aiming at — a spread rather than a monotonic march — is demonstrably there.
Carried to `FINDINGS.md` as a manifest defect.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings, no `allow` added |
| `cargo test --all-targets` | **1179 passed, 0 failed, 0 ignored**, across 43 targets — the 41 integration test files, the lib's 125 unit tests and the `rb` bin |
| `./rbops/verify.sh phase-037` | **not run — `rbops/verify.sh` does not exist in this checkout** |

All three cargo gates above were re-run on the resume run and printed what is
recorded here; nothing below is carried over unverified.

On the fourth gate: `ls rbops` → `No such file or directory`,
`ls verify.sh` → `No such file or directory`, and `git ls-files | grep verify`
finds nothing either. `rbops/` is absent entirely. I did not create it
(hard rule 1). The three cargo gates above are what I ran and what they
printed; the fourth is **unverified by me**, and the reviewer must run it before
this phase is `.done`.

## The determinism diff

`target/tmp/det.rb`, two separate `rb run` processes, seed 2024:

```
$ ./target/debug/rb run target/tmp/det.rb > target/tmp/r1.txt
$ ./target/debug/rb run target/tmp/det.rb > target/tmp/r2.txt
$ diff target/tmp/r1.txt target/tmp/r2.txt
$ echo $?
0
```

**The diff is empty.** 200 lines of output, exit 0, byte for byte. The program is
50 iterations of `random(0, 1000)`, `random_number(0, 1)`, `random_choice` over
four and `random_shuffle` of the same four.

The same check with **no `random_seed` call at all** also diffs empty (200
lines), which is the `DEFAULT_SEED` property `edge_two_processes_that_never_seed_still_agree`
pins.

## Invariants touched

- **None.** No `Value` variant, no `Error` variant, no grammar change, no `.rb`
  extension change, no `to…end` change, no `set x to` change, no change to what
  `say` prints. All 7 files under `examples/` and both under `modules/` still
  run — checked directly: `pass=9 fail=0`.
- Additive to the language surface: `random_seed(n)` is a new builtin, and
  `random` now takes the arguments it always appeared to take. SPEC.md is
  updated to say so.

### Behaviour changes a program could notice

Four, three from the first attempt and one from this run, listed so the reviewer
can check them rather than find them:

1. `random(...)` answers in its range instead of ignoring its arguments. Any
   program that read `random()` as "some number" still gets a number.
2. `random_choice([])` is refused. It used to answer `nothing`, which a program
   could not tell apart from a choice that had not happened yet.
3. Two programs that both drew without seeding used to differ. They now agree,
   unless they seed differently. This is the point of the phase, but it is a
   change in what a program prints.
4. **This run:** `random` with a fractional bound answers a whole number from
   the range's whole members, and a range holding no whole number is refused.
   `random(0, 100.5)` used to answer `100.5` or `101`; it now answers `0..=100`.
   `random(0.2, 0.8)` used to answer `0.2`; it is now a `Runtime` error.

## Known gaps / follow-ups

- **`./rbops/verify.sh phase-037` was not run** — `rbops/` does not exist in
  this checkout. See above and `FINDINGS.md`.
- Two Definition-of-Done lines are arithmetically unsatisfiable as written. See
  above and `FINDINGS.md`; no test was weakened to accommodate them.
- The byte-identical stage-2 comparison count in
  `tests/bootstrap_selfhost_test.rs` went 8 → 9 because `examples/random.rb` is
  new. The comparison itself passed for all 9 unchanged; only the hardcoded
  total moved. The assertion still requires every `.rb` under `examples/` and
  `modules/` to be compared, and it caught the new file for me.
- The generator is not cryptographically secure, and does not need to be. A
  program that needs unpredictability has no way to ask for it, and a phase that
  added one would be reaching past this one. Carried to `FINDINGS.md`.
- `random_number(min, max)` is half-open and `random(min, max)` is closed. That
  is deliberate and documented in SPEC.md, but the two spellings of the same
  idea do not agree at the high end. Worth a phase of its own; carried to
  `FINDINGS.md`.
- Nothing seeds from the clock any more, so a program that wants genuinely
  unpredictable data has no source. `FINDINGS.md` carries that as work.