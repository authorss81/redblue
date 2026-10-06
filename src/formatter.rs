use crate::lexer::Lexer;
use crate::parser::{BinaryOp, Expr, Program, Statement, Stmt, UnaryOp};

/// A `//` comment recovered from the source text, together with the line it was
/// written on.
///
/// The lexer throws comments away, so the formatter reads them back out of the
/// source itself and re-attaches them by line number: [`Stmt::span`] says where
/// each statement begins, so every comment above that line is emitted just
/// before it.
struct Comment {
    line: usize,
    text: String,
}

/// How tightly an operator binds, loosest first. Mirrors the parser's own
/// ladder (`parse_or` → `parse_and` → `parse_comparison` → `parse_addition` →
/// `parse_multiplication` → `parse_unary` → `parse_postfix`), because the
/// formatter prints the tree it is given and has to put back every pair of
/// parentheses the source used to force a different grouping.
const PRECEDENCE_OR: u8 = 1;
const PRECEDENCE_AND: u8 = 2;
const PRECEDENCE_COMPARISON: u8 = 3;
const PRECEDENCE_ADDITION: u8 = 4;
const PRECEDENCE_MULTIPLICATION: u8 = 5;
const PRECEDENCE_UNARY: u8 = 6;
const PRECEDENCE_PRIMARY: u8 = 7;

fn binary_precedence(op: &BinaryOp) -> u8 {
    match op {
        BinaryOp::Or => PRECEDENCE_OR,
        BinaryOp::And => PRECEDENCE_AND,
        BinaryOp::Equal
        | BinaryOp::NotEqual
        | BinaryOp::Less
        | BinaryOp::LessEqual
        | BinaryOp::Greater
        | BinaryOp::GreaterEqual
        | BinaryOp::In => PRECEDENCE_COMPARISON,
        BinaryOp::Add | BinaryOp::Sub => PRECEDENCE_ADDITION,
        BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => PRECEDENCE_MULTIPLICATION,
    }
}

/// The precedence an expression prints at: the precedence of its loosest
/// operator, or [`PRECEDENCE_PRIMARY`] when it is an atom.
fn expression_precedence(expr: &Expr) -> u8 {
    match expr {
        Expr::Binary { op, .. } => binary_precedence(op),
        Expr::Unary { .. } => PRECEDENCE_UNARY,
        // `expect … to be …` parses as a whole expression, so nothing may bind
        // into it and anything it holds must be parenthesised around it.
        Expr::Expect { .. } => 0,
        _ => PRECEDENCE_PRIMARY,
    }
}

/// Reads every `//` comment out of `source`, with its 1-based line.
///
/// A `//` inside a text literal is text, not a comment, so the scan tracks
/// string state exactly as the lexer does: `"` opens and closes, `\` escapes
/// the next character, and a literal may span lines.
fn collect_comments(source: &str) -> Vec<Comment> {
    let mut comments = Vec::new();
    let mut in_text = false;
    let mut escaped = false;
    let mut line = 1usize;

    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        if in_text {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_text = false;
            }
            if c == '\n' {
                line += 1;
            }
            continue;
        }

        if c == '"' {
            in_text = true;
        } else if c == '/' && chars.peek() == Some(&'/') {
            let mut text = String::from("//");
            chars.next();
            while let Some(&next) = chars.peek() {
                if next == '\n' {
                    break;
                }
                text.push(next);
                chars.next();
            }
            comments.push(Comment {
                line,
                text: text.trim_end().to_string(),
            });
            continue;
        }

        if c == '\n' {
            line += 1;
        }
    }

    comments
}

pub struct Formatter {
    indent: usize,
    output: String,
    comments: Vec<Comment>,
    next_comment: usize,
}

impl Formatter {
    pub fn new() -> Self {
        Self {
            indent: 0,
            output: String::new(),
            comments: Vec::new(),
            next_comment: 0,
        }
    }

    /// Formats `source`, replacing whatever a previous call left behind: a
    /// formatter is reusable, and one source must never inherit another's text.
    pub fn format(&mut self, source: &str) -> Result<String, String> {
        let tokens = Lexer::tokenize(source).map_err(|e| format!("Lexer error: {}", e))?;

        let mut parser = crate::parser::Parser::new(tokens);
        let program = parser.parse().map_err(|e| format!("Parser error: {}", e))?;

        self.indent = 0;
        self.output = String::new();
        self.comments = collect_comments(source);
        self.next_comment = 0;

        self.format_program(&program);

        // Exactly one trailing newline, so a formatted file ends the way every
        // text file should and `--check` can compare the bytes exactly.
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.newline();
        }

        Ok(self.output.clone())
    }

    fn format_program(&mut self, program: &Program) {
        self.format_statements(&program.statements);
        // Comments inside the last block, or after the last statement, have no
        // later statement to attach to; they keep their text at the end.
        self.emit_comments(usize::MAX);
    }

    /// Writes each statement on its own line, and the comments that precede it.
    ///
    /// `give back x` is one line of source but two statements in the tree: the
    /// lexer reads `give` and `back` as the same token, so `give back` is a bare
    /// `give` and the pair only reads back as the pair it was. A pair is
    /// therefore printed on one line; a lone one is printed as the single token
    /// it re-reads as (`give` or `back x`), because `give back` on a line of
    /// its own would read back as two statements and the formatter would never
    /// settle.
    fn format_statements(&mut self, body: &[Stmt]) {
        let mut i = 0;
        while i < body.len() {
            let stmt = &body[i];
            let bare = matches!(stmt.statement, Statement::GiveBack(None));
            let next_give_back = if bare {
                body.get(i + 1)
                    .map(|next| matches!(next.statement, Statement::GiveBack(_)))
                    .unwrap_or(false)
            } else {
                false
            };

            self.emit_comments(stmt.span.line);

            if bare && next_give_back {
                self.write_indent();
                self.write("give back");
                if let Statement::GiveBack(Some(expr)) = &body[i + 1].statement {
                    self.write(" ");
                    self.format_expression(expr);
                }
                self.newline();
                i += 2;
                continue;
            }

            self.write_indent();
            match &stmt.statement {
                // One token, so it reads back as the one statement it is.
                Statement::GiveBack(None) => self.write("give"),
                Statement::GiveBack(Some(expr)) => {
                    self.write("back ");
                    self.format_expression(expr);
                }
                _ => self.format_statement(stmt),
            }
            self.newline();
            i += 1;
        }
    }

    /// Writes one statement's own text, without the comments that precede it —
    /// [`Formatter::format_statements`] has already written those, and the
    /// indentation with them.
    fn format_statement(&mut self, stmt: &Stmt) {
        match &stmt.statement {
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
            Statement::Constant { name, value } => {
                self.write("constant ");
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
                // `then` is not optional to the parser.
                self.write(" then");
                self.newline();
                self.format_block(then_branch);

                if !else_branch.is_empty() {
                    self.write_keyword_line("else");
                    self.newline();
                    self.format_block(else_branch);
                }
                self.write_keyword_line("end");
            }
            Statement::Unless { condition, body } => {
                self.write("unless ");
                self.format_expression(condition);
                // `then` is not optional to the parser.
                self.write(" then");
                self.newline();
                self.format_block(body);
                self.write_keyword_line("end");
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
                self.format_block(body);
                self.write_keyword_line("end");
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
                self.format_block(body);
                self.write_keyword_line("end");
            }
            Statement::Repeat { count, body } => {
                self.write("repeat ");
                self.format_expression(count);
                self.write(" times");
                self.newline();
                self.format_block(body);
                self.write_keyword_line("end");
            }
            Statement::While { condition, body } => {
                self.write("while ");
                self.format_expression(condition);
                self.newline();
                self.format_block(body);
                self.write_keyword_line("end");
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
                self.write_signature("to ", name, params);
                self.newline();
                self.format_block(body);
                self.write_keyword_line("end");
            }
            Statement::Method { name, params, body } => {
                self.write_signature("to can ", name, params);
                self.newline();
                self.format_block(body);
                self.write_keyword_line("end");
            }
            Statement::Has { name, default } => {
                self.write("has ");
                self.write(name);
                if let Some(expr) = default {
                    self.write(" default ");
                    self.format_expression(expr);
                }
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
                self.format_block(body);
                self.write_keyword_line("end");
            }
            Statement::Module { name, body } => {
                self.write("module ");
                self.write(name);
                self.newline();
                self.format_block(body);
                self.write_keyword_line("end");
            }
            Statement::Export { names, all } => {
                self.write("export ");
                if *all {
                    self.write("all");
                }
                for (i, exported) in names.iter().enumerate() {
                    if *all || i > 0 {
                        self.write(", ");
                    }
                    self.write(exported);
                }
            }
            Statement::Try {
                body,
                catch_var,
                catch_body,
                finally_body,
            } => {
                self.write("try");
                self.newline();
                self.format_block(body);

                // A bare `catch` — one with no name binding — is legal: the parser
                // leaves catch_var as None and still collects catch_body. Gating
                // the whole block on `Some(var)` therefore DELETED the body: the
                // formatter silently discarded code the user wrote. Losing a
                // statement is the worst thing a formatter can do, so the block is
                // emitted whenever a catch is actually present.
                if catch_var.is_some() || !catch_body.is_empty() {
                    self.write_keyword_line("catch");
                    if let Some(var) = catch_var {
                        self.write(" ");
                        self.write(var);
                    }
                    self.newline();
                    self.format_block(catch_body);
                }

                if !finally_body.is_empty() {
                    self.write_keyword_line("finally");
                    self.newline();
                    self.format_block(finally_body);
                }
                self.write_keyword_line("end");
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
                self.format_block(body);
                self.write_keyword_line("end");
            }
            Statement::Expr(expr) => {
                self.format_expression(expr);
            }
        }
    }

    /// Writes a block's statements, each on its own line, indented one level.
    fn format_block(&mut self, body: &[Stmt]) {
        self.indent();
        self.format_statements(body);
        self.dedent();
    }

    fn write_signature(&mut self, keyword: &str, name: &str, params: &[String]) {
        self.write(keyword);
        self.write(name);
        self.write("(");
        self.write(&params.join(", "));
        self.write(")");
    }

    /// Writes `expr`, wrapping it in parentheses when it binds more loosely than
    /// `min_precedence` and would therefore re-parse as a different tree.
    fn format_expression(&mut self, expr: &Expr) {
        self.format_expression_at(expr, 0);
    }

    fn format_expression_at(&mut self, expr: &Expr, min_precedence: u8) {
        let precedence = expression_precedence(expr);
        let parenthesise = precedence < min_precedence;
        if parenthesise {
            self.write("(");
        }

        match expr {
            // Every f64 prints as a literal the lexer reads back as the same number —
            // Rust's own display is the shortest decimal that round-trips. The
            // one exception is a value that is not finite: `inf` would read
            // back as a variable called `inf`. Only a literal out of range
            // (`1e400`) can hold one, and the lexer parses it as an f64, so
            // printing the same out-of-range literal gives back the same value
            // and the program still fails the way it did.
            Expr::Number(n) => {
                if n.is_finite() {
                    self.write(&n.to_string());
                } else if *n > 0.0 {
                    self.write("1e400");
                } else {
                    self.write("-1e400");
                }
            }
            Expr::Text(s) => self.format_string_literal(s),
            Expr::YesNo(b) => self.write(if *b { "yes" } else { "no" }),
            Expr::Nothing => self.write("nothing"),
            Expr::Variable(name) => self.write(name),
            Expr::Binary { op, left, right } => {
                // Every level above binds left to right, so the left side needs
                // parentheses only when it is looser than this operator, and the
                // right side when it is looser *or equal* — `1 - (2 - 3)` and
                // `1 - 2 - 3` are different programs.
                self.format_expression_at(left, precedence);
                self.write(" ");
                self.format_binary_op(op);
                self.write(" ");
                // `x is not y` is a `is not` comparison, so a right operand
                // that begins with the word `not` needs its parentheses back.
                let guard = matches!(op, BinaryOp::Equal)
                    && matches!(
                        right.as_ref(),
                        Expr::Unary {
                            op: UnaryOp::Not,
                            ..
                        }
                    );
                if guard {
                    self.write("(");
                }
                self.format_expression_at(right, precedence + 1);
                if guard {
                    self.write(")");
                }
            }
            Expr::Unary { op, expr: operand } => {
                self.format_unary_op(op);
                // `- 1` and `-1` read the same, and the tight form is what a
                // person would have written.
                if !matches!(operand.as_ref(), Expr::Number(_)) {
                    self.write(" ");
                }
                self.format_expression_at(operand, PRECEDENCE_UNARY);
            }
            Expr::Call { name, args } => {
                self.write(name);
                self.write_arguments(args);
            }
            Expr::Property { object, property } => {
                self.format_expression_at(object, PRECEDENCE_PRIMARY);
                self.write(".");
                self.write(property);
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
            } => {
                self.format_expression_at(receiver, PRECEDENCE_PRIMARY);
                self.write(".");
                self.write(method);
                self.write_arguments(args);
            }
            // `object at index` is not syntax the parser accepts; only the
            // bracketed form reads back as an index.
            Expr::Index { object, index } => {
                self.format_expression_at(object, PRECEDENCE_PRIMARY);
                self.write("[");
                self.format_expression(index);
                self.write("]");
            }
            Expr::InterpolatedText(parts) => {
                self.write("\"");
                for part in parts {
                    match part {
                        Expr::Text(s) => self.write(&escape_text(s)),
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
                // `{a: 1}` is the whole syntax: `record` is an ordinary
                // identifier, so writing it would read back as a bare word.
                self.write("{");
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

        if parenthesise {
            self.write(")");
        }
    }

    fn write_arguments(&mut self, args: &[Expr]) {
        self.write("(");
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                self.write(", ");
            }
            self.format_expression(arg);
        }
        self.write(")");
    }

    /// Writes every comment that begins before `line` and has not been written
    /// yet, at the indentation of the statement it precedes.
    fn emit_comments(&mut self, line: usize) {
        while self.next_comment < self.comments.len() {
            let comment = &self.comments[self.next_comment];
            if comment.line >= line {
                break;
            }
            let text = comment.text.clone();
            self.write_indent();
            self.write(&text);
            self.newline();
            self.next_comment += 1;
        }
    }

    /// Writes the `//` escape hatch the lexer understands, so the text reads
    /// back as the same characters. Everything else — `{`, `}`, emoji, CJK,
    /// combining marks — is written as it stands.
    fn format_string_literal(&mut self, s: &str) {
        self.write("\"");
        self.write(&escape_text(s));
        self.write("\"");
    }

    /// Writes an operator the way the parser reads it back. Only `is` and
    /// `is not` are spellings the lexer produces today: `%` is the remainder
    /// operator, and `<`, `<=`, `>`, `>=` and `in` have no token at all, so
    /// those variants cannot reach this formatter from source text and are
    /// written in the symbolic form for a future lexer that grows them.
    fn format_binary_op(&mut self, op: &BinaryOp) {
        let s = match op {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Equal => "is",
            BinaryOp::NotEqual => "is not",
            BinaryOp::Less => "<",
            BinaryOp::LessEqual => "<=",
            BinaryOp::Greater => ">",
            BinaryOp::GreaterEqual => ">=",
            BinaryOp::And => "and",
            BinaryOp::Or => "or",
            // `in` has no symbol of its own, and `x in list` does not parse:
            // membership is reached through `is`, so the formatter must write
            // the `is` back or the formatted source stops being Redblue.
            BinaryOp::In => "is in",
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

    /// Writes a word that opens or closes a block at the start of its own
    /// line: `else`, `catch`, `finally` and the `end` that closes a block all
    /// sit at the indentation of the statement they belong to, which is one
    /// level out from the block itself.
    fn write_keyword_line(&mut self, word: &str) {
        self.write_indent();
        self.write(word);
    }

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.output.push(' ');
        }
    }

    fn newline(&mut self) {
        self.output.push('\n');
    }

    fn write(&mut self, s: &str) {
        self.output.push_str(s);
    }
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new()
    }
}

/// Escapes the characters a text literal cannot carry verbatim.
fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Whether `formatted` differs from `source`.
///
/// The comparison is exact: a missing trailing newline, a line of trailing
/// spaces, or a blank line at the end of the file are differences, because
/// trimming them first is how a file with trailing whitespace came to be
/// reported as already formatted.
pub fn needs_reformat(source: &str, formatted: &str) -> bool {
    source != formatted
}

pub fn format(source: &str) -> Result<String, String> {
    let mut formatter = Formatter::new();
    formatter.format(source)
}
