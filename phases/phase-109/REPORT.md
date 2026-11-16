# Phase 109 — Research whether a native backend for Redblue is feasible, and report

## What changed

| File | Lines | What |
|---|---|---|
| NATIVE.md | +390 −0 | The feasibility study. It names the existing components with file:line citations and says which are reusable, states each obstacle as a cost, compares LLVM and Cranelift, analyses the `.rbc` version 6 and `rb dis` impact, gives a bounded speedup estimate, recommends **defer**, lists what would make that recommendation wrong, and separates what a backend would unlock from what it would not fix. |
| src/bytecode/format.rs | +243 −0 | Three `#[cfg(test)]` tests in `native_study_tests`: they pin the study's recommendation and citations, including the actual twelve-variant count of `Value` in `src/value.rs`, pin the bytecode surface a backend would consume against the version the study documents, and prove that a future format version is refused rather than guessed at. |
| docs/OUT_OF_SCOPE_FINDINGS_LEDGER.md | +7 −1 | Queues F-109-01, the `docs/BYTECODE.md` version drift, as `[OPEN]` for Section 14 remediation so the finding is not lost outside this phase, and assigns it to Phase 170's documentation-consistency scope. |
| docs/FUTURE_ROADMAP_AND_DEFERRED.md | +3 −3 | Extends Phase 170's scope to cover documentation drift alongside parameter order and stdlib consistency, so the ledger's ownership of F-109-01 matches the roadmap. |

`Cargo.toml` and `Cargo.lock` are unchanged: `git diff -- Cargo.toml Cargo.lock` is empty. No crate was added, no dependency was upgraded, and `FORMAT_VERSION` was not changed. The phase adds no code generator and no runtime path.

## Reproduction

The finding was reproduced before any change with the smallest command that shows it:

```text
$ ls NATIVE.md
ls: cannot access 'NATIVE.md': No such file or directory
```

The repository had the bytecode compiler, `.rbc` format, disassembler and VM described by the finding, but no feasibility study and no recorded decision about a native backend.

## Tests added

| Test | Edge class covered |
|---|---|
| `native_study_records_a_recommendation_and_the_components_it_cites` | documentation completeness — watches the study fail while NATIVE.md is absent, then pass once the decision and citations exist; also counts the `Value` variants in `src/value.rs` and requires the study's twelve-variant claim to match |
| `native_study_pins_the_bytecode_surface_a_backend_would_consume` | singleton / boundary — one compiled program, the version the study documents (6), 13-byte instructions, lossless encode/decode round trip, and the disassembly naming `SAY` and `v6` |
| `edge_native_study_rejects_an_unknown_future_bytecode_version` | malformed input / failure — a version word of `FORMAT_VERSION + 1` is refused with a message naming the unknown version |

The first test was written first and failed with:

```text
NATIVE.md should be readable at /home/runner/work/rbops/rbops/redblue/NATIVE.md: No such file or directory (os error 2)
```

It turned green when NATIVE.md was written. The other two tests were then added and run with `cargo test native_study`.

### Test requirement matrix

| Row | Status |
|---|---|
| empty | N/A — this phase adds no runtime operation, so there is no new empty input to exercise. |
| singleton | covered — one program is compiled, encoded, decoded and disassembled. |
| boundary | covered — version 6 is accepted and version 7 is refused. |
| out_of_bounds | N/A — no indexing, slicing or collection operation was changed. |
| type_mismatch | N/A — no value or type operation was changed. |
| numeric_boundary | N/A — no arithmetic or numeric representation was changed. |
| unicode | N/A — no text or Unicode path was changed. |
| nesting_recursion | N/A — no recursion or block-nesting behaviour was changed. |
| duplicate_missing_keys | N/A — no record or field behaviour was changed. |
| malformed_input | covered — an unknown bytecode format version is rejected. |
| resource_limit | N/A — no limit, allocation or resource path was changed. |

The quota is met: three new `#[test]` functions, one named `edge_*`, and one that asserts `Chunk::decode` returns an error. No test was skipped, ignored, weakened, deleted or re-scoped, and no `allow(clippy::` suppression was added.

## Gates

| Gate | Result |
|---|---|
| cargo fmt --all -- --check | pass |
| cargo clippy --all-targets -- -D warnings | pass |
| cargo test --all-targets | pass — 85 binaries, 3041 passed, 0 failed, 0 ignored |
| rbops/verify.sh phase-109 | pass — all report, diff, fmt, clippy, test, example-sweep and test-policy checks passed; reported 3041 tests matched actual 3041 |

## Invariants touched

- None. The language surface, `.rbc` format version 6, opcode bytes, `Value` variants, error variants and existing tests are unchanged.
- No OS, kernel, GPU, desktop or packaging work is authorised by this phase. The finding asks a question; the recommendation is a research decision, not a mandate.
- No crate was added to `Cargo.toml`, so the study's LLVM and Cranelift costs remain analysis rather than a build obligation.

## Known gaps / follow-ups

- `docs/BYTECODE.md:3` still says "Format version: 4" while `src/bytecode/format.rs:81` and `docs/BYTECODE.md:56` say version 6. This study uses 6. The defect is recorded in `phases/phase-109/FINDINGS.md` and queued as `F-109-01` in `docs/OUT_OF_SCOPE_FINDINGS_LEDGER.md`, and is not changed here because it is outside the native-backend question.
- The speedup figures are estimates bounded by the project's own comparison table and by the dynamic value model; this phase deliberately did not add a dependency or benchmark a backend that does not exist.
- If the deferral trigger in NATIVE.md fires, the next phase should be a Cranelift translator over `.rbc` version 6 with a differential corpus and the byte-identical self-hosted fixed-point test, not an LLVM integration.
