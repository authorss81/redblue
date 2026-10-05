//! Turning bytecode back into text.
//!
//! The disassembler is the read side of `docs/BYTECODE.md`: it is what a
//! person or a test reads a `.rbc` file through. It is a pure function of the
//! chunk — it walks blocks and instructions in index order and never sorts or
//! hashes — so `rb dis` prints the same bytes for the same file every time.
//!
//! Every index it prints is checked. A hand-built or corrupt chunk with a jump
//! past the end of its block prints `?` rather than indexing out of range.

use std::fmt::Write as _;

use crate::bytecode::format::{Block, Chunk, Constant};
use crate::bytecode::opcode::Opcode;

/// The width the mnemonic column is padded to.
///
/// Taken from the whole opcode table rather than from the opcodes a particular
/// chunk happens to use, so a column does not shift with the program.
const MNEMONIC_WIDTH: usize = 17;

/// Renders `chunk` as disassembly.
pub fn render(chunk: &Chunk) -> String {
    let mut out = String::new();

    let _ = writeln!(
        out,
        "; redblue bytecode v{}",
        crate::bytecode::FORMAT_VERSION
    );
    let _ = writeln!(out, "; constants: {}", chunk.constants.len());
    for (index, constant) in chunk.constants.iter().enumerate() {
        let _ = writeln!(out, ";   [{index}] {constant}");
    }
    let _ = writeln!(out);

    render_block(&mut out, chunk, &chunk.main, "main", 0);
    out
}

fn render_block(out: &mut String, chunk: &Chunk, block: &Block, path: &str, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
    let _ = writeln!(
        out,
        "{} ({}, arity {})",
        path,
        block.kind.name(),
        block.arity
    );

    for (offset, instruction) in block.code.iter().enumerate() {
        let opcode = instruction.opcode;
        let mut line = format!("{offset:04}  {:<MNEMONIC_WIDTH$}", opcode.name());

        if opcode.has_operand() {
            let _ = write!(line, "{}", instruction.arg);
            if opcode.has_aux() {
                let _ = write!(line, ", {}", instruction.aux);
            }
        }

        if let Some(comment) = describe(chunk, block, instruction) {
            let _ = write!(line, "  ; {comment}");
        }

        out.push_str(&line);
        out.push('\n');
    }

    for (index, child) in block.blocks.iter().enumerate() {
        let _ = writeln!(out);
        let child_path = format!("{path}.blocks[{index}]");
        render_block(out, chunk, child, &child_path, depth + 1);
    }
}

/// The trailing comment: the source line the instruction came from, and the
/// value an operand names when it names one.
fn describe(
    chunk: &Chunk,
    block: &Block,
    instruction: &crate::bytecode::Instruction,
) -> Option<String> {
    let mut comment = format!("line {}", instruction.line);

    if instruction.opcode.takes_constant_index() {
        match chunk.constants.get(instruction.arg as usize) {
            Some(Constant::Text(text)) => {
                let _ = write!(comment, ": {}", text.escape_debug());
            }
            Some(constant) => {
                let _ = write!(comment, ": {constant}");
            }
            None => {
                let _ = write!(
                    comment,
                    ": constant {} is out of range (the pool holds {})",
                    instruction.arg,
                    chunk.constants.len()
                );
            }
        }
    }

    if matches!(instruction.opcode, Opcode::Jump | Opcode::JumpIfFalse)
        && (instruction.arg as usize) >= block.code.len()
    {
        let _ = write!(
            comment,
            ": target {} is outside this block's {} instructions",
            instruction.arg,
            block.code.len()
        );
    }

    Some(comment)
}
