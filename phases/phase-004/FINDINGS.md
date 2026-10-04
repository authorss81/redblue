# Phase 004 — Findings

Out-of-scope items discovered while working this phase. Recorded for the auditor;
none were fixed here.

## 1. `rbops/verify.sh` and `phases/` are missing from the checkout

The phase prompt requires running four gates, the fourth being
`./rbops/verify.sh phase-004`. In this working tree there is no `rbops/`
directory, no `phases/` directory, and no `.opencode/agent/`. Only
`.github/` exists. The gate could not be run, and per hard rule 1 nothing under
`rbops/` may be authored or edited to work around that.

The three gates that do exist were run and are green:

- `cargo fmt --all -- --check` — clean
- `cargo clippy --all-targets -- -D warnings` — 0 warnings
- `cargo test --all-targets` — 92 passed, 0 failed, 0 ignored

A `find . -name "verify*"` over the checkout (excluding `target/` and `.git/`)
returns nothing. `.github/workflows/ci.yml` runs `cargo build`, `cargo test` and
`cargo clippy -- -D warnings` — i.e. CI enforces the first three gates and has
no equivalent of `verify.sh`.

`phases/phase-004/` holds only this phase's `REPORT.md` and this file, as
required by AGENTS.md section 4. It contains no pipeline code.

Suggested phase: audit why `rbops/` is absent from the dispatched checkout, or
amend the phase prompt so the gate list matches what agents actually receive.

## 2. `SPEC.md:107` documents a record form the parser rejects

`SPEC.md:107` (and `SPEC.md:148`) show:

```redblue
set person to record {
    name: "Alice",
    age: 30
}
```

`src/parser.rs:1244` only accepts a bare `{…}` literal in expression position;
the `record` keyword form fails with `ParserError: Expected field name`.

Reproduce: `printf 'set p to record {\n    name: "Alice"\n}\nsay p\n' > /tmp/r.rb && ./target/debug/rb run /tmp/r.rb`

Pre-existing on `4ec2dd9` (verified by running the pre-phase build), untouched by
this phase, and it is a spec-vs-parser drift question rather than an ordering one
— so it needs its own phase with an explicit decision on which side moves.

## 3. VM scopes are still `HashMap` (latent, not observable)

`src/vm.rs:11` (`globals`) and `src/vm.rs:12` (`locals`) remain
`HashMap<String, Value>`. Grepped for iteration into output — there is none
today, so no current behaviour depends on their order. They are the same hazard
this phase just closed for records, waiting for the first feature that prints a
scope. Left alone deliberately: replacing them is not required for deterministic
record ordering and would widen this diff.

## 4. `assert_type` in `src/testing/runner.rs:285` has no callers

`assert_type` is `pub` and never invoked anywhere in `src/` or `tests/`. It had
to be edited by this phase (`TypeId::of::<HashMap<String, Value>>()` no longer
type-matched the new storage), but nothing exercises it, so its `TypeId` mapping
is unverified. Worth either wiring up or removing.

## 5. List index out of range reads as `nothing`; negative indices wrap

`src/vm.rs` bounds-checks list access leniently rather than erroring. Measured
against the current binary:

| Expression on `l = [1,2,3]` | Result |
|---|---|
| `l[0]` / `l[2]` | `1` / `3` |
| `l[3]` (== len) | `nothing` |
| `l[999]` | `nothing` |
| `l[-1]` / `l[-3]` | `3` / `1` — negative indices count from the end |
| `l[-4]` / `l[-99]` | `nothing` |

No panic and no UB in any of these, so there is no safety defect. But two things
are worth deciding in their own phase: an out-of-range positive index is silent
where AGENTS.md §3.2 asks for "a clean runtime error", and a negative index is
*not* rejected at all — a program that meant `l[-1]` as "off the front" silently
reads the last element.

Pre-existing, unrelated to record field ordering. Records are unaffected:
`r[0]` on a record is `RuntimeError: Cannot index non-list`, now asserted by
`test_record_is_not_indexable_at_any_position`.

## 6. Two dead assertion helpers, one of them inverted

- `assert_value_is_record` (`src/testing/assertions.rs:174`) is `pub` with **no
  callers** anywhere in `src/` or `tests/`. Its `Value::Record(_) => Ok(())` arm
  is correct for a type check — it does *not* make `to contain` vacuous, contrary
  to my first note on this file.
- The trait default `Assertion::contains` (`src/testing/assertions.rs:100`) is
  **inverted**: it returns `Err` when `self.actual == *item` and `Ok` otherwise,
  i.e. it fails exactly when the value is present. It also takes `T: PartialEq`
  rather than a container, so `contains(1)` on a list can never succeed. It has no
  callers — the live `expect … to contain` path is `runner.rs:240 assert_contains(&str, &str)`
  for text.

Dead today, but an inverted assertion helper is a trap for the next caller.
Worth deleting or fixing; untouched by this phase.