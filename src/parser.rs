use crate::error::{Error, Result, Span};
use crate::lexer::{Token, TokenKind};

#[derive(Debug, Clone)]
pub enum Expr {
    // Literals
    Number(f64),
    Text(String),
    YesNo(bool),
    Nothing,

    // Variables
    Variable(String),

    // Binary operations
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },

    // Unary operations
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },

    // Function call
    Call {
        name: String,
        args: Vec<Expr>,
    },

    // Property access
    Property {
        object: Box<Expr>,
        property: String,
    },

    // receiver.method(args) — a call on a value, as opposed to the bare
    // `name(args)` of [`Expr::Call`]. The runtime reads it as an object method
    // and, for a receiver that names no object, as a module function.
    MethodCall {
        receiver: Box<Expr>,
        method: String,
        args: Vec<Expr>,
    },

    // Index access
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },

    // String interpolation
    InterpolatedText(Vec<Expr>),

    // List literal
    List(Vec<Expr>),

    // Record literal
    Record(Vec<(String, Expr)>),

    // expect expression to be expected_value
    Expect {
        actual: Box<Expr>,
        expected: Box<Expr>,
    },
}

#[derive(Debug, Clone)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    In,
}

#[derive(Debug, Clone)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub enum Statement {
    // say "Hello"
    Say(Expr),

    // Expression statement (for side effects)
    Expr(Expr),

    // print "Hello"
    Print(Expr),

    // set x to 10
    Set {
        name: String,
        value: Expr,
    },

    // constant NAME to <expr> — a name bound once, which no assignment may
    // rebind. A module file uses it for the values every function of the
    // module shares, as `modules/MathUtils.rb` does for `PI`.
    Constant {
        name: String,
        value: Expr,
    },

    // set x.y to 10
    SetProperty {
        object: String,
        property: String,
        value: Expr,
    },

    // if condition then ... end
    If {
        condition: Expr,
        then_branch: Vec<Stmt>,
        else_branch: Vec<Stmt>,
    },

    // unless condition then ... end
    //
    // The body is taken when the condition is *false*, and there is no second
    // branch: `unless` has an `else` only as its own absence, so a second
    // alternative is written `if`. `unless condition ... end` and
    // `if not condition ... end` therefore mean the same thing.
    Unless {
        condition: Expr,
        body: Vec<Stmt>,
    },

    // for each x in list ... end
    ForEach {
        variable: String,
        iterable: Expr,
        body: Vec<Stmt>,
    },

    // for each i from 1 to 10 ... end
    ForRange {
        variable: String,
        start: Expr,
        end: Expr,
        step: Option<Expr>,
        body: Vec<Stmt>,
    },

    // repeat 10 times ... end
    Repeat {
        count: Expr,
        body: Vec<Stmt>,
    },

    // while condition ... end
    While {
        condition: Expr,
        body: Vec<Stmt>,
    },

    // break
    Break,

    // skip
    Skip,

    // return value
    Return(Option<Expr>),

    // give back value
    GiveBack(Option<Expr>),

    // to function(args) ... end
    Function {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
    },

    // to can method(args) ... end
    Method {
        name: String,
        params: Vec<String>,
        body: Vec<Stmt>,
    },

    // has name [default <expr>] — a field declaration, only inside an object
    Has {
        name: String,
        default: Option<Expr>,
    },

    // object Name ... end
    Object {
        name: String,
        extends: Option<String>,
        body: Vec<Stmt>,
    },

    // try ... catch ... end
    Try {
        body: Vec<Stmt>,
        catch_var: Option<String>,
        catch_body: Vec<Stmt>,
        finally_body: Vec<Stmt>,
    },

    // import module
    Import(Vec<ImportItem>),

    // test "name" ... end
    Test {
        name: String,
        body: Vec<Stmt>,
    },
}

#[derive(Debug, Clone)]
pub struct ImportItem {
    pub name: String,
    pub alias: Option<String>,
}

/// A statement together with the source position it was parsed from.
#[derive(Debug, Clone)]
pub struct Stmt {
    pub span: Span,
    pub statement: Statement,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Stmt>,
}

/// The deepest an expression may nest before the parser gives up with a
/// spanned [`Error::Parser`].
///
/// Every nested bracket, prefix operator, call argument and left-associative
/// binary chain makes the expression tree one level deeper. Parsing recurses
/// per level and dropping the finished tree recurses per level, so an
/// unbounded depth turns a long line of source into "has overflowed its stack"
/// and a core dump instead of a diagnostic.
///
/// The value is deliberately small. libstd gives a spawned thread a 2 MiB
/// stack, and a debug build spends roughly 16 KiB of stack per level of nested
/// brackets (eleven frames of `parse_primary` .. `parse_postfix`). Measured on
/// this repository, 123 levels is where a 2 MiB test thread dies; 64 leaves
/// nearly a 2x margin while staying far above anything hand-written.
pub const MAX_NESTING_DEPTH: usize = 64;

/// The deepest `if` / `for` / `to` / `object` / `try` / `test` blocks may nest
/// before the parser gives up with a spanned [`Error::Parser`].
///
/// A block body is parsed by a recursive call back into `parse_statement`, so
/// each nested block is a level of parser recursion, and the analyzer and VM
/// then walk the same tree. The same 64-level budget as
/// [`MAX_NESTING_DEPTH`] is used, because the same 2 MiB stack has to hold it.
pub const MAX_BLOCK_DEPTH: usize = 64;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    depth: usize,
    block_depth: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            depth: 0,
            block_depth: 0,
        }
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    /// The position of the token about to be read. Falls back to the last
    /// token when the input has run out, and to [`Span::unknown`] only when
    /// there is no token at all (an empty file).
    fn span(&self) -> Span {
        match self.current().or_else(|| self.tokens.last()) {
            Some(token) => token.span(),
            None => Span::unknown(),
        }
    }

    fn advance(&mut self) -> Option<Token> {
        if self.pos < self.tokens.len() {
            self.pos += 1;
            Some(self.tokens[self.pos - 1].clone())
        } else {
            None
        }
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<Token> {
        let token = self
            .advance()
            .ok_or_else(|| Error::Parser("Unexpected end of input".to_string(), self.span()))?;

        if &token.kind != kind {
            return Err(Error::Parser(
                format!("Expected {:?} but got {:?}", kind, token.kind),
                token.span(),
            ));
        }

        Ok(token)
    }

    /// Accounts for one more level of expression nesting.
    ///
    /// Returns a spanned [`Error::Parser`] once the nesting passes
    /// [`MAX_NESTING_DEPTH`], so a pathological source line is a diagnostic
    /// instead of a stack overflow.
    fn enter_nesting(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > MAX_NESTING_DEPTH {
            return Err(Error::Parser(
                format!("Expression nests more than {MAX_NESTING_DEPTH} levels deep"),
                self.span(),
            ));
        }
        Ok(())
    }

    /// Releases `count` nesting levels claimed by [`Parser::enter_nesting`].
    fn leave_nesting(&mut self, count: usize) {
        self.depth = self.depth.saturating_sub(count);
    }

    /// Whether the upcoming statement opens a `... end` block, and so parses its
    /// body by calling back into [`Parser::parse_statement`].
    fn opens_block(&self) -> bool {
        matches!(
            self.current().map(|token| &token.kind),
            Some(TokenKind::If)
                | Some(TokenKind::Unless)
                | Some(TokenKind::For)
                | Some(TokenKind::Repeat)
                | Some(TokenKind::While)
                | Some(TokenKind::To)
                | Some(TokenKind::Object)
                | Some(TokenKind::Try)
                | Some(TokenKind::Test)
        )
    }

    /// Accounts for one more level of block nesting.
    fn enter_block(&mut self) -> Result<()> {
        self.block_depth += 1;
        if self.block_depth > MAX_BLOCK_DEPTH {
            return Err(Error::Parser(
                format!("Blocks nest more than {MAX_BLOCK_DEPTH} levels deep"),
                self.span(),
            ));
        }
        Ok(())
    }

    fn leave_block(&mut self) {
        self.block_depth = self.block_depth.saturating_sub(1);
    }

    fn skip_newlines(&mut self) {
        while let Some(Token {
            kind: TokenKind::Newline,
            ..
        }) = self.current()
        {
            self.advance();
        }
    }

    pub fn parse(&mut self) -> Result<Program> {
        let mut statements = Vec::new();

        self.skip_newlines();

        // `None` is checked as well as `Eof`: a token stream handed straight
        // to `Parser::new` need not end in `Eof`, and without this the loop
        // would spin forever once `current()` runs off the end.
        while let Some(token) = self.current() {
            if token.kind == TokenKind::Eof {
                break;
            }
            if let Some(stmt) = self.parse_statement()? {
                statements.push(stmt);
            }
            self.skip_newlines();
        }

        Ok(Program { statements })
    }

    fn parse_statement(&mut self) -> Result<Option<Stmt>> {
        let opened_block = self.opens_block();
        if opened_block {
            self.enter_block()?;
        }

        let result = self.parse_statement_inner();

        // Expression nesting is counted per statement, so one long expression
        // cannot spend the budget of the next one.
        self.depth = 0;
        if opened_block {
            self.leave_block();
        }

        result
    }

    fn parse_statement_inner(&mut self) -> Result<Option<Stmt>> {
        let span = self.span();
        let token = match self.current() {
            Some(t) => t.clone(),
            None => return Ok(None),
        };

        let stmt: Option<Statement> = match &token.kind {
            TokenKind::Say => {
                self.advance();
                let expr = self.parse_expression()?;
                Some(Statement::Say(expr))
            }
            TokenKind::Print => {
                self.advance();
                let expr = self.parse_expression()?;
                Some(Statement::Print(expr))
            }
            TokenKind::Set => self.parse_set()?,
            TokenKind::Constant => self.parse_constant()?,
            TokenKind::If => self.parse_if()?,
            TokenKind::Unless => self.parse_unless()?,
            TokenKind::For => self.parse_for()?,
            TokenKind::Repeat => self.parse_repeat()?,
            TokenKind::While => self.parse_while()?,
            TokenKind::Break => {
                self.advance();
                Some(Statement::Break)
            }
            TokenKind::Skip => {
                self.advance();
                Some(Statement::Skip)
            }
            TokenKind::Return => {
                self.advance();
                let expr = if self.is_expression_start() {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                Some(Statement::Return(expr))
            }
            TokenKind::GiveBack => {
                self.advance();
                let expr = if self.is_expression_start() {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                Some(Statement::GiveBack(expr))
            }
            TokenKind::To => self.parse_function()?,
            TokenKind::Object => self.parse_object()?,
            TokenKind::Try => self.parse_try()?,
            TokenKind::Test => self.parse_test()?,
            TokenKind::Expect => self.parse_expect()?,
            TokenKind::Import => {
                self.advance();
                let mut items = Vec::new();
                loop {
                    if let Some(Token {
                        kind: TokenKind::Identifier(name),
                        ..
                    }) = self.current()
                    {
                        let name = name.clone();
                        self.advance();

                        let alias = if let Some(Token {
                            kind: TokenKind::To,
                            ..
                        }) = self.current()
                        {
                            self.advance();
                            if let Some(Token {
                                kind: TokenKind::Identifier(alias),
                                ..
                            }) = self.current()
                            {
                                let a = alias.clone();
                                self.advance();
                                Some(a)
                            } else {
                                return Err(Error::Parser(
                                    "Expected alias after 'to'".to_string(),
                                    self.span(),
                                ));
                            }
                        } else {
                            None
                        };

                        items.push(ImportItem { name, alias });
                    } else {
                        return Err(Error::Parser(
                            "Expected module name".to_string(),
                            self.span(),
                        ));
                    }

                    if let Some(Token {
                        kind: TokenKind::Comma,
                        ..
                    }) = self.current()
                    {
                        self.advance();
                        continue;
                    }
                    break;
                }
                Some(Statement::Import(items))
            }
            TokenKind::Newline => {
                self.advance();
                return Ok(None);
            }
            _ => {
                let expr = self.parse_expression()?;
                Some(Statement::Expr(expr))
            }
        };

        Ok(stmt.map(|statement| Stmt { span, statement }))
    }

    fn parse_set(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'set'

        let name = match self.current() {
            Some(Token {
                kind: TokenKind::Identifier(name),
                ..
            }) => {
                let n = name.clone();
                self.advance();
                n
            }
            // `set this.field to …` is how a method writes to the object it was
            // called on, so `this` is a name a `set` accepts as a target.
            Some(Token {
                kind: TokenKind::This,
                ..
            }) => {
                self.advance();
                "this".to_string()
            }
            _ => {
                return Err(Error::Parser(
                    "Expected variable name".to_string(),
                    self.span(),
                ))
            }
        };

        // Check for property access: set x.y to value
        if let Some(Token {
            kind: TokenKind::Dot,
            ..
        }) = self.current()
        {
            self.advance();
            let property = match self.current() {
                Some(Token {
                    kind: TokenKind::Identifier(prop),
                    ..
                }) => {
                    let p = prop.clone();
                    self.advance();
                    p
                }
                _ => {
                    return Err(Error::Parser(
                        "Expected property name".to_string(),
                        self.span(),
                    ))
                }
            };

            self.expect(&TokenKind::To)?;
            let value = self.parse_expression()?;

            return Ok(Some(Statement::SetProperty {
                object: name,
                property,
                value,
            }));
        }

        self.expect(&TokenKind::To)?;
        let value = self.parse_expression()?;

        Ok(Some(Statement::Set { name, value }))
    }

    /// Parses `constant NAME to <expr>` — a `set` whose name may not be bound
    /// again afterwards, so the runtime can refuse a later assignment to it.
    fn parse_constant(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'constant'

        let name = match self.current() {
            Some(Token {
                kind: TokenKind::Identifier(name),
                ..
            }) => {
                let n = name.clone();
                self.advance();
                n
            }
            _ => {
                return Err(Error::Parser(
                    "Expected constant name after 'constant'".to_string(),
                    self.span(),
                ))
            }
        };

        self.expect(&TokenKind::To)?;
        let value = self.parse_expression()?;

        Ok(Some(Statement::Constant { name, value }))
    }

    fn parse_if(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'if'
        let condition = self.parse_expression()?;

        self.skip_newlines();
        self.expect(&TokenKind::Then)?;
        self.skip_newlines();

        let mut then_branch = Vec::new();
        while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
            && self.current().map(|t| &t.kind) != Some(&TokenKind::Else)
            && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
        {
            if let Some(stmt) = self.parse_statement()? {
                then_branch.push(stmt);
            }
            self.skip_newlines();
        }

        let mut else_branch = Vec::new();

        if let Some(Token {
            kind: TokenKind::Else,
            ..
        }) = self.current()
        {
            self.advance();
            self.skip_newlines();

            while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
                && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
            {
                if let Some(stmt) = self.parse_statement()? {
                    else_branch.push(stmt);
                }
                self.skip_newlines();
            }
        }

        self.expect(&TokenKind::End)?;

        Ok(Some(Statement::If {
            condition,
            then_branch,
            else_branch,
        }))
    }

    /// Parses `unless <expr> then ... end`.
    ///
    /// The condition grammar is `if`'s — the same [`Parser::parse_expression`]
    /// call — and the block is closed by the same `end`. There is no `else`
    /// arm: `else` here is a token the grammar does not accept, and it fails
    /// at [`Parser::expect`] with a spanned parser error rather than being
    /// swallowed as an alternative branch.
    fn parse_unless(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'unless'
        let condition = self.parse_expression()?;

        self.skip_newlines();
        self.expect(&TokenKind::Then)?;
        self.skip_newlines();

        let mut body = Vec::new();
        while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
            && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
        {
            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
            self.skip_newlines();
        }

        self.expect(&TokenKind::End)?;

        Ok(Some(Statement::Unless { condition, body }))
    }

    fn parse_for(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'for'

        match self.current() {
            Some(Token {
                kind: TokenKind::Each,
                ..
            }) => {
                self.advance();

                let variable = match self.current() {
                    Some(Token {
                        kind: TokenKind::Identifier(name),
                        ..
                    }) => {
                        let n = name.clone();
                        self.advance();
                        n
                    }
                    _ => {
                        return Err(Error::Parser(
                            "Expected variable name".to_string(),
                            self.span(),
                        ))
                    }
                };

                // `from A to B [by C]` and `in <iterable>` share the loop
                // variable and the body; only what sits between them differs.
                if self.current().map(|t| &t.kind) == Some(&TokenKind::From) {
                    self.advance();

                    let start = self.parse_expression()?;
                    self.expect(&TokenKind::To)?;
                    let end = self.parse_expression()?;

                    let step = if self.at_range_step_marker() {
                        self.advance();
                        Some(self.parse_expression()?)
                    } else {
                        None
                    };

                    let body = self.parse_loop_body()?;

                    Ok(Some(Statement::ForRange {
                        variable,
                        start,
                        end,
                        step,
                        body,
                    }))
                } else {
                    self.expect(&TokenKind::In)?;
                    let iterable = self.parse_expression()?;
                    let body = self.parse_loop_body()?;

                    Ok(Some(Statement::ForEach {
                        variable,
                        iterable,
                        body,
                    }))
                }
            }
            _ => Err(Error::Parser(
                "Expected 'each' after 'for'".to_string(),
                self.span(),
            )),
        }
    }

    /// Whether the token at the cursor is the `by` of a range loop's optional
    /// step.
    ///
    /// `by` is read positionally rather than taken from the `KEYWORDS` table
    /// (`src/lexer.rs:14-69`) because it is not a reserved word: reserving it
    /// would refuse `to can grow(by)` / `say by`, a parameter name the language
    /// has always allowed (`tests/bytecode_test.rs:602`). It means "step" in
    /// this one place and nowhere else, which is what a positional marker buys.
    /// `TokenKind::By` is accepted too, so the token is honoured if `by` is ever
    /// added to the table without this arm needing to change.
    fn at_range_step_marker(&self) -> bool {
        match self.current() {
            Some(Token {
                kind: TokenKind::By,
                ..
            }) => true,
            Some(Token {
                kind: TokenKind::Identifier(name),
                ..
            }) => name == "by",
            _ => false,
        }
    }

    /// Parses the `{ statement } 'end'` tail every loop form shares, consuming
    /// the `end`.
    fn parse_loop_body(&mut self) -> Result<Vec<Stmt>> {
        self.skip_newlines();

        let mut body = Vec::new();
        while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
            && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
        {
            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
            self.skip_newlines();
        }

        self.expect(&TokenKind::End)?;

        Ok(body)
    }

    fn parse_repeat(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'repeat'
        let count = self.parse_expression()?;
        self.expect(&TokenKind::Times)?;
        self.skip_newlines();

        let mut body = Vec::new();
        while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
            && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
        {
            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
            self.skip_newlines();
        }

        self.expect(&TokenKind::End)?;

        Ok(Some(Statement::Repeat { count, body }))
    }

    fn parse_while(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'while'
        let condition = self.parse_expression()?;
        self.skip_newlines();

        let mut body = Vec::new();
        while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
            && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
        {
            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
            self.skip_newlines();
        }

        self.expect(&TokenKind::End)?;

        Ok(Some(Statement::While { condition, body }))
    }

    fn parse_function(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'to'

        let (name, params, body) = self.parse_callable()?;
        Ok(Some(Statement::Function { name, params, body }))
    }

    /// Parses a `to can name(params) ... end` method declaration, the form
    /// SPEC.md gives inside an `object` body.
    fn parse_method(&mut self) -> Result<Option<Stmt>> {
        let span = self.span();
        self.advance(); // consume 'to'

        if self.current().map(|t| &t.kind) == Some(&TokenKind::Can) {
            self.advance();
        }

        let (name, params, body) = self.parse_callable()?;
        Ok(Some(Stmt {
            span,
            statement: Statement::Method { name, params, body },
        }))
    }

    /// Parses `has name [default <expr>]` — a field declaration.
    fn parse_has(&mut self) -> Result<Option<Stmt>> {
        let span = self.span();
        self.advance(); // consume 'has'

        let name = match self.current() {
            Some(Token {
                kind: TokenKind::Identifier(name),
                ..
            }) => {
                let n = name.clone();
                self.advance();
                n
            }
            _ => {
                return Err(Error::Parser(
                    "Expected field name after 'has'".to_string(),
                    self.span(),
                ))
            }
        };

        // `default` is not a keyword in the lexer, so it is matched by name
        // here rather than by a token kind.
        let default = match self.current() {
            Some(Token {
                kind: TokenKind::Identifier(word),
                ..
            }) if word == "default" => {
                self.advance();
                Some(self.parse_expression()?)
            }
            _ => None,
        };

        Ok(Some(Stmt {
            span,
            statement: Statement::Has { name, default },
        }))
    }

    /// Parses the shared tail of `to name(...)` and `to can name(...)`: the
    /// name, the parameter list, the body, and the closing `end`.
    fn parse_callable(&mut self) -> Result<(String, Vec<String>, Vec<Stmt>)> {
        let name = match self.current() {
            Some(Token {
                kind: TokenKind::Identifier(name),
                ..
            }) => {
                let n = name.clone();
                self.advance();
                n
            }
            _ => {
                return Err(Error::Parser(
                    "Expected function name".to_string(),
                    self.span(),
                ))
            }
        };

        // Parse parameters
        let mut params = Vec::new();
        if let Some(Token {
            kind: TokenKind::LeftParen,
            ..
        }) = self.current()
        {
            self.advance();

            while let Some(Token {
                kind: TokenKind::Identifier(name),
                ..
            }) = self.current()
            {
                params.push(name.clone());
                self.advance();

                if let Some(Token {
                    kind: TokenKind::Comma,
                    ..
                }) = self.current()
                {
                    self.advance();
                } else {
                    break;
                }
            }

            self.expect(&TokenKind::RightParen)?;
        }

        self.skip_newlines();

        let mut body = Vec::new();
        while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
            && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
        {
            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
            self.skip_newlines();
        }

        self.expect(&TokenKind::End)?;

        Ok((name, params, body))
    }

    fn parse_object(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'object'

        let name = match self.current() {
            Some(Token {
                kind: TokenKind::Identifier(name),
                ..
            }) => {
                let n = name.clone();
                self.advance();
                n
            }
            _ => {
                return Err(Error::Parser(
                    "Expected object name".to_string(),
                    self.span(),
                ))
            }
        };

        let extends = if let Some(Token {
            kind: TokenKind::Extends,
            ..
        }) = self.current()
        {
            self.advance();
            match self.current() {
                Some(Token {
                    kind: TokenKind::Identifier(name),
                    ..
                }) => {
                    let n = name.clone();
                    self.advance();
                    Some(n)
                }
                _ => {
                    return Err(Error::Parser(
                        "Expected parent object name".to_string(),
                        self.span(),
                    ))
                }
            }
        } else {
            None
        };

        self.skip_newlines();

        let mut body = Vec::new();
        while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
            && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
        {
            // Inside an object body, `has` and `to can` declare a field and a
            // method of the object rather than running as statements. SPEC.md
            // writes the constructor as `to create(name, age)` without the
            // `can`, so a bare `to` is a method too.
            let stmt = match self.current().map(|t| &t.kind) {
                Some(TokenKind::Has) => self.parse_has()?,
                Some(TokenKind::To) => self.parse_method()?,
                _ => self.parse_statement()?,
            };
            if let Some(stmt) = stmt {
                body.push(stmt);
            }
            self.skip_newlines();
        }

        self.expect(&TokenKind::End)?;

        Ok(Some(Statement::Object {
            name,
            extends,
            body,
        }))
    }

    fn parse_try(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'try'
        self.skip_newlines();

        let mut body = Vec::new();
        while self.current().map(|t| &t.kind) != Some(&TokenKind::Catch)
            && self.current().map(|t| &t.kind) != Some(&TokenKind::Finally)
            && self.current().map(|t| &t.kind) != Some(&TokenKind::End)
            && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
        {
            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
            self.skip_newlines();
        }

        let mut catch_var = None;
        let mut catch_body = Vec::new();

        if let Some(Token {
            kind: TokenKind::Catch,
            ..
        }) = self.current()
        {
            self.advance();
            if let Some(Token {
                kind: TokenKind::Identifier(name),
                ..
            }) = self.current()
            {
                catch_var = Some(name.clone());
                self.advance();
            }
            self.skip_newlines();

            while self.current().map(|t| &t.kind) != Some(&TokenKind::Finally)
                && self.current().map(|t| &t.kind) != Some(&TokenKind::End)
                && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
            {
                if let Some(stmt) = self.parse_statement()? {
                    catch_body.push(stmt);
                }
                self.skip_newlines();
            }
        }

        let mut finally_body = Vec::new();

        if let Some(Token {
            kind: TokenKind::Finally,
            ..
        }) = self.current()
        {
            self.advance();
            self.skip_newlines();

            while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
                && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
            {
                if let Some(stmt) = self.parse_statement()? {
                    finally_body.push(stmt);
                }
                self.skip_newlines();
            }
        }

        self.expect(&TokenKind::End)?;

        Ok(Some(Statement::Try {
            body,
            catch_var,
            catch_body,
            finally_body,
        }))
    }

    fn parse_test(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'test'
        self.skip_newlines();

        let name = match self.current() {
            Some(Token {
                kind: TokenKind::Text(s),
                ..
            }) => {
                let name = s.clone();
                self.advance();
                name
            }
            Some(Token {
                kind: TokenKind::Identifier(s),
                ..
            }) => {
                let name = s.clone();
                self.advance();
                name
            }
            _ => return Err(Error::Parser("Expected test name".to_string(), self.span())),
        };

        self.skip_newlines();

        let mut body = Vec::new();
        while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
            && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
        {
            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
            self.skip_newlines();
        }

        self.expect(&TokenKind::End)?;

        Ok(Some(Statement::Test { name, body }))
    }

    /// expect <actual> to be <expected>
    /// expect <actual> to <expected>
    fn parse_expect(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'expect'

        let actual = self.parse_expression()?;
        self.expect(&TokenKind::To)?;

        if let Some(Token {
            kind: TokenKind::Identifier(word),
            ..
        }) = self.current()
        {
            if word == "be" {
                self.advance();
            }
        }

        let expected = self.parse_expression()?;

        Ok(Some(Statement::Expr(Expr::Expect {
            actual: Box::new(actual),
            expected: Box::new(expected),
        })))
    }

    fn is_expression_start(&self) -> bool {
        matches!(
            self.current().map(|t| &t.kind),
            Some(TokenKind::Number(_))
                | Some(TokenKind::Text(_))
                | Some(TokenKind::YesNo(_))
                | Some(TokenKind::Nothing)
                | Some(TokenKind::Identifier(_))
                | Some(TokenKind::LeftParen)
                | Some(TokenKind::LeftBracket)
                | Some(TokenKind::LeftBrace)
                | Some(TokenKind::Not)
                | Some(TokenKind::Minus)
        )
    }

    fn parse_expression(&mut self) -> Result<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut left = self.parse_and()?;
        let mut chained = 0usize;

        while let Some(Token {
            kind: TokenKind::Or,
            ..
        }) = self.current()
        {
            self.advance();
            let right = self.parse_and()?;
            chained += 1;
            self.enter_nesting()?;
            left = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        self.leave_nesting(chained);

        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_comparison()?;
        let mut chained = 0usize;

        while let Some(Token {
            kind: TokenKind::And,
            ..
        }) = self.current()
        {
            self.advance();
            let right = self.parse_comparison()?;
            chained += 1;
            self.enter_nesting()?;
            left = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        self.leave_nesting(chained);

        Ok(left)
    }

    /// Whether the token `offset` positions after the current one is an
    /// identifier spelled `word`. The word forms of comparison (`greater`,
    /// `less`, `than`, `equal`) are plain identifiers to the lexer, so they
    /// are matched by name here rather than by token kind.
    fn is_word_ahead(&self, offset: usize, word: &str) -> bool {
        matches!(
            self.tokens.get(self.pos + offset),
            Some(Token {
                kind: TokenKind::Identifier(found),
                ..
            }) if found == word
        )
    }

    /// Whether the whole `or equal to` tail of a compound word comparison sits
    /// at the current position. It is matched as one phrase rather than as an
    /// `or` for `parse_or` to bind later, so `x is greater than or equal to y`
    /// is one comparison and not `(x is greater than y) or equal to y`.
    fn or_equal_to_ahead(&self) -> bool {
        matches!(
            self.current(),
            Some(Token {
                kind: TokenKind::Or,
                ..
            })
        ) && self.is_word_ahead(1, "equal")
            && matches!(
                self.tokens.get(self.pos + 2),
                Some(Token {
                    kind: TokenKind::To,
                    ..
                })
            )
    }

    /// Reads the comparison operator that follows an `is`, in either its word
    /// form (`is greater than or equal to`) or its symbolic one (`is =`,
    /// `is not`). Bare `is` is equality; `is in` is membership.
    ///
    /// A word only counts as an operator when the *whole* phrase is there: a
    /// variable named `greater` or `than` still compares for equality, so
    /// nothing is consumed on a partial match and the rest of the line is
    /// parsed exactly as it was before this phase. `in` needs no such guard
    /// because it is a keyword: it cannot name a variable, so nothing that used
    /// to parse as `is <variable>` can be mistaken for `is in`.
    fn parse_is_operator(&mut self) -> BinaryOp {
        if self.is_word_ahead(0, "equal") && matches!(self.current_at(1), Some(TokenKind::To)) {
            self.advance();
            self.advance();
            return BinaryOp::Equal;
        }

        let adjectives = [
            ("greater", BinaryOp::Greater, BinaryOp::GreaterEqual),
            ("less", BinaryOp::Less, BinaryOp::LessEqual),
        ];
        for (adjective, strict_op, loose_op) in adjectives {
            if !(self.is_word_ahead(0, adjective) && self.is_word_ahead(1, "than")) {
                continue;
            }
            self.advance();
            self.advance();
            if self.or_equal_to_ahead() {
                self.advance();
                self.advance();
                self.advance();
                return loose_op;
            }
            return strict_op;
        }

        match self.current() {
            Some(Token {
                kind: TokenKind::Equal,
                ..
            }) => {
                self.advance();
                BinaryOp::Equal
            }
            Some(Token {
                kind: TokenKind::Not,
                ..
            }) => {
                self.advance();
                BinaryOp::NotEqual
            }
            Some(Token {
                kind: TokenKind::In,
                ..
            }) => {
                self.advance();
                BinaryOp::In
            }
            // "is" alone means equality
            _ => BinaryOp::Equal,
        }
    }

    /// The kind of the token `offset` positions after the current one.
    fn current_at(&self, offset: usize) -> Option<&TokenKind> {
        self.tokens.get(self.pos + offset).map(|token| &token.kind)
    }

    fn parse_comparison(&mut self) -> Result<Expr> {
        let mut left = self.parse_addition()?;
        let mut chained = 0usize;

        loop {
            let op = match self.current() {
                Some(Token {
                    kind: TokenKind::Is,
                    ..
                }) => {
                    self.advance();
                    self.parse_is_operator()
                }
                Some(Token {
                    kind: TokenKind::Equal,
                    ..
                }) => {
                    self.advance();
                    BinaryOp::Equal
                }
                Some(Token {
                    kind: TokenKind::NotEqual,
                    ..
                }) => {
                    self.advance();
                    BinaryOp::NotEqual
                }
                Some(Token {
                    kind: TokenKind::Less,
                    ..
                }) => {
                    self.advance();
                    if let Some(Token {
                        kind: TokenKind::Equal,
                        ..
                    }) = self.current()
                    {
                        self.advance();
                        BinaryOp::LessEqual
                    } else {
                        BinaryOp::Less
                    }
                }
                Some(Token {
                    kind: TokenKind::Greater,
                    ..
                }) => {
                    self.advance();
                    if let Some(Token {
                        kind: TokenKind::Equal,
                        ..
                    }) = self.current()
                    {
                        self.advance();
                        BinaryOp::GreaterEqual
                    } else {
                        BinaryOp::Greater
                    }
                }
                Some(Token {
                    kind: TokenKind::LessEqual,
                    ..
                }) => {
                    self.advance();
                    BinaryOp::LessEqual
                }
                Some(Token {
                    kind: TokenKind::GreaterEqual,
                    ..
                }) => {
                    self.advance();
                    BinaryOp::GreaterEqual
                }
                _ => break,
            };

            let right = self.parse_addition()?;
            chained += 1;
            self.enter_nesting()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }

        self.leave_nesting(chained);

        Ok(left)
    }

    fn parse_addition(&mut self) -> Result<Expr> {
        let mut left = self.parse_multiplication()?;
        let mut chained = 0usize;

        while let Some(token) = self.current() {
            let op = match &token.kind {
                TokenKind::Plus => Some(BinaryOp::Add),
                TokenKind::Minus => Some(BinaryOp::Sub),
                _ => None,
            };

            if let Some(op) = op {
                self.advance();
                let right = self.parse_multiplication()?;
                chained += 1;
                self.enter_nesting()?;
                left = Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        self.leave_nesting(chained);

        Ok(left)
    }

    fn parse_multiplication(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        let mut chained = 0usize;

        while let Some(token) = self.current() {
            let op = match &token.kind {
                TokenKind::Star => Some(BinaryOp::Mul),
                TokenKind::Slash => Some(BinaryOp::Div),
                TokenKind::Mod => Some(BinaryOp::Mod),
                _ => None,
            };

            if let Some(op) = op {
                self.advance();
                let right = self.parse_unary()?;
                chained += 1;
                self.enter_nesting()?;
                left = Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }

        self.leave_nesting(chained);

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        if let Some(token) = self.current() {
            match &token.kind {
                TokenKind::Not => {
                    self.advance();
                    self.enter_nesting()?;
                    let expr = self.parse_unary()?;
                    self.leave_nesting(1);
                    return Ok(Expr::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(expr),
                    });
                }
                TokenKind::Minus => {
                    self.advance();
                    self.enter_nesting()?;
                    let expr = self.parse_unary()?;
                    self.leave_nesting(1);
                    return Ok(Expr::Unary {
                        op: UnaryOp::Neg,
                        expr: Box::new(expr),
                    });
                }
                _ => {}
            }
        }

        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut expr = self.parse_primary()?;

        loop {
            if let Some(Token {
                kind: TokenKind::Dot,
                ..
            }) = self.current()
            {
                self.advance();
                let property = match self.current() {
                    Some(Token {
                        kind: TokenKind::Identifier(name),
                        ..
                    }) => {
                        let n = name.clone();
                        self.advance();
                        n
                    }
                    _ => {
                        return Err(Error::Parser(
                            "Expected property name".to_string(),
                            self.span(),
                        ))
                    }
                };
                expr = Expr::Property {
                    object: Box::new(expr),
                    property,
                };
            } else if let Some(Token {
                kind: TokenKind::LeftParen,
                ..
            }) = self.current()
            {
                self.advance();
                let mut args = Vec::new();

                self.enter_nesting()?;
                while self.current().map(|t| &t.kind) != Some(&TokenKind::RightParen)
                    && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
                {
                    args.push(self.parse_expression()?);

                    if let Some(Token {
                        kind: TokenKind::Comma,
                        ..
                    }) = self.current()
                    {
                        self.advance();
                    }
                }
                self.leave_nesting(1);

                self.expect(&TokenKind::RightParen)?;

                match expr {
                    Expr::Variable(name) => {
                        expr = Expr::Call { name, args };
                    }
                    Expr::Property { object, property } => {
                        // `receiver.method(args)` — the runtime decides whether
                        // the receiver names an object (a method) or a module
                        // (a `module_function` builtin), so both forms stay one
                        // expression rather than being guessed here.
                        expr = Expr::MethodCall {
                            receiver: object,
                            method: property,
                            args,
                        };
                    }
                    _ => {
                        return Err(Error::Parser(
                            "Expected function name".to_string(),
                            self.span(),
                        ));
                    }
                }
            } else if let Some(Token {
                kind: TokenKind::LeftBracket,
                ..
            }) = self.current()
            {
                self.advance();
                self.enter_nesting()?;
                let index = self.parse_expression()?;
                self.leave_nesting(1);
                self.expect(&TokenKind::RightBracket)?;
                expr = Expr::Index {
                    object: Box::new(expr),
                    index: Box::new(index),
                };
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        let token = self
            .current()
            .ok_or_else(|| Error::Parser("Unexpected end of input".to_string(), self.span()))?
            .clone();

        match &token.kind {
            TokenKind::Number(n) => {
                self.advance();
                Ok(Expr::Number(*n))
            }
            TokenKind::Text(s) => {
                self.advance();
                Ok(Expr::Text(s.clone()))
            }
            TokenKind::YesNo(b) => {
                self.advance();
                Ok(Expr::YesNo(*b))
            }
            TokenKind::Nothing => {
                self.advance();
                Ok(Expr::Nothing)
            }
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(Expr::Variable(name.clone()))
            }
            // `this` is a plain variable that only a method call binds, so a
            // program that reaches for it outside a method gets the ordinary
            // "Unknown variable 'this'" error rather than a silent nothing.
            TokenKind::This => {
                self.advance();
                Ok(Expr::Variable("this".to_string()))
            }
            TokenKind::LeftParen => {
                self.advance();
                self.enter_nesting()?;
                let expr = self.parse_expression()?;
                self.leave_nesting(1);
                self.expect(&TokenKind::RightParen)?;
                Ok(expr)
            }
            TokenKind::LeftBracket => {
                self.advance();
                let mut items = Vec::new();

                self.enter_nesting()?;
                while self.current().map(|t| &t.kind) != Some(&TokenKind::RightBracket)
                    && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
                {
                    items.push(self.parse_expression()?);

                    if let Some(Token {
                        kind: TokenKind::Comma,
                        ..
                    }) = self.current()
                    {
                        self.advance();
                    }
                }

                self.leave_nesting(1);
                self.expect(&TokenKind::RightBracket)?;
                Ok(Expr::List(items))
            }
            TokenKind::LeftBrace => {
                self.advance();
                let mut fields = Vec::new();

                self.enter_nesting()?;
                while self.current().map(|t| &t.kind) != Some(&TokenKind::RightBrace)
                    && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
                {
                    let key = match self.current() {
                        Some(Token {
                            kind: TokenKind::Identifier(name),
                            ..
                        }) => {
                            let n = name.clone();
                            self.advance();
                            n
                        }
                        _ => {
                            return Err(Error::Parser(
                                "Expected field name".to_string(),
                                self.span(),
                            ))
                        }
                    };

                    self.expect(&TokenKind::Colon)?;
                    let value = self.parse_expression()?;
                    fields.push((key, value));

                    if let Some(Token {
                        kind: TokenKind::Comma,
                        ..
                    }) = self.current()
                    {
                        self.advance();
                    }
                }

                self.leave_nesting(1);
                self.expect(&TokenKind::RightBrace)?;
                Ok(Expr::Record(fields))
            }
            _ => Err(Error::Parser(
                format!("Unexpected token {:?}", token.kind),
                token.span(),
            )),
        }
    }
}

pub fn parse(tokens: Vec<Token>) -> Result<Program> {
    let mut parser = Parser::new(tokens);
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::Lexer;

    #[test]
    fn test_parse_say() {
        let tokens = Lexer::tokenize(r#"say "Hello""#).unwrap();
        let program = parse(tokens).unwrap();
        assert_eq!(program.statements.len(), 1);
    }

    #[test]
    fn test_parse_set() {
        let tokens = Lexer::tokenize("set x to 10").unwrap();
        let program = parse(tokens).unwrap();
        assert_eq!(program.statements.len(), 1);
    }
}
