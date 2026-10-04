use crate::error::{Error, Result, Span};
use crate::parser::{BinaryOp, Expr, Program, Statement, Stmt};

pub struct Analyzer {
    scopes: Vec<std::collections::HashSet<String>>,
    functions: std::collections::HashMap<String, Vec<String>>,
    errors: Vec<(String, Span)>,
}

impl Analyzer {
    pub fn new() -> Self {
        Self {
            scopes: vec![std::collections::HashSet::new()],
            functions: std::collections::HashMap::new(),
            errors: Vec::new(),
        }
    }

    pub fn analyze(&mut self, program: &Program) -> Result<()> {
        for statement in &program.statements {
            self.analyze_statement(statement);
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
        self.scopes.push(std::collections::HashSet::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: &str) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string());
        }
    }

    fn lookup(&self, name: &str) -> bool {
        self.scopes.iter().any(|scope| scope.contains(name))
    }

    fn add_error(&mut self, msg: &str, span: Span) {
        self.errors.push((msg.to_string(), span));
    }

    fn analyze_statement(&mut self, stmt: &Stmt) {
        let span = stmt.span;
        match &stmt.statement {
            Statement::Say(expr) | Statement::Print(expr) => {
                self.analyze_expr(expr, &span);
            }
            Statement::Set { name, value } => {
                self.analyze_expr(value, &span);
                self.declare(name);
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
                for stmt in body {
                    self.analyze_statement(stmt);
                }
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
                for stmt in body {
                    self.analyze_statement(stmt);
                }
                self.pop_scope();
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
                for stmt in body {
                    self.analyze_statement(stmt);
                }
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
            Statement::Import(_) => {}
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
                if !self.lookup(name) {
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

pub fn analyze(program: &Program) -> Result<()> {
    let mut analyzer = Analyzer::new();
    analyzer.analyze(program)
}
