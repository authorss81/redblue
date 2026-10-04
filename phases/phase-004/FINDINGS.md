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
- `cargo test --all-targets` — 54 passed, 0 failed, 0 ignored

`phases/phase-004/` was created solely to hold this phase's `REPORT.md` and this
file, as required by AGENTS.md section 4. It contains no pipeline code.

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