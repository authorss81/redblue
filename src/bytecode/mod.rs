//! The Redblue bytecode format: compiler, `.rbc` encoding and disassembler.
//!
//! This is bootstrap stage **S1a**. `rb compile` turns a `.rb` program into a
//! `.rbc` file and `rb dis` prints one back as text. Nothing *runs* a `.rbc`
//! yet — that is stage S1b — so this module is the format and the tools for
//! it, not a second interpreter.
//!
//! The format itself is written down in `docs/BYTECODE.md`, which is the
//! normative description; the three submodules are the encoder, the decoder and
//! the disassembler for it.
//!
//! ```
//! use redblue::bytecode::{compile_source, disassemble, Chunk};
//!
//! let chunk = compile_source("say \"Hello, World!\"\n")?;
//! let bytes = chunk.encode();
//! assert_eq!(Chunk::decode(&bytes)?, chunk);
//! assert!(disassemble(&chunk).contains("SAY"));
//! # Ok::<(), redblue::Error>(())
//! ```

mod codegen;
mod disasm;
mod format;
mod opcode;
pub mod vm;

pub use codegen::compile as compile_program;
pub use disasm::render as disassemble;
pub use format::{
    compile_source, Block, BlockKind, Chunk, Constant, Instruction, FORMAT_VERSION,
    INSTRUCTION_SIZE, MAGIC, MAX_BLOCK_DEPTH, NO_BLOCK, NO_CONST,
};
pub use opcode::Opcode;
