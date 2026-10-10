# phase-037 — FINDINGS

Work this phase produced, and what it found next.

## 1. `rbops/` is not present in the checkout

`ls rbops` → `No such file or directory`. `rbops/verify.sh`, `rbops/phases.json`
and `rbops/dispatch.sh` are all absent, so `./rbops/verify.sh phase-037` — the
fourth gate in AGENTS.md section 3.4 — could not be run.

I did not create it (hard rule 1: do not touch `rbops/`). The three cargo gates
were run and are recorded in REPORT.md with what they actually printed. The
fourth gate is **unverified**; a reviewer must run it before this phase is
`.done`.

Re-checked on the resume run, from a clean `git ls-files`: `git ls-files |
grep verify` returns nothing and `ls verify.sh` at the root returns
`No such file or directory`. The directory has not appeared since.

Re-checked again on this run: `ls rbops` → `No such file or directory`,
`git ls-files | grep -c rbops` → **0**. No tracked file under `rbops/` exists in
this checkout at any commit. The fourth gate remains unrunnable here.

## 2. RESOLVED — the manifest was corrected, do not resend this complaint

An earlier run recorded (here) that two Definition-of-Done lines were
arithmetically unsatisfiable: the 101-distinct bound was attached to
`random_number`, and the decile band read 400–600 against a 1000-draw count.

**The manifest has since been corrected and the complaint is withdrawn.** The
101-distinct bound now attaches to `random()` only — the whole-number draw, which
has 101 members — `random_number` carries the non-monotonicity property it can
actually have, and the band reads 40–160.

Re-measured against the corrected lines on this run:

| Line | Required | Measured |
|---|---|---|
| 200 draws of `random(0, 100)`, distinct | ≤ 101 | **91** at seed 12345, **85** unseeded |
| 200 draws of `random_number(0, 100)`, distinct | 200 | **200** |
| consecutive differences, 200 draws | alternate sign | 99 rises, 100 falls |
| deciles of 1000 draws of `random_number(0, 100)` | 40–160 each | `[105, 95, 103, 94, 101, 98, 116, 100, 89, 99]` |

Every line is met, and the last two are now asserted by
`edge_random_number_draws_are_not_a_march_and_not_an_arithmetic_sequence`
(written this run) rather than merely measured — see §8. **An implementer who finds this complaint in a resumed context
should re-read it against their own current PROMPT.md before acting on it** — this
run's predecessor spent an attempt satisfying lines that no longer existed.

## 3. A seeded generator is not an unpredictable one

Phase 037 replaced the wall clock in the `random*` builtins with a seeded
SplitMix64, so draws are reproducible — which is what the bootstrap fixed point
needs. But a *seeded* generator is deterministic by construction, so a Redblue
program now has no source of unpredictable data at all.

This is a real capability gap, not a defect: the previous behaviour was
unpredictable and also unreproducible, and this phase was asked for the second
without asking about the first. `src/runtime.rs` has no path to a
cryptographically secure source, and adding one is a language-surface decision
(a new builtin, plus a documented guarantee about what it does and does not
promise). Out of scope here; worth its own phase.

## 4. `random` and `random_number` disagree at the high end

`random(min, max)` is inclusive at both ends. `random_number(min, max)` is
inclusive at the low end and exclusive at the high one. Both readings are
individually defensible — the closed one is what makes `random(5, 5)` answer
`5`, which the phase's Definition of Done requires, and the half-open one is
what `random_number` has always meant — but a reader meeting both spellings will
reasonably expect them to agree.

Not changed here: the phase pinned `random(5, 5) == 5` and preserving
`random_number`'s existing range are both explicit requirements, so reconciling
them is a language decision rather than a bug fix. `tests/numeric_edge_test.rs`
depends on the half-open reading (`random_number(1, 2)` in `1.0..2.0`), so
changing it would touch an existing test.

## 5. The bytecode compiler still refuses function literals

Found while writing `both_engines_agree_on_a_seeded_sequence`: `redblue::compile_source`
rejects `to (x) ... end` with *"`to (x) ... end` as an expression is not compiled
yet; run programs that use one with `rb run`"*.

Not this phase's work, and the test works around it by not using one. It does
mean no seeded-draw test can compare the two engines through `map`, and
`examples/random.rb` cannot either.

## 6. A resumed phase is not a finished phase

Worth recording as process, because the phase was previously reported complete
and its gate claims were taken at face value — and it was not complete.

The seeded generator was in place and the suite was green, but `random_int` had
a hole the existing tests could not see: when a bound was a fraction it scaled
the raw width instead of counting the range's members.

```redblue
random(0, 100.5)   // answered 100.5 at seed 581, and 101 — outside the range
random(0.2, 0.8)   // answered 0.2, a range holding no whole number at all
```

26 of 3000 seeds produced a fraction. `random` is the whole-number draw —
`random_honours_its_range_arguments` asserts `value.fract() == 0.0` on every one
of 200 draws — so the code contradicted its own passing test. The test passed
because it only ever drew from integer bounds, where the buggy branch is not
taken.

Fixed in `src/runtime.rs`: the members of a range are its whole numbers,
`ceil(min)..=floor(max)`, and a range with none is refused by name. Pinned by
`edge_random_answers_a_whole_number_from_a_range_with_a_fractional_end`, which sweeps 3000
seeds across four fractional-ended ranges and asserts every answer is whole,
inside the range, and that every whole member is reachable.

The lesson for the auditor: a phase's own green suite is evidence about the
assertions that exist, not about the branches that do not. The previous report
was careful about arithmetic it could check and did not notice a branch it did
not have a test for. Coverage of the *edge* rows in AGENTS.md 3.2 is what finds
this, and it only finds it if the edge is drawn somewhere the happy path does
not reach.

## 7. The shuffle test passed on the exact defect the phase names

The same lesson, a second time, in the same phase — found on this run by mutating
`src/runtime.rs` rather than by reading it.

`edge_random_shuffle_permutes_its_input_for_every_seed` sweeps seeds 0..=99 and
asserts each answer is a permutation of its input and is not the identity. Those
are the two properties a shuffle must have. **The original defect passes both.**

Reinstating the old shuffle — one draw reused as `seed % (i + 1)` for every swap —
left all 17 tests green:

```
test result: ok. 17 passed; 0 failed
```

A single draw reused across the loop still yields a permutation (swaps are
permutations, and they compose to one) and is still rarely the identity (with 840
reachable outcomes, the chance of landing on the identity is about 1/840). So both
assertions were satisfiable by the bug they were written for.

What the defect cannot do is *reach*. With one draw reused, every `j` is
`once % (i + 1)`, so the whole permutation is determined by
`once % lcm(2..8)` = **840** of the `8!` = 40320 permutations. A Fisher-Yates that
draws per step has no such ceiling. Measured on seed 12345:

| | Real Fisher-Yates | One-draw defect | Ceiling |
|---|---|---|---|
| 2000 shuffles, 8 elements | **1969** distinct | 770 | 840 |
| 500 shuffles, 6 elements | **359** distinct | — | 60 |

New test `edge_random_shuffle_reaches_far_more_permutations_than_it_can` asserts
each count exceeds its ceiling. Under the mutation it fails by name:

```
2000 shuffles of an eight-element list reached only 770 distinct
permutations. Reusing one draw for every step caps the reachable set at
lcm(2..=8) = 840, so this is the `seed % (i + 1)` computed once and reused
for every swap — the exact shape of the original defect
```

The threshold is deliberately loose — 840 against a measured 1969, 60 against a
measured 359 — so it survives a change of mixer without being retuned, and it
does not name a particular permutation or compare against a golden list, which
would encode the generator's internals.

The first attempt at this test was weaker and I have written that down rather
than quietly replacing it: it asserted that 40 consecutive shuffles differ from
one another. That passes under the mutation too, because reusing the draw across
*steps* still leaves each *call* with a fresh one. A variation check is not a
reach check.

**Generalisable for the auditor:** for any randomised operation, "the output is
well-formed" and "the output varies" are both satisfiable by a one-value
generator. What distinguishes a real draw is the size of the set it can reach.
Any future phase that seeds a generator should pin reach, not variance.

## 8. A measured property in a report is not a pinned property in a test

The third instance of the §6/§7 lesson, and the reason the phase was not finished
even though everything was green.

The corrected Definition of Done adds a property for `random_number` that the
inherited suite did not assert anywhere: *consecutive draws are not monotonic and
are not a fixed arithmetic sequence*. The inherited REPORT quoted measurements for
it — "200 draws, 200 distinct, 99 rises / 100 falls" — but those were numbers the
run had computed, not assertions the suite made. Nothing in
`tests/random_builtin_test.rs` could fail on the property. The only neighbouring
test, `edge_random_number_spreads_its_draws_across_the_range`, measures the spread
of 1000 draws into ten deciles, which is a different question from the shape of
consecutive draws.

A test whose absence is invisible from the outside is worse than no test, because
the report says the property holds and a reviewer checking report-against-diff
sees a number, not a hole.

Closed by `edge_random_number_draws_are_not_a_march_and_not_an_arithmetic_sequence`
(tests/random_builtin_test.rs:266), which asserts 200 distinct draws, that
consecutive steps include both a rise and a fall, and that the 199 consecutive
differences take at least 100 distinct values. Proven failable twice by mutation
(`FINDINGS.md` §3 of this report's mutations, in REPORT.md): against a monotonic
counter it fails naming the marching symptom, and against a coarse counter it
fails naming the arithmetic-sequence symptom.

**Generalisable for the auditor:** a Definition-of-Done line must map to a named
test that fails when the line is violated. Where a report quotes a measurement
with no test behind it, that line is unverified however green the suite is.
