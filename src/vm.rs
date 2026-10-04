use crate::error::{Error, Result, Span};
use crate::lexer::Lexer;
use crate::parser as redblue_parser;
use crate::parser::{BinaryOp, Expr, Program, Statement, Stmt, UnaryOp};
use crate::stdlib;
use crate::value::{finite_number, Captured, CapturedScope, Fields, FunctionValue, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

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
    /// The live local scopes, outermost first. A call's parameters are the
    /// innermost scope, and a function value that closed over scopes pushes
    /// them above its caller's frames — see [`Vm::call_user_function`].
    locals: Vec<CapturedScope>,
    output: Vec<String>,
    _modules: HashMap<String, Program>,
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

impl Vm {
    pub fn new() -> Self {
        let globals = stdlib::builtins();
        Self {
            globals,
            locals: vec![CapturedScope::new()],
            output: Vec::new(),
            _modules: HashMap::new(),
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

    fn load_module(&mut self, path: &str) -> Result<()> {
        let source = std::fs::read_to_string(path)
            .map_err(|e| Error::Io(format!("Cannot load module '{}': {}", path, e)))?;

        let tokens = Lexer::tokenize(&source)?;
        let ast = redblue_parser::Parser::new(tokens).parse()?;

        // A module's functions are not bound to a name, so a member stays unreachable
        // — see FINDINGS.md. The `set` below is the whole of what an import
        // currently contributes.
        for stmt in &ast.statements {
            if let Statement::Set { name, value } = &stmt.statement {
                let val = self.evaluate(value)?;
                self.globals.insert(name.clone(), val);
            }
        }

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
            body: Arc::new(body.to_vec()),
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

    /// Binds `name` in the innermost live scope that already has it, and in a
    /// global when no local scope does.
    ///
    /// This resolves a name exactly as [`Vm::get_var`] does, so a read and a
    /// write of one name agree. It used to write only into the innermost scope
    /// and fall straight through to a global otherwise, which meant that
    /// assigning to a variable of an enclosing scope — a parameter from inside
    /// a loop body, or a captured name from inside a function that closed over
    /// it — silently created a global of that name instead.
    fn set_var(&mut self, name: &str, value: Value) {
        for scope in self.locals.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), value);
                return;
            }
        }
        self.globals.insert(name.to_string(), value);
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
                self.set_var(name, val);
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
                    self.set_var(object, Value::Record(fields));
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
                        self.set_var(variable, item);
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

                if let (Value::Number(start), Value::Number(end), Value::Number(step)) =
                    (start_val, end_val, step_val)
                {
                    let mut i = start;
                    let mut loop_iterations = 0;
                    while i <= end {
                        self.charge_iteration(&mut loop_iterations, "for each from")?;
                        self.push_scope();
                        self.declare(variable);
                        self.set_var(variable, Value::number(i, self.span())?);
                        for stmt in body {
                            self.execute_statement(stmt)?;
                        }
                        self.pop_scope();
                        // The stepped value is checked too: a step that
                        // overflows the counter is the same failure as one that
                        // would put an infinity in the loop variable.
                        i = finite_number(i + step, self.span())?;
                    }
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
                self.set_var(name, function);

                Ok(Value::Nothing)
            }
            Statement::Method { name, params, body } => {
                let method = self.make_function(name, params, body);
                self.set_var(name, method);
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

                if result.is_err() {
                    if let Some(var) = catch_var {
                        self.push_scope();
                        self.declare(var);
                        self.set_var(var, Value::Text("error".to_string()));
                        for stmt in catch_body {
                            self.execute_statement(stmt)?;
                        }
                        self.pop_scope();
                    }
                }

                for stmt in finally_body {
                    self.execute_statement(stmt)?;
                }

                Ok(Value::Nothing)
            }
            Statement::Import(items) => {
                for item in items {
                    let module_path = format!("modules/{}.rb", item.name);
                    let module_path_dot = format!("./modules/{}", item.name);
                    let search_paths = vec![
                        module_path.as_str(),
                        module_path_dot.as_str(),
                        item.name.as_str(),
                    ];

                    let mut loaded = false;
                    for path in &search_paths {
                        if Path::new(path).exists() {
                            self.load_module(path)?;
                            loaded = true;
                            break;
                        }
                    }

                    if !loaded {
                        return Err(Error::Runtime(
                            format!("Cannot find module '{}'", item.name),
                            self.span(),
                        ));
                    }

                    let target_name = item.alias.as_ref().unwrap_or(&item.name);
                    self.globals.insert(target_name.clone(), Value::Nothing);
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
                if let Value::Record(fields) = obj {
                    Ok(fields.get(property).cloned().unwrap_or(Value::Nothing))
                } else {
                    Err(Error::Runtime(
                        "Cannot access property on non-object".to_string(),
                        self.span(),
                    ))
                }
            }
            Expr::Index { object, index } => {
                let obj = self.evaluate(object)?;
                let idx = self.evaluate(index)?;
                let Value::List(items) = obj else {
                    return Err(Error::Runtime(
                        "Cannot index non-list".to_string(),
                        self.span(),
                    ));
                };
                let Value::Number(n) = idx else {
                    return Err(Error::Runtime(
                        "Index must be a number".to_string(),
                        self.span(),
                    ));
                };
                // A fractional index names no element, so it is rejected rather
                // than truncated: `items[0.5]` must not quietly answer `items[0]`.
                if n.fract() != 0.0 {
                    return Err(Error::Runtime(
                        format!(
                            "Index {} is out of bounds: a list index must be a whole number",
                            Value::Number(n)
                        ),
                        self.span(),
                    ));
                }
                // A negative index counts from the end, so `n as i64` saturates to
                // `i64::MIN` at one extreme. That sum is in range while `len` is
                // non-negative, but the bound is arithmetic rather than a stated
                // invariant, so it is checked instead of assumed, and the result
                // is converted rather than cast: a negative offset cast to `usize`
                // wraps to a huge value and would name a different element than
                // the program asked for.
                let element = if n < 0.0 {
                    (items.len() as i64).checked_add(n as i64)
                } else {
                    Some(n as i64)
                }
                .and_then(|offset| usize::try_from(offset).ok())
                .and_then(|offset| items.get(offset))
                .cloned();
                element.ok_or_else(|| {
                    Error::Runtime(
                        format!(
                            "Index {} is out of bounds: length is {}, {}",
                            Value::Number(n),
                            items.len(),
                            if items.is_empty() {
                                "the list is empty, so it has no valid index".to_string()
                            } else {
                                format!("valid indexes are 0 to {}", items.len() - 1)
                            }
                        ),
                        self.span(),
                    )
                })
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
        match op {
            BinaryOp::Add => match (left, right) {
                (Value::Number(a), Value::Number(b)) => Value::number(a + b, self.span()),
                (Value::Text(a), Value::Text(b)) => Ok(Value::Text(format!("{}{}", a, b))),
                _ => Err(Error::Runtime(
                    "Cannot add non-numbers".to_string(),
                    self.span(),
                )),
            },
            BinaryOp::Sub => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    Value::number(a - b, self.span())
                } else {
                    Err(Error::Runtime(
                        "Cannot subtract non-numbers".to_string(),
                        self.span(),
                    ))
                }
            }
            BinaryOp::Mul => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    Value::number(a * b, self.span())
                } else {
                    Err(Error::Runtime(
                        "Cannot multiply non-numbers".to_string(),
                        self.span(),
                    ))
                }
            }
            BinaryOp::Div => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    if b == 0.0 {
                        Err(Error::Runtime("Division by zero".to_string(), self.span()))
                    } else {
                        Value::number(a / b, self.span())
                    }
                } else {
                    Err(Error::Runtime(
                        "Cannot divide non-numbers".to_string(),
                        self.span(),
                    ))
                }
            }
            BinaryOp::Mod => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    if b == 0.0 {
                        Err(Error::Runtime("Modulo by zero".to_string(), self.span()))
                    } else {
                        Value::number(a % b, self.span())
                    }
                } else {
                    Err(Error::Runtime(
                        "Cannot modulo non-numbers".to_string(),
                        self.span(),
                    ))
                }
            }
            BinaryOp::Equal => Ok(Value::YesNo(left == right)),
            BinaryOp::NotEqual => Ok(Value::YesNo(left != right)),
            BinaryOp::Less => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    Ok(Value::YesNo(a < b))
                } else {
                    Err(Error::Runtime(
                        "Cannot compare non-numbers".to_string(),
                        self.span(),
                    ))
                }
            }
            BinaryOp::LessEqual => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    Ok(Value::YesNo(a <= b))
                } else {
                    Err(Error::Runtime(
                        "Cannot compare non-numbers".to_string(),
                        self.span(),
                    ))
                }
            }
            BinaryOp::Greater => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    Ok(Value::YesNo(a > b))
                } else {
                    Err(Error::Runtime(
                        "Cannot compare non-numbers".to_string(),
                        self.span(),
                    ))
                }
            }
            BinaryOp::GreaterEqual => {
                if let (Value::Number(a), Value::Number(b)) = (left, right) {
                    Ok(Value::YesNo(a >= b))
                } else {
                    Err(Error::Runtime(
                        "Cannot compare non-numbers".to_string(),
                        self.span(),
                    ))
                }
            }
            BinaryOp::And => Ok(Value::YesNo(left.is_truthy() && right.is_truthy())),
            BinaryOp::Or => Ok(Value::YesNo(left.is_truthy() || right.is_truthy())),
            BinaryOp::In => {
                if let Value::List(items) = right {
                    Ok(Value::YesNo(items.contains(&left)))
                } else {
                    Err(Error::Runtime(
                        "Right side of 'in' must be a list".to_string(),
                        self.span(),
                    ))
                }
            }
        }
    }

    fn unary_op(&self, op: &UnaryOp, value: Value) -> Result<Value> {
        match op {
            UnaryOp::Neg => {
                if let Value::Number(n) = value {
                    Ok(Value::Number(-n))
                } else {
                    Err(Error::Runtime(
                        "Cannot negate non-number".to_string(),
                        self.span(),
                    ))
                }
            }
            UnaryOp::Not => Ok(Value::YesNo(!value.is_truthy())),
        }
    }

    fn call(&mut self, name: &str, args: &[Value]) -> Result<Value> {
        // Check for built-in functions
        match name {
            "say" => {
                if let Some(arg) = args.first() {
                    println!("{}", arg);
                    Ok(Value::Nothing)
                } else {
                    Err(Error::Runtime(
                        "say requires an argument".to_string(),
                        self.span(),
                    ))
                }
            }
            "length" | "len" => {
                if let Some(Value::List(items)) = args.first().cloned() {
                    Ok(Value::Number(items.len() as f64))
                } else if let Some(Value::Text(s)) = args.first().cloned() {
                    Ok(Value::Number(s.len() as f64))
                } else {
                    Err(Error::Runtime(
                        "length requires a list or text".to_string(),
                        self.span(),
                    ))
                }
            }
            "input" | "ask" => {
                let mut input = String::new();
                if let Some(prompt) = args.first() {
                    print!("{}", prompt);
                }
                std::io::stdin()
                    .read_line(&mut input)
                    .map_err(|e| Error::Runtime(e.to_string(), self.span()))?;
                input.pop(); // Remove newline
                Ok(Value::Text(input))
            }
            "random" => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                Ok(Value::Number((now.as_nanos() % 1000) as f64))
            }
            // Files module
            "files_read" => {
                let path = match args.first() {
                    Some(Value::Text(p)) => p,
                    _ => {
                        return Err(Error::Runtime(
                            "files.read requires a text path".to_string(),
                            self.span(),
                        ))
                    }
                };
                std::fs::read_to_string(path)
                    .map(Value::Text)
                    .map_err(|e| Error::Io(format!("Failed to read '{}': {}", path, e)))
            }
            "files_write" => {
                let (path, content) = match (args.first(), args.get(1)) {
                    (Some(Value::Text(p)), Some(Value::Text(c))) => (p, c),
                    _ => {
                        return Err(Error::Runtime(
                            "files.write requires two text arguments".to_string(),
                            self.span(),
                        ))
                    }
                };
                std::fs::write(path, content)
                    .map_err(|e| Error::Io(format!("Failed to write '{}': {}", path, e)))?;
                Ok(Value::Nothing)
            }
            "files_append" => {
                let (path, content) = match (args.first(), args.get(1)) {
                    (Some(Value::Text(p)), Some(Value::Text(c))) => (p, c),
                    _ => {
                        return Err(Error::Runtime(
                            "files.append requires two text arguments".to_string(),
                            self.span(),
                        ))
                    }
                };
                std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .and_then(|mut f| std::io::Write::write_all(&mut f, content.as_bytes()))
                    .map_err(|e| Error::Io(format!("Failed to append to '{}': {}", path, e)))?;
                Ok(Value::Nothing)
            }
            "files_exists" => {
                let path = match args.first() {
                    Some(Value::Text(p)) => p,
                    _ => {
                        return Err(Error::Runtime(
                            "files.exists requires a text path".to_string(),
                            self.span(),
                        ))
                    }
                };
                Ok(Value::YesNo(std::path::Path::new(path).exists()))
            }
            "files_lines" => {
                let path = match args.first() {
                    Some(Value::Text(p)) => p,
                    _ => {
                        return Err(Error::Runtime(
                            "files.lines requires a text path".to_string(),
                            self.span(),
                        ))
                    }
                };
                let content = std::fs::read_to_string(path)
                    .map_err(|e| Error::Io(format!("Failed to read '{}': {}", path, e)))?;
                let lines: Vec<Value> = content
                    .lines()
                    .map(|l| Value::Text(l.to_string()))
                    .collect();
                Ok(Value::List(lines))
            }
            "files_delete" => {
                let path = match args.first() {
                    Some(Value::Text(p)) => p,
                    _ => {
                        return Err(Error::Runtime(
                            "files.delete requires a text path".to_string(),
                            self.span(),
                        ))
                    }
                };
                std::fs::remove_file(path)
                    .map_err(|e| Error::Io(format!("Failed to delete '{}': {}", path, e)))?;
                Ok(Value::Nothing)
            }
            "files_copy" => {
                let (from, to) = match (args.first(), args.get(1)) {
                    (Some(Value::Text(f)), Some(Value::Text(t))) => (f, t),
                    _ => {
                        return Err(Error::Runtime(
                            "files.copy requires two text arguments".to_string(),
                            self.span(),
                        ))
                    }
                };
                std::fs::copy(from, to)
                    .map(|_| Value::Nothing)
                    .map_err(|e| Error::Io(format!("Failed to copy '{}' to '{}': {}", from, to, e)))
            }
            "files_rename" => {
                let (from, to) = match (args.first(), args.get(1)) {
                    (Some(Value::Text(f)), Some(Value::Text(t))) => (f, t),
                    _ => {
                        return Err(Error::Runtime(
                            "files.rename requires two text arguments".to_string(),
                            self.span(),
                        ))
                    }
                };
                std::fs::rename(from, to)
                    .map(|_| Value::Nothing)
                    .map_err(|e| {
                        Error::Io(format!("Failed to rename '{}' to '{}': {}", from, to, e))
                    })
            }
            // Time module
            "time_now" => {
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| Error::Runtime(e.to_string(), self.span()))?;
                let secs = now.as_secs();
                let nanos = now.subsec_nanos();
                let record = crate::value::Fields::from([
                    ("seconds".to_string(), Value::Number(secs as f64)),
                    ("nanoseconds".to_string(), Value::Number(nanos as f64)),
                ]);
                Ok(Value::Record(record))
            }
            "time_sleep" => {
                let seconds = match args.first() {
                    Some(Value::Number(n)) => *n,
                    _ => {
                        return Err(Error::Runtime(
                            "time.sleep requires a number".to_string(),
                            self.span(),
                        ))
                    }
                };
                std::thread::sleep(std::time::Duration::from_secs_f64(seconds));
                Ok(Value::Nothing)
            }
            "time_format" => {
                use std::time::UNIX_EPOCH;
                let (timestamp, format) = match (args.first(), args.get(1)) {
                    (Some(Value::Number(ts)), Some(Value::Text(fmt))) => (*ts, fmt.clone()),
                    (Some(Value::Number(ts)), None) => (*ts, "%Y-%m-%d %H:%M:%S".to_string()),
                    _ => {
                        return Err(Error::Runtime(
                            "time.format requires a number and optional text".to_string(),
                            self.span(),
                        ))
                    }
                };
                let datetime = UNIX_EPOCH + std::time::Duration::from_secs(timestamp as u64);
                let tm = chrono::DateTime::from_timestamp(
                    datetime.duration_since(UNIX_EPOCH).unwrap().as_secs() as i64,
                    0,
                )
                .ok_or_else(|| Error::Runtime("Invalid timestamp".to_string(), self.span()))?;
                Ok(Value::Text(tm.format(&format).to_string()))
            }
            "time_unix" => {
                let text = match args.first() {
                    Some(Value::Text(s)) => s,
                    _ => {
                        return Err(Error::Runtime(
                            "time.unix requires a text".to_string(),
                            self.span(),
                        ))
                    }
                };
                let parsed = chrono::NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S")
                    .map_err(|_| {
                        Error::Runtime(
                            "Invalid date format, use YYYY-MM-DD HH:MM:SS".to_string(),
                            self.span(),
                        )
                    })?;
                Ok(Value::Number(parsed.and_utc().timestamp() as f64))
            }
            // Formats module (JSON/CSV)
            "json_parse" => {
                let text = match args.first() {
                    Some(Value::Text(s)) => s,
                    _ => {
                        return Err(Error::Runtime(
                            "json.parse requires text".to_string(),
                            self.span(),
                        ))
                    }
                };
                parse_json(text, self.span())
                    .map_err(|e| Error::Runtime(e.to_string(), self.span()))
            }
            "json_stringify" => {
                let value = match args.first() {
                    Some(v) => v.clone(),
                    _ => {
                        return Err(Error::Runtime(
                            "json.stringify requires a value".to_string(),
                            self.span(),
                        ))
                    }
                };
                Ok(Value::Text(json_stringify(&value)))
            }
            "csv_parse" => {
                let text = match args.first() {
                    Some(Value::Text(s)) => s,
                    _ => {
                        return Err(Error::Runtime(
                            "csv.parse requires text".to_string(),
                            self.span(),
                        ))
                    }
                };
                let lines: Vec<Value> = text
                    .lines()
                    .map(|line| {
                        let cells: Vec<Value> = line
                            .split(',')
                            .map(|cell| Value::Text(cell.trim().to_string()))
                            .collect();
                        Value::List(cells)
                    })
                    .collect();
                Ok(Value::List(lines))
            }
            // Network module
            "network_get" => {
                let url = match args.first() {
                    Some(Value::Text(u)) => u,
                    _ => {
                        return Err(Error::Runtime(
                            "network.get requires a URL".to_string(),
                            self.span(),
                        ))
                    }
                };
                let client = reqwest::blocking::Client::new();
                let response = client.get(url).send().map_err(|e| {
                    Error::Runtime(format!("HTTP request failed: {}", e), self.span())
                })?;
                let body = response.text().map_err(|e| {
                    Error::Runtime(format!("Failed to read response: {}", e), self.span())
                })?;
                Ok(Value::Text(body))
            }
            "network_post" => {
                let (url, data) = match (args.first(), args.get(1)) {
                    (Some(Value::Text(u)), Some(Value::Text(d))) => (u, d),
                    _ => {
                        return Err(Error::Runtime(
                            "network.post requires URL and data".to_string(),
                            self.span(),
                        ))
                    }
                };
                let client = reqwest::blocking::Client::new();
                let response = client.post(url).body(data.clone()).send().map_err(|e| {
                    Error::Runtime(format!("HTTP request failed: {}", e), self.span())
                })?;
                let body = response.text().map_err(|e| {
                    Error::Runtime(format!("Failed to read response: {}", e), self.span())
                })?;
                Ok(Value::Text(body))
            }
            // Testing module
            "expect" | "assert" => {
                let (actual, expected) = match (args.first(), args.get(1)) {
                    (Some(a), Some(e)) => (a.clone(), e.clone()),
                    _ => {
                        return Err(Error::Runtime(
                            "expect requires two arguments".to_string(),
                            self.span(),
                        ))
                    }
                };
                if actual != expected {
                    return Err(Error::Runtime(
                        format!(
                            "Assertion failed: expected {:?} but got {:?}",
                            expected, actual
                        ),
                        self.span(),
                    ));
                }
                Ok(Value::Nothing)
            }
            // Console module
            "console_log" => {
                if let Some(arg) = args.first() {
                    println!("{}", arg);
                }
                Ok(Value::Nothing)
            }
            "console_error" => {
                if let Some(arg) = args.first() {
                    eprintln!("{}", arg);
                }
                Ok(Value::Nothing)
            }
            "console_clear" => {
                print!("\x1B[2J\x1B[1H");
                Ok(Value::Nothing)
            }
            // Random module
            "random_number" => {
                let (min, max) = match (args.first(), args.get(1)) {
                    (Some(Value::Number(min)), Some(Value::Number(max))) => (*min, *max),
                    (Some(Value::Number(max)), None) => (0.0, *max),
                    _ => (0.0, 1.0),
                };
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                let r = (now.as_nanos() % 1000000) as f64 / 1000000.0;
                // `max - min` overflows for a range as ordinary as
                // `-1e308` to `1e308`, which is a number that does not exist.
                Value::number(min + r * (max - min), self.span())
            }
            "random_choice" => {
                if let Some(Value::List(items)) = args.first() {
                    if items.is_empty() {
                        return Ok(Value::Nothing);
                    }
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                    let idx = (now.as_nanos() as usize) % items.len();
                    Ok(items[idx].clone())
                } else {
                    Err(Error::Runtime(
                        "random_choice requires a list".to_string(),
                        self.span(),
                    ))
                }
            }
            "random_shuffle" => {
                if let Some(Value::List(mut items)) = args.first().cloned() {
                    use std::time::{SystemTime, UNIX_EPOCH};
                    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
                    let seed = now.as_nanos() as usize;

                    for i in (1..items.len()).rev() {
                        let j = seed % (i + 1);
                        items.swap(i, j);
                    }
                    Ok(Value::List(items))
                } else {
                    Err(Error::Runtime(
                        "random_shuffle requires a list".to_string(),
                        self.span(),
                    ))
                }
            }
            // Type conversion
            "type_of" => {
                let type_name = match args.first() {
                    Some(Value::Number(_)) => "number",
                    Some(Value::Text(_)) => "text",
                    Some(Value::YesNo(_)) => "yes/no",
                    Some(Value::Nothing) => "nothing",
                    Some(Value::List(_)) => "list",
                    Some(Value::Record(_)) => "record",
                    Some(Value::Function(_)) => "function",
                    Some(Value::Builtin(_)) => "builtin",
                    Some(Value::Object(_, _)) => "object",
                    None => "nothing",
                };
                Ok(Value::Text(type_name.to_string()))
            }
            _ => {
                // User-defined function
                if let Some(Value::Function(function)) = self.get_var(name) {
                    self.call_user_function(name, &function, args)
                } else {
                    Err(Error::Runtime(
                        format!("Unknown function '{}'", name),
                        self.span(),
                    ))
                }
            }
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
        self.set_var("this", this);
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
        self.set_var(name, record);

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
            self.set_var(param, value);
        }
        self.call_depth += 1;
        let result = self.execute_statements(&function.body);
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

fn parse_json(json: &str, span: Span) -> Result<Value> {
    let json = json.trim();
    if json.starts_with('{') {
        parse_json_object(json, span)
    } else if json.starts_with('[') {
        parse_json_array(json, span)
    } else if json.starts_with('"') {
        Ok(Value::Text(parse_json_string(json, span)?))
    } else if json == "null" {
        Ok(Value::Nothing)
    } else if json == "true" {
        Ok(Value::YesNo(true))
    } else if json == "false" {
        Ok(Value::YesNo(false))
    } else {
        match json.parse::<f64>() {
            Ok(n) => Value::number(n, span),
            Err(_) => Err(Error::Runtime(format!("Invalid JSON: {}", json), span)),
        }
    }
}

fn parse_json_object(json: &str, span: Span) -> Result<Value> {
    let json = json.trim();
    if !json.starts_with('{') || !json.ends_with('}') {
        return Err(Error::Runtime("Invalid JSON object".to_string(), span));
    }
    let mut map = crate::value::Fields::new();
    let content = &json[1..json.len() - 1];
    if content.trim().is_empty() {
        return Ok(Value::Record(map));
    }
    for pair in split_json_pairs(content) {
        let parts: Vec<&str> = pair.splitn(2, ':').collect();
        if parts.len() != 2 {
            continue;
        }
        let key = parse_json_string(parts[0].trim(), span)?;
        let value = parse_json(parts[1].trim(), span)?;
        map.insert(key, value);
    }
    Ok(Value::Record(map))
}

fn parse_json_array(json: &str, span: Span) -> Result<Value> {
    let json = json.trim();
    if !json.starts_with('[') || !json.ends_with(']') {
        return Err(Error::Runtime("Invalid JSON array".to_string(), span));
    }
    let content = &json[1..json.len() - 1];
    if content.trim().is_empty() {
        return Ok(Value::List(Vec::new()));
    }
    let mut items = Vec::new();
    for item in split_json_elements(content) {
        items.push(parse_json(item, span)?);
    }
    Ok(Value::List(items))
}

fn parse_json_string(json: &str, span: Span) -> Result<String> {
    let json = json.trim();
    if !json.starts_with('"') || !json.ends_with('"') {
        return Err(Error::Runtime("Invalid JSON string".to_string(), span));
    }
    let mut result = String::new();
    let chars: Vec<char> = json[1..json.len() - 1].chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            match chars[i + 1] {
                'n' => result.push('\n'),
                't' => result.push('\t'),
                'r' => result.push('\r'),
                '\"' => result.push('"'),
                '\\' => result.push('\\'),
                _ => result.push(chars[i + 1]),
            }
            i += 2;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    Ok(result)
}

fn split_json_pairs(content: &str) -> Vec<&str> {
    let mut pairs = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let mut in_string = false;
    let mut prev_char: Option<char> = None;
    for (i, c) in content.char_indices() {
        if c == '"' && prev_char != Some('\\') {
            in_string = !in_string;
        }
        if !in_string {
            if c == '{' || c == '[' {
                depth += 1;
            } else if c == '}' || c == ']' {
                depth -= 1;
            } else if c == ',' && depth == 0 {
                pairs.push(&content[start..i]);
                start = i + c.len_utf8();
            }
        }
        prev_char = Some(c);
    }
    if start < content.len() {
        pairs.push(&content[start..]);
    }
    pairs
}

fn split_json_elements(content: &str) -> Vec<&str> {
    split_json_pairs(content)
}

fn json_stringify(value: &Value) -> String {
    match value {
        Value::Nothing => "null".to_string(),
        Value::YesNo(b) => {
            if *b {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Value::Number(n) => {
            // JSON has no literal for a number that is not finite, so it is
            // written as `null`, which is what every JSON writer emits for one.
            // No Redblue program can reach this branch: see SPEC.md.
            if !n.is_finite() {
                "null".to_string()
            } else if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Value::Text(s) => {
            let mut result = String::from("\"");
            for c in s.chars() {
                match c {
                    '\n' => result.push_str("\\n"),
                    '\r' => result.push_str("\\r"),
                    '\t' => result.push_str("\\t"),
                    '"' => result.push_str("\\\""),
                    '\\' => result.push_str("\\\\"),
                    _ => result.push(c),
                }
            }
            result.push('"');
            result
        }
        Value::List(items) => {
            let elements: Vec<String> = items.iter().map(json_stringify).collect();
            format!("[{}]", elements.join(", "))
        }
        Value::Record(fields) => {
            let pairs: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", k, json_stringify(v)))
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
        Value::Function(_) => "null".to_string(),
        Value::Builtin(_) => "null".to_string(),
        Value::Object(_, _) => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JSON has no literal for a number that is not finite. A host program
    /// embedding Redblue can still build one through the public `Value`, so the
    /// writer has to answer for it, and `null` is the answer every JSON writer
    /// gives. SPEC.md states this.
    #[test]
    fn edge_json_writes_a_number_that_is_not_finite_as_null() {
        for (value, what) in [
            (f64::NAN, "NaN"),
            (f64::INFINITY, "infinity"),
            (f64::NEG_INFINITY, "negative infinity"),
        ] {
            assert_eq!(
                json_stringify(&Value::Number(value)),
                "null",
                "{} must be written as null, not as a JSON number",
                what
            );
        }
        // The widest finite numbers are numbers, and must not be nulled.
        for finite in [f64::MAX, f64::MIN, f64::MIN_POSITIVE, 0.0, -0.0, 1.5] {
            let written = json_stringify(&Value::Number(finite));
            assert_ne!(written, "null", "{} should still be written", finite);
        }
        assert_eq!(json_stringify(&Value::Number(42.0)), "42");
        assert_eq!(json_stringify(&Value::Number(1.5)), "1.5");
    }
}
