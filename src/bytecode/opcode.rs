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
    /// Does nothing. A filler for later stages; never emitted by this compiler.
    Nop = 0,
    /// Pushes `constants[arg]`.
    PushConst = 1,
    /// Discards the top of the stack.
    Pop = 2,
    /// Reads the variable named `constants[arg]`.
    Load = 3,
    /// Binds the top of the stack to the variable named `constants[arg]`.
    Store = 4,
    /// Prints the top of the stack.
    Say = 5,
    /// Prints the top of the stack in its internal form.
    Print = 6,
    /// Reads `constants[arg]` off the record on top of the stack.
    LoadProperty = 7,
    /// Sets `constants[arg]` on the object below the top of the stack.
    SetProperty = 8,
    /// Indexes the list on top of the stack by the index below it.
    Index = 9,
    /// Builds a list from `arg` values.
    BuildList = 10,
    /// Builds a record from `arg` key/value pairs.
    BuildRecord = 11,
    /// Joins `arg` text parts into one value.
    BuildText = 12,
    /// Numeric and comparison addition.
    Add = 13,
    /// Numeric subtraction.
    Sub = 14,
    /// Numeric multiplication.
    Mul = 15,
    /// Numeric division.
    Div = 16,
    /// Numeric remainder.
    Mod = 17,
    /// Equality.
    Equal = 18,
    /// Inequality.
    NotEqual = 19,
    /// Less than.
    Less = 20,
    /// Less than or equal.
    LessEqual = 21,
    /// Greater than.
    Greater = 22,
    /// Greater than or equal.
    GreaterEqual = 23,
    /// Short-circuiting conjunction.
    And = 24,
    /// Short-circuiting disjunction.
    Or = 25,
    /// Membership.
    In = 26,
    /// Arithmetic negation.
    Neg = 27,
    /// Logical negation.
    Not = 28,
    /// Calls the function named `constants[arg]` with `aux` arguments.
    Call = 29,
    /// Calls `constants[arg]` on the receiver below the arguments, with `aux` arguments.
    CallMethod = 30,
    /// Returns the top of the stack, or `nothing` when the return had no value.
    Return = 31,
    /// Leaves the innermost loop.
    Break = 32,
    /// Starts the next turn of the innermost loop.
    Skip = 33,
    /// Continues at instruction `arg`.
    Jump = 34,
    /// Pops a value; continues at instruction `arg` when it is not truthy.
    JumpIfFalse = 35,
    /// Turns the value on top of the stack into an iterator.
    GetIter = 36,
    /// Builds a range from `aux` operands: 1 for a repeat count, 2 or 3 when the bounds and step are given.
    GetRange = 37,
    /// Binds a closure whose body is block `arg` and which takes `aux` parameters.
    DefFunction = 38,
    /// Adds a method whose body is block `arg` and which takes `aux` parameters to the object being defined.
    DefMethod = 39,
    /// Starts an object body; block `arg` holds its fields and methods and `aux` is 1 when the declaration extends another object.
    DefObject = 40,
    /// Declares a field named `constants[arg]` on the object being defined.
    DefField = 41,
    /// Runs what follows with handlers in place: block `arg` is the catch body and block `aux` the finally body, either being `NO_BLOCK` when the source had none.
    Try = 42,
    /// Imports the module named `constants[arg]`.
    Import = 43,
    /// Runs the test body, block `arg`.
    Test = 44,
    /// Compares the two values on top of the stack as an `expect`.
    Expect = 45,
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
}
