# Phase 094 — Write down what Redblue libraries should be written in Redblue, bound, or not built

## Reproduction of the finding

The finding is an absence, so the smallest command is existence plus a search
for a written policy:

```
$ test -f LIBRARIES.md; echo "LIBRARIES.md exists? exit=$?"
LIBRARIES.md exists? exit=1

$ grep -rniE "numpy pattern|written in Redblue|binding|native backend" README.md AGENTS.md docs/*.md
...only unrelated hits ("name binding", "bytecode binding", "web server written in Redblue")
```

No file stated which libraries belong in Redblue, which must be Rust bindings,
or which should not exist. Every stdlib phase had to re-argue that from scratch,
which is exactly the finding. The new `tests/libraries_policy_test.rs` was written
first and run against the missing document: five tests failed at
`LIBRARIES.md must exist and be readable`; the sixth, which re-checks the
binding premise, passed. After the document and links were written, all six pass.

## What changed

| File | Lines | What |
|---|---|---|
| `LIBRARIES.md` | new | the standing policy: three sections (written in Redblue, Rust bindings, not built), every top-ten SURVEY gap decided exactly once, named crates, and the speed/PyTorch truth |
| `tests/libraries_policy_test.rs` | new, 6 tests | holds the policy to its sections, rank decisions, crate names, links and stated speed answer; re-checks that binding-required libraries are still refused today |
| `README.md` | +5 −1 | links `LIBRARIES.md` from the language-at-a-glance line and from the Standard Library intro |
| `AGENTS.md` | +4 −0 | links `LIBRARIES.md` in Resources with "read this before adding a stdlib module" |

`must_touch` is satisfied by the changed `README.md` and `AGENTS.md`; the policy
itself is the root `LIBRARIES.md` the phase asked for. No file under `src/`,
`examples/`, `modules/`, `corpus/`, `.github/`, `rbops/` or `.opencode/agent/` was
touched.

### The three decisions

- **Written in Redblue** (interpreted speed irrelevant): list/collection
  functions (`SURVEY.md` rank 1), record enumeration (rank 3), ordering and
  min/max (rank 4), text slicing/padding (rank 5), number formatting (rank 9),
  CSV/JSON writers (rank 10), plus parsers/compilers, servers, developer tools
  and test frameworks as classes.
- **Bindings to Rust crates** (numeric or native): regex (`regex`), date/time
  (`chrono`), directories (`walkdir`, `glob`), encodings/digests (`base64`,
  `hex`, `percent-encoding`, `sha2`), plus the existing `reqwest` / `std::fs`
  precedents, tensor (`tch` or `candle-core`), matrix (`ndarray`, `nalgebra`),
  statistics (`statrs`) and cryptography (`ring`, `aes-gcm`, `ed25519-dalek`,
  `sha2`).
- **Not built at all**: a PyTorch-equivalent tensor/autograd library in
  Redblue, a regex engine from scratch, hand-rolled cryptography, a GPU runtime
  from scratch, a native backend as a "library", and a packaging ecosystem
  before the language's own tooling exists. Each carries its reason.

The policy states plainly that interpreted Redblue is slower than Python, that
readability rather than speed is the advantage, that a PyTorch-equivalent written
in Redblue is not achievable, that the numpy-style binding is the answer, and
that a native backend is the only thing that would change the speed answer
(without committing to build one).

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_every_ranked_gap_is_decided_in_exactly_one_section` | **edge_** / duplicate and missing keys — all ten SURVEY ranks appear in exactly one of the three sections; a duplicated or omitted decision fails |
| `edge_policy_is_linked_from_readme_and_agents` | **edge_** / missing link — both entry points must reach the policy |
| `each_section_states_a_reason_not_just_a_list` | empty/boundary — each section body is non-empty and says why |
| `binding_entries_name_specific_rust_crates` | missing entries — every numeric/native capability names its crate; `regex`, `chrono`, `reqwest` are verified against `Cargo.toml` |
| `the_speed_answer_is_stated_plainly_and_not_hedged` | missing claim — slower than Python, readability, native backend, PyTorch, numpy and "not achievable" all present |
| `binding_required_libraries_are_refused_today` | **asserts failure** — `base64.decode` is refused by the analyzer and `time.add_days` by the runtime, so the binding entries describe gaps that are still open |

No `#[ignore]`, no `// skip`, no `allow(clippy::` suppression, and no existing
test was changed or weakened.

## Edge-case matrix

| Row | Verdict |
|---|---|
| empty | covered — `each_section_states_a_reason_not_just_a_list` refuses an empty or purely nominal section body (under 200 bytes). |
| singleton | covered — `edge_every_ranked_gap_is_decided_in_exactly_one_section` requires exactly one section per rank; zero is an omission and two is a contradiction. |
| boundary | covered — the loop spans rank 1 and rank 10, so both first and last entries are checked; the section-length floor is a second boundary. |
| out_of_bounds | N/A — this phase adds no indexing or VM path; the only runtime call is the existing library refusal re-checked by `binding_required_libraries_are_refused_today`. |
| type_mismatch | N/A — no runtime types or coercions were changed; the document phase does not evaluate Redblue values. |
| numeric_boundary | N/A — no arithmetic was added or changed; the document contains no computed numeric output. |
| unicode | N/A — no text handling was changed; the Markdown is read as UTF-8 and contains no program output. |
| nesting_recursion | N/A — the policy is flat Markdown; the test slices sections, it does not execute nested code. |
| duplicate_missing_keys | covered — duplicate rank markers and missing rank markers both fail `edge_every_ranked_gap_is_decided_in_exactly_one_section`; a missing `LIBRARIES.md` fails `read()`. |
| malformed_input | covered — a missing section, a missing heading and a missing file each panic with the document and heading named; the refusal test also covers the clean-error channel. |
| resource_limit | N/A — the test reads a small Markdown file and loops ten times; there is no unbounded work, network or filesystem outside the checkout. |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass — exit 0, no diff |
| `cargo clippy --all-targets -- -D warnings` | pass — exit 0, no warnings |
| `cargo test --all-targets` | pass — exit 0, **2214 passed, 0 failed, 0 ignored** across 76 test binaries |
| `../rbops/verify.sh phase-094` | **pass** — exit 0, `VERIFY PASS — phase-094`; report sections, diff hygiene, must_touch, fmt, clippy, 2214 tests, example sweep, failure assertion (1), test quota (6 rust), edge tests (2) and no new skips all pass. |

The local checkout is uncommitted, so the final verify run used
`GIT_INDEX_FILE=target/tmp/phase094_index` (a copy of the index) to keep the
`git add -N` that `verify.sh` itself performs. Without it the main `.git/index` is
reset during the test run in this environment and the gate reads 0 new tests; the
pipeline commits the phase before verifying, where the same diff is ordinary.
No gate check was skipped or relaxed.

## Invariants touched

- **None.** No `Value` variant, `Error` variant, grammar production, `.rb`
  extension, `to … end` / `set x to …` / `say` form, trailing-comma rule or
  `{interp}` behaviour changed. `examples/`, `modules/` and `corpus/` were not
  modified.

## Known gaps / follow-ups

- The binding section names crates on crates.io but does not yet pin versions for
  the ones that are not dependencies (`walkdir`, `glob`, `base64`, `hex`,
  `percent-encoding`, `sha2`, `tch`, `ndarray`, `nalgebra`, `statrs`, `ring`,
  `aes-gcm`, `ed25519-dalek`). Version selection belongs to the phase that adds
  each binding, not to the policy.
- A future phase that actually adds a binding must re-run
  `binding_required_libraries_are_refused_today` conceptually: once the binding
  lands, that refusal is expected to change, and the test must be updated with
  the new successful surface rather than deleted.
- No native backend was promised or built; the document explicitly says it is
  the only thing that would change the speed answer and that this phase does not
  commit to it.
