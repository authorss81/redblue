//! The tree-walking interpreter: what `rb run` executes.
//!
//! It is the specification of Redblue's behaviour. The bytecode VM in
//! [`crate::bytecode::vm`] has to answer the same way, so the operations both
//! need — arithmetic, indexing, the builtin library — live in
//! [`crate::runtime`] and are read from here rather than written twice.

use crate::error::{Error, Result, Span};
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

// The HTTP timeouts belong to the builtin library, which is `runtime`'s; they
// are part of this module's public surface because they were declared here
// before the move.
pub use crate::runtime::{NETWORK_CONNECT_TIMEOUT_SECS, NETWORK_TIMEOUT_SECS};

/// The default number of user function calls that may be active at once.
/// Exceeding it is a `RuntimeError`, not a Rust stack overflow.
pub const MAX_CALL_DEPTH: usize = 1000;

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
pub const MAX_STEPS: usize = 10_000_000;

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

/// The stack one active call frame is allowed. A frame costs roughly 58 KiB in an
/// unoptimised build — six nested Rust frames per Redblue call — so
/// [`MAX_CALL_DEPTH`] needs about 60 MiB, well past the 16 MiB a main thread
/// gets by default. A limit the process cannot physically reach would turn a
/// clean `RuntimeError` back into the abort this counter exists to prevent, so
/// the interpreter runs on a thread sized from the limit.
const STACK_BYTES_PER_CALL: usize = 256 * 1024;

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
    let limit = resolve_max_call_depth();
    let program = program.clone();
    let builder = std::thread::Builder::new()
        .name("redblue-vm".to_string())
        .stack_size(limit.saturating_mul(STACK_BYTES_PER_CALL));

    let joined = match builder.spawn(move || {
        let mut vm = Vm::new();
        let result = vm.run(&program);
        (vm, result)
    }) {
        Ok(handle) => handle.join(),
        Err(e) => {
            return (
                Vm::new(),
                Err(Error::Io(format!(
                    "Cannot start the interpreter thread: {}",
                    e
                ))),
            );
        }
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
    output: Vec<String>,
    /// The module programs already run, by the name the `import` that loaded
    /// them goes by. An entry is what makes a second import of the same module a
    /// no-op rather than a second binding of the same names.
    modules: HashMap<String, Program>,
    /// Every module name an `import` has bound, to the module it names.
    ///
    /// An alias — `import json as J` — binds `J`, and `J.parse(...)` has to
    /// reach the same function `json.parse(...)` does. That is what this table
    /// is for: a call through a name it holds is a call through the module it
    /// points at, so the alias is a name for the module rather than a second
    /// copy of it.
    module_aliases: HashMap<String, String>,
    /// The modules whose bodies are running, innermost last. A module that
    /// reaches itself through an `import` is a circular import, and this is
    /// what says so instead of walking the cycle.
    importing: Vec<String>,
    /// Every `module` declaration that has run, by module name.
    ///
    /// An entry is what a second declaration of the same name is refused
    /// against, and what an `import` of the name finds in place of a file.
    declared_modules: HashMap<String, DeclaredModule>,
    /// How many `module` bodies are running. Above zero a `set` writes into the
    /// module's own scope instead of becoming a global, which is what makes a
    /// module declaration a boundary rather than a sequence of top-level
    /// statements.
    module_depth: usize,
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
    qualified_members: HashSet<String>,
    /// Every `object` declaration, by type name. The table is the inheritance
    /// model: an entry already holds its own fields and methods merged with
    /// everything it inherits, nearest declaration first, so a lookup is a
    /// single map read and cannot walk a cyclic parent chain.
    objects: HashMap<String, ObjectType>,
    expectation_failure: Option<crate::testing::assertions::TestAssertionError>,
    current_span: Span,
    call_depth: usize,
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
/// [`Vm::modules`] because a declared module is one the program itself
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
#[derive(Debug, Clone)]
struct ObjectType {
    /// The object this one extends, kept so that a child can be resolved
    /// against its own parent after a later declaration changes nothing.
    parent: Option<String>,
    /// Declared fields, defaults included, in declaration order. A field the
    /// child declares keeps the child's position and the child's default, and
    /// a parent field that reaches the same name is not copied in.
    fields: Fields,
    /// Methods the type answers to, nearest declaration first.
    methods: Fields,
}

/// The paths `import <name>` looks for, in order: the `modules/` directory, the
/// same directory written with a leading `./`, and the name as a path of its
/// own.
fn module_search_paths(name: &str) -> Vec<String> {
    vec![
        format!("modules/{name}.rb"),
        format!("./modules/{name}"),
        name.to_string(),
    ]
}

/// The first path the module `name` resolves to, or `None` when no path is
/// there.
pub fn module_path(name: &str) -> Option<String> {
    module_search_paths(name)
        .into_iter()
        .find(|path| Path::new(path).exists())
}

/// The source of the module `name` resolves to, or `None` when no path is
/// there. A path that is there but cannot be read is an error, not a miss: the
/// two say different things about the import.
pub fn module_source(name: &str) -> Result<Option<String>> {
    let Some(path) = module_path(name) else {
        return Ok(None);
    };
    let source = std::fs::read_to_string(&path)
        .map_err(|e| Error::Io(format!("Cannot load module '{}': {}", path, e)))?;
    Ok(Some(source))
}

/// The names `import <name>` binds into the importing program: the module's own
/// `set` and `constant` declarations, which is everything the loader runs.
///
/// Read by the analyzer, which cannot see into a separate file, so that a read
/// of one of these names is a read of a name the program binds. A module that
/// cannot be found, read or parsed binds nothing here — the loader still
/// reports that at runtime, and the analyzer must not report it first.
pub fn module_bound_names(name: &str) -> Vec<String> {
    let Some(source) = module_source(name).unwrap_or(None) else {
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
        let globals = stdlib::builtins();
        Self {
            globals,
            constants: HashSet::new(),
            locals: vec![CapturedScope::new()],
            output: Vec::new(),
            modules: HashMap::new(),
            module_aliases: HashMap::new(),
            importing: Vec::new(),
            declared_modules: HashMap::new(),
            module_depth: 0,
            qualified_members: HashSet::new(),
            objects: HashMap::new(),
            expectation_failure: None,
            current_span: Span::unknown(),
            call_depth: 0,
            max_call_depth: resolve_max_call_depth(),
            steps: 0,
            max_steps: resolve_max_steps(),
            max_iterations: resolve_max_iterations(),
            loop_depth: 0,
            loop_control: None,
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
            return Err(Error::Runtime(
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
        std::mem::take(&mut self.output)
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
    /// out — see [`Vm::qualified_members`]. They are in `globals` because that is
    /// where [`Vm::call_method`] looks for them, and a name the program never
    /// wrote is not one of its names: after `import MathUtils` this used to offer
    /// `MathUtils_circle_area` to completion and list it in `:vars`, which is a
    /// name no line of Redblue can call.
    pub fn user_names(&self) -> Vec<String> {
        let builtins = stdlib::builtins();
        let mut names: Vec<String> = self
            .globals
            .keys()
            .filter(|name| !builtins.contains_key(*name) && !self.qualified_members.contains(*name))
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
        self.qualified_members.insert(name.clone());
        self.globals.insert(name, value);
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
        if self.modules.contains_key(name) {
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

        self.modules.insert(name.to_string(), program);
        Ok(())
    }

    /// Whether `name` names a module this program can call into: a module file
    /// it imported, a module it declared, or a builtin namespace.
    fn is_module_name(&self, name: &str) -> bool {
        self.modules.contains_key(name)
            || self.declared_modules.contains_key(name)
            || stdlib::is_module(name)
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
        if self.declared_modules.contains_key(name) {
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
        self.declared_modules
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
            self.declared_modules.remove(name);
            return Err(error);
        }
        if self.loop_control.is_some() {
            // An abandoned body declares nothing: leaving the name registered
            // would answer a later `module` of the same name with `is already
            // declared` and a later `import` of it with a module that published
            // nothing. The bytecode VM drops it for the same reason on the way
            // out of an abrupt exit.
            self.declared_modules.remove(name);
            return Ok(Value::Nothing);
        }

        self.declared_modules
            .insert(name.to_string(), DeclaredModule { members });
        self.module_aliases
            .entry(name.to_string())
            .or_insert_with(|| name.to_string());
        Ok(Value::Nothing)
    }

    pub fn run(&mut self, program: &Program) -> Result<Value> {
        let mut result = Value::Nothing;

        for statement in &program.statements {
            result = self.execute_statement(statement)?;
        }

        // Print collected output
        for line in &self.output {
            println!("{}", line);
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
        let result = self
            .charge_step()
            .and_then(|()| self.execute_statement_body(&stmt.statement));
        self.current_span = previous_span;
        result
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
                self.output.push(value.to_string());
                Ok(Value::Nothing)
            }
            Statement::Print(expr) => {
                let value = self.evaluate(expr)?;
                print!("{}", value);
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
                self.raise_loop_control("break", LoopControl::Break)?;
                Ok(Value::Nothing)
            }
            Statement::Skip => {
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
                    // No catch to handle it. The `finally` is owed anyway: a
                    // `try ... finally` promises its cleanup whatever the region
                    // does, and returning above `run_finally_body` skipped it on
                    // exactly the path where the cleanup is wanted — the region
                    // failing is what left the thing to clean up. The failure is
                    // propagated *after* the cleanup rather than instead of it,
                    // which is also what an enclosing `try` written around this
                    // one expects: `?` hands the failure to it, and it is the
                    // `catch` there that decides what the program does next.
                    if !has_catch {
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
                    self.run_catch_body(catch_var.as_deref(), catch_body)?;
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

                    if let Some(path) = module_path(&item.name) {
                        // The path is there but may not be readable: that is an
                        // Io error, not a miss, so it is reported rather than
                        // folded into the "cannot find" below.
                        self.load_module(&path, &item.name)?;
                    } else if !self.declared_modules.contains_key(&item.name)
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
                    self.module_aliases
                        .insert(target_name.clone(), item.name.clone());
                    // The module's own name names it too — `import json as J`
                    // leaves `json.parse` working — unless the program has bound
                    // that name already, which is the shadowing rule every
                    // declaration in the language follows.
                    self.module_aliases
                        .entry(item.name.clone())
                        .or_insert_with(|| item.name.clone());

                    // A module this program declared has no file to read, so
                    // its published members are copied to the alias's own
                    // `alias_member` names here rather than by the loader.
                    if let Some(declared) = self.declared_modules.get(&item.name) {
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
    /// bound to the word `error` if there is one to bind.
    ///
    /// The scope is truncated rather than popped on the way out, so a `catch`
    /// that fails part way through — or that a `break` leaves part way through
    /// — does not leave the name it binds alive in the enclosing block.
    fn run_catch_body(&mut self, catch_var: Option<&str>, catch_body: &[Stmt]) -> Result<()> {
        let scope_base = self.locals.len();
        self.push_scope();
        let outcome = match catch_var {
            Some(var) => {
                self.declare(var);
                self.set_var(var, Value::Text("error".to_string()))
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
            Expr::Variable(name) => self
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
            Expr::Expect { actual, expected } => {
                let a = self.evaluate(actual)?;
                let e = self.evaluate(expected)?;
                match crate::testing::assertions::assert_values_equal(&e, &a) {
                    Ok(()) => Ok(Value::Nothing),
                    Err(failure) => {
                        self.expectation_failure = Some(failure.clone());
                        Err(Error::Runtime(failure.to_string(), self.span()))
                    }
                }
            }
        }
    }

    fn binary_op(&mut self, op: &BinaryOp, left: Value, right: Value) -> Result<Value> {
        runtime::binary_op(self.span(), op, left, right)
    }

    fn unary_op(&self, op: &UnaryOp, value: Value) -> Result<Value> {
        runtime::unary_op(self.span(), op, value)
    }

    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value> {
        // `append` writes through a binding, so it needs the frame the free
        // `runtime::builtin` does not have; every other builtin is asked first.
        if name == "append" {
            let span = self.span();
            let target = runtime::append_target(span, args)?;
            runtime::append_through(span, target, self.get_var_mut(target), args[1].clone())?;
            return Ok(Value::Nothing);
        }
        if let Some(value) = runtime::builtin(self.span(), name, args)? {
            return Ok(value);
        }

        match self.get_var(name) {
            Some(Value::Function(function)) => self.call_user_function(name, &function, args),
            // `map` is registered as a builtin name but has to *call* a
            // Redblue function, which the free `runtime::builtin` cannot do.
            _ if matches!(name, "map" | "list_map") => self.map_builtin(args),
            _ => Err(Error::Runtime(
                format!("Unknown function '{}'", name),
                self.span(),
            )),
        }
    }

    /// `map(list, function)` — the higher-order builtin, in the three spellings
    /// `SPEC.md` uses: `map(xs, f)`, `list.map(xs, f)` and `xs.map(f)`.
    ///
    /// It lives here rather than in `runtime::builtin` because applying a
    /// Redblue function needs this walker's captured scopes and its call-depth
    /// budget. The function runs once per element in order and its result
    /// becomes the mapped element; a failure inside it fails the whole call,
    /// with the depth released as `call_user_function` releases it.
    fn map_builtin(&mut self, args: &[Value]) -> Result<Value> {
        let [Value::List(items), Value::Function(function)] = args else {
            return Err(Error::Runtime(
                "map requires a list and a function".to_string(),
                self.span(),
            ));
        };

        self.apply_map(&**items, &function)
    }

    /// Applies `function` to every element of `items`, in order.
    fn apply_map(&mut self, items: &[Value], function: &FunctionValue) -> Result<Value> {
        let mut mapped = Vec::with_capacity(items.len());
        for item in items {
            mapped.push(self.call_user_function(ANONYMOUS_FUNCTION, function, &[item.clone()])?);
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
    /// Anything else is an error rather than a guess.
    fn call_method(&mut self, receiver: &Expr, method: &str, args: &[Expr]) -> Result<Value> {
        let arg_values: Result<Vec<Value>> = args.iter().map(|a| self.evaluate(a)).collect();
        let args = arg_values?;

        let name = match receiver {
            Expr::Variable(name) => name.clone(),
            _ => {
                let this = self.evaluate(receiver)?;
                // `xs.map(f)` is the method spelling of the higher-order builtin,
                // and a list is the one receiver that is not an object and still
                // has a method. Every other receiver is still the error below.
                if method == "map" {
                    if let Value::List(items) = this {
                        let function = match args.as_slice() {
                            [Value::Function(function)] => function.clone(),
                            _ => {
                                return Err(Error::Runtime(
                                    "map requires a function".to_string(),
                                    self.span(),
                                ))
                            }
                        };
                        return self.apply_map(&items, &function);
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

        let object = match self.objects.get(&name) {
            Some(object) => object.clone(),
            None => {
                // A module function: the receiver names a module, so the
                // function is `module_function`. An alias is resolved to the
                // module it names first, so `import json as J` then
                // `J.parse(..)` reaches the same function `json.parse(..)`
                // does.
                let module = self
                    .module_aliases
                    .get(&name)
                    .cloned()
                    .unwrap_or_else(|| name.clone());
                let qualified = qualified_member(&module, method);
                if self.get_var(&qualified).is_none() && self.is_module_name(&module) {
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
        if self.objects.contains_key(name) {
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
            let Some(object) = self.objects.get(&current).cloned() else {
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

        self.objects.insert(
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
            return Err(Error::Runtime(
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
        self.push_scope();
        for (index, param) in function.params.iter().enumerate() {
            let value = args.get(index).cloned().unwrap_or(Value::Nothing);
            self.declare(param);
            self.set_var(param, value)?;
        }
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

        result
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}
