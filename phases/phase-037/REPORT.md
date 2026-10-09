# Phase 037 — Seed the `random` builtins and honour their range arguments

## What changed

| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +197 −19 | A seeded SplitMix64 generator in a thread-local, the `random_seed` builtin, and `random`/`random_number`/`random_choice`/`random_shuffle` redrawn from it with their range arguments honoured |
| `src/stdlib.rs` | +7 −0 | Registers `random` and `random_seed` as reachable builtins |
| `src/repl/completer.rs` | +1 −0 | Completes `random_seed` |
| `tests/random_builtin_test.rs` | +565 | New file: 14 tests |
| `examples/random.rb` | +27 | New file: a seeded example, now deterministic |
| `tests/bytecode_vm_test.rs` | +7 −8 | `examples/random.rb` leaves `NOT_COMPARABLE` — it is deterministic now, so the comparison says something |
| `tests/bootstrap_selfhost_test.rs` | +2 −2 | The stage-2 corpus count 8 → 9, for the new example. The byte-identical comparison itself passed unchanged |
| `SPEC.md` | +20 −2 | Documents the five random builtins and the seeding contract |

### The four arms

Every one took `SystemTime::now()`; none is left with a clock in it. The only
remaining `SystemTime::now()` in `src/runtime.rs` is in `time_now`, which is
supposed to read the clock.

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
$ printf 'say random(1, 1)\nsay random(0, 100)\n' > target/tmp/repro.rb
$ ./target/debug/rb run target/tmp/repro.rb
444
82
```

`random(1, 1)` answered `444`. After: `1`.

## Tests added

All in `tests/random_builtin_test.rs`. Every one seeds before it draws.

| Test | Edge class covered |
|---|---|
| `random_honours_its_range_arguments` | singleton/boundary — `random(b, b) == b` for b in {5, 0, −3, 1e6}; 200 draws of `random(0, 100)` name at most 101 distinct values (was 179) and every one is in `[0, 100]` and integral |
| `edge_two_processes_seeded_alike_print_identical_output` | determinism — two `rb` processes, same seed, byte-identical output over 50 iterations of all four builtins |
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

### Definition of done, measured

- `say random(5, 5)` prints `5`.
- 200 draws of `random(0, 100)` in one program: **89 distinct** (≤ 101). Was 179.
- 1000 draws of `random_number(0, 100)` from seed 12345: every decile bucket in
  [40, 160], asserted on that seed.
- `random_shuffle` is a permutation of its input for every seed 0..=99 and is
  never the identity. `random_choice` over four elements reaches all four in
  1000 draws.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 1118 passed, 0 failed, 0 ignored |
| `./rbops/verify.sh phase-037` | **not run — `rbops/` is not present in this checkout** (see below) |

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
10 iterations of `random(0, 1000)`, `random_number(0, 1)`,
`random_choice` over three, and `random_shuffle` of five:

```
511          0.6055906605277261   b   [5, 1, 4, 3, 2]
661          0.23018223886373812  b   [3, 4, 5, 1, 2]
108          0.281309831218282    a   [2, 3, 1, 5, 4]
...
```

## Invariants touched

- **None.** No `Value` variant, no `Error` variant, no grammar change, no `.rb`
  extension change, no `to…end` change, no `set x to` change, no change to what
  `say` prints. `examples/*.rb` and `modules/*.rb` all still run — the gate's
  example run and the differential corpus both cover them, and one new example
  was added.
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

- **`./rbops/verify.sh phase-037` was not run.** `rbops/` does not exist in this
  checkout — `ls rbops` returns "No such file or directory", and the phases in
  `phases/` carry only `FINDINGS.md` and `REPORT.md`. I did not create it, per
  hard rule 1. The three cargo gates above are what I could run and what I ran;
  the fourth gate is unverified by me and the reviewer should run it.
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
  is deliberate and documented in SPEC.md, but it means the two spellings of the
  same idea do not agree at the high end. Worth a phase of its own if it is
  found confusing.
- Nothing seeds from the clock any more, so a program that wants genuinely
  unpredictable data has no source. `FINDINGS.md` carries that as work.
