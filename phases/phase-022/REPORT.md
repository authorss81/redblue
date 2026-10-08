# Phase 022 — Self-hosting fixed point (bootstrap S3)

## Status: BLOCKED. The definition of done is not met and is not claimed.

The finding reproduces. S2 exists (`bootstrap/compiler.rb`, 2 748 lines, `rb
compile` → `rb vm` byte-identical over 306 corpus programs). Nothing proves it can
build itself, and the reason nothing proves it is that **the check cannot run**:
one self-compilation of `bootstrap/compiler.rb` is a ~24-minute job and the four
boxes in the DoD are all downstream of it.

This phase changed no production code. It wrote the failing test, watched it fail,
implemented the fix it pointed at, measured that the fix does not reach the DoD,
and **reverted it**. `git status` is clean. Full analysis, measurements and the
three ways forward are in `phases/phase-022/FINDINGS.md`.

## What changed

| File | Lines | What |
|---|---|---|
| — | +0 −0 | **No production file changed.** See below. |

The four files I edited and reverted: `src/bytecode/opcode.rs` (a new `APPEND_LOCAL`
opcode, byte 49), `src/bytecode/codegen.rs` (`set N to push(N, x)` compiles to it),
`src/bytecode/vm.rs` (`append_local`), `src/bytecode/format.rs`
(`FORMAT_VERSION` 5 → 6). Plus `tests/bootstrap_selfhost_test.rs`.

**Why it is gone.** The optimisation is sound — a Redblue list is never shared, so
appending through a variable's own handle is unobservable, and `push` itself is
untouched — and it worked: it made the lexer's ~30 000-record token list linear.
The self-compilation still did not finish in 900 s, because the dominant cost is
a *different* copy in a *different* place (measured in FINDINGS §3: the codegen's
`ctx` record is a parameter, so it is deep-copied once per emitted instruction,
n^1.97). Shipping a bytecode format version bump and a new instruction in a phase
that delivers none of the four DoD boxes is the half-measure AGENTS.md §5 calls
fake completion. It is roughly twenty lines of work in whichever phase makes the
accumulator cheap, and it should be written there.

## Tests added

| Test | Edge class covered |
|---|---|
| `edge_the_self_hosted_compiler_compiles_itself_byte_identically` (**written, watched fail, reverted**) | resource_limit / resource — the whole point of the phase |

That is the only test this phase produced, and it does not survive in the tree:
it was killed by a **900 s** wall clock with no output, because the run it
performs does not finish. A suite that never returns cannot be read, which is why
`phases/phase-021/FINDINGS.md` §3c removed its own copy of the same test. Leaving
it in would hand the next run a suite that hangs rather than a gate that fails.

**Test quota: not met.** Zero `#[test]` functions survive, so the floor of ≥3 and
the `edge_*` floor are unmet, and there is no test asserting a failure. This is
not an oversight and not a judgement call — the quota exists to catch "no
verification at all", and the honest state is that there is no reachable
behaviour of this phase to verify.

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass (exit 0, no diff) |
| `cargo clippy --all-targets -- -D warnings` | pass (exit 0, zero warnings) |
| `cargo test --all-targets` | **1030 passed, 0 failed, 0 ignored** |
| `./rbops/verify.sh phase-022` | **not run — `rbops/` is not in this checkout** |

`ls rbops/` returns `No such file or directory`. The dispatch instructions place
the RBOPS pipeline outside this checkout and forbid inspecting it, so I could not
run the fourth gate and am not claiming it.

**`must_touch` will fail.** This phase declares `must_touch: ["src/", "bootstrap/"]`
and changed no file under either. That is the honest consequence of a blocked
phase, and it is reported here rather than worked around with a change too small
to matter.

Zero new `#[ignore]`, zero `// skip`, zero `allow(clippy::…)`. Zero newly-failing
pre-existing tests. The 1 030 passing are the suite as it arrived — the tree is
byte-identical to `564fdf2` apart from `phases/phase-022/`.

## Definition of done

- [ ] **`rb vm stage1.rbc -> stage2.rbc`, byte-identical to `stage1.rbc` over the
      corpus — NOT MET.** The self-compilation does not complete in 900 s
      (FINDINGS §1), so no stage 2 of the compiler exists and no bytes can be
      compared. Not claimed in either direction.
- [ ] **the fixed-point check runs in CI on every push — NOT MET.** A check that
      cannot return is not a check.
- [ ] **three consecutive runs produce identical bytes — NOT MET** for the
      self-compilation, for the same reason. Determinism of the smaller runs is
      unaffected and is stated in FINDINGS §5; it is not the same claim and is not
      offered as one.
- [ ] **documented in `docs/BOOTSTRAP.md` — NOT MET.** The file does not exist
      (`ls docs/` → `BYTECODE.md`, `GRAMMAR.md`). Writing it would mean
      documenting a fixed point that does not exist.

## Test-requirement matrix (AGENTS.md §3.2)

Every row is **N/A: this phase changed no behaviour.** No source file, no
grammar, no builtin, no value type. A matrix ticked "covered" here would be
attesting to tests that do not exist.

empty · singleton · boundary · out_of_bounds · type_mismatch · numeric_boundary ·
unicode · nesting_recursion · duplicate_missing_keys · malformed_input ·
resource_limit — all N/A, same one line.

## Invariants touched

- **None.** No `.rb` extension, no `end`/brace, no `set … to`, no `Value` variant,
  no `Error` variant, no grammar change. The reverted diff touched
  `src/bytecode/*` and `FORMAT_VERSION`, and none of it is in the tree.

## Known gaps / follow-ups

- **The blocker, with its measurement.** Redblue has no shared mutable value:
  `Value::List`/`Value::Record` own their contents and every variable read clones
  them (`src/interpreter.rs:927`, `src/bytecode/vm.rs:979`). `bootstrap/compiler.rb`'s
  codegen threads one immutable `ctx` through every call
  (`bootstrap/compiler.rb:2213`), so a ~30 000-record code list is deep-copied once
  per emitted instruction. Measured **n^1.97** on `emit_const` alone (FINDINGS §3).
  No change to `push` can reach it. → FINDINGS §3, §4.
- **This cannot be fixed inside phase-022.** Every route is a language change:
  a shared mutable buffer, copy-on-write records/lists, or a rewrite of the
  compiler's codegen. The first two need interior mutability in a `Value`
  payload, and AGENTS.md §2 lists the `Value` variants as an invariant with
  `redblue::Value` exported. This phase's PROMPT does not re-open the language
  design, so it is not this phase's diff (AGENTS.md §1 rules 7 and 8).
  → FINDINGS §4, with an acceptance gate that counts appends rather than timing
  them, because a `Value` payload change alone would pass any timing-free test
  while leaving the loop quadratic.
- **`docs/BYTECODE.md:3` says "Format version: 4"; `FORMAT_VERSION` is 5.** Stale
  header from phase-020. One line, another phase's area. → FINDINGS §6.
- **`rb run bootstrap/compiler.rb bootstrap/compiler.rb out.rbc` is not a usable
  command today** for the reason above. Anyone reaching for it will wait.