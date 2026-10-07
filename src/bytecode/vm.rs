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
//! That is what makes deep recursion safe here: [`BytecodeVm::call_depth`]
//! counts frames in a `Vec`, so a call chain as deep as
//! [`DEFAULT_MAX_CALL_DEPTH`](crate::vm::MAX_CALL_DEPTH) costs heap rather than
//! machine stack. There is no path from a `.rbc` to a native stack overflow.
//!
//! # What the file has to say
//!
//! Names are stored rather than resolved to slots, which means two opcodes have
//! to carry two names in one operand: `CALL_METHOD` and `SET_PROPERTY` carry
//! `receiver.method` and `object.property`. A call is resolved against the
//! receiver's *name* — `files.read` is the builtin `files_read`, and
//! `Counter.bump` is a method on a declared type — so a file that held only the
//! method name could not say which one it meant.
//!
//! Loops are the one place where a block's *shape* matters. A loop is a backward
//! `JUMP`, and the instruction it jumps to is the loop's `top`: the `STORE` that
//! binds the loop variable for `for each` and `repeat`, and the first
//! instruction of the condition for a `while`. [`loop_sites`] finds those once
//! per block; everything else about a loop follows from them.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::bytecode::format::{Block, Chunk, Constant, Instruction};
use crate::bytecode::opcode::Opcode;
use crate::bytecode::{END_TRY_MARKER, NO_BLOCK, NO_CONST};
use crate::error::{Error, Result, Span};
use crate::lexer::Lexer;
use crate::parser::{self, BinaryOp, Program, Statement, UnaryOp};
use crate::runtime;
use crate::stdlib;
use crate::value::{
    expect_range_number, finite_number, range_has_next, Captured, CapturedScope, Fields,
    FunctionBody, FunctionValue, Value,
};
use crate::vm::{qualified_member, DeclaredModule};
use crate::vm::{
    resolve_max_call_depth, resolve_max_iterations, resolve_max_iterations_from, resolve_max_steps,
    resolve_max_steps_from, MAX_CALL_DEPTH, MAX_ITERATIONS, MAX_STEPS,
};

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
    /// The instruction that jumps back to `top`. It is the last instruction of
    /// the loop body, so it is also the last one `break` is inside.
    back_edge: u32,
    /// Where the loop is left when the body is finished or `break` runs.
    exit: u32,
    /// Turns run so far, against [`BytecodeVm::max_iterations`].
    iterations: usize,
    /// The frame running this loop, so that a loop is looked up by the block it
    /// is in rather than by its offsets alone.
    frame: usize,
    /// The operand stack height before this loop pushed anything, which is what
    /// leaving the loop truncates back to.
    stack_base: usize,
    /// The binding this loop's variable shadowed when the loop started, put back
    /// when the loop is left, or `None` for a loop that binds no variable.
    ///
    /// The tree-walking VM pushes a scope for each turn of a `for each`, declares
    /// the variable in it and pops the scope when the turn ends, so the variable
    /// belongs to the loop and a name of the same name outside it is untouched.
    /// This VM compiles the body inline and so has one binding for the whole loop;
    /// recording what it displaced is what makes leaving the loop restore the
    /// outer name.
    variable: Option<LoopVariable>,
}

/// The name a loop binds, and what it was bound to before.
struct LoopVariable {
    /// The name the loop's `STORE` writes.
    name: String,
    /// What the name was bound to when the loop started.
    shadowed: Shadowed,
}

/// The value a name was bound to before a loop started, and where.
#[derive(Default)]
enum Shadowed {
    /// The name was bound nowhere, so leaving the loop unbinds it.
    #[default]
    Unbound,
    /// The name was bound in the local scope at this index, to this value.
    Local(usize, Value),
    /// The name was bound as a global, to this value.
    Global(Value),
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
        Sequence::Range {
            current, end, step, ..
        } => {
            if range_has_next(*current, *end, *step) {
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
/// A range's counter is checked against the step here rather than after the
/// body, because a step that overflows it is the same failure as one that would
/// put an infinity in the loop variable, and the tree-walking VM raises it at the
/// same point.
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
            if !range_has_next(*current, *end, *step) {
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

/// A `catch` body paired with the name its handler binds.
type CatchHandler = ((Arc<Chunk>, Vec<u32>), String);

/// One `try` whose protected code is still running.
///
/// `TRY` names its handlers rather than jumping to them, and the protected code
/// is compiled inline, so a handler covers the instructions up to the marked
/// `NOP` that closes the region — which is what `docs/BYTECODE.md` specifies and
/// what this VM implements; a `try` that is the last statement of its block —
/// which is what every program in `examples/` and `tests/` writes — covers
/// exactly the statements the tree-walking VM protects.
#[derive(Clone)]
struct Handler {
    /// The `catch` body together with the name it binds, or `None` for a `try`
    /// with no `catch`.
    catch: Option<CatchHandler>,
    /// The name a `catch` binds, empty when there is no `catch` to bind it.
    catch_var: String,
    /// The `finally` body, or `None` for a `try` with no `finally`.
    finally: Option<(Arc<Chunk>, Vec<u32>)>,
    /// The operand stack height the protected code started at, which is what the
    /// handlers start from.
    stack_base: usize,
    /// How many loops were running when the protected code started, so a handler
    /// does not inherit a sequence the failed code left half drawn.
    loop_base: usize,
}

/// One `object` declaration being assembled. Its body declares the fields and
/// the methods, and the type is registered when the body finishes.
struct ObjectPending {
    name: String,
    parent: Option<String>,
    fields: Fields,
    methods: Fields,
}

/// One `object` declaration, after its parent has been merged into it.
#[derive(Clone)]
struct ObjectType {
    /// The object this one extends, kept so that a child can be resolved
    /// against its own parent after a later declaration changes nothing.
    parent: Option<String>,
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
    /// The `module ... end` declaration this frame is running the body of, or
    /// `None` for every other frame.
    ///
    /// The frame is what carries the declaration's name, because a module body
    /// is published when the frame finishes — and a frame that a failure
    /// discards rather than finishes has to unregister the module rather than
    /// leave it half-declared, which needs the name here too.
    module_body: Option<String>,
    /// The names the module declaration in `module_body` publishes, read out of
    /// the body block's leading `EXPORT` run when the declaration began. Empty
    /// for every other frame, and for a module that publishes nothing — which is
    /// what a body with no `export` at all says.
    module_exports: Vec<String>,
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
/// instruction the compiler inserted, and has no line to name.
fn line_span(line: u32) -> Span {
    Span::new(line as usize, 1)
}

/// The `BinaryOp` an arithmetic opcode stands for.
///
/// The operators are the parser's own enum rather than a second copy, which is
/// what makes a bytecode `ADD` and a tree-walked `+` the same operation instead
/// of two that agree today.
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

/// Where the `try` name a file wrote splits into the two names it carries.
///
/// `CALL_METHOD` carries `receiver.method` and `SET_PROPERTY` carries
/// `object.property`. A name with no dot in it is the one shape the language
/// does not have: a method call whose receiver was an expression rather than a
/// name.
fn split_dotted(name: &str) -> Option<(&str, &str)> {
    name.find('.').map(|dot| (&name[..dot], &name[dot + 1..]))
}

/// Runs a compiled Redblue program.
pub struct BytecodeVm {
    globals: HashMap<String, Value>,
    /// The names bound by a `constant` declaration. The value lives in
    /// [`BytecodeVm::globals`] so a read resolves through one name-resolution
    /// path; what this table adds is the refusal to rebind the name.
    constants: HashSet<String>,
    /// The live local scopes, outermost first. A call's parameters are the
    /// innermost scope, and a closure pushes the scopes it captured above its
    /// caller's frames — see [`BytecodeVm::call_function`].
    locals: Vec<CapturedScope>,
    /// The lines `say` has produced. Printed when the program finishes, which is
    /// where the tree-walking VM prints them too, so a program that mixes `say`
    /// and `print` writes its lines in the same order either way.
    output: Vec<String>,
    /// Every `object` declaration, by type name. See [`ObjectType`].
    objects: HashMap<String, ObjectType>,
    /// The modules an `import` has already run, by the name the program wrote.
    ///
    /// A module is run once, because running it twice declares its names twice:
    /// a `constant` cannot be declared twice at all, so a second `import` of the
    /// same module would fail on the module's own first declaration. The
    /// tree-walking VM keeps the parsed program here for the same reason — see
    /// [`crate::vm::Vm::load_module`], which answers the second import with a
    /// no-op.
    modules: HashSet<String>,
    /// The name each imported module is called by, to the module it names.
    ///
    /// An `import X as Y` binds `Y`, and a call written `Y.member` has to reach
    /// the same function `X.member` does — which is a function named
    /// `X_member`, not `Y_member`. The tree-walking VM keeps the same table; see
    /// [`crate::vm::Vm`]'s `module_aliases`.
    module_aliases: HashMap<String, String>,
    /// Every `module` declaration that has run, by module name.
    ///
    /// An entry is what a second declaration of the same name is refused
    /// against, what `is_module_name` counts, and what an `import` of the name
    /// finds in place of a file. The tree-walking VM keeps the same table; see
    /// [`crate::vm::Vm`].
    declared_modules: HashMap<String, DeclaredModule>,
    /// How many `module` bodies are running. Above zero a `set` binds in the
    /// module's own scope rather than becoming a global, which is what makes a
    /// module declaration a boundary rather than a run of top-level statements.
    module_depth: usize,
    /// The chain of modules currently being loaded, outermost first.
    ///
    /// A module that reaches one already in the chain is a cycle. See
    /// [`BytecodeVm::import`].
    importing: Vec<String>,
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
    /// Builds a VM with the configured limits and an empty program.
    pub fn new() -> Self {
        Self {
            globals: stdlib::builtins(),
            constants: HashSet::new(),
            locals: vec![CapturedScope::new()],
            output: Vec::new(),
            objects: HashMap::new(),
            modules: HashSet::new(),
            module_aliases: HashMap::new(),
            declared_modules: HashMap::new(),
            module_depth: 0,
            importing: Vec::new(),
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

    /// Runs `chunk` and returns what `main` left as its value.
    ///
    /// The `say` output is printed when the program finishes rather than as it
    /// happens, which is where the tree-walking VM prints it too, so a program
    /// that mixes `say` and `print` writes its lines in the same order either
    /// way.
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

    /// Takes the lines `say` produced, for a caller that wants them rather than
    /// the printing.
    pub fn take_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.output)
    }

    // -- the block tree -----------------------------------------------------

    /// A frame for `path` of `chunk`, with this VM's current stack heights as
    /// its bases.
    fn frame_for(&self, chunk: &Arc<Chunk>, path: Vec<u32>) -> Frame {
        let sites = block_at(chunk, &path).map(loop_sites).unwrap_or_default();
        Frame {
            chunk: chunk.clone(),
            path,
            ip: 0,
            stack_base: self.stack.len(),
            loop_base: self.loops.len(),
            handler_base: self.handlers.len(),
            locals_base: self.locals.len(),
            object_body: false,
            module_body: None,
            module_exports: Vec::new(),
            is_call: false,
            globals_only: false,
            sites,
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
        let mut path = self.frames[frame].path.clone();
        path.push(arg);
        Ok((self.frames[frame].chunk.clone(), path))
    }

    /// The name of the block `arg` names inside the frame's own block, or a
    /// failure naming the path — a block index is an operand a file wrote, so it
    /// is checked rather than trusted.
    ///
    /// The name is taken out by value because the chunk it comes from is a clone
    /// that dies with this call, so a borrow of it could not outlive the frame
    /// that named the block.
    fn child_block_name(&self, frame: usize, arg: u32) -> Result<String> {
        let (chunk, path) = self.child_path(frame, arg)?;
        Ok(self.block_of(&chunk, &path)?.name.clone())
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

    // -- the limits ---------------------------------------------------------

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

    /// Charges one iteration to `iterations`.
    ///
    /// Called where the loop is about to run its body again, which is the same
    /// point the tree-walking VM charges: after a sequence has been found to have
    /// a value left, and after a `while`'s condition has come out true.
    fn charge_loop(&mut self, index: usize) -> Result<()> {
        let Some(entry) = self.loops.get_mut(index) else {
            return Ok(());
        };
        let (iterations, kind) = (&mut entry.iterations, entry.kind);
        if *iterations >= self.max_iterations {
            return Err(Error::Runtime(
                format!(
                    "Maximum of {} iterations reached in a '{}' loop",
                    self.max_iterations, kind
                ),
                self.span(),
            ));
        }
        *iterations += 1;
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
    ///
    /// A frame may only take what it pushed: anything below `stack_base` belongs
    /// to its caller, so asking for more than the frame pushed is malformed
    /// bytecode and is reported rather than reaching into the caller's values.
    fn pop_n(&mut self, count: u32) -> Result<Vec<Value>> {
        let count = count as usize;
        let floor = self.frames.last().map_or(0, |frame| frame.stack_base);
        let available = self.stack.len().saturating_sub(floor);
        if count > available {
            return Err(Error::Runtime(
                format!("bytecode asked for {count} values its frame never pushed"),
                self.span(),
            ));
        }
        let base = self.stack.len() - count;
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
    ///
    /// Inside a `module` body a name no scope holds binds in the module's own
    /// scope rather than becoming a global, which is the boundary the
    /// declaration draws — the same rule the tree-walking VM writes. A name an
    /// enclosing scope already holds is still that scope's, which the loop above
    /// has already answered.
    fn set_var(&mut self, name: &str, value: Value) {
        for scope in self.locals.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return;
            }
        }
        if self.module_depth > 0 {
            if let Some(scope) = self.locals.last_mut() {
                scope.insert(name.to_string(), value);
            }
            return;
        }
        self.globals.insert(name.to_string(), value);
    }

    /// Binds `name` as a constant, refusing a second declaration of a name that
    /// already holds one.
    ///
    /// The refusal is the tree-walking VM's, down to the wording, because a
    /// `constant` that declared twice silently kept its first value in one VM
    /// and its second in the other would not be the same language.
    fn declare_const(&mut self, name: &str, value: Value) -> Result<()> {
        if !self.constants.insert(name.to_string()) {
            return Err(Error::Runtime(
                format!("Constant '{name}' is already declared"),
                self.span(),
            ));
        }
        self.globals.insert(name.to_string(), value);
        Ok(())
    }

    /// The refusal every write onto a constant shares: `Cannot assign to
    /// constant 'NAME'`.
    fn refuse_constant_rebind(&self, name: &str) -> Result<()> {
        if self.constants.contains(name) {
            return Err(Error::Runtime(
                format!("Cannot assign to constant '{name}'"),
                self.span(),
            ));
        }
        Ok(())
    }

    // -- the constant pool --------------------------------------------------

    fn constant(&self, index: u32) -> Result<Constant> {
        let chunk = self.frames[self.frames.len() - 1].chunk.clone();
        chunk.constants.get(index as usize).cloned().ok_or_else(|| {
            Error::Runtime(
                format!(
                    "bytecode names constant {index}, and the pool holds {}",
                    chunk.constants.len()
                ),
                self.span(),
            )
        })
    }

    /// The text constant at `index`, which is a name or a string literal.
    fn constant_text(&self, index: u32) -> Result<String> {
        match self.constant(index)? {
            Constant::Text(text) => Ok(text.clone()),
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
            Constant::Text(text) => Ok(Value::Text(text.clone())),
            // Checked here rather than at decode, so that a number a file
            // carries as `NaN` is refused when a program uses it rather than
            // making an otherwise sound file unreadable.
            Constant::Number(n) => Value::number(n, self.span()),
        }
    }

    // -- the interpreter loop -----------------------------------------------

    /// Runs instructions until the frame stack is down to `base`.
    fn drive(&mut self, base: usize) -> Result<()> {
        loop {
            // Unwinding is a place a failure can come from as much as stepping
            // is: an `object` body registers its type on the way out, so a
            // self-extending declaration faults here rather than at its
            // `DEF_OBJECT`. Both go through the handlers, or a `catch` around
            // such a declaration would not catch it.
            if self.frames.len() > base && self.top_finished() {
                match self.unwind_frame() {
                    Ok(()) => continue,
                    Err(error) => {
                        if !self.handle_failure(&error)? {
                            return Err(error);
                        }
                        continue;
                    }
                }
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

    /// Finishes the top frame, leaving its value for whoever was waiting on it.
    ///
    /// Only a *call* leaves a value behind. A frame entered as a block — a
    /// `test` body, a `try` handler, an `object` body, a module — is a
    /// statement, and a statement consumes what it produced, so those frames
    /// discard the stack rather than pushing onto their caller's. Pushing for
    /// them too would leak one operand-stack slot per block run, which a
    /// program with many `test` blocks notices as a stack that is deeper than its
    /// frames can account for.
    fn unwind_frame(&mut self) -> Result<()> {
        let Some(frame) = self.frames.pop() else {
            return Ok(());
        };
        if frame.module_body.is_some() {
            // Published before the module's own scope is taken away, because a
            // member is what the module's scope holds. See
            // `BytecodeVm::finish_module`.
            self.finish_module(&frame);
            self.module_depth = self.module_depth.saturating_sub(1);
            self.importing.pop();
        }
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
        if !frame.is_call {
            return Ok(());
        }
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
        let pending_parent = pending.parent.clone();
        let mut fields = pending.fields;
        let mut methods = pending.methods;

        let mut chain = Vec::new();
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
            parent = object.parent.clone();
            chain.push((current, object));
        }

        for (_, object) in chain.iter().rev() {
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
                parent: pending_parent,
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

    fn execute(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let opcode = instruction.opcode;
        match opcode {
            Opcode::Nop => {
                // A `NOP` is the filler and does nothing — including inside a
                // protected region, which is what keeps a filler a file happens
                // to contain from running an enclosing `finally` early. Only the
                // marker operand closes a region.
                if instruction.arg != END_TRY_MARKER {
                    self.advance(frame);
                    return Ok(());
                }
                // The protected region ran to here without failing, so only the
                // `finally` is owed. The handler is popped on this path so that
                // the failure path does not run the `finally` twice; it is
                // resumed *after* this instruction, so the two paths cannot both
                // be reaching this one. A marker with no handler above it is a
                // file the compiler did not write, and does nothing.
                let Some(handler) = self.handlers.last() else {
                    self.advance(frame);
                    return Ok(());
                };
                let handler = handler.clone();
                self.handlers.pop();
                self.run_finally(&handler)?;
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
                let value = self.get_var(&name).ok_or_else(|| {
                    Error::Runtime(format!("Unknown variable '{}'", name), self.span())
                })?;
                self.push(value);
                self.advance(frame);
                Ok(())
            }
            Opcode::Store => self.store(instruction, frame),
            Opcode::DeclareConst => {
                let name = self.constant_text(instruction.arg)?;
                let value = self.pop()?;
                self.declare_const(&name, value)?;
                self.advance(frame);
                Ok(())
            }
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
                for pair in flat.as_chunks::<2>().0 {
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
            Opcode::Call => self.call(instruction, frame),
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
            Opcode::GetIter => self.start_loop(None, frame),
            Opcode::GetRange => self.start_range(instruction, frame),
            Opcode::DefFunction => self.def_function(instruction, frame),
            Opcode::DefMethod => self.def_method(instruction, frame),
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
            Opcode::Module => self.module(instruction, frame),
            // The run of `EXPORT`s at the head of a module body is data the
            // `MODULE` that entered the block read before it started, so running
            // one does nothing. An `EXPORT` anywhere else is a file the compiler
            // did not write, and does nothing for the same reason.
            Opcode::Export => {
                self.advance(frame);
                Ok(())
            }
            Opcode::Test => {
                let (chunk, path) = self.child_path(frame, instruction.arg)?;
                let mut new_frame = self.frame_for(&chunk, path);
                new_frame.stack_base = self.stack.len();
                self.frames.push(new_frame);
                self.advance(frame);
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

    /// `JUMP`, which is a `while` loop's turn-over when it goes backwards.
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
                self.charge_loop(index)?;
            }
        }
        self.set_ip(frame, target);
        Ok(())
    }

    // -- loops --------------------------------------------------------------

    /// `GET_ITER`, and `GET_RANGE` after it has built its sequence.
    ///
    /// The sequence is pushed as a placeholder purely so the operand stack height
    /// is where the file expects it; the sequence itself is held in the loop
    /// entry, and the `STORE` at the loop's `top` draws from it.
    fn start_loop(&mut self, sequence: Option<Sequence>, frame: usize) -> Result<()> {
        let value = self.pop()?;
        let sequence = Some(sequence.unwrap_or(match value {
            // Anything that is not a list has no values to draw, so the loop runs
            // no times — the tree-walking VM does the same rather than failing.
            Value::List(items) => Sequence::Each { items, index: 0 },
            _ => Sequence::Each {
                items: Vec::new(),
                index: 0,
            },
        }));
        self.push_loop(sequence, frame);
        Ok(())
    }

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
                let span = self.span();
                let start =
                    expect_range_number(operands.first().unwrap_or(&Value::Nothing), "from", span)?;
                let end =
                    expect_range_number(operands.get(1).unwrap_or(&Value::Nothing), "to", span)?;
                let step = expect_range_number(
                    operands.get(2).unwrap_or(&Value::Number(1.0)),
                    "by",
                    span,
                )?;
                Sequence::Range {
                    current: start,
                    end,
                    step,
                }
            }
        };
        self.push_loop(Some(sequence), frame);
        Ok(())
    }

    /// Records the loop the instruction at `frame`'s current position starts.
    fn push_loop(&mut self, sequence: Option<Sequence>, frame: usize) {
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
            frame,
            stack_base,
            variable: None,
        });
        self.advance(frame);
    }

    /// `STORE`, which is either a plain binding or a loop's variable.
    ///
    /// A loop's variable is the one `STORE` that sits on its loop's `top`: the
    /// loop keeps its sequence on the operand stack for the whole body, and the
    /// instruction at `top` is where the next value is drawn from it. Every other
    /// `STORE` binds what the statement pushed.
    ///
    /// Only the plain binding refuses a write onto a constant. The loop-variable
    /// path is the loop's own per-turn binding of its iterator's element rather
    /// than a write the program wrote, and it shadows: the tree-walking VM gives
    /// each turn of a `for each` its own scope, so `constant X to 1` followed by
    /// `for each X in [1, 2]` reads 1 and 2 inside the loop and 1 after it.
    fn store(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let ip = self.frames[frame].ip as u32;
        let name = self.constant_text(instruction.arg)?;
        let Some(site) = self.frames[frame]
            .sites
            .iter()
            .find(|site| site.iterator && site.top == ip)
            .copied()
        else {
            // A write that lands in a scope is not a write to a constant, which
            // is why this is refused only above the module boundary: the
            // tree-walking VM refuses it in `set_var` on the same path.
            if self.module_depth == 0 {
                self.refuse_constant_rebind(&name)?;
            }
            let value = self.pop()?;
            self.bind(frame, &name, value);
            self.advance(frame);
            return Ok(());
        };

        let index = self.loop_entry(frame, site);
        // The value for this turn is taken before the turn is charged, so a loop
        // that has run out does not spend an iteration on the fact.
        let has_next = self.loops[index].iterator.as_ref().and_then(peek).is_some();
        if !has_next {
            self.leave_loop(frame, index);
            return Ok(());
        }
        self.charge_loop(index)?;
        let span = self.span();
        let sequence = self.loops[index].iterator.as_mut().expect("a sequence");
        let value = take(sequence, span)?.unwrap_or(Value::Nothing);
        self.bind(frame, &name, value);
        self.advance(frame);
        Ok(())
    }

    fn bind(&mut self, frame: usize, name: &str, value: Value) {
        if self.frames[frame].globals_only {
            self.globals.insert(name.to_string(), value);
        } else {
            self.set_var(name, value);
        }
    }

    /// The index in [`BytecodeVm::loops`] of the loop at `site`, creating it the
    /// first time that loop is reached.
    ///
    /// Matched on the frame as well as the offsets. Offsets alone are not
    /// enough: two blocks in one file — a second `test`, say — have the same
    /// instruction offsets for their first loop, so a lookup that ignored the
    /// frame would find the *other* block's loop, and its stack base would be
    /// truncated to on the way out.
    fn loop_entry(&mut self, frame: usize, site: LoopSite) -> usize {
        if let Some(index) = self.loops.iter().position(|entry| {
            entry.frame == frame && entry.top == site.top && entry.back_edge == site.back_edge
        }) {
            // The entry was made by the loop's own opening instruction, which runs
            // before the loop's `STORE` — and so before the loop has displaced
            // anything. Recording the shadow here, on the first turn, is what puts
            // back the binding that was there *before* the loop rather than the
            // one the loop itself wrote.
            if self.loops[index].variable.is_none() {
                let shadowed = self.shadow_of(site, frame);
                self.loops[index].variable = shadowed;
            }
            return index;
        }
        self.loops.push(Loop {
            kind: site.kind,
            iterator: None,
            top: site.top,
            back_edge: site.back_edge,
            exit: site.exit,
            iterations: 0,
            frame,
            stack_base: self.frames[frame].stack_base,
            variable: self.shadow_of(site, frame),
        });
        self.loops.len() - 1
    }

    /// What the name at `site`'s `STORE` is bound to right now, or `None` when the
    /// site is not an iterator's variable or the name is bound nowhere.
    ///
    /// Recorded once, when the loop entry is created, so every turn of the loop
    /// displaces the same binding — the one that was there before the loop, not
    /// the one a previous turn wrote.
    fn shadow_of(&self, site: LoopSite, frame: usize) -> Option<LoopVariable> {
        if !site.iterator {
            return None;
        }
        let instruction = self.instruction_at(frame, site.top as usize).ok()??;
        let name = self.constant_text(instruction.arg).ok()?;
        let shadowed = match self
            .locals
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, scope)| scope.get(&name).map(|value| (index, value.clone())))
        {
            Some((index, value)) => Shadowed::Local(index, value),
            None => match self.globals.get(&name) {
                Some(value) => Shadowed::Global(value.clone()),
                None => Shadowed::Unbound,
            },
        };
        Some(LoopVariable { name, shadowed })
    }

    /// Leaves the loop at `index`: its sequence is released and execution
    /// continues after it.
    fn leave_loop(&mut self, frame: usize, index: usize) {
        let loop_entry = self.loops.remove(index);
        let exit = loop_entry.exit;
        let base = loop_entry.stack_base;
        self.stack.truncate(base);
        if let Some(variable) = loop_entry.variable {
            self.restore_shadowed(variable);
        }
        self.set_ip(frame, exit as usize);
    }

    /// Puts back the binding a loop's variable displaced, as the tree-walking VM
    /// does when it pops the scope each turn of the loop ran in.
    fn restore_shadowed(&mut self, variable: LoopVariable) {
        let LoopVariable { name, shadowed } = variable;
        match shadowed {
            Shadowed::Unbound => {
                for scope in self.locals.iter_mut().rev() {
                    if scope.shift_remove(&name).is_some() {
                        return;
                    }
                }
                self.globals.remove(&name);
            }
            Shadowed::Local(index, value) => {
                if let Some(scope) = self.locals.get_mut(index) {
                    scope.insert(name, value);
                }
            }
            Shadowed::Global(value) => {
                self.globals.insert(name, value);
            }
        }
    }

    /// `BREAK`: no effect.
    ///
    /// The tree-walking VM parses `break` and does nothing with it — see the
    /// `Statement::Break` arm in `crate::vm` and the two tests in
    /// `tests/test_lists.rb` that pin the iteration count rather than the
    /// break. This VM has the loop-exit machinery that a real `break` would
    /// need, and it is not wired up here, because the two VMs disagreeing about
    /// what a program means is worse than a language feature being unfinished.
    /// `FINDINGS.md` carries the gap.
    fn break_loop(&mut self, frame: usize) -> Result<()> {
        self.advance(frame);
        Ok(())
    }

    /// `SKIP`: no effect, for the same reason as [`Self::break_loop`].
    fn skip_loop(&mut self, frame: usize) -> Result<()> {
        self.advance(frame);
        Ok(())
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
        // `return` is an ordinary statement in Redblue, not an escape: the
        // tree-walking VM evaluates it and the block carries on to the next
        // statement, and only the block's *last* statement is its value. So the
        // value survives only when this return is the block's final
        // instruction, which is exactly when the tree-walker would return it.
        //
        // The operand stack is cut back to where the frame started either way,
        // because a `return` in the middle of a block leaves nothing behind:
        // the tree-walker has no stack to leave a stray value on.
        let last = self.frames[frame].ip + 1 >= self.code_len(frame);
        self.stack.truncate(self.frames[frame].stack_base);
        if last {
            self.stack.push(value);
            self.frames[frame].ip = usize::MAX;
        } else {
            self.advance(frame);
        }
        Ok(())
    }

    // -- calls --------------------------------------------------------------

    /// `CALL`: a builtin, or a function the program declared.
    fn call(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let name = self.constant_text(instruction.arg)?;
        let args = self.pop_n(instruction.aux)?;
        self.call_named(&name, &args, frame)
    }

    /// Calls `name`, which is either a builtin or a declared function.
    fn call_named(&mut self, name: &str, args: &[Value], frame: usize) -> Result<()> {
        if let Some(value) = runtime::builtin(self.span(), name, args)? {
            self.push(value);
            self.advance(frame);
            return Ok(());
        }
        match self.get_var(name) {
            Some(Value::Function(function)) => {
                self.call_function(&function, args, None)?;
                self.advance(frame);
                Ok(())
            }
            // A module function: `text.uppercase` is the builtin `uppercase`
            // reached through the module the documents spell it with.
            Some(Value::Builtin(_)) => {
                let Some(result) = stdlib::call_module_function(self.span(), name, args) else {
                    return Err(Error::Runtime(
                        format!("Unknown function '{}'", name),
                        self.span(),
                    ));
                };
                self.push(result?);
                self.advance(frame);
                Ok(())
            }
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

        let Some((object_name, method)) = split_dotted(&dotted) else {
            // A receiver that was an expression rather than a name pushed its
            // value, and the file says so by carrying the bare method name.
            let receiver = self.pop()?;
            return Err(Error::Runtime(
                format!(
                    "Cannot call method '{}' on {}, which is not an object",
                    dotted, receiver
                ),
                self.span(),
            ));
        };
        // A receiver that was a name pushed nothing: the name is the whole of
        // what a call resolves against. An object method still needs a `this`,
        // which is the *binding* the name holds rather than the type's declared
        // fields — `set Greeter.name to "Ada"` writes that binding, and the
        // method has to see it, which is why the tree-walking VM evaluates its
        // receiver as a variable.
        let receiver = if self.objects.contains_key(object_name) {
            Some(self.get_var(object_name).unwrap_or(Value::Nothing))
        } else {
            None
        };

        let Some(object) = self.objects.get(object_name).cloned() else {
            // A module function, which a builtin namespace member is too. An
            // alias is resolved to the module it names first, so `import json
            // as J` then `J.stringify(..)` reaches the same function
            // `json.stringify(..)` does — the tree-walking VM resolves the same
            // way; see [`crate::vm::Vm::call_method`].
            let module = self
                .module_aliases
                .get(object_name)
                .cloned()
                .unwrap_or_else(|| object_name.to_string());
            let qualified = qualified_member(&module, method);
            if self.get_var(&qualified).is_none() && self.is_module_name(&module) {
                return Err(Error::Runtime(
                    format!("Module '{module}' has no function '{method}'"),
                    self.span(),
                ));
            }
            return self.call_named(&qualified, &args, frame);
        };
        let Some(Value::Function(function)) = object.methods.get(method).cloned() else {
            return Err(Error::Runtime(
                format!("Object '{object_name}' has no method '{method}'"),
                self.span(),
            ));
        };
        // `this` is bound below the callee's captured scopes and its parameters,
        // so a parameter named `this` shadows the receiver the way it shadows a
        // captured binding in the tree-walking VM.
        self.call_function(&function, &args, receiver)?;
        self.advance(frame);
        Ok(())
    }

    /// Runs a closure's block, binding `args` to its parameters.
    fn call_function(
        &mut self,
        function: &FunctionValue,
        args: &[Value],
        this: Option<Value>,
    ) -> Result<()> {
        let Some((chunk, path)) = function.body.block() else {
            return Err(Error::Runtime(
                format!(
                    "'{}' has no compiled body, which only `rb vm` can run",
                    function.name
                ),
                self.span(),
            ));
        };
        if self.call_depth >= self.max_call_depth {
            return Err(Error::Runtime(
                format!(
                    "Maximum call depth of {} reached while calling '{}'",
                    self.max_call_depth, function.name
                ),
                self.span(),
            ));
        }
        let (chunk, path) = (chunk.clone(), path.to_vec());
        let block = self.block_of(&chunk, &path)?.clone();
        // The captured scopes go on the stack *above* the caller's frames, and
        // the parameters above those, so the body reads its own environment
        // rather than a same-named binding of whoever called it, and a parameter
        // shadows what it captured.
        // Taken before `this` is pushed, so the binding lives exactly as long as
        // the frame and is gone when the call returns.
        let locals_base = self.locals.len();
        if let Some(receiver) = this {
            let mut scope = CapturedScope::new();
            scope.insert("this".to_string(), receiver);
            self.locals.push(scope);
        }
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
        let mut new_frame = self.frame_for(&chunk, path.to_vec());
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

    /// `DEF_FUNCTION`: the closure whose body is `arg`'s block. The `STORE` the
    /// file writes next binds it.
    fn def_function(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let (chunk, path) = self.child_path(frame, instruction.arg)?;
        let block = self.block_of(&chunk, &path)?.clone();
        let closure = self.make_function(&chunk, &path, &block);
        self.push(closure);
        self.advance(frame);
        Ok(())
    }

    /// `DEF_METHOD`: adds a method to the object being declared, or binds it as
    /// a value when it appears outside one — which is what the tree-walking VM
    /// does with a top-level `to can`.
    fn def_method(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let (chunk, path) = self.child_path(frame, instruction.arg)?;
        let block = self.block_of(&chunk, &path)?.clone();
        let method = self.make_function(&chunk, &path, &block);
        let name = block.name.clone();
        if let Some(pending) = self.pending_object.as_mut() {
            pending.methods.insert(name, method);
        } else {
            self.set_var(&name, method);
        }
        self.advance(frame);
        Ok(())
    }

    /// `DEF_OBJECT`: starts an object body, whose instructions declare the fields
    /// and methods and then register the type.
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
        self.pending_object = Some(ObjectPending {
            name,
            parent,
            fields: Fields::new(),
            methods: Fields::new(),
        });
        self.frames.push(new_frame);
        self.advance(frame);
        Ok(())
    }

    /// The name the object instruction declares: the one the `STORE` right after
    /// it binds.
    fn object_name(&self, frame: usize, instruction: Instruction) -> Result<String> {
        let next = self.frames[frame].ip + 1;
        if next < self.code_len(frame) {
            let chunk = self.frames[frame].chunk.clone();
            let path = self.frames[frame].path.clone();
            let block = self.block_of(&chunk, &path)?;
            let following = &block.code[next];
            if following.opcode == Opcode::Store {
                return self.constant_text(following.arg);
            }
        }
        self.child_block_name(frame, instruction.arg)
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
            catch: catch.clone(),
            catch_var: catch
                .as_ref()
                .map(|(_, var)| var.clone())
                .unwrap_or_default(),
            finally,
            stack_base: self.frames[frame].stack_base,
            loop_base: self.loops.len(),
        });
        self.advance(frame);
        Ok(())
    }

    /// Runs the handlers of the innermost installed `try`, if the failure is one
    /// of them to handle.
    ///
    /// Answers whether the failure was handled. A handler that fails itself leaves
    /// that failure in place, which is what the tree-walking VM does — the second
    /// `?` — so an error in a `catch` is not swallowed by the `try` that caught
    /// it.
    fn handle_failure(&mut self, _error: &Error) -> Result<bool> {
        // The handler is the innermost frame's only when the failure happened in
        // the frame that installed it. A failure inside a call made by the
        // protected code is caught by the caller's `try` instead, which is what
        // the tree-walking VM does when `?` propagates out of a call — so the
        // frames in between are dropped first, and the call depth they charged
        // is given back so a caught failure leaves the counter balanced.
        let frame = match (0..self.frames.len())
            .rev()
            .find(|index| self.handlers.len() > self.frames[*index].handler_base)
        {
            Some(frame) => frame,
            None => return Ok(false),
        };
        self.discard_frames_above(frame);

        let handler = self.handlers.pop().expect("a handler");
        // Truncated to where the protected code started rather than to the frame
        // that raised the failure: a loop the failing statement opened is half
        // drawn and cannot be resumed, but a loop the `try` is *inside* is still
        // the one that turns next.
        self.loops.truncate(handler.loop_base);
        self.stack.truncate(handler.stack_base);
        self.run_catch(&handler)?;
        self.run_finally(&handler)?;

        // The protected region is the instructions between `TRY` and the marked
        // `NOP` that closes it, and both handlers have run above, so execution
        // carries on *after* that `NOP` rather than at it. Resuming at it would
        // be wrong twice over: the instruction pops whatever handler is on top,
        // which for an inner `try` is the *enclosing* one — its `finally` would
        // run before the rest of its protected code and it would lose its
        // protection — and the `finally` owed here has already run. A `try` with
        // no marker to go to — a file the compiler did not write — ends the
        // block instead of resuming into whatever follows.
        if self.frames.is_empty() {
            return Ok(true);
        }
        match self.end_try_after(frame) {
            Some(target) => self.set_ip(frame, target + 1),
            None => self.set_ip(frame, self.code_len(frame)),
        }
        Ok(true)
    }

    /// Drops every frame above `keep`, releasing the call depth they charged.
    ///
    /// Nothing is left on the operand stack for them: the failure is being
    /// handled, so the partial values a frame had built are gone rather than
    /// handed to whoever catches it — the tree-walking VM drops them with the
    /// frames when `?` leaves a call.
    fn discard_frames_above(&mut self, keep: usize) {
        while self.frames.len() > keep + 1 {
            let Some(frame) = self.frames.pop() else {
                break;
            };
            if let Some(name) = frame.module_body.as_deref() {
                // A module body that failed publishes nothing, so the module is
                // not declared either: leaving it registered would answer a later
                // `module` of the same name with `is already declared` and a
                // later `import` of it with a module that published nothing. The
                // tree-walking VM removes it for the same reason.
                self.declared_modules.remove(name);
                self.module_depth = self.module_depth.saturating_sub(1);
                self.importing.pop();
            }
            self.loops.truncate(frame.loop_base);
            self.handlers.truncate(frame.handler_base);
            self.locals.truncate(frame.locals_base);
            if frame.is_call {
                self.call_depth = self.call_depth.saturating_sub(1);
            }
        }
    }

    /// The `catch` body of `handler`, if it has one.
    ///
    /// A `catch` binds the failure's message to its name; the tree-walking VM
    /// binds the word `error`, so this does too rather than inventing a message
    /// the language does not produce.
    fn run_catch(&mut self, handler: &Handler) -> Result<()> {
        let Some(((chunk, path), _)) = &handler.catch else {
            return Ok(());
        };
        self.locals.push(CapturedScope::new());
        if !handler.catch_var.is_empty() {
            self.locals
                .last_mut()
                .expect("a scope to bind into")
                .insert(handler.catch_var.clone(), Value::Text("error".to_string()));
        }
        let mut catch_frame = self.frame_for(chunk, path.clone());
        catch_frame.locals_base = self.locals.len() - 1;
        catch_frame.stack_base = handler.stack_base;
        self.frames.push(catch_frame);
        let outcome = self.drive(self.frames.len() - 1);
        self.locals.pop();
        outcome
    }

    /// The `finally` body of `handler`, if it has one.
    ///
    /// It runs whether or not the protected code failed: the success path runs
    /// it at the marked `NOP` that closes the region, and the failure path runs
    /// it here. Both reach the same code, by the same handler.
    fn run_finally(&mut self, handler: &Handler) -> Result<()> {
        let Some((chunk, path)) = &handler.finally else {
            return Ok(());
        };
        let locals_base = self.locals.len();
        let mut finally_frame = self.frame_for(chunk, path.clone());
        finally_frame.locals_base = locals_base;
        finally_frame.stack_base = handler.stack_base;
        self.frames.push(finally_frame);
        self.drive(self.frames.len() - 1)
    }

    /// The offset of the marked `NOP` that closes the `try` running in `frame`,
    /// or `None` when the block has none.
    ///
    /// Scanned forward from the current instruction, because a failure inside
    /// the protected code is in the middle of the region it has to skip. Only
    /// the marked `NOP` closes a region — a filler `NOP` inside protected code
    /// is not an end, so it cannot truncate the region — while a *nested* `try`
    /// ends at its own marked `NOP` first, which is what the nesting count is
    /// for: it steps over those and finds the end of this one.
    fn end_try_after(&self, frame: usize) -> Option<usize> {
        let code = block_at(&self.frames[frame].chunk, &self.frames[frame].path)?;
        let mut depth = 0usize;
        for (offset, instruction) in code.code.iter().enumerate().skip(self.frames[frame].ip) {
            match (instruction.opcode, instruction.arg) {
                (Opcode::Try, _) => depth += 1,
                (Opcode::Nop, END_TRY_MARKER) if depth == 0 => return Some(offset),
                (Opcode::Nop, END_TRY_MARKER) => depth -= 1,
                // A filler `NOP` closes nothing, so it is stepped over like any
                // other instruction.
                _ => {}
            }
        }
        None
    }

    // -- imports ------------------------------------------------------------

    /// `IMPORT`: loads a module's declarations into the globals, then the
    /// `STORE` the file writes next binds the name.
    ///
    /// A module already loaded is not loaded again — see
    /// [`BytecodeVm::modules`] — but the name is still bound, so a second
    /// `import` of the same module says the same thing to the program as the first
    /// did. A module the program *declared* has nothing to load: what it published
    /// was bound when the declaration ran, and is copied here onto the alias's own
    /// `alias_member` names.
    ///
    /// The declarations are compiled and run here rather than evaluated from the
    /// module's syntax tree, because this VM has no evaluator — but the selection
    /// is the tree-walking VM's, so the bindings that appear are the same ones.
    fn import(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let name = self.constant_text(instruction.arg)?;
        let alias = self.constant_text(instruction.aux)?;

        // A module reaching itself through the import chain is a cycle, and
        // saying so is the difference between a clean error and a load that
        // never ends. Scanned over the chain being loaded, which is as deep as
        // the chain of module files.
        if self.importing.iter().any(|loading| loading == &name) {
            return Err(Error::Runtime(
                format!("Circular import of module '{name}'"),
                self.span(),
            ));
        }

        // A builtin namespace has no file behind it, and refusing the import
        // because of that was the whole difference between this VM and the
        // tree-walking one for `import json`.
        let path = module_paths(&name)
            .into_iter()
            .find(|path| std::path::Path::new(path).exists());
        if path.is_none() && !stdlib::is_module(&name) && !self.declared_modules.contains_key(&name)
        {
            return Err(Error::Runtime(
                format!("Cannot find module '{name}'"),
                self.span(),
            ));
        }

        if let Some(path) = path {
            // Skipped when the module is already loaded: running it twice
            // declares its names twice, and a `constant` cannot be declared at
            // all a second time. The chain in `self.importing` is what refuses a
            // module reaching *itself*, so the re-entrance the set used to
            // prevent is already a cycle by the time it could happen.
            if !self.modules.contains(&name) {
                let (compiled, functions) = self.compile_module(&path)?;
                let chunk = Arc::new(compiled);
                // Read before the module runs, because the module's own `STORE`
                // is what overwrites them.
                let saved: Vec<(String, Option<Value>)> = functions
                    .iter()
                    .map(|member| (member.clone(), self.globals.get(member).cloned()))
                    .collect();
                self.importing.push(name.clone());
                let mut module_frame = self.frame_for(&chunk, Vec::new());
                module_frame.globals_only = true;
                module_frame.stack_base = self.stack.len();
                module_frame.locals_base = self.locals.len();
                self.frames.push(module_frame);
                let failure = self.drive(self.frames.len() - 1).err();
                self.importing.pop();
                if let Some(error) = failure {
                    // The module failed, so it is not loaded: leaving it in the
                    // set would answer a later import of the same name with a
                    // module that was never successfully bound.
                    self.modules.remove(&name);
                    return Err(error);
                }
                self.publish_module_functions(&name, saved);
                // Recorded only once the module has compiled *and* run, so a
                // module that failed either way is not in the set: a name left
                // there by a compile failure would answer a later import of the
                // same name with a module that was never bound, binding the alias
                // to nothing rather than reporting the failure again.
                self.modules.insert(name.clone());
            }
        }

        // A module this program declared has no file behind it, so what it
        // published is copied here onto the `alias_member` names a call through
        // the alias resolves to — the tree-walking VM's own copy.
        if let Some(declared) = self.declared_modules.get(&name) {
            let members = declared.members.clone();
            for (member, _, value) in members {
                self.globals
                    .insert(qualified_member(&alias, &member), value);
            }
        }

        // The alias is a name for the module, so a call written through it
        // reaches the same function the unaliased name does. The module's own
        // name names it too, which is what leaves `json.stringify` working after
        // `import json as J`.
        self.module_aliases.insert(alias.clone(), name.clone());
        self.module_aliases
            .entry(name.clone())
            .or_insert_with(|| name.clone());

        // The module's *own* name is a name of the program as well, so an
        // unbound one is bound to `nothing` — which is what makes `say MathUtils`
        // read a module rather than fail on an unknown variable. The tree-walking
        // VM binds it under the same guard.
        if self.get_var(&name).is_none() {
            self.globals.insert(name.clone(), Value::Nothing);
        }

        self.advance(frame);
        // `IMPORT` is followed by a `STORE` that binds the name the import
        // resolved, and an import does not take over a name the program already
        // holds: a name the program bound is the program's. So a name that is
        // already bound is left alone and the `STORE` is stepped over rather than
        // run — writing it back would work for a `set`, but a `constant` refuses
        // the write, so `constant M to 5` then `import MathUtils as M` would
        // fault here and not on the tree-walking VM. An unbound name is pushed as
        // `nothing`, which is what the `STORE` binds.
        let follows = matches!(
            self.instruction_at(frame, self.frames[frame].ip).ok().flatten(),
            Some(next) if next.opcode == Opcode::Store
        );
        if self.get_var(&alias).is_none() {
            self.push(Value::Nothing);
        } else if follows {
            self.advance(frame);
        }
        Ok(())
    }

    /// Whether `name` names a module this program can call into: a module file it
    /// imported, a module it declared, or a builtin namespace.
    fn is_module_name(&self, name: &str) -> bool {
        self.modules.contains(name)
            || self.declared_modules.contains_key(name)
            || stdlib::is_module(name)
    }

    /// `MODULE`: runs a `module NAME ... end` declaration's body block.
    ///
    /// The body runs in a scope and a frame of its own, so a `set` inside a
    /// module binds in the module's scope rather than becoming a global of the
    /// program that declared it — the boundary `module ... end` exists to draw.
    /// What the module publishes is bound back as `Name.member` when the body
    /// finishes; see [`BytecodeVm::finish_module`].
    ///
    /// Everything refused here is refused before the body runs, which is where
    /// the tree-walking VM refuses it: a second declaration of the same name, and
    /// an `export` of a name the declaration does not define.
    fn module(&mut self, instruction: Instruction, frame: usize) -> Result<()> {
        let (chunk, path) = self.child_path(frame, instruction.arg)?;
        // The block's own name is the module's, as a test block's name is the
        // test's — so a `MODULE` needs one operand, and a file that names a
        // block which is not there is reported rather than read.
        let name = self.block_of(&chunk, &path)?.name.clone();
        if self.declared_modules.contains_key(&name) {
            return Err(Error::Runtime(
                format!("Module '{name}' is already declared"),
                self.span(),
            ));
        }

        let exports = self.module_exports(&chunk, &path, &name)?;

        // Registered before the body runs, so an `import` of this module from
        // inside its own body is a circular import rather than a second
        // declaration or a miss.
        self.declared_modules
            .insert(name.clone(), DeclaredModule::default());
        self.importing.push(name.clone());

        self.locals.push(CapturedScope::new());
        self.module_depth += 1;
        let mut module_frame = self.frame_for(&chunk, path);
        module_frame.stack_base = self.stack.len();
        // The scope pushed above is the frame's, so leaving it is what taking the
        // module's scope away is — the same arrangement a `catch` body's bound
        // name uses.
        module_frame.locals_base -= 1;
        module_frame.module_body = Some(name);
        module_frame.module_exports = exports;
        self.frames.push(module_frame);
        self.advance(frame);
        Ok(())
    }

    /// The names the declaration whose body is `path` publishes, read from the
    /// run of `EXPORT`s the body block opens with.
    ///
    /// An `EXPORT` naming a name the declaration does not define carries the
    /// reserved `NO_CONST` in its second operand, and is refused here rather than
    /// published: the tree-walking VM reports the same fault, with the same
    /// wording, before the body runs.
    fn module_exports(&self, chunk: &Chunk, path: &[u32], name: &str) -> Result<Vec<String>> {
        let block = self.block_of(chunk, path)?;
        let mut exports = Vec::new();
        for instruction in &block.code {
            if instruction.opcode != Opcode::Export {
                break;
            }
            if instruction.aux == NO_CONST {
                let published = self.constant_of(chunk, instruction.arg)?;
                return Err(Error::Runtime(
                    format!("Module '{name}' exports '{published}', which it does not define"),
                    self.span(),
                ));
            }
            exports.push(self.constant_of(chunk, instruction.arg)?);
        }
        Ok(exports)
    }

    /// The text constant at `index` of `chunk`, which a caller already holds the
    /// chunk for.
    fn constant_of(&self, chunk: &Chunk, index: u32) -> Result<String> {
        match chunk.constants.get(index as usize) {
            Some(Constant::Text(text)) => Ok(text.clone()),
            Some(other) => Err(Error::Runtime(
                format!("bytecode expects a name at constant {index}, found {other}"),
                self.span(),
            )),
            None => Err(Error::Runtime(
                format!(
                    "bytecode names constant {index}, and the pool holds {}",
                    chunk.constants.len()
                ),
                self.span(),
            )),
        }
    }

    /// Publishes what a finished module body bound.
    ///
    /// Read while the module's own scope is still live, so a member that is not
    /// bound is not published — which is what stops a module offering a function
    /// whose declaration failed. The tree-walking VM reads the same scope at the
    /// same point; see [`crate::vm::Vm::declare_module`].
    fn finish_module(&mut self, frame: &Frame) {
        let Some(name) = frame.module_body.clone() else {
            return;
        };
        let mut members = Vec::new();
        for member in &frame.module_exports {
            let Some(value) = self.declare_member(member) else {
                continue;
            };
            let qualified = qualified_member(&name, member);
            self.globals.insert(qualified.clone(), value.clone());
            members.push((member.clone(), qualified, value));
        }
        self.declared_modules
            .insert(name.clone(), DeclaredModule { members });
        self.module_aliases
            .entry(name.clone())
            .or_insert_with(|| name.clone());
    }

    /// The value a name a module body declares is bound to, or `None` when it
    /// declares nothing of that name — an `export` of a name the module does not
    /// define is what notices the absence, not this.
    fn declare_member(&mut self, name: &str) -> Option<Value> {
        let index = self
            .locals
            .iter()
            .rposition(|scope| scope.contains_key(name))?;
        self.locals[index].get(name).cloned()
    }

    /// Moves a module file's functions onto the `module_function` names a call
    /// through the module resolves to.
    ///
    /// A module declares `to circle_area`, and a `globals_only` frame binds that
    /// under the bare name `circle_area` — a name the importing program would
    /// then read as its own. The tree-walking VM publishes those under
    /// `MathUtils_circle_area` from inside a scope of the module's own; here the
    /// same move is made afterwards, and whatever the program held under the bare
    /// name is put back, because an import does not take over a name the program
    /// already holds.
    ///
    /// Only functions are moved. A module's `set` and `constant` are bound under
    /// their own names on purpose: that is how an importing program reads a
    /// module's `PI`.
    ///
    /// `saved` is each member's value in the importing program from before the
    /// module ran, taken by the caller because this is the point where what the
    /// module bound is still readable.
    fn publish_module_functions(&mut self, name: &str, saved: Vec<(String, Option<Value>)>) {
        for (member, previous) in saved {
            let Some(value) = self.globals.remove(&member) else {
                continue;
            };
            self.globals.insert(qualified_member(name, &member), value);
            if let Some(previous) = previous {
                self.globals.insert(member, previous);
            }
        }
    }

    /// Compiles a module down to the declarations its top level holds, and the
    /// names of the functions among them.
    ///
    /// The `set` and `constant` statements are compiled because both are bindings
    /// the importing program reads: a module that declares `constant PI` and one
    /// that declares `set PI` bind the same name, and dropping the first leaves
    /// the name resolving to whatever else was already bound — for `PI` that is
    /// the builtin's, so `import` followed by `say PI` printed the builtin's
    /// value on this VM and the module's on the tree-walking one.
    ///
    /// The `to` functions are compiled for the same reason: a call written
    /// `MathUtils.circle_area(5)` resolves to `MathUtils_circle_area`, which
    /// exists on the tree-walking VM and did not here.
    fn compile_module(&self, path: &str) -> Result<(Chunk, Vec<String>)> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| Error::Io(format!("Cannot load module '{}': {}", path, e)))?;
        let tokens = Lexer::tokenize(&source)?;
        let ast = parser::parse(tokens)?;
        let body = parser::module_body(&ast);
        let functions: Vec<String> = body
            .iter()
            .filter_map(|stmt| match &stmt.statement {
                Statement::Function { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        let program = Program {
            statements: body
                .iter()
                .filter(|stmt| {
                    matches!(
                        stmt.statement,
                        Statement::Set { .. }
                            | Statement::Constant { .. }
                            | Statement::Function { .. }
                    )
                })
                .cloned()
                .collect(),
        };
        Ok((crate::bytecode::compile_program(&program)?, functions))
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
        max_iterations: crate::vm::resolve_max_iterations(),
        max_call_depth: resolve_max_call_depth(),
    }
}

impl Default for Limits {
    fn default() -> Self {
        limits()
    }
}

/// Runs `chunk` and returns what `main` produced.
pub fn run(chunk: &Chunk) -> Result<Value> {
    BytecodeVm::new().run(chunk)
}

/// The limits, re-exported from the tree-walking VM so a caller does not have to
/// know which VM declares them.
pub const DEFAULT_MAX_CALL_DEPTH: usize = MAX_CALL_DEPTH;
/// See [`DEFAULT_MAX_CALL_DEPTH`].
pub const DEFAULT_MAX_ITERATIONS: usize = MAX_ITERATIONS;
/// See [`DEFAULT_MAX_CALL_DEPTH`].
pub const DEFAULT_MAX_STEPS: usize = MAX_STEPS;
