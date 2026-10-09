# Phase 037 — Seed the `random` builtins and honour their range arguments

## What changed

| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +216 −19 | A seeded SplitMix64 generator in a thread-local, the `random_seed` builtin, and `random`/`random_number`/`random_choice`/`random_shuffle` redrawn from it with their range arguments honoured |
| `src/stdlib.rs` | +7 −0 | Registers `random` and `random_seed` as reachable builtins |
| `src/repl/completer.rs` | +1 −0 | Completes `random_seed` |
| `tests/random_builtin_test.rs` | +615 | New file: 16 tests |
| `examples/random.rb` | +27 | New file: a seeded example, now deterministic |
| `tests/bytecode_vm_test.rs` | +7 −8 | `examples/random.rb` leaves `NOT_COMPARABLE` — it is deterministic now, so the comparison says something |
| `tests/bootstrap_selfhost_test.rs` | +2 −2 | The stage-2 corpus count 8 → 9, for the new example. The byte-identical comparison itself passed unchanged |
| `SPEC.md` | +20 −2 | Documents the five random builtins and the seeding contract |

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
| `edge_two_processes_that_never_seed_still_agree` | determinism — the same, with **no** `random_seed` call at all. Pins the `DEFAULT_SEED` claim in `src/runtime.rs` and SPEC.md, which no other test covered |
| `edge_seeding_moves_the_generator` | determinism — the unseeded sequence, seed 424242 and seed 424243 are three different sequences, so `random_seed` reaches the generator rather than being a no-op |
| `random_draws_are_reproducible_from_a_seed` | determinism — same seed, same 64-draw sequence, in one process |
| `edge_random_number_spreads_its_draws_across_the_range` | distribution — 1000 draws of `random_number(0, 100)` put 40–160 in each of ten decile buckets. The old source was monotonic, so all 1000 landed in the first decile |
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

I mutated `src/runtime.rs` so the generator reads `SystemTime::now()` on first
use per thread instead of `DEFAULT_SEED`, and re-ran the suite:

```
test edge_two_processes_that_never_seed_still_agree ... FAILED
test edge_a_range_of_no_width_is_still_a_range ... FAILED
test result: FAILED. 14 passed; 2 failed
  panicked: two processes that never seeded printed different things, so the
            default seed is not fixed
```

The mutation was then reverted; `git diff --stat src/runtime.rs` is empty, so
the shipped file is byte-identical to the committed one.

### Edge-case matrix

| Row | Result |
|---|---|
| empty | covered — `random_choice([])` is refused, `random_shuffle([])` is `[]`, `random(5,5)` and `random_number(1,1)` |
| singleton | covered — one-element list for `random_choice` (4 seeds) and `random_shuffle`; `random(b,b)` for 4 bounds |
| boundary | covered — both members of `[0,1]` reachable over 65 seeds; zero-width ranges |
| out_of_bounds | covered — 200 draws of `random(0, 100)` stay in `[0, 100]`; a reversed range is refused |
| type_mismatch | covered — 8 non-number arguments across all five builtins, each a clean `Runtime` error |
| numeric_boundary | covered — `1e400` seed, `-1e308..1e308` for both range builtins, exactness at `2^53` |
| unicode | N/A — the draws are numbers; the list a draw selects from is held as `Value`, and unicode is a lexer/value concern covered by `lexer_robustness_test.rs` and `comparison_lex_test.rs`. Nothing here was changed to affect text handling |
| nesting_recursion | covered — nested list members chosen and shuffled whole |
| duplicate_missing_keys | covered — a list with repeated members keeps its multiplicity after a shuffle. A missing key is N/A: no builtin here reads a record |
| malformed_input | covered — five malformed `random_seed` calls; every one a `Runtime` error, not a panic |
| resource_limit | covered — 20 000 draws stay in range and evenly spread; the generator is one `u64` and allocates nothing, and no existing guard was raised |

## Definition of done, measured

Every item below was measured against the built binary, not read off the test
suite.

- `say random(5, 5)` prints `5`. **Met** — printed `5`.
- 200 draws of `random(0, 100)` in one program: **91 distinct**, all integral,
  min 0, max 100. Was 179. **Met.**
- `random_shuffle` is a permutation of its input for every seed 0..=99 and is
  never the identity. **Met** — checked in 100 separate `rb` processes:
  `non-permutations=0 identity=0`.
- `random_choice` over four elements reaches all four in 1000 draws. **Met** —
  266/245/266/223.
- Two separate processes with an explicit seed print byte-identical output.
  **Met** — the diff below is empty.

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
expected 100. The behaviour the line is aiming at, a spread rather than a
monotonic march, is demonstrably there. Carried to `FINDINGS.md` as a manifest
defect.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass, zero warnings, no `allow` added |
| `cargo test --all-targets` | **1120 passed, 0 failed, 0 ignored** |
| `./rbops/verify.sh phase-037` | **not run — `rbops/verify.sh` does not exist in this checkout** |

On the fourth gate: `ls rbops/verify.sh` → `No such file or directory`, and
`find . -name verify.sh` outside `target/` finds nothing. `rbops/` is absent
entirely. I did not create it (hard rule 1). The three cargo gates above are
what I ran and what they printed; the fourth is **unverified by me**, and the
reviewer must run it before this phase is `.done`.

## The determinism diff

`target/tmp/det.rb`, two separate `rb run` processes, seed 2024:

```
$ ./target/debug/rb run target/tmp/det.rb > target/tmp/run1.txt
$ ./target/debug/rb run target/tmp/det.rb > target/tmp/run2.txt
$ diff target/tmp/run1.txt target/tmp/run2.txt
$ echo $?
0
```

**The diff is empty.** 40 lines of output, exit 0, byte for byte. The output is
10 iterations of `random(0, 1000)`, `random_number(0, 1)`, `random_choice` over
three, and `random_shuffle` of five:

```
511
0.6055906605277261
2
[5, 1, 4, 3, 2]
...
```

The same check with **no `random_seed` call at all** also diffs empty, which is
the `DEFAULT_SEED` property `edge_two_processes_that_never_seed_still_agree`
now pins.

## Invariants touched

- **None.** No `Value` variant, no `Error` variant, no grammar change, no `.rb`
  extension change, no `to…end` change, no `set x to` change, no change to what
  `say` prints. All 9 files under `examples/` and `modules/` still run — checked
  directly: `pass=9 fail=0`.
- Additive to the language surface: `random_seed(n)` is a new builtin, and
  `random` now takes the arguments it always appeared to take. SPEC.md is
  updated to say so.

### Behaviour changes a program could notice

Three, all previously-broken calls, listed so the reviewer can check them rather
than find them:

1. `random(...)` answers in its range instead of ignoring its arguments. Any
   program that read `random()` as "some number" still gets a number.
2. `random_choice([])` is refused. It used to answer `nothing`, which a program
   could not tell apart from a choice that had not happened yet.
3. Two programs that both drew without seeding used to differ. They now agree,
   unless they seed differently. This is the point of the phase, but it is a
   change in what a program prints.

## Known gaps / follow-ups

- **`./rbops/verify.sh phase-037` was not run** — `rbops/` does not exist in
  this checkout. See above and `FINDINGS.md`.
- Two Definition-of-Done lines are arithmetically unsatisfiable as written. See
  above and `FINDINGS.md`; I did not weaken any test to accommodate them.
- The byte-identical stage-2 comparison count in
  `tests/bootstrap_selfhost_test.rs` went 8 → 9 because `examples/random.rb` is
  new. The comparison itself passed for all 9 unchanged; only the hardcoded
  total moved. This is not a loosened threshold — the assertion still requires
  every `.rb` under `examples/` and `modules/` to be compared, and it caught the
  new file for me.
- The generator is not cryptographically secure, and does not need to be. A
  program that needs unpredictability has no way to ask for it, and a phase that
  added one would be reaching past this one.
- `random_number(min, max)` is half-open and `random(min, max)` is closed. That
  is deliberate and documented in SPEC.md, but the two spellings of the same
  idea do not agree at the high end. Worth a phase of its own.
- Nothing seeds from the clock any more, so a program that wants genuinely
  unpredictable data has no source. `FINDINGS.md` carries that as work.