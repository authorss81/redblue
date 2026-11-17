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
///
/// Version 6 added `MIGHT_FAIL`, at byte 49, which no earlier version used, so
/// the numbers written into a file still mean what they meant. A version-5 file
/// carrying one is refused by the decoder as an unknown instruction — but the
/// version word is bumped as well, so the refusal names the version rather than
/// the instruction.
pub const FORMAT_VERSION: u16 = 6;

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

    /// Whether any block contains an `import`.
    ///
    /// The bytecode VM has no syntax tree, so this is how its pre-run manifest
    /// check knows whether a malformed manifest has an `import` at which to be
    /// reported, exactly as the tree-walking VM asks the program.
    pub fn has_import(&self) -> bool {
        self.all_blocks().iter().any(|block| {
            block
                .code
                .iter()
                .any(|instruction| instruction.opcode == Opcode::Import)
        })
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

#[cfg(test)]
mod native_study_tests {
    use std::fs;
    use std::path::Path;

    use super::{compile_source, Chunk, FORMAT_VERSION, INSTRUCTION_SIZE, MAGIC};

    const RECOMMENDATION: &str = "## Recommendation

**Defer it.**

Build the native backend after the bytecode VM has slot-resolved names and some
form of type or shape feedback, and use Cranelift for the first implementation.
Do not build an LLVM AOT backend now, and do not describe the native backend as
the next step merely because it is the only remaining way to improve speed.

The deferral is not a rejection. The existing pipeline has already reached the
point where a backend is technically possible: the tree-walking frontend,
bytecode compiler, stable file format, disassembler, bytecode VM and
self-hosted fixed point all exist. What is missing is not a code generator
framework. It is the semantic layer a code generator needs to produce code that
is actually faster than the interpreter: resolved variables, closure layout,
object shape feedback, and a runtime ABI that preserves Redblue's visible
behaviour. Building machine-code emission before those exists would produce a
faster instruction decoder attached to the same dynamic hash lookups.

The concrete trigger for reversing the deferral is in
[When the recommendation would be wrong](#when-the-recommendation-would-be-wrong).";

    fn native_document() -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("NATIVE.md");
        let raw = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "NATIVE.md should be readable at {}: {error}",
                path.display()
            )
        });
        raw.replace("\r\n", "\n")
    }

    #[test]
    fn native_study_records_a_recommendation_and_the_components_it_cites() {
        let document = native_document();

        let start = document
            .find("## Recommendation")
            .expect("NATIVE.md has no Recommendation section");
        let end = document
            .find("## What already exists")
            .expect("NATIVE.md has no section after Recommendation");
        assert!(
            start < end,
            "the Recommendation section should precede What already exists"
        );
        assert_eq!(
            document[start..end].trim_end(),
            RECOMMENDATION,
            "NATIVE.md's Recommendation section changed; update the study and this test together"
        );

        for (path, line, symbol) in [
            ("src/lib.rs", 114, "run_source_in"),
            ("src/bytecode/format.rs", 275, "compile"),
            ("src/bytecode/codegen.rs", 32, "compile"),
            ("src/bytecode/codegen.rs", 7, "Names"),
            ("src/bytecode/opcode.rs", 60, "Opcode"),
            ("src/bytecode/opcode.rs", 294, "from_byte"),
            ("src/bytecode/opcode.rs", 305, "name"),
            ("src/bytecode/format.rs", 244, "Chunk"),
            ("src/bytecode/format.rs", 222, "Block"),
            ("src/bytecode/format.rs", 158, "Instruction"),
            ("src/bytecode/format.rs", 111, "Constant"),
            ("src/bytecode/format.rs", 290, "encode"),
            ("src/bytecode/format.rs", 308, "decode"),
            ("src/bytecode/format.rs", 81, "FORMAT_VERSION"),
            ("src/bytecode/disasm.rs", 102, "render"),
            ("src/bytecode/disasm.rs", 35, "render_compact"),
            ("src/bytecode/disasm.rs", 107, "redblue bytecode"),
            ("src/bytecode/vm.rs", 685, "BytecodeVm"),
            ("src/bytecode/vm.rs", 1094, "run"),
            ("src/bytecode/vm.rs", 1304, "charge_step"),
            ("src/bytecode/vm.rs", 3427, "call_function"),
            ("src/interpreter.rs", 333, "Vm"),
            ("src/interpreter.rs", 1699, "run"),
            ("src/interpreter.rs", 1790, "captured"),
            ("src/runtime.rs", 219, "binary_op"),
            ("src/runtime.rs", 166, "index"),
            ("src/runtime.rs", 107, "property"),
            ("src/runtime.rs", 3271, "builtin"),
            ("src/value.rs", 387, "Value"),
            ("src/value.rs", 21, "Fields"),
            ("src/value.rs", 37, "Captured"),
            ("src/value.rs", 48, "FunctionBody"),
            ("src/value.rs", 85, "FunctionValue"),
            ("src/value.rs", 470, "Shared"),
            ("src/stdlib.rs", 520, "builtins"),
            ("docs/BOOTSTRAP.md", 16, "S3"),
            ("docs/BOOTSTRAP.md", 17, "S4"),
            ("docs/BOOTSTRAP.md", 87, "Rust fast path"),
            ("src/lib.rs", 331, "compile"),
            ("src/lib.rs", 337, "dis"),
            ("src/lib.rs", 343, "vm"),
            ("src/lib.rs", 608, "compile_command"),
            ("src/lib.rs", 622, "dis_command"),
            ("src/lib.rs", 641, "vm_command"),
            ("docs/BYTECODE.md", 22, "A byte value already written"),
            ("docs/BYTECODE.md", 56, "reads and writes"),
            ("Cargo.toml", 38, "[dependencies]"),
            ("docs/PERFORMANCE_HISTORY.md", 28, "Cranelift"),
            ("docs/FUTURE_ROADMAP_AND_DEFERRED.md", 112, "Rust"),
            ("docs/FUTURE_ROADMAP_AND_DEFERRED.md", 115, "LuaJIT"),
            ("LIBRARIES.md", 15, "readability"),
            ("LIBRARIES.md", 13, "The rule"),
            ("LIBRARIES.md", 29, "native backend"),
        ] {
            let reference = format!("{path}:{line}");
            assert!(
                document.contains(reference.as_str()),
                "NATIVE.md does not cite {reference}"
            );
            let source_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
            let source = fs::read_to_string(&source_path).unwrap_or_else(|error| {
                panic!(
                    "the cited file should be readable at {}: {error}",
                    source_path.display()
                )
            });
            let cited = source.lines().nth(line - 1).unwrap_or_else(|| {
                panic!(
                    "{reference} is past the end of {path} ({} lines)",
                    source.lines().count()
                )
            });
            assert!(
                cited.contains(symbol),
                "NATIVE.md cites {reference} for `{symbol}`, but that line is {cited:?}"
            );
        }

        let value_source =
            fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/value.rs"))
                .expect("src/value.rs should be readable");
        let enum_start = value_source
            .find("pub enum Value {")
            .expect("src/value.rs should define the Value enum");
        let enum_body = &value_source[enum_start..];
        let enum_body = &enum_body[..enum_body
            .find("\n}")
            .expect("the Value enum should close at column zero")];
        let variant_count = enum_body
            .lines()
            .skip(1)
            .filter(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty()
                    && !trimmed.starts_with("///")
                    && !trimmed.starts_with("#[")
                    && line.starts_with("    ")
                    && !line.starts_with("     ")
                    && trimmed
                        .chars()
                        .next()
                        .is_some_and(|character| character.is_ascii_uppercase())
            })
            .count();
        assert_eq!(
            variant_count, 12,
            "the Value enum should have exactly twelve variants"
        );
        assert!(
            document.contains("`Value` is a Rust enum with twelve variants"),
            "NATIVE.md should say Value has twelve variants"
        );
        assert!(
            document.contains("`Value` is an enum of twelve\n  variants"),
            "NATIVE.md should repeat the twelve-variant count in its speedup analysis"
        );
    }

    #[test]
    fn native_study_pins_the_bytecode_surface_a_backend_would_consume() {
        let document = native_document();
        let marker = "## Effect on `.rbc` version ";
        let version_at = document
            .find(marker)
            .map(|index| index + marker.len())
            .expect("NATIVE.md should describe the .rbc version a backend consumes");
        let documented_version: String = document[version_at..]
            .chars()
            .take_while(|character| character.is_ascii_digit())
            .collect();
        let documented_version: u16 = documented_version
            .parse()
            .expect("the .rbc version heading should start with a number");
        assert_eq!(
            FORMAT_VERSION, documented_version,
            "the build must implement the .rbc version the study documents"
        );
        assert_eq!(INSTRUCTION_SIZE, 13);
        assert_eq!(MAGIC, *b"RED\x1a");

        let chunk =
            compile_source("set x to 2 + 3\nsay x\n").expect("the sample program should compile");
        assert!(!chunk.main.code.is_empty());

        let bytes = chunk.encode();
        let decoded = Chunk::decode(&bytes).expect("a chunk this module wrote should decode");
        assert_eq!(decoded, chunk);

        let listing = decoded.disassemble();
        assert!(
            listing.contains("SAY"),
            "the disassembly should name the SAY instruction, got:\n{listing}"
        );
        assert!(
            listing.contains(&format!("v{FORMAT_VERSION}")),
            "the disassembly should name format version {FORMAT_VERSION}, got:\n{listing}"
        );
    }

    #[test]
    fn edge_native_study_rejects_an_unknown_future_bytecode_version() {
        let chunk = compile_source("say 1\n").expect("the sample program should compile");
        let mut bytes = chunk.encode();
        let version_at = MAGIC.len();
        let future = FORMAT_VERSION + 1;
        bytes[version_at..version_at + 2].copy_from_slice(&future.to_le_bytes());

        let error = Chunk::decode(&bytes)
            .expect_err("a bytecode file from a future format version must be refused");
        let message = error.to_string();

        assert!(
            message.contains("unknown bytecode format version"),
            "the refusal should name the version problem, got: {message}"
        );
        assert!(
            message.contains(&future.to_string()),
            "the refusal should name version {future}, got: {message}"
        );
    }

    #[test]
    fn edge_native_study_rejects_a_truncated_chunk() {
        let chunk = compile_source("say 1\n").expect("the sample program should compile");
        let bytes = chunk.encode();
        let truncated = &bytes[..bytes.len() - 1];

        let result = Chunk::decode(truncated);
        assert!(
            result.is_err(),
            "a chunk that ends one byte early must be refused, not decoded"
        );
        let message = result.unwrap_err().to_string();
        assert!(
            message.contains("truncated bytecode file"),
            "the refusal should name the truncation, got: {message}"
        );
    }

    #[test]
    fn edge_native_study_rejects_a_foreign_magic() {
        let chunk = compile_source("say 1\n").expect("the sample program should compile");
        let mut bytes = chunk.encode();
        bytes[0] = b'X';

        let result = Chunk::decode(&bytes);
        assert!(
            result.is_err(),
            "a file whose magic is not RED\\x1a must be refused"
        );
        let message = result.unwrap_err().to_string();
        assert!(
            message.contains("not a Redblue bytecode file"),
            "the refusal should name the bad magic, got: {message}"
        );
    }

    #[test]
    fn edge_native_study_empty_source_compiles_to_an_empty_chunk() {
        let chunk = compile_source("").expect("an empty program should compile");
        assert!(
            chunk.main.code.is_empty(),
            "an empty program should emit no instructions, got {:?}",
            chunk.main.code
        );

        let decoded = Chunk::decode(&chunk.encode()).expect("an empty chunk should round trip");
        assert_eq!(decoded, chunk);
        assert!(
            decoded
                .disassemble()
                .contains(&format!("v{FORMAT_VERSION}")),
            "the empty chunk should still disassemble with the format version"
        );
    }
}
