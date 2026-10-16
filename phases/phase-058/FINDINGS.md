# Phase 058 — FINDINGS

Work found while doing phase-058 that does not belong to this phase. Recorded
here so the auditor can promote it rather than have it re-derived. Nothing below
was fixed in this phase.

1. **The `.bytecode` listing is uncapped.** `.bytecode` prints one line per
   instruction for an expression of any length, and
   `disasm::render_compact` (`src/bytecode/disasm.rs`) has no limit either. A
   pasted 2000-line program renders in full and quickly (asserted in
   `edge_a_two_thousand_line_program_renders_without_hanging`), so this is not a
   hang today, but a REPL command with no output ceiling is the shape the
   reviewer hunts for under "unbounded resource use". A cap would have to be a
   new user-visible rule ("… 4120 more lines", or the first N and a pointer to
   `rb dis` for the rest), which is a design decision, not a bug fix. Candidate
   follow-up phase.

2. **`disassemble_compact` is a formatting function, not a second renderer.**
   It shares `write_operands` and `write_block_header` with `render` so the two
   cannot drift, and `asm_says_byte_for_byte_what_the_disassembler_says` pins
   that they agree. If a machine-readable form of the listing is ever wanted
   (`.bytecode --json`, or a `.rbc` dump tool), it belongs beside
   `render_compact` in `src/bytecode/disasm.rs` as a third projection of the same
   `Chunk`, not in the REPL.

3. **Non-UTF-8 source never reaches the renderer.** `bytecode::compile_source`
   takes a `&str`, so the "non-UTF8 bytes" row of the edge-case matrix is
   unreachable by construction for `.bytecode`/`.asm`; a file that is not valid
   UTF-8 is refused when it is read, before the frontend is called. This is noted
   in `REPORT.md` rather than worked around, and no test asserts something that
   cannot happen.

## Not a finding

The `unreviewed_work_ack_test` failure this phase hit on resume was this phase's
own `resume phase-058` commit missing from `ACKNOWLEDGED`. The test's own message
names the fix — one entry, with the reason — and that is what was added, in data,
with no assertion touched. Listed here only so the next resume does not read it
as a gate failure.