use crate::error::{Error, Result, Span};
use crate::parser::{BinaryOp, Expr, Program, Statement, Stmt};
use crate::stdlib;
use std::collections::{HashMap, HashSet};

pub struct Analyzer {
    scopes: Vec<HashSet<String>>,
    functions: HashMap<String, Vec<String>>,
    errors: Vec<(String, Span)>,
    /// The names the program binds somewhere the walk may reach before the
    /// declaration that binds them. Collected before the statements are walked.
    later: LaterNames,
    /// The directory the program resolves its dependencies from: the one holding
    /// the `redblue.manifest` an `import` is answered by.
    ///
    /// The working directory for a program run as itself — [`analyze`] — and its
    /// own parameter for a program asked about from somewhere else, so that the
    /// manifest consulted is the one the program was found beside rather than
    /// whichever directory the host happens to be in.
    root: std::path::PathBuf,
    /// Whether the manifest in `root` could not be resolved, and so the walk
    /// stops where it found the import that found it.
    ///
    /// A program whose pins conflict has an unanswerable question at its every
    /// name: the import binds nothing, so every read of a name it would have
    /// bound says "unknown variable" and says nothing about why. Walking on turns
    /// one conflict into a page of consequences, and the reader goes to the page
    /// instead of the cause. The conflict is what the walk stops for.
    manifest_failed: bool,
    /// How many function, method and object bodies the walk is inside.
    ///
    /// A body runs when it is called, not where it is written, so a read inside
    /// one is answered by the program as it is at the call rather than as it was
    /// where the body sits. That is what makes a name a later declaration binds
    /// visible to a body declared above it, and it is why the fallback below
    /// applies here and nowhere else: a read at the top level, in the order the
    /// file writes it, is an error when the declaration has not been reached.
    deferred_depth: usize,
}

impl Analyzer {
    pub fn new() -> Self {
        Self::in_dir(std::path::Path::new("."))
    }

    /// An analyzer that resolves the program's imports against `root`.
    fn in_dir(root: &std::path::Path) -> Self {
        Self {
            scopes: vec![HashSet::new()],
            functions: HashMap::new(),
            errors: Vec::new(),
            later: LaterNames::default(),
            root: root.to_path_buf(),
            manifest_failed: false,
            deferred_depth: 0,
        }
    }

    /// An analyzer whose program scope already holds `names`.
    ///
    /// This is the entry point for a host that runs one program at a time and
    /// keeps the bindings between them — a REPL reading a line at a time. The
    /// VM is reused across those lines, so the *value* a line bound survives, but
    /// an analyzer that starts empty every time does not know the name exists and
    /// rejects the very next line: `set x to 1` then `say x` was
    /// `Unknown variable 'x'`, and no line could read anything an earlier line
    /// had bound — a variable, a `to`, an `import`ed module.
    ///
    /// The names go in the program scope ([`Analyzer::declare_in_program_scope`])
    /// because that is where a `set` a session made lands, so a later line's read
    /// and write of the name resolve the same way they would inside one program.
    pub fn with_bound_names(names: &[String]) -> Self {
        let mut analyzer = Self::new();
        for name in names {
            analyzer.declare_in_program_scope(name);
        }
        analyzer
    }

    pub fn analyze(&mut self, program: &Program) -> Result<()> {
        collect_later_names(&program.statements, &self.root, &mut self.later);

        for statement in &program.statements {
            self.analyze_statement(statement);
            if self.manifest_failed {
                break;
            }
        }

        if self.errors.is_empty() {
            Ok(())
        } else {
            let messages: Vec<String> = self.errors.iter().map(|(msg, _)| msg.clone()).collect();
            let span = self
                .errors
                .first()
                .map(|(_, span)| *span)
                .unwrap_or_else(Span::unknown);
            Err(Error::Analyzer(messages.join("\n"), span))
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    /// Declares a name a `set` binds, where the VM would put the write.
    ///
    /// A `set` writes to the innermost live scope that *already holds* the name
    /// and to the program's own scope otherwise — that is what `Vm::set_var`
    /// does, and it is what makes a read and a write of one name agree. So a
    /// name first set inside a loop, an `if` or a `try` body is a name of the
    /// whole program, because the write lands on a global and outlives the
    /// scope. The analyzer used to declare every `set` in the scope it was
    /// written in, which disagreed with the VM there and reported reading the
    /// name after the scope ended as an unknown variable.
    fn declare_assigned(&mut self, name: &str) {
        for scope in self.scopes.iter().rev() {
            if scope.contains(name) {
                return;
            }
        }
        self.declare_in_program_scope(name);
    }

    /// Declares a name a `constant` binds.
    ///
    /// The VM binds a constant to a global whatever scope the declaration is
    /// written in, so the analyzer does the same: `constant LIMIT to 5` inside
    /// a function body is a name of the whole program, and the second call of
    /// that body is the duplicate declaration.
    fn declare_constant(&mut self, name: &str) {
        self.declare_in_program_scope(name);
    }

    fn declare_in_program_scope(&mut self, name: &str) {
        if let Some(scope) = self.scopes.first_mut() {
            scope.insert(name.to_string());
        }
    }

    fn lookup(&self, name: &str) -> bool {
        self.scopes.iter().any(|scope| scope.contains(name))
    }

    /// Whether `name` is a name the program binds somewhere the walk has not
    /// reached, and so a name a body that runs later may read.
    ///
    /// This is the second of two questions a read asks: [`Analyzer::lookup`]
    /// answers whether a scope holds the name where the read is written, and
    /// this answers whether the program binds the name at all — a `constant`
    /// declared below the function body that reads it, or a name an `import`
    /// brings in from a module file. Consulted only inside a body that runs
    /// later, which [`Analyzer::deferred_depth`] counts; a name neither holds
    /// is the unknown variable the analyzer reports.
    fn bound_later(&self, name: &str) -> bool {
        self.later.bound(name)
    }

    /// Whether `name` is readable where the walk is.
    ///
    /// True when a scope holds it, and — inside a body that runs when it is
    /// called rather than where it is written — when the program binds it later
    /// than the walk has reached.
    fn name_is_bound(&self, name: &str) -> bool {
        self.lookup(name)
            || self.builtin_global(name)
            || (self.deferred_depth > 0 && self.bound_later(name))
    }

    /// Whether `name` is a builtin global that the program has not shadowed.
    ///
    /// A builtin is bound by definition — `stdlib::builtins` puts it in scope for
    /// every program — so a *method receiver* naming one is not an unknown
    /// variable, which is what `PI.times(..)` and SPEC.md's Properties example
    /// need and what the bare-name path already allowed.
    ///
    /// The shadowing test is what keeps this honest. A program may declare its
    /// own `constant PI`, and reading that name *before* the declaration is an
    /// error the language means: `edge_constant_used_before_declaration_is_an_error`
    /// pins it. So a builtin global only answers when nothing in the program has
    /// claimed the name, and a claimed one is left to the ordinary scope rules.
    fn builtin_global(&self, name: &str) -> bool {
        if !(stdlib::is_builtin(name) || stdlib::is_global(name)) {
            return false;
        }
        !self.shadows(name)
    }

    /// Whether this program declares a binding of its own called `name`.
    ///
    /// `LaterNames` is the set of every `constant` and every name an `import`
    /// brings in, collected before the walk begins — which is exactly "this
    /// program claims the name somewhere".
    fn shadows(&self, name: &str) -> bool {
        self.later.bound(name)
    }

    fn add_error(&mut self, msg: &str, span: Span) {
        self.errors.push((msg.to_string(), span));
    }

    /// Analyzes the body of a function, method or object, where a read is
    /// answered by the program as it is at the call rather than as it was where
    /// the body is written.
    fn analyze_deferred_body(&mut self, body: &[Stmt]) {
        self.deferred_depth += 1;
        for stmt in body {
            self.analyze_statement(stmt);
        }
        self.deferred_depth -= 1;
    }

    fn analyze_statement(&mut self, stmt: &Stmt) {
        let span = stmt.span;
        match &stmt.statement {
            Statement::Say(expr) | Statement::Print(expr) => {
                self.analyze_expr(expr, &span);
            }
            Statement::Set { name, value } => {
                self.analyze_expr(value, &span);
                self.declare_assigned(name);
            }
            // A `constant` binds its name for every statement after it, exactly
            // as a `set` does — whether the name may be bound again is a runtime
            // question, because a function body and a module file can each
            // declare one without either being visible here.
            Statement::Constant { name, value } => {
                self.analyze_expr(value, &span);
                self.declare_constant(name);
            }
            Statement::SetProperty {
                object: _,
                property: _,
                value,
            } => {
                self.analyze_expr(value, &span);
                // Property access is valid if object exists
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.analyze_expr(condition, &span);
                self.push_scope();
                for stmt in then_branch {
                    self.analyze_statement(stmt);
                }
                self.pop_scope();
                self.push_scope();
                for stmt in else_branch {
                    self.analyze_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::Unless { condition, body } => {
                self.analyze_expr(condition, &span);
                self.push_scope();
                for stmt in body {
                    self.analyze_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::ForEach {
                variable,
                iterable,
                body,
            } => {
                self.analyze_expr(iterable, &span);
                self.push_scope();
                self.declare(variable);
                for stmt in body {
                    self.analyze_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::ForRange {
                variable,
                start,
                end,
                step,
                body,
            } => {
                self.analyze_expr(start, &span);
                self.analyze_expr(end, &span);
                if let Some(s) = step {
                    self.analyze_expr(s, &span);
                }
                self.push_scope();
                self.declare(variable);
                for stmt in body {
                    self.analyze_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::Repeat { count, body } => {
                self.analyze_expr(count, &span);
                self.push_scope();
                for stmt in body {
                    self.analyze_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::While { condition, body } => {
                self.analyze_expr(condition, &span);
                self.push_scope();
                for stmt in body {
                    self.analyze_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::RepeatUntil {
                body, condition, ..
            } => {
                // The body runs before the condition is read, so it is walked
                // first. A name the body binds is therefore a name the condition
                // may read — the write lands in the program's own scope, which
                // outlives this body, and both VMs read the condition after the
                // body has run. Walking the condition first reported
                // `repeat / set t to 1 / until t is 1` as an unknown variable,
                // refusing a program the language runs on both engines.
                self.push_scope();
                for stmt in body {
                    self.analyze_statement(stmt);
                }
                self.pop_scope();
                self.analyze_expr(condition, &span);
            }
            Statement::Break | Statement::Skip => {}
            Statement::Return(expr) | Statement::GiveBack(expr) => {
                if let Some(e) = expr {
                    self.analyze_expr(e, &span);
                }
            }
            Statement::Function { name, params, body } => {
                self.declare(name);
                self.functions.insert(name.clone(), params.clone());
                self.push_scope();
                for param in params {
                    self.declare(param);
                }
                self.analyze_deferred_body(body);
                self.pop_scope();
            }
            Statement::Method {
                name: _,
                params,
                body,
            } => {
                self.push_scope();
                self.declare("this");
                for param in params {
                    self.declare(param);
                }
                self.analyze_deferred_body(body);
                self.pop_scope();
            }
            Statement::Has { default, .. } => {
                if let Some(expr) = default {
                    self.analyze_expr(expr, &span);
                }
            }
            Statement::Object {
                name,
                extends,
                body,
            } => {
                self.declare(name);
                if let Some(parent) = extends {
                    if !self.lookup(parent) {
                        self.add_error(&format!("Unknown parent object '{}'", parent), span);
                    }
                }
                self.push_scope();
                self.declare("this");
                self.analyze_deferred_body(body);
                self.pop_scope();
            }
            Statement::Try {
                body,
                catch_var,
                catch_body,
                finally_body,
            } => {
                for stmt in body {
                    self.analyze_statement(stmt);
                }
                if let Some(var) = catch_var {
                    self.push_scope();
                    self.declare(var);
                    for stmt in catch_body {
                        self.analyze_statement(stmt);
                    }
                    self.pop_scope();
                }
                for stmt in finally_body {
                    self.analyze_statement(stmt);
                }
            }
            Statement::Import(items) => {
                // An import binds the module's names into the program it runs
                // in, so it declares them into the scope it is written in, as
                // any other declaration does: a read after the import is an
                // ordinary lookup, and a read before it is the unknown variable
                // any other name read too early gives. A module's names are only
                // known by reading its file, so they are parsed here — a module
                // that cannot be found or parsed binds nothing and is still
                // reported by the loader, at runtime, where it belongs.
                //
                // A manifest that cannot be resolved is the exception. It is
                // about the program rather than about one module, so every name
                // the import would have bound is missing and every read of one of
                // them answers "unknown variable" — a message about a variable,
                // for a program whose real problem is that its pins conflict.
                // The conflict is what the user has to be shown, and it is named
                // here rather than left to a read that may never happen.
                if let Some(failure) = crate::manifest::failure_in(&self.root) {
                    self.add_error(&failure, span);
                    self.manifest_failed = true;
                    return;
                }
                for item in items {
                    for name in crate::interpreter::module_bound_names_in(&self.root, &item.name) {
                        self.declare(&name);
                    }
                    // Both the name the import gives the module and the alias
                    // it may give it under are names of the program: an
                    // import's own name is readable (`import json as J` leaves
                    // `json.parse` working), so both are declared.
                    self.declare(&item.name);
                    if let Some(alias) = &item.alias {
                        self.declare(alias);
                    }
                }
            }
            // A module declaration is a boundary: its body is analyzed in a
            // scope of its own, so a `set` inside a module is not a name the
            // importing program reads, and an `export` publishes a name rather
            // than declaring one here.
            Statement::Module { name, body } => {
                self.declare(name);
                self.push_scope();
                for stmt in body {
                    self.analyze_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::Export { .. } => {}
            Statement::Expr(expr) => {
                self.analyze_expr(expr, &span);
            }
            Statement::Test { body, .. } => {
                for stmt in body {
                    self.analyze_statement(stmt);
                }
            }
        }
    }

    fn analyze_expr(&mut self, expr: &Expr, span: &Span) {
        match expr {
            Expr::Number(_) | Expr::Text(_) | Expr::YesNo(_) | Expr::Nothing => {}
            Expr::Variable(name) => {
                if !self.name_is_bound(name) {
                    self.add_error(&format!("Unknown variable '{}'", name), *span);
                }
            }
            Expr::Binary { op, left, right } => {
                self.check_binary_op(op, left, right);
                self.analyze_expr(left, span);
                self.analyze_expr(right, span);
            }
            Expr::Unary { op: _, expr } => {
                self.analyze_expr(expr, span);
            }
            Expr::Call { name: _, args } => {
                for arg in args {
                    self.analyze_expr(arg, span);
                }
                // Function existence is checked at runtime for builtins
            }
            Expr::Property {
                object,
                property: _,
            } => {
                self.analyze_expr(object, span);
            }
            Expr::MethodCall {
                receiver,
                method: _,
                args,
            } => {
                match &**receiver {
                    // A bare receiver may be a module rather than a variable —
                    // `json.parse` is a module function — so it is an unknown
                    // variable only when it is neither in scope nor a module.
                    Expr::Variable(name)
                        if !self.name_is_bound(name) && !stdlib::is_module(name) =>
                    {
                        self.add_error(&format!("Unknown variable '{}'", name), *span);
                    }
                    Expr::Variable(_) => {}
                    other => self.analyze_expr(other, span),
                }
                for arg in args {
                    self.analyze_expr(arg, span);
                }
            }
            Expr::Index { object, index } => {
                self.analyze_expr(object, span);
                self.analyze_expr(index, span);
            }
            Expr::InterpolatedText(parts) => {
                for part in parts {
                    self.analyze_expr(part, span);
                }
            }
            Expr::List(items) => {
                for item in items {
                    self.analyze_expr(item, span);
                }
            }
            Expr::Record(fields) => {
                for (_, value) in fields {
                    self.analyze_expr(value, span);
                }
            }
            // A literal's body is checked in a scope of its own, so a name it
            // assigns or reads is accounted for there rather than in the
            // enclosing statement's scope.
            Expr::FunctionLiteral { params, body } => {
                self.push_scope();
                for param in params {
                    self.declare(param);
                }
                self.analyze_deferred_body(body);
                self.pop_scope();
            }
            Expr::MightFail(inner) => self.analyze_expr(inner, span),
            Expr::Expect { .. } => {}
        }
    }

    fn check_binary_op(&mut self, op: &BinaryOp, _left: &Expr, _right: &Expr) {
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                // Arithmetic operations require numbers
            }
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual => {
                // Comparison operations
            }
            BinaryOp::And | BinaryOp::Or => {
                // Logical operations
            }
            BinaryOp::In => {
                // Membership test
            }
        }
    }
}

impl Default for Analyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// The names a program binds in a declaration the walk may not have reached
/// when it reads one of them.
///
/// Collected over the whole program before the walk, and read by
/// [`Analyzer::bound_later`], which consults it only inside a body that runs
/// later. The walk covers every body at every depth: a `constant` or an
/// `import` inside a loop, an `if`, a function or a `try` binds the same
/// program-level name every time that runs, so one of them anywhere counts.
#[derive(Default)]
struct LaterNames {
    /// Every name a `constant` declares.
    constants: HashSet<String>,
    /// Every name an `import` brings in, read out of the module files.
    imported: HashSet<String>,
}

impl LaterNames {
    fn bound(&self, name: &str) -> bool {
        self.constants.contains(name) || self.imported.contains(name)
    }
}

fn collect_later_names(statements: &[Stmt], root: &std::path::Path, out: &mut LaterNames) {
    for stmt in statements {
        match &stmt.statement {
            Statement::Constant { name, .. } => {
                out.constants.insert(name.clone());
            }
            Statement::Import(items) => {
                for item in items {
                    out.imported
                        .extend(crate::interpreter::module_bound_names_in(root, &item.name));
                }
            }
            Statement::If {
                then_branch,
                else_branch,
                ..
            } => {
                collect_later_names(then_branch, root, out);
                collect_later_names(else_branch, root, out);
            }
            Statement::Unless { body, .. } => {
                collect_later_names(body, root, out);
            }
            Statement::ForEach { body, .. }
            | Statement::ForRange { body, .. }
            | Statement::Repeat { body, .. }
            | Statement::RepeatUntil { body, .. }
            | Statement::While { body, .. } => collect_later_names(body, root, out),
            Statement::Function { body, .. }
            | Statement::Method { body, .. }
            | Statement::Object { body, .. }
            | Statement::Module { body, .. }
            | Statement::Test { body, .. } => collect_later_names(body, root, out),
            Statement::Try {
                body,
                catch_body,
                finally_body,
                ..
            } => {
                collect_later_names(body, root, out);
                collect_later_names(catch_body, root, out);
                collect_later_names(finally_body, root, out);
            }
            Statement::Say(_)
            | Statement::Print(_)
            | Statement::Set { .. }
            | Statement::SetProperty { .. }
            | Statement::Return(_)
            | Statement::GiveBack(_)
            | Statement::Break
            | Statement::Skip
            | Statement::Has { .. }
            | Statement::Export { .. }
            | Statement::Expr(_) => {}
        }
    }
}

pub fn analyze(program: &Program) -> Result<()> {
    analyze_in(program, std::path::Path::new("."))
}

/// Analyzes `program` as the program in `root` resolves its dependencies from.
///
/// `analyze` is this against the working directory. A `root` of its own is what
/// lets the pins that decide what an `import` binds be the pins of the directory
/// the program was found beside — and, when those pins cannot be resolved, what
/// lets the conflict be reported here rather than surfacing later as an unknown
/// variable with nothing in it saying why.
pub fn analyze_in(program: &Program, root: &std::path::Path) -> Result<()> {
    let mut analyzer = Analyzer::in_dir(root);
    analyzer.analyze(program)
}

/// Analyzes `program` as the next line of a session that has already bound
/// `names`.
///
/// The REPL runs each line as a program of its own, so this is how the lines
/// before it stay readable to it. See [`Analyzer::with_bound_names`].
pub fn analyze_with_bound_names(program: &Program, names: &[String]) -> Result<()> {
    Analyzer::with_bound_names(names).analyze(program)
}

#[cfg(test)]
mod tests {
    use super::{analyze, analyze_with_bound_names};
    use crate::lexer::Lexer;
    use crate::parser::{parse, Program};

    fn program(source: &str) -> Program {
        parse(Lexer::tokenize(source).expect("the source should lex"))
            .expect("the source should parse")
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_string()).collect()
    }

    /// The entry point a REPL depends on: a name an earlier line bound is
    /// readable on this one. Without it every session rejected its own bindings
    /// on the next line — `set x to 1` then `say x` was `Unknown variable 'x'`.
    #[test]
    fn a_bound_name_is_readable_by_the_next_program() {
        let outcome = analyze_with_bound_names(&program("say x"), &names(&["x"]));

        assert!(
            outcome.is_ok(),
            "a name the session already bound is readable: {:?}",
            outcome
        );
    }

    /// Seeding a scope must not make the analyzer accept everything: a name the
    /// session never bound is still unknown, or the check would be off rather
    /// than carried across lines.
    #[test]
    fn edge_a_name_the_session_never_bound_is_still_unknown() {
        let outcome = analyze_with_bound_names(&program("say other"), &names(&["x"]));

        assert!(
            outcome.is_err(),
            "seeding the session's names does not turn the analyzer off"
        );
    }

    /// The seed lives in the program scope, which is where a `set` writes, so
    /// rebinding a name the session bound is the plain reassignment it would be
    /// inside one program — not a shadowing that a later read cannot see.
    #[test]
    fn edge_a_name_the_session_bound_can_be_assigned_again() {
        let outcome = analyze_with_bound_names(&program("set x to 2\nsay x"), &names(&["x"]));

        assert!(
            outcome.is_ok(),
            "reassigning a session name is not an error: {:?}",
            outcome
        );
    }

    /// The whole point is that the two lines pass the analyzer *separately*,
    /// exactly as the REPL runs them.
    #[test]
    fn two_programs_of_one_session_both_pass() {
        let first = program("set x to 1");
        let second = program("say x");

        assert!(
            analyze(&first).is_ok(),
            "the line that binds the name is a whole program"
        );
        assert!(
            analyze_with_bound_names(&second, &names(&["x"])).is_ok(),
            "and the line that reads it is the next one"
        );
    }
}
