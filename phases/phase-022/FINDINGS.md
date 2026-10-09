# phase-022 — FINDINGS

The finding this phase was opened for was `S2 exists but nothing proves it can
build itself.` It reproduced, it was fixed, and **the fixed point holds**:
`stage1.rbc` and `stage2.rbc` are byte-identical, with no environment override
of any limit. The previous run of this phase recorded the work as blocked. It
was not. Two of the three things it measured as the blocker had already been
fixed by the time it measured them, and the third — the step budget — is a
constant, not a language change. Everything below was measured in this run.

---

## 1. What was actually slow, and where

`bootstrap/compiler.rb` at `564fdf2` cannot compile itself in usable time. It is
not the code generator's `ctx` record, and it is not `push`. `bootstrap/compiler.rb`
already accumulated through named globals and `append` — `G_POOL`, `G_INTERN`,
`G_CODE`, `G_JUMPS`, `G_BLOCKS` — for exactly the reason the previous run's
§3 says it could not. `phases/phase-021/FINDINGS.md` §3b and this phase's own
previous §2 quoted `emit_const` as `set emit_const_pool to push(ctx.p, …)` at
`bootstrap/compiler.rb:2213`. At `dff04f6` that line reads:

```redblue
to emit_const(tag, raw, line)
    set emit_const_index to 0
    set emit_const_index to length(G_POOL)
    append("G_POOL", {t: tag, b: raw})
```

The measurement that produced "n^1.97 on `emit_const`" was of a compiler that no
longer exists in that shape. **The previous run's §3 root-cause analysis does not
apply to this tree and should not be promoted to a phase.**

The cost that is real is in two places, both of them `push` on a list that grows
to the size of the *input*, not of the program:

| Site | Was | Cost |
|---|---|---|
| `encode` / `encode_block` | `set out to cat(out, more)` per field and per byte | quadratic in the size of the **`.rbc`** |
| `lex` | `set lex_tokens to push(lex_tokens, lex_r[0])` | quadratic in the number of **tokens** |

Measured on this machine, `rb vm` of the 400-statement program, release build:

| | before | after |
|---|---|---|
| `encode` | 24.0 s of a 27.2 s run | < 1 s |
| whole run (800 statements) | 29.0 s | 3.5 s |
| whole run (`bootstrap/compiler.rb`) | ≈ 6 minutes (extrapolated) | 13.4 s |

Phase timings for the self-compilation after both fixes, release build:
read+lex 2 s, parse 5 s, codegen 6 s, encode < 1 s, write < 1 s; 18 251 tokens.

## 2. The step budget, and why it had to move

With the time fixed, the run still died:

```
Error: RuntimeError: Step budget of 10000000 reached before the program finished
```

Measured by bisecting `REDBLUE_MAX_STEPS`: one self-compilation needs
**16 062 500** steps. The default was 10 000 000.

`MAX_STEPS` is a constant, not a language change, and the run it stopped is
bounded and terminates in 13 seconds. The change is 10 000 000 → 32 000 000,
about twice what the compiler needs. `MAX_ITERATIONS` still caps a single loop at
a million, so the ordinary runaway `while` is caught by that first and this
budget is the backstop for the rest. Nothing else about the guard moved, and
`REDBLUE_MAX_STEPS` still lowers it.

The claim "the compiler needs more steps than the default allows" is pinned by
`edge_the_self_hosted_compiler_compiles_itself_byte_identically`, which runs with
no environment override; and the claim "the budget still refuses a run that
outlasts it, and writes nothing" is pinned by
`edge_the_self_compilation_still_obeys_the_step_budget`.

## 3. The defect the fixed point found

With the compiler able to run on itself, `stage1.rbc` and `stage2.rbc` differed.
Two jump operands, in one block — `lex_text`, whose body is a `while` inside a
`while`:

```
0030  JUMP_IF_FALSE    207   ; line 581          (stage 1 — one past the loop)
0030  JUMP_IF_FALSE    222   ; line 581          (stage 2 — end of the block)
0166  JUMP_IF_FALSE    185   ; line 618          (stage 1)
0166  JUMP_IF_FALSE    207   ; line 618          (stage 2)
```

`compile_while` kept the slot of its own `JumpIfFalse` in a name across the call
that compiled its body. A Redblue function's variable names are program-wide, so
the inner loop's `compile_while` took the outer loop's slot and patched the wrong
jump. Fixed by passing the slot as a parameter — the idiom `compile_if_branches`,
`compile_if_else` and `compile_unless_body` already used for exactly this reason,
and which `compile_while` was the last one not using.

`compile_while` was the only site. Every other `emit_jump_at` result is either
passed straight to a helper as an argument (`compile_statement`'s `if` and
`unless`) or held in a helper's parameter. Checked at
`bootstrap/compiler.rb:2225-2244` (the three jump helpers) and at each of the
seven `emit_jump_at` / `patch_at` call sites.

## 4. Smaller things found on the way, not fixed (AGENTS.md §1 rule 7)

- **Four clippy errors are pre-existing on `main`** and had to be fixed here,
  because `cargo clippy --all-targets -- -D warnings` is one of the four gates and
  it was red before this phase touched anything: `src/runtime.rs:294` (a lifetime
  that could be elided) and `src/interpreter.rs:1602,1609` (a redundant deref and
  a `&[item.clone()]` that should be `std::slice::from_ref(item)`), inside
  `append_target` and `apply_map`. Verified by running clippy on a clean stash of
  `main`: the same four errors, at lines 1580/1587/294 before this phase's edits
  shifted them. None is in the changed logic; they are one-liners made so the
  gate can be run at all. **They should not be attributed to this phase's work.**
- **`docs/BYTECODE.md:3` says "Format version: 4"; `FORMAT_VERSION` is 5**
  (`src/bytecode/format.rs:75`). The version table at `docs/BYTECODE.md:73`
  already documents version 5, so only the header line is stale. Carried over
  from `phases/phase-021/FINDINGS.md`. One line, another phase's area.
- **`bootstrap/compiler.rb` does not satisfy `rb format --check`** — it did not
  before this phase either (`git stash` and re-run: `File would be reformatted`
  on `main` as well). No gate checks it. A formatter phase should decide whether
  `bootstrap/` is in scope; if it is, the file is ~2 800 lines to reformat and
  the fixed point has to be re-verified afterwards.
- **`intern` is a linear scan of the whole intern table on every name emitted.**
  That is most of what is left of the code generator's 6 seconds: the intern
  table holds over a thousand entries and it is walked for each of the several
  thousand names the compiler emits. A hash map in the language would fix it; that
  is a language change and belongs in its own phase. It is a *time* cost, not a
  step cost — `intern` runs the same number of steps either way.
- **The self-compilation costs 3 m 25 s of `cargo test`.** Three runs, three
  processes, four cores. In a test build the interpreter is about 11× slower than
  the release build, so a run that takes 13 s in `cargo run --release` takes
  2 m 30 s under `cargo test`. This is affordable but it is the largest single
  cost in the suite, and it will grow with the compiler. A phase that makes the
  compiler faster should revisit it.

## 5. What this phase did not verify, and why

- **Not** that the fixed point holds for *any* Redblue program — it is a
  statement about one, `bootstrap/compiler.rb`. The general statement is phase
  021's: 306 corpus programs, 8 examples and modules, and a list of statement
  shapes, all byte-identical through `rb vm`. Untouched here.
- **Not** anything about S4. The `rb` that ships is still built by Cargo.
- **Not** `verify.sh`. `rbops/` is not in this checkout and the dispatch
  instructions place the pipeline outside it, so the fourth gate could not be
  run. The other three were.