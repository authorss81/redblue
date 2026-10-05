//! The instruction set of the Redblue bytecode format.
//!
//! A byte value is part of the file format: it is written into a `.rbc` and
//! read back by a later compiler, so **the numbers below never change**. An
//! opcode that is retired keeps its number reserved rather than being reused,
//! and anything new takes the next free number and bumps
//! [`FORMAT_VERSION`](super::FORMAT_VERSION).

/// One instruction. The byte values are stable; see [`Opcode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    /// Does nothing. Written as a filler by later stages; never emitted by the
    /// compiler in this stage.
    Nop = 0,
    /// Pushes `constants[arg]`.
    PushConst,
    /// Discards the top of the stack.
    Pop,
    /// Reads the variable named `constants[arg]`.
    Load,
    /// Binds the top of the stack to the variable named `constants[arg]`.
    Store,
    /// Prints the top of the stack.
    Say,
    /// Prints the top of the stack in its internal form.
    Print,
    /// Reads `constants[arg]` off the record on top of the stack.
    LoadProperty,
    /// Sets `constants[arg]` on the object below the top of the stack.
    SetProperty,
    /// Indexes the list on top of the stack by the index below it.
    Index,
    /// Builds a list from `arg` values.
    BuildList,
    /// Builds a record from `arg` key/value pairs.
    BuildRecord,
    /// Joins `arg` text parts into one value.
    BuildText,
    /// Numeric and comparison addition.
    Add,
    /// Numeric subtraction.
    Sub,
    /// Numeric multiplication.
    Mul,
    /// Numeric division.
    Div,
    /// Numeric remainder.
    Mod,
    /// Equality.
    Equal,
    /// Inequality.
    NotEqual,
    /// Less than.
    Less,
    /// Less than or equal.
    LessEqual,
    /// Greater than.
    Greater,
    /// Greater than or equal.
    GreaterEqual,
    /// Short-circuiting conjunction.
    And,
    /// Short-circuiting disjunction.
    Or,
    /// Membership.
    In,
    /// Arithmetic negation.
    Neg,
    /// Logical negation.
    Not,
    /// Calls the function named `constants[arg]` with `aux` arguments.
    Call,
    /// Calls `constants[arg]` on the receiver below the arguments, with `aux`
    /// arguments.
    CallMethod,
    /// Returns the top of the stack, or `nothing` when the return had no value.
    Return,
    /// Leaves the innermost loop.
    Break,
    /// Starts the next turn of the innermost loop.
    Skip,
    /// Continues at instruction `arg`.
    Jump,
    /// Pops a value; continues at instruction `arg` when it is not truthy.
    JumpIfFalse,
    /// Turns the value on top of the stack into an iterator.
    GetIter,
    /// Builds a range from `aux` operands (1 for a repeat count, 2 or 3 when
    /// the bounds and step are given).
    GetRange,
    /// Binds a closure whose body is block `arg` and which takes `aux`
    /// parameters.
    DefFunction,
    /// Adds a method whose body is block `arg` and which takes `aux` parameters
    /// to the object being defined.
    DefMethod,
    /// Starts an object body; block `arg` holds its fields and methods and
    /// `aux` is the constant index of the object it extends, or
    /// [`NO_CONST`](super::NO_CONST) when it extends nothing.
    DefObject,
    /// Pops a field's initial value and declares a field named
    /// `constants[arg]` on the object being defined.
    DefField,
    /// Runs what follows with handlers in place: block `arg` is the catch body
    /// and block `aux` the finally body, either being
    /// [`NO_BLOCK`](super::NO_BLOCK) when the source had none.
    Try,
    /// Imports the module named `constants[arg]`.
    Import,
    /// Runs the test body, block `arg`.
    Test,
    /// Compares the two values on top of the stack as an `expect`.
    Expect,
}

impl Opcode {
    /// Every opcode, in byte order. The disassembler and the encoder's
    /// self-check both walk this, so an opcode cannot exist in one and be
    /// missing from the other.
    pub const ALL: &'static [Opcode] = &[
        Opcode::Nop,
        Opcode::PushConst,
        Opcode::Pop,
        Opcode::Load,
        Opcode::Store,
        Opcode::Say,
        Opcode::Print,
        Opcode::LoadProperty,
        Opcode::SetProperty,
        Opcode::Index,
        Opcode::BuildList,
        Opcode::BuildRecord,
        Opcode::BuildText,
        Opcode::Add,
        Opcode::Sub,
        Opcode::Mul,
        Opcode::Div,
        Opcode::Mod,
        Opcode::Equal,
        Opcode::NotEqual,
        Opcode::Less,
        Opcode::LessEqual,
        Opcode::Greater,
        Opcode::GreaterEqual,
        Opcode::And,
        Opcode::Or,
        Opcode::In,
        Opcode::Neg,
        Opcode::Not,
        Opcode::Call,
        Opcode::CallMethod,
        Opcode::Return,
        Opcode::Break,
        Opcode::Skip,
        Opcode::Jump,
        Opcode::JumpIfFalse,
        Opcode::GetIter,
        Opcode::GetRange,
        Opcode::DefFunction,
        Opcode::DefMethod,
        Opcode::DefObject,
        Opcode::DefField,
        Opcode::Try,
        Opcode::Import,
        Opcode::Test,
        Opcode::Expect,
    ];

    /// The opcode a byte stands for, or `None` when the byte is not assigned.
    pub fn from_byte(byte: u8) -> Option<Opcode> {
        Opcode::ALL.get(byte as usize).copied()
    }

    /// The byte written to a `.rbc` file.
    pub fn to_byte(self) -> u8 {
        self as u8
    }

    /// The mnemonic the disassembler prints. Also the name in
    /// `docs/BYTECODE.md`.
    pub fn name(self) -> &'static str {
        match self {
            Opcode::Nop => "NOP",
            Opcode::PushConst => "PUSH_CONST",
            Opcode::Pop => "POP",
            Opcode::Load => "LOAD",
            Opcode::Store => "STORE",
            Opcode::Say => "SAY",
            Opcode::Print => "PRINT",
            Opcode::LoadProperty => "LOAD_PROPERTY",
            Opcode::SetProperty => "SET_PROPERTY",
            Opcode::Index => "INDEX",
            Opcode::BuildList => "BUILD_LIST",
            Opcode::BuildRecord => "BUILD_RECORD",
            Opcode::BuildText => "BUILD_TEXT",
            Opcode::Add => "ADD",
            Opcode::Sub => "SUB",
            Opcode::Mul => "MUL",
            Opcode::Div => "DIV",
            Opcode::Mod => "MOD",
            Opcode::Equal => "EQUAL",
            Opcode::NotEqual => "NOT_EQUAL",
            Opcode::Less => "LESS",
            Opcode::LessEqual => "LESS_EQUAL",
            Opcode::Greater => "GREATER",
            Opcode::GreaterEqual => "GREATER_EQUAL",
            Opcode::And => "AND",
            Opcode::Or => "OR",
            Opcode::In => "IN",
            Opcode::Neg => "NEG",
            Opcode::Not => "NOT",
            Opcode::Call => "CALL",
            Opcode::CallMethod => "CALL_METHOD",
            Opcode::Return => "RETURN",
            Opcode::Break => "BREAK",
            Opcode::Skip => "SKIP",
            Opcode::Jump => "JUMP",
            Opcode::JumpIfFalse => "JUMP_IF_FALSE",
            Opcode::GetIter => "GET_ITER",
            Opcode::GetRange => "GET_RANGE",
            Opcode::DefFunction => "DEF_FUNCTION",
            Opcode::DefMethod => "DEF_METHOD",
            Opcode::DefObject => "DEF_OBJECT",
            Opcode::DefField => "DEF_FIELD",
            Opcode::Try => "TRY",
            Opcode::Import => "IMPORT",
            Opcode::Test => "TEST",
            Opcode::Expect => "EXPECT",
        }
    }

    /// Whether the disassembler shows this opcode's operands.
    ///
    /// False for the opcodes that carry nothing: pushing a constant is
    /// `PUSH_CONST 3`, popping is `POP`. Without this the disassembly of
    /// `say 1` would print a trailing `0` that means nothing.
    pub fn has_operand(self) -> bool {
        !matches!(
            self,
            Opcode::Nop
                | Opcode::Pop
                | Opcode::Say
                | Opcode::Print
                | Opcode::Index
                | Opcode::Add
                | Opcode::Sub
                | Opcode::Mul
                | Opcode::Div
                | Opcode::Mod
                | Opcode::Equal
                | Opcode::NotEqual
                | Opcode::Less
                | Opcode::LessEqual
                | Opcode::Greater
                | Opcode::GreaterEqual
                | Opcode::And
                | Opcode::Or
                | Opcode::In
                | Opcode::Neg
                | Opcode::Not
                | Opcode::Return
                | Opcode::Break
                | Opcode::Skip
                | Opcode::GetIter
                | Opcode::Expect
        )
    }
}
impl Opcode {
    /// Whether the disassembler prints this opcode's second operand.
    ///
    /// The opcodes that carry two: a call and a declaration name a target and
    /// count their arguments or parameters, `GetRange` counts the bounds and
    /// step it was given, and `Try` names two blocks.
    pub fn has_aux(self) -> bool {
        matches!(
            self,
            Opcode::Call
                | Opcode::CallMethod
                | Opcode::GetRange
                | Opcode::DefFunction
                | Opcode::DefMethod
                | Opcode::DefObject
                | Opcode::Try
        )
    }

    /// Whether this opcode's `arg` is an index into the constant pool.
    ///
    /// The disassembler prints the named value next to the index, so a reader
    /// does not have to count down the pool to see which name a `LOAD` reads.
    pub fn takes_constant_index(self) -> bool {
        matches!(
            self,
            Opcode::PushConst
                | Opcode::Load
                | Opcode::Store
                | Opcode::LoadProperty
                | Opcode::SetProperty
                | Opcode::Call
                | Opcode::CallMethod
                | Opcode::DefField
                | Opcode::Import
        )
    }

    /// Whether this opcode's `arg` is an index into the block list of the
    /// block that holds the instruction.
    ///
    /// The disassembler checks it against that list and says so when it is out
    /// of range, so a corrupt file is read as a diagnostic rather than left for
    /// whoever runs it next to discover. `Try`'s is the one that may be
    /// [`NO_BLOCK`](super::NO_BLOCK).
    pub fn takes_block_index(self) -> bool {
        matches!(
            self,
            Opcode::DefFunction
                | Opcode::DefMethod
                | Opcode::DefObject
                | Opcode::Try
                | Opcode::Test
        )
    }
}
