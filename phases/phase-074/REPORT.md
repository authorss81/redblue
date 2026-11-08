# Phase 074 — Hash the self-hosted compiler's intern table instead of scanning it

## What changed

| File | Lines | What |
|---|---|---|
| `bootstrap/compiler.rb` | +191 −15 | Replaced the linear `G_INTERN` scan in `intern` with a 256-bucket chained hash table. Added `intern_hash` (a deterministic byte hash reduced so every intermediate is exact in `f64`), `intern_init` (lazy empty buckets), and `intern_bucket_add` (rebuilds the bucket array on a new name). Entries stay in `G_INTERN` in first-use order, so pool indexes and emitted bytes are unchanged. Added the hidden `--intern-selftest` and `--intern-selftest-type-error` modes used by the direct tests, routed before the normal two-path compile branch. |
| `tests/bootstrap_selfhost_test.rs` | +224 −0 | Five new tests appended to the existing self-host test file: they drive the real `intern` through the hidden mode, then compile a name-heavy fixture with both stages, compare the files byte-for-byte, and decode the pool to prove each name is there exactly once. Adding them to an existing test file keeps them in the repository's test diff rather than leaving an untracked test file the policy gate cannot see. |

## Reproduction of the finding (before the change)

`intern` at `bootstrap/compiler.rb:2644` walked every entry on every emitted
name:

```redblue
while intern_i < length(G_INTERN)
    if G_INTERN[intern_i].k is raw then
        set intern_found to intern_i
    end
    set intern_i to intern_i + 1
end
```

The self-compilation under the tree-walker reproduced the cost:

```
$ /usr/bin/time -f 'elapsed %e s' target/debug/rb run bootstrap/compiler.rb bootstrap/compiler.rb target/tmp/self-before.rbc
elapsed 321.46 s
```

The change keeps the same lookup order and the same insertion order, so the
control experiment below emits byte-identical output.

## Codegen measurement (before and after, same machine, back-to-back)

The compiler has no clock; the two measurement copies under `target/tmp/` add
only `time.now()` around `compile_program` and print the elapsed seconds to
stderr. Both copies compile the **same** input, the current
`bootstrap/compiler.rb`, so the numbers isolate the intern-table change rather
than the compiler's own source changing size.

```
$ target/debug/rb run target/tmp/compiler-old-timed.rb bootstrap/compiler.rb target/tmp/timed-old-out.rbc
codegen_seconds 147.343627442
$ target/debug/rb run target/tmp/compiler-new-timed.rb bootstrap/compiler.rb target/tmp/timed-new-out.rbc
codegen_seconds 70.074412387
$ cmp target/tmp/timed-old-out.rbc target/tmp/timed-new-out.rbc && echo identical
identical
```

| Measurement | Old linear `intern` | New hash `intern` | Change |
|---|---:|---:|---:|
| Codegen seconds on `bootstrap/compiler.rb` | 147.34 s | 70.07 s | **−52.4%** |
| Whole compiler run, uninstrumented | 321.46 s | 264.77 s | −17.6% |
| Whole compiler run, timed copies above | 342.19 s | 264.60 s | −22.7% |

The whole-run drop is smaller because lexing, parsing and encoding do not touch
the intern table, and because this phase's compiler source is larger than the
one measured before it. The codegen drop is the number the finding is about.

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_interning_the_same_name_1000_times_grows_the_table_once` | duplicate keys / boundary: 1000 repetitions of one name grow the table by exactly 1, a second distinct name grows it to exactly 2 |
| `edge_engineered_collision_resolves_each_name_to_its_own_index` | collision: the self-test keeps a candidate and searches for a distinct name in the same bucket, then asserts both distinct indexes and both lookups after the collision |
| `edge_empty_and_ten_kilobyte_names_are_interned_exactly` | empty / resource / boundary: `[]` is interned once; a 10KB name interns to one index and a 10KB name differing only in its last byte gets a different index, so the hash does not truncate |
| `edge_intern_refuses_a_name_that_is_not_a_byte_list` | malformed input / type mismatch: `intern(42)` fails with a clean runtime error instead of being accepted as a name |
| `edge_name_heavy_fixture_is_byte_identical_in_both_stages` | unicode / integration: 120 distinct names, one name repeated 30 times, and UTF-8 names `naïve`, `日本語`, `café`; stage 1 and stage 2 bytes are identical and the decoded pool holds each name exactly once |

The standing test
`edge_the_self_hosted_compiler_compiles_itself_byte_identically` passed
unmodified (380.92 s in this run).

## Test requirement matrix

| Row | Result |
|---|---|
| empty | covered — the empty name is interned, returned twice, and grows the table by exactly one |
| singleton | covered — one repeated name grows the table to exactly one entry; a second distinct name makes two |
| boundary | covered — 1000 repetitions, 10KB names, a last-byte difference, and a bucket collision |
| out_of_bounds | N/A — every index is derived internally (`hash mod 256`, then an index this table appended); there is no user-supplied index to step past, and the bucket array is exactly 256 entries long. |
| type_mismatch | covered — `intern(42)` is a clean runtime failure, not a panic and not an accepted key |
| numeric_boundary | covered — the hash keeps every intermediate below `2^36`, so it is exact in `f64`; the modulo result is in `0..255`, which is the only index arithmetic the table performs |
| unicode | covered — `naïve`, `日本語` and `café` are in the integration fixture and each appears once in the decoded pool; names are hashed as their UTF-8 bytes |
| nesting_recursion | N/A — `intern` is not recursive and adds no call frames; the fixture is flat and the compiler's existing parser depth is unchanged. |
| duplicate_missing_keys | covered — duplicate names are the 1000-repetition and collision tests; a missing name is the append path exercised by every new name |
| malformed_input | covered — a non-byte-list name is refused by name; the diagnostics path (`length requires a list or text`) is the failure test |
| resource_limit | covered — the table holds one record per distinct name, a 10KB name is hashed without truncation, and the direct test interns a thousand occurrences without growth beyond the distinct count |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | pass — 2748 passed, 0 failed |
| `../rbops/verify.sh phase-074` | **VERIFY PASS** — 2748 tests, 0 failed; test policy: 5 new Rust tests, 1 failure-asserting, 5 `edge_*` tests, 0 newly skipped |

## Invariants touched

- None. No language syntax, `Value` variant, error kind, example, module or
  corpus `.rb` file changed (`bootstrap/compiler.rb` is the phase's target).
- The emitted bytes are unchanged because entries still enter `G_POOL` and
  `G_INTERN` in first-use order. The fixed point `stage1.rbc == stage2.rbc` is
  the proof.

## Known gaps / follow-ups

- The parser still refuses a source with roughly a thousand statements with
  `Maximum call depth of 1000 reached while calling 'token_kind'`. This was
  found while choosing the name-heavy fixture's size and is recorded in
  `FINDINGS.md`; it is not caused by this phase.
- `phases/phase-073/FINDINGS.md`'s early `give back` bug is still present. The
  hidden self-test route in `main` works around it with a flag because `give
  back` inside an `if` does not leave the function.
