# Phase 037 — Seed the `random` builtins and honour their range arguments

## What changed on this run

The implementation arrived with the resume commit (`8910456 resume phase-037:
adopt preserved work`) and its three cargo gates were green before this run
started. This run audited that implementation for the same thing three earlier
runs had each found — a piece of behaviour with no test behind it — found three
such pieces, and closed them.

| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +9 −3 | Corrected the comment on the seed cast: it claimed the `f64 → i64` cast wraps "rather than saturating", and it saturates. Behaviour unchanged. |
| `tests/random_builtin_test.rs` | +231 | Three new tests: the two spellings of a range nothing ever executed, the `> 2^53` scaling path, and the seed-saturation contract |
| `SPEC.md` | +12 −6 | Documents the call shapes (`random()`, `random(6)`) and the seed bound that the new tests now pin |
| `phases/phase-037/FINDINGS.md` | +95 | §9 (untaken argument-shape branches and the untested scaling path), §10 (the comment that described the opposite of the code) |
| `phases/phase-037/REPORT.md` | rewritten | This file |

`src/runtime.rs` changed by one comment. Every behaviour line in the "What
changed (whole phase)" table below came from the earlier attempts and is
reported, not re-claimed.

## What the audit found

The finding this phase fixes is *a builtin that ignores its arguments*. The fix
therefore has three ways of being given a range — none, one, two — and every
inherited test passed two. Read `src/runtime.rs:608` against the suite:

| Branch | Reached by | Tested before this run |
|---|---|---|
| `(None, None)` → `[0, default_max]` | `random()`, `random_number()` | **no** |
| `(Some(Number(max)), None)` → `[0, n]` | `random(6)`, `random_number(10)` | **no** |
| `(Some(Number(min)), Some(Number(max)))` | everything else | yes, every other test |

`random(6)` — the ordinary spelling of a draw in any language — was never
executed by a test, and the code could have been answering `[0, 100]` for it
without a thing turning red.

The same sweep found `random_int`'s scaling path (`src/runtime.rs:570-574`, taken
only above `2^53` members) unreachable by the entire suite: every range the tests
draw from has at most a thousand members, and the one enormous range they do use,
`-1e308..1e308`, is refused before the branch is reached.

And `src/runtime.rs:680` described the seed cast as wrapping "rather than
saturating". It saturates: `random_seed(1e300)` is the seed
`random_seed(9223372036854775807)` names. Behaviour left alone (refusing an
out-of-range seed is a new error surface this phase was not asked for), comment
corrected, contract pinned by a test.

## The tests can fail — three mutations, each reverted

Each mutation was made in `src/runtime.rs`, the suite run, the result recorded
here, and the mutation reverted. `git diff src/runtime.rs` after all three is the
comment change above and nothing else.

**(a) The scaling path, unrounded and unclamped.** `low + (next_unit() * width)`
instead of `low + (next_unit() * width).floor().min(width - 1.0)`:

```
test edge_random_draws_from_a_range_too_wide_to_count_exactly ... FAILED
panicked at tests/random_builtin_test.rs:999:9:
assertion `left == right` failed: `random` is the whole-number draw, and a range
of 10000000000000000 is wider than 2^53 without making the draw fractional:
random(0, 10000000000000000) answered 3681895156516694.5 for seed 1
  left: 0.5
 right: 0.0

test result: FAILED. 21 passed; 1 failed; 0 ignored; 0 measured
```

Twenty-one of twenty-two tests were green on it. The new test was the only thing
in the suite that could see that branch at all.

**(b) The arguments ignored — the phase's original defect.** `random_range`
answering `(0.0, default_max)` whatever it was handed:

```
test edge_a_range_of_no_width_is_still_a_range ... FAILED
test edge_random_number_spreads_its_draws_across_the_range ... FAILED
test edge_random_reads_its_range_from_every_spelling_of_the_call ... FAILED
test edge_random_draws_from_a_range_too_wide_to_count_exactly ... FAILED
test edge_random_refuses_a_range_no_number_can_measure ... FAILED
test edge_random_refuses_the_arguments_it_cannot_use ... FAILED
test random_honours_its_range_arguments ... FAILED
test edge_many_draws_stay_in_range_and_do_not_grow ... FAILED
test edge_random_answers_a_whole_number_from_a_range_with_a_fractional_end ... FAILED
test result: FAILED. 13 passed; 9 failed; 0 ignored; 0 measured
```

Nine tests fail, including `random_honours_its_range_arguments` (the
`random(5, 5) == 5` line) and the new
`edge_random_reads_its_range_from_every_spelling_of_the_call`.

**(c) The seed cast folded to the magnitude.** `seed.abs().trunc() as u64`:

```
test edge_random_seed_beyond_a_machine_integer_saturates_at_the_bound ... FAILED
panicked at tests/random_builtin_test.rs:866:5:
assertion `left == right` failed: random_seed(1e300) and
random_seed(9223372036854775807) are both past the top of an i64 and should draw
the same sequence, the way a saturating cast makes them
  left: [296.0, 366.0, 899.0, 892.0, 591.0, 963.0, 262.0, 707.0, 689.0, 700.0, 864.0, 968.0, 33.0, 212.0, 370.0, 992.0]
 right: [356.0, 529.0, 323.0, 554.0, 546.0, 721.0, 791.0, 412.0, 549.0, 857.0, 289.0, 346.0, 352.0, 596.0, 243.0, 323.0]
```

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_random_reads_its_range_from_every_spelling_of_the_call` | the no-argument and one-argument spellings, which nothing executed before; `random()` reaches both ends of `[0, 100]`, `random(6)` names all seven members of `[0, 6]`, `random_number()` stays in `[0, 1)`, `random_number(10)` reaches the top; `random(-5)` and `random("a")` are refused (three `eval_err` assertions) |
| `edge_random_draws_from_a_range_too_wide_to_count_exactly` | the `> 2^53` scaling path: 300 draws from `[0, 1e16]` and 100 from `[-1e16, 1e16]`, every answer whole, inside the range, distinct, and reaching both halves |
| `edge_random_seed_beyond_a_machine_integer_saturates_at_the_bound` | numeric boundary — a seed past the ends of an `i64` is accepted, reproducible, equal to the bound it saturates to, different from the other end and from a seed just below the bound |

The nineteen tests inherited with the implementation are listed in the previous
report's table and are unchanged here: 19 tests, 16 of them `edge_*`, and the
same 8 `eval_err` call sites that were asserting clean `Runtime` failures before
this run, in the project's standard idiom for asserting a pipeline produced a
failure.

## What changed (whole phase, cumulative)

| File | Lines | What |
|---|---|---|
| `src/runtime.rs` | +226 −19 | A seeded SplitMix64 generator in a thread-local, the `random_seed` builtin, and `random`/`random_number`/`random_choice`/`random_shuffle` redrawn from it with their range arguments honoured |
| `src/stdlib.rs` | +7 | Registers `random` and `random_seed` as reachable builtins |
| `src/repl/completer.rs` | +1 | Completes `random_seed` |
| `tests/random_builtin_test.rs` | +1060 | 22 tests, 19 of them `edge_*` |
| `examples/random.rb` | +27 | A seeded example, now deterministic |
| `tests/bytecode_vm_test.rs` | +10 −6 | `examples/random.rb` leaves `NOT_COMPARABLE` — it is deterministic now, so the comparison says something |
| `tests/bootstrap_selfhost_test.rs` | +2 −2 | Stage-2 corpus count 8 → 9 |
| `SPEC.md` | +28 −2 | Documents the five random builtins, the seeding contract, the whole-member rule, the call shapes, and the seed bound |

### The four arms

Every one took `SystemTime::now()`; none has a clock left in it. The only
`SystemTime::now()` in `src/runtime.rs` is `src/runtime.rs:815`, in `time_now`,
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

## Definition of done — every line measured on this run

Measured against the binary built from this tree, not read off the test suite,
and each against **this run's brief**. The manifest was corrected after an
earlier report was written: the 101-distinct bound attaches to `random()` only,
`random_number` carries the non-monotonicity property, and the bucket band reads
40–160. All of those are met.

- [x] **`say random(5, 5)` prints exactly `5`** — printed `5`. `random(1, 1)`
      prints `1`; it printed `403` before.

  ```
  $ ./target/debug/rb run target/tmp/dod/dod1.rb   # say random(5,5) / say random(1,1)
  5
  1
  ```

- [x] **200 draws of `random(0, 100)` name at most 101 distinct values** — **85**
      distinct, 0 fractional, 0 outside `[0, 100]`. Was 179.

  ```
  distinct: 85    outside: 0    fractional: 0
  ```

- [x] **200 draws of `random_number(0, 100)` produce 200 distinct values, and are
      neither monotonic nor a fixed arithmetic sequence** — **200 distinct**, 0
      outside `[0, 100)`, and the consecutive steps split **104 rises / 95
      falls**.

  ```
  1.582909387275222   66.39942340488506  16.920437987610846 52.241421811607566
  1.2837748982554853  7.712950416341902  56.5279726411436   31.01882998970825
  ```

  Before, eight consecutive draws were `4.8086, 5.8041, 6.0445, 6.2037, 6.36,
  6.5312, 6.6935, 6.8537` — strictly increasing.

- [x] **1000 draws of `random_number(0, 100)` from the pinned seed put 40–160 in
      each of the ten decile buckets** — `[105, 95, 103, 94, 101, 98, 116, 100,
      89, 99]` at seed 12345, mean 100.0, none outside the band. Pinned by
      `edge_random_number_spreads_its_draws_across_the_range`.

- [x] **`random_shuffle` is a permutation of its input for every seed 0..=99 and
      is not the identity for any of them** — `non_permutations=0 identity=0`,
      checked in 100 separate `rb` processes.

- [x] **`random_choice` over a four-element list hits all four in 1000 draws** —
      `255 / 246 / 253 / 246`.

- [x] **Two separate processes with an explicit seed print byte-identical output**
      — `diff` empty, both 1842 bytes, SHA-256 `53e018d03a553f9f276584357b70b01…`.
      Program: `random_seed(987654321)` then 50 iterations of all four builtins.
      The same program with the seeding line removed is also byte-identical
      between two processes, which is the `DEFAULT_SEED` claim.

  ```
  $ ./target/debug/rb run target/tmp/dod/dod4.rb > a.txt
  $ ./target/debug/rb run target/tmp/dod/dod4.rb > b.txt
  $ diff a.txt b.txt ; echo $?
  0
  ```

- [x] **edge_\* tests cover an empty list, a single-element list, min greater than
      max, a negative range, and a non-number argument** — all five in
      `edge_random_refuses_the_arguments_it_cannot_use`, plus three more refusals
      in `edge_random_seed_refuses_a_seed_it_cannot_use` and three in the new
      `edge_random_reads_its_range_from_every_spelling_of_the_call`.

- [x] **No `SystemTime` or `Instant` remains in the `random`, `random_number`,
      `random_choice` or `random_shuffle` arms; clippy clean with no
      `allow(dead_code)`** — the only `SystemTime::now()` in `src/runtime.rs` is
      `src/runtime.rs:815`, in `time_now`. `grep -c "allow(dead_code)"
      src/runtime.rs` → 0; clippy is clean at zero warnings with `-D warnings`.

- [x] **Every `.rb` file under `examples/` and `modules/` still runs** — 9 files,
      `pass=9 fail=0`.

- [x] **2000 shuffles of an eight-element list reach more permutations than a
      one-draw shuffle can** (the property the last run added, re-measured here)
      — **1969** distinct, against the `lcm(2..=8)` = 840 ceiling. 500 shuffles of
      a six-element list reach **359**, against 60.

## Edge-case matrix

| Row | Result |
|---|---|
| empty | covered — `random_choice([])` refused, `random_shuffle([])` is `[]`, `random(5,5)` and `random_number(1,1)`, `random(0.2, 0.8)` (no whole member) |
| singleton | covered — one-element list for `random_choice` (4 seeds) and `random_shuffle`; `random(b,b)` for 4 bounds; `random(5, 5.5)`'s single member |
| boundary | covered — both members of `[0,1]` reachable over 65 seeds; both ends of the default `[0, 100]`; zero-width ranges; fractional ends over 3000 seeds each |
| out_of_bounds | covered — 200 draws of `random(0, 100)` stay in `[0, 100]`; 3000 draws of each fractional range and 400 draws of each `1e16`-wide range stay inside it; a reversed range is refused, including the one-argument spelling `random(-5)` |
| type_mismatch | covered — 8 non-number arguments across all five builtins, each a clean `Runtime` error |
| numeric_boundary | covered — `1e400` seed, `-1e308..1e308` for both range builtins, ranges wider than `2^53` on both sides of zero, the saturation of a seed past the ends of an `i64`, and whole-number answers from fractional bounds |
| unicode | **N/A** — the draws are numbers. The list a draw selects from is held as a `Value`, and text handling is a lexer/value concern covered by `lexer_robustness_test.rs` and `comparison_lex_test.rs`. Nothing this phase changed touches text; text values do travel through the choice and shuffle paths unaltered (`random_choice(["only"])` and a `"only"`-shuffled list are asserted). |
| nesting_recursion | covered — nested list members chosen and shuffled whole |
| duplicate_missing_keys | covered — `[1, 1, 2, 2, 2]` keeps its multiplicity after a shuffle. A *missing key* is N/A: no builtin here reads a record |
| malformed_input | covered — five malformed `random_seed` calls and eight non-number arguments, every one a `Runtime` error not a panic |
| resource_limit | covered — 20 000 draws stay in range and evenly spread; the generator is one `u64`, allocates nothing, and no existing guard was raised |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | **pass** (exit 0, no diff) |
| `cargo clippy --all-targets -- -D warnings` | **pass** (exit 0, zero warnings, no `allow` added) |
| `cargo test --all-targets` | **1223 passed, 0 failed, 0 ignored** |
| `cargo test` (incl. doc-tests) | **pass** (2 doc-tests passed, 0 failed) |
| `./rbops/verify.sh phase-037` | **not run — `rbops/verify.sh` does not exist in this checkout** |

All four cargo commands were run on this run, before and after the change.

**On the fourth gate.** `ls rbops` → `No such file or directory`.
`./rbops/verify.sh phase-037` → `bash: ./rbops/verify.sh: No such file or
directory`. `git ls-files | grep -c rbops` → `0`: no tracked file under `rbops/`
exists in this checkout at any commit. I did not create it — hard rule 1 forbids
touching `rbops/`. The fourth gate is **unverified**, and a reviewer must run it
before this phase is `.done`. `FINDINGS.md` §1 carries the same fact.

The failure-assertion check it performs is satisfied in the `eval_err` idiom
(`let e = eval_err("random_choice([])"); assert!(matches!(e, Error::Runtime(_, _)))`)
at 9 call sites in `tests/random_builtin_test.rs`, which is the project's
standard idiom for asserting that a pipeline produced a failure.

## Invariants touched

None. No entry in `phases/INVARIANTS.md` was opened, `.rb` is still the
extension, `Value`'s variants and `Error`'s variants are unchanged, and no
existing test was weakened, deleted or re-scoped.

## Known gaps / follow-ups

All in `FINDINGS.md`, none fixed here (hard rule 7 — one phase, one concern):

- **§3** — a seeded generator is not an unpredictable one. Redblue now has no
  source of unpredictable data at all. A capability gap worth its own phase.
- **§4** — `random` is inclusive at both ends, `random_number` at the low end
  only. Both are individually defensible and both are pinned by existing tests
  (`tests/numeric_edge_test.rs` depends on the half-open reading), so reconciling
  them is a language decision, not a bug fix.
- **§5** — the bytecode compiler still refuses function literals, so no
  seeded-draw test can compare the two engines through `map`.
- **§9** — for a phase that fixes "the arguments were ignored", audit every shape
  the argument list can take. The defect hid in the untaken arms of the argument
  parser, where a suite that only ever writes two arguments cannot see it.
- **§10** — a comment that claims what code does needs a test beside it, or it
  should claim less. This one said "wraps rather than saturating" and survived
  two review passes because a comment cannot fail a test.
