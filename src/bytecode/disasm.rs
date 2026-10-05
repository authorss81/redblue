//! Turning bytecode back into text.
//!
//! The disassembler is the read side of `docs/BYTECODE.md`: it is what a
//! person or a test reads a `.rbc` file through. It is a pure function of the
//! chunk — it walks blocks and instructions in index order and never sorts or
//! hashes — so `rb dis` prints the same bytes for the same file every time.
//!
//! Every index it prints is checked: a constant index, a block index, the
//! object a `DEF_OBJECT` extends, and a jump target are each resolved against
//! the thing they index, and one that is out of range says so in the
//! instruction's comment rather than indexing out of range. The walk over the
//! block tree keeps its pending blocks on the heap, so it renders a tree far
//! deeper than the compiler emits rather than running out of stack.

use std::fmt::Write as _;

use crate::bytecode::format::{Block, Chunk, Constant, Instruction, NO_BLOCK, NO_CONST};
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

    render_blocks(&mut out, chunk, &chunk.main);
    out
}

/// A block still to render, and where it lands in the output.
struct Pending<'a> {
    block: &'a Block,
    /// `main`, then `main.blocks[0]`, then `main.blocks[0].blocks[1]`.
    path: String,
    depth: usize,
    /// Whether a blank line comes before this block. Every nested block has
    /// one; `main` has nothing in front of it.
    blank_before: bool,
}

/// Renders `main` and every block nested in it, depth first.
///
/// The pending blocks are held in a `Vec` rather than in a call frame per level,
/// so the cost is the tree and not the machine's stack. Output is identical
/// either way; only the bookkeeping differs.
fn render_blocks(out: &mut String, chunk: &Chunk, main: &Block) {
    let mut pending = vec![Pending {
        block: main,
        path: "main".to_string(),
        depth: 0,
        blank_before: false,
    }];

    while let Some(item) = pending.pop() {
        if item.blank_before {
            out.push('\n');
        }
        let Pending {
            block,
            path,
            depth,
            blank_before: _,
        } = item;

        for _ in 0..depth {
            out.push_str("  ");
        }
        let params = if block.params.is_empty() {
            String::new()
        } else {
            format!(" ({})", block.params.join(", "))
        };
        let _ = writeln!(
            out,
            "{path} ({}, arity {}){params}",
            block.kind.name(),
            block.arity
        );

        for (offset, instruction) in block.code.iter().enumerate() {
            out.push_str(&render_instruction(chunk, block, offset, instruction));
        }

        for (index, child) in block.blocks.iter().enumerate().rev() {
            pending.push(Pending {
                block: child,
                path: format!("{path}.blocks[{index}]"),
                depth: depth + 1,
                blank_before: true,
            });
        }
    }
}

/// One instruction line: the offset, the mnemonic, the operands, and the
/// comment.
fn render_instruction(
    chunk: &Chunk,
    block: &Block,
    offset: usize,
    instruction: &Instruction,
) -> String {
    let opcode = instruction.opcode;
    let mut line = format!("{offset:04}  {:<MNEMONIC_WIDTH$}", opcode.name());

    if opcode.has_operand() {
        let _ = write!(line, "{}", instruction.arg);
        if opcode.has_aux() {
            // `DEF_OBJECT`'s second operand is a name or the reserved index
            // that means "extends nothing". Printing the reserved index as a
            // number would bury the one fact a reader wants from the line.
            if opcode == Opcode::DefObject && instruction.aux == NO_CONST {
                let _ = write!(line, ", none");
            } else {
                let _ = write!(line, ", {}", instruction.aux);
            }
        }
    }

    let _ = write!(line, "  ; {}", describe(chunk, block, instruction));

    line.push('\n');
    line
}

/// The trailing comment: the source line the instruction came from, then what
/// each operand names when it names something, then where a value is out of
/// range.
fn describe(chunk: &Chunk, block: &Block, instruction: &Instruction) -> String {
    let mut clauses = Vec::new();

    if instruction.opcode.takes_constant_index() {
        clauses.push(named_constant(chunk, instruction.arg, ""));
    }

    // `DEF_OBJECT`'s parent is a name, so it is a constant index too — in
    // `aux`, and reserved when the object extends nothing.
    if instruction.opcode == Opcode::DefObject && instruction.aux != NO_CONST {
        clauses.push(named_constant(chunk, instruction.aux, "extends "));
    }

    if instruction.opcode.takes_block_index() {
        clauses.extend(named_block(block, instruction.arg, ""));
    }
    if instruction.opcode == Opcode::Try {
        clauses.extend(named_block(block, instruction.arg, "catch "));
        clauses.extend(named_block(block, instruction.aux, "finally "));
    }

    if matches!(instruction.opcode, Opcode::Jump | Opcode::JumpIfFalse) {
        let target = instruction.arg as usize;
        if target == block.code.len() {
            // A jump to one past the last instruction is how a loop or an `if`
            // at the end of a block leaves it. It is a target, not a defect.
            clauses.push("end of block".to_string());
        } else if target > block.code.len() {
            clauses.push(format!(
                "target {} is outside this block's {} instructions",
                instruction.arg,
                block.code.len()
            ));
        }
    }

    if clauses.is_empty() {
        return format!("line {}", instruction.line);
    }
    format!("line {}: {}", instruction.line, clauses.join("; "))
}

/// What the constant at `index` holds, or that the index is past the end of the
/// pool. `what` is what the constant is for, so the clause reads as a
/// sentence: `extends Thing`.
fn named_constant(chunk: &Chunk, index: u32, what: &str) -> String {
    match chunk.constants.get(index as usize) {
        Some(Constant::Text(text)) => format!("{what}{}", text.escape_debug()),
        Some(constant) => format!("{what}{constant}"),
        None => format!(
            "constant {index} is out of range (the pool holds {})",
            chunk.constants.len()
        ),
    }
}

/// What the block at `index` is, or that the index is past the end of the
/// block's own list. `NO_BLOCK` is a legal operand rather than an out-of-range
/// one, so it says nothing.
fn named_block(block: &Block, index: u32, what: &str) -> Option<String> {
    match block.blocks.get(index as usize) {
        Some(child) => Some(format!(
            "{what}block {index} ({}, `{}`)",
            child.kind.name(),
            child.name
        )),
        None if index == NO_BLOCK => None,
        None => Some(format!(
            "{what}block {index} is out of range (this block holds {})",
            block.blocks.len()
        )),
    }
}
