# phase-091 — Document the memory model: shared values, copying, and the cycle leak

## Round 1 — review findings fixed

Five findings, all fixed. No line of `src/` was touched; `git diff --stat` over
tracked files is `README.md | 2 +-`, and `git status` shows five paths, none of
them under `src/`.

| # | Severity | Finding | What changed |
|---|---|---|---|
| 1 | BLOCKER | the cycle test built a bespoke `Node { payload: u32 }`, so it could not fail if Redblue's representation changed, and no Redblue program leaked | the cycle is now built from records a **real Redblue program** made, the leak is **measured in bytes** with a counting global allocator, and the opposite direction — that a finished program hands every byte back — is measured with the same allocator. Four new tests back it. |
| 2 | MAJOR | the override test never set `REDBLUE_MAX_CALL_DEPTH`, never reached `resolve_max_call_depth()`, never used `Vm::with_max_call_depth` | the test now sets and unsets the variable, checks the limit named in the error for unset / set / padded / non-numeric / zero / negative, and checks `Vm::with_max_call_depth` both with the variable set and for a zero. |
| 3 | MAJOR | the collector scan forbade `Weak`/`Gc`/`weak_count` but permitted `downgrade`/`upgrade`/`strong_count`/`ptr_eq`, and ignored `Cargo.toml` | the forbidden list now includes `::downgrade`, `::upgrade`, `strong_count`, `weak_count` and `ptr_eq` over every `.rs` under `src/`, plus a `Cargo.toml` dependency scan for seven collector crates. |
| 4 | MAJOR | `docs/MEMORY.md:64` called `src/interpreter.rs:1626` "the shape of every property write", which is false — both engines match `Value::Record` only and silently fall through | the document now names both sites, both guards, and states the silent fall-through; a new test pins all of it behaviourally and textually. |
| 5 | MINOR | `docs/MEMORY.md:42` cited `src/value.rs:88-90` for a claim covering `captured: Arc<Captured>`, which is at line 93 outside the range | the document cites `src/value.rs:85-91` for the body, `:88-90` for the `Arc` rationale and `:93` for the captures; `memory_md_citations_still_match_the_code` now pins all three. |

### Finding 1, in detail

The reviewer's objection had two halves and both are addressed.

**"not a `Value`/`Shared` cycle, so it cannot fail if Redblue's representation
changes."** The cycle's nodes now hold `Value`s that `CYCLE_SOURCE` — a real
Redblue program — built, in `std::sync::Arc`, which is what `Shared` is an alias
for. The test asserts on `Arc::ptr_eq` and `Arc::strong_count` over the *record's
own* `Shared<Fields>` storage and over the payload list inside it, so the thing
being watched is Redblue's value representation. Four further guards make the
representation load-bearing:

| Guard | Breaks when |
|---|---|
| `edge_a_reference_cycle_is_never_reclaimed` | the leak stops being a leak — a collector, a `Weak`, or `Shared` leaving `Arc` makes the byte count fall |
| `edge_writing_a_record_into_its_own_field_copies_instead_of_closing_a_cycle` | the write stops copying, i.e. `src/interpreter.rs:1626` stops being copy-on-write |
| `a_redblue_value_has_no_interior_mutability_and_no_back_reference` | a payload gains interior mutability or a back-reference — the condition that would make the cycle reachable from a program |
| `no_cycle_collector_exists_anywhere_in_the_crate` | collection is added, by text, by inferred type, or by dependency |

**"the DoD requirement for a leaking Redblue program plus a test that it leaks is
unmet."** This is where the previous round was wrong, and the correction is in the
measurement rather than in the wording. `tests/memory_model_test.rs` installs a
counting global allocator (`LIVE_BYTES`) and reports live bytes:

- `edge_a_reference_cycle_is_never_reclaimed` — runs the program, joins its two
  records into a cycle, drops every handle that is not part of the cycle, and
  asserts the allocator is still holding **46,138,670 bytes** (measured; the
  floor is 4 MiB, an 11× margin) with nothing outside the cycle naming it, then
  drops the last external handle and asserts that released **zero** bytes.
- `edge_a_redblue_program_hands_back_everything_it_allocated` — runs the *same*
  program to completion, drops the VM and its result, and asserts the residual is
  under 1 MiB (measured: 4,692 bytes, a 218× margin).

So the leak is now a byte count rather than a description, and the two tests are
one measurement in opposite directions: the cycle keeps what the program gives
up. That is a stronger answer to "the suite stays green anyway" than the previous
round's `u32` payload, and it fails if the representation changes.

**What is still not true, and is not claimed to be:** no Redblue program can
build a cycle today. The document says so in its own heading and the test that
proves it (`a_redblue_program_cannot_build_a_reference_cycle`) is kept. Filling a
`Shared<Fields>` after its handle exists is not a missing function but a missing
type — `Arc` hands out a `&mut` only to the last handle, and `Vec<Value>` and
`Fields` have no interior mutability — which is why the cycle above has to
supply that one capability by hand. `a_redblue_value_has_no_interior_mutability_and_no_back_reference`
checks the capability is still absent, which is what keeps the limitation latent
rather than quietly active.

### Gates re-run after the fixes

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass (no diff) |
| `cargo clippy --all-targets -- -D warnings` | pass (exit 0, no warnings) |
| `cargo test --all-targets` | pass — **1799 passed, 0 failed, 0 ignored** across 65 test binaries (was 1795: four tests added, none removed) |
| `./rbops/verify.sh phase-091` | **not run — `rbops/` does not exist in this checkout**, unchanged from round 1. `ls rbops` → `No such file or directory`; `find . -name 'verify*.sh'` → nothing outside `target/`. The other three were run by hand and are reported above as they printed. |

### Mutation checks — the new gates were each verified to fail

A gate that has never been seen red is not a gate, so every new assertion was
broken on purpose and the suite watched:

| Mutation | Caught by |
|---|---|
| `resolve_max_call_depth()` returns `MAX_CALL_DEPTH` unconditionally | `the_call_depth_limit_has_the_documented_environment_override` — `REDBLUE_MAX_CALL_DEPTH=7 must bound the call chain rather than the default; left: "1000", right: "7"` |
| `Vm::with_max_call_depth` ignores its argument | same test — `nine frames is not enough for twenty frames of recursion: Nothing` |
| an inferred-type `Arc::downgrade` + `strong_count` back-reference added to `src/error.rs` | `no_cycle_collector_exists_anywhere_in_the_crate` — `src/error.rs mentions '::downgrade'`. This is the exact hole the reviewer named: the mutation contains none of `Weak`/`WeakRef`/`Gc`/`gc::`/`weak_count`, so the round-1 version of this test passed it. |
| `object Node` binds `Value::object` instead of `Value::record` | `a_property_write_reaches_a_record_and_no_program_can_make_an_object` — `left: "object", right: "record"` |
| a `RefCell` reaches `src/value.rs` | `a_redblue_value_has_no_interior_mutability_and_no_back_reference` — `src/value.rs mentions 'RefCell'` |
| `PAYLOAD_CEILING` driven to 0 | `edge_a_redblue_program_hands_back_everything_it_allocated` — `4692 bytes were still held` |
| `PAYLOAD_FLOOR` driven past the measurement | `edge_a_reference_cycle_is_never_reclaimed` — `is holding 46138670` |

`tests/memory_model_test.rs` was also run five times back to back: 14 passed each
time, 2.8 s each, no flake.

## What changed

Documentation and tests only. No line of `src/` was touched.

| File | Lines | What |
|---|---|---|
| `docs/MEMORY.md` | 250 (new) | The memory model as implemented: immutable `Arc`-shared values, copy-on-read, no GC, no cycle collection measured in bytes, and `MAX_CALL_DEPTH` as a catchable `Error::Limit` with the `REDBLUE_MAX_CALL_DEPTH` override. |
| `README.md` | +1 −1 | Links `docs/MEMORY.md` from the grammar/doc pointer line (README.md:198). |
| `tests/memory_model_test.rs` | 1110 (new) | 14 tests pinning every claim in `docs/MEMORY.md` against the code, including the `file:line` citations the document makes, and measuring the cycle leak and its absence in live bytes. |

## Reproduction

The finding was: *the memory model is documented nowhere a user would look*.

```
$ grep -rniE "memory model|arc|shared value|copy.on.(read|write)|cycle" README.md docs/*.md
README.md:8:...power of modern programming languages.
docs/GRAMMAR.md:...      (grammar productions only)
$ ls docs/
BOOTSTRAP.md  BYTECODE.md  GRAMMAR.md
```

No document named the model. Every element of the finding was then confirmed in
the source before any file was written:

- `src/value.rs:276` — `pub type Shared<T> = std::sync::Arc<T>;`
- `src/value.rs:253-255` — `List(Shared<Vec<Value>>)`, `Record(Shared<Fields>)`, `Object(String, Shared<Fields>)`
- `src/value.rs:88-90` — "Behind an `Arc` because reading any variable clones the value bound to it …"
- `src/interpreter.rs:1619-1630`, `src/bytecode/vm.rs:2652-2675` — the only two property writes, each matching `Value::Record` only
- `grep -rn "Weak\|WeakRef\|Gc\|gc::\|weak_count\|::downgrade\|::upgrade\|strong_count\|ptr_eq" src/` — no matches anywhere in the crate
- `src/bytecode/vm.rs:18-21` — "There is no path from a `.rbc` to a native stack overflow."
- `src/interpreter.rs:42` — `pub const MAX_CALL_DEPTH_ENV: &str = "REDBLUE_MAX_CALL_DEPTH";`
- `build.rs:19` — `const MAX_CALL_DEPTH: usize = 1000;`

## Tests

| Test | Edge class covered |
|---|---|
| `list_record_and_object_storage_is_shared_not_copied` | shared storage — `Arc::strong_count` on the `List`, `Record` and `Object` payloads after a clone |
| `reading_a_variable_copies_rather_than_aliases` | copy-on-read — growing one binding is invisible to the other; asserts `length(first) - length(second) == -1` |
| `edge_writing_through_one_name_copies_so_the_other_name_cannot_see_it` | boundary — copy-on-write; the shared allocation the second name holds does not grow, the written one does |
| `edge_a_reference_cycle_is_never_reclaimed` | resource limit — **the cycle leak itself, in bytes**: a cycle of records a Redblue program made still holds ~44 MB after every external handle is gone, and dropping the last one releases zero |
| `edge_a_redblue_program_hands_back_everything_it_allocated` | resource limit — **the other direction, in bytes**: the same program, run to completion, returns to within 4,692 bytes of the baseline |
| `edge_writing_a_record_into_its_own_field_copies_instead_of_closing_a_cycle` | boundary — the write at `src/interpreter.rs:1626` on real `Shared<Fields>`, asserting with `Arc::ptr_eq` that the field holds a *different* allocation from the storage containing it |
| `a_redblue_value_has_no_interior_mutability_and_no_back_reference` | resource limit — the representation cannot be filled after construction, which is why the leak is latent |
| `a_redblue_program_cannot_build_a_reference_cycle` | nesting/recursion — `set node.self to node` stores the pre-write record, so `node.self.self` is `nothing` |
| `a_property_write_reaches_a_record_and_no_program_can_make_an_object` | type mismatch — a property write against a non-record is silently left alone, and `object` binds a `Record` so no program reaches the `Object` variant |
| `no_cycle_collector_exists_anywhere_in_the_crate` | resource limit — no `Weak`/`WeakRef`/`Gc`/`gc::`/`weak_count`/`::downgrade`/`::upgrade`/`strong_count`/`ptr_eq` in any `.rs` under `src/`, no `Rc` in `src/value.rs`, no collector crate in `Cargo.toml` |
| `deep_recursion_raises_a_catchable_limit_not_a_stack_overflow` | resource limit — **asserts a failure**: unbounded recursion is `Error::Limit`, `is_resource_limit()`, names no stack overflow, and a `try … catch error` around it lets the program continue |
| `the_call_depth_limit_has_the_documented_environment_override` | boundary — `MAX_CALL_DEPTH_ENV`; the limit named in the error for unset, set, padded, non-numeric, zero and negative values, and for `Vm::with_max_call_depth` with the variable set and for a zero |
| `memory_md_citations_still_match_the_code` | the doc's own citations: `src/value.rs:276`, `:253-255`, `:85-91`, `:88-90`, `:93`, `src/interpreter.rs:42`, `:139`, `:1619-1630`, `:1625`, `:1626`, `src/bytecode/vm.rs:18-21`, `:2652-2675`, `:2664` |
| `memory_md_is_linked_from_the_readme` | `docs/MEMORY.md` exists, names the cycle, and `README.md` links it |

14 `#[test]` functions; 5 named `edge_*`; 2 assert a failure is produced; 2 measure
live bytes. Zero `#[ignore]`, zero skips, zero new lints, no test deleted.

### Edge-case matrix (AGENTS.md §3.2)

- **empty / zero / nothing** — covered. `a_redblue_program_cannot_build_a_reference_cycle` asserts `node.self.self` is `nothing`; the collector test asserts `Vm::with_max_call_depth(0)` is ignored rather than obeyed, and that `REDBLUE_MAX_CALL_DEPTH=0` falls back.
- **singleton** — covered. `list_record_and_object_storage_is_shared_not_duplicated` builds one-element and two-element lists plus empty record and object payloads.
- **boundary** — covered. `edge_writing_through_one_name_copies_so_the_other_name_cannot_see_it` is exactly the last-element-before / first-element-after boundary of a copy-on-write; `edge_writing_a_record_into_its_own_field_copies_instead_of_closing_a_cycle` is the last-step-before / cycle-after boundary of a self-referential write; `the_call_depth_limit_has_the_documented_environment_override` walks the limit across its refusals.
- **out_of_bounds** — N/A. This phase changed no indexing code and documented no index. Bounds behaviour is `tests/`'s existing subject, not this diff's.
- **type_mismatch** — covered. `a_property_write_reaches_a_record_and_no_program_can_make_an_object` writes a property onto a number and asserts the number is untouched, and that `object` produces a `Record` rather than the `Object` variant.
- **numeric_boundary** — covered. `the_call_depth_limit_has_the_documented_environment_override` reads the limit back out of the error message for `7`, `12`, `0`, `-3` and a non-numeric string; `reading_a_variable_copies_rather_than_aliases` asserts the list-length difference `-1`.
- **unicode** — N/A. No string handling changed. The one text assertion is `name: "head"` compared with `PartialEq`, which is unicode-correct by construction.
- **nesting / recursion** — covered. `a_redblue_program_cannot_build_a_reference_cycle` (a self-referential record), `deep_recursion_raises_a_catchable_limit_not_a_stack_overflow` (unbounded self-recursion plus the catch), and `the_call_depth_limit_has_the_documented_environment_override` (two functions recursing into each other, bounded and unbounded).
- **duplicate / missing keys** — covered. `edge_writing_a_record_into_its_own_field_copies_instead_of_closing_a_cycle` inserts `self` into a record that has no such key and reads it back; `a_property_write_reaches_a_record_and_no_program_can_make_an_object` writes `a` onto a value with no fields at all.
- **malformed input** — N/A. The phase parses no user input. Every `eval` helper `expect`s a clean lex/parse, and the only "malformed" thing in the diff is a deliberately cyclic Rust structure, which is well-formed Rust.
- **resource limit** — covered. `edge_a_reference_cycle_is_never_reclaimed`, `edge_a_redblue_program_hands_back_everything_it_allocated` and `no_cycle_collector_exists_anywhere_in_the_crate` are the unbounded-growth row; `deep_recursion_raises_a_catchable_limit_not_a_stack_overflow` is the recursion-depth row.

## Invariants touched

- None. No `src/` file changed, so no `Value` variant, no `Error` variant, no grammar production and no `.rb` extension moved. The documented model is the one the code already implements.

## Known gaps / follow-ups

1. **`rbops/verify.sh` is not in this checkout**, so the fourth gate cannot be
   run from here. `ls rbops` → `No such file or directory`, and `find . -name
   'verify*.sh' -not -path './target/*'` finds nothing. This is unchanged from
   round 1 and is a property of the checkout, not of the diff.
2. **No Redblue program leaks, and the round-1 reviewer was right to say so.**
   The cycle leak is real and is now measured in bytes, but the *program* the
   DoD asked for does not exist and cannot: see `FINDINGS.md`, which carries the
   proof and the substitution. `docs/MEMORY.md` says so under its own heading
   rather than implying a leak that is not there.
3. **`MAX_CALL_DEPTH` is generated, not written down.** `docs/MEMORY.md` states
   `1000` and cites `build.rs:19`. `memory_md_citations_still_match_the_code`
   checks the `src/` citations but deliberately does not pin `build.rs:19`'s
   line number, because that file is generated machinery rather than the model;
   a change there moves the limit without making the document untrue.
4. `MEMORY.md` documents `REDBLUE_MAX_ITERATIONS` and `REDBLUE_MAX_STEPS` in one
   sentence, by analogy. Neither is a claim this phase's tests pin, and neither
   is part of the phase's finding.
5. **`tests/memory_model_test.rs` sets a process environment variable.** It holds
   `exclusive()`, a file-local mutex that every program-running test in the file
   also takes, because `REDBLUE_MAX_CALL_DEPTH` is read by every VM in the
   process. A future test in another file that reads it without the lock would
   be a race; the comment on `exclusive()` says so.
