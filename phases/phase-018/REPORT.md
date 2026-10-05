# Phase 018 — Bytecode format and disassembler (bootstrap S1a)

## Finding reproduced

```
$ cargo run --bin rb -- compile examples/hello.rb
Redblue v0.1.0 - A programming language as readable as plain English

Usage:
  rb              Start interactive REPL
  rb <file>      Run a Redblue file
  rb run <file>  Run a Redblue file
  …
$ echo $?
0
```

`rb compile` was not a subcommand: the fourth argument of `rb` was treated as an
unknown command and printed the help, exiting **0**. There was no `bytecode`
module in `src/`, no `.rbc` file format, no disassembler and no version field
anywhere:

```
$ grep -rn "bytecode\|MAGIC\|FORMAT_VERSION" src/ | wc -l
0
```

Redblue was tree-walking only, which is what S1a exists to change. The finding
reproduces.

## Red test, before any production change

`tests/bytecode_test.rs` was written first, 36 tests against an API that did
not exist. It failed to compile, for the right reason:

```
error[E0432]: unresolved import `redblue::bytecode`
error[E0599]: no function or associated item named `from_byte` found for struct `Opcode`
```

After the change the same file passes, and each test was checked to fail for
its own reason by breaking the thing it names. Two examples, both reverted and
re-run:

- removing `depth + 1` from the nested-block call in `src/bytecode/codegen.rs`
  makes `edge_a_program_nested_past_the_block_limit_is_refused` fail — the
  compile-depth guard was dead code until that test forced it live;
- the `-o` guard was first written against `args[2]`, which is the *source path*
  in `rb compile <file> -o <out>`, and `rb_compile_writes_a_rbc_that_rb_dis_reads_back`
  caught it (`rb compile wrote no .rbc file`).

## What changed

| File | Lines | What |
|---|---|---|
| `src/bytecode/opcode.rs` (new) | +297 | the 46 opcodes with **explicit, stable byte values**, `from_byte`/`to_byte`, `name`, `has_operand`/`has_aux`/`takes_constant_index` |
| `src/bytecode/format.rs` (new) | +497 | `MAGIC`, `FORMAT_VERSION`, `Constant`, `Instruction`, `Block`, `BlockKind`, `Chunk::encode`, `Chunk::decode`, `Chunk::compile`, the bounds-checked `Reader`, `NO_BLOCK`, `MAX_BLOCK_DEPTH`, `INSTRUCTION_SIZE` |
| `src/bytecode/codegen.rs` (new) | +530 | AST → `Chunk`: one pass, one instruction list per block, jump patching, interned names, `Decl` for the six constructs that own a body |
| `src/bytecode/disasm.rs` (new) | +119 | `disassemble`: header, constant pool, per-block listing, per-operand comments, every printed index checked |
| `src/bytecode/mod.rs` (new) | +33 | the module, its re-exports, and a doc-test that compiles, encodes, decodes and disassembles |
| `src/lib.rs` | +61 −0 | `pub mod bytecode`, `compile_source`/`Chunk` re-exports, `rb compile <file> [-o out.rbc]`, `rb dis <file.rbc>`, `bytecode_path_for`, `compile_command`, `dis_command`, help text |
| `docs/BYTECODE.md` (new) | +301 | the normative format: layout, every field, the full opcode table with stack effects, per-statement lowering, what a decoder must refuse, what `rb dis` prints |
| `tests/bytecode_test.rs` (new) | +1233 | 38 tests, 21 of them `edge_*` |
| `phases/phase-018/FINDINGS.md` (new) | +75 | five findings, none of them fixed here |

### The format, in one paragraph

A `.rbc` is a packed little-endian byte stream: the 4-byte magic `RED\x1a`, a
`u16` format version, a `u32` constant count, the constant pool, and one block
tree. Each block is a kind byte, a `u32` arity, a length-prefixed UTF-8 name, a
`u32` instruction count, that many **fixed 13-byte** instruction records
(`opcode: u8`, `arg: u32`, `aux: u32`, `line: u32`), a `u32` child count, and
its children. There is one constant pool per file, shared by every block, so a
name used inside a function body is one entry.

### What makes the format stable rather than merely defined

- **Opcode byte values are part of the file.** Every variant carries its byte
  written out (`Nop = 0,` … `Expect = 45,`), so inserting one between two
  others is a deliberate act rather than an accident of declaration order.
  `Opcode::ALL` is the table in byte order and `from_byte` reads it, and
  `the_opcode_table_in_the_spec_is_the_opcode_table_in_the_code` asserts the
  code against **the opcode table in `docs/BYTECODE.md`** and that the bytes
  are consecutive from zero — so neither the code nor the spec can drift from
  the other or from the file. Verified by swapping two rows of the spec's table
  and watching that test fail.
- **A retired byte is reserved, never reused.** Written into the module docs and
  into `docs/BYTECODE.md`.
- **Anything that would make an older file decode differently bumps
  `FORMAT_VERSION`.** A reader refuses an unknown version rather than guessing
  (`edge_unknown_format_version_is_rejected`).
- **Jumps are block-local.** A block is the unit a jump target is resolved
  against, which is what lets a decoder check every target against a length it
  already knows.

### Why names and not slots

`LOAD`/`STORE` carry a name, not a slot number. A Redblue function closes over
the local scopes live where it was declared (`src/value.rs:29-41`), so a name
has to survive into the file for a VM to resolve it against those captured
scopes. Slot allocation is a later stage and a version bump, because it changes
what a version-1 file means. The one name the compiler introduces is
`$counter`; `$` is not an identifier character in the lexer, so it cannot
collide with a program-declared name.

### Determinism

Compiling the same source twice gives the same bytes, and disassembling the same
bytes gives the same text: no `HashMap` iteration, no clock, no address, no
sorting. Names are interned through an `IndexMap` in first-use order, so the
pool does not depend on which use the compiler reached first.

## Tests added

All in `tests/bytecode_test.rs`. 38 tests, 21 `edge_*`, 1 doctest.

| Test | Edge class covered |
|---|---|
| `compile_encode_decode_round_trips_losslessly` | the format: decode(encode(c)) == c **and** encode(decode(b)) == b, so the encoding is a fixed point, not just reversible |
| `every_example_compiles_and_encodes_deterministically` | backwards compatibility: all six `examples/*.rb` compile, encode twice to identical bytes, and survive a decode/encode round trip |
| `disassembly_is_a_deterministic_function_of_the_bytes` | nondeterminism: disassembled twice, and the decoded chunk disassembles identically; asserts the opcodes are actually printed |
| `edge_unknown_format_version_is_rejected` | **failure asserted**: version bumped to 2 → `Err` naming the version |
| `edge_bad_magic_is_rejected` | **failure asserted**: first byte flipped → `Err` saying the file is not bytecode |
| `edge_every_truncation_of_a_valid_file_is_rejected_without_panicking` | malformed input / out of bounds: **every** prefix shorter than the file is refused — each of the 90-odd byte offsets of a real `.rbc`, not one hand-picked |
| `edge_unknown_opcode_byte_is_rejected` | malformed input: opcode byte 255 → `Err` naming it |
| `edge_non_utf8_string_constant_is_rejected` | unicode/escapes: a byte inside a pooled text flipped to 0xFF → `Err` naming the text constant |
| `edge_declared_counts_larger_than_the_file_are_refused` | resource limit: constant count and instruction count set to `u32::MAX` → refused on the count, before any allocation |
| `expressions_compile_to_the_documented_instruction_sequences` | the codegen: the exact mnemonic sequence for list/index/record/property/`say`, and each count-carrying operand |
| `calls_carry_their_name_and_arity_and_properties_carry_their_name` | the codegen: `files.read("a")` is a `CALL_METHOD` whose `arg` names `read` in the pool and whose `aux` is 1 |
| `loops_and_branches_produce_jumps_inside_their_own_block` | the codegen: the exact sequence for `for each` + `if` + `while` + `skip`, and that exactly two jumps are backward |
| `every_jump_target_is_inside_its_own_block` | **out of bounds**: every `JUMP`/`JUMP_IF_FALSE` in every block of the tree points inside that block; walks all blocks, not just `main` |
| `functions_tests_and_methods_become_named_blocks` | nesting: a function nested in a function, an object body, a `to can` method, a `test` — names, arities, and the parent/child shape |
| `try_compiles_to_a_handler_instruction_and_separate_blocks` | nesting: `TRY`'s two operands are the indices of a catch block and a finally block, in that order |
| `try_without_handlers_names_no_blocks` | **empty**: `try` with no handler writes `NO_BLOCK` twice and creates no block |
| `imports_compile_to_an_import_per_item_bound_to_its_alias` | duplicate/missing keys: each import item is an `IMPORT` naming the module and a `STORE` naming the *alias* |
| `edge_the_same_name_is_one_constant_so_the_pool_is_canonical` | determinism: a name used three times is one pooled entry, and all three instructions point at it |
| `edge_duplicate_record_keys_and_missing_names_still_compile` | duplicate/missing keys: `{a: 1, a: 2}` is `BUILD_RECORD 2`, and the key written twice is one pooled entry |
| `edge_unicode_and_escapes_survive_the_constant_pool_byte_for_byte` | unicode: emoji, CJK, an RTL mark, an escaped quote, an escaped backslash, an escape-produced newline — all read back out of the file and re-encoded to the same bytes |
| `edge_numeric_boundaries_survive_the_constant_pool` | numeric boundary: `0`, `-0.0`, `9007199254740993`, `1e308`, `3.14159`, `0.1` — every one decodes finite, the file is exactly `46 + 13 * instructions` bytes, and `-0.0` is a `NEG` |
| `compile_refuses_a_program_the_frontend_rejects` | **failure asserted**: an unterminated string is `Err` with a message |
| `edge_deeply_nested_declarations_still_compile_within_a_bounded_block_tree` | nesting: 8 nested `to … end` produce 8 nested blocks and round-trip |
| `rb_compile_writes_a_rbc_that_rb_dis_reads_back` | the CLI: `rb compile … -o` writes a file starting with the magic, `rb dis` prints exactly `disassemble()`, and two separate processes print the same bytes |
| `edge_rb_dis_refuses_a_source_file_and_a_missing_file` | **failure asserted**: `rb dis a.rb` and `rb dis missing.rbc` both exit non-zero with a message |
| `edge_rb_compile_reports_a_bad_source_and_writes_nothing` | **failure asserted**: a program that does not parse exits non-zero, says why, and leaves **no** `.rbc` behind |
| `rb_compile_without_an_output_flag_writes_beside_the_source` | the CLI: the default output path, and that the bytes written are the documented encoding |
| `rb_help_documents_the_two_new_subcommands` | drift: `rb help` mentions `compile` and `dis` |
| `edge_an_interpolated_text_becomes_build_text_over_its_parts` | the one AST node Redblue source cannot reach (`Expr::InterpolatedText` — see FINDINGS.md §2), lowered from a constructed AST |
| `edge_a_program_nested_past_the_block_limit_is_refused` | **resource limit**: a constructed program nested past `MAX_BLOCK_DEPTH` is a diagnostic, not unbounded recursion. This test found the guard dead |
| `edge_blocks_nested_past_the_limit_are_refused_when_decoding` | resource limit: a file that nests past the limit is refused, so a decoder cannot be walked off the stack |
| `edge_an_unassigned_byte_in_any_enum_is_rejected` | malformed input: block kind 200, constant tag 99, `yes`/`no` byte 7 — each refused by name |
| `edge_trailing_bytes_after_the_last_block_are_refused` | malformed input: one byte past the end |
| `edge_the_disassembler_reports_an_out_of_range_operand_instead_of_panicking` | panic path: a hand-built chunk with `PUSH_CONST 9` (pool of 1) and `JUMP 40` (block of 2) disassembles with both reported as out of range, and does not panic |
| `the_opcode_table_in_the_spec_is_the_opcode_table_in_the_code` | drift: the 46 opcodes, byte values and mnemonics in `docs/BYTECODE.md` are the ones the code writes, and the bytes are consecutive from zero so no variant can be slipped in between two others |
| `all_blocks_reaches_every_block_of_a_nested_program` | nesting: every block of an eight-block tree is reached exactly once, including both `try` handlers |
| `edge_empty_program_compiles_to_an_empty_main_block` | **empty**: `""` → no instructions, no constants, no blocks; still encodes, round-trips, and disassembles with its version header |
| `edge_a_single_statement_program_is_not_empty` | singleton: one statement is exactly two instructions and one constant |

### Mandatory edge-case matrix

| Row | Status |
|---|---|
| empty / zero / "nothing" | covered — `edge_empty_program_compiles_to_an_empty_main_block` (empty source: no instructions, no constants, still a valid file), `try_without_handlers_names_no_blocks` (`NO_BLOCK` operands), `edge_unicode_…` (empty-ish text, the `""` in the escaped-quote case), `edge_numeric_…` (`0`) |
| singleton and boundary | covered — `edge_a_single_statement_program_is_not_empty` (exactly one statement → exactly two instructions, one constant), `edge_the_same_name_is_one_constant…` (one pool entry for three uses), `edge_duplicate_record_keys…` (one pair and two pairs), `every_jump_target_is_inside_its_own_block` (a jump to the **last** instruction, `JUMP_IF_FALSE` to `end`, is in range — the assertion is `< code.len()`, not `<=`) |
| out of bounds | covered — `every_jump_target_is_inside_its_own_block` (a jump target past the block is impossible from the compiler and is reported, not read, when constructed by hand); `edge_the_disassembler_reports_an_out_of_range_operand_instead_of_panicking` (constant index 9 in a pool of 1, jump 40 in a block of 2); `edge_every_truncation_of_a_valid_file_is_rejected_without_panicking` (every prefix, including one byte short of every field); `edge_declared_counts_larger_than_the_file_are_refused` (`u32::MAX` counts) |
| type mismatch | **N/A** — this phase produces and reads a file; no runtime value is ever combined. The nearest analogue is the decoder meeting a tagged byte it does not assign, which is a mismatch between the file's claim and the format's vocabulary, and it is covered by `edge_an_unassigned_byte_in_any_enum_is_rejected` and `edge_bad_magic_is_rejected` |
| numeric boundary | covered — `edge_numeric_boundaries_survive_the_constant_pool`: `0`, `-0.0`, `0.1`, `3.14159`, `9007199254740993` (past `MAX_EXACT_INT`), `1e308` (finite but at the edge), each asserted to decode finite; `-0.0` asserted to be a `NEG`; `1e308 * 1e308` needs no case because `Value::number` (`src/value.rs:181`) already refuses non-finite numbers before a constant is ever built, so no constant can hold `Infinity` or `NaN`. `Constant::Number` is public, so `Constant`'s `PartialEq` compares `f64` **bits** — a `NaN` round-trips as itself instead of comparing unequal to itself |
| unicode and escapes | covered — `edge_unicode_and_escapes_survive_the_constant_pool_byte_for_byte` (emoji, CJK, RTL mark U+202E, `\"`, `\\`, escape-produced newline), `edge_non_utf8_string_constant_is_rejected` (bytes that are not UTF-8 at all) |
| nesting and recursion | covered — `functions_tests_and_methods_become_named_blocks` (a function inside a function, an object body holding a method, a test), `try_compiles_to_a_handler_instruction_and_separate_blocks`, `edge_deeply_nested_declarations_still_compile_within_a_bounded_block_tree` (8 deep), and both directions of the limit: `edge_a_program_nested_past_the_block_limit_is_refused` (compiling) and `edge_blocks_nested_past_the_limit_are_refused_when_decoding` (decoding). `all_blocks_reaches_every_block_of_a_nested_program` walks a tree holding a function in a function, an object method, a test and both of a `try`'s handlers |
| duplicate and missing keys | covered — `edge_the_same_name_is_one_constant_so_the_pool_is_canonical` (a repeated name), `edge_duplicate_record_keys_and_missing_names_still_compile` (a record with a repeated key and a read of a field that is not there), `imports_compile_to_…` (a module name and an alias that differ) |
| malformed input | covered — every prefix of a real file refused; bad magic; wrong version; unknown opcode, block-kind, constant-tag and `yes`/`no` bytes; non-UTF-8 text; a trailing byte; `compile_refuses_a_program_the_frontend_rejects` and `edge_rb_compile_reports_a_bad_source_and_writes_nothing` (the frontend side), including that a failed compile leaves no file behind |
| resource and state | covered — `edge_declared_counts_larger_than_the_file_are_refused` (a declared count is checked against the bytes left *before* `Vec::with_capacity`, so a 4-billion claim cannot become a 4-billion-element allocation), `edge_blocks_nested_past_the_limit_are_refused_when_decoding` and `edge_a_program_nested_past_the_block_limit_is_refused` (both recursion directions bounded at 64), `edge_the_disassembler_reports_an_out_of_range_operand_instead_of_panicking`. Tests write only under `target/tmp/bytecode-test/`, never the system temp dir. No test reads the clock, the network, or a file outside the checkout |

## Gates

Run in the project checkout, 2026-10-05.

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test` | **380 passed, 0 failed** (38 new in `bytecode_test.rs`, 21 of them `edge_*`, plus 1 new doctest) |
| `./rbops/verify.sh phase-018` | **NOT RUN** — `rbops/` is not present in this checkout (`ls: cannot access 'rbops/verify.sh'`). Same as phase-017; the dispatcher must run it. See FINDINGS.md §4 |
| `examples/*.rb` compile (`rb compile … -o …`) | 6/6 |
| `examples/*.rb` disassemble deterministically (`rb dis` twice, byte-compare) | 6/6 |
| `examples/*.rb` still run (`rb run`) | 4/4 run; `files.rb`, `formats.rb` and `time.rb` skipped by the existing convention — they write files, do network I/O and read the clock, which `tests/formatter_test.rs:859` already excludes |

Zero new `#[ignore]` or `allow(clippy::` attributes anywhere in the new files.
The only `skip` occurrences in them are the Redblue `skip` keyword inside test
source strings and one comment — no test is skipped. Zero pre-existing tests
were modified, skipped or deleted: `git diff --stat` over tracked files is
`src/lib.rs | 61 +++`, additive only.

## Invariants touched

- **None.** No grammar change, no `Value` variant added, no `Error` variant
  added, `.rb` unchanged, `set … to` / `say` / `to … end` unchanged, and no
  existing `rb` subcommand altered.
- Additive only: a `bytecode` module, two subcommands, two re-exports and two
  help lines.
- `pub use bytecode::{compile_source, Chunk}` in `src/lib.rs` adds to the
  public API; nothing is removed or re-typed.
- Decode failures are `Error::Parser` with `Span::unknown()`. That is a
  *pre-existing* variant used for a new kind of input (a `.rbc` rather than
  `.rb`); no new error type enters the public API.

## Known gaps / follow-ups

- **Nothing runs a `.rbc`.** `rb vm file.rbc` does not exist. The ladder in
  `AGENTS.md` §7 continues at S1b → FINDINGS.md §5
- `Expr::InterpolatedText` is unreachable from Redblue source — the parser never
  builds it, so `BUILD_TEXT` is only reachable from a constructed AST. `say
  "n is {n}"` prints the braces. Not a bytecode bug → FINDINGS.md §2
- `modules/MathUtils.rb` cannot be compiled: `constant PI to 3.14159` does not
  parse, because `constant` is not a keyword. `rb run` fails on it identically,
  so it predates this phase → FINDINGS.md §1
- `AGENTS.md` documents `import a, b as c`; the parser accepts `import a to c` →
  FINDINGS.md §3
- `BREAK`/`SKIP` semantics, where a loop releases its iterator, and what value a
  `catch` block is entered with are written down as requirements for S1b and are
  **not** implemented → FINDINGS.md §5
- Slot resolution for names is deferred to a later stage and a version bump, by
  design