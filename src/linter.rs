use crate::error::{Error, Span};
use crate::lexer::Lexer;
use crate::parser::{Expr, Program, Statement, Stmt};
use std::collections::HashMap;
use std::collections::HashSet;

/// The linter walks a parsed program and reports what a reader would already
/// suspect: a variable that is never read, a file-level import that the file
/// never uses, a binding that hides one of the same name further out, and
/// source that does not parse at all.
///
/// It reports only what it can prove from the syntax tree. Anything it cannot
/// decide is left alone: a warning that is wrong costs more than a warning that
/// is missing, because `rb lint` is trusted by the people who read its output.
pub struct Linter {
    errors: Vec<LintError>,
    warnings: Vec<LintWarning>,
    defined_vars: HashSet<String>,
    definition_lines: HashMap<String, usize>,
    used_vars: HashSet<String>,
    defined_functions: HashSet<String>,
    imports: Vec<Import>,
    scopes: Vec<HashSet<String>>,
}

/// A file-level `import`, with every name the file can reach the module
/// through: `import files` binds `files`, `import files as f` binds `f` and
/// leaves `files` bound too.
#[derive(Clone)]
struct Import {
    names: Vec<String>,
    line: usize,
}

#[derive(Debug, Clone)]
pub struct LintError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone)]
pub struct LintWarning {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl Linter {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            defined_vars: HashSet::new(),
            definition_lines: HashMap::new(),
            used_vars: HashSet::new(),
            defined_functions: HashSet::new(),
            imports: Vec::new(),
            scopes: vec![HashSet::new()],
        }
    }

    pub fn lint(&mut self, program: &Program) {
        for stmt in &program.statements {
            self.analyze_statement(stmt);
        }

        let mut unused: Vec<(&String, usize)> = self
            .defined_vars
            .iter()
            .filter(|var| !self.used_vars.contains(*var) && !var.starts_with('_'))
            .map(|var| (var, self.definition_lines.get(var).copied().unwrap_or(0)))
            .collect();
        // `HashSet` iteration order is not stable, so findings are sorted
        // before they are reported: two runs over one file must print the same
        // warnings in the same order.
        unused.sort_by_key(|(name, line)| (*line, (*name).clone()));
        for (var, line) in unused {
            self.warnings.push(LintWarning {
                message: format!("Unused variable: '{}'", var),
                line,
                column: 1,
            });
        }

        for import in self.imports.clone() {
            if import
                .names
                .iter()
                .all(|name| !self.used_vars.contains(name))
            {
                let name = &import.names[0];
                self.warnings.push(LintWarning {
                    message: format!("Unused import: '{}'", name),
                    line: import.line,
                    column: 1,
                });
            }
        }

        self.warnings
            .sort_by_key(|warning| (warning.line, warning.column, warning.message.clone()));
    }

    fn warn(&mut self, message: String, span: Span) {
        self.warnings.push(LintWarning {
            message,
            line: span.line,
            column: span.column,
        });
    }

    /// Records an ordinary assignment. `set x to ...` is assignment and not
    /// declaration, so re-assigning a name — `set n to n + 1` inside a loop,
    /// say — introduces no new binding and shadows nothing.
    fn assign(&mut self, name: &str, span: Span) {
        self.track(name, span);
    }

    /// Records a binding that introduces a name of its own, such as a loop
    /// variable. Unlike [`Linter::assign`] it is checked against the
    /// enclosing scopes for shadowing.
    fn declare(&mut self, name: &str, span: Span) {
        if self.shadows_outer(name) {
            let message = format!("Variable '{}' shadows an outer variable", name);
            self.warn(message, span);
        }
        self.track(name, span);
    }

    /// Records a name that is part of a signature — a function parameter or a
    /// `catch` binding. It is shadow-checked, but it is not an unused
    /// variable: an unread parameter describes the callable, it is not a
    /// leftover from an abandoned computation.
    fn bind_signature(&mut self, name: &str, span: Span) {
        if self.shadows_outer(name) {
            let message = format!("Parameter '{}' shadows an outer variable", name);
            self.warn(message, span);
        }
        self.bind_local(name);
    }

    /// Adds a name to the innermost scope without tracking it for the unused
    /// check.
    fn bind_local(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    fn track(&mut self, name: &str, span: Span) {
        self.bind_local(name);
        self.defined_vars.insert(name.to_string());
        self.definition_lines
            .entry(name.to_string())
            .or_insert(span.line);
    }

    /// True when an enclosing scope — any scope but the innermost one — has
    /// already bound this name.
    fn shadows_outer(&self, name: &str) -> bool {
        let outer = self.scopes.len().saturating_sub(1);
        self.scopes[..outer]
            .iter()
            .any(|scope| scope.contains(name))
    }

    /// Analyses a loop body. The loop variable belongs to the body's own scope,
    /// so it is checked against every enclosing scope for shadowing.
    fn analyze_loop_body(&mut self, variable: &str, span: Span, body: &[Stmt]) {
        self.scopes.push(HashSet::new());
        self.declare(variable, span);
        self.analyze_body_in_current_scope(body);
    }

    /// Analyses a block of statements, whose parameters and loop variables
    /// belong to a scope of its own.
    fn analyze_body(&mut self, body: &[Stmt]) {
        self.scopes.push(HashSet::new());
        self.analyze_body_in_current_scope(body);
    }

    fn analyze_body_in_current_scope(&mut self, body: &[Stmt]) {
        for stmt in body {
            self.analyze_statement(stmt);
        }
        self.scopes.pop();
    }

    fn analyze_callable(&mut self, params: &[String], body: &[Stmt], span: Span) {
        self.scopes.push(HashSet::new());
        for param in params {
            self.bind_signature(param, span);
        }
        self.analyze_body_in_current_scope(body);
    }

    fn use_name(&mut self, name: &str) {
        self.used_vars.insert(name.to_string());
    }

    fn analyze_statement(&mut self, stmt: &Stmt) {
        let span = stmt.span;
        match &stmt.statement {
            Statement::Set { name, value } => {
                self.assign(name, span);
                self.analyze_expr(value);
            }
            Statement::SetProperty { object, value, .. } => {
                // `set record.field to ...` writes a field of a record. The
                // record is read here; the field is not a variable binding, so
                // it can never be an unused variable.
                self.use_name(object);
                self.analyze_expr(value);
            }
            Statement::Say(expr) | Statement::Print(expr) => {
                self.analyze_expr(expr);
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.analyze_expr(condition);
                self.analyze_body(then_branch);
                self.analyze_body(else_branch);
            }
            Statement::ForEach {
                variable,
                iterable,
                body,
            } => {
                self.analyze_expr(iterable);
                self.analyze_loop_body(variable, span, body);
            }
            Statement::ForRange {
                variable,
                start,
                end,
                step,
                body,
            } => {
                self.analyze_expr(start);
                self.analyze_expr(end);
                if let Some(s) = step {
                    self.analyze_expr(s);
                }
                self.analyze_loop_body(variable, span, body);
            }
            Statement::Repeat { count, body } => {
                self.analyze_expr(count);
                self.analyze_body(body);
            }
            Statement::While { condition, body } => {
                self.analyze_expr(condition);
                self.analyze_body(body);
            }
            Statement::Break | Statement::Skip => {}
            Statement::Return(expr) | Statement::GiveBack(expr) => {
                if let Some(e) = expr {
                    self.analyze_expr(e);
                }
            }
            Statement::Function {
                name, params, body, ..
            } => {
                self.defined_functions.insert(name.clone());
                self.analyze_callable(params, body, span);
            }
            Statement::Method { params, body, .. } => {
                self.analyze_callable(params, body, span);
            }
            Statement::Has { default, .. } => {
                if let Some(expr) = default {
                    self.analyze_expr(expr);
                }
            }
            Statement::Object {
                name,
                extends: _,
                body,
            } => {
                self.declare(name, span);
                self.analyze_body(body);
            }
            Statement::Try {
                body,
                catch_var,
                catch_body,
                finally_body,
            } => {
                self.analyze_body(body);
                self.scopes.push(HashSet::new());
                if let Some(var) = catch_var {
                    self.bind_signature(var, span);
                }
                self.analyze_body_in_current_scope(catch_body);
                self.analyze_body(finally_body);
            }
            Statement::Import(items) => {
                // Only a file-level import is a dependency of the file. An
                // import inside a `try`, a loop or a `test` block is a
                // statement run for what it does there — a test that imports a
                // module to prove it resolves is not carrying an unused
                // dependency.
                if self.scopes.len() == 1 {
                    for item in items {
                        let mut names = vec![item.name.clone()];
                        if let Some(alias) = &item.alias {
                            names.insert(0, alias.clone());
                        }
                        self.imports.push(Import {
                            names,
                            line: span.line,
                        });
                    }
                }
            }
            Statement::Test { body, .. } => {
                self.analyze_body(body);
            }
            Statement::Expr(expr) => {
                self.analyze_expr(expr);
            }
        }
    }

    fn analyze_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Number(_) | Expr::Text(_) | Expr::YesNo(_) | Expr::Nothing => {}
            Expr::Variable(name) => {
                self.use_name(name);
            }
            Expr::Binary { left, right, .. } => {
                self.analyze_expr(left);
                self.analyze_expr(right);
            }
            Expr::Unary { expr, .. } => {
                self.analyze_expr(expr);
            }
            Expr::Call { name, args } => {
                self.use_name(name);
                for arg in args {
                    self.analyze_expr(arg);
                }
            }
            Expr::Property { object, .. } => {
                self.analyze_expr(object);
            }
            Expr::MethodCall { receiver, args, .. } => {
                self.analyze_expr(receiver);
                for arg in args {
                    self.analyze_expr(arg);
                }
            }
            Expr::Index { object, index } => {
                self.analyze_expr(object);
                self.analyze_expr(index);
            }
            Expr::InterpolatedText(parts) => {
                for part in parts {
                    self.analyze_expr(part);
                }
            }
            Expr::List(items) => {
                for item in items {
                    self.analyze_expr(item);
                }
            }
            Expr::Record(fields) => {
                for (_, value) in fields {
                    self.analyze_expr(value);
                }
            }
            Expr::Expect { actual, expected } => {
                self.analyze_expr(actual);
                self.analyze_expr(expected);
            }
        }
    }

    pub fn get_errors(&self) -> Vec<LintError> {
        self.errors.clone()
    }

    pub fn get_warnings(&self) -> Vec<LintWarning> {
        self.warnings.clone()
    }
}

impl Default for Linter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn lint(source: &str) -> (Vec<LintError>, Vec<LintWarning>) {
    let tokens = match Lexer::tokenize(source) {
        Ok(t) => t,
        Err(e) => return (vec![syntax_error(&e)], Vec::new()),
    };

    let mut parser = crate::parser::Parser::new(tokens);
    let program = match parser.parse() {
        Ok(p) => p,
        Err(e) => return (vec![syntax_error(&e)], Vec::new()),
    };

    let mut linter = Linter::new();
    linter.lint(&program);

    (linter.get_errors(), linter.get_warnings())
}

/// Turns a failure from the lexer or the parser into a lint error, so that a
/// file which does not parse is reported instead of being silently passed over.
fn syntax_error(error: &Error) -> LintError {
    let rendered;
    let (message, span) = match error {
        Error::Lexer(message, span) | Error::Parser(message, span) => (message.as_str(), *span),
        other => {
            rendered = other.to_string();
            (rendered.as_str(), Span::unknown())
        }
    };

    LintError {
        message: format!("Syntax error: {}", message),
        line: span.line,
        column: span.column,
    }
}
