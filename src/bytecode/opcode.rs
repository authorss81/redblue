//! The instruction set of the Redblue bytecode format.
//!
//! A byte value is part of the file format: it is written into a `.rbc` and
//! read back by a later compiler, so **the numbers below never change**. An
//! opcode that is retired keeps its number reserved rather than being reused,
//! and anything new takes the next free number and bumps
//! [`FORMAT_VERSION`](super::FORMAT_VERSION).

/// The `arg` that makes a [`Opcode::Nop`] the end of a `try`'s protected region
/// rather than the filler it is everywhere else.
///
/// A `NOP` names no constant and no block, so its `arg` indexes nothing and is
/// free to say this. **Only** this value is a marker: every other operand —
/// including the `0` a filler carries — leaves the instruction a filler that
/// does nothing. That is what keeps a `NOP` a file happens to contain from
/// closing a protected region, running an enclosing `finally` early, or
/// truncating the region a failure has to skip.
///
/// The marker is the maximum `u32`, which is the format's existing "reserved"
/// value ([`NO_CONST`](super::NO_CONST), [`NO_BLOCK`](super::NO_BLOCK)): it can
/// never be a valid index into a pool or a block list, so it cannot be mistaken
/// for one.
pub const END_TRY_MARKER: u32 = u32::MAX;

/// The `arg` that makes a [`Opcode::Nop`] the start of a statement.
///
/// The compiler emits exactly one of these immediately before each statement's
/// code, so the file says where its statements are rather than leaving a reader
/// to infer it. The step budget is charged here and nowhere else, which is what
/// makes the bytecode VM count **one step per statement** — the unit the
/// tree-walking VM charges — rather than one per instruction. The two then reach
/// the same limit on the same turn of the same loop, and report it in the same
/// words; counting instructions instead meant the two budgets were crossed at
/// different points of the same program, so the same program was stopped by a
/// different limit on each engine.
///
/// A version-4 file carries no such marker, so it would be charged nothing at
/// all: [`FORMAT_VERSION`](super::FORMAT_VERSION) moved to 5 rather than let an
/// old file run under a budget it does not have.
///
/// The value is one below [`END_TRY_MARKER`], which is the other reserved
/// operand a `NOP` can carry. Like that one it can never be a valid index into a
/// constant pool or a block list, so the two meanings cannot be confused.
pub const STATEMENT_MARKER: u32 = u32::MAX - 1;

/// The `arg` that makes a [`Opcode::Nop`] the end of a `might fail` region.
///
/// The guard [`Opcode::MightFail`] installs is popped here on the path where
/// nothing failed, which is the same shape [`END_TRY_MARKER`] has for a `try`'s
/// protected region and for the same reason: the guard has to be taken away
/// again, and there is nowhere else in the instruction stream to say so.
///
/// The value is one below [`STATEMENT_MARKER`], so it can never be confused with
/// that marker or with the filler `0` the same byte carries everywhere else.
pub const MIGHT_FAIL_END_MARKER: u32 = u32::MAX - 2;

/// One instruction. The byte values are stable; see [`Opcode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    /// Does nothing.
    ///
    /// The two exceptions are `NOP`s whose `arg` is a marker. A `NOP` whose `arg`
    /// is [`END_TRY_MARKER`] ends the protected region of the `try` whose `TRY`
    /// is above it, popping its handlers and running its `finally`, so the
    /// protected region is the run of instructions between the two rather than
    /// the rest of the block. The compiler emits exactly one such `NOP` per
    /// `try`, immediately after the protected code. Without it a failure in a
    /// statement *after* the `try` would be caught by that `try`, and a `finally`
    /// would never run at all on the path where nothing failed.
    ///
    /// A `NOP` whose `arg` is [`STATEMENT_MARKER`] begins a statement: it does
    /// nothing itself, and is the point at which the statement is charged to the
    /// step budget.
    ///
    /// The markers ride on this byte rather than on one of their own because the
    /// opcode table was frozen when the first marker was defined: byte 46,
    /// [`Opcode::DeclareConst`], was the end of the table, so an `END_TRY` byte of
    /// its own would have had to renumber an instruction whose byte is part of the
    /// file format. What distinguishes the meanings is the operand, not the
    /// byte — see [`END_TRY_MARKER`].
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
    ///
    /// The name is dotted: `files.read` for a receiver the source named `files`,
    /// and the bare `read` when the receiver was an expression rather than a
    /// name. A call is resolved against the receiver's *name* — `files.read` is
    /// the builtin `files_read`, and `Counter.bump` is a method on a declared
    /// type — so the file has to carry the name, not only the value the receiver
    /// held. A bare name is the one shape the language rejects, because it says
    /// the receiver was not a name at all.
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
    /// Imports the module named `constants[arg]`, binding it the name
    /// `constants[aux]`.
    ///
    /// The second operand is the alias the source wrote, or the module's own
    /// name when it wrote none. It is not reserved: an import always binds a
    /// name, so there is no encoding of "no alias" to keep clear of.
    Import,
    /// Runs the test body, block `arg`.
    Test,
    /// Compares the two values on top of the stack as an `expect`.
    Expect,
    /// Binds the top of the stack to the variable named `constants[arg]` as a
    /// constant: the name may not be bound again, and no `STORE` writes it.
    ///
    /// The declaration `constant NAME to <expr>` compiles to this rather than to
    /// [`Opcode::Store`], so the file says the name is read-only. A version-2
    /// file has no way to say that — a `constant` compiled to a `STORE` there —
    /// which is what version 3 changed.
    DeclareConst,
    /// Runs the `module NAME ... end` declaration whose body is block `arg`. The
    /// module's name is that block's `name`, as a test's is its test block's and
    /// a `catch`'s variable is its catch block's.
    ///
    /// The body runs in a scope and a frame of its own, so a `set` inside a
    /// module is the module's name and not a name of the program that declared
    /// it. A version-3 file compiled the body's statements inline and the
    /// declaration to nothing at all, which is what version 4 changed.
    Module,
    /// Names what a module declaration publishes: `constants[arg]`, or
    /// [`NO_CONST`](super::NO_CONST) for a name the declaration does not define,
    /// which a VM refuses rather than publishes.
    ///
    /// A module body block begins with a run of these, one per name the
    /// declaration publishes, and the rest of the block is the body itself. The
    /// run is data the [`Opcode::Module`] that entered the block has already
    /// read: executing one does nothing.
    Export,
    /// Guards the instructions that follow against failure: a failure raised
    /// before the region closes is discarded, and execution continues at
    /// `arg` with `nothing` on the operand stack.
    ///
    /// This is the bytecode of the expression `might fail <call>`. The region is
    /// the instructions between this one and the [`Opcode::Nop`] carrying
    /// [`MIGHT_FAIL_END_MARKER`], which closes it on the path where nothing
    /// failed; the recovery code the operand names is compiled after that marker
    /// and reached only by jumping to it. `arg` is the offset of that recovery
    /// code, so it is patched once the rest of the expression is compiled.
    MightFail,
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
        Opcode::DeclareConst,
        Opcode::Module,
        Opcode::Export,
        Opcode::MightFail,
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
            Opcode::DeclareConst => "DECLARE_CONST",
            Opcode::Module => "MODULE",
            Opcode::Export => "EXPORT",
            Opcode::MightFail => "MIGHT_FAIL",
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
    /// step it was given, `Try` names two blocks, and `Import` names the module
    /// and the alias it binds it to.
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
                | Opcode::Import
        )
    }

    /// Whether this opcode's `arg` is an index into the constant pool.
    ///
    /// The disassembler prints the named value next to the index, so a reader
    /// does not have to count down the pool to see which name a `LOAD` reads.
    /// `Export` is excluded: its `arg` may be the reserved `NO_CONST`, which is
    /// not a name to look up, and the disassembler names it on its own.
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
                | Opcode::DeclareConst
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
                | Opcode::Module
        )
    }
}
