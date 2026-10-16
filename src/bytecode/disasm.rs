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
use crate::bytecode::opcode::{Opcode, END_TRY_MARKER, MIGHT_FAIL_END_MARKER, STATEMENT_MARKER};

/// The width the mnemonic column is padded to.
///
/// Taken from the whole opcode table rather than from the opcodes a particular
/// chunk happens to use, so a column does not shift with the program.
const MNEMONIC_WIDTH: usize = 17;

/// Renders `chunk` as a compact listing: the version, then one line per
/// instruction, and nothing else.
///
/// The same walk and the same operand spellings as [`render`] — the listing is
/// not a second opinion about what a chunk holds — with the constants table and
/// every trailing comment left out. Those two are what make the annotated form
/// longer than the listing, which is the whole difference between the two: this
/// is what the program is made of, and [`render`] is what each of those pieces
/// came from.
pub fn render_compact(chunk: &Chunk) -> String {
    let mut out = String::new();

    let _ = writeln!(
        out,
        "; redblue bytecode v{}",
        crate::bytecode::FORMAT_VERSION
    );

    render_compact_blocks(&mut out, &chunk.main);
    out
}

/// Renders `main` and every block nested in it as a compact listing.
fn render_compact_blocks(out: &mut String, main: &Block) {
    let mut pending = vec![Pending {
        block: main,
        path: "main".to_string(),
        depth: 0,
        blank_before: true,
    }];

    while let Some(item) = pending.pop() {
        let Pending {
            block,
            path,
            depth,
            blank_before: _,
        } = item;

        for _ in 0..depth {
            out.push_str("  ");
        }
        write_block_header(out, &path, block);

        for (offset, instruction) in block.code.iter().enumerate() {
            out.push_str(&compact_instruction(instruction, offset));
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

/// One listing line: the offset, the mnemonic, and the operands, with no
/// comment.
///
/// The mnemonic column keeps its padding, and the padding an instruction with no
/// operands would end with is trimmed off: the listing is read on a terminal,
/// where a line's trailing spaces are invisible and are only ever a difference
/// between this text and what a file holding it would compare equal to.
fn compact_instruction(instruction: &Instruction, offset: usize) -> String {
    let opcode = instruction.opcode;
    let mut line = format!("{offset:04}  {:<MNEMONIC_WIDTH$}", opcode.name());

    write_operands(&mut line, instruction);
    line.push('\n');
    line.trim_end().to_string() + "\n"
}

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
        write_block_header(out, &path, block);

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

/// The block's own header line: where it is in the tree, what kind it is, how
/// many arguments it takes, and what they are called.
///
/// Shared with the compact listing, so a block is named the same way in both.
fn write_block_header(out: &mut String, path: &str, block: &Block) {
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

    write_operands(&mut line, instruction);

    let _ = write!(line, "  ; {}", describe(chunk, block, instruction));

    line.push('\n');
    line
}

/// The operands of `instruction`, in the spelling both renderings use.
///
/// The reserved operands are printed by name rather than as the index they are
/// stored as, because the number would bury the one fact a reader wants: what
/// the instruction does when there is no parent, no alias, or nothing defined.
fn write_operands(line: &mut String, instruction: &Instruction) {
    let opcode = instruction.opcode;
    if !opcode.has_operand() {
        return;
    }
    let _ = write!(line, "{}", instruction.arg);
    if opcode.has_aux() {
        // `DEF_OBJECT`'s second operand is a name or the reserved index
        // that means "extends nothing".
        if opcode == Opcode::DefObject && instruction.aux == NO_CONST {
            let _ = write!(line, ", none");
        } else {
            let _ = write!(line, ", {}", instruction.aux);
        }
    } else if opcode == Opcode::Export && instruction.aux == NO_CONST {
        // `EXPORT` carries its one operand in `arg` and a refusal marker in
        // `aux`, so the marker is printed here rather than as a second index
        // that would read as one — the same treatment `DEF_OBJECT`'s reserved
        // value gets above.
        let _ = write!(line, ", not defined");
    }
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

    // An `IMPORT`'s second operand is the alias it binds, which is a name for a
    // reader exactly as the parent above is: without it the line says what was
    // imported and not what it was imported *as*.
    if instruction.opcode == Opcode::Import {
        clauses.push(named_constant(chunk, instruction.aux, "as "));
    }

    if instruction.opcode.takes_block_index() {
        clauses.extend(named_block(block, instruction.arg, ""));
    }
    if instruction.opcode == Opcode::Try {
        clauses.extend(named_block(block, instruction.arg, "catch "));
        clauses.extend(named_block(block, instruction.aux, "finally "));
    }

    // An `EXPORT` names what a module declaration publishes, and its second
    // operand is the reserved index when the declaration does not define that
    // name — which is the one fact a reader wants from the line, and which
    // printing as a four-billion-and-something index would bury.
    if instruction.opcode == Opcode::Export {
        clauses.push(named_constant(chunk, instruction.arg, ""));
        if instruction.aux == NO_CONST {
            clauses.push("which the module does not define".to_string());
        }
    }

    // A `NOP` is the filler, and the one thing it is not is noise: this is where
    // the two markers a `NOP` can carry are shown, so a reader looking for where
    // a statement begins — or where a `try` stops being protected — can see it
    // here rather than inferring it.
    if instruction.opcode == Opcode::Nop && instruction.arg == END_TRY_MARKER {
        clauses.push("end of a protected region".to_string());
    }
    if instruction.opcode == Opcode::Nop && instruction.arg == STATEMENT_MARKER {
        clauses.push("start of a statement".to_string());
    }
    if instruction.opcode == Opcode::Nop && instruction.arg == MIGHT_FAIL_END_MARKER {
        clauses.push("end of a `might fail` region".to_string());
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
