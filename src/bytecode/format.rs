//! The on-disk shape of a Redblue `.rbc` file.
//!
//! The format is documented in `docs/BYTECODE.md`. This module is the encoder
//! and the decoder for it, and the only place that knows the byte layout:
//! everything else works in terms of [`Chunk`], [`Block`] and [`Opcode`].
//!
//! Every field is read with a bounds check, and every declared count is
//! checked against the bytes actually left before anything is reserved, so a
//! corrupt or hostile file is a diagnostic rather than a panic or an
//! allocation of four billion elements.

use std::fmt;

use crate::bytecode::opcode::Opcode;
use crate::error::{Error, Result, Span};
use crate::lexer::Lexer;
use crate::parser;

/// The first four bytes of a `.rbc` file.
///
/// `0x1A` is the "end of text" control character, so a file that some tool
/// converted line endings on is detected rather than half-parsed.
pub const MAGIC: [u8; 4] = *b"RED\x1a";

/// The format version this build reads and writes.
///
/// A file whose version is anything else is refused rather than guessed at.
/// Changing anything that an older file would decode differently must bump
/// this.
///
/// Version 5 changed one thing a version-4 file reads differently, so a
/// version-4 file is refused rather than half-understood:
///
/// - every statement begins with a `NOP` whose operand is
///   [`STATEMENT_MARKER`](super::STATEMENT_MARKER), and that marker is where the
///   step budget is charged. A version-4 file has no marker, so it would be
///   charged nothing at all and a budget this build applies to every other file
///   would silently not apply to it.
///
/// Version 4 changed three things a version-3 file reads differently, so a
/// version-3 file is refused rather than half-understood:
///
/// - `IMPORT` carries the name it binds in its second operand, where version 3
///   wrote a filler `0`. An older file has no alias to say, so the `0` would be
///   read as `constants[0]` — the first name in the pool, whatever that
///   happened to be.
/// - `MODULE` names a `module NAME ... end` declaration's body block and the
///   module it declares. A version-3 file compiled the body's statements inline,
///   so a file written that way says nothing about where the module begins.
/// - `EXPORT` says what a module declaration publishes. A version-3 file has no
///   `MODULE` to read one for, so the two never meet; they are one change.
///
/// Version 3 changed three things a version-2 file reads differently, so a
/// version-2 file is refused rather than half-understood:
///
/// - a block record carries its parameter names after its arity, because a
///   `LOAD` in a function body names a parameter and a file that held only the
///   count could not say which name each argument is bound to.
/// - `CallMethod`'s constant is the dotted `receiver.method` when the receiver
///   is a name, and the bare method name when it is not. Version 2 wrote only
///   the method name, which cannot say whether `files.read` is the module
///   function `files_read` or a method on something called `files`.
/// - `SetProperty`'s constant is the dotted `object.property`, for the same
///   reason: the assignment writes the field back to the binding the name
///   denotes, and a file that held only the field name could not say which one.
///
/// Version 2 changed two operands a version-1 file reads differently, so a
/// version-1 file is refused rather than half-understood:
///
/// - `DefObject`'s secondary operand is the constant index of the object it
///   extends — [`NO_CONST`] when it extends nothing — where version 1 wrote a
///   flag that discarded the parent's name.
/// - `DefField` consumes the value pushed immediately before it, so a field's
///   `default` is compiled instead of being dropped.
pub const FORMAT_VERSION: u16 = 5;

/// The operand that says "there is no block here".
///
/// `Try` names a catch body and a finally body; a `try` with neither writes
/// this value rather than inventing an empty block.
pub const NO_BLOCK: u32 = u32::MAX;

/// The operand that says "there is no constant here".
///
/// `DefObject` names the object it extends through its secondary operand and
/// writes this value when it extends nothing. It has to be a value the pool
/// can never hold: [`Chunk::decode`] refuses a pool that reaches it, so the
/// operand is never ambiguous.
pub const NO_CONST: u32 = u32::MAX;

/// Bytes per instruction record: opcode, `arg`, `aux`, `line`.
pub const INSTRUCTION_SIZE: usize = 13;

/// The deepest block nesting a file may declare.
///
/// The compiler refuses to nest deeper than this, so a file that does is not
/// one this build wrote.
pub const MAX_BLOCK_DEPTH: usize = 64;

/// A value the program puts in the constant pool.
///
/// Numbers are held as the `f64` the frontend produced, which
/// `Value::number` has already refused to make non-finite.
#[derive(Debug, Clone)]
pub enum Constant {
    /// A number literal.
    Number(f64),
    /// A text literal, or a name or module name.
    Text(String),
    /// `yes` or `no`.
    YesNo(bool),
    /// The keyword `nothing`, and the value a `return` with no value returns.
    Nothing,
}

/// Compares numbers by their bits rather than by `f64` equality.
///
/// A derived `PartialEq` would report a `NaN` constant as different from
/// itself, so a decode of a chunk that holds one would not equal the chunk it
/// came from. No Redblue program can build a `NaN` constant — `Value::number`
/// refuses them — but `Constant` is public, so the identity a round trip needs
/// is defined here rather than assumed.
impl PartialEq for Constant {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Constant::Number(a), Constant::Number(b)) => a.to_bits() == b.to_bits(),
            (Constant::Text(a), Constant::Text(b)) => a == b,
            (Constant::YesNo(a), Constant::YesNo(b)) => a == b,
            (Constant::Nothing, Constant::Nothing) => true,
            _ => false,
        }
    }
}

impl fmt::Display for Constant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Constant::Number(n) => write!(f, "{n}"),
            Constant::Text(text) => write!(f, "{}", text.escape_debug()),
            Constant::YesNo(b) => write!(f, "{}", if *b { "yes" } else { "no" }),
            Constant::Nothing => write!(f, "nothing"),
        }
    }
}

/// One instruction: what to do, and where.
///
/// `arg` is the primary operand and `aux` the secondary one. Which of them
/// each opcode uses is documented per opcode and in `docs/BYTECODE.md`;
/// an opcode that uses one leaves the other at `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    pub opcode: Opcode,
    /// Primary operand: a constant index, a block index or a jump target.
    pub arg: u32,
    /// Secondary operand: an argument count or a second block index.
    pub aux: u32,
    /// The 1-based source line the instruction came from. `0` for a synthetic
    /// instruction the compiler inserted.
    pub line: u32,
}

/// What a block is. The kind tells a reader what a block is for; the compiler
/// emits one kind per construct that owns a body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BlockKind {
    Main = 0,
    Function,
    Method,
    Object,
    Test,
    CatchBody,
    FinallyBody,
    /// The body of a `module ... end` declaration, whose leading run of
    /// `EXPORT`s is what the declaration publishes.
    Module,
}

impl BlockKind {
    fn from_byte(byte: u8) -> Option<BlockKind> {
        match byte {
            0 => Some(BlockKind::Main),
            1 => Some(BlockKind::Function),
            2 => Some(BlockKind::Method),
            3 => Some(BlockKind::Object),
            4 => Some(BlockKind::Test),
            5 => Some(BlockKind::CatchBody),
            6 => Some(BlockKind::FinallyBody),
            7 => Some(BlockKind::Module),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            BlockKind::Main => "main",
            BlockKind::Function => "function",
            BlockKind::Method => "method",
            BlockKind::Object => "object",
            BlockKind::Test => "test",
            BlockKind::CatchBody => "catch body",
            BlockKind::FinallyBody => "finally body",
            BlockKind::Module => "module body",
        }
    }
}

/// A stretch of instructions, and the blocks nested inside it.
///
/// One block is one `to ... end`, one `object ... end`, one `test ... end`, one
/// `module ... end` or one handler of a `try`, plus the program's own `main`.
/// Instructions in a block may only jump to instructions of that same block, so
/// a block is the unit a jump table is checked against.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The declaration's name: the function, method, object or test name, and
    /// `main` for the program's own code.
    pub name: String,
    pub kind: BlockKind,
    /// How many parameters a function, method or test block takes. `0` for
    /// every other kind.
    pub arity: u32,
    /// The parameter names, in order, for a function or method block.
    ///
    /// Names and not slots, for the reason [`Opcode::Load`](crate::bytecode::Opcode::Load)
    /// keeps names: a body reads its own arguments by name, so the file has to
    /// say which name each argument is bound to. Empty for every other kind.
    pub params: Vec<String>,
    pub code: Vec<Instruction>,
    /// Nested blocks, in the order the compiler emitted them. An instruction
    /// refers to one by its index here, so this order is part of the format.
    pub blocks: Vec<Block>,
}

/// A compiled program: one constant pool and the block tree that uses it.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    /// Every value a `PUSH_CONST` can name, in the order they were first used.
    ///
    /// One pool for the whole file, shared by every block, so a name used in a
    /// function body is the same constant as the same name used outside it.
    pub constants: Vec<Constant>,
    /// The program's own instructions.
    pub main: Block,
}

/// Dropping a nested `Vec<Block>` would put one frame of drop glue on the stack
/// per level of nesting, so a tree deeper than the compiler emits — the public
/// fields let a caller build one — could abort the process in a destructor
/// rather than in a walk. The children are taken out and dropped here one at a
/// time, so the depth costs heap instead of stack.
impl Drop for Block {
    fn drop(&mut self) {
        let mut pending = std::mem::take(&mut self.blocks);
        while let Some(mut block) = pending.pop() {
            // Moved out before the block is dropped, so the drop below finds no
            // children and this implementation does not recurse.
            pending.append(&mut block.blocks);
        }
    }
}

impl Chunk {
    /// Compiles Redblue source to bytecode.
    ///
    /// Runs the whole frontend first, so a program that does not lex, parse or
    /// analyze is reported here rather than encoded.
    pub fn compile(source: &str) -> Result<Chunk> {
        let tokens = Lexer::tokenize(source)?;
        let program = parser::parse(tokens)?;
        crate::analyzer::analyze(&program)?;

        crate::bytecode::codegen::compile(&program)
    }

    /// The bytes of this chunk, in the format `docs/BYTECODE.md` specifies.
    ///
    /// Walks the block tree with an explicit stack, so a `Chunk` nested deeper
    /// than [`MAX_BLOCK_DEPTH`] — which the public fields let a caller build,
    /// and which neither the compiler nor the decoder produces — is encoded
    /// rather than being a stack overflow. The bytes are the same either way;
    /// a tree past the limit is a file this build's decoder refuses.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        write_u32(&mut out, self.constants.len() as u32);
        for constant in &self.constants {
            write_constant(&mut out, constant);
        }
        write_block_tree(&mut out, &self.main);

        out
    }

    /// Reads a `.rbc` file.
    ///
    /// Refuses a file whose magic, version, opcode or constant is not one this
    /// build knows, and a file that ends in the middle of a record, rather
    /// than reading past the end or trusting a declared count.
    pub fn decode(bytes: &[u8]) -> Result<Chunk> {
        let mut reader = Reader { bytes, pos: 0 };

        let magic = reader.take(MAGIC.len())?;
        if magic != MAGIC {
            return Err(Error::Parser(
                format!(
                    "not a Redblue bytecode file: magic is {:?}, expected {:?}",
                    String::from_utf8_lossy(magic),
                    String::from_utf8_lossy(&MAGIC)
                ),
                Span::unknown(),
            ));
        }

        let version = u16::from_le_bytes(reader.array::<2>()?);
        if version != FORMAT_VERSION {
            return Err(Error::Parser(
                format!(
                    "unknown bytecode format version {version}: this build reads version {}",
                    FORMAT_VERSION
                ),
                Span::unknown(),
            ));
        }

        // `NO_CONST` is a reserved index, so a pool that could reach it would make the
        // operand ambiguous. It is the maximum `u32`, so equality is the whole
        // of this check; it is refused from the declared count, before any
        // entry is read or anything is allocated.
        let constant_count = reader.u32()?;
        if constant_count == NO_CONST {
            return Err(Error::Parser(
                format!(
                    "a file claims {constant_count} constants, which reaches the reserved index \
                     {NO_CONST} that means 'no constant'",
                ),
                Span::unknown(),
            ));
        }
        let constants = reader.count(constant_count, "constant", 1, |reader: &mut Reader| {
            reader.constant()
        })?;

        let main = reader.block(0)?;

        if reader.pos != bytes.len() {
            return Err(Error::Parser(
                format!(
                    "{} bytes follow the last block of a Redblue bytecode file",
                    bytes.len() - reader.pos
                ),
                Span::unknown(),
            ));
        }

        Ok(Chunk { constants, main })
    }

    /// The text form of this chunk. See
    /// [`disassemble`](crate::bytecode::disassemble).
    pub fn disassemble(&self) -> String {
        crate::bytecode::disasm::render(self)
    }

    /// Every block of the tree, `main` first, then each block's own children in
    /// order.
    ///
    /// Walks with an explicit stack, so the depth of the tree is bounded by the
    /// heap rather than by the call stack.
    pub fn all_blocks(&self) -> Vec<&Block> {
        let mut all = Vec::new();
        let mut pending = vec![&self.main];
        while let Some(block) = pending.pop() {
            all.push(block);
            pending.extend(block.blocks.iter().rev());
        }
        all
    }
}

/// Compiles Redblue source to bytecode: the whole frontend, then codegen.
pub fn compile_source(source: &str) -> Result<Chunk> {
    Chunk::compile(source)
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_string(out: &mut Vec<u8>, text: &str) {
    write_u32(out, text.len() as u32);
    out.extend_from_slice(text.as_bytes());
}

fn write_constant(out: &mut Vec<u8>, constant: &Constant) {
    match constant {
        Constant::Nothing => out.push(0),
        Constant::Number(n) => {
            out.push(1);
            out.extend_from_slice(&n.to_le_bytes());
        }
        Constant::YesNo(b) => {
            out.push(2);
            out.push(u8::from(*b));
        }
        Constant::Text(text) => {
            out.push(3);
            write_string(out, text);
        }
    }
}

/// Writes `main` and every block nested in it, in the order a depth-first walk
/// visits them.
///
/// The pending blocks sit on the heap, not on the call stack: the walk costs
/// the same whether the tree is two deep or two hundred, and nothing here can
/// overflow. Children are pushed in reverse because the stack is popped from
/// its end, which makes them come out first-to-last.
fn write_block_tree(out: &mut Vec<u8>, main: &Block) {
    let mut pending = vec![main];
    while let Some(block) = pending.pop() {
        out.push(block.kind as u8);
        write_u32(out, block.arity);
        write_u32(out, block.params.len() as u32);
        for param in &block.params {
            write_string(out, param);
        }
        write_string(out, &block.name);
        write_u32(out, block.code.len() as u32);
        for instruction in &block.code {
            out.push(instruction.opcode.to_byte());
            write_u32(out, instruction.arg);
            write_u32(out, instruction.aux);
            write_u32(out, instruction.line);
        }
        write_u32(out, block.blocks.len() as u32);
        pending.extend(block.blocks.iter().rev());
    }
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

/// A cursor over the file that cannot be read past its end.
///
/// `count` is what keeps a declared length from becoming an allocation: the
/// smallest thing a single entry can be is one byte, so a count larger than
/// the bytes left is refused before `Vec::with_capacity` is reached.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

/// The failure for any field that runs off the end of the file, or that claims
/// more entries than the file has bytes for.
fn shortfall(what: &str, remaining: usize) -> Error {
    Error::Parser(
        format!("truncated bytecode file: {what} needs more than the {remaining} bytes left"),
        Span::unknown(),
    )
}

impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        if count > self.remaining() {
            return Err(shortfall(&format!("{count} bytes"), self.remaining()));
        }
        let slice = &self.bytes[self.pos..self.pos + count];
        self.pos += count;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0u8; N];
        bytes.copy_from_slice(self.take(N)?);
        Ok(bytes)
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    /// Reads `count` entries, refusing a count the file cannot hold.
    ///
    /// `per_entry` is the smallest number of bytes one entry can occupy; a
    /// count above `remaining / per_entry` is malformed, not merely large. The
    /// count itself is read by the caller, which is what lets a caller bound it
    /// before this is reached.
    fn count<T>(
        &mut self,
        count: u32,
        what: &str,
        per_entry: usize,
        mut read: impl FnMut(&mut Reader<'a>) -> Result<T>,
    ) -> Result<Vec<T>> {
        if per_entry > 0 && count as usize > self.remaining() / per_entry {
            return Err(shortfall(&format!("{count} {what}s"), self.remaining()));
        }

        let mut items = Vec::new();
        for index in 0..count {
            items.push(read(self).map_err(|e| annotate(what, index, e))?);
        }
        Ok(items)
    }

    fn string(&mut self, what: &str) -> Result<String> {
        let len = self.u32()? as usize;
        if len > self.remaining() {
            return Err(shortfall(
                &format!("a {what} of {len} bytes"),
                self.remaining(),
            ));
        }
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| {
            Error::Parser(
                format!("bytecode {what} is not valid UTF-8"),
                Span::unknown(),
            )
        })
    }

    fn constant(&mut self) -> Result<Constant> {
        match self.u8()? {
            0 => Ok(Constant::Nothing),
            1 => Ok(Constant::Number(f64::from_le_bytes(self.array()?))),
            2 => match self.u8()? {
                0 => Ok(Constant::YesNo(false)),
                1 => Ok(Constant::YesNo(true)),
                other => Err(Error::Parser(
                    format!("unknown yes/no constant byte {other}"),
                    Span::unknown(),
                )),
            },
            3 => Ok(Constant::Text(self.string("text constant")?)),
            other => Err(Error::Parser(
                format!("unknown constant tag {other}"),
                Span::unknown(),
            )),
        }
    }

    fn block(&mut self, depth: usize) -> Result<Block> {
        if depth > MAX_BLOCK_DEPTH {
            return Err(Error::Parser(
                format!("bytecode nests blocks more than {MAX_BLOCK_DEPTH} levels deep"),
                Span::unknown(),
            ));
        }

        let kind_byte = self.u8()?;
        let kind = BlockKind::from_byte(kind_byte).ok_or_else(|| {
            Error::Parser(format!("unknown block kind {kind_byte}"), Span::unknown())
        })?;

        let arity = self.u32()?;
        let param_count = self.u32()?;
        let params = self.count(param_count, "parameter name", 4, |reader: &mut Reader| {
            reader.string("parameter name")
        })?;
        let name = self.string("block name")?;
        let instruction_count = self.u32()?;
        let code = self.count(
            instruction_count,
            "instruction",
            INSTRUCTION_SIZE,
            |reader: &mut Reader| reader.instruction(),
        )?;
        let child_count = self.u32()?;
        let blocks = self.count(child_count, "block", 1, |reader: &mut Reader| {
            reader.block(depth + 1)
        })?;

        Ok(Block {
            name,
            kind,
            arity,
            params,
            code,
            blocks,
        })
    }

    fn instruction(&mut self) -> Result<Instruction> {
        let opcode_byte = self.u8()?;
        let opcode = Opcode::from_byte(opcode_byte).ok_or_else(|| {
            Error::Parser(
                format!("unknown opcode byte {opcode_byte}"),
                Span::unknown(),
            )
        })?;
        Ok(Instruction {
            opcode,
            arg: self.u32()?,
            aux: self.u32()?,
            line: self.u32()?,
        })
    }
}

/// Names the entry a failure happened in, so a bad constant says which one.
fn annotate(what: &str, index: u32, error: Error) -> Error {
    match error {
        Error::Parser(message, span) => Error::Parser(format!("{what} {index}: {message}"), span),
        other => other,
    }
}
