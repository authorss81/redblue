# Phase 018 — Bytecode format and disassembler (bootstrap S1a)

## Reproduction of the finding

Before this phase there was no bytecode representation to bootstrap toward:

```
$ git show 563a351^:src/lib.rs | grep -c bytecode
0
$ ls src/bytecode
ls: cannot access 'src/bytecode': No such file or directory
```

`Lexer → Parser → Analyzer → VM` was the whole pipeline; nothing emitted
anything another process could read. Both commands are run at the parent of the
phase-018 checkpoint and are reproduced by `git show 563a351^:src/lib.rs`.

The finding no longer reproduces on `main`.

## What changed

| File | Lines | What |
|---|---|---|
| `docs/BYTECODE.md` | +375 −0 | the normative format: magic, `FORMAT_VERSION`, header, constant pool, block records, 13-byte instruction record, the opcode table, the reading rules and what `rb dis` prints |
| `src/bytecode/mod.rs` | +33 −0 | module surface — `compile_source`, `Chunk`, `disassemble`, `Opcode`, `FORMAT_VERSION`, `MAX_BLOCK_DEPTH`, `NO_BLOCK`, `NO_CONST` |
| `src/bytecode/opcode.rs` | +325 −0 | the opcode table, each opcode's operand/aux usage, and which operand is a constant index or a block index |
| `src/bytecode/format.rs` | +568 −0 | `Chunk`/`Block`/`Constant`/`Instruction`, the encoder, and a `Reader` that bounds-checks every field and every declared count |
| `src/bytecode/codegen.rs` | +548 −0 | AST → bytecode, one pass per block, interning names into a canonical constant pool |
| `src/bytecode/disasm.rs` | +206 −0 | chunk → text; index order only, every printed index checked, heap walk over the block tree |
| `src/lib.rs` | +67 −2 | `pub mod bytecode`, re-exports, `rb compile` and `rb dis`, help text |
| `tests/bytecode_test.rs` | +1602 −0 | 43 tests |

Continuation pass — a defect found in the checkpoint's own output, fixed:

| File | Lines | What |
|---|---|---|
| `src/bytecode/disasm.rs` | +15 −11 | a jump to one past the last instruction is the block exit, not an out-of-range target; only a target *past* it is reported |
| `docs/BYTECODE.md` | +10 −4 | the "every jump target is an instruction of the block" claim was false; the exit form is now specified |
| `tests/bytecode_test.rs` | +109 −0 | two tests, below |

### The defect the continuation pass fixed

`patch_here` (`src/bytecode/codegen.rs`) points an exit jump at the next
instruction to be emitted, which for a `while` or a bare `if` at the end of a
block is the instruction count. The disassembler compared with `>=`, so the
compiler's own valid output was reported as malformed:

```
$ ./target/debug/rb compile target/tmp/w.rb -o target/tmp/w.rbc
$ ./target/debug/rb dis target/tmp/w.rbc
0005  JUMP_IF_FALSE    9  ; line 2: target 9 is outside this block's 9 instructions
```

The instruction count is now a documented, legal target meaning "leave this
block", and only a target past it is reported. `docs/BYTECODE.md` stated the
opposite of what the compiler emits; it now states what the compiler emits.

## Review round 1

An adversarial review of the pass above returned five findings: two BLOCKER,
three MAJOR. All five are fixed. Two of them were lossy encodings — source
constructs the compiler silently dropped on the way to the file — so fixing them
changed what a file means, and `FORMAT_VERSION` moved from 1 to 2 by the
format's own rule.

| File | Lines | What |
|---|---|---|
| `src/bytecode/codegen.rs` | +18 | `extends` interns the parent's name instead of a flag; a field's `default` is compiled in front of the `DEF_FIELD` that consumes it |
| `src/bytecode/format.rs` | +71 | `NO_CONST` and the refusal of a pool that reaches it; `encode`, `all_blocks` and `Drop` all walk on the heap |
| `src/bytecode/opcode.rs` | +20 | what `DEF_OBJECT`'s second operand and `DEF_FIELD`'s stack effect are, and which opcodes take a block index |
| `src/bytecode/disasm.rs` | +83 | a heap walk over the block tree, and a check for every index it prints |
| `src/bytecode/mod.rs` | ±0 | re-exports `NO_CONST` |
| `docs/BYTECODE.md` | +67 | version 2, a version history, the stack discipline, what the decoder refuses versus what `rb dis` reports |
| `tests/bytecode_test.rs` | +349 | five new tests; four existing ones tightened, none deleted |

### BLOCKER 1 — `extends` threw the parent's name away

`Statement::Object` compiled to `u32::from(extends.is_some())`: one bit in
`DEF_OBJECT`'s secondary operand, no operand for *which* object. The parent
chain is walked by name (`src/vm.rs`, `declare_object`), so a file written
before this fix could not be resolved by anything:

```
$ ./target/debug/rb dis target/tmp/ob.rbc          # before
main (main, arity 0)
0002  DEF_OBJECT       1, 1  ; line 5               # 1 = "extends something"
...
; constants: 5                                     # and "Base" is only the
;   [1] Base                                      # parent's own STORE
```

The parent is now interned like every other name and carried as a constant
index, with the reserved index `NO_CONST` for an object that extends nothing —
the same shape `TRY` uses for an absent handler. `rb dis` prints `none` rather
than `4294967295`, and names the parent in the comment:

```
; redblue bytecode v2
main (main, arity 0)
0002  DEF_OBJECT       1, 2  ; line 5: extends Base; block 1 (object, `Child`)
0000  DEF_OBJECT       0, none  ; line 1: block 0 (object, `Base`)
```

A pool that could reach `NO_CONST` would make the operand ambiguous, so the
declared count is refused at `NO_CONST` before a single entry is read.

### BLOCKER 2 — `has f default e` threw the default away

`Statement::Has { name, .. }` ignored `default: Option<Expr>`, and `DEF_FIELD`
had only a name operand, so `has size default 5 + 2` compiled to a declaration
with no value at all — the tree-walking VM evaluates the default
(`src/vm.rs:1397`) and the file said nothing about it.

`DEF_FIELD` is now the consumer of the value pushed in front of it, so a default
may be any expression; a `has` with no default pushes `nothing`, which is what
the VM gives such a field. The object body reads as one initialisation per
field, in source order:

```
  main.blocks[1] (object, arity 0)
0000  PUSH_CONST       3  ; line 6: 5
0001  PUSH_CONST       4  ; line 6: 2
0002  ADD                ; line 6
0003  DEF_FIELD        5  ; line 6: size
0004  PUSH_CONST       6  ; line 7: nothing
0005  DEF_FIELD        7  ; line 7: label
```

### MAJOR 3 — a test contradicted the format it was pinning

`every_jump_target_is_inside_its_own_block` asserted `target < code.len()`,
while `docs/BYTECODE.md` and the continuation-pass test above define
`target == code.len()` as the legal block exit. The old invariant only passed
because its program had a loop-back jump after every exit jump. It now asserts
`<=`, and the test runs three programs so that the exit form is exercised: the
`while` and the bare `if` must each exit by jumping to the instruction count,
which is asserted as `exits == 2`. Reverting `<=` to `<` fails the test, with
`"JUMP_IF_FALSE" jumps to 9 but block holds 9 instructions`.

### MAJOR 4 — three walks recursed per nesting level

`disasm::render_blocks`, `Block::collect` and `write_block` each put a call
frame on the stack per level of nesting, so a `Chunk` deeper than
`MAX_BLOCK_DEPTH` — which the public fields let a caller build — overflowed
the stack through the public `disassemble`/`all_blocks`/`encode`. All three are
now explicit-stack walks in the same depth-first order, so the output is
byte-identical and the depth costs heap. Dropping a `Block` had the same problem
in the destructor, which is where the first version of this test died:
`Drop for Block` takes the children out and frees them one at a time.

`MAX_BLOCK_DEPTH` is unchanged and is still what bounds a *file*: the compiler
refuses to nest deeper, `Reader::block` refuses to decode deeper, and a
hand-built tree past it still encodes to a file this build's decoder refuses.

### MAJOR 5 — the disassembler's "every index is checked" was false

`describe` checked constant-pool indices and jump targets only, while
`Reader::instruction` accepted any `u32` for either operand. A corrupt block
index decoded silently and disassembled without comment. Block operands are now
checked against the holding block's own list — `DEF_FUNCTION`, `DEF_METHOD`,
`DEF_OBJECT`, `TEST`, and both halves of a `TRY` — and the parent in
`DEF_OBJECT` is checked against the pool. An operand with no value to name says
nothing: `NO_BLOCK` is legal, not out of range.

The decoder deliberately still accepts an out-of-range *operand*: it reads
records and does not interpret what they address, so such a file is structurally
sound. What the decoder refuses is a structure it cannot hold. `docs/BYTECODE.md`
now says exactly that, in the same words, where before it implied operands were
validated too.

### Every new test was checked against the code it replaces

| Test | Restored the old code and it… |
|---|---|
| `an_extending_object_names_its_parent_in_the_constant_pool` | fails: `left: 0`, the reserved index is not written |
| `a_field_default_is_compiled_before_the_declaration_that_consumes_it` | fails: `left: ["DEF_FIELD", "DEF_FIELD"]` |
| `edge_a_pool_that_reaches_the_reserved_constant_index_is_refused` | fails: the file with `NO_CONST` constants decodes |
| `edge_a_tree_deeper_than_the_block_limit_still_renders_and_walks` | aborts with `has overflowed its stack` — once per recursive walk restored (`all_blocks`, `write_block`, `render_blocks`) and once per recursive `Drop` |
| `edge_the_disassembler_reports_a_block_index_out_of_range` | fails: no comment names the block |
| `every_jump_target_is_inside_its_own_block` | fails on `<` instead of `<=` |

`edge_declared_counts_larger_than_the_file_are_refused` now claims 1,000,000
constants rather than `u32::MAX`. Its assertion is unchanged and still fails on
a count past the bytes left; the reserved-index refusal is a new, separate test
rather than a shadowing one.

## Definition of done

- [x] `docs/BYTECODE.md` specifies the format, version and encoding — 375
      lines; `rb dis` output shown in it is generated by the disassembler.
- [x] `rb compile` emits `.rbc` for every `examples/*.rb` — pinned by
      `rb_compile_and_dis_cover_every_example_through_the_binary`, which runs
      the binary over the corpus and compares its bytes to the library's.
- [x] `rb dis` round-trips and its output is deterministic — pinned by
      `rb_compile_writes_a_rbc_that_rb_dis_reads_back` (two runs, byte-equal
      stdout) and `disassembly_is_a_deterministic_function_of_the_bytes`.
- [x] unknown opcode version → clean error — `edge_unknown_format_version_is_rejected`,
      `edge_unknown_opcode_byte_is_rejected`; both are `Error::Parser` with a
      message naming the version/byte, exit 1 through `rb dis`.

## Tests added

`tests/bytecode_test.rs`, 43 tests (36 from the checkpoint, 2 from the
continuation pass, 5 from review round 1).

| Test | Edge class covered |
|---|---|
| `edge_a_tree_deeper_than_the_block_limit_still_renders_and_walks` | nesting_recursion / resource_limit — **new**; 3000 levels walked on a 128 KiB stack by `all_blocks`, `encode` and `disassemble`, and freed |
| `edge_the_disassembler_reports_a_block_index_out_of_range` | out_of_bounds — **new**; a block index past the list, a parent past the pool, and `NO_BLOCK` as the legal value it is |
| `edge_a_pool_that_reaches_the_reserved_constant_index_is_refused` | malformed_input / resource_limit — **new**; the reserved index is refused from the declared count |
| `an_extending_object_names_its_parent_in_the_constant_pool` | codegen shape — **new**; the parent's name is in the pool, interned once, and says so in the disassembly |
| `a_field_default_is_compiled_before_the_declaration_that_consumes_it` | codegen shape — **new**; a non-trivial default is compiled, and a field with none is `nothing` |
| `edge_a_jump_to_the_end_of_a_block_is_the_exit_not_an_out_of_range_target` | boundary / out_of_bounds — the exit jump is `code.len()`, is documented, is not reported as out of range, and survives a round trip |
| `rb_compile_and_dis_cover_every_example_through_the_binary` | the phase's definition of done, end to end |
| `edge_empty_program_compiles_to_an_empty_main_block` | empty — zero instructions, zero constants, round trips |
| `edge_a_single_statement_program_is_not_empty` | singleton — one statement, one constant, two instructions |
| `edge_declared_counts_larger_than_the_file_are_refused` | resource_limit / out_of_bounds — a constant count and an instruction count past the bytes left |
| `edge_the_disassembler_reports_an_out_of_range_operand_instead_of_panicking` | out_of_bounds — a constant index and a jump target past the block |
| `edge_bad_magic_is_rejected` | malformed_input — asserts a failure |
| `edge_unknown_format_version_is_rejected` | malformed_input — asserts a failure |
| `edge_unknown_opcode_byte_is_rejected` | malformed_input — asserts a failure |
| `edge_every_truncation_of_a_valid_file_is_rejected_without_panicking` | malformed_input — every prefix of a valid file, and the whole file still decodes after |
| `edge_an_unassigned_byte_in_any_enum_is_rejected` | malformed_input — block kind 200, constant tag 99, yes/no byte 7 |
| `edge_trailing_bytes_after_the_last_block_are_refused` | malformed_input — one byte past the end |
| `edge_non_utf8_string_constant_is_rejected` | unicode / malformed_input |
| `edge_unicode_and_escapes_survive_the_constant_pool_byte_for_byte` | unicode — emoji, CJK, an RTL override, `\"`, `\\`, a newline |
| `edge_numeric_boundaries_survive_the_constant_pool` | numeric_boundary — `0`, `-0.0`, `9007199254740993`, `1e308`, `3.14159`, `0.1`; every one finite, and the file length is the documented size |
| `edge_duplicate_record_keys_and_missing_names_still_compile` | duplicate_missing_keys |
| `edge_deeply_nested_declarations_still_compile_within_a_bounded_block_tree` | nesting_recursion — 8 levels, one block each |
| `edge_a_program_nested_past_the_block_limit_is_refused` | nesting_recursion / resource_limit — asserts a failure |
| `edge_blocks_nested_past_the_limit_are_refused_when_decoding` | nesting_recursion / malformed_input — asserts a failure |
| `edge_an_interpolated_text_becomes_build_text_over_its_parts` | nesting_recursion — the AST node the parser cannot build (FINDINGS §2) |
| `edge_the_same_name_is_one_constant_so_the_pool_is_canonical` | determinism — one name, one constant |
| `every_example_compiles_and_encodes_deterministically` | determinism — same bytes from two compiles |
| `disassembly_is_a_deterministic_function_of_the_bytes` | determinism |
| `rb_compile_writes_a_rbc_that_rb_dis_reads_back` | determinism — two `rb dis` runs agree |
| `edge_rb_dis_refuses_a_source_file_and_a_missing_file` | asserts a failure — non-`.rbc`, and a missing file |
| `edge_rb_compile_reports_a_bad_source_and_writes_nothing` | asserts a failure — a non-parsing program leaves no file |
| `rb_compile_without_an_output_flag_writes_beside_the_source` | resource / CLI surface |
| `rb_help_documents_the_two_new_subcommands` | CLI surface |
| `compile_encode_decode_round_trips_losslessly` | format |
| `expressions_compile_to_the_documented_instruction_sequences` | codegen shape |
| `calls_carry_their_name_and_arity_and_properties_carry_their_name` | codegen shape |
| `loops_and_branches_produce_jumps_inside_their_own_block` | codegen shape |
| `every_jump_target_is_inside_its_own_block` | codegen shape / boundary — every target in range *or* the documented exit, with the exit form asserted to occur |
| `functions_tests_and_methods_become_named_blocks` | codegen shape |
| `try_compiles_to_a_handler_instruction_and_separate_blocks` | codegen shape |
| `try_without_handlers_names_no_blocks` | empty handler |
| `imports_compile_to_an_import_per_item_bound_to_its_alias` | codegen shape |

Quota: 43 `#[test]` functions (floor 3); 25 named `edge_*` (floor 1); 15
assert a failure is produced (floor 1); 0 new `#[ignore]`, `// skip`, or
`allow(clippy::` suppressions; 0 pre-existing tests failing.

### Mandatory edge-case matrix

Every row is covered. The 11 rows and where each is pinned:

- **empty** — `edge_empty_program_compiles_to_an_empty_main_block`,
  `try_without_handlers_names_no_blocks`
- **singleton** — `edge_a_single_statement_program_is_not_empty`
- **boundary** — `edge_a_jump_to_the_end_of_a_block_is_the_exit_not_an_out_of_range_target`
  (target == instruction count, the boundary of the legal range),
  `every_jump_target_is_inside_its_own_block` (the same boundary, asserted to
  be reached twice), and `edge_numeric_boundaries_survive_the_constant_pool`
  (`0`, `-0.0`, `2^53+1`)
- **out_of_bounds** — `edge_the_disassembler_reports_an_out_of_range_operand_instead_of_panicking`,
  `edge_the_disassembler_reports_a_block_index_out_of_range`,
  `edge_declared_counts_larger_than_the_file_are_refused`
- **type_mismatch** — the format is untyped, so there is no runtime type check
  to fail here. What *is* refused is a mistyped tag: a constant that is not one
  of `Number/Text/YesNo/Nothing`, a `yes`/`no` byte that is not 0 or 1, and a
  block kind this build does not assign —
  `edge_an_unassigned_byte_in_any_enum_is_rejected`.
- **numeric_boundary** — `edge_numeric_boundaries_survive_the_constant_pool`
- **unicode** — `edge_unicode_and_escapes_survive_the_constant_pool_byte_for_byte`,
  `edge_non_utf8_string_constant_is_rejected`
- **nesting_recursion** — `edge_deeply_nested_declarations_still_compile_within_a_bounded_block_tree`,
  `edge_a_program_nested_past_the_block_limit_is_refused`,
  `edge_blocks_nested_past_the_limit_are_refused_when_decoding`,
  `edge_a_tree_deeper_than_the_block_limit_still_renders_and_walks`
- **duplicate_missing_keys** — `edge_duplicate_record_keys_and_missing_names_still_compile`
- **malformed_input** — the eight `edge_*` malformed-input tests listed above
- **resource_limit** — `edge_declared_counts_larger_than_the_file_are_refused`
  (a count past the bytes left is refused *before* `Vec::with_capacity`),
  `edge_a_pool_that_reaches_the_reserved_constant_index_is_refused`,
  `MAX_BLOCK_DEPTH` in the three nesting tests, and the existing
  `call_depth_test` / `loop_bounds_test` suites that bound execution

## Gates

Run after review round 1, on the tree this report describes:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 433 passed, 0 failed, 0 ignored (43 in `bytecode_test`) |
| `cargo test --doc` | 1 passed (`src/bytecode/mod.rs` round-trip example) |
| `./rbops/verify.sh phase-018` | **not run** — `rbops/` is not in this checkout: `ls: cannot access 'rbops/verify.sh': No such file or directory`. Recorded as not run, not as passed. See FINDINGS §4. |
| `rb run examples/*.rb` (backwards compatibility) | 6/6 ok |

## Invariants touched

- **The bytecode format version moved from 1 to 2.** That is the one thing that
  changed for something outside this phase: a `.rbc` written by the phase-018
  checkpoint is refused by this build with
  `unknown bytecode format version 1: this build reads version 2`. Rule 2 of
  `docs/BYTECODE.md` requires it, because `DEF_OBJECT`'s second operand and
  `DEF_FIELD`'s stack effect both changed meaning; nothing else in the file's
  layout, and no byte value, moved.
- No language surface changed: `.rb` is still the extension, `say` still
  prints, `to … end` / `if … end` / `for … end` are untouched, `Value`
  and `Error` are unchanged, and `rb run` still tree-walks. `rb compile` and
  `rb dis` read source and write a file; nothing existing routes through them,
  which is why all 6 examples still run unchanged.
- The tree-walking VM is untouched. `DEF_FIELD` popping a value is a fact about
  the file format; stage S1b is where a VM consumes it.

## Known gaps / follow-ups

- Nothing runs a `.rbc`. `rb vm` does not exist. This is stage S1a and the
  ladder starts at S1b. Recorded in FINDINGS §5.
- `modules/MathUtils.rb` does not parse, so `rb compile` cannot compile it
  (`constant … to …` is not in the lexer). Predates this phase —
  `rb run modules/MathUtils.rb` fails identically. FINDINGS §1.
- `Expr::InterpolatedText` is unreachable from source; the compiler lowers it
  correctly and a constructed AST pins that. FINDINGS §2.
- Nothing decides what a `.rbc` VM should do with a parent that is not
  declared. The tree-walking VM reports it at run time, and the file now
  carries the parent's name for a VM to do the same. FINDINGS §5.