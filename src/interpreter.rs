//! The tree-walking interpreter: what `rb run` executes.
//!
//! It is the specification of Redblue's behaviour. The bytecode VM in
//! [`crate::bytecode::vm`] has to answer the same way, so the operations both
//! need — arithmetic, indexing, the builtin library — live in
//! [`crate::runtime`] and are read from here rather than written twice.

use crate::debugger::{DebugEvent, DebugHook};
use crate::error::{Error, Result, Span};
use crate::failure::{value_of as failure_value, FailureKind};
use crate::lexer::Lexer;
use crate::parser as redblue_parser;
use crate::parser::{BinaryOp, Expr, Program, Statement, Stmt, UnaryOp};
use crate::runtime;
use crate::stdlib;
use crate::value::{
    expect_range_number, expect_repeat_count, finite_number, range_has_next, Captured,
    CapturedScope, Fields, FunctionBody, FunctionValue, Value,
};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

// The HTTP timeouts belong to the builtin library, which is `runtime`'s; they
// are part of this module's public surface because they were declared here
// before the move.
pub use crate::runtime::{NETWORK_CONNECT_TIMEOUT_SECS, NETWORK_TIMEOUT_SECS};

// The call-depth limit and the stack one call frame is allowed are declared in
// `build.rs`, which writes them into `$OUT_DIR/stack_budget.rs` for the
// `include!` below.
//
// They live there and not here because they are the same number twice: the
// interpreter counts calls against `MAX_CALL_DEPTH`, and the linker sizes the
// WebAssembly shadow stack at `MAX_CALL_DEPTH * STACK_BYTES_PER_CALL` so that
// counter is physically reachable there. Two copies could drift, and drift
// between them would make the playground trap on a program the native binary
// reports a catchable `Limit` for.
include!(concat!(env!("OUT_DIR"), "/stack_budget.rs"));

/// The environment variable that overrides [`MAX_CALL_DEPTH`].
pub const MAX_CALL_DEPTH_ENV: &str = "REDBLUE_MAX_CALL_DEPTH";

/// The default number of iterations any single loop may run. A `while` whose
/// condition never becomes false, or a `repeat` with a huge count, stops with a
/// `RuntimeError` naming this limit instead of running until the host kills the
/// process.
pub const MAX_ITERATIONS: usize = 1_000_000;

/// The environment variable that overrides [`MAX_ITERATIONS`].
pub const MAX_ITERATIONS_ENV: &str = "REDBLUE_MAX_ITERATIONS";

/// The default number of statements the VM may execute for one program.
///
/// [`MAX_ITERATIONS`] bounds one loop; this bounds the program, because
/// unboundedness can also be written as many short loops rather than one long
/// one. It is what a test harness lowers to bound a run deterministically.
///
/// It was 10 000 000, and that is below what the project needs its own compiler
/// to be allowed to do. Ladder stage S3 — `bootstrap/compiler.rb` compiled by
/// itself, run as bytecode — takes **16 062 500** steps on this repository's
/// `bootstrap/compiler.rb` (measured by bisecting `REDBLUE_MAX_STEPS` until the
/// run stopped being refused), and `rb vm stage1.rbc bootstrap/compiler.rb
/// stage2.rbc` is the command the ladder is named for. At 10 000 000 it died
/// with `Step budget of 10000000 reached before the program finished` after
/// doing most of the work, which is the worst of both: the guard spent its time
/// and the program produced nothing.
///
/// 32 000 000 is about twice what the self-compilation needs, so the compiler can
/// grow before the budget is the reason it stops, and it is still a bound: a
/// program that runs away is stopped in seconds of release-build time rather
/// than never, and `REDBLUE_MAX_STEPS` still lowers it for anyone who wants a
/// tighter one. Nothing else about the guard moved — [`MAX_ITERATIONS`] still
/// caps a single loop at a million iterations, which is what catches the ordinary
/// runaway loop before this budget does.
///
/// `edge_the_self_hosted_compiler_compiles_itself_byte_identically` is what pins
/// this: it runs the self-compilation with no environment override at all, so
/// the default either admits the compiler or the fixed point is not a fixed
/// point.
pub const MAX_STEPS: usize = 32_000_000;

/// The environment variable that overrides [`MAX_STEPS`].
pub const MAX_STEPS_ENV: &str = "REDBLUE_MAX_STEPS";

/// The policy for reading a limit: a limit of zero would make every loop
/// illegal, which is never what an operator means, so `None` and `Some(0)` both
/// fall back to the default. Split from the `std::env::var` call so a test can
/// exercise the policy without mutating the process environment, which is
/// shared with every test running in parallel.
fn resolve_limit_from(raw: Option<usize>, default: usize) -> usize {
    match raw {
        Some(limit) if limit > 0 => limit,
        _ => default,
    }
}

/// [`resolve_limit_from`] applied to [`MAX_ITERATIONS_ENV`]. `None` means the
/// variable was unset or not a number.
pub fn resolve_max_iterations_from(raw: Option<usize>) -> usize {
    resolve_limit_from(raw, MAX_ITERATIONS)
}

/// [`resolve_limit_from`] applied to [`MAX_STEPS_ENV`]. `None` means the
/// variable was unset or not a number.
pub fn resolve_max_steps_from(raw: Option<usize>) -> usize {
    resolve_limit_from(raw, MAX_STEPS)
}

/// The configured per-loop iteration cap.
pub fn resolve_max_iterations() -> usize {
    resolve_max_iterations_from(
        std::env::var(MAX_ITERATIONS_ENV)
            .ok()
            .and_then(|raw| raw.trim().parse::<usize>().ok()),
    )
}

/// The configured step budget for one program.
pub fn resolve_max_steps() -> usize {
    resolve_max_steps_from(
        std::env::var(MAX_STEPS_ENV)
            .ok()
            .and_then(|raw| raw.trim().parse::<usize>().ok()),
    )
}

/// The name a function literal carries.
///
/// A literal has no name to carry, so diagnostics about one — a call-depth
/// limit reached inside it, a body that could not be run — need a word that
/// says the function has none rather than printing an empty quote.
pub(crate) const ANONYMOUS_FUNCTION: &str = "anonymous function";

/// Reads the call-depth limit from [`MAX_CALL_DEPTH_ENV`], falling back to
/// [`MAX_CALL_DEPTH`] for an absent, non-numeric or zero value — a limit of zero
/// would make every function call illegal, which is never what an operator
/// means.
pub fn resolve_max_call_depth() -> usize {
    match std::env::var(MAX_CALL_DEPTH_ENV) {
        Ok(raw) => match raw.trim().parse::<usize>() {
            Ok(limit) if limit > 0 => limit,
            _ => MAX_CALL_DEPTH,
        },
        Err(_) => MAX_CALL_DEPTH,
    }
}

/// Runs `program` on a thread whose stack is sized for the configured call
/// depth, and returns the VM it ran in together with the program's result.
///
/// The VM is returned rather than dropped so a caller such as the test harness
/// can still read [`Vm::take_expectation_failure`], which is set on the VM that
/// ran the assertions.
pub fn run_isolated(program: &Program) -> (Vm, Result<Value>) {
    run_isolated_in(program, std::path::Path::new("."))
}

/// [`run_isolated`] with the directory the program was found beside, which is
/// where its `redblue.manifest` is and what its module paths are read against.
///
/// `run_isolated` is this against the working directory, which is what a program
/// run as itself wants. A root of its own is what makes `rb run subdir/prog.rb`
/// resolve the manifest *that program* declares rather than the one belonging to
/// whatever directory the host was in.
pub fn run_isolated_in(program: &Program, root: &std::path::Path) -> (Vm, Result<Value>) {
    run_isolated_with_depth_in(program, resolve_max_call_depth(), root)
}

/// [`run_isolated_with_depth`] against `root` — see [`run_isolated_in`].
pub fn run_isolated_with_depth(program: &Program, depth: usize) -> (Vm, Result<Value>) {
    run_isolated_with_depth_in(program, depth, std::path::Path::new("."))
}

/// [`run_isolated_with_depth`] against `root` — see [`run_isolated_in`].
///
/// Split out for the same reason [`run_isolated_with_depth`] is: a test has to
/// be able to ask for a stack this process cannot reserve — the case
/// [`run_without_a_sized_thread`] decides — and the only way to reach it without
/// mutating the process environment is to hand the number in. It reads a depth,
/// not bytes: the size asked for is `depth * STACK_BYTES_PER_CALL`, so a depth
/// past what any thread can carry asks for the impossible and the refusal is the
/// answer under test.
pub fn run_isolated_with_depth_in(
    program: &Program,
    depth: usize,
    root: &std::path::Path,
) -> (Vm, Result<Value>) {
    let limit = depth.max(1);
    let threaded = program.clone();
    let root = root.to_path_buf();
    let builder = std::thread::Builder::new()
        .name("redblue-vm".to_string())
        .stack_size(limit.saturating_mul(STACK_BYTES_PER_CALL));

    let joined = match builder.spawn({
        let root = root.clone();
        move || {
            let mut vm = Vm::new_in(&root);
            let result = vm.run(&threaded);
            (vm, result)
        }
    }) {
        Ok(handle) => handle.join(),
        Err(_) => return run_without_a_sized_thread(program, limit, root),
    };

    match joined {
        Ok(result) => result,
        Err(_) => (
            Vm::new(),
            Err(Error::Runtime(
                "The interpreter thread stopped unexpectedly".to_string(),
                Span::unknown(),
            )),
        ),
    }
}

/// What to do when the thread [`run_isolated`] needs cannot be started.
///
/// The thread buys exactly one thing: stack room for the configured call depth.
/// `MAX_CALL_DEPTH` frames of `STACK_BYTES_PER_CALL` is what the depth counter
/// assumes is physically reachable, and a stack that is not that big turns the
/// catchable `Limit` the counter raises back into a native stack overflow — an
/// abort, not a failure a host can catch and not a page that can show anything.
///
/// So there is no fallback here that runs the program on a stack smaller than
/// the counter's assumption. There is one place where running inline is not
/// running unprotected: `wasm32-unknown-unknown` has no threads, `spawn` always
/// fails there, and `build.rs` links the module's single shadow stack from the
/// very same `limit * STACK_BYTES_PER_CALL` product. On that target the inline
/// run is on a stack sized for the whole limit, which is why the two builds can
/// be trusted to stop at the same depth — and it is scoped to that target
/// rather than to "the spawn failed", because on any other target a failed
/// spawn means the stack could not be had and the run has to be refused.
fn run_without_a_sized_thread(
    program: &Program,
    limit: usize,
    root: std::path::PathBuf,
) -> (Vm, Result<Value>) {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = limit;
        let mut vm = Vm::new_in(&root);
        let result = vm.run(program);
        (vm, result)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = (program, root);
        (
            Vm::new(),
            Err(Error::Limit(
                format!(
                    "Cannot start the interpreter thread: a stack for {limit} calls could not be \
                     reserved, and running on a smaller one would stop the process rather than \
                     the program"
                ),
                Span::unknown(),
            )),
        )
    }
}

/// The language context a program has built: the module files it has loaded,
/// the aliases it named them by, the modules it declared, the members they
/// published, and the objects it declared.
///
/// A dotted call resolves against this and against nothing else.
/// `MathUtils.circle_area(5)` is a member of a module the program declared,
/// `J.parse(..)` is a member of a module imported under an alias, and
/// `counter.bump()` is a method of an object the program declared. An
/// expression evaluated *for* a program — the debug sub-REPL's `.watch` — has
/// to be given this as well as the program's bindings, or every one of those
/// three is a "no such function" it cannot answer.
///
/// It is one value rather than five because it is one thing: what a program
/// has *declared*, as opposed to what it has bound. The bindings are copied
/// afresh at every step; this changes only when the program declares
/// something, and a copy of it is cheap — the module programs are shared
/// rather than cloned.
#[derive(Clone, Default)]
pub struct CallContext {
    /// The module programs already run, by the name the `import` that loaded
    /// them goes by. An entry is what makes a second import of the same module a
    /// no-op rather than a second binding of the same names.
    ///
    /// The programs are shared rather than copied: nothing reads one back, and
    /// a copy of the table would be a copy of every module's syntax tree.
    pub(crate) modules: HashMap<String, Arc<Program>>,
    /// Every module name an `import` has bound, to the module it names.
    ///
    /// An alias — `import json as J` — binds `J`, and `J.parse(...)` has to
    /// reach the same function `json.parse(...)` does. That is what this table
    /// is for: a call through a name it holds is a call through the module it
    /// points at, so the alias is a name for the module rather than a second
    /// copy of it.
    pub(crate) module_aliases: HashMap<String, String>,
    /// Every `module` declaration that has run, by module name.
    ///
    /// An entry is what a second declaration of the same name is refused
    /// against, and what an `import` of the name finds in place of a file.
    pub(crate) declared_modules: HashMap<String, DeclaredModule>,
    /// The `module_member` names this VM has published for a module's own
    /// members, which is where a call `MathUtils.circle_area(5)` resolves to.
    ///
    /// They are recorded rather than inferred from the name, because
    /// `Module_member` is a naming convention and a program may bind a name with
    /// an underscore in it: guessing would drop a name the program wrote.
    /// [`Vm::user_names`] uses this to leave them out — they are the interior of
    /// `MathUtils.circle_area`, so offering `MathUtils_circle_area` to
    /// completion, `:vars`, `:funcs` and the analyzer's seed shows the program a
    /// name it cannot write.
    pub(crate) qualified_members: HashSet<String>,
    /// Every `object` declaration, by type name. The table is the inheritance
    /// model: an entry already holds its own fields and methods merged with
    /// everything it inherits, nearest declaration first, so a lookup is a
    /// single map read and cannot walk a cyclic parent chain.
    pub(crate) objects: HashMap<String, ObjectType>,
}

pub struct Vm {
    globals: HashMap<String, Value>,
    /// The names bound by a `constant` declaration. The value lives in
    /// [`Vm::globals`] so a read resolves through one name resolution path; what
    /// this table adds is the refusal to rebind the name.
    constants: HashSet<String>,
    /// The live local scopes, outermost first. A call's parameters are the
    /// innermost scope, and a function value that closed over scopes pushes
    /// them above its caller's frames — see [`Vm::call_user_function`].
    locals: Vec<CapturedScope>,
    /// The lines `say` has produced, printed when the program finishes — see
    /// [`Vm::run`]. Bounded by [`Vm::output_bytes`] and [`Vm::output_full`];
    /// see [`Vm::charge_output`].
    output: Vec<String>,
    /// How many bytes [`Vm::output`] holds, the newline [`Vm::run`] will append
    /// to each line included.
    ///
    /// The lines are *buffered*, so charging at [`crate::wasm::emit`] bounds
    /// the capture and not this: a program whose `say` never reached the
    /// boundary — because it failed before the end, or because it never
    /// finished — would still have grown a `Vec<String>` without limit while it
    /// did. This is the counter that stops that, and it is why a `repeat` of
    /// [`MAX_ITERATIONS`] long lines fails instead of exhausting memory before
    /// any output is ever written.
    output_bytes: usize,
    /// Whether [`Vm::charge_output`] has already stopped a `say` in this run.
    ///
    /// Once the bound has stopped one line it stops every later one. The
    /// alternative — letting a shorter line through into whatever room the last
    /// long line happened to leave — would make what a program can still say
    /// depend on how its earlier lines divided the limit, which is not a
    /// property anyone can rely on or write a test against.
    output_full: bool,
    /// How many `break` statements have been evaluated, including one that was
    /// then refused for being outside every loop. See
    /// [`Vm::loop_control_counts`].
    break_statements: usize,
    /// How many `skip` statements have been evaluated, on the same terms as
    /// [`Vm::break_statements`].
    skip_statements: usize,
    /// What the program has *declared*: the modules it has imported, the
    /// aliases it named them by, the modules it declared, the members they
    /// published and the objects it declared — see [`CallContext`].
    ///
    /// One value rather than five fields, because a caller that evaluates an
    /// expression for this program needs all five of them together: a watch
    /// that cannot see the module a program imported cannot call it.
    context: CallContext,
    /// The directory this program's imports are resolved against: where its
    /// `redblue.manifest` is, and what every path an `import` returns is read
    /// against.
    ///
    /// Data rather than process state. A program at `subdir/prog.rb` resolves
    /// against `subdir/redblue.manifest` and opens `subdir/modules/Foo.rb`,
    /// whichever directory the host happens to be in — so `rb run subdir/prog.rb`
    /// and `analyze_in(&program, "subdir")` and `BytecodeVm::new_in("subdir")`
    /// are all answering about one program, rather than the first two consulting
    /// the program's own manifest and the last consulting `./`.
    root: std::path::PathBuf,
    /// The pins of that manifest, resolved once for this run and answered to
    /// every `import` the program has.
    ///
    /// Not a global cache: it is held by the thing running *this* program, so a
    /// second program in the same process — a REPL line, a harness running files
    /// one after another — is answered from its own manifest.
    pins: crate::manifest::Pins,
    /// The modules whose bodies are running, innermost last. A module that
    /// reaches itself through an `import` is a circular import, and this is
    /// what says so instead of walking the cycle.
    importing: Vec<String>,
    /// How many `module` bodies are running. Above zero a `set` writes into the
    /// module's own scope instead of becoming a global, which is what makes a
    /// module declaration a boundary rather than a sequence of top-level
    /// statements.
    module_depth: usize,
    /// The names every VM is born holding: the builtins
    /// [`stdlib::builtins`] provides, by name.
    ///
    /// A copy of the keys rather than a call to [`stdlib::builtins`] per
    /// question, because the debugger asks "which globals did the program
    /// write" at *every statement it is shown* and a frame listing two hundred
    /// builtins answers nothing.
    builtin_names: HashSet<String>,
    /// Whether this VM is answering a *question* about a value rather than
    /// running a program, which changes two things about how it evaluates.
    ///
    /// Off on every VM a program runs on, and on for exactly one: the fork the
    /// REPL evaluates `.type` on ([`Vm::ask_questions`]).
    ///
    /// It changes two things, and both are about the difference between *what a
    /// value is* and *what reaching it costs the process*:
    ///
    /// - A call to one of [`stdlib::EFFECTFUL_BUILTINS`] is refused, in
    ///   [`Vm::call`], before the effect it would have. This half exists because
    ///   the other half of that gate — reading the calls out of the expression
    ///   and refusing the effectful ones by name — can only see what is written
    ///   in the expression. A `to` function's body is not written there, an
    ///   object's method is not written there, and a function *value* handed to
    ///   `map` is not written there at all, so all three reach a builtin through
    ///   a name no reading of the expression could have found. This is the
    ///   backstop under all three: every builtin this interpreter calls goes
    ///   through [`Vm::call`] — [`Vm::call_method`] resolves `files.delete(…)`
    ///   to `files_delete` and then calls that — so the refusal happens while
    ///   the effect still has not happened.
    /// - A `print` writes nothing ([`Vm::run_quietly`] already keeps `say` off
    ///   the terminal by not flushing the output buffer). `print` writes at the
    ///   point it is reached, so the only way to keep it out of a question is
    ///   to refuse to write.
    ///
    /// The call half is a flag rather than a list of forbidden names so the rule
    /// is [`stdlib::is_effectful`] itself and the table stays in one place — and
    /// so a refusal cannot drift from the early one, because both ask the same
    /// question of the same name.
    asking_a_question: bool,
    expectation_failure: Option<crate::testing::assertions::TestAssertionError>,
    /// How many test assertions have failed on this run. The count of failures
    /// rather than whether one is recorded: `might fail` has to tell a failure of
    /// its own expression from a test result that expression recorded, and once a
    /// first result exists "is one recorded?" answers yes for every later failure
    /// too — including ones that are not test results at all. See
    /// [`Expr::MightFail`].
    assertions_failed: usize,
    current_span: Span,
    /// How many statements are executing right now, the one in
    /// [`Vm::execute_statement`] itself counting as one.
    ///
    /// Distinct from [`Vm::call_depth`], which counts *calls*: a `say` nested in
    /// a one-line `if` is one call deep but two statements deep, and the
    /// debugger needs the second to tell a breakpoint on that line from a stop
    /// it has already made inside the statement it is looking at.
    statement_depth: usize,
    /// The debugger watching this VM, if one is attached — see
    /// [`Vm::set_debug_hook`].
    ///
    /// `None` for every program that is not being debugged, which is what makes
    /// [`Vm::debug_before`] a single predictable branch on the hot path and
    /// nothing else.
    debug_hook: Option<DebugHook>,
    call_depth: usize,
    /// The user functions currently executing, outermost first — the frame list
    /// [`Vm::debug_before`] hands a hook as [`DebugEvent::call_stack`].
    ///
    /// Pushed with the frame itself in [`Vm::call_user_function`] and popped
    /// with it on every path out, so a frame is on this stack exactly while its
    /// body is running.
    call_stack: Vec<String>,
    /// Where in [`Vm::locals`] the innermost frame's own scopes begin — see
    /// [`DebugEvent::scope_base`]. Zero at the top level.
    frame_scope_base: usize,
    /// The globals each live frame has written, innermost frame last — see
    /// [`DebugEvent::frame_writes`]. One entry per live frame, pushed and popped
    /// with [`Vm::call_stack`].
    frame_writes: Vec<HashSet<String>>,
    max_call_depth: usize,
    /// Statements executed so far, against [`Vm::max_steps`]. Monotonic for the
    /// life of the VM and never restored by `try`, so a caught error cannot buy
    /// a program more budget.
    steps: usize,
    max_steps: usize,
    /// The per-loop iteration cap, applied afresh to every loop so nesting does
    /// not multiply it.
    max_iterations: usize,
    /// Loops the statement being executed is lexically inside. Zero means a
    /// `break` or a `skip` has no loop to act on, which is a
    /// [`Error::Runtime`] rather than a silent no-op. Raised for the body of a
    /// loop and released on every path out of it, so an error caught by a `try`
    /// inside the body does not leave it raised.
    loop_depth: usize,
    /// The signal the body of the running loop raised, cleared by the loop that
    /// consumes it. A flag rather than a returned error, because a `break` is
    /// not a failure: it has to unwind past `if` statements and `try` bodies
    /// that have no idea a loop is around them.
    loop_control: Option<LoopControl>,
    /// Per-source-line self time, while `.profile` is measuring a block.
    ///
    /// `None` for every ordinary run, so the only cost a program pays for this
    /// instrument is the branch in [`Vm::execute_statement`].
    line_timer: Option<LineTimer>,
}

/// One statement's clock reading, while it is executing.
///
/// A frame holds its own start and how much of its elapsed time belongs to
/// statements nested inside it. Subtracting the child time leaves the statement's
/// own cost, so a parent and a child on different source lines are not counted
/// twice in the report.
struct ActiveLine {
    line: usize,
    started: Instant,
    child_nanos: u128,
}

/// The per-line totals one profiled block is building.
#[derive(Default)]
struct LineTimer {
    /// Self time by source line, first-completion order. A `Vec` rather than a
    /// map so two runs of the same block report the same order.
    totals: Vec<(usize, u128)>,
    stack: Vec<ActiveLine>,
}

/// What a `break` or a `skip` asks the innermost enclosing loop to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopControl {
    /// Leave the loop, discarding the iterations that would have followed.
    Break,
    /// Go on to the next iteration, discarding the rest of this one's body.
    Skip,
}

/// One `module` declaration, after its body has run.
///
/// The published names are the members the module's own namespace answers to,
/// which is what an `export` decides. They are held here rather than in
/// [`CallContext::modules`] because a declared module is one the program itself
/// declared, not one read out of a file.
///
/// Shared with the bytecode VM, which records a declared module the same way —
/// see [`crate::bytecode::vm::BytecodeVm::finish_module`].
#[derive(Debug, Default, Clone)]
pub(crate) struct DeclaredModule {
    /// The published members of a declared module: each name, the
    /// `module_member` name it is reached by, and its value.
    pub(crate) members: Vec<(String, String, Value)>,
}

/// The name a member of module `module` is reached by: `MathUtils.circle_area`
/// is `MathUtils_circle_area`, which is the same `module_function` name the
/// builtin namespaces use, so both kinds are called by the one lookup.
///
/// Shared with the bytecode VM, which resolves a call the same way — see
/// [`crate::bytecode::vm::BytecodeVm::call_method`].
pub(crate) fn qualified_member(module: &str, member: &str) -> String {
    format!("{module}_{member}")
}

/// One `object` declaration, after its parent has been merged into it.
///
/// Crate-visible rather than private because it is one of the tables
/// [`CallContext`] holds, and that struct is public: a public struct cannot
/// hold a value of a type nobody outside the crate can name.
#[derive(Debug, Clone)]
pub(crate) struct ObjectType {
    /// The object this one extends, kept so that a child can be resolved
    /// against its own parent after a later declaration changes nothing.
    pub(crate) parent: Option<String>,
    /// Declared fields, defaults included, in declaration order. A field the
    /// child declares keeps the child's position and the child's default, and
    /// a parent field that reaches the same name is not copied in.
    pub(crate) fields: Fields,
    /// Methods the type answers to, nearest declaration first.
    pub(crate) methods: Fields,
}

/// The paths `import <name>` looks for under `root`, in order: `pinned` — the
/// version the manifest there pins the module to — then the `modules/`
/// directory, the same directory written with a leading `./`, and the name as a
/// path of its own.
///
/// The pin comes first because it is the only one of the four that says which
/// version the program asked for; the rest are what an import did before there
/// was a manifest, and a program with no manifest still resolves through them.
///
/// `pinned` is a parameter rather than a lookup because a program is answered
/// from one manifest, and whoever runs it resolves that manifest once and holds
/// the answer: the analyzer in [`crate::analyzer::analyze_in`], the tree-walking
/// VM in [`crate::manifest::Pins`] it carries, the bytecode VM in the one it
/// carries. Each of them is asked once per program rather than once per import,
/// which is both the honest reading of "one manifest per program" and the only
/// way the three agree about a program run from anywhere but its own directory.
fn module_search_paths(pinned: Option<String>, name: &str) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(pinned) = pinned {
        paths.push(pinned);
    }
    paths.extend([
        format!("modules/{name}.rb"),
        format!("./modules/{name}"),
        name.to_string(),
    ]);
    paths
}

/// The file the module `name` resolves to under `root`, reading `pinned` first,
/// or `None` when no path is there.
///
/// The path it returns is **rooted** — already joined to `root` — because that
/// is the text a filesystem is asked about: on `wasm32-unknown-unknown` the
/// filesystem is a map from path text to bytes, and on a disk a path relative to
/// the working directory is not a path relative to the program. Returning a
/// rooted path is what makes "find the file by one rule and open it by another"
/// impossible rather than merely unlikely.
///
/// A directory is not a module. `modules/Fixture/` is where the versions of
/// `Fixture` are published — see [`crate::manifest`] — so it is a path the
/// search skips rather than one it fails to read: without a manifest naming a
/// version, `import Fixture` has no module to import, and saying so beats
/// `Is a directory` on a directory that was never meant to be opened.
fn module_file_in(root: &Path, pinned: Option<&str>, name: &str) -> Option<String> {
    module_search_paths(pinned.map(str::to_string), name)
        .into_iter()
        .map(|path| crate::vfs::rooted(root, &path))
        .find(|path| crate::vfs::is_file(path))
}

/// The source of the module `name` resolves to under `root`, reading `pinned`
/// first, or `None` when no path is there. A path that is there but cannot be
/// read is an error, not a miss: the two say different things about the import.
///
/// Through [`crate::vfs`], for the reason [`crate::runtime::module_program`]
/// gives — a module is a file in the same filesystem a `files.write` writes to,
/// and a `std::fs` read here would compile, pass every native test and fail on
/// `wasm32-unknown-unknown`.
fn module_source(root: &Path, pinned: Option<&str>, name: &str) -> Result<Option<String>> {
    let Some(path) = module_file_in(root, pinned, name) else {
        return Ok(None);
    };
    let source = crate::vfs::read_to_string(&path)
        .map_err(|e| Error::Io(format!("Cannot load module '{path}': {e}")))?;
    Ok(Some(source))
}

/// The names an `import` of `name` under `root` binds into the importing
/// program: the module's own `set` and `constant` declarations, which is
/// everything the loader runs.
///
/// Read by the analyzer, which cannot see into a separate file, so that a read of
/// one of these names is a read of a name the program binds. A module that cannot
/// be found, read or parsed binds nothing here — the loader still reports that at
/// runtime, and the analyzer must not report it first. Which version of the module
/// those names come from is the manifest's answer, passed in as `pinned` because
/// the analyzer resolved it once for the whole program: a program importing the
/// same module at two versions is told which of the two it is reading.
pub(crate) fn module_bound_names(root: &Path, pinned: Option<&str>, name: &str) -> Vec<String> {
    let Ok(Some(source)) = module_source(root, pinned, name) else {
        return Vec::new();
    };
    let Ok(tokens) = Lexer::tokenize(&source) else {
        return Vec::new();
    };
    let Ok(ast) = redblue_parser::Parser::new(tokens).parse() else {
        return Vec::new();
    };

    redblue_parser::module_body(&ast)
        .iter()
        .filter_map(|stmt| match &stmt.statement {
            Statement::Set { name, .. } | Statement::Constant { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

impl Vm {
    pub fn new() -> Self {
        Self::new_in(std::path::Path::new("."))
    }

    /// A VM that resolves its imports against `root`.
    ///
    /// The manifest in `root` is the one that decides which version of a module
    /// an `import` binds, and it is the manifest
    /// [`crate::analyzer::analyze_in`] reads when it was asked about the same
    /// program. A VM that resolved against `.` instead would load a different
    /// file than the analyzer checked, for every program not run from its own
    /// directory.
    pub fn new_in(root: &std::path::Path) -> Self {
        let globals = stdlib::builtins();
        Self {
            globals,
            constants: HashSet::new(),
            locals: vec![CapturedScope::new()],
            output: Vec::new(),
            output_bytes: 0,
            output_full: false,
            break_statements: 0,
            skip_statements: 0,
            context: CallContext::default(),
            root: root.to_path_buf(),
            pins: crate::manifest::Pins::for_running(root),
            importing: Vec::new(),
            module_depth: 0,
            builtin_names: stdlib::builtins().keys().cloned().collect(),
            asking_a_question: false,
            expectation_failure: None,
            assertions_failed: 0,
            current_span: Span::unknown(),
            statement_depth: 0,
            debug_hook: None,
            call_depth: 0,
            call_stack: Vec::new(),
            frame_scope_base: 0,
            frame_writes: Vec::new(),
            max_call_depth: resolve_max_call_depth(),
            steps: 0,
            max_steps: resolve_max_steps(),
            max_iterations: resolve_max_iterations(),
            loop_depth: 0,
            loop_control: None,
            line_timer: None,
        }
    }

    /// A VM holding a copy of this one's bindings and none of its history.
    ///
    /// What it copies is everything a program can *read*: the globals, the
    /// `constant` names, the live local scopes, the modules already run, the
    /// aliases they were imported under, the modules the program declared, the
    /// members those published, and the objects it declared. So an expression
    /// written against this session resolves exactly as it would against the
    /// session itself — `nums[0]` and `MathUtils.circle_area(5)` both answer.
    ///
    /// What it does not copy is everything that *happened*: the output buffer,
    /// the step and call budgets already spent, the recorded test failures, the
    /// current span, and the debugger's hook. It is a VM on which to try
    /// something out, not the VM the session is made of — which is what a REPL
    /// command that asks a question needs. `.type` runs on a fork precisely so
    /// that asking what `f()` answers cannot bind `x`, cannot print, and cannot
    /// half-run a program against the session's own bindings.
    ///
    /// The copy is shallow where the values are shared: a [`Value::Function`]
    /// body is an `Arc`, so the fork calls the same declaration the session does
    /// rather than a copy of it. What the fork keeps is the *bindings* those
    /// declarations read and write, which is the part `.type` must not change.
    ///
    /// The question flag is **not** copied, and starts off: a fork is a VM
    /// somebody may run a question on, and whether this one is for that is the
    /// caller's decision ([`Vm::ask_questions`]), not something inherited from
    /// the VM it was made from.
    pub fn fork(&self) -> Vm {
        let mut forked = Vm::new_in(&self.root);
        forked.globals = self.globals.clone();
        forked.constants = self.constants.clone();
        forked.locals = self.locals.clone();
        forked.context = self.context.clone();
        forked
    }

    /// The directory this VM resolves a program's `import` against.
    ///
    /// A VM that answers a question *for* a program - the debug sub-REPL's
    /// scratch VM, evaluating a watch expression - has to resolve an import
    /// where that program resolves it, or `MathUtils.circle_area(5)` is "no such
    /// module" because the watcher looked in the wrong directory.
    pub(crate) fn root(&self) -> &std::path::Path {
        &self.root
    }

    /// Re-points this VM at the bindings the observed step was running with, so
    /// an expression evaluated now reads the program's variables as they are
    /// *at that step* rather than as they were at some earlier one.
    ///
    /// The debug sub-REPL's `.watch` uses this on a scratch VM the hook owns:
    /// each observed step overwrites the bindings wholesale, so nothing
    /// accumulates between steps and nothing the watched expression itself does
    /// — a `set` it contains, an output it triggers — survives to the next one.
    /// The output buffer is drained with the bindings for the same reason, and
    /// the statement nesting restarts at zero because this VM is between
    /// statements, not inside one.
    ///
    /// The counters of a run restart with the bindings, because one evaluation
    /// is one run: the steps already spent, the calls still open, the loop
    /// still running and the failures already recorded are all of the
    /// *previous* evaluation. A counter that carried over would make the second
    /// watch of a step start where the first left off — and, with the step
    /// budget, would eventually refuse every watch with a step-limit failure
    /// that has nothing to do with the watch being watched.
    ///
    /// Everything else about the VM is left alone: the scratch's hook (there is
    /// none — a watcher is not itself watched), its question flag, and its
    /// budgets.
    pub fn observe_bindings(&mut self, scopes: &[CapturedScope], globals: &HashMap<String, Value>) {
        self.locals = scopes.to_vec();
        self.globals = globals.clone();
        self.statement_depth = 0;
        self.steps = 0;
        self.call_depth = 0;
        self.call_stack.clear();
        self.frame_writes.clear();
        // The locals stack was replaced wholesale, so the base the frame's own
        // scopes begin at is a number into a stack that no longer exists. Left
        // stale it would index the wrong scopes, and `DebugEvent::scope_base`
        // would hand the watcher the caller's bindings as the frame's own.
        self.frame_scope_base = 0;
        self.loop_depth = 0;
        self.loop_control = None;
        self.assertions_failed = 0;
        self.expectation_failure = None;
        let _ = self.take_output();
    }

    /// Re-points this VM at the language context the observed step was running
    /// with: the modules the program has, the aliases it named them by, the
    /// members they published and the objects it declared.
    ///
    /// The bindings alone are not enough to evaluate an expression *for* a
    /// program. `MathUtils.circle_area(5)` is a member of a module the program
    /// declared, `J.parse(..)` is a member of a module it imported under an
    /// alias, and `counter.bump()` is a method of an object it declared — and
    /// without this copy every one of the three is a "no such function" the
    /// expression cannot answer. See [`CallContext`].
    pub fn observe_context(&mut self, context: &CallContext) {
        self.context = context.clone();
    }

    /// The language context this VM has built: the modules it has imported, the
    /// aliases it named them by, the modules it declared, the members they
    /// published and the objects it declared.
    ///
    /// Borrowed rather than copied, so a caller can hand it to a VM of its own
    /// without paying for the copy twice.
    pub fn call_context(&self) -> &CallContext {
        &self.context
    }

    /// The bindings this VM currently holds: the scope stack and the globals,
    /// borrowed.
    ///
    /// The debug sub-REPL's watches need them after a run finishes — the value
    /// a program's last statement produced is first visible *there*, at the end,
    /// rather than at any statement stop — and this hands them over without a
    /// clone. The caller may not write through them.
    pub fn current_bindings(&self) -> (&[CapturedScope], &HashMap<String, Value>) {
        (&self.locals, &self.globals)
    }

    /// Declares that this VM is answering a question about a value rather than
    /// running a program, and is either.
    ///
    /// Off everywhere by default. A caller answering a question — the REPL's
    /// `.type`, on a [`Vm::fork`] — says so for the VM it evaluates that question
    /// on and nothing else does, because for a program these builtins and this
    /// output are the point. See [`Vm::asking_a_question`] for what changes.
    pub fn ask_questions(&mut self, asking: bool) {
        self.asking_a_question = asking;
    }

    /// The body of the function `name` names, when `name` holds one.
    ///
    /// Every way a call can reach a Redblue function this VM declared, in the
    /// order [`Vm::call`] and [`Vm::call_method`] reach it, so a caller asking
    /// "what does calling this do?" reads the same declaration the run would:
    ///
    /// - a bare name — `evil()` is `call("evil", …)`, which ends at
    ///   [`Vm::get_var`] — read out of the globals and the live scopes, because a
    ///   function declared inside a function closes over them;
    /// - a module member — `MathUtils.circle_area(5)` is `call("MathUtils_circle_area", …)`
    ///   after [`Vm::call_method`] resolved the module and its alias;
    /// - an object's method — `counter.bump()` is not a name at all, it is
    ///   `objects[counter].methods[bump]`, and the receiver is a name the caller
    ///   has in hand.
    ///
    /// `None` for anything else, and for a name that holds something that is not
    /// a function: a builtin has no body to read, and a caller that reached a
    /// builtin reads [`stdlib::EFFECTFUL_BUILTINS`] instead.
    pub fn function_body(&self, name: &str) -> Option<&FunctionBody> {
        match self.get_var_ref(name) {
            Some(Value::Function(function)) => Some(&function.body),
            _ => None,
        }
    }

    /// The method `method` of the `object` declaration named `object`.
    ///
    /// The third of [`Vm::function_body`]'s three shapes, and separate because
    /// the receiver of an object's method is *not* a name a value is bound to —
    /// it is the type's own name — so a caller reading a dotted call off an AST
    /// has both halves and needs them put back together.
    pub fn object_method(&self, object: &str, method: &str) -> Option<&Value> {
        self.context
            .objects
            .get(object)
            .and_then(|declaration| declaration.methods.get(method))
    }

    /// The module an `import … as <name>` bound `<name>` to, or `name` itself.
    ///
    /// `import files as F` binds `F`, and `F.delete(…)` reaches
    /// `files.delete(…)`. A caller that has to recognise what a dotted call
    /// touches — what `.type` refuses, what `.which` names — has to resolve the
    /// alias first or it is looking at a name the language never registered.
    /// Every module declares itself in this table under its own name
    /// ([`Vm::declare_module`]), so the unaliased case is a hit rather than a
    /// miss.
    pub fn module_behind<'a>(&'a self, name: &'a str) -> &'a str {
        self.context
            .module_aliases
            .get(name)
            .map(String::as_str)
            .unwrap_or(name)
    }

    /// Builds a VM with an explicit call-depth limit, ignoring the environment.
    pub fn with_max_call_depth(max_call_depth: usize) -> Self {
        let mut vm = Self::new();
        if max_call_depth > 0 {
            vm.max_call_depth = max_call_depth;
        }
        vm
    }

    /// Builds a VM with an explicit per-loop iteration cap, ignoring the
    /// environment. A cap of zero is ignored: it would make every loop illegal,
    /// which is never what a caller means.
    pub fn with_max_iterations(max_iterations: usize) -> Self {
        let mut vm = Self::new();
        vm.max_iterations = resolve_max_iterations_from(Some(max_iterations));
        vm
    }

    /// Builds a VM with an explicit step budget, ignoring the environment. A
    /// budget of zero is ignored, for the same reason as
    /// [`Vm::with_max_iterations`].
    pub fn with_max_steps(max_steps: usize) -> Self {
        let mut vm = Self::new();
        vm.max_steps = resolve_max_steps_from(Some(max_steps));
        vm
    }

    /// Sets the step budget this VM enforces, ignoring the environment.
    ///
    /// A budget of zero is ignored, for the same reason as
    /// [`Vm::with_max_steps`]. This is the setter for a VM that already
    /// exists — the debug sub-REPL's watch scratch, which is built once and
    /// then answers every watch.
    pub fn set_max_steps(&mut self, max_steps: usize) {
        if max_steps > 0 {
            self.max_steps = max_steps;
        }
    }

    /// Builds a VM with both limits set at once, ignoring the environment.
    ///
    /// Neither [`Vm::with_max_iterations`] nor [`Vm::with_max_steps`] can be
    /// followed by the other — each builds a VM from the defaults — and a caller
    /// that wants to know *which* of the two stops a program has to set both. A
    /// zero is ignored, as it is for either of them.
    pub fn with_limits(max_iterations: usize, max_steps: usize) -> Self {
        let mut vm = Self::new();
        vm.max_iterations = resolve_max_iterations_from(Some(max_iterations));
        vm.max_steps = resolve_max_steps_from(Some(max_steps));
        vm
    }

    /// Charges one statement to the step budget, failing once the program has
    /// run [`Vm::max_steps`] statements. Called for every statement, including
    /// those inside a function body, so the budget bounds the program rather
    /// than one statement list.
    ///
    /// One turn of a loop is charged as well — see [`Vm::charge_iteration`] — so
    /// the unit is a statement on both engines and the two reach the budget at
    /// the same turn of the same loop.
    fn charge_step(&mut self) -> Result<()> {
        if self.steps >= self.max_steps {
            return Err(Error::Limit(
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

    /// Charges one iteration to `loop_iterations`, failing once the loop that
    /// owns the counter has run [`Vm::max_iterations`] times. The counter is a
    /// local of the loop statement, so the cap is per loop and nesting does not
    /// multiply it.
    ///
    /// The turn is charged to the step budget as well, and after the iteration
    /// cap, so a loop inside its own cap is never stopped by the budget instead:
    /// the cap is what says how many turns a loop takes, so naming it is what
    /// both engines then say about the same program.
    fn charge_iteration(&mut self, loop_iterations: &mut usize, kind: &str) -> Result<()> {
        if *loop_iterations >= self.max_iterations {
            return Err(Error::Limit(
                format!(
                    "Maximum of {} iterations reached in a '{}' loop",
                    self.max_iterations, kind
                ),
                self.span(),
            ));
        }
        *loop_iterations += 1;
        self.charge_step()
    }

    /// Runs one iteration of a loop: binds `variable` to `value` in a fresh
    /// scope, runs `body`, and reports the [`LoopControl`] the body raised.
    ///
    /// The scope and [`Vm::loop_depth`] are released on every path out,
    /// including the failing one, so an error that a `try` inside the body
    /// catches does not leave the depth raised and make a later `break` — one
    /// written outside every loop — look legal.
    ///
    /// Stops at the first `break` or `skip` in the body rather than running the
    /// statements after it, and hands the signal to the caller, which is the
    /// only place that knows whether it leaves the loop or goes round again.
    /// A `break` therefore leaves the innermost loop and only that one: the
    /// enclosing loop never sees the signal, because this call already took it.
    fn run_iteration(
        &mut self,
        variable: Option<&str>,
        value: Option<Value>,
        body: &[Stmt],
    ) -> Result<Option<LoopControl>> {
        self.loop_depth += 1;
        let outcome = self.run_iteration_body(variable, value, body);
        self.loop_depth -= 1;
        if outcome.is_err() {
            // A turn that failed is unwinding out of the loop on its own, so a
            // signal it raised before failing — a `break` inside a `try` body
            // whose `finally` then failed — has no loop left to act on. Left
            // pending it would be handed to the next loop in the program, which
            // would end one turn early for a `break` it never contained.
            self.loop_control = None;
        }
        outcome
    }

    /// The body half of [`Vm::run_iteration`], run with the scope and the loop
    /// depth already raised.
    fn run_iteration_body(
        &mut self,
        variable: Option<&str>,
        value: Option<Value>,
        body: &[Stmt],
    ) -> Result<Option<LoopControl>> {
        let scope_base = self.locals.len();
        self.push_scope();
        let bound = match variable {
            Some(variable) => {
                self.declare(variable);
                self.set_var(variable, value.unwrap_or(Value::Nothing))
            }
            None => Ok(()),
        };
        let outcome = bound.and_then(|()| self.run_block(body));
        // Truncated rather than popped, so the stack is balanced even when the
        // turn failed part way through: a scope left behind keeps the loop's
        // variable alive for the rest of the program, and it is a different
        // variable for every one of those turns.
        self.locals.truncate(scope_base);
        let signal = outcome?;
        // The signal was this turn's loop to act on and it has been handed over,
        // so nothing outside this turn may find it still pending.
        self.loop_control = None;
        Ok(signal)
    }

    /// Runs `statements` and reports the [`LoopControl`] one of them raised,
    /// leaving the signal in place for the loop that owns it.
    ///
    /// Every block in the language runs its statements through here or through
    /// [`Vm::execute_statements`], because a `break` or a `skip` has to stop the
    /// block it was written in as well as the loop: `if c then break say "x" end`
    /// must not run the `say`, and the statements after a `break` inside a
    /// `try` body must not run either. A block that kept going would leave the
    /// signal raised while the program ran on past the loop, and the next loop
    /// to reach a `break` of its own would find it already set.
    fn run_block(&mut self, statements: &[Stmt]) -> Result<Option<LoopControl>> {
        self.execute_statements(statements)?;
        Ok(self.loop_control)
    }

    /// The signal a `break` or a `skip` raises in the innermost enclosing loop,
    /// or a [`Error::Runtime`] when there is no loop to raise it in.
    ///
    /// The refusal is the point: a `break` in no loop is a mistake in the
    /// program, and a mistake that runs to completion reporting success is
    /// worse than one that stops with a message naming the statement.
    fn raise_loop_control(&mut self, keyword: &str, signal: LoopControl) -> Result<()> {
        if self.loop_depth == 0 {
            return Err(Error::Runtime(
                format!("'{keyword}' is only valid inside a loop"),
                self.span(),
            ));
        }
        self.loop_control = Some(signal);
        Ok(())
    }

    /// Takes the structured failure from the most recent failed `expect`, so a
    /// caller such as the test harness can report expected and actual values
    /// instead of only a rendered message.
    pub fn take_expectation_failure(
        &mut self,
    ) -> Option<crate::testing::assertions::TestAssertionError> {
        self.expectation_failure.take()
    }

    /// Takes the lines `say` produced, for a caller that wants them rather than
    /// the printing.
    ///
    /// The printing happens in [`Vm::run`], so this is only useful afterwards.
    /// It exists so that a caller comparing this VM with the bytecode VM can see
    /// what each printed without capturing a process's stdout.
    pub fn take_output(&mut self) -> Vec<String> {
        self.output_bytes = 0;
        self.output_full = false;
        std::mem::take(&mut self.output)
    }

    /// Charges `bytes` to the buffered `say` output, failing once one program's
    /// lines would need more than [`crate::wasm::MAX_OUTPUT_BYTES`].
    ///
    /// The same bound [`crate::wasm::emit`] enforces, charged here rather than
    /// there, and on the same `Error::Limit` channel — so the limit is the
    /// interpreter's own, a Redblue program can `catch` it, and the two engines
    /// stop at the same number because they read the same constant.
    ///
    /// One byte per line is the newline [`Vm::run`] appends on the way out, so
    /// what is charged is what the boundary will eventually be handed. The
    /// charge sticks: see [`Vm::output_full`].
    fn charge_output(&mut self, line: &str) -> Result<()> {
        let limit = crate::wasm::MAX_OUTPUT_BYTES;
        let needed = line.len().saturating_add(1);
        if self.output_full || self.output_bytes.saturating_add(needed) > limit {
            self.output_full = true;
            return Err(Error::Limit(
                format!(
                    "Output past the limit of {limit} bytes: this program said more than the \
                     interpreter will hold"
                ),
                self.span(),
            ));
        }
        self.output_bytes += needed;
        Ok(())
    }

    /// How many `break` and `skip` statements this VM has evaluated, as
    /// `(breaks, skips)`.
    ///
    /// The printed lines say what a `break` *did*; this says whether the
    /// statement was reached at all, which is the half the output cannot
    /// distinguish. A `break` that was decoded and dropped and a `break` that
    /// ended a loop print the same lines in a loop that would have ended anyway.
    /// The bytecode VM counts the same two things over its opcodes, through
    /// [`crate::bytecode::vm::BytecodeVm::loop_control_counts`].
    pub fn loop_control_counts(&self) -> (usize, usize) {
        (self.break_statements, self.skip_statements)
    }

    /// The names this VM has bound that the standard library did not: what a
    /// `set`, a `to` or an `import` put here.
    ///
    /// A REPL asks for this three times over — to seed the analyzer so the next
    /// line can read a name an earlier line bound, to offer its own names for
    /// completion, and to list them for `:vars` and `:funcs`. It has to be the
    /// names the standard library did *not* put there: offering `abs` or `files`
    /// as a session's own is noise, and it is the only way to tell the two apart,
    /// since both live in `globals`.
    ///
    /// The local scopes are read as well as `globals`, because a name is not
    /// always a global: `to` binds a function with [`Vm::declare`] and
    /// [`Vm::set_var`], which lands in the innermost live scope. Scanning
    /// `globals` alone therefore never offered a function the program had just
    /// defined, which is the name a REPL user is most likely to be reaching for
    /// next.
    ///
    /// Sorted and deduplicated, because `globals` and each scope are hash maps and
    /// their iteration order is not a result.
    ///
    /// The `module_member` names a module's members are published under are left
    /// out — see [`CallContext::qualified_members`]. They are in `globals` because that is
    /// where [`Vm::call_method`] looks for them, and a name the program never
    /// wrote is not one of its names: after `import MathUtils` this used to offer
    /// `MathUtils_circle_area` to completion and list it in `:vars`, which is a
    /// name no line of Redblue can call.
    pub fn user_names(&self) -> Vec<String> {
        let builtins = stdlib::builtins();
        let mut names: Vec<String> = self
            .globals
            .keys()
            .filter(|name| {
                !builtins.contains_key(*name) && !self.context.qualified_members.contains(*name)
            })
            .cloned()
            .collect();
        for scope in &self.locals {
            names.extend(scope.keys().cloned());
        }
        names.sort();
        names.dedup();
        names
    }

    /// Publishes `value` as the member `member` of `module`, under the
    /// `module_member` name a call through the module resolves to.
    ///
    /// Every writer of such a name goes through here, so the set of them is
    /// known exactly and [`Vm::user_names`] does not have to guess which globals
    /// are mangled names and which are a program's own.
    fn publish_member(&mut self, module: &str, member: &str, value: Value) {
        let name = qualified_member(module, member);
        self.context.qualified_members.insert(name.clone());
        self.globals.insert(name, value);
    }

    /// Whether `name` is a module member this VM published from a module a
    /// program declared or imported.
    ///
    /// Not the same as "the name is bound": every builtin is bound in `globals`,
    /// so `files_read` answers here and is not a member any program published.
    /// What a program's module publishes goes through [`Vm::publish_member`], and
    /// that is what is being asked about — a REPL's `.which` has to tell
    /// `files.read` (a stdlib member) from `MathUtils.circle_area` (a member of a
    /// module this session imported), and only the record of publication tells
    /// them apart.
    pub fn published_member(&self, name: &str) -> bool {
        self.context.qualified_members.contains(name)
    }

    /// The values this VM published under module-member names, sorted by name.
    ///
    /// A session switching to the bytecode engine seeds these alongside its own
    /// names: `M.circle_area(..)` resolves to `MathUtils_circle_area`, and the
    /// call has nothing to reach unless that binding crosses with the context
    /// that says which module the alias names.
    pub(crate) fn published_member_bindings(&self) -> Vec<(String, Value)> {
        let mut bindings: Vec<(String, Value)> = self
            .context
            .qualified_members
            .iter()
            .filter_map(|name| {
                self.globals
                    .get(name)
                    .map(|value| (name.clone(), value.clone()))
            })
            .collect();
        bindings.sort_by(|left, right| left.0.cmp(&right.0));
        bindings
    }

    /// Every module name an `import` bound in this program, paired with the
    /// module it names, sorted by the bound name.
    ///
    /// `import files as F` binds both `F` and `files`, and both are entries
    /// here: the tree a REPL prints is the names a program can write, not one
    /// canonical spelling of each module.
    pub fn imported_modules(&self) -> Vec<(String, String)> {
        let mut modules: Vec<(String, String)> = self
            .context
            .module_aliases
            .iter()
            .map(|(bound, module)| (bound.clone(), module.clone()))
            .collect();
        modules.sort();
        modules
    }

    /// The members `module` exposes, as `(member, value)`, sorted by member.
    ///
    /// Read from the same places a module resolves through: the standard
    /// library's member table, a `module … end` declaration's published
    /// members, and the names an imported module file declares — its functions
    /// under their qualified names, its `set` and `constant` names under the
    /// ordinary bindings the import gives them. A module name nothing publishes
    /// anything under is an empty list rather than an invented one.
    pub fn module_member_values(&self, module: &str) -> Vec<(String, Value)> {
        let mut members: std::collections::BTreeMap<String, Value> =
            std::collections::BTreeMap::new();

        if let Some((_, listed)) = crate::stdlib::MODULE_MEMBERS
            .iter()
            .find(|(name, _)| *name == module)
        {
            for member in *listed {
                if crate::stdlib::member_resolves(module, member) {
                    members.insert(
                        (*member).to_string(),
                        Value::Builtin(format!("{module}.{member}")),
                    );
                }
            }
        }

        if let Some(declared) = self.context.declared_modules.get(module) {
            for (member, _, value) in &declared.members {
                members.insert(member.clone(), value.clone());
            }
        }

        // A loaded module's members are read from the names its own body
        // declares rather than by stripping `module_` off every qualified name.
        // A prefix is not a boundary: module `alpha` and module `alpha_beta`
        // share one, so `alpha_beta_two` would otherwise be listed as `alpha`'s
        // member `beta_two`. A file's `set` and `constant` names are bound flat
        // rather than under a qualified name, so their ordinary binding is what
        // the import contributes and what `.env` lists under the module.
        if let Some(program) = self.context.modules.get(module) {
            for member in crate::parser::module_declared_names(crate::parser::module_body(program))
            {
                let qualified = qualified_member(module, &member);
                let value = if self.context.qualified_members.contains(&qualified) {
                    self.globals.get(&qualified)
                } else {
                    self.globals.get(&member)
                };
                if let Some(value) = value {
                    members.insert(member, value.clone());
                }
            }
        }

        members.into_iter().collect()
    }

    /// The declared fields and methods of the object type `name`, in declaration
    /// order, or `None` when no `object` declaration has that name.
    pub fn object_type_members(&self, name: &str) -> Option<(Vec<String>, Vec<String>)> {
        let object = self.context.objects.get(name)?;
        Some((
            object.fields.keys().cloned().collect(),
            object.methods.keys().cloned().collect(),
        ))
    }

    /// The object type `value` is an instance of, by the shape it carries.
    ///
    /// A declaration binds a record rather than a tagged object, and a copy of
    /// it — `set c to Counter`, or what a `create` method hands back — is a
    /// record too. The field names in declaration order are the shape both
    /// carry, so a value whose field order matches a declared object is that
    /// object's instance. `hint` disambiguates two declarations with the same
    /// shape when the expression names one of them; without a matching hint an
    /// ambiguous shape is a plain record rather than a guess.
    pub fn object_type_of_value(&self, value: &Value, hint: Option<&str>) -> Option<String> {
        match value {
            Value::Object(name, _) => Some(name.clone()),
            Value::Record(fields) => {
                let mut matches: Vec<&String> = self
                    .context
                    .objects
                    .iter()
                    .filter(|(_, object)| object.fields.keys().eq(fields.keys()))
                    .map(|(name, _)| name)
                    .collect();
                matches.sort();
                if let Some(hint) = hint {
                    if matches.iter().any(|name| name.as_str() == hint) {
                        return Some(hint.to_string());
                    }
                }
                if matches.len() == 1 {
                    Some(matches[0].clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Every object type the program declared, as `(name, fields, methods)`,
    /// sorted by name.
    pub fn object_types(&self) -> Vec<(String, Vec<String>, Vec<String>)> {
        let mut types: Vec<(String, Vec<String>, Vec<String>)> = self
            .context
            .objects
            .iter()
            .map(|(name, object)| {
                (
                    name.clone(),
                    object.fields.keys().cloned().collect(),
                    object.methods.keys().cloned().collect(),
                )
            })
            .collect();
        types.sort_by(|left, right| left.0.cmp(&right.0));
        types
    }

    /// The value bound to `name`, wherever this VM keeps it.
    ///
    /// What a REPL's `:inspect` and `:vars` read, and neither of them could
    /// answer from its own bookkeeping: a `set`, a `to` and an `import` all bind
    /// here, not in the REPL's own map, so `:inspect <session-name>` reported
    /// "not found" for every name a session had actually bound.
    pub fn user_value(&self, name: &str) -> Option<&Value> {
        self.get_var_ref(name)
    }

    /// The names in [`Vm::user_names`] that hold a callable the program
    /// declared, sorted.
    ///
    /// `:funcs` is a listing, so it reads this rather than printing a sentence
    /// about where functions are stored: "User-defined functions are stored in
    /// the VM" told the user nothing they could not already see from the same
    /// line, and named none of the functions they had just written. A builtin is
    /// a [`Value::Builtin`] and is already excluded from [`Vm::user_names`], so
    /// what is left is what a `to` declared.
    ///
    /// A module's functions are not listed. They are reached as
    /// `MathUtils.circle_area(..)`, and [`Vm::user_names`] deliberately holds the
    /// published `MathUtils_circle_area` back rather than showing a name the user
    /// cannot type; the module's own name is one of the session's names, so it is
    /// still offered and inspectable.
    pub fn user_functions(&self) -> Vec<String> {
        self.user_names()
            .into_iter()
            .filter(|name| matches!(self.user_value(name), Some(Value::Function(_))))
            .collect()
    }

    /// Runs a module's declarations into this VM, under the name the `import`
    /// that reached it goes by.
    ///
    /// A module already loaded is left alone: importing it twice binds the same
    /// names twice, and binding a name twice is what a `set` refuses to do and a
    /// `constant` cannot do at all — the second declaration of the module's own
    /// `TAU` would be refused as a duplicate of the one the first import bound.
    ///
    /// The file is read and parsed once: the bindings and the record that the
    /// module is loaded both come out of that one `Program`. Reading it a second
    /// time could fail after some of the bindings were already installed, which
    /// would leave the program with half a module in it.
    fn load_module(&mut self, path: &str, name: &str) -> Result<()> {
        if self.context.modules.contains_key(name) {
            return Ok(());
        }

        self.importing.push(name.to_string());
        let program = match runtime::module_program(path) {
            Ok(program) => program,
            Err(error) => {
                self.importing.pop();
                return Err(error);
            }
        };
        let bound = match runtime::module_bindings(&program, |expr| self.evaluate(expr)) {
            Ok(bound) => bound,
            Err(error) => {
                self.importing.pop();
                return Err(error);
            }
        };
        for (name, value, is_const) in bound {
            if is_const {
                self.bind_constant(&name, value)?;
            } else {
                self.bind_module_name(&name, value)?;
            }
        }
        self.importing.pop();

        // The module's functions are reached as `Name.function`, which is the
        // `module_function` name a builtin namespace member has too, so the two
        // kinds of module are called the same way. Binding them is what makes
        // `import MathUtils as M` then `M.circle_area(5)` run.
        //
        // They are built in a scope of the module's own, so a function the
        // module declares is not a name the importing program reads.
        self.push_scope();
        let mut failure = None;
        for stmt in crate::parser::module_body(&program) {
            if failure.is_some() {
                continue;
            }
            if matches!(stmt.statement, Statement::Function { .. }) {
                if let Err(error) = self.execute_statement(stmt) {
                    failure = Some(error);
                }
            }
        }
        if failure.is_none() {
            for member in crate::parser::module_declared_names(crate::parser::module_body(&program))
            {
                let Some(value) = self.declare_member(&member) else {
                    continue;
                };
                self.publish_member(name, &member, value);
            }
        }
        self.pop_scope();
        if let Some(error) = failure {
            return Err(error);
        }

        self.context
            .modules
            .insert(name.to_string(), Arc::new(program));
        Ok(())
    }

    /// Whether `name` names a module this program built: a module file it
    /// imported, or a `module … end` it declared.
    ///
    /// This is the half of "is this a namespace" the program's own declarations
    /// answer, and it is what tells a module the program wrote apart from one
    /// [`stdlib::is_module`] lists. The two are asked apart on purpose: the
    /// stdlib's member table is the agreed surface of *its* modules, and applying
    /// it to a module the program declared refuses members the program just
    /// published — `module text … end` publishing `greet` is not
    /// `Module 'text' has no function 'greet'`. Where the two meet is
    /// [`stdlib::dotted_call_reaches`].
    ///
    /// Public because a REPL asks it about a name rather than about a call: a
    /// `module Foo … export all end` binds `Foo_greet` and no `Foo`, so the only
    /// evidence that the declaration ran is this. See
    /// [`crate::repl::Repl::record_definitions`].
    pub fn declares_module(&self, name: &str) -> bool {
        self.context.modules.contains_key(name) || self.context.declared_modules.contains_key(name)
    }

    /// The value of a name a module body declares, or `None` when it declares
    /// nothing of that name — an `export` of a name the module does not define
    /// is what notices the absence, not this.
    fn declare_member(&mut self, name: &str) -> Option<Value> {
        let index = self
            .locals
            .iter()
            .rposition(|scope| scope.contains_key(name))?;
        self.locals[index].get(name).cloned()
    }

    /// Runs a `module Name ... export ... end` declaration.
    ///
    /// The body runs in a scope of its own, so a `set` inside a module is not a
    /// name of the program that declared it — the module is a boundary, which
    /// is the whole point of the declaration. What the module publishes is
    /// bound back as `Name.member`, the name a call through the module reaches.
    ///
    /// The body is a block like any other: a `break` or a `skip` written in it
    /// ends the statements after it there as well as the loop the declaration
    /// is written in, and a body left that way publishes nothing and declares
    /// nothing. A module body is *not* exempt from the caller's loop the way a
    /// function body is: a function body runs when it is called, while a module
    /// body runs where it is written, which is what makes a `break` in it a
    /// `break` in the loop around the declaration.
    fn declare_module(&mut self, name: &str, body: &[Stmt]) -> Result<Value> {
        if self.context.declared_modules.contains_key(name) {
            return Err(Error::Runtime(
                format!("Module '{name}' is already declared"),
                self.span(),
            ));
        }

        // What the module publishes is decided from its own declarations,
        // before anything runs: an `export` of a name the module does not
        // define is a fault in the declaration, and reporting it here leaves
        // the module unregistered rather than half-declared.
        let declared = crate::parser::module_declared_names(body);
        let exported = match crate::parser::module_exports(body) {
            // No `export` at all: a module publishes nothing, which is not an
            // error — the declaration says what it offers and offers nothing.
            None => Vec::new(),
            Some((_, true)) => declared.clone(),
            Some((names, false)) => {
                for exported in &names {
                    if !declared.contains(exported) {
                        return Err(Error::Runtime(
                            format!(
                                "Module '{name}' exports '{exported}', which it does not define"
                            ),
                            self.span(),
                        ));
                    }
                }
                names
            }
        };

        // Registered before the body runs, so an `import` of this module from
        // inside its own body is a circular import rather than a second
        // declaration or a miss.
        self.context
            .declared_modules
            .insert(name.to_string(), DeclaredModule::default());
        self.importing.push(name.to_string());

        self.push_scope();
        self.module_depth += 1;
        let mut failure = None;
        for stmt in body {
            if failure.is_some() {
                continue;
            }
            // A `break` or a `skip` written in the module body ends the body as
            // well as the loop the declaration is written in, which is the block
            // rule every other block here already follows: the statements after
            // it are part of the turn the signal stopped. A module body runs
            // where it is written, so it is inside that loop exactly as an
            // `object` body is — and the signal is left pending for the loop,
            // which is the only thing that consumes it.
            if self.loop_control.is_some() {
                break;
            }
            // The `export` has already been read; running it again would
            // publish nothing.
            if !matches!(stmt.statement, Statement::Export { .. }) {
                if let Err(error) = self.execute_statement(stmt) {
                    failure = Some(error);
                }
            }
        }

        // Read while the module's scope is still live: a member that is not
        // bound is not published, which is what stops a module offering a
        // function whose declaration failed.
        //
        // A body an exit left early publishes nothing either, for the same
        // reason a failed one does: the statements that would have bound the
        // members did not run, so there is nothing to read.
        let mut members = Vec::new();
        if failure.is_none() && self.loop_control.is_none() {
            for member in &exported {
                let Some(value) = self.declare_member(member) else {
                    continue;
                };
                let qualified = qualified_member(name, member);
                self.publish_member(name, member, value.clone());
                members.push((member.clone(), qualified, value));
            }
        }

        self.module_depth -= 1;
        self.pop_scope();
        self.importing.pop();

        if let Some(error) = failure {
            self.context.declared_modules.remove(name);
            return Err(error);
        }
        if self.loop_control.is_some() {
            // An abandoned body declares nothing: leaving the name registered
            // would answer a later `module` of the same name with `is already
            // declared` and a later `import` of it with a module that published
            // nothing. The bytecode VM drops it for the same reason on the way
            // out of an abrupt exit.
            self.context.declared_modules.remove(name);
            return Ok(Value::Nothing);
        }

        self.context
            .declared_modules
            .insert(name.to_string(), DeclaredModule { members });
        self.context
            .module_aliases
            .entry(name.to_string())
            .or_insert_with(|| name.to_string());
        Ok(Value::Nothing)
    }

    pub fn run(&mut self, program: &Program) -> Result<Value> {
        let result = self.run_statements(program)?;

        // Print collected output. Through the one boundary a program writes
        // through, so the WebAssembly playground can collect what the native
        // binary would have printed.
        for line in &self.output {
            crate::wasm::emit(&format!("{}\n", line));
        }

        Ok(result)
    }

    /// Runs `program` and answers with what it returned, printing nothing.
    ///
    /// [`Vm::run`] prints what the program *said*, which is right for a program
    /// and wrong for a question about a value: `.type f()` is not asking to hear
    /// what `f` prints, and a REPL that printed it would answer a question with
    /// output the user did not type. The lines are still buffered — nothing is
    /// lost, and a caller that wants them reads them with [`Vm::take_output`] —
    /// so this is about what reaches the terminal and not about the VM keeping
    /// a record.
    ///
    /// `print` and `console.log` do not go through this buffer: they write at
    /// the point they are reached, so a question has to keep them out of the
    /// program it evaluates rather than rely on there being nothing to print.
    pub fn run_quietly(&mut self, program: &Program) -> Result<Value> {
        self.run_statements(program)
    }

    /// The statements, without the printing [`Vm::run`] does afterwards.
    ///
    /// Split out so the two entry points cannot disagree about what a program
    /// *does* — a failure returns here in both, before any output is emitted,
    /// exactly as it did when [`Vm::run`] returned straight from the loop.
    fn run_statements(&mut self, program: &Program) -> Result<Value> {
        let mut result = Value::Nothing;

        for statement in &program.statements {
            result = self.execute_statement(statement)?;
        }

        Ok(result)
    }

    fn push_scope(&mut self) {
        self.locals.push(CapturedScope::new());
    }

    fn pop_scope(&mut self) {
        self.locals.pop();
    }

    /// Builds the function value for a declaration, closing over every local
    /// scope that is live where the declaration is executed.
    ///
    /// The capture is a copy taken at declaration time, so a variable the
    /// function reads is the value it had where the function was written, and
    /// a variable it assigns is assigned only for that one call. Globals are
    /// not captured and are read live, which is what a declaration that
    /// changes a global — a counter, a registry — is written to do.
    fn make_function(&self, name: &str, params: &[String], body: &[Stmt]) -> Value {
        let captured: Captured = self.locals.clone();
        Value::Function(FunctionValue {
            name: name.to_string(),
            params: params.to_vec(),
            body: FunctionBody::Statements(Arc::new(body.to_vec())),
            captured: Arc::new(captured),
        })
    }

    fn get_var(&self, name: &str) -> Option<Value> {
        self.get_var_ref(name).cloned()
    }

    /// The binding a read of `name` resolves to, mutably.
    ///
    /// This is the one door a value is changed *through* rather than copied
    /// into, and `append` is the only caller: handing back a `&mut` is what
    /// lets it grow a list in place, because a `Value` read out of a scope has
    /// one more handle on the storage than the scope itself and `make_mut`
    /// would copy for it. The name resolves exactly as [`Vm::get_var`] does,
    /// and nothing is ever created here — a name that is not bound stays not
    /// bound, because growing a name into existence would be a `set`.
    fn get_var_mut(&mut self, name: &str) -> Option<&mut Value> {
        for scope in self.locals.iter_mut().rev() {
            if scope.contains_key(name) {
                return scope.get_mut(name);
            }
        }
        self.globals.get_mut(name)
    }

    /// [`Vm::get_var`] without the copy, for a caller that is only going to look
    /// at what is bound — `:vars` and `:inspect` both are.
    fn get_var_ref(&self, name: &str) -> Option<&Value> {
        // Check local scopes first
        for scope in self.locals.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v);
            }
        }
        // Check globals
        self.globals.get(name)
    }

    /// Binds `name` to `value` as a constant, refusing a second declaration of
    /// a name that already holds one.
    ///
    /// The value goes into [`Vm::globals`] so that a read resolves through the
    /// one name-resolution path [`Vm::get_var`] already has, and the name goes
    /// into [`Vm::constants`] so that a later assignment is refused. A refused
    /// declaration leaves the first binding in place, so a module that
    /// declares the same name twice keeps the value it had.
    fn bind_constant(&mut self, name: &str, value: Value) -> Result<()> {
        if !self.constants.insert(name.to_string()) {
            return Err(Error::Runtime(
                format!("Constant '{name}' is already declared"),
                self.span(),
            ));
        }
        // A constant declared inside a call lands in the globals, exactly as a
        // `set` that reaches a global does — and, exactly as that `set` does,
        // it belongs to the frame that wrote it as far as `.locals` is
        // concerned. See [`DebugEvent::frame_writes`].
        if let Some(writes) = self.frame_writes.last_mut() {
            writes.insert(name.to_string());
        }
        self.globals.insert(name.to_string(), value);
        Ok(())
    }

    /// Binds `name` in the innermost live scope that already has it, and in a
    /// global when no local scope does.
    ///
    /// This resolves a name exactly as [`Vm::get_var`] does, so a read and a
    /// write of one name agree. It used to write only into the innermost scope
    /// and fall straight through to a global otherwise, which meant that
    /// assigning to a variable of an enclosing scope — a parameter from inside
    /// a loop body, or a captured name from inside a function that closed over
    /// it — silently created a global of that name instead.
    ///
    /// A write that would land on a global is refused when that global is a
    /// constant, which is what makes `constant` a constant. A name a live local
    /// scope already holds is that local's, so it is written there and a
    /// constant is shadowed rather than overwritten.
    /// The refusal every write onto a constant shares: `Cannot assign to
    /// constant 'NAME'`.
    ///
    /// Split out because there are two places a write can reach a program-level
    /// name — an assignment ([`Vm::set_var`]) and a module's own `set` run by
    /// [`Vm::load_module`] — and a name the program has bound as a constant is
    /// read-only whichever of them arrives.
    fn refuse_constant_rebind(&self, name: &str) -> Result<()> {
        if self.constants.contains(name) {
            return Err(Error::Runtime(
                format!("Cannot assign to constant '{name}'"),
                self.span(),
            ));
        }
        Ok(())
    }

    /// Binds a name an `import` contributes to the program, refusing to write
    /// onto a constant the program has already bound.
    ///
    /// The write lands in [`Vm::globals`] and not through [`Vm::set_var`],
    /// because a module's names are names of the whole program: the import may
    /// run inside a function body, and a local of the same name would otherwise
    /// capture the module's binding for the length of that scope.
    fn bind_module_name(&mut self, name: &str, value: Value) -> Result<()> {
        self.refuse_constant_rebind(name)?;
        self.globals.insert(name.to_string(), value);
        Ok(())
    }

    fn set_var(&mut self, name: &str, value: Value) -> Result<()> {
        for scope in self.locals.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return Ok(());
            }
        }
        if self.module_depth > 0 {
            // A `set` in a module body is the module's own name, not a global
            // of the program that declared it — the boundary `module ... end`
            // exists to draw. A name already held by an enclosing scope is
            // still that scope's, which the loop above has already answered.
            if let Some(scope) = self.locals.last_mut() {
                scope.insert(name.to_string(), value);
            }
            return Ok(());
        }
        self.refuse_constant_rebind(name)?;
        // A write that reaches a global from inside a call belongs to the frame
        // that made it as far as `.locals` is concerned — see
        // [`DebugEvent::frame_writes`]. At the top level there is no frame to
        // own it and nothing is recorded, which is why this is a `last` and not
        // an unconditional insert.
        if let Some(writes) = self.frame_writes.last_mut() {
            writes.insert(name.to_string());
        }
        self.globals.insert(name.to_string(), value);
        Ok(())
    }

    fn declare(&mut self, name: &str) {
        if let Some(scope) = self.locals.last_mut() {
            scope.insert(name.to_string(), Value::Nothing);
        }
    }

    fn execute_statement(&mut self, stmt: &Stmt) -> Result<Value> {
        let previous_span = std::mem::replace(&mut self.current_span, stmt.span);
        self.statement_depth += 1;
        self.debug_before(stmt.span.line);
        // Opened before the step charge and before the body, closed on every
        // path out, so a statement that fails still has its cost recorded.
        let profile = self.profile_enter(stmt.span.line);
        let result = self
            .charge_step()
            .and_then(|()| self.execute_statement_body(&stmt.statement));
        self.profile_exit(profile);
        // Restored on the failing path as well as the succeeding one: a
        // statement that raised an error still finished, and leaving the counter
        // raised would make every later statement look nested inside it.
        self.statement_depth -= 1;
        self.current_span = previous_span;
        result
    }

    /// Starts collecting per-source-line self time for the next run.
    ///
    /// Only the tree-walker is timed: it is the engine a REPL session runs, and
    /// `.profile` is a question about the session the user is sitting in. A
    /// block that binds or reads something changes the session exactly as it
    /// would without the instrument; the timer only reads the clock.
    pub fn start_line_profile(&mut self) {
        self.line_timer = Some(LineTimer::default());
    }

    /// Stops collecting and answers what was collected, by source line.
    pub fn take_line_profile(&mut self) -> Option<Vec<(usize, u128)>> {
        self.line_timer.take().map(|timer| timer.totals)
    }

    /// Opens a timing frame for `line`, if this statement belongs to the
    /// program being profiled.
    ///
    /// Statements inside a user function call are not timed: their source lines
    /// belong to the function's declaration, not to the block the user typed, so
    /// their time stays in the call-site line rather than being attributed to a
    /// line the block never showed. That is every statement at `call_depth > 0`,
    /// not only the body's direct statements: a `catch` or `finally` body that
    /// runs inside the call has declaration lines too, even when it is unwinding
    /// a loop, because saving and restoring `loop_depth` around the call does not
    /// change which frame the code belongs to. A `finally` that unwinds a loop at
    /// the top level is still at depth zero and is timed like any other statement.
    fn profile_enter(&mut self, line: usize) -> bool {
        if self.call_depth != 0 {
            return false;
        }
        let Some(timer) = self.line_timer.as_mut() else {
            return false;
        };
        timer.stack.push(ActiveLine {
            line,
            started: Instant::now(),
            child_nanos: 0,
        });
        true
    }

    /// Closes the frame opened by [`Vm::profile_enter`] and books its self time.
    fn profile_exit(&mut self, entered: bool) {
        if !entered {
            return;
        }
        let Some(timer) = self.line_timer.as_mut() else {
            return;
        };
        let Some(frame) = timer.stack.pop() else {
            return;
        };
        let elapsed = frame.started.elapsed().as_nanos();
        let own = elapsed.saturating_sub(frame.child_nanos);
        if let Some((_, recorded)) = timer
            .totals
            .iter_mut()
            .find(|(line, _)| *line == frame.line)
        {
            *recorded += own;
        } else {
            timer.totals.push((frame.line, own));
        }
        if let Some(parent) = timer.stack.last_mut() {
            parent.child_nanos += elapsed;
        }
    }

    /// Installs the debugger, or takes it away again with `None`.
    ///
    /// The hook is asked before every statement and may only answer what to do
    /// next. It cannot change what a statement evaluates to, cannot raise an
    /// error and cannot skip a step charge — a program run with a debugger
    /// attached is the same program, observed.
    pub fn set_debug_hook(&mut self, hook: Option<DebugHook>) {
        self.debug_hook = hook;
    }

    /// Gives the attached debugger the chance to stop before this statement on
    /// `line` runs.
    ///
    /// Called first in [`Vm::execute_statement`], before the step charge and
    /// before the statement body, so a breakpoint on a line stops *on* that
    /// line rather than after it. Every diagnostic a program can raise still
    /// comes from the statement itself: the hook has no way to answer with an
    /// error.
    fn debug_before(&mut self, line: usize) {
        if self.debug_hook.is_none() {
            return;
        }
        // Two fields of the same VM at once: the hook is taken mutably and the
        // scopes it reads immutably, which is fine because they are disjoint.
        let event = DebugEvent {
            line,
            depth: self.call_depth,
            statement_depth: self.statement_depth,
            scopes: &self.locals,
            globals: &self.globals,
            builtins: &self.builtin_names,
            module_members: &self.context.qualified_members,
            context: &self.context,
            call_stack: &self.call_stack,
            scope_base: self.frame_scope_base,
            frame_writes: self.frame_writes.last(),
        };
        if let Some(hook) = self.debug_hook.as_mut() {
            let _ = hook(event);
        }
    }

    /// The source position of the statement being executed. Every runtime
    /// failure is reported against it.
    fn span(&self) -> Span {
        self.current_span
    }

    fn execute_statement_body(&mut self, stmt: &Statement) -> Result<Value> {
        match stmt {
            Statement::Say(expr) => {
                let value = self.evaluate(expr)?;
                let line = value.to_string();
                self.charge_output(&line)?;
                self.output.push(line);
                Ok(Value::Nothing)
            }
            Statement::Print(expr) => {
                let value = self.evaluate(expr)?;
                // A question is not a request to hear anything, and `print` is
                // the one way out of the value system that does not go through a
                // builtin: it is a statement, so `[Vm::is_effectful`] cannot
                // name it, and it writes where the line is rather than into the
                // output buffer [`Vm::run_quietly`] declines to flush. It is
                // evaluated and dropped — so a `print` inside a `to` function a
                // question called is as silent as one at the top level, and a
                // program's own `print` is untouched.
                if !self.asking_a_question {
                    crate::wasm::emit(&value.to_string());
                }
                Ok(Value::Nothing)
            }
            Statement::Set { name, value } => {
                let val = self.evaluate(value)?;
                self.set_var(name, val)?;
                Ok(Value::Nothing)
            }
            Statement::Constant { name, value } => {
                let val = self.evaluate(value)?;
                self.bind_constant(name, val)?;
                Ok(Value::Nothing)
            }
            Statement::SetProperty {
                object,
                property,
                value,
            } => {
                let val = self.evaluate(value)?;
                if let Some(Value::Record(mut fields)) = self.get_var(object) {
                    std::sync::Arc::make_mut(&mut fields).insert(property.clone(), val);
                    self.set_var(object, Value::Record(fields))?;
                }
                Ok(Value::Nothing)
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond = self.evaluate(condition)?;
                // A `break` or a `skip` in either branch ends the branch as
                // well as the loop: the statements after it in the same branch
                // are not part of what the program meant to run. The signal is
                // left pending for the loop, which is what consumes it.
                if cond.is_truthy() {
                    self.run_block(then_branch)?;
                } else {
                    self.run_block(else_branch)?;
                }
                Ok(Value::Nothing)
            }
            Statement::Unless { condition, body } => {
                // The body is taken exactly when the condition is false, so a
                // true condition leaves the interpreter where it was.
                let cond = self.evaluate(condition)?;
                if !cond.is_truthy() {
                    // Through `run_block`, like the `if` branches above: an
                    // `unless` body is a block, so a `break` or a `skip` written
                    // in it ends the statements after it as well as the loop.
                    // Run as a plain statement list it would leave the rest of
                    // the body going while the signal was already raised, which
                    // is a leak the bytecode VM — whose `unless` body compiles
                    // into the enclosing block, so its `BREAK`/`SKIP` jumps
                    // over the rest — does not have.
                    self.run_block(body)?;
                }
                Ok(Value::Nothing)
            }
            Statement::ForEach {
                variable,
                iterable,
                body,
            } => {
                let iterable_value = self.evaluate(iterable)?;
                if let Value::List(items) = iterable_value {
                    let mut loop_iterations = 0;
                    for item in items.iter() {
                        self.charge_iteration(&mut loop_iterations, "for each")?;
                        if self.run_iteration(Some(variable), Some(item.clone()), body)?
                            == Some(LoopControl::Break)
                        {
                            break;
                        }
                    }
                }
                Ok(Value::Nothing)
            }
            Statement::ForRange {
                variable,
                start,
                end,
                step,
                body,
            } => {
                let start_val = self.evaluate(start)?;
                let end_val = self.evaluate(end)?;
                let step_val = match step {
                    Some(s) => self.evaluate(s)?,
                    None => Value::Number(1.0),
                };

                let span = self.span();
                let start = expect_range_number(&start_val, "from", span)?;
                let end = expect_range_number(&end_val, "to", span)?;
                let step = expect_range_number(&step_val, "by", span)?;

                let mut i = start;
                let mut loop_iterations = 0;
                // The step's sign is the direction of travel, so a negative
                // `by` counts down. A zero step never leaves the bound, and the
                // iteration guard is what stops it: a range is not special
                // enough to get its own escape.
                while range_has_next(i, end, step) {
                    self.charge_iteration(&mut loop_iterations, "for each from")?;
                    let value = Value::number(i, self.span())?;
                    // One turn through `run_iteration`, so a `break` or a
                    // `skip` written in the body of this range leaves it, and
                    // the loop's variable is bound and released on every path.
                    if self.run_iteration(Some(variable), Some(value), body)?
                        == Some(LoopControl::Break)
                    {
                        break;
                    }
                    // The stepped value is checked too: a step that
                    // overflows the counter is the same failure as one that
                    // would put an infinity in the loop variable.
                    i = finite_number(i + step, self.span())?;
                }
                Ok(Value::Nothing)
            }
            Statement::Repeat { count, body } => {
                let count_val = self.evaluate(count)?;
                // The turn count is the count truncated towards zero, and a
                // count that is not a number runs no times — see
                // `expect_repeat_count`, which the bytecode VM reads the same
                // count through.
                let turns = expect_repeat_count(&count_val, self.span())?;
                let mut loop_iterations = 0;
                for _ in 0..turns {
                    self.charge_iteration(&mut loop_iterations, "repeat")?;
                    if self.run_iteration(None, None, body)? == Some(LoopControl::Break) {
                        break;
                    }
                }
                Ok(Value::Nothing)
            }
            // `repeat ... until <condition>`: the body has its turn and the
            // condition is read afterwards, so the first turn is one whatever
            // the condition would have said before it ran. A turn is charged at
            // the top of the loop, before the body runs, which is where the
            // bytecode VM charges it too — the filler its `top` is.
            //
            // A `skip` starts the next turn without reading the condition,
            // because "skip" means the same thing in every loop: go on to the
            // top of this one. In a loop whose condition is at its top that
            // reads the condition; here it goes straight back to the body.
            Statement::RepeatUntil {
                body,
                condition,
                condition_span,
            } => {
                let mut loop_iterations = 0;
                loop {
                    self.charge_iteration(&mut loop_iterations, "repeat")?;
                    match self.run_iteration(None, None, body)? {
                        Some(LoopControl::Break) => break,
                        Some(LoopControl::Skip) => continue,
                        None => {}
                    }
                    if self.evaluate_at(condition, *condition_span)?.is_truthy() {
                        break;
                    }
                }
                Ok(Value::Nothing)
            }
            Statement::While { condition, body } => {
                let mut loop_iterations = 0;
                while self.evaluate(condition)?.is_truthy() {
                    self.charge_iteration(&mut loop_iterations, "while")?;
                    if self.run_iteration(None, None, body)? == Some(LoopControl::Break) {
                        break;
                    }
                }
                Ok(Value::Nothing)
            }
            Statement::Break => {
                self.break_statements += 1;
                self.raise_loop_control("break", LoopControl::Break)?;
                Ok(Value::Nothing)
            }
            Statement::Skip => {
                self.skip_statements += 1;
                self.raise_loop_control("skip", LoopControl::Skip)?;
                Ok(Value::Nothing)
            }
            Statement::Return(expr) | Statement::GiveBack(expr) => match expr {
                Some(e) => self.evaluate(e),
                None => Ok(Value::Nothing),
            },
            Statement::Function { name, params, body } => {
                let function = self.make_function(name, params, body);
                self.declare(name);
                self.set_var(name, function)?;

                Ok(Value::Nothing)
            }
            Statement::Method { name, params, body } => {
                let method = self.make_function(name, params, body);
                self.set_var(name, method)?;
                Ok(Value::Nothing)
            }
            Statement::Has { name, .. } => Err(Error::Runtime(
                format!("'has {name}' is only valid inside an object declaration"),
                self.span(),
            )),
            Statement::Object {
                name,
                extends,
                body,
            } => self.declare_object(name, extends.as_ref(), body),
            Statement::Try {
                body,
                catch_var,
                catch_kind,
                catch_body,
                finally_body,
            } => {
                // All three of the protected code, the catch and the finally run
                // through `run_block`, so a `break` or a `skip` written in any of
                // them stops the rest of that block. An abrupt exit from the
                // protected region is not a failure: the catch does not run, the
                // finally does, and the signal stays pending for the loop.
                let failure = self.run_block(body).err();

                // A catch is PRESENT whenever the parser saw one, which is not the
                // same as catch_var being Some: `catch` with no name binding parses
                // to catch_var == None with a non-empty body. Testing the Option
                // therefore (a) never ran a bare catch and (b) fell through to
                // `Ok(Value::Nothing)` below, silently swallowing the error. A
                // swallowed error is the interpreter lying about what happened.
                let has_catch = catch_var.is_some() || !catch_body.is_empty();

                if let Some(failure) = failure {
                    // A typed handler is a handler only for the kinds it names.
                    // One that does not match is not a handler at all, and the
                    // failure it declined belongs to whatever is written around
                    // it — which is the same road a `try` with no `catch` hands
                    // it down, and the same road both engines walk it.
                    let declined = catch_kind.is_some_and(|kind| !kind.matches(&failure));

                    // No catch to handle it, or one that declined it. The
                    // `finally` is owed anyway: a
                    // `try ... finally` promises its cleanup whatever the region
                    // does, and returning above `run_finally_body` skipped it on
                    // exactly the path where the cleanup is wanted — the region
                    // failing is what left the thing to clean up. The failure is
                    // propagated *after* the cleanup rather than instead of it,
                    // which is also what an enclosing `try` written around this
                    // one expects: `?` hands the failure to it, and it is the
                    // `catch` there that decides what the program does next.
                    if !has_catch || declined {
                        self.run_finally_body(finally_body)?;
                        // A cleanup that left the region abruptly is the way the
                        // region was left, so the failure is not the one leaving
                        // it and there is nothing for an enclosing `catch` to
                        // handle — propagating it would run that handler *after*
                        // being told to go, which is what the bytecode VM does
                        // not do and what the two engines disagreed about. This
                        // is the replacement a failing cleanup already makes
                        // ("a failing finally still leaves the outer try to catch
                        // the failure", `tests/test_control_flow.rb`), with an
                        // abrupt exit in place of a failure: the signal is
                        // pending exactly when the cleanup raised one, because a
                        // body that raised one instead of failing never got here.
                        if self.loop_control.is_none() {
                            return Err(failure);
                        }
                        return Ok(Value::Nothing);
                    }
                    self.run_catch_body(*catch_kind, catch_var.as_deref(), &failure, catch_body)?;
                }

                self.run_finally_body(finally_body)?;

                Ok(Value::Nothing)
            }
            Statement::Import(items) => {
                for item in items {
                    if self.importing.iter().any(|name| name == &item.name) {
                        // A module reaching itself is a cycle, and saying so is
                        // the difference between a clean error and a walk that
                        // never ends. The scan is over the chain of imports
                        // currently being loaded, which is as deep as the
                        // chain of module files — a bounded walk, not a walk
                        // that grows with the program.
                        return Err(Error::Runtime(
                            format!("Circular import of module '{}'", item.name),
                            self.span(),
                        ));
                    }

                    // The manifest is resolved once for the whole run — see
                    // `Vm::pins` — and every import here is answered from that
                    // one resolution. An `Err` is a manifest this program cannot
                    // be resolved from, and is passed on rather than ignored,
                    // because the answer to "that module cannot be resolved"
                    // must not be an import of whichever file happens to be at
                    // `modules/<name>.rb`.
                    let pinned = self.pins.module(&item.name)?;
                    if let Some(path) = module_file_in(&self.root, pinned.as_deref(), &item.name) {
                        // The path is there but may not be readable: that is an
                        // Io error, not a miss, so it is reported rather than
                        // folded into the "cannot find" below.
                        self.load_module(&path, &item.name)?;
                    } else if !self.context.declared_modules.contains_key(&item.name)
                        && !stdlib::is_module(&item.name)
                    {
                        return Err(Error::Runtime(
                            format!("Cannot find module '{}'", item.name),
                            self.span(),
                        ));
                    }

                    let target_name = item.alias.clone().unwrap_or_else(|| item.name.clone());
                    // The alias is a name for the module, so a call through it
                    // reaches the module it names.
                    self.context
                        .module_aliases
                        .insert(target_name.clone(), item.name.clone());
                    // The module's own name names it too — `import json as J`
                    // leaves `json.parse` working — unless the program has bound
                    // that name already, which is the shadowing rule every
                    // declaration in the language follows.
                    self.context
                        .module_aliases
                        .entry(item.name.clone())
                        .or_insert_with(|| item.name.clone());

                    // A module this program declared has no file to read, so
                    // its published members are copied to the alias's own
                    // `alias_member` names here rather than by the loader.
                    if let Some(declared) = self.context.declared_modules.get(&item.name) {
                        let members: Vec<(String, String, Value)> = declared.members.clone();
                        for (member, _, value) in members {
                            self.publish_member(&target_name, &member, value);
                        }
                    }

                    // The names the import itself binds go through the same
                    // refusal as the names the module contributes: they are
                    // program-level names like they are. A name the program
                    // already holds is that program's, and an import does not
                    // take it over.
                    if self.get_var(&item.name).is_none() {
                        self.bind_module_name(&item.name, Value::Nothing)?;
                    }
                    if self.get_var(&target_name).is_none() {
                        self.bind_module_name(&target_name, Value::Nothing)?;
                    }
                }
                Ok(Value::Nothing)
            }
            Statement::Module { name, body } => self.declare_module(name, body),
            // An `export` outside a module declaration publishes nothing: there
            // is no module for it to publish into.
            Statement::Export { .. } => Ok(Value::Nothing),
            Statement::Test { name: _, body } => {
                // Execute test body
                self.run_block(body)?;
                Ok(Value::Nothing)
            }
            Statement::Expr(expr) => self.evaluate(expr),
        }
    }

    /// The `catch` body of a `try`, in a scope of its own, with `catch_var`
    /// bound to the failure if there is one to bind.
    ///
    /// An untyped handler binds the word `error` — the text it has always bound,
    /// so `say error` in a handler written before typed failures still prints
    /// what it printed. A typed handler binds the failure itself: the record of
    /// its kind, its message and where it happened, which is the value there is
    /// something to read the kind off.
    ///
    /// The scope is truncated rather than popped on the way out, so a `catch`
    /// that fails part way through — or that a `break` leaves part way through
    /// — does not leave the name it binds alive in the enclosing block.
    fn run_catch_body(
        &mut self,
        catch_kind: Option<FailureKind>,
        catch_var: Option<&str>,
        failure: &Error,
        catch_body: &[Stmt],
    ) -> Result<()> {
        let bound = match catch_kind {
            Some(_) => failure_value(failure),
            None => Value::Text("error".to_string()),
        };
        let scope_base = self.locals.len();
        self.push_scope();
        let outcome = match catch_var {
            Some(var) => {
                self.declare(var);
                self.set_var(var, bound)
            }
            None => Ok(()),
        }
        .and_then(|()| self.run_block(catch_body));
        self.locals.truncate(scope_base);
        outcome?;
        Ok(())
    }

    /// The `finally` body of a `try`, on the way out of its region however the
    /// region was left — by its own end, by a failure its `catch` handled, or by
    /// a `break` or a `skip` passing through it.
    ///
    /// A pending signal is *held* across the body rather than left in place while
    /// it runs. Two things depend on that. The cleanup is owed, so every
    /// statement of it runs: a field left pending would stop the block after its
    /// first statement, which is a `finally` that cleans up one thing and not the
    /// other. And the `finally` is a block in its own right, so a `break` written
    /// in it ends *it* — the statements after it in the same block — and names
    /// the loop the signal already named, which is then still that loop's to
    /// consume.
    fn run_finally_body(&mut self, finally_body: &[Stmt]) -> Result<()> {
        let pending = self.loop_control.take();
        let outcome = self.run_block(finally_body);
        // A signal the `finally` raised replaces the one it was holding rather
        // than joining it: there is one loop to act on it, and the nearest
        // statement to it asked for that.
        self.loop_control = self.loop_control.or(pending);
        outcome?;
        Ok(())
    }

    /// Runs `statements` and answers what the last of them produced.
    ///
    /// Stops at the first `break` or a `skip` one of them raises and leaves the
    /// signal pending, because the block is not the thing that consumes it — the
    /// loop is. The signal is reported through [`Vm::loop_control`] for the
    /// caller to hand over, rather than returned here, so that every block in
    /// the language stops at one and none of them can swallow it.
    fn execute_statements(&mut self, statements: &[Stmt]) -> Result<Value> {
        let mut result = Value::Nothing;
        for stmt in statements {
            result = self.execute_statement(stmt)?;
            if self.loop_control.is_some() {
                return Ok(result);
            }
        }
        Ok(result)
    }

    fn evaluate(&mut self, expr: &Expr) -> Result<Value> {
        match expr {
            Expr::Number(n) => Value::number(*n, self.span()),
            Expr::Text(s) => Ok(Value::Text(s.clone())),
            Expr::YesNo(b) => Ok(Value::YesNo(*b)),
            Expr::Nothing => Ok(Value::Nothing),
            Expr::Variable(name, _) => self
                .get_var(name)
                .ok_or_else(|| Error::Runtime(format!("Unknown variable '{}'", name), self.span())),
            Expr::Binary { op, left, right } => {
                let l = self.evaluate(left)?;
                let r = self.evaluate(right)?;
                self.binary_op(op, l, r)
            }
            Expr::Unary { op, expr } => {
                let v = self.evaluate(expr)?;
                self.unary_op(op, v)
            }
            Expr::Call { name, args } => {
                let arg_values: Result<Vec<Value>> =
                    args.iter().map(|a| self.evaluate(a)).collect();
                self.call(name, &arg_values?)
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
            } => self.call_method(receiver, method, args),
            Expr::Property { object, property } => {
                let obj = self.evaluate(object)?;
                runtime::property(self.span(), obj, property)
            }
            Expr::Index { object, index } => {
                let obj = self.evaluate(object)?;
                let idx = self.evaluate(index)?;
                runtime::index(self.span(), obj, idx)
            }
            Expr::InterpolatedText(parts) => {
                let mut result = String::new();
                for part in parts {
                    result.push_str(&self.evaluate(part)?.to_string());
                }
                Ok(Value::Text(result))
            }
            Expr::List(items) => {
                let values: Result<Vec<Value>> = items.iter().map(|i| self.evaluate(i)).collect();
                Ok(Value::list(values?))
            }
            Expr::Record(fields) => {
                let mut record = Fields::new();
                for (key, value) in fields {
                    record.insert(key.clone(), self.evaluate(value)?);
                }
                Ok(Value::record(record))
            }
            // The literal captures exactly where it is written, the same way a
            // named declaration does: `make_function` copies the live scopes,
            // so a literal passed to `map` sees its own environment rather than
            // whatever the caller binds.
            Expr::FunctionLiteral { params, body } => {
                Ok(self.make_function(ANONYMOUS_FUNCTION, params, body))
            }
            // The guarded call's own failure is discarded and becomes `nothing`.
            //
            // Three things are not "its own failure" and are re-raised rather than
            // discarded. A `break` or a `skip` inside the call is not a failure at
            // all, so a guarded call that leaves the loop still leaves the loop.
            // A failed `expect` is a test result rather than an error, and the
            // harness reads it off this VM rather than off the returned error — so
            // discarding it here would report a red test green, which is the one
            // thing a guard must never do. A resource limit is the host refusing
            // to keep going rather than this call going wrong: a recursion stopped
            // at the call-depth limit has not "carried on", and yielding `nothing`
            // for it would report the limit as a successful value and leave the
            // program running against a bound it has already reached.
            Expr::MightFail(inner) => {
                let assertions_before = self.assertions_failed;
                match self.evaluate(inner) {
                    Ok(value) => Ok(value),
                    Err(failure) => {
                        // A count rather than a yes/no, because once one failed
                        // `expect` is recorded the yes/no form cannot tell a
                        // later failure that is a test result from one that is
                        // not: a second failing `expect` inside a guard written
                        // after the first would be discarded as "already
                        // asserted", reporting a red test green. Only a count
                        // that has gone up names a result this expression
                        // recorded.
                        let asserted_now = self.assertions_failed > assertions_before;
                        if self.loop_control.is_some()
                            || asserted_now
                            || failure.is_resource_limit()
                        {
                            return Err(failure);
                        }
                        Ok(Value::Nothing)
                    }
                }
            }
            Expr::Expect { actual, expected } => {
                let a = self.evaluate(actual)?;
                let e = self.evaluate(expected)?;
                match crate::testing::assertions::assert_values_equal(&e, &a) {
                    Ok(()) => Ok(Value::Nothing),
                    Err(failure) => {
                        self.expectation_failure = Some(failure.clone());
                        self.assertions_failed += 1;
                        Err(Error::Runtime(failure.to_string(), self.span()))
                    }
                }
            }
        }
    }

    /// Evaluates `expr`, reporting any failure in it against `span` rather than
    /// against the statement being executed.
    ///
    /// Nearly every expression is written on the line of the statement that holds
    /// it, so [`Vm::evaluate`] answers for all of them. A `repeat ... until` is
    /// the one place it does not: its condition is on the `until` line, at the
    /// bottom of the loop, so a failure there belongs to the line its author wrote
    /// it on rather than to the `repeat` that opens the loop.
    fn evaluate_at(&mut self, expr: &Expr, span: Span) -> Result<Value> {
        let previous_span = std::mem::replace(&mut self.current_span, span);
        let result = self.evaluate(expr);
        self.current_span = previous_span;
        result
    }

    fn binary_op(&self, op: &BinaryOp, left: Value, right: Value) -> Result<Value> {
        runtime::binary_op(self.span(), op, left, right)
    }

    fn unary_op(&self, op: &UnaryOp, value: Value) -> Result<Value> {
        runtime::unary_op(self.span(), op, value)
    }

    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value> {
        // The one place a builtin is reached, so it is the one place a refusal
        // has to be. See `Vm::refuse_effectful_builtins`: a question must not
        // change the world, and this fires before the effect rather than after
        // it. A name that is not effectful costs one table lookup and is the
        // overwhelmingly common case.
        if self.asking_a_question && stdlib::is_effectful(name) {
            return Err(Error::Runtime(
                format!(
                    "{} does something outside the value it answers, and this is a question \
                     about a value rather than a line to run",
                    name
                ),
                self.span(),
            ));
        }
        // `append` writes through a binding, so it needs the frame the free
        // `runtime::builtin` does not have; every other builtin is asked first.
        if name == "append" {
            let span = self.span();
            let target = runtime::append_target(span, args)?;
            runtime::append_through(span, target, self.get_var_mut(target), args[1].clone())?;
            return Ok(Value::Nothing);
        }
        // `stdlib::builtin` is the one place a builtin name resolves, so a name
        // answered here cannot be missing from the bytecode VM. `runtime::builtin`
        // alone answers only the names that need a `Result`; the rest are
        // `stdlib::builtin_function`'s, and until this call existed they were
        // registered in the global table and answered by nothing.
        // `map` is excluded here and answered by this walker's `map_builtin` below: it
        // is registered as a builtin name but has to *call* a Redblue function,
        // which the shared resolver cannot do. Asking it first would turn a
        // working higher-order call into a refusal.
        if !matches!(name, "map" | "list_map") {
            if let Some(value) = stdlib::builtin(self.span(), stdlib::resolve(name), args)? {
                return Ok(value);
            }
        }

        // `map` is registered as a builtin name but has to *call* a Redblue function,
        // which the free `runtime::builtin` cannot do, so it is answered by this
        // walker's `map_builtin` below. It must be asked for before the shared
        // resolver, which would otherwise claim the name and refuse it as one it
        // cannot use — turning a working higher-order call into an error.
        if matches!(name, "map" | "list_map") {
            return self.map_builtin(args);
        }
        match self.get_var(name) {
            Some(Value::Function(function)) => self.call_user_function(name, &function, args),
            _ => Err(Error::Runtime(
                format!("Unknown function '{}'", name),
                self.span(),
            )),
        }
    }

    /// `map(list, function)` — the higher-order builtin, in the three spellings
    /// `SPEC.md` uses: `map(xs, f)`, `list.map(xs, f)` and `[1, 2, 3].map(f)`.
    /// The fourth, `xs.map(f)`, takes the list from the name rather than as an
    /// argument and is answered by [`Vm::call_method`], which is where the
    /// receiver is in hand.
    ///
    /// It lives here rather than in `runtime::builtin` because applying a
    /// Redblue function needs this walker's captured scopes and its call-depth
    /// budget. The function runs once per element in order and its result
    /// becomes the mapped element; a failure inside it fails the whole call,
    /// with the depth released as `call_user_function` releases it.
    ///
    /// The list is the receiver and is matched here; the function is the
    /// argument and is left to [`stdlib::map_function`], which is also what the
    /// method spelling asks. One operation has one wording: `map(xs, 5)` and
    /// `xs.map(5)` both name the `5`, and `map(xs, f, extra)` says how many
    /// arguments it was given instead of only that the shape was wrong. Only a
    /// first argument that is not a list is refused here, because that is the
    /// receiver and no argument check could describe it.
    fn map_builtin(&mut self, args: &[Value]) -> Result<Value> {
        let [Value::List(items), rest @ ..] = args else {
            return Err(Error::Runtime(
                "map requires a list and a function".to_string(),
                self.span(),
            ));
        };

        let function =
            stdlib::map_function(rest).map_err(|message| Error::Runtime(message, self.span()))?;

        self.apply_map(items, function)
    }

    /// Applies `function` to every element of `items`, in order.
    ///
    /// The call is reported under `function.name` rather than under a fixed
    /// "anonymous function", so a limit reached inside a `map` of a *named* `to`
    /// function names that function — which is what the bytecode VM says, and
    /// what a reader needs to be told. A function *literal* carries
    /// [`ANONYMOUS_FUNCTION`] as its name already, so it says the same thing
    /// either way.
    fn apply_map(&mut self, items: &[Value], function: &FunctionValue) -> Result<Value> {
        let mut mapped = Vec::with_capacity(items.len());
        for item in items {
            mapped.push(self.call_user_function(
                &function.name,
                function,
                std::slice::from_ref(item),
            )?);
        }

        Ok(Value::list(mapped))
    }

    /// Runs the body of a user function `name` in its own scope and returns the
    /// value of its last statement.
    ///
    /// The call depth is checked first and released again on every exit path,
    /// including an error, so a caught failure leaves the counter balanced.
    /// Resolves `receiver.method(args)`.
    ///
    /// A receiver that names a declared `object` is a method call on that
    /// type: the method is the one the nearest declaration of that name
    /// provides, and `this` is bound to the receiver for the length of the
    /// call. A receiver that names no object is a module function, spelled
    /// `module_function` — which is what `files.read` and `time.now` are.
    /// A receiver that reaches no module function but holds a **list** is the
    /// method spelling of the higher-order builtin, which is what `xs.map(f)`
    /// is and what SPEC.md's "First-Class Functions" section introduces
    /// function values with. Anything else is an error rather than a guess.
    fn call_method(&mut self, receiver: &Expr, method: &str, args: &[Expr]) -> Result<Value> {
        let arg_values: Result<Vec<Value>> = args.iter().map(|a| self.evaluate(a)).collect();
        let args = arg_values?;

        let name = match receiver {
            Expr::Variable(name, _) => name.clone(),
            _ => {
                let this = self.evaluate(receiver)?;
                // `xs.map(f)` is the method spelling of the higher-order builtin,
                // and a list is the one receiver that is not an object and still
                // has a method. Every other receiver is still the error below.
                if method == "map" {
                    if let Value::List(items) = this {
                        let function = stdlib::map_function(&args)
                            .map_err(|message| Error::Runtime(message, self.span()))?;
                        return self.apply_map(&items, function);
                    }
                }
                return Err(Error::Runtime(
                    format!(
                        "Cannot call method '{}' on {}, which is not an object",
                        method, this
                    ),
                    self.span(),
                ));
            }
        };

        let object = match self.context.objects.get(&name) {
            Some(object) => object.clone(),
            None => {
                // A module function: the receiver names a module, so the
                // function is `module_function`. An alias is resolved to the
                // module it names first, so `import json as J` then
                // `J.parse(..)` reaches the same function `json.parse(..)`
                // does.
                let module = self
                    .context
                    .module_aliases
                    .get(&name)
                    .cloned()
                    .unwrap_or_else(|| name.clone());
                let qualified = qualified_member(&module, method);
                // The whole rule for a receiver that is not an object lives in
                // `stdlib::dotted_call_reaches`, which the bytecode VM asks
                // through the same function; see
                // [`crate::bytecode::vm::BytecodeVm::call_method`].
                //
                // Three things it decides, and each of them was a way for a
                // dotted call to answer something the language does not have:
                //
                // - the receiver has to name a module. It used not to: any
                //   `<receiver>_<member>` in the registry was enough to make the
                //   call, so every qualified builtin was a namespace and
                //   `random.seed(1)` answered through `random_seed`.
                // - a module the program declared or imported owns the name, so
                //   `stdlib::MODULE_MEMBERS` does not decide it. That table is
                //   the agreed surface of the *stdlib's* modules and says
                //   nothing about the members a program publishes.
                // - a stdlib module answers only for a pair the table holds, so
                //   `files_read` being registered is not by itself a reason
                //   `files.read` answers.
                if !stdlib::dotted_call_reaches(
                    &module,
                    method,
                    self.declares_module(&module),
                    self.get_var(&qualified).is_some(),
                    stdlib::module_member_name(&qualified).is_some(),
                ) {
                    // Nothing the module path can reach answers this, and `map`
                    // is the one member that is also a method on a list, so a
                    // name holding a list is asked here rather than refused.
                    // `xs.map(f)` is the program SPEC.md's "First-Class
                    // Functions" section uses to introduce function values, and
                    // before this it died as `xs_map`, a name nothing
                    // registers.
                    //
                    // It is asked *after* the module path, never before it, so a
                    // module function of the same spelling still wins: `list`
                    // bound to a list and called as `list.map(f)` is `list_map`
                    // with one argument, and stays the refusal that says so.
                    if method == "map" {
                        if let Some(Value::List(items)) = self.get_var(&name) {
                            let function = stdlib::map_function(&args)
                                .map_err(|message| Error::Runtime(message, self.span()))?;
                            return self.apply_map(&items, function);
                        }
                    }
                    return Err(Error::Runtime(
                        format!("Module '{module}' has no function '{method}'"),
                        self.span(),
                    ));
                }
                return self.call(&qualified, &args);
            }
        };

        let function = match object.methods.get(method) {
            Some(Value::Function(function)) => function.clone(),
            Some(_) | None => {
                return Err(Error::Runtime(
                    format!("Object '{name}' has no method '{method}'"),
                    self.span(),
                ))
            }
        };

        let this = self.evaluate(receiver)?;
        self.push_scope();
        self.declare("this");
        self.set_var("this", this)?;
        let result = self.call_user_function(method, &function, &args);
        self.pop_scope();
        result
    }

    /// Registers an `object Name [extends Parent]` declaration and binds `Name`
    /// to a record of its resolved fields.
    ///
    /// Lookup order, and the whole of it: the declaration's own `has` fields
    /// and `to can` methods first, then the nearest parent's, then the
    /// grandparent's, and so on. The first declaration of a name in that walk
    /// wins and the further ones are not copied in, so a child field shadows
    /// its parent's field rather than merging with it.
    ///
    /// The chain is walked once, here, with every name it visits collected, so
    /// `object A extends A` and a two-object cycle are reported instead of
    /// walked again. A parent that is not declared is the same error.
    fn declare_object(
        &mut self,
        name: &str,
        extends: Option<&String>,
        body: &[Stmt],
    ) -> Result<Value> {
        if self.context.objects.contains_key(name) {
            return Err(Error::Runtime(
                format!("Object '{name}' is already declared"),
                self.span(),
            ));
        }

        let mut fields = Fields::new();
        let mut methods = Fields::new();
        let mut rest = Vec::new();
        for stmt in body {
            match &stmt.statement {
                Statement::Has {
                    name: field,
                    default,
                } => {
                    let value = match default {
                        Some(expr) => self.evaluate(expr)?,
                        None => Value::Nothing,
                    };
                    fields.insert(field.clone(), value);
                }
                Statement::Method {
                    name: method,
                    params,
                    body,
                } => {
                    let function = self.make_function(method, params, body);
                    methods.insert(method.clone(), function);
                }
                _ => rest.push(stmt.clone()),
            }
        }

        // Nearest parent first, so the child's own entries stay ahead of every
        // inherited one and `entry` keeps them there.
        let mut chain = Vec::new();
        let mut seen = vec![name.to_string()];
        let mut parent = extends.cloned();
        while let Some(current) = parent {
            if seen.contains(&current) {
                return Err(Error::Runtime(
                    format!(
                        "Object '{name}' extends '{current}', which is already in its own parent chain"
                    ),
                    self.span(),
                ));
            }
            let Some(object) = self.context.objects.get(&current).cloned() else {
                return Err(Error::Runtime(
                    format!("Object '{name}' extends '{current}', which is not declared"),
                    self.span(),
                ));
            };
            seen.push(current);
            parent = object.parent.clone();
            chain.push(object);
        }

        for object in &chain {
            for (field, value) in &object.fields {
                fields.entry(field.clone()).or_insert_with(|| value.clone());
            }
            for (method, function) in &object.methods {
                methods
                    .entry(method.clone())
                    .or_insert_with(|| function.clone());
            }
        }

        self.context.objects.insert(
            name.to_string(),
            ObjectType {
                parent: extends.cloned(),
                fields: fields.clone(),
                methods,
            },
        );
        let record = Value::record(fields);
        self.declare(name);
        self.set_var(name, record)?;

        // A body that is not a field or a method declaration runs in the
        // enclosing scope, in order, and after the type is registered, so a
        // nested `object` in the body may extend this one.
        self.run_block(&rest)?;

        Ok(Value::Nothing)
    }

    fn call_user_function(
        &mut self,
        name: &str,
        function: &FunctionValue,
        args: &[Value],
    ) -> Result<Value> {
        if self.call_depth >= self.max_call_depth {
            return Err(Error::Limit(
                format!(
                    "Maximum call depth of {} reached while calling '{}'",
                    self.max_call_depth, name
                ),
                self.span(),
            ));
        }

        // The captured scopes go on the stack *above* the caller's frames, and
        // the parameters above those, so the body reads its own environment
        // rather than a same-named binding of whoever called it, and a
        // parameter shadows what it captured.
        let caller_depth = self.locals.len();
        for scope in function.captured.iter() {
            self.locals.push(scope.clone());
        }
        // Where this frame's own scopes begin: above the captured ones it
        // dragged in, so `.locals` inside the frame lists the frame's bindings
        // and not its caller's world. Saved and restored with the frame.
        let caller_scope_base = self.frame_scope_base;
        self.frame_scope_base = self.locals.len();
        self.push_scope();
        for (index, param) in function.params.iter().enumerate() {
            let value = args.get(index).cloned().unwrap_or(Value::Nothing);
            self.declare(param);
            self.set_var(param, value)?;
        }
        // The frame is on the stack while its body runs and only then: every
        // statement the body evaluates sees it, and the caller's statements
        // before the call and after the return do not. Both lists pop on every
        // path out of the call, including the failing one.
        self.call_stack.push(name.to_string());
        self.frame_writes.push(HashSet::new());
        self.call_depth += 1;
        // A function body is not lexically inside the caller's loop, so a
        // `break` written there has no loop of its own and must be refused
        // rather than reaching out and ending the caller's iteration. The
        // depth is restored on the way out, including the failing path.
        //
        // The pending signal is saved with it: a call made while a `break` or a
        // `skip` is raised — from a `finally` running on the way out of a loop —
        // is still inside the region that raised it, so the signal belongs to
        // the loop's caller and the call must not touch it.
        let caller_loop_depth = std::mem::replace(&mut self.loop_depth, 0);
        let caller_loop_control = self.loop_control.take();
        // A function value the bytecode VM built names a compiled block instead
        // of statements, and cannot be run by this walker. `rb run` only ever
        // builds statements itself, so reaching this is a caller mixing the two
        // VMs rather than a program that did something.
        let FunctionBody::Statements(body) = &function.body else {
            self.call_depth -= 1;
            self.loop_depth = caller_loop_depth;
            self.loop_control = caller_loop_control;
            self.locals.truncate(caller_depth);
            self.frame_scope_base = caller_scope_base;
            self.frame_writes.pop();
            self.call_stack.pop();
            return Err(Error::Runtime(
                format!(
                    "'{}' has a compiled body, which only `rb vm` can run",
                    function.name
                ),
                self.span(),
            ));
        };
        let result = self.execute_statements(body);
        self.call_depth -= 1;
        self.loop_depth = caller_loop_depth;
        self.loop_control = caller_loop_control;
        // Truncated rather than popped one at a time, so the stack is balanced
        // even when the body failed part way through and left a scope behind.
        self.locals.truncate(caller_depth);
        self.frame_scope_base = caller_scope_base;
        self.frame_writes.pop();
        self.call_stack.pop();

        result
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::HashMap;

    /// The frame base is an index into the locals stack, and it says where the
    /// frame's own scopes begin inside it.
    ///
    /// `observe_bindings` replaces that stack wholesale - it hands the VM the
    /// bindings of an observed step, and the step's scopes are not the scopes
    /// the VM was holding. A base left over from before the replacement is an
    /// index into a stack that no longer exists, so it points at the wrong
    /// scopes, and `DebugEvent::scope_base` would hand the watcher the
    /// caller's bindings as the frame's own.
    ///
    /// The base is only non-zero while a call is open, so the test makes one:
    /// a function that calls another puts a frame on the stack, and the base
    /// moves past the scopes the call captured.
    #[test]
    fn edge_observing_bindings_restarts_the_frame_base() {
        let mut vm = Vm::new();

        // The base is only non-zero while a call is open, and no call is open
        // here, so it is put where a live frame would have put it: the point is
        // that `observe_bindings` restarts it whatever it was.
        assert_eq!(
            vm.frame_scope_base, 0,
            "a VM with no call open has its frame base at the top of the stack"
        );
        vm.frame_scope_base = 3;

        // The bindings of an observed step replace the stack. The base must be
        // restarted with them, not carried over from the stack they replaced.
        vm.observe_bindings(&[], &HashMap::new());

        assert_eq!(
            vm.frame_scope_base, 0,
            "the frame base is an index into the locals stack, and \
             observe_bindings replaced that stack, so the base has to be \
             restarted with it"
        );
    }

    /// A user function's body is not part of the block being profiled, so its
    /// time belongs to the line that called it. The body's own lines are never
    /// timed, even though they run inside the call-site statement.
    #[test]
    fn profile_keeps_a_user_function_body_at_its_call_site() {
        let source = "\
to add_up(n)
    set total to 0
    for each i from 1 to n
        set total to total + i
    end
    give back total
end
set answer to add_up(20000)
";
        let program = crate::lexer::Lexer::tokenize(source)
            .and_then(|tokens| crate::parser::Parser::new(tokens).parse())
            .expect("the profiling fixture is a valid program");

        let mut vm = Vm::new();
        vm.start_line_profile();
        vm.run(&program).expect("the fixture runs");

        let timings = vm
            .take_line_profile()
            .expect("a profile was started and not yet taken");
        assert!(
            timings.iter().any(|(line, _)| *line == 8),
            "the call-site line is timed: {:?}",
            timings
        );
        for body_line in 2..=6 {
            assert!(
                !timings.iter().any(|(line, _)| *line == body_line),
                "line {body_line} is inside the function body and must stay \
                 attributed to the call site, not itself: {:?}",
                timings
            );
        }
    }
}
