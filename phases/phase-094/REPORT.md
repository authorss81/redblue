# Phase 094 — Write down what Redblue libraries should be written in Redblue, bound, or not built

## Reproduction of the finding

The original finding is an absence: no file said which capabilities belong in
Redblue, which are Rust bindings, and which should not exist. On this phase's
base commit the previous attempt had already landed `LIBRARIES.md`, so the
file-missing form no longer reproduces:

```
$ test -f LIBRARIES.md; echo "exit=$?"
exit=0
```

Re-reading the policy against the phase's definition of done found two clauses
still not met, and those are what this run fixes:

```
$ grep -in "cannot out-run a native library" LIBRARIES.md
(no match)

$ grep -in "data structures" LIBRARIES.md
(no match)
```

The document said "slower than Python" but never stated the stronger, honest
limit that interpreted Redblue cannot out-run a native library at all. It also
listed collections but not the data-structures class the phase names (stacks,
queues, sets, trees, graphs, priority queues). Both are real omissions in a
policy whose job is to decide those questions once.

The new tests were written first. `the_native_speed_limit_is_stated_without_a_promise`
failed at `LIBRARIES.md never says interpreted Redblue cannot out-run a native
library` and `edge_data_structures_are_written_in_redblue` failed on the missing
class; after the two edits both pass, and the existing refusal test still proves
the binding-required gaps are open.

## What changed

| File | Lines | What |
|---|---|---|
| `LIBRARIES.md` | +2 −1 | states that interpreted Redblue cannot out-run a native library; adds the data-structures class (stacks, queues, sets, trees, graphs, priority queues) to the written-in-Redblue section with its reason |
| `AGENTS.md` | +6 −0 | puts the three-way decision and the "cannot out-run a native library" limit in the Standard Library section, next to the existing `LIBRARIES.md` link |
| `tests/libraries_policy_test.rs` | +70 −0 | three tests holding the new policy clauses: data structures are decided, the speed limit is stated without a promise, and a binding-required directory call is still a clean refusal |

`must_touch` is satisfied by the changed `AGENTS.md`. The policy itself remains
the root `LIBRARIES.md` the phase asked for. No file under `src/`, `examples/`,
`modules/`, `corpus/`, `.github/`, `rbops/` or `.opencode/agent/` was touched.

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_data_structures_are_written_in_redblue` | **edge_** / missing class — the written-in-Redblue section must name data structures and give a reason, not just a list |
| `the_native_speed_limit_is_stated_without_a_promise` | boundary / missing claim — "cannot out-run a native library" must be present and the three speed promises must be absent |
| `edge_binding_required_directory_gap_is_a_clean_refusal` | **edge_** / asserts failure — `files.glob` is still refused cleanly, naming both module and member, so the binding entry describes an open gap |

No `#[ignore]`, no `// skip`, no `allow(clippy::` suppression, and no existing
test was changed or weakened.

## Edge-case matrix

| Row | Verdict |
|---|---|
| empty | covered — `each_section_states_a_reason_not_just_a_list` refuses an empty or purely nominal section body (under 200 bytes). |
| singleton | covered — `edge_every_ranked_gap_is_decided_in_exactly_one_section` requires exactly one section per rank; zero is an omission and two is a contradiction. |
| boundary | covered — the rank loop spans 1 and 10, and the new speed test rejects the three promise phrases at the boundary between honest limitation and marketing. |
| out_of_bounds | N/A — this phase adds no indexing or VM path; the only runtime call is the existing library refusal re-checked by `edge_binding_required_directory_gap_is_a_clean_refusal`. |
| type_mismatch | N/A — no runtime types or coercions were changed; the document phase does not evaluate Redblue values. |
| numeric_boundary | N/A — no arithmetic was added or changed; the document contains no computed numeric output. |
| unicode | N/A — no text handling was changed; the Markdown is read as UTF-8 and contains no program output. |
| nesting_recursion | N/A — the policy is flat Markdown; the tests slice sections, they do not execute nested code. |
| duplicate_missing_keys | covered — duplicate rank markers and missing rank markers both fail `edge_every_ranked_gap_is_decided_in_exactly_one_section`; a missing class fails `edge_data_structures_are_written_in_redblue`. |
| malformed_input | covered — a missing section, a missing heading and a missing file each panic with the document and heading named; the refusal test also covers the clean-error channel. |
| resource_limit | N/A — the tests read a small Markdown file and loop ten times; there is no unbounded work, network or filesystem outside the checkout. |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — exit 0, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — exit 0, no warnings |
| `cargo test --all-targets` | pass — exit 0, **2217 passed, 0 failed, 0 ignored** |
| `../rbops/verify.sh phase-094` | **pass** — exit 0, `VERIFY PASS — phase-094`; report sections, diff hygiene, must_touch, fmt, clippy, 2217 tests, example sweep, failure assertion (1), test quota (3 rust), edge tests (2) and no new skips all pass |

## Invariants touched

- **None.** No `Value` variant, `Error` variant, grammar production, `.rb`
  extension, `to … end` / `set x to …` / `say` form, trailing-comma rule or
  `{interp}` behaviour changed. `examples/`, `modules/` and `corpus/` were not
  modified.

## Known gaps / follow-ups

- The binding section names crates on crates.io but does not pin versions for the
  ones that are not dependencies (`walkdir`, `glob`, `base64`, `hex`,
  `percent-encoding`, `sha2`, `tch`, `ndarray`, `nalgebra`, `statrs`, `ring`,
  `aes-gcm`, `ed25519-dalek`). Version selection belongs to the phase that adds
  each binding, not to the policy.
- A future phase that actually adds a binding must update the refusal tests with
  the new successful surface rather than deleting them.
- No native backend was promised or built; the document says it is the only thing
  that would change the speed answer and that this phase does not commit to it.
