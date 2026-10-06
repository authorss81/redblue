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
    expect_range_number, finite_number, range_has_next, Captured, CapturedScope, Fields,
    FunctionBody, FunctionValue, Value,
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

    ast.statements
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
            objects: HashMap::new(),
            expectation_failure: None,
            current_span: Span::unknown(),
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

    /// Charges one statement to the step budget, failing once the program has
    /// run [`Vm::max_steps`] statements. Called for every statement, including
    /// those inside a function body, so the budget bounds the program rather
    /// than one statement list.
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
    fn charge_iteration(&self, loop_iterations: &mut usize, kind: &str) -> Result<()> {
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

        let program = runtime::module_program(path)?;
        let bound = runtime::module_bindings(&program, |expr| self.evaluate(expr))?;
        for (name, value, is_const) in bound {
            if is_const {
                self.bind_constant(&name, value)?;
            } else {
                self.bind_module_name(&name, value)?;
            }
        }

        self.modules.insert(name.to_string(), program);
        Ok(())
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
        // Check local scopes first
        for scope in self.locals.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        // Check globals
        self.globals.get(name).cloned()
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
                    fields.insert(property.clone(), val);
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
                if cond.is_truthy() {
                    for stmt in then_branch {
                        self.execute_statement(stmt)?;
                    }
                } else {
                    for stmt in else_branch {
                        self.execute_statement(stmt)?;
                    }
                }
                Ok(Value::Nothing)
            }
            Statement::Unless { condition, body } => {
                // The body is taken exactly when the condition is false, so a
                // true condition leaves the interpreter where it was.
                let cond = self.evaluate(condition)?;
                if !cond.is_truthy() {
                    for stmt in body {
                        self.execute_statement(stmt)?;
                    }
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
                    for item in items {
                        self.charge_iteration(&mut loop_iterations, "for each")?;
                        self.push_scope();
                        self.declare(variable);
                        self.set_var(variable, item)?;
                        for stmt in body {
                            self.execute_statement(stmt)?;
                        }
                        self.pop_scope();
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
                    self.push_scope();
                    self.declare(variable);
                    self.set_var(variable, Value::number(i, self.span())?)?;
                    for stmt in body {
                        self.execute_statement(stmt)?;
                    }
                    self.pop_scope();
                    // The stepped value is checked too: a step that
                    // overflows the counter is the same failure as one that
                    // would put an infinity in the loop variable.
                    i = finite_number(i + step, self.span())?;
                }
                Ok(Value::Nothing)
            }
            Statement::Repeat { count, body } => {
                let count_val = self.evaluate(count)?;
                if let Value::Number(n) = count_val {
                    let mut loop_iterations = 0;
                    for _ in 0..(n as i64) {
                        self.charge_iteration(&mut loop_iterations, "repeat")?;
                        self.push_scope();
                        for stmt in body {
                            self.execute_statement(stmt)?;
                        }
                        self.pop_scope();
                    }
                }
                Ok(Value::Nothing)
            }
            Statement::While { condition, body } => {
                let mut loop_iterations = 0;
                while self.evaluate(condition)?.is_truthy() {
                    self.charge_iteration(&mut loop_iterations, "while")?;
                    self.push_scope();
                    for stmt in body {
                        self.execute_statement(stmt)?;
                    }
                    self.pop_scope();
                }
                Ok(Value::Nothing)
            }
            Statement::Break => {
                // TODO: Implement proper control flow
                Ok(Value::Nothing)
            }
            Statement::Skip => {
                // TODO: Implement proper control flow
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
                let result = self.execute_statements(body);

                // A catch is PRESENT whenever the parser saw one, which is not the
                // same as catch_var being Some: `catch` with no name binding parses
                // to catch_var == None with a non-empty body. Testing the Option
                // therefore (a) never ran a bare catch and (b) fell through to
                // `Ok(Value::Nothing)` below, silently swallowing the error. A
                // swallowed error is the interpreter lying about what happened.
                let has_catch = catch_var.is_some() || !catch_body.is_empty();

                match (result, has_catch) {
                    (Err(_), true) => {
                        self.push_scope();
                        if let Some(var) = catch_var {
                            self.declare(var);
                            self.set_var(var, Value::Text("error".to_string()))?;
                        }
                        for stmt in catch_body {
                            self.execute_statement(stmt)?;
                        }
                        self.pop_scope();
                    }
                    // No catch to handle it: propagate rather than discard.
                    (Err(e), false) => return Err(e),
                    (Ok(_), _) => {}
                }

                for stmt in finally_body {
                    self.execute_statement(stmt)?;
                }

                Ok(Value::Nothing)
            }
            Statement::Import(items) => {
                for item in items {
                    let Some(path) = module_path(&item.name) else {
                        return Err(Error::Runtime(
                            format!("Cannot find module '{}'", item.name),
                            self.span(),
                        ));
                    };
                    // The path is there but may not be readable: that is an Io
                    // error, not a miss, so it is reported rather than folded
                    // into the "cannot find" above.
                    self.load_module(&path, &item.name)?;

                    let target_name = item.alias.as_ref().unwrap_or(&item.name);
                    // The name the import itself binds goes through the same
                    // refusal as the names the module contributes: it is a
                    // program-level name like they are.
                    let target_name = target_name.clone();
                    self.bind_module_name(&target_name, Value::Nothing)?;
                }
                Ok(Value::Nothing)
            }
            Statement::Test { name: _, body } => {
                // Execute test body
                for stmt in body {
                    self.execute_statement(stmt)?;
                }
                Ok(Value::Nothing)
            }
            Statement::Expr(expr) => self.evaluate(expr),
        }
    }

    fn execute_statements(&mut self, statements: &[Stmt]) -> Result<Value> {
        let mut result = Value::Nothing;
        for stmt in statements {
            result = self.execute_statement(stmt)?;
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
                Ok(Value::List(values?))
            }
            Expr::Record(fields) => {
                let mut record = Fields::new();
                for (key, value) in fields {
                    record.insert(key.clone(), self.evaluate(value)?);
                }
                Ok(Value::Record(record))
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
        if let Some(value) = runtime::builtin(self.span(), name, args)? {
            return Ok(value);
        }

        match self.get_var(name) {
            Some(Value::Function(function)) => self.call_user_function(name, &function, args),
            _ => Err(Error::Runtime(
                format!("Unknown function '{}'", name),
                self.span(),
            )),
        }
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
            None => return self.call(&format!("{}_{}", name, method), &args),
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
        let record = Value::Record(fields);
        self.declare(name);
        self.set_var(name, record)?;

        // A body that is not a field or a method declaration runs in the
        // enclosing scope, in order, and after the type is registered, so a
        // nested `object` in the body may extend this one.
        for stmt in &rest {
            self.execute_statement(stmt)?;
        }

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
        // A function value the bytecode VM built names a compiled block instead
        // of statements, and cannot be run by this walker. `rb run` only ever
        // builds statements itself, so reaching this is a caller mixing the two
        // VMs rather than a program that did something.
        let FunctionBody::Statements(body) = &function.body else {
            self.call_depth -= 1;
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
