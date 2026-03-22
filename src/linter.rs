use crate::error::{Error, Result};
use crate::parser::{Program, Statement, Expr, BinaryOp, UnaryOp, ImportItem};
use std::collections::HashSet;

pub struct Linter {
    errors: Vec<LintError>,
    warnings: Vec<LintWarning>,
    defined_vars: HashSet<String>,
    used_vars: HashSet<String>,
    defined_functions: HashSet<String>,
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
            used_vars: HashSet::new(),
            defined_functions: HashSet::new(),
        }
    }
    
    pub fn lint(&mut self, program: &Program) {
        for stmt in &program.statements {
            self.analyze_statement(stmt);
        }
        
        // Check for unused variables
        for var in &self.defined_vars {
            if !self.used_vars.contains(var) && !var.starts_with('_') {
                self.warnings.push(LintWarning {
                    message: format!("Unused variable: '{}'", var),
                    line: 0,
                    column: 0,
                });
            }
        }
    }
    
    fn analyze_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Set { name, value } => {
                self.defined_vars.insert(name.clone());
                self.analyze_expr(value);
            },
            Statement::SetProperty { object, property, value } => {
                self.analyze_expr(object);
                self.defined_vars.insert(format!("{}.{}", object, property));
                self.analyze_expr(value);
            },
            Statement::Say(expr) | Statement::Print(expr) => {
                self.analyze_expr(expr);
            },
            Statement::If { condition, then_branch, else_branch } => {
                self.analyze_expr(condition);
                for s in then_branch {
                    self.analyze_statement(s);
                }
                for s in else_branch {
                    self.analyze_statement(s);
                }
            },
            Statement::ForEach { variable, iterable, body } => {
                self.defined_vars.insert(variable.clone());
                self.analyze_expr(iterable);
                for s in body {
                    self.analyze_statement(s);
                }
            },
            Statement::ForRange { variable, start, end, step, body } => {
                self.defined_vars.insert(variable.clone());
                self.analyze_expr(start);
                self.analyze_expr(end);
                if let Some(s) = step {
                    self.analyze_expr(s);
                }
                for st in body {
                    self.analyze_statement(st);
                }
            },
            Statement::Repeat { count, body } => {
                self.analyze_expr(count);
                for s in body {
                    self.analyze_statement(s);
                }
            },
            Statement::While { condition, body } => {
                self.analyze_expr(condition);
                for s in body {
                    self.analyze_statement(s);
                }
            },
            Statement::Break | Statement::Skip => {},
            Statement::Return(expr) | Statement::GiveBack(expr) => {
                if let Some(e) = expr {
                    self.analyze_expr(e);
                }
            },
            Statement::Function { name, params, body } => {
                self.defined_functions.insert(name.clone());
                for param in params {
                    self.defined_vars.insert(param.clone());
                }
                for s in body {
                    self.analyze_statement(s);
                }
            },
            Statement::Method { name, params, body } => {
                for param in params {
                    self.defined_vars.insert(param.clone());
                }
                for s in body {
                    self.analyze_statement(s);
                }
            },
            Statement::Object { name, extends, body } => {
                self.defined_vars.insert(name.clone());
                for s in body {
                    self.analyze_statement(s);
                }
            },
            Statement::Try { body, catch_body, finally_body, .. } => {
                for s in body {
                    self.analyze_statement(s);
                }
                for s in catch_body {
                    self.analyze_statement(s);
                }
                for s in finally_body {
                    self.analyze_statement(s);
                }
            },
            Statement::Import(_) => {},
            Statement::Test { body, .. } => {
                for s in body {
                    self.analyze_statement(s);
                }
            },
            Statement::Expr(expr) => {
                self.analyze_expr(expr);
            },
        }
    }
    
    fn analyze_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Number(_) | Expr::Text(_) | Expr::YesNo(_) | Expr::Nothing => {},
            Expr::Variable(name) => {
                self.used_vars.insert(name.clone());
            },
            Expr::Binary { op: _, left, right } => {
                self.analyze_expr(left);
                self.analyze_expr(right);
            },
            Expr::Unary { op: _, expr } => {
                self.analyze_expr(expr);
            },
            Expr::Call { name, args } => {
                self.used_vars.insert(name.clone());
                for arg in args {
                    self.analyze_expr(arg);
                }
            },
            Expr::Property { object, property: _ } => {
                self.analyze_expr(object);
            },
            Expr::Index { object, index } => {
                self.analyze_expr(object);
                self.analyze_expr(index);
            },
            Expr::InterpolatedText(parts) => {
                for part in parts {
                    self.analyze_expr(part);
                }
            },
            Expr::List(items) => {
                for item in items {
                    self.analyze_expr(item);
                }
            },
            Expr::Record(fields) => {
                for (_, value) in fields {
                    self.analyze_expr(value);
                }
            },
            Expr::Expect { actual, expected } => {
                self.analyze_expr(actual);
                self.analyze_expr(expected);
            },
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
    let tokens = match crate::lexer::tokenize(source) {
        Ok(t) => t,
        Err(_) => return (Vec::new(), Vec::new()),
    };
    
    let parser = crate::parser::Parser::new(tokens);
    let program = match parser.parse() {
        Ok(p) => p,
        Err(_) => return (Vec::new(), Vec::new()),
    };
    
    let mut linter = Linter::new();
    linter.lint(&program);
    
    (linter.get_errors(), linter.get_warnings())
}