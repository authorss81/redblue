use crate::error::{Error, Result};
use crate::parser::{BinaryOp, Expr, Program, Statement};

pub struct Analyzer {
    scopes: Vec<std::collections::HashSet<String>>,
    functions: std::collections::HashMap<String, Vec<String>>,
    errors: Vec<String>,
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
            Err(Error::Analyzer(self.errors.join("\n")))
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

    fn add_error(&mut self, msg: &str) {
        self.errors.push(msg.to_string());
    }

    fn analyze_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Say(expr) | Statement::Print(expr) => {
                self.analyze_expr(expr);
            }
            Statement::Set { name, value } => {
                self.analyze_expr(value);
                self.declare(name);
            }
            Statement::SetProperty {
                object: _,
                property: _,
                value,
            } => {
                self.analyze_expr(value);
                // Property access is valid if object exists
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.analyze_expr(condition);
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
                self.analyze_expr(iterable);
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
                self.analyze_expr(start);
                self.analyze_expr(end);
                if let Some(s) = step {
                    self.analyze_expr(s);
                }
                self.push_scope();
                self.declare(variable);
                for stmt in body {
                    self.analyze_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::Repeat { count, body } => {
                self.analyze_expr(count);
                self.push_scope();
                for stmt in body {
                    self.analyze_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::While { condition, body } => {
                self.analyze_expr(condition);
                self.push_scope();
                for stmt in body {
                    self.analyze_statement(stmt);
                }
                self.pop_scope();
            }
            Statement::Break | Statement::Skip => {}
            Statement::Return(expr) | Statement::GiveBack(expr) => {
                if let Some(e) = expr {
                    self.analyze_expr(e);
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
                        self.add_error(&format!("Unknown parent object '{}'", parent));
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
                self.analyze_expr(expr);
            }
            Statement::Test { body, .. } => {
                for stmt in body {
                    self.analyze_statement(stmt);
                }
            }
        }
    }

    fn analyze_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Number(_) | Expr::Text(_) | Expr::YesNo(_) | Expr::Nothing => {}
            Expr::Variable(name) => {
                if !self.lookup(name) {
                    self.add_error(&format!("Unknown variable '{}'", name));
                }
            }
            Expr::Binary { op, left, right } => {
                self.check_binary_op(op, left, right);
                self.analyze_expr(left);
                self.analyze_expr(right);
            }
            Expr::Unary { op: _, expr } => {
                self.analyze_expr(expr);
            }
            Expr::Call { name: _, args } => {
                for arg in args {
                    self.analyze_expr(arg);
                }
                // Function existence is checked at runtime for builtins
            }
            Expr::Property {
                object,
                property: _,
            } => {
                self.analyze_expr(object);
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
