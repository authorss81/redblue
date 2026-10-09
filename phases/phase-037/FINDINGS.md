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

## 2. Two Definition-of-Done lines in `rbops/phases.json` are unsatisfiable

Recorded so the auditor can correct the manifest rather than a later implementer
being sent to fail them again. Both are in phase-037's "Definition of done".

**(a) "200 draws of `random_number(0, 100)` produce at most 101 distinct
values".** `random_number` is the fractional draw over `[0, 100)`, about
`100 × 2^53 ≈ 9.0 × 10^17` representable values. The expected number of
collisions in 200 draws is `200 × 199 / 2 / 9.0e17 ≈ 2 × 10^-14`; getting to
101 distinct needs ~99 of them. Measured on the built binary: **200 distinct of
200**.

The 101-member bound is a true statement about `random(0, 100)`, the whole-number
draw, and that one meets it (91 ≤ 101). It cannot be a statement about
`random_number(0, 100)` without discarding the fractional part — which would
contradict the *next* line of the same section, the decile-bucket requirement,
since a whole-number draw over `[0, 100)` cannot occupy ten deciles of width 10
in the way that line describes. The two lines cannot both hold.

**(b) "1000 draws of `random_number(0, 100)` ... put between 400 and 600 draws
in each of the ten decile buckets".** 1000 draws over ten buckets is a mean of
100 a bucket; 400–600 per bucket implies 4000–6000 total draws. The line is off
by 5× against its own stated draw count.

What the line is reaching for — draws spread across the range rather than
marching monotonically across it, which was exactly the old
`now.as_nanos()` defect — is implemented and pinned:
`tests/random_builtin_test.rs::edge_random_number_spreads_its_draws_across_the_range`
asserts every decile of the 1000-draw distribution lies in 40–160, and measures
`[105, 95, 103, 94, 101, 98, 116, 100, 89, 99]` — mean 100.0.

Neither line was made to pass by weakening a test. Both are reported in REPORT.md.

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
