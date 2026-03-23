use crate::lexer::Lexer;
use crate::parser::{BinaryOp, Expr, Program, Statement, UnaryOp};

pub struct Formatter {
    indent: usize,
    output: String,
}

impl Formatter {
    pub fn new() -> Self {
        Self {
            indent: 0,
            output: String::new(),
        }
    }

    pub fn format(&mut self, source: &str) -> Result<String, String> {
        let tokens = Lexer::tokenize(source).map_err(|e| format!("Lexer error: {}", e))?;

        let mut parser = crate::parser::Parser::new(tokens);
        let program = parser.parse().map_err(|e| format!("Parser error: {}", e))?;

        self.format_program(&program);
        Ok(self.output.clone())
    }

    fn format_program(&mut self, program: &Program) {
        for (i, stmt) in program.statements.iter().enumerate() {
            if i > 0 {
                self.newline();
            }
            self.format_statement(stmt);
        }
    }

    fn format_statement(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Say(expr) => {
                self.write("say ");
                self.format_expression(expr);
            }
            Statement::Print(expr) => {
                self.write("print ");
                self.format_expression(expr);
            }
            Statement::Set { name, value } => {
                self.write("set ");
                self.write(name);
                self.write(" to ");
                self.format_expression(value);
            }
            Statement::SetProperty {
                object,
                property,
                value,
            } => {
                self.write("set ");
                self.write(object);
                self.write(".");
                self.write(property);
                self.write(" to ");
                self.format_expression(value);
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.write("if ");
                self.format_expression(condition);
                self.newline();
                self.indent();
                for stmt in then_branch {
                    self.format_statement(stmt);
                    self.newline();
                }
                self.dedent();

                if !else_branch.is_empty() {
                    self.write("else");
                    self.newline();
                    self.indent();
                    for stmt in else_branch {
                        self.format_statement(stmt);
                        self.newline();
                    }
                    self.dedent();
                }
                self.write("end");
            }
            Statement::ForEach {
                variable,
                iterable,
                body,
            } => {
                self.write("for each ");
                self.write(variable);
                self.write(" in ");
                self.format_expression(iterable);
                self.newline();
                self.indent();
                for stmt in body {
                    self.format_statement(stmt);
                    self.newline();
                }
                self.dedent();
                self.write("end");
            }
            Statement::ForRange {
                variable,
                start,
                end,
                step,
                body,
            } => {
                self.write("for each ");
                self.write(variable);
                self.write(" from ");
                self.format_expression(start);
                self.write(" to ");
                self.format_expression(end);
                if let Some(s) = step {
                    self.write(" by ");
                    self.format_expression(s);
                }
                self.newline();
                self.indent();
                for stmt in body {
                    self.format_statement(stmt);
                    self.newline();
                }
                self.dedent();
                self.write("end");
            }
            Statement::Repeat { count, body } => {
                self.write("repeat ");
                self.format_expression(count);
                self.write(" times");
                self.newline();
                self.indent();
                for stmt in body {
                    self.format_statement(stmt);
                    self.newline();
                }
                self.dedent();
                self.write("end");
            }
            Statement::While { condition, body } => {
                self.write("while ");
                self.format_expression(condition);
                self.newline();
                self.indent();
                for stmt in body {
                    self.format_statement(stmt);
                    self.newline();
                }
                self.dedent();
                self.write("end");
            }
            Statement::Break => {
                self.write("break");
            }
            Statement::Skip => {
                self.write("skip");
            }
            Statement::Return(expr) => {
                self.write("return");
                if let Some(e) = expr {
                    self.write(" ");
                    self.format_expression(e);
                }
            }
            Statement::GiveBack(expr) => {
                self.write("give back");
                if let Some(e) = expr {
                    self.write(" ");
                    self.format_expression(e);
                }
            }
            Statement::Function { name, params, body } => {
                self.write("to ");
                self.write(name);
                self.write("(");
                self.write(&params.join(", "));
                self.write(")");
                self.newline();
                self.indent();
                for stmt in body {
                    self.format_statement(stmt);
                    self.newline();
                }
                self.dedent();
                self.write("end");
            }
            Statement::Method { name, params, body } => {
                self.write("to can ");
                self.write(name);
                self.write("(");
                self.write(&params.join(", "));
                self.write(")");
                self.newline();
                self.indent();
                for stmt in body {
                    self.format_statement(stmt);
                    self.newline();
                }
                self.dedent();
                self.write("end");
            }
            Statement::Object {
                name,
                extends,
                body,
            } => {
                self.write("object ");
                self.write(name);
                if let Some(parent) = extends {
                    self.write(" extends ");
                    self.write(parent);
                }
                self.newline();
                self.indent();
                for stmt in body {
                    self.format_statement(stmt);
                    self.newline();
                }
                self.dedent();
                self.write("end");
            }
            Statement::Try {
                body,
                catch_var,
                catch_body,
                finally_body,
            } => {
                self.write("try");
                self.newline();
                self.indent();
                for stmt in body {
                    self.format_statement(stmt);
                    self.newline();
                }
                self.dedent();

                if let Some(var) = catch_var {
                    self.write("catch ");
                    self.write(var);
                    self.newline();
                    self.indent();
                    for stmt in catch_body {
                        self.format_statement(stmt);
                        self.newline();
                    }
                    self.dedent();
                }

                if !finally_body.is_empty() {
                    self.write("finally");
                    self.newline();
                    self.indent();
                    for stmt in finally_body {
                        self.format_statement(stmt);
                        self.newline();
                    }
                    self.dedent();
                }
                self.write("end");
            }
            Statement::Import(items) => {
                self.write("import ");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write(&item.name);
                    if let Some(alias) = &item.alias {
                        self.write(" to ");
                        self.write(alias);
                    }
                }
            }
            Statement::Test { name, body } => {
                self.write("test ");
                self.format_string_literal(name);
                self.newline();
                self.indent();
                for stmt in body {
                    self.format_statement(stmt);
                    self.newline();
                }
                self.dedent();
                self.write("end");
            }
            Statement::Expr(expr) => {
                self.format_expression(expr);
            }
        }
    }

    fn format_expression(&mut self, expr: &Expr) {
        match expr {
            Expr::Number(n) => self.write(&n.to_string()),
            Expr::Text(s) => self.format_string_literal(s),
            Expr::YesNo(b) => self.write(if *b { "yes" } else { "no" }),
            Expr::Nothing => self.write("nothing"),
            Expr::Variable(name) => self.write(name),
            Expr::Binary { op, left, right } => {
                self.format_expression(left);
                self.write(" ");
                self.format_binary_op(op);
                self.write(" ");
                self.format_expression(right);
            }
            Expr::Unary { op, expr } => {
                self.format_unary_op(op);
                self.write(" ");
                self.format_expression(expr);
            }
            Expr::Call { name, args } => {
                self.write(name);
                self.write("(");
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expression(arg);
                }
                self.write(")");
            }
            Expr::Property { object, property } => {
                self.format_expression(object);
                self.write(".");
                self.write(property);
            }
            Expr::Index { object, index } => {
                self.format_expression(object);
                self.write(" at ");
                self.format_expression(index);
            }
            Expr::InterpolatedText(parts) => {
                self.write("\"");
                for part in parts {
                    match part {
                        Expr::Text(s) => self.write(s),
                        _ => {
                            self.write("{");
                            self.format_expression(part);
                            self.write("}");
                        }
                    }
                }
                self.write("\"");
            }
            Expr::List(items) => {
                self.write("[");
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.format_expression(item);
                }
                self.write("]");
            }
            Expr::Record(fields) => {
                self.write("record {");
                for (i, (key, value)) in fields.iter().enumerate() {
                    if i > 0 {
                        self.write(", ");
                    }
                    self.write(key);
                    self.write(": ");
                    self.format_expression(value);
                }
                self.write("}");
            }
            Expr::Expect { actual, expected } => {
                self.write("expect ");
                self.format_expression(actual);
                self.write(" to be ");
                self.format_expression(expected);
            }
        }
    }

    fn format_string_literal(&mut self, s: &str) {
        self.write("\"");
        self.write(s);
        self.write("\"");
    }

    fn format_binary_op(&mut self, op: &BinaryOp) {
        let s = match op {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "mod",
            BinaryOp::Equal => "is equal to",
            BinaryOp::NotEqual => "is not equal to",
            BinaryOp::Less => "is less than",
            BinaryOp::LessEqual => "is less than or equal to",
            BinaryOp::Greater => "is greater than",
            BinaryOp::GreaterEqual => "is greater than or equal to",
            BinaryOp::And => "and",
            BinaryOp::Or => "or",
            BinaryOp::In => "in",
        };
        self.write(s);
    }

    fn format_unary_op(&mut self, op: &UnaryOp) {
        match op {
            UnaryOp::Neg => self.write("-"),
            UnaryOp::Not => self.write("not"),
        }
    }

    fn indent(&mut self) {
        self.indent += 4;
    }

    fn dedent(&mut self) {
        self.indent = self.indent.saturating_sub(4);
    }

    fn newline(&mut self) {
        self.output.push('\n');
    }

    fn write(&mut self, s: &str) {
        if !self.output.is_empty() && !self.output.ends_with('\n') && !self.output.ends_with(' ') {
            if s == "else" || s == "end" || s == "catch" || s == "finally" {
                // Don't add space before these keywords
            } else {
                self.output.push(' ');
            }
        }
        self.output.push_str(s);
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn format(source: &str) -> Result<String, String> {
    let mut formatter = Formatter::new();
    formatter.format(source)
}
