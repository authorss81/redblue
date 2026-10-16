# Phase 058 — Show the compiler inside the REPL: `.bytecode` and `.asm`

## Reproduce (before the change)

At the parent of this phase's work (`37fa537`):

```
$ git show 37fa537:src/repl/commands.rs | grep -ci 'bytecode\|"asm"'
0
```

`bytecode::disassemble` existed (`src/bytecode/disasm.rs`) and `rb dis` called
it, but no REPL command reached it, so *what does this compile to* meant leaving
the session, writing a file, compiling it and disassembling the artifact by
hand. Reproduced against the merge base rather than at the prompt because the
change is already merged in this tree — see "Resumed work" below.

## The surface

```
>>> .bytecode say 1
bytecode: `say 1`
; redblue bytecode v6
main (main, arity 0)
0000  NOP
0001  PUSH_CONST       0
0002  SAY

>>> .asm say 1
asm: `say 1`
; redblue bytecode v6
; constants: 1
;   [0] 1

main (main, arity 0)
0000  NOP                ; line 1: start of a statement
0001  PUSH_CONST       0  ; line 1: 1
0002  SAY                ; line 1

>>> .asm might fail 1 +
>>> ParserError: Unexpected token Eof
  --> 1:15
1 | might fail 1 +
  |               ^
>>> .bytecode
>>> Usage: :bytecode <expression>
```

* Both commands compile the expression through `bytecode::compile_source` — the
  same frontend `rb compile` uses — and render through the same disassembler
  `rb dis` prints, so `.asm` at the prompt is `rb dis` byte for byte, not a
  second opinion. The header line the prompt adds is the only text that is not
  the disassembler's own.
* **Read-only.** The chunk is never handed to the session's VM and nothing is
  written to disk; it exists only as the text just printed.
* Both sigils work: `:bytecode say 1` is the same command.
* The listing carries no comments at all; the annotated form adds the constants
  table and one comment per instruction. That difference is the whole difference
  between them, and it is asserted (`asm_is_longer_than_the_listing_on_the_same_input`).
* The version line is the crate's `FORMAT_VERSION`, read rather than written out,
  so the assertion moves when the format moves.
* `.bytecode` with nothing to compile is refused by name. Whitespace compiles to
  an empty program, so a listing with no instructions in it would answer a
  question nobody asked.
* A compile error is reported as the frontend's own error against the text that
  was given (source line and caret), not as an empty listing.

## What changed

| File | Lines | What |
|---|---|---|
| `src/bytecode/disasm.rs` | +123 −30 | `render_compact`: the listing — version line, the block tree, one line per instruction, operands only. The operand spelling and block header were pulled out of `render`/`render_instruction` into `write_operands`/`write_block_header` so both renderings say the same thing; no output of `render` changed. |
| `src/bytecode/mod.rs` | +1 −0 | `pub use disasm::render_compact as disassemble_compact`. |
| `src/repl/bytecode_view.rs` | +304 (new) | `bytecode_lines` / `asm_lines`: compile through `compile_source`, render, split into lines; `compile` refuses empty input by name and returns the frontend's rendered error. No VM, no disk. |
| `src/repl/commands.rs` | +34 −0 | `ReplCommand::{Bytecode, Asm}`, two `CommandSpec` rows (`<expression>`, argument required), and `argument_text` covering both. |
| `src/repl/mod.rs` | +45 −0 | Dispatch to `print_bytecode`/`print_asm`, and `print_code_answer` — the header, then the listing or the painted refusal. |
| `tests/repl_test.rs` | +105 −0 | Four end-to-end session tests. |
| `tests/unreviewed_work_ack_test.rs` | +1 −0 | This phase's own resume commit added to the acknowledgement list, with the reason. Carried no code. |

`must_touch: ["src/"]` is satisfied: four files under `src/`, +507 lines.

## Tests added

| Test | Edge class covered |
|---|---|
| `repl::bytecode_view::tests::bytecode_names_the_format_version_and_the_constant_load` | happy path, and the version derived from `FORMAT_VERSION`; also asserts no listing line ends in padding |
| `repl::bytecode_view::tests::bytecode_lists_the_blocks_nested_inside_the_program` | nesting — a nested function's block, its arity and its instructions appear in both renderings; the listing carries no comments |
| `repl::bytecode_view::tests::asm_is_longer_than_the_listing_on_the_same_input` | boundary — annotated is strictly longer on one fixed input, by ≥ 3 lines, not by a header alone |
| `repl::bytecode_view::tests::asm_says_byte_for_byte_what_the_disassembler_says` | agreement — both renderings equal `disassemble` / `disassemble_compact` of the same chunk |
| `repl::bytecode_view::tests::edge_an_expression_that_does_not_compile_is_reported_rather_than_listed` | malformed input / failure — `might fail 1 +` reports the parser error from both commands, prints no listing, and the two agree |
| `repl::bytecode_view::tests::edge_nothing_to_compile_is_a_refusal_and_not_an_empty_listing` | empty / failure — whitespace is refused by name, not listed as an empty program |
| `repl::bytecode_view::tests::edge_a_two_thousand_line_program_renders_without_hanging` | resource limit — 2000 `say` lines render in full, exact line count (3 instructions per statement + version + block), annotated still longer |
| `repl::bytecode_view::tests::edge_text_a_terminal_would_mangle_is_escaped_rather_than_written_out` | unicode and escapes — NUL, newline, RTL override, combining acute, emoji; nothing raw reaches the terminal in either rendering |
| `tests/repl_test.rs::bytecode_shows_the_instruction_listing_the_session_compiles_to` | happy path end-to-end — session stdout carries the version, `PUSH_CONST`, `SAY`, and no unknown command |
| `tests/repl_test.rs::edge_bytecode_with_no_argument_is_told_what_it_wanted` | empty / failure — both commands print `Usage: :… <expression>` and print no listing |
| `tests/repl_test.rs::bytecode_reports_a_compile_error_instead_of_listing_nothing` | failure end-to-end — the error reaches stdout, no listing does |
| `tests/repl_test.rs::asm_at_the_prompt_agrees_with_rb_dis_on_the_same_source` | agreement end-to-end — compiles a scratch `.rb`, `rb compile` → `rb dis`, and diffs that text against what the session printed after its header |

12 new `#[test]` functions, 5 named `edge_*`, 3 asserting a failure is produced.
Two assertions were added to the existing `ReplCommand::parse` test in
`src/repl/commands.rs` (both sigils, whole expression). No test was weakened,
skipped, ignored or suppressed; `git diff` over the phase contains no `#[ignore]`,
`// skip` or `allow(clippy::`.

## Edge-case matrix

| Row | Covered by |
|---|---|
| empty / zero / nothing | `edge_nothing_to_compile_is_a_refusal_and_not_an_empty_listing`, `edge_bytecode_with_no_argument_is_told_what_it_wanted` |
| singleton and boundary | `asm_is_longer_than_the_listing_on_the_same_input` (exact line counts on one fixed input), `bytecode_names_the_format_version_and_the_constant_load` (exact first line, no trailing padding) |
| out of bounds | N/A — no indexing is introduced. The listing walks `block.blocks` by index the block already holds, and an operand that does not name a constant is already rendered as a remark by `disasm::describe`; N/A for constant-index bounds because this phase adds no operand that can be out of range |
| type mismatch | N/A — the commands never evaluate the expression, so a program that would be a runtime type error is listed as instructions; the frontend's own analysis is unchanged and owns that error |
| numeric boundary | N/A — the renderer prints operands verbatim from the chunk; `0`, `-0`, `2^53+1` and integer-overflow constants are the same text the existing `rb dis` prints. No arithmetic was added, so there is no new numeric boundary to pin |
| unicode and escapes | `edge_text_a_terminal_would_mangle_is_escaped_rather_than_written_out`. **Non-UTF-8 constant bytes are N/A**: `compile_source` takes `&str`, so a source that is not valid UTF-8 cannot reach the renderer at all — the lexer rejects it when the file is read. The reachable form of that row (control characters a terminal would mangle) is covered |
| nesting and recursion | `bytecode_lists_the_blocks_nested_inside_the_program` — the walk is the block tree, one level of nesting, in both renderings |
| duplicate and missing keys | N/A — records are not touched. A duplicate-key record compiles to the same `DEF_OBJECT`/`LOAD_CONST` pair whether or not the key repeats; there is no key table in the listing to differ |
| malformed input | `edge_an_expression_that_does_not_compile_is_reported_rather_than_listed`, `bytecode_reports_a_compile_error_instead_of_listing_nothing` (`might fail 1 +`), and empty input as above |
| resource and state | `edge_a_two_thousand_line_program_renders_without_hanging` — 2000 statements, exact output length, both renderings; nothing is written to disk and the session's VM is never handed the chunk |

## Gates

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | pass |
| `cargo clippy --all-targets -- -D warnings` | pass |
| `cargo test --all-targets` | 1732 passed, 0 failed (62 test binaries, log in `target/tmp/phase058-test.log`) |
| `cargo test --doc` | 2 passed, 0 failed |
| `./rbops/verify.sh phase-058` | **not run — `rbops/` is not in this checkout** (see below) |

`rbops/` and `rbops/verify.sh` are not present in this working tree
(`ls rbops/` → *No such file or directory*), exactly as phase-057 recorded. I
did not create, inspect or work around them. The four cargo gates above were run
here and their output is the log named above; `./rbops/verify.sh` will have to
run where it exists.

One pre-existing gate needed data before it went green: `cargo test --test
unreviewed_work_ack_test` reported

```
1 commit(s) in HEAD's history carry unreviewed work and are not on the
acknowledgement list in this file: 977218301bd20726a43a15d7fa64344939ee097f
resume phase-058: adopt preserved work
```

That is this phase's own resume commit, and the fix is the one the message asks
for: one entry in `ACKNOWLEDGED` naming what it carries (+612/−30, 6 files,
+507 lines under `src/` — the `.bytecode`/`.asm` work) and that it was
deliberately not rewritten. No assertion was touched; `validate_acknowledgements`
still rejects an entry with no reason, a duplicate, or a SHA that is not a full
object id.

## Resumed work

This phase resumed a commit that had already landed (`9772183`, the previous
attempt's code and tests). It was verified rather than rewritten: every test
above was run against that tree, the surface was exercised at the prompt by hand,
and the one thing the previous attempt had not done — `REPORT.md` — is this file.
No production code was changed by the resume; the only edit is the acknowledgement
entry above.

## Invariants touched

- None. No `Value` variant, `Error` variant, `.rb` extension, keyword or
  `set x to <expr>` / `say` / `to … end` form changed, and `rb dis`'s output is
  byte-identical — the operand and block-header refactor in `disasm.rs` moved
  text between two functions without altering what either prints, which
  `asm_says_byte_for_byte_what_the_disassembler_says` and the full `disasm`
  suite pin. `.bytecode`/`.asm` are REPL commands behind the existing sigils, so
  a language program that begins with `.`/`:` is still not a command.

## Known gaps / follow-ups

- `.bytecode`/`.asm` compile the expression from source text, so they cannot show
  what a *bound value* would compile to; unlike `.time`, they cannot read a
  session binding. That is inherent to the question, not a gap.
- The listing is unbounded — a large program prints a line per instruction. The
  2000-line case is asserted to render in full and fast; an output cap would be a
  new truncation rule and is recorded in `FINDINGS.md` rather than invented here.
- `.bytecode` is a compact listing of the same chunk `rb dis` prints; it is not a
  machine-readable form. If a `.bytecode --json` is ever wanted, that is
  formatting output on stdout, not a change to the chunk.