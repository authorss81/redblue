//! The bytecode virtual machine: what `rb vm file.rbc` executes.
//!
//! This is bootstrap stage **S1b**. The tree-walking VM in [`crate::vm`] is what
//! `rb run` uses and is the specification of what a Redblue program *does*;
//! this runs the compiled form and has to answer the same way, down to the
//! wording of a failure. Where the two have to agree on an operation — the
//! arithmetic operators, indexing, the builtin library — neither writes it: both
//! read [`crate::runtime`], so there is one implementation and it cannot drift.
//!
//! # How it runs
//!
//! A program is a tree of blocks. Running one means running `main`; running a
//! block means stepping through its instructions with an operand stack, a scope
//! stack, a stack of active loops and a stack of installed `try` handlers. A
//! call pushes a *frame* — the same stacks, with their bases remembered — rather
//! than calling back into Rust.
//!
//! That is what makes deep recursion safe here: the call depth counts frames in
//! a `Vec`, so a call chain as deep as [`DEFAULT_MAX_CALL_DEPTH`] costs heap
//! rather than machine stack. There is no path from a `.rbc` to a native stack
//! overflow.
//!
//! # What the file has to say
//!
//! Names are stored rather than resolved to slots, which means two opcodes carry
//! two names in one operand: `CALL_METHOD` and `SET_PROPERTY` hold
//! `receiver.method` and `object.property`. A call is resolved against the
//! receiver's *name* — `files.read` is the builtin `files_read`, and
//! `Counter.bump` is a method on a declared type — so a file holding only the
//! method name could not say which one it meant.
//!
//! Loops are the one place where a block's *shape* matters. A loop is a backward
//! `JUMP`, and the instruction it jumps to is the loop's `top`: the `STORE` that
//! binds the loop variable for `for each` and `repeat`, and the first
//! instruction of the condition for a `while`. [`loop_sites`] finds those once
//! per block, and everything else about a loop follows from them.

use std::collections::HashMap;
use std::sync::Arc;

use crate::bytecode::format::{Block, Chunk, Constant, Instruction};
use crate::bytecode::opcode::Opcode;
use crate::bytecode::{NO_BLOCK, NO_CONST};
use crate::error::{Error, Result, Span};
use crate::lexer::Lexer;
use crate::parser::{self, BinaryOp, Program, Statement, UnaryOp};
use crate::runtime;
use crate::stdlib;
use crate::value::{
    finite_number, Captured, CapturedScope, Fields, FunctionBody, FunctionValue, Value,
};
use crate::vm::{
    resolve_max_call_depth, resolve_max_iterations, resolve_max_iterations_from, resolve_max_steps,
    resolve_max_steps_from, MAX_CALL_DEPTH, MAX_ITERATIONS, MAX_STEPS,
};

/// The default number of user calls that may be active at once. Exceeding it is
/// a `Runtime` error, not a stack overflow. The same number the tree-walking VM
/// enforces, so one program runs under one budget whichever VM executes it.
pub const DEFAULT_MAX_CALL_DEPTH: usize = MAX_CALL_DEPTH;
/// The default per-loop iteration cap. See [`DEFAULT_MAX_CALL_DEPTH`].
pub const DEFAULT_MAX_ITERATIONS: usize = MAX_ITERATIONS;
/// The default step budget. See [`DEFAULT_MAX_CALL_DEPTH`].
pub const DEFAULT_MAX_STEPS: usize = MAX_STEPS;

/// The block at `path` of `chunk`, or `None` when a name a file wrote points past
/// the end of a block's own list.
///
/// The path is checked rather than trusted: it came out of an operand, and a file
/// that names a block which is not there is a file that cannot be run.
fn block_at<'a>(chunk: &'a Chunk, path: &[u32]) -> Option<&'a Block> {
    let mut block = &chunk.main;
    for index in path {
        block = block.blocks.get(*index as usize)?;
    }
    Some(block)
}

/// One active loop.
///
/// The four kinds of loop this VM runs differ only in where they get their next
/// value from and in what the iteration limit calls them, so they share one
/// counter, one exit and one operand-stack base.
struct Loop {
    /// What the per-loop iteration limit calls this loop in its failure: `for
    /// each`, `for each from`, `repeat` or `while`.
    kind: &'static str,
    /// Where this loop's next value comes from, or `None` for a `while`, which
    /// re-runs its condition rather than drawing from a sequence.
    iterator: Option<Sequence>,
    /// The instruction this loop jumps back to: the `STORE` that binds its
    /// variable, or the first instruction of a `while`'s condition.
    top: u32,
    /// The instruction that jumps back to `top`. It is the last instruction of the
    /// loop body, so it is also the last one `break` is inside.
    back_edge: u32,
    /// Where the loop is left when the body is finished or `break` runs.
    exit: u32,
    /// Turns run so far, against the VM's iteration cap.
    iterations: usize,
    /// The operand stack height before this loop pushed anything, which is what
    /// leaving the loop truncates back to.
    stack_base: usize,
}

/// The sequence a loop draws its values from.
enum Sequence {
    /// `for each x in items`. A list is its own sequence; anything else has none
    /// at all, which is what makes `for each x in 5` run no times rather than
    /// fail — the behaviour the tree-walking VM has.
    Each { items: Vec<Value>, index: usize },
    /// `for each x from start to end by step`.
    Range { current: f64, end: f64, step: f64 },
    /// `repeat n times`. A count that is not a number runs no times.
    Repeat { remaining: i64 },
}

/// Whether `sequence` has a value left, and what it is.
fn peek(sequence: &Sequence) -> Option<Value> {
    match sequence {
        Sequence::Each { items, index } => items.get(*index).cloned(),
        Sequence::Range { current, end, .. } => {
            if current <= end {
                Some(Value::Number(*current))
            } else {
                None
            }
        }
        Sequence::Repeat { remaining } => {
            if *remaining > 0 {
                Some(Value::Nothing)
            } else {
                None
            }
        }
    }
}

/// Draws the next value from `sequence`.
///
/// A range's counter is stepped here rather than after the body, because a step
/// that overflows it is the same failure as one that would put an infinity in the
/// loop variable, and the tree-walking VM raises it at the same point.
fn take(sequence: &mut Sequence, span: Span) -> Result<Option<Value>> {
    match sequence {
        Sequence::Each { items, index } => {
            let Some(value) = items.get(*index).cloned() else {
                return Ok(None);
            };
            *index += 1;
            Ok(Some(value))
        }
        Sequence::Range { current, end, step } => {
            if current > end {
                return Ok(None);
            }
            let value = *current;
            *current = finite_number(*current + *step, span)?;
            Ok(Some(Value::Number(value)))
        }
        Sequence::Repeat { remaining } => {
            if *remaining <= 0 {
                return Ok(None);
            }
            *remaining -= 1;
            Ok(Some(Value::Nothing))
        }
    }
}

/// Charges one turn of a loop against `max`, or fails.
///
/// Called where the loop is about to run its body again, which is the same point
/// the tree-walking VM charges: after a sequence has been found to have a value
/// left, and after a `while`'s condition has come out true.
fn charge_iteration(max: usize, iterations: &mut usize, kind: &str, span: Span) -> Result<()> {
    if *iterations >= max {
        return Err(Error::Runtime(
            format!("Maximum of {max} iterations reached in a '{kind}' loop",),
            span,
        ));
    }
    *iterations += 1;
    Ok(())
}

/// One `try` whose protected code is still running.
///
/// `TRY` names its handlers rather than jumping to them, and the protected code
/// is compiled inline, so a handler covers the rest of the block it is in. That
/// is what `docs/BYTECODE.md` specifies and what this VM implements; a `try`
/// that is the last statement of its block — which is what every program in
/// `examples/` and `tests/` writes — covers exactly the statements the
/// tree-walking VM protects.
struct Handler {
    /// The `catch` body together with the name it binds, or `None` for a `try`
    /// with no `catch`.
    catch: Option<((Arc<Chunk>, Vec<u32>), String)>,
    /// The `finally` body, or `None` for a `try` with no `finally`.
    finally: Option<(Arc<Chunk>, Vec<u32>)>,
    /// The operand stack height the protected code started at, which is what the
    /// handlers start from.
    stack_base: usize,
    /// How many loops were running when the protected code started, so a handler
    /// does not inherit a sequence the failed code left half drawn.
    loop_base: usize,
}

/// One `object` declaration being assembled. Its body declares the fields and the
/// methods, and the type is registered when the body finishes.
struct ObjectPending {
    name: String,
    parent: Option<String>,
    fields: Fields,
    methods: Fields,
}

/// One `object` declaration, after its parent has been merged into it.
#[derive(Clone)]
struct ObjectType {
    fields: Fields,
    methods: Fields,
}

/// One active call: a block being run.
struct Frame {
    /// The compiled program the block belongs to. A frame may be running a
    /// module's chunk rather than the program's own.
    chunk: Arc<Chunk>,
    /// Child-block indexes from that program's `main`, to the block being run.
    path: Vec<u32>,
    /// The next instruction to run. `usize::MAX` once the frame has returned.
    ip: usize,
    /// The operand stack height this frame started at.
    stack_base: usize,
    /// How many loops and handlers were active when it started.
    loop_base: usize,
    handler_base: usize,
    /// The scope stack height when it started.
    locals_base: usize,
    /// Whether this frame is running an `object` body.
    object_body: bool,
    /// Whether this frame is a user call, which is what the call-depth limit
    /// counts.
    is_call: bool,
    /// Whether a `STORE` here writes a global rather than a local — a module's
    /// bindings are the importing program's globals, exactly as they are for the
    /// tree-walking VM.
    globals_only: bool,
    /// The loops in this block, found once when the frame was pushed.
    sites: Vec<LoopSite>,
}

/// A loop in a block, located from its backward jump.
#[derive(Clone, Copy)]
struct LoopSite {
    /// The instruction the loop jumps back to.
    top: u32,
    /// The `JUMP` that jumps back to it.
    back_edge: u32,
    /// Where the loop is left when the body is finished or `break` runs.
    exit: u32,
    /// Whether the loop draws its values from a sequence rather than
    /// re-evaluating a condition.
    iterator: bool,
    /// What the iteration limit calls this loop.
    kind: &'static str,
}

/// Every loop in `block`, found from its backward jumps.
///
/// A loop is the backward `JUMP` the compiler writes at the end of every loop
/// body: it targets the `STORE` that binds the loop variable for `for each` and
/// `repeat`, and the first instruction of the condition for a `while`. No other
/// jump in the instruction set goes backwards, so that jump *is* the loop and
/// nothing in the file has to say where one begins.
///
/// A `while` is told from the other three by what precedes its `top`: a
/// `GET_ITER`, or a `GET_RANGE` whose operand count says whether it is a
/// `repeat` count or a pair of bounds. A `while` has neither, because its `top`
/// is its condition.
///
/// `exit` is where the loop is left. A sequence loop runs out at the instruction
/// after its backward jump. A `while` leaves through its own `JUMP_IF_FALSE`, so
/// that is the instruction sought: the one in the loop's body that jumps past the
/// backward jump.
fn loop_sites(block: &Block) -> Vec<LoopSite> {
    let mut sites = Vec::new();
    for (index, instruction) in block.code.iter().enumerate() {
        if instruction.opcode != Opcode::Jump || instruction.arg as usize >= index {
            continue;
        }
        let top = instruction.arg;
        let back_edge = index as u32;
        let (iterator, kind) = match top
            .checked_sub(1)
            .and_then(|at| block.code.get(at as usize))
        {
            Some(previous) if previous.opcode == Opcode::GetIter => (true, "for each"),
            Some(previous) if previous.opcode == Opcode::GetRange => {
                if previous.aux == 1 {
                    (true, "repeat")
                } else {
                    (true, "for each from")
                }
            }
            _ => (false, "while"),
        };
        let exit = if iterator {
            back_edge + 1
        } else {
            (top as usize..index)
                .find(|at| {
                    block.code[*at].opcode == Opcode::JumpIfFalse && block.code[*at].arg > back_edge
                })
                .map(|at| block.code[at].arg)
                // A `while` whose body cannot leave of its own accord — an empty
                // body — has no exit jump, so the end of the loop is the
                // instruction after the backward jump.
                .unwrap_or(back_edge + 1)
        };
        sites.push(LoopSite {
            top,
            back_edge,
            exit,
            iterator,
            kind,
        });
    }
    sites
}

/// The `Span` a failure at `line` is reported against. A `0` is a synthetic
/// instruction the compiler inserted, and names no line.
fn line_span(line: u32) -> Span {
    Span::new(line as usize, 1)
}

/// The `BinaryOp` an arithmetic opcode stands for.
///
/// The operators are the parser's own enum rather than a second copy, which is
/// what makes a bytecode `ADD` and a tree-walked `+` the same operation instead
/// of two that happen to agree today.
fn binary_operator(opcode: Opcode) -> &'static BinaryOp {
    static ADD: BinaryOp = BinaryOp::Add;
    static SUB: BinaryOp = BinaryOp::Sub;
    static MUL: BinaryOp = BinaryOp::Mul;
    static DIV: BinaryOp = BinaryOp::Div;
    static MOD: BinaryOp = BinaryOp::Mod;
    static EQUAL: BinaryOp = BinaryOp::Equal;
    static NOT_EQUAL: BinaryOp = BinaryOp::NotEqual;
    static LESS: BinaryOp = BinaryOp::Less;
    static LESS_EQUAL: BinaryOp = BinaryOp::LessEqual;
    static GREATER: BinaryOp = BinaryOp::Greater;
    static GREATER_EQUAL: BinaryOp = BinaryOp::GreaterEqual;
    static AND: BinaryOp = BinaryOp::And;
    static OR: BinaryOp = BinaryOp::Or;
    static IN: BinaryOp = BinaryOp::In;
    match opcode {
        Opcode::Add => &ADD,
        Opcode::Sub => &SUB,
        Opcode::Mul => &MUL,
        Opcode::Div => &DIV,
        Opcode::Mod => &MOD,
        Opcode::Equal => &EQUAL,
        Opcode::NotEqual => &NOT_EQUAL,
        Opcode::Less => &LESS,
        Opcode::LessEqual => &LESS_EQUAL,
        Opcode::Greater => &GREATER,
        Opcode::GreaterEqual => &GREATER_EQUAL,
        Opcode::And => &AND,
        Opcode::Or => &OR,
        _ => &IN,
    }
}

/// The `UnaryOp` a unary opcode stands for.
fn unary_operator(opcode: Opcode) -> &'static UnaryOp {
    static NEG: UnaryOp = UnaryOp::Neg;
    static NOT: UnaryOp = UnaryOp::Not;
    match opcode {
        Opcode::Neg => &NEG,
        _ => &NOT,
    }
}

/// The paths an `import` of `name` is looked for under, in order — the same
/// three, in the same order, as the tree-walking VM uses.
fn module_paths(name: &str) -> Vec<String> {
    vec![
        format!("modules/{name}.rb"),
        format!("./modules/{name}"),
        name.to_string(),
    ]
}

/// Where a name a file wrote splits into the two names it carries.
///
/// `CALL_METHOD` carries `receiver.method` and `SET_PROPERTY` carries
/// `object.property`. A name with no dot in it is the one shape the language does
/// not have: a method call whose receiver was an expression rather than a name.
fn split_dotted(name: &str) -> Option<(&str, &str)> {
    name.find('.').map(|dot| (&name[..dot], &name[dot + 1..]))
}

/// Runs a compiled Redblue program.
pub struct BytecodeVm {
    globals: HashMap<String, Value>,
    /// The live local scopes, outermost first. A call's parameters are the
    /// innermost scope, and a closure pushes the scopes it captured above its
    /// caller's frames — see [`BytecodeVm::call_function`].
    locals: Vec<CapturedScope>,
    /// The lines `say` has produced. Printed when the program finishes, which is
    /// where the tree-walking VM prints them too, so a program that mixes `say`
    /// and `print` writes its lines in the same order either way.
    output: Vec<String>,
    /// Every `object` declaration, by type name.
    objects: HashMap<String, ObjectType>,
    expectation_failure: Option<crate::testing::assertions::TestAssertionError>,
    current_span: Span,
    /// The operand stack. Every block is entered with an empty one; the bases
    /// live in [`Frame`].
    stack: Vec<Value>,
    /// The active call frames, the program's own at the bottom.
    frames: Vec<Frame>,
    /// The loops currently running, innermost last.
    loops: Vec<Loop>,
    /// The `try` handlers currently installed, innermost last.
    handlers: Vec<Handler>,
    /// The `object` declaration the top frame is assembling, if it is an object
    /// body.
    pending_object: Option<ObjectPending>,
    /// What the last frame to finish produced.
    outcome: Value,
    call_depth: usize,
    max_call_depth: usize,
    steps: usize,
    max_steps: usize,
    max_iterations: usize,
}

impl Default for BytecodeVm {
    fn default() -> Self {
        Self::new()
    }
}

impl BytecodeVm {
    /// Builds a VM with the configured limits and no program loaded.
    pub fn new() -> Self {
        Self {
            globals: stdlib::builtins(),
            locals: vec![CapturedScope::new()],
            output: Vec::new(),
            objects: HashMap::new(),
            expectation_failure: None,
            current_span: Span::unknown(),
            stack: Vec::new(),
            frames: Vec::new(),
            loops: Vec::new(),
            handlers: Vec::new(),
            pending_object: None,
            outcome: Value::Nothing,
            call_depth: 0,
            max_call_depth: resolve_max_call_depth(),
            steps: 0,
            max_steps: resolve_max_steps(),
            max_iterations: resolve_max_iterations(),
        }
    }

    /// Builds a VM with an explicit call-depth limit, ignoring the environment.
    pub fn with_max_call_depth(max_call_depth: usize) -> Self {
        let mut vm = Self::new();
        if max_call_depth > 0 {
            vm.max_call_depth = max_call_depth;
        }
        vm
    }

    /// Builds a VM with an explicit step budget, ignoring the environment. A
    /// budget of zero is ignored, as it is for the tree-walking VM.
    pub fn with_max_steps(max_steps: usize) -> Self {
        let mut vm = Self::new();
        vm.max_steps = resolve_max_steps_from(Some(max_steps));
        vm
    }

    /// Builds a VM with an explicit per-loop iteration cap, ignoring the
    /// environment. A cap of zero is ignored, for the same reason.
    pub fn with_max_iterations(max_iterations: usize) -> Self {
        let mut vm = Self::new();
        vm.max_iterations = resolve_max_iterations_from(Some(max_iterations));
        vm
    }

    /// Takes the structured failure from the most recent failed `expect`, so a
    /// caller can report expected and actual values rather than only a message.
    pub fn take_expectation_failure(
        &mut self,
    ) -> Option<crate::testing::assertions::TestAssertionError> {
        self.expectation_failure.take()
    }

    /// Takes the lines `say` produced, for a caller that wants them rather than
    /// the printing.
    pub fn take_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.output)
    }

    /// Runs `chunk` and returns what `main` left as its value.
    ///
    /// The `say` output is printed when the program finishes rather than as it
    /// happens, which is where the tree-walking VM prints it too.
    pub fn run(&mut self, chunk: &Chunk) -> Result<Value> {
        self.frames.clear();
        self.loops.clear();
        self.handlers.clear();
        self.stack.clear();
        self.locals.truncate(1);
        self.outcome = Value::Nothing;

        let chunk = Arc::new(chunk.clone());
        self.frames.push(self.frame_for(&chunk, Vec::new()));
        let result = self.drive(0);
        if result.is_ok() {
            for line in &self.output {
                println!("{}", line);
            }
        }
        result.map(|()| self.outcome.clone())
    }

    // -- the block tree -----------------------------------------------------

    /// A frame for `path` of `chunk`, with this VM's current stack heights as its
    /// bases.
    fn frame_for(&self, chunk: &Arc<Chunk>, path: Vec<u32>) -> Frame {
        Frame {
            sites: block_at(chunk, &path).map(loop_sites).unwrap_or_default(),
            chunk: chunk.clone(),
            path,
            ip: 0,
            stack_base: self.stack.len(),
            loop_base: self.loops.len(),
            handler_base: self.handlers.len(),
            locals_base: self.locals.len(),
            object_body: false,
            is_call: false,
            globals_only: false,
        }
    }

    /// `arg` as a chunk-and-path into the block the frame is running.
    fn child_path(&self, frame: usize, arg: u32) -> Result<(Arc<Chunk>, Vec<u32>)> {
        if arg == NO_BLOCK {
            return Err(Error::Runtime(
                "bytecode names a block where there is none".to_string(),
                self.span(),
            ));
        }
        let Some(source) = self.frames.get(frame) else {
            return Err(Error::Runtime(
                "bytecode left no frame to run".to_string(),
                self.span(),
            ));
        };
        let mut path = source.path.clone();
        path.push(arg);
        Ok((source.chunk.clone(), path))
    }

    /// The block `arg` names inside the frame's own block, or a failure naming
    /// the path — a block index is an operand a file wrote, so it is checked
    /// rather than trusted.
    fn child_block(&self, frame: usize, arg: u32) -> Result<Block> {
        let (chunk, path) = self.child_path(frame, arg)?;
        Ok(self.block_of(&chunk, &path)?.clone())
    }

    /// [`block_at`] with a failure that says which path was not there.
    fn block_of<'a>(&self, chunk: &'a Chunk, path: &[u32]) -> Result<&'a Block> {
        block_at(chunk, path).ok_or_else(|| {
            let mut named = String::from("main");
            for index in path {
                named.push_str(&format!(".blocks[{index}]"));
            }
            Error::Runtime(
                format!("bytecode names a block that is not there: {named}"),
                self.span(),
            )
        })
    }

    /// The frame's chunk, cloned so an instruction can be read without borrowing
    /// the VM for the length of the read.
    fn chunk_of(&self, frame: usize) -> Result<Arc<Chunk>> {
        self.frames
            .get(frame)
            .map(|frame| frame.chunk.clone())
            .ok_or_else(|| {
                Error::Runtime(
                    "bytecode left no frame to run".to_string(),
                    self.current_span,
                )
            })
    }

    fn code_len(&self, frame: usize) -> usize {
        let Ok(chunk) = self.chunk_of(frame) else {
            return 0;
        };
        match block_at(&chunk, &self.frames[frame].path) {
            Some(block) => block.code.len(),
            None => 0,
        }
    }

    /// The instruction at `ip` of the frame's block.
    fn instruction_at(&self, frame: usize, ip: usize) -> Result<Option<Instruction>> {
        let chunk = self.chunk_of(frame)?;
        Ok(block_at(&chunk, &self.frames[frame].path)
            .and_then(|block| block.code.get(ip))
            .copied())
    }

    fn span(&self) -> Span {
        self.current_span
    }

    // -- the step budget ----------------------------------------------------

    /// Charges one instruction to the step budget.
    ///
    /// Charged per instruction rather than per statement, which is what an
    /// instruction-counting VM has: the budget bounds the program either way, and
    /// an instruction is the thing actually run.
    fn charge_step(&mut self) -> Result<()> {
        if self.steps >= self.max_steps {
            return Err(Error::Runtime(
                format!(
                    "Step budget of {} reached before the program finished",
                    self.max_steps
                ),
                self.span(),
            ));
        }
        self.steps += 1;
        Ok(())
    }

    // -- the operand stack --------------------------------------------------

    fn push(&mut self, value: Value) {
        self.stack.push(value);
    }

    fn pop(&mut self) -> Result<Value> {
        self.stack.pop().ok_or_else(|| {
            Error::Runtime(
                "bytecode left the operand stack empty where a value was needed".to_string(),
                self.span(),
            )
        })
    }

    /// Pops `count` values, oldest first.
    fn pop_n(&mut self, count: u32) -> Result<Vec<Value>> {
        let count = count as usize;
        let base = self.stack.len().saturating_sub(count);
        let floor = self.frames.last().map_or(0, |frame| frame.stack_base);
        if base < floor {
            return Err(Error::Runtime(
                format!("bytecode asked for {count} values its frame never pushed"),
                self.span(),
            ));
        }
        Ok(self.stack.split_off(base))
    }

    // -- names --------------------------------------------------------------

    fn get_var(&self, name: &str) -> Option<Value> {
        for scope in self.locals.iter().rev() {
            if let Some(value) = scope.get(name) {
                return Some(value.clone());
            }
        }
        self.globals.get(name).cloned()
    }

    /// Binds `name` in the innermost live scope that already has it, and in a
    /// global when no local scope does. This resolves a name exactly as
    /// [`BytecodeVm::get_var`] does, so a read and a write of one name agree.
    fn set_var(&mut self, name: &str, value: Value) {
        for scope in self.locals.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return;
            }
        }
        self.globals.insert(name.to_string(), value);
    }

    // -- the constant pool --------------------------------------------------

    fn constant(&self, index: u32) -> Result<Constant> {
        let Some(frame) = self.frames.last() else {
            return Err(Error::Runtime(
                "bytecode left no frame to read".to_string(),
                self.current_span,
            ));
        };
        let pool = frame.chunk.constants.len();
        frame
            .chunk
            .constants
            .get(index as usize)
            .cloned()
            .ok_or_else(|| {
                Error::Runtime(
                    format!("bytecode names constant {index}, and the pool holds {pool}"),
                    self.span(),
                )
            })
    }

    /// The text constant at `index`, which is a name or a string literal.
    fn constant_text(&self, index: u32) -> Result<String> {
        match self.constant(index)? {
            Constant::Text(text) => Ok(text),
            other => Err(Error::Runtime(
                format!("bytecode expects a name at constant {index}, found {other}"),
                self.span(),
            )),
        }
    }

    /// The constant at `index` as a value.
    fn constant_value(&self, index: u32) -> Result<Value> {
        match self.constant(index)? {
            Constant::Nothing => Ok(Value::Nothing),
            Constant::YesNo(b) => Ok(Value::YesNo(b)),
            Constant::Text(text) => Ok(Value::Text(text)),
            // Checked here rather than at decode, so a number a file carries as
            // `NaN` is refused when a program uses it rather than making an
            // otherwise sound file unreadable.
            Constant::Number(n) => Value::number(n, self.span()),
        }
    }

    // -- the interpreter loop -----------------------------------------------

    /// Runs instructions until the frame stack is down to `base`.
    fn drive(&mut self, base: usize) -> Result<()> {
        loop {
            while self.frames.len() > base && self.top_finished() {
                self.unwind_frame()?;
            }
            if self.frames.len() <= base {
                return Ok(());
            }
            if let Err(error) = self.step() {
                if !self.handle_failure(&error)? {
                    return Err(error);
                }
            }
        }
    }

    fn top_finished(&self) -> bool {
        match self.frames.last() {
            None => true,
            Some(frame) => frame.ip >= self.code_len(self.frames.len() - 1),
        }
    }

    /// Finishes the top frame and leaves what it produced where the caller can
    /// see it.
    ///
    /// A block's value is the value its last statement produced, so it is whatever
    /// the frame's own slice of the operand stack holds; a block that produced
    /// nothing — because every statement consumed what it made, or because it ran
    /// out of instructions first — is `nothing`.
    fn unwind_frame(&mut self) -> Result<()> {
        let Some(frame) = self.frames.pop() else {
            return Ok(());
        };
        self.loops.truncate(frame.loop_base);
        self.handlers.truncate(frame.handler_base);
        self.locals.truncate(frame.locals_base);
        if frame.is_call {
            self.call_depth = self.call_depth.saturating_sub(1);
        }
        if frame.object_body {
            // An `object` body does not return a value: it registers the type and
            // leaves the record the type resolved to, which the `STORE` the file
            // writes next binds to the type's name.
            let fields = self.finish_object()?;
            if !self.frames.is_empty() {
                self.stack.push(Value::Record(fields));
            }
            return Ok(());
        }
        let mut value = Value::Nothing;
        if self.stack.len() > frame.stack_base {
            value = self.stack.pop().expect("the stack holds a value");
        }
        self.stack.truncate(frame.stack_base);
        if self.frames.is_empty() {
            self.outcome = value;
        } else {
            self.stack.push(value);
        }
        Ok(())
    }

    /// Registers the object type the finished body declared, and returns the
    /// record it resolved to.
    ///
    /// Lookup order, and the whole of it: the declaration's own `has` fields and
    /// `to can` methods first, then the nearest parent's, then the
    /// grandparent's, and so on. The first declaration of a name in that walk
    /// wins and the further ones are not copied in, so a child field shadows its
    /// parent's field rather than merging with it.
    ///
    /// The chain is walked once, here, with every name it visits collected, so
    /// `object A extends A` and a two-object cycle are reported instead of being
    /// walked again. A parent that is not declared is the same failure.
    fn finish_object(&mut self) -> Result<Fields> {
        let pending = self
            .pending_object
            .take()
            .expect("an object declaration being assembled");
        let mut fields = pending.fields;
        let mut methods = pending.methods;

        let mut chain: Vec<ObjectType> = Vec::new();
        let mut seen = vec![pending.name.clone()];
        let mut parent = pending.parent.clone();
        while let Some(current) = parent {
            if seen.contains(&current) {
                return Err(Error::Runtime(
                    format!(
                        "Object '{}' extends '{current}', which is already in its own parent chain",
                        pending.name
                    ),
                    self.span(),
                ));
            }
            let Some(object) = self.objects.get(&current).cloned() else {
                return Err(Error::Runtime(
                    format!(
                        "Object '{}' extends '{current}', which is not declared",
                        pending.name
                    ),
                    self.span(),
                ));
            };
            seen.push(current.clone());
            parent = None;
            chain.push(object);
        }

        for object in chain.iter().rev() {
            for (field, value) in &object.fields {
                fields.entry(field.clone()).or_insert_with(|| value.clone());
            }
            for (method, function) in &object.methods {
                methods
                    .entry(method.clone())
                    .or_insert_with(|| function.clone());
            }
        }

        self.objects.insert(
            pending.name,
            ObjectType {
                methods,
                fields: fields.clone(),
            },
        );
        Ok(fields)
    }

    fn step(&mut self) -> Result<()> {
        let frame = self.frames.len() - 1;
        let ip = self.frames[frame].ip;
        let Some(instruction) = self.instruction_at(frame, ip)? else {
            self.frames[frame].ip = usize::MAX;
            return Ok(());
        };
        let previous_span = std::mem::replace(&mut self.current_span, line_span(instruction.line));
        let outcome = self
            .charge_step()
            .and_then(|()| self.execute(instruction, frame));
        self.current_span = previous_span;
        outcome
    }

    fn advance(&mut self, frame: usize) {
        if let Some(frame) = self.frames.get_mut(frame) {
            frame.ip = frame.ip.saturating_add(1);
        }
    }

    fn set_ip(&mut self, frame: usize, target: usize) {
        let limit = self.code_len(frame);
        if let Some(frame) = self.frames.get_mut(frame) {
            frame.ip = target.min(limit);
        }
    }

    fn bind(&mut self, frame: usize, name: &str, value: Value) {
        if self.frames[frame].globals_only {
            self.globals.insert(name.to_string(), value);
        } else {
            self.set_var(name, value);
        }
    }

    // -- instructions -------------------------------------------------------

    fn execute(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let opcode = instruction.opcode;
        match opcode {
            Opcode::Nop => {
                self.advance(frame);
                Ok(())
            }
            Opcode::PushConst => {
                let value = self.constant_value(instruction.arg)?;
                self.push(value);
                self.advance(frame);
                Ok(())
            }
            Opcode::Pop => {
                self.pop()?;
                self.advance(frame);
                Ok(())
            }
            Opcode::Load => {
                let name = self.constant_text(instruction.arg)?;
                let value = match self.get_var(&name) {
                    Some(value) => value,
                    // A module is named by the receiver of a `module.function`
                    // call and is never bound to anything: the tree-walking VM
                    // resolves such a call from the receiver's *name*, so it never
                    // reads the name as a variable at all. A file that pushes the
                    // receiver does, so an unbound module name reads as `nothing`
                    // — which is the value the tree-walking VM binds an imported
                    // name to anyway.
                    None if stdlib::is_module(&name) => Value::Nothing,
                    None => {
                        return Err(Error::Runtime(
                            format!("Unknown variable '{}'", name),
                            self.span(),
                        ))
                    }
                };
                self.push(value);
                self.advance(frame);
                Ok(())
            }
            Opcode::Store => self.store(instruction, frame),
            Opcode::Say => {
                let value = self.pop()?;
                self.output.push(value.to_string());
                self.advance(frame);
                Ok(())
            }
            Opcode::Print => {
                let value = self.pop()?;
                print!("{}", value);
                self.advance(frame);
                Ok(())
            }
            Opcode::LoadProperty => {
                let object = self.pop()?;
                let field = self.constant_text(instruction.arg)?;
                let value = runtime::property(self.span(), object, &field)?;
                self.push(value);
                self.advance(frame);
                Ok(())
            }
            Opcode::SetProperty => self.set_property(instruction, frame),
            Opcode::Index => {
                let index_value = self.pop()?;
                let object = self.pop()?;
                let value = runtime::index(self.span(), object, index_value)?;
                self.push(value);
                self.advance(frame);
                Ok(())
            }
            Opcode::BuildList => {
                let values = self.pop_n(instruction.arg)?;
                self.push(Value::List(values));
                self.advance(frame);
                Ok(())
            }
            Opcode::BuildRecord => {
                let flat = self.pop_n(instruction.arg.saturating_mul(2))?;
                let mut fields = Fields::new();
                for pair in flat.chunks_exact(2) {
                    let Value::Text(key) = &pair[0] else {
                        return Err(Error::Runtime(
                            "a record key must be text".to_string(),
                            self.span(),
                        ));
                    };
                    fields.insert(key.clone(), pair[1].clone());
                }
                self.push(Value::Record(fields));
                self.advance(frame);
                Ok(())
            }
            Opcode::BuildText => {
                let parts = self.pop_n(instruction.arg)?;
                let mut text = String::new();
                for part in parts {
                    text.push_str(&part.to_string());
                }
                self.push(Value::Text(text));
                self.advance(frame);
                Ok(())
            }
            Opcode::Add
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
            | Opcode::In => {
                let right = self.pop()?;
                let left = self.pop()?;
                let value = runtime::binary_op(self.span(), binary_operator(opcode), left, right)?;
                self.push(value);
                self.advance(frame);
                Ok(())
            }
            Opcode::Neg | Opcode::Not => {
                let value = self.pop()?;
                let value = runtime::unary_op(self.span(), unary_operator(opcode), value)?;
                self.push(value);
                self.advance(frame);
                Ok(())
            }
            Opcode::Call => {
                let name = self.constant_text(instruction.arg)?;
                let args = self.pop_n(instruction.aux)?;
                // Advanced before the call, because a call pushes a frame and the
                // caller's next instruction is the one after this one — not this
                // one again when the callee returns.
                self.advance(frame);
                self.call_named(&name, &args)
            }
            Opcode::CallMethod => self.call_method(instruction, frame),
            Opcode::Return => self.return_value(frame),
            Opcode::Break => self.break_loop(frame),
            Opcode::Skip => self.skip_loop(frame),
            Opcode::Jump => self.jump(instruction, frame),
            Opcode::JumpIfFalse => {
                let target = instruction.arg as usize;
                let value = self.pop()?;
                if value.is_truthy() {
                    self.advance(frame);
                } else {
                    self.set_ip(frame, target);
                }
                Ok(())
            }
            Opcode::GetIter => {
                let value = self.pop()?;
                // Anything that is not a list has no values to draw, so the loop
                // runs no times — the tree-walking VM does the same rather than
                // failing.
                let sequence = match value {
                    Value::List(items) => Sequence::Each { items, index: 0 },
                    _ => Sequence::Each {
                        items: Vec::new(),
                        index: 0,
                    },
                };
                self.open_loop(Some(sequence), frame);
                Ok(())
            }
            Opcode::GetRange => self.start_range(instruction, frame),
            Opcode::DefFunction => {
                let (chunk, path) = self.child_path(frame, instruction.arg)?;
                let block = self.child_block(frame, instruction.arg)?;
                let closure = self.make_function(&chunk, &path, &block);
                self.push(closure);
                self.advance(frame);
                Ok(())
            }
            Opcode::DefMethod => {
                let (chunk, path) = self.child_path(frame, instruction.arg)?;
                let block = self.child_block(frame, instruction.arg)?;
                let method = self.make_function(&chunk, &path, &block);
                let name = block.name.clone();
                if let Some(pending) = self.pending_object.as_mut() {
                    pending.methods.insert(name, method);
                } else {
                    // A `to can` outside an object body binds its own name as a
                    // value, which is what the tree-walking VM does with it.
                    self.set_var(&name, method);
                }
                self.advance(frame);
                Ok(())
            }
            Opcode::DefObject => self.def_object(instruction, frame),
            Opcode::DefField => {
                let value = self.pop()?;
                let name = self.constant_text(instruction.arg)?;
                let Some(pending) = self.pending_object.as_mut() else {
                    return Err(Error::Runtime(
                        format!("'has {name}' is only valid inside an object declaration"),
                        self.span(),
                    ));
                };
                pending.fields.insert(name, value);
                self.advance(frame);
                Ok(())
            }
            Opcode::Try => self.push_handler(instruction, frame),
            Opcode::Import => self.import(instruction, frame),
            Opcode::Test => {
                let (chunk, path) = self.child_path(frame, instruction.arg)?;
                let mut new_frame = self.frame_for(&chunk, path);
                new_frame.stack_base = self.stack.len();
                self.frames.push(new_frame);
                Ok(())
            }
            Opcode::Expect => {
                let expected = self.pop()?;
                let actual = self.pop()?;
                match crate::testing::assertions::assert_values_equal(&expected, &actual) {
                    Ok(()) => {
                        self.advance(frame);
                        Ok(())
                    }
                    Err(failure) => {
                        self.expectation_failure = Some(failure.clone());
                        Err(Error::Runtime(failure.to_string(), self.span()))
                    }
                }
            }
        }
    }

    /// `JUMP`, which is a `while` loop's turn-over when it goes backwards.
    ///
    /// A sequence loop turns over at its `STORE`, not here, so only the `while`
    /// case charges an iteration and names a kind. A backward jump that belongs to
    /// no loop is followed rather than reported, so a file the compiler does not
    /// write still cannot hang.
    fn jump(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let target = instruction.arg as usize;
        if target < self.frames[frame].ip {
            if let Some(site) = self.frames[frame]
                .sites
                .iter()
                .find(|site| site.top as usize == target && !site.iterator)
                .copied()
            {
                let index = self.loop_entry(frame, site);
                let kind = self.loops[index].kind;
                let max = self.max_iterations;
                let span = self.span();
                let iterations = &mut self.loops[index].iterations;
                charge_iteration(max, iterations, kind, span)?;
            }
        }
        self.set_ip(frame, target);
        Ok(())
    }

    // -- loops --------------------------------------------------------------

    /// `GET_RANGE`: builds a loop's sequence from its operands.
    ///
    /// `aux` counts them: one for `repeat n times`, two for `for each x from a to
    /// b`, three when a step is given.
    fn start_range(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let operands = self.pop_n(instruction.aux)?;
        let sequence = match instruction.aux {
            1 => Sequence::Repeat {
                remaining: match operands.first() {
                    Some(Value::Number(n)) => *n as i64,
                    _ => 0,
                },
            },
            _ => {
                let (Some(Value::Number(start)), Some(Value::Number(end))) =
                    (operands.first(), operands.get(1))
                else {
                    // Bounds that are not numbers run no times, which is what the
                    // tree-walking VM does with them.
                    self.open_loop(Some(Sequence::Repeat { remaining: 0 }), frame);
                    return Ok(());
                };
                let step = match operands.get(2) {
                    Some(Value::Number(n)) => *n,
                    _ => 1.0,
                };
                Sequence::Range {
                    current: *start,
                    end: *end,
                    step,
                }
            }
        };
        self.open_loop(Some(sequence), frame);
        Ok(())
    }

    /// Records the loop the instruction after the current one starts.
    ///
    /// A placeholder is pushed purely so the operand stack height is where the
    /// file expects it; the sequence itself is held in the loop entry, and the
    /// `STORE` at the loop's `top` draws from it.
    fn open_loop(&mut self, sequence: Option<Sequence>, frame: usize) {
        let top = self.frames[frame].ip as u32 + 1;
        let Some(site) = self.frames[frame]
            .sites
            .iter()
            .find(|site| site.top == top)
            .copied()
        else {
            // Nothing jumps back to the next instruction, so this is not a loop
            // the file wrote a loop shape for. The body runs once, which is all a
            // file that says this can mean.
            self.advance(frame);
            return;
        };
        let stack_base = self.stack.len().saturating_sub(1);
        self.loops.push(Loop {
            kind: site.kind,
            iterator: sequence,
            top: site.top,
            back_edge: site.back_edge,
            exit: site.exit,
            iterations: 0,
            stack_base,
        });
        self.advance(frame);
    }

    /// `STORE`, which is either a plain binding or a loop's variable.
    ///
    /// A loop's variable is the one `STORE` that sits on its loop's `top`: the
    /// loop keeps a placeholder on the operand stack for the whole body, and the
    /// instruction at `top` is where the next value is drawn. Every other `STORE`
    /// binds what the statement pushed.
    fn store(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let ip = self.frames[frame].ip as u32;
        let name = self.constant_text(instruction.arg)?;
        let Some(site) = self.frames[frame]
            .sites
            .iter()
            .find(|site| site.iterator && site.top == ip)
            .copied()
        else {
            let value = self.pop()?;
            self.bind(frame, &name, value);
            self.advance(frame);
            return Ok(());
        };

        let index = self.loop_entry(frame, site);
        // The value for this turn is taken before the turn is charged, so a loop
        // that has run out does not spend an iteration on the fact.
        let has_next = self.loops[index]
            .iterator
            .as_ref()
            .and_then(|sequence| peek(sequence))
            .is_some();
        if !has_next {
            self.leave_loop(frame, index);
            return Ok(());
        }
        let kind = self.loops[index].kind;
        let max = self.max_iterations;
        let span = self.span();
        let iterations = &mut self.loops[index].iterations;
        charge_iteration(max, iterations, kind, span)?;
        let sequence = self.loops[index]
            .iterator
            .as_mut()
            .expect("a sequence for a sequence loop");
        let value = take(sequence, span)?.unwrap_or(Value::Nothing);
        self.bind(frame, &name, value);
        self.advance(frame);
        Ok(())
    }

    /// The index in [`BytecodeVm::loops`] of the loop at `site`, creating it the
    /// first time that loop is reached.
    fn loop_entry(&mut self, frame: usize, site: LoopSite) -> usize {
        if let Some(index) = self
            .loops
            .iter()
            .position(|entry| entry.top == site.top && entry.back_edge == site.back_edge)
        {
            return index;
        }
        self.loops.push(Loop {
            kind: site.kind,
            iterator: None,
            top: site.top,
            back_edge: site.back_edge,
            exit: site.exit,
            iterations: 0,
            stack_base: self.frames[frame].stack_base,
        });
        self.loops.len() - 1
    }

    /// Leaves the loop at `index`: its placeholder is released and execution
    /// continues after it.
    fn leave_loop(&mut self, frame: usize, index: usize) {
        let exit = self.loops[index].exit;
        let base = self.loops[index].stack_base;
        self.loops.remove(index);
        self.stack.truncate(base);
        self.set_ip(frame, exit as usize);
    }

    /// `break`: leaves the innermost loop the current instruction is inside.
    fn break_loop(&mut self, frame: usize) -> Result<()> {
        let Some(index) = self.enclosing_loop(frame) else {
            return Err(Error::Runtime(
                "'break' is not inside a loop".to_string(),
                self.span(),
            ));
        };
        self.leave_loop(frame, index);
        Ok(())
    }

    /// `skip`: the innermost loop goes on to its next turn.
    fn skip_loop(&mut self, frame: usize) -> Result<()> {
        let Some(index) = self.enclosing_loop(frame) else {
            return Err(Error::Runtime(
                "'skip' is not inside a loop".to_string(),
                self.span(),
            ));
        };
        let top = self.loops[index].top as usize;
        self.set_ip(frame, top);
        Ok(())
    }

    /// The innermost active loop whose region holds the current instruction.
    fn enclosing_loop(&self, frame: usize) -> Option<usize> {
        let ip = self.frames[frame].ip as u32;
        self.loops
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.top <= ip && ip <= entry.back_edge)
            .map(|(index, _)| index)
            .next_back()
    }

    // -- properties ---------------------------------------------------------

    /// `SET_PROPERTY`: writes a field back to the binding its name denotes.
    fn set_property(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let value = self.pop()?;
        let target = self.constant_text(instruction.arg)?;
        let Some((object, field)) = split_dotted(&target) else {
            return Err(Error::Runtime(
                format!("bytecode names the field '{target}' without an object"),
                self.span(),
            ));
        };
        // The tree-walking VM reads the object out of the variable, and an object
        // that is not a record is left alone rather than reported.
        if let Some(Value::Record(mut fields)) = self.get_var(object) {
            fields.insert(field.to_string(), value);
            self.bind(frame, object, Value::Record(fields));
        }
        self.advance(frame);
        Ok(())
    }

    // -- returns ------------------------------------------------------------

    /// `RETURN`.
    ///
    /// A `return` inside a `try` does not leave the function in the tree-walking
    /// VM — it yields a value and the next statement runs — so a bytecode `RETURN`
    /// with a handler installed above this frame does the same rather than
    /// quietly abandoning the handlers.
    fn return_value(&mut self, frame: usize) -> Result<()> {
        let value = self.pop()?;
        if self.handlers.len() > self.frames[frame].handler_base {
            self.advance(frame);
            return Ok(());
        }
        self.stack.truncate(self.frames[frame].stack_base);
        self.stack.push(value);
        self.frames[frame].ip = usize::MAX;
        Ok(())
    }

    // -- calls --------------------------------------------------------------

    /// Calls `name`, which is either a builtin or a declared function.
    fn call_named(&mut self, name: &str, args: &[Value]) -> Result<()> {
        if let Some(value) = runtime::builtin(self.span(), name, args)? {
            self.push(value);
            return Ok(());
        }
        match self.get_var(name) {
            Some(Value::Function(function)) => self.call_function(&function, args),
            _ => Err(Error::Runtime(
                format!("Unknown function '{}'", name),
                self.span(),
            )),
        }
    }

    /// `receiver.method(args)`.
    ///
    /// Resolved against the receiver's *name*, as the tree-walking VM resolves
    /// it: a name that is a declared `object` is a method on that type, and
    /// anything else is the `receiver_method` spelling a module function is
    /// written with.
    fn call_method(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let dotted = self.constant_text(instruction.arg)?;
        let args = self.pop_n(instruction.aux)?;
        let receiver = self.pop()?;
        self.advance(frame);

        let Some((object_name, method)) = split_dotted(&dotted) else {
            return Err(Error::Runtime(
                format!(
                    "Cannot call method '{}' on {}, which is not an object",
                    dotted, receiver
                ),
                self.span(),
            ));
        };

        let Some(object) = self.objects.get(object_name).cloned() else {
            return self.call_named(&format!("{object_name}_{method}"), &args);
        };
        let Some(Value::Function(function)) = object.methods.get(method).cloned() else {
            return Err(Error::Runtime(
                format!("Object '{object_name}' has no method '{method}'"),
                self.span(),
            ));
        };
        // `this` is bound for the length of the call, in a scope of its own.
        self.locals.push(CapturedScope::new());
        self.locals
            .last_mut()
            .expect("a scope to bind into")
            .insert("this".to_string(), receiver);
        let outcome = self.call_function(&function, &args);
        self.locals.pop();
        outcome
    }

    /// Runs a closure's block, binding `args` to its parameters.
    fn call_function(&mut self, function: &FunctionValue, args: &[Value]) -> Result<()> {
        if self.call_depth >= self.max_call_depth {
            return Err(Error::Runtime(
                format!(
                    "Maximum call depth of {} reached while calling '{}'",
                    self.max_call_depth, function.name
                ),
                self.span(),
            ));
        }
        let Some((chunk, path)) = function.body.block() else {
            return Err(Error::Runtime(
                format!(
                    "'{}' has no compiled body, which only `rb vm` can run",
                    function.name
                ),
                self.span(),
            ));
        };
        let (chunk, path) = (chunk.clone(), path.to_vec());
        let block = self.block_of(&chunk, &path)?.clone();
        // The captured scopes go on the stack *above* the caller's frames, and
        // the parameters above those, so the body reads its own environment
        // rather than a same-named binding of whoever called it, and a parameter
        // shadows what it captured.
        let locals_base = self.locals.len();
        let captured: Captured = (*function.captured).clone();
        for scope in captured {
            self.locals.push(scope);
        }
        self.locals.push(CapturedScope::new());
        for (index, param) in block.params.iter().enumerate() {
            let value = args.get(index).cloned().unwrap_or(Value::Nothing);
            self.locals
                .last_mut()
                .expect("a scope to bind into")
                .insert(param.clone(), value);
        }
        let mut new_frame = self.frame_for(&chunk, path);
        new_frame.locals_base = locals_base;
        new_frame.stack_base = self.stack.len();
        new_frame.is_call = true;
        self.call_depth += 1;
        self.frames.push(new_frame);
        Ok(())
    }

    /// The closure for a declared function, closing over every local scope live
    /// where the declaration is executed.
    fn make_function(&self, chunk: &Arc<Chunk>, path: &[u32], block: &Block) -> Value {
        let captured: Captured = self.locals.clone();
        Value::Function(FunctionValue {
            name: block.name.clone(),
            params: block.params.clone(),
            body: FunctionBody::Block {
                chunk: chunk.clone(),
                path: path.to_vec(),
            },
            captured: Arc::new(captured),
        })
    }

    /// `DEF_OBJECT`: starts an object body, whose instructions declare the
    /// fields and the methods and then register the type.
    fn def_object(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let name = self.object_name(frame, instruction)?;
        if self.objects.contains_key(&name) {
            return Err(Error::Runtime(
                format!("Object '{name}' is already declared"),
                self.span(),
            ));
        }
        let parent = if instruction.aux == NO_CONST {
            None
        } else {
            Some(self.constant_text(instruction.aux)?)
        };
        let (chunk, path) = self.child_path(frame, instruction.arg)?;
        let mut new_frame = self.frame_for(&chunk, path);
        new_frame.object_body = true;
        new_frame.stack_base = self.stack.len();
        new_frame.locals_base = self.locals.len();
        self.pending_object = Some(ObjectPending {
            name,
            parent,
            fields: Fields::new(),
            methods: Fields::new(),
        });
        self.frames.push(new_frame);
        Ok(())
    }

    /// The name the object instruction declares: the one the `STORE` right after
    /// it binds.
    fn object_name(&self, frame: usize, instruction: Instruction) -> Result<String> {
        let next = self.frames[frame].ip + 1;
        if let Some(following) = self.instruction_at(frame, next)? {
            if following.opcode == Opcode::Store {
                return self.constant_text(following.arg);
            }
        }
        Ok(self.child_block(frame, instruction.arg)?.name.clone())
    }

    // -- try ----------------------------------------------------------------

    /// `TRY`: installs the handlers the rest of this block runs under.
    fn push_handler(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let catch = if instruction.arg == NO_BLOCK {
            None
        } else {
            let (chunk, path) = self.child_path(frame, instruction.arg)?;
            let catch_var = self.block_of(&chunk, &path)?.name.clone();
            Some(((chunk, path), catch_var))
        };
        let finally = if instruction.aux == NO_BLOCK {
            None
        } else {
            Some(self.child_path(frame, instruction.aux)?)
        };
        self.handlers.push(Handler {
            catch,
            finally,
            stack_base: self.frames[frame].stack_base,
            loop_base: self.loops.len(),
        });
        self.advance(frame);
        Ok(())
    }

    /// Runs the handlers of the innermost installed `try`, and answers whether
    /// the failure was handled.
    ///
    /// A handler that fails itself leaves that failure in place rather than
    /// swallowing it, which is what the tree-walking VM does: the second `?` in
    /// its `try` propagates.
    fn handle_failure(&mut self, _error: &Error) -> Result<bool> {
        let frame = self.frames.len() - 1;
        if self.handlers.len() <= self.frames[frame].handler_base {
            return Ok(false);
        }
        let handler = self.handlers.pop().expect("a handler");
        self.loops.truncate(handler.loop_base);
        self.stack.truncate(handler.stack_base);

        if let Some(((chunk, path), catch_var)) = handler.catch {
            // A `catch` binds the failure to its name. The tree-walking VM binds
            // the word `error`, so this does too rather than inventing a message
            // the language does not produce.
            self.locals.push(CapturedScope::new());
            if !catch_var.is_empty() {
                self.locals
                    .last_mut()
                    .expect("a scope to bind into")
                    .insert(catch_var, Value::Text("error".to_string()));
            }
            let depth = self.frames.len();
            let mut catch_frame = self.frame_for(&chunk, path);
            catch_frame.locals_base = self.locals.len() - 1;
            catch_frame.stack_base = handler.stack_base;
            self.frames.push(catch_frame);
            self.drive(depth)?;
            self.locals.pop();
        }
        if let Some((chunk, path)) = handler.finally {
            let depth = self.frames.len();
            let locals_base = self.locals.len();
            let mut finally_frame = self.frame_for(&chunk, path);
            finally_frame.locals_base = locals_base;
            finally_frame.stack_base = handler.stack_base;
            self.frames.push(finally_frame);
            self.drive(depth)?;
        }
        if self.frames.is_empty() {
            return Ok(true);
        }
        // The protected code is the rest of the block, so the statement after it
        // is past the end of the block. Inside a loop the next turn is where
        // execution goes on.
        let resume = self
            .loops
            .last()
            .map(|entry| entry.top as usize)
            .unwrap_or_else(|| self.code_len(self.frames.len() - 1));
        self.set_ip(self.frames.len() - 1, resume);
        Ok(true)
    }

    // -- imports ------------------------------------------------------------

    /// `IMPORT`: loads a module's `set` bindings into the globals, and the
    /// `STORE` the file writes next binds the name.
    ///
    /// Only the `set` statements are kept, and they are compiled and run here
    /// rather than evaluated from the module's syntax tree, because this VM has
    /// no evaluator — but the selection is the tree-walking VM's, so the bindings
    /// that appear are the same ones.
    fn import(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let name = self.constant_text(instruction.arg)?;
        for path in module_paths(&name) {
            if std::path::Path::new(&path).exists() {
                let chunk = Arc::new(self.compile_module(&path)?);
                let depth = self.frames.len();
                let mut module_frame = self.frame_for(&chunk, Vec::new());
                module_frame.globals_only = true;
                module_frame.stack_base = self.stack.len();
                module_frame.locals_base = self.locals.len();
                self.frames.push(module_frame);
                self.drive(depth)?;
                self.advance(frame);
                return Ok(());
            }
        }
        Err(Error::Runtime(
            format!("Cannot find module '{}'", name),
            self.span(),
        ))
    }

    /// Compiles a module down to the `set` statements its top level declares.
    fn compile_module(&self, path: &str) -> Result<Chunk> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| Error::Io(format!("Cannot load module '{}': {}", path, e)))?;
        let tokens = Lexer::tokenize(&source)?;
        let ast = parser::parse(tokens)?;
        let program = Program {
            statements: ast
                .statements
                .iter()
                .filter(|stmt| matches!(stmt.statement, Statement::Set { .. }))
                .cloned()
                .collect(),
        };
        crate::bytecode::compile_program(&program)
    }
}

/// The three limits a program runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_steps: usize,
    pub max_iterations: usize,
    pub max_call_depth: usize,
}

/// The limits this build enforces, which are the tree-walking VM's: one program
/// runs under one budget whichever VM executes it.
pub fn limits() -> Limits {
    Limits {
        max_steps: resolve_max_steps(),
        max_iterations: resolve_max_iterations(),
        max_call_depth: resolve_max_call_depth(),
    }
}

impl Default for Limits {
    fn default() -> Self {
        limits()
    }
}

/// Runs `chunk` with the configured limits and returns what `main` produced.
pub fn run(chunk: &Chunk) -> Result<Value> {
    BytecodeVm::new().run(chunk)
}
