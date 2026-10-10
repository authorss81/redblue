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

    // to (x, y) ... end — a function literal, so an anonymous function is a
    // value rather than a declaration. `docs/GRAMMAR.md` § 6 and SPEC.md
    // § First-Class Functions.
    FunctionLiteral {
        params: Vec<String>,
        body: Vec<Stmt>,
    },

    // expect expression to be expected_value
    Expect {
        actual: Box<Expr>,
        expected: Box<Expr>,
    },

    // might fail <call> — a call whose failure is discarded, yielding `nothing`
    // instead of ending the program. The right-hand side is required to be a
    // call, so this is the expression-position form of a fallible operation
    // rather than a general "ignore any failure" prefix; `docs/GRAMMAR.md`
    // § 5.11 and SPEC.md § Error Handling.
    MightFail(Box<Expr>),
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

    // repeat ... until condition — no `end`: the `until` line closes it
    RepeatUntil {
        body: Vec<Stmt>,
        condition: Expr,
        /// Where the condition was written, which is the `until` line rather than
        /// the `repeat` line the statement's own span names.
        ///
        /// The body runs before the condition is read, so the two are not at the
        /// same place in the source, and a failure in the condition belongs on
        /// the line its author wrote it on. The counted form's condition is on
        /// its `while`'s keyword line, which is why every other loop's span and
        /// this one cover the whole statement.
        condition_span: Span,
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

    // module Name ... export ... end
    Module {
        name: String,
        body: Vec<Stmt>,
    },

    // export name, ... | export all
    Export {
        names: Vec<String>,
        all: bool,
    },

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

/// The statements a module file's body is: the inside of its `module ...
/// end` declaration where it has one, and its top level where it does not.
///
/// The two spellings of a module are the same module — `module MathUtils ...
/// export all end` and a bare file of `constant` and `to` are both read as the
/// declarations they hold — so everything that reads a module's names reads
/// them through here.
pub fn module_body(program: &Program) -> &[Stmt] {
    let declarations: Vec<&Stmt> = program
        .statements
        .iter()
        .filter(|stmt| matches!(stmt.statement, Statement::Module { .. }))
        .collect();
    if let [only] = declarations.as_slice() {
        return match &only.statement {
            Statement::Module { body, .. } => body,
            _ => &program.statements,
        };
    }
    &program.statements
}

/// The names a module body's top-level statements declare: every `to`
/// function's, plus every `set` and `constant`.
///
/// Read by the VM to decide what `export all` publishes and what an `export`
/// naming one thing has already found. Takes the body rather than a file so a
/// declared module and a loaded one are read by the same rule.
pub fn module_declared_names(body: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in body {
        match &stmt.statement {
            Statement::Function { name, .. } => names.push(name.clone()),
            Statement::Set { name, .. } | Statement::Constant { name, .. } => {
                names.push(name.clone())
            }
            _ => {}
        }
    }
    names
}

/// What a module body's last `export` says: the names it lists, and whether it
/// says `all`. `None` is a body with no `export` at all.
///
/// The *last* `export` is what counts, because that is the one written where
/// the declaration closes — the position `docs/GRAMMAR.md` § 3.1 gives it.
///
/// The one rule for reading it, shared by the tree-walking VM (which reports a
/// bad `export` when the declaration runs) and by the compiler (which has to put
/// the same names in the file), so the two cannot disagree about which `export`
/// counts.
pub fn module_exports(body: &[Stmt]) -> Option<(Vec<String>, bool)> {
    let mut found = None;
    for stmt in body {
        if let Statement::Export { names, all } = &stmt.statement {
            found = Some((names.clone(), *all));
        }
    }
    found
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
    ///
    /// `module` is one of them, for the same reason `if` is: [`Parser::parse_module`]
    /// parses its body by calling back into [`Parser::parse_statement`], which is
    /// one more level of parser recursion per nesting. Counting it here and not in
    /// [`opens_block_at_statement_start`] left nested `module` declarations with no
    /// budget at all, so a file of them recursed until the stack overflowed and
    /// aborted the process — rather than being the
    /// `Blocks nest more than 64 levels deep` every other block form gets.
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
                | Some(TokenKind::Module)
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
            TokenKind::Module => self.parse_module()?,
            TokenKind::Export => self.parse_export()?,
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

                        let alias =
                            match self.current().map(|t| &t.kind) {
                                // `as` is the spelling `SPEC.md` and
                                // `docs/GRAMMAR.md` § 3.1 write; `to` is the older
                                // one, kept because programs in `tests/` use it.
                                Some(TokenKind::As) | Some(TokenKind::To) => {
                                    self.advance();
                                    match self.current() {
                                        Some(Token {
                                            kind: TokenKind::Identifier(alias),
                                            ..
                                        }) => {
                                            let a = alias.clone();
                                            self.advance();
                                            Some(a)
                                        }
                                        _ => return Err(Error::Parser(
                                            "Expected alias name after the import alias keyword"
                                                .to_string(),
                                            self.span(),
                                        )),
                                    }
                                }
                                _ => None,
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

    /// Whether the token at the cursor is the `by` that LABELS the next call
    /// argument, as in `split("a,b", by ",")`.
    ///
    /// Both spellings of the token count, for the same reason
    /// [`Parser::at_range_step_marker`] takes both: `by` is read as a word
    /// rather than taken from the keyword table, so `TokenKind::By` is honoured
    /// in case `by` is ever promoted to a keyword without this arm needing to
    /// change.
    ///
    /// The second half of the test is what makes it a *label* rather than a
    /// variable: the token after `by` must be one that can begin an expression,
    /// judged by the same table [`Parser::kind_starts_expression`] judges the
    /// token at the cursor. So `split("a,b", by ",")` labels the separator, and
    /// `grow(by)` — where `by` is the parameter the language has always let a
    /// program name, with `)` after it — passes a variable.
    ///
    /// The test is the *positive* half of that table, never a blacklist of the
    /// tokens that do not begin an expression. A blacklist has to be kept in
    /// step with every new token kind, and when it was not it read `f(by + 1)`
    /// and `f(by is 1)` as labelled, dropping a `by` the program meant as a
    /// variable and turning working code into a parse error.
    ///
    /// A newline after `by` is skipped on both sides of the decision: the
    /// lookahead here walks over it and [`Parser::parse_postfix`] skips it again
    /// after the `advance()`. Classifying the label by looking past a newline
    /// while parsing it without would be a disagreement between the two halves.
    fn at_argument_label(&self) -> bool {
        if !self.at_range_step_marker() {
            return false;
        }
        let mut ahead = self.pos + 1;
        while ahead < self.tokens.len() && matches!(self.tokens[ahead].kind, TokenKind::Newline) {
            ahead += 1;
        }
        self.tokens
            .get(ahead)
            .map(|token| Self::kind_starts_expression(&token.kind))
            .unwrap_or(false)
    }

    /// Whether `callee` is one of the builtins SPEC.md writes an argument label
    /// for.
    ///
    /// SPEC.md:1017-1018 gives the label exactly two spellings — `text.split`,
    /// `text.join`, and their flat names — so the label is read for those and
    /// nowhere else. A label that any call accepted would be one more way to
    /// write a call that happens to work, and would silently swallow the `by` in
    /// `pow(2, by 10)` for a program that meant a variable. Refusing it there
    /// keeps `by` meaning what the specification says it means: `by` labels the
    /// separator of `split`/`join`, and everywhere else it is an ordinary word.
    fn callee_takes_argument_label(callee: Option<&str>) -> bool {
        matches!(callee, Some("split") | Some("join"))
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
        if self.opens_post_test_loop() {
            return self.parse_repeat_until();
        }
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

    /// Whether this `repeat` opens the post-test loop rather than the counted
    /// one.
    ///
    /// `repeat` opens two forms and the tokens after it are what tell them
    /// apart. The counted form is `repeat <expression> times`, all on one line,
    /// and the post-test form is `repeat` and a body an `until` ends — so what
    /// follows the keyword decides between them.
    ///
    /// Looking for an `until` anywhere would not do: it is at the *end* of the
    /// body, and a counted loop whose body holds a post-test loop of its own has
    /// one too, at a nesting level this parser is nowhere near yet. So the scan
    /// stops at the end of the line, where the counted form's `times` and the
    /// post-test form's `until` are the two keywords that can settle it.
    fn opens_post_test_loop(&self) -> bool {
        match self.current().map(|token| &token.kind) {
            // A `repeat` on a line of its own opens a body the lines after it
            // hold; the counted form always has its count on the keyword's line.
            Some(TokenKind::Newline) => true,
            // Nothing at all follows the keyword, so there is no count for the
            // counted form to read and this is a post-test loop that never got
            // its `until`. Saying so is a better diagnostic than an expression
            // error about a count that was never written.
            Some(TokenKind::Eof) | None => true,
            Some(_) => self.line_ends_in_until(),
        }
    }

    /// Whether the first of `until` and `times` written on the current line is
    /// an `until`.
    ///
    /// Only this line is looked at, for the reason
    /// [`Parser::opens_post_test_loop`] gives: a keyword on a later line belongs
    /// to a statement this parser has not reached. Within the line, the first of
    /// the two wins — `repeat 3 times` is the counted form whatever follows it,
    /// and `repeat set n to n + 1 until n is 3` is the post-test one.
    fn line_ends_in_until(&self) -> bool {
        let Some(first) = self.current() else {
            return false;
        };
        let line = first.line;
        for token in self.tokens[self.pos..].iter() {
            if token.line != line {
                return false;
            }
            match token.kind {
                TokenKind::Until => return true,
                TokenKind::Times => return false,
                _ => {}
            }
        }
        false
    }

    /// `repeat` { statement } `until` expression — the post-test loop, which is
    /// closed by the `until` line rather than by an `end` of its own.
    ///
    /// Its body runs before its condition is read, so the condition is not
    /// looked at until the body has had a turn: a `while`'s condition comes out
    /// false and no body runs at all, and this one's comes out true and one body
    /// has already run.
    fn parse_repeat_until(&mut self) -> Result<Option<Statement>> {
        self.skip_newlines();

        let mut body = Vec::new();
        while !matches!(
            self.current().map(|token| &token.kind),
            Some(TokenKind::Until) | Some(TokenKind::End) | Some(TokenKind::Eof)
        ) {
            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
            self.skip_newlines();
        }

        // The body stops at the `until`, and the loop is only written by one.
        // Reaching here without it means the program ends first — or ends with an
        // `end` that belongs to whatever block this loop was written inside — so
        // the diagnostic says which keyword is missing rather than reporting the
        // token found in its place.
        let until = self.expect(&TokenKind::Until).map_err(|_| {
            Error::Parser(
                "A `repeat` loop is closed by an `until <condition>`, and this one has not got one"
                    .to_string(),
                self.span(),
            )
        })?;
        let condition = self.parse_expression()?;

        Ok(Some(Statement::RepeatUntil {
            body,
            condition,
            condition_span: until.span(),
        }))
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

    /// Parses the function literal `to [ '(' parameter { ',' parameter } ')' ]
    /// { statement } [ 'end' ]` — `docs/GRAMMAR.md` § 6 and SPEC.md
    /// § First-Class Functions.
    ///
    /// The parameter list is optional, so `to give back 42 end` and
    /// `to () give back 42 end` are both literals of no parameters. The `end` is
    /// optional only for a body written entirely on the `to`'s own line, which
    /// is the form SPEC.md uses; a body that runs onto a second line without one
    /// is a spanned `ParserError` that names the `end` the program has to write.
    ///
    /// The body is parsed as statements, so it claims the same block budget as
    /// every other `... end` form and cannot nest past [`MAX_BLOCK_DEPTH`].
    fn parse_function_literal(&mut self) -> Result<Expr> {
        self.enter_block()?;
        let result = self.parse_function_literal_inner();
        self.leave_block();
        result
    }

    fn parse_function_literal_inner(&mut self) -> Result<Expr> {
        let open_line = self.span().line;
        self.advance(); // consume 'to'

        let params = self.parse_optional_params()?;
        let body = self.parse_literal_body(open_line)?;

        match self.current().map(|t| &t.kind) {
            Some(TokenKind::End) => {
                self.advance();
            }
            // The one-line form SPEC.md writes — `to (x) give back x * 2` and
            // `list.map([1, 2, 3], to (x) give back x * 2)` — where the end of
            // the `to`'s own line closes the literal. Only a body that never
            // left that line may close this way, so a block whose `end` is
            // missing is still the unterminated literal to report.
            _ if self.last_consumed_line() == open_line => {}
            _ => {
                return Err(Error::Parser(
                    "Expected 'end' to close the function literal".to_string(),
                    self.span(),
                ))
            }
        }

        Ok(Expr::FunctionLiteral { params, body })
    }

    /// The body of a function literal.
    ///
    /// SPEC.md writes a literal two ways — a block that runs to its `end`, and
    /// the one-line `to (x) give back x * 2` whose line ends it — and which one
    /// this is is settled by the parameters. More of the body on the same line
    /// means the body is what is on that line; the parameters ending the line
    /// means the body is a block whose `end` is required.
    fn parse_literal_body(&mut self, open_line: usize) -> Result<Vec<Stmt>> {
        self.skip_newlines();

        if self.span().line != open_line {
            return self.parse_block_body();
        }

        let mut body = Vec::new();
        while self.span().line == open_line && self.can_begin_statement() {
            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
        }

        Ok(body)
    }

    /// Whether the upcoming token could begin a statement — used by the
    /// one-line literal body to stop where the literal ends rather than reading
    /// whatever follows it on the line, such as the `,` or `)` that closes the
    /// call the literal was written into.
    fn can_begin_statement(&self) -> bool {
        matches!(
            self.current().map(|token| &token.kind),
            Some(TokenKind::Say)
                | Some(TokenKind::Print)
                | Some(TokenKind::Set)
                | Some(TokenKind::Constant)
                | Some(TokenKind::Module)
                | Some(TokenKind::Export)
                | Some(TokenKind::Import)
                | Some(TokenKind::If)
                | Some(TokenKind::Unless)
                | Some(TokenKind::For)
                | Some(TokenKind::Repeat)
                | Some(TokenKind::While)
                | Some(TokenKind::Break)
                | Some(TokenKind::Skip)
                | Some(TokenKind::Return)
                | Some(TokenKind::GiveBack)
                | Some(TokenKind::To)
                | Some(TokenKind::Object)
                | Some(TokenKind::Try)
                | Some(TokenKind::Test)
                | Some(TokenKind::Expect)
        ) || self.is_expression_start()
    }

    /// The line of the token most recently read, or `0` before the first one.
    fn last_consumed_line(&self) -> usize {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map(|token| token.span().line)
            .unwrap_or(0)
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
        let params = self.parse_optional_params()?;

        let body = self.parse_block_body()?;

        self.expect(&TokenKind::End)?;

        Ok((name, params, body))
    }

    /// The `(a, b)` of a callable, when it has one: the parameter list is
    /// optional, so `to f give back 1 end` and `to f() give back 1 end` are
    /// the same zero-parameter declaration.
    fn parse_optional_params(&mut self) -> Result<Vec<String>> {
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

        Ok(params)
    }

    /// The statements of an `... end` block, up to but not including the
    /// `end`, so each caller can close the block with the message its own form
    /// owes the author.
    fn parse_block_body(&mut self) -> Result<Vec<Stmt>> {
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

        Ok(body)
    }

    /// `module Name ... export ... end` — the declaration form `SPEC.md` §
    /// Modules and `docs/GRAMMAR.md` § 3.1 write.
    ///
    /// The body is any statement, so an `export` inside it parses as the
    /// statement it is and the VM reads what it published. An unclosed module
    /// is refused here rather than swallowing the rest of the file.
    fn parse_module(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'module'

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
                    "Expected module name".to_string(),
                    self.span(),
                ))
            }
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

        Ok(Some(Statement::Module { name, body }))
    }

    /// `export name, ...` or `export all` — what a module declaration
    /// publishes.
    fn parse_export(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'export'

        let mut names = Vec::new();
        let mut all = false;
        loop {
            // `all` is not a keyword: it is a word only in this one position,
            // so it is read as the identifier it lexes to and nothing else in
            // the language changes.
            match self.current() {
                Some(Token {
                    kind: TokenKind::Identifier(name),
                    ..
                }) if name == "all" => {
                    self.advance();
                    all = true;
                }
                Some(Token {
                    kind: TokenKind::Identifier(name),
                    ..
                }) => {
                    let n = name.clone();
                    self.advance();
                    names.push(n);
                }
                _ => {
                    return Err(Error::Parser(
                        "Expected a name to export after 'export'".to_string(),
                        self.span(),
                    ))
                }
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

        Ok(Some(Statement::Export { names, all }))
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
        self.current()
            .map(|token| Self::kind_starts_expression(&token.kind))
            .unwrap_or(false)
    }

    /// Whether a token of this kind can begin an expression.
    ///
    /// Split out from [`Parser::is_expression_start`] so a decision made about a
    /// token *ahead* of the cursor is taken by the same table as one made about
    /// the token at it. A second, hand-written list would drift — and when it
    /// drifted the wrong way it read a two-token blacklist as if it were this
    /// set, so a variable `by` in front of an operator became a label.
    fn kind_starts_expression(kind: &TokenKind) -> bool {
        match kind {
            TokenKind::Number(_)
            | TokenKind::Text(_)
            | TokenKind::YesNo(_)
            | TokenKind::Nothing
            | TokenKind::Identifier(_)
            | TokenKind::LeftParen
            | TokenKind::LeftBracket
            | TokenKind::LeftBrace
            | TokenKind::Not
            | TokenKind::Minus
            | TokenKind::To => true,
            // `might fail f()` is an expression in its own right, so a
            // statement may begin with one and an argument list may hold one.
            TokenKind::MightFail => true,
            _ => false,
        }
    }

    fn parse_expression(&mut self) -> Result<Expr> {
        if self.current().map(|t| &t.kind) == Some(&TokenKind::MightFail) {
            return self.parse_might_fail();
        }
        self.parse_or()
    }

    /// Parses `might fail <call>` — the expression form of a fallible call, whose
    /// failure is discarded rather than ending the program.
    ///
    /// The lexer gives `might` and `fail` the same token, so the two-word spelling
    /// arrives as two of them, and both halves are required here. The single-word
    /// spellings `might f()` and `fail f()` are *not* the form: they are refused
    /// rather than read as the guard, because the pair is what every documented
    /// example writes and a prefix that silently accepted either half would make a
    /// program that means something else by it — `might` as a variable name, say —
    /// a guard instead of a refusal.
    ///
    /// Only a call may be guarded. A prefix that took any expression would accept
    /// `might fail 1 + 1`, whose right-hand side cannot fail at all, and would
    /// read as though discarding a failure were something a number could do — so
    /// the shape is refused at parse time, with the position of the expression
    /// that is not a call.
    fn parse_might_fail(&mut self) -> Result<Expr> {
        let first = self.span();
        self.advance(); // consume 'might'
        if self.current().map(|t| &t.kind) != Some(&TokenKind::MightFail) {
            return Err(Error::Parser(
                "`might` is half of the guard on its own: the form is `might fail \
                 <call>`, not `might <call>` and not `fail <call>`"
                    .to_string(),
                first,
            ));
        }
        self.advance(); // consume 'fail', the pair's other half

        if !self.is_expression_start() {
            return Err(Error::Parser(
                "Expected a call after `might fail`".to_string(),
                self.span(),
            ));
        }

        let operand_span = self.span();
        self.enter_nesting()?;
        let operand = self.parse_expression()?;
        self.leave_nesting(1);

        match operand {
            Expr::Call { .. } | Expr::MethodCall { .. } => {}
            _ => {
                return Err(Error::Parser(
                    "`might fail` must be followed by a call, which this is not".to_string(),
                    operand_span,
                ))
            }
        }

        Ok(Expr::MightFail(Box::new(operand)))
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

                // The callee is known before its arguments are read, so whether a
                // `by` labels the argument that follows is decided by the name
                // being called rather than by whatever the call happens to be.
                // SPEC.md:1017-1018 writes the label for `text.split` and
                // `text.join` only, so `callee_takes_argument_label` allows
                // exactly those; every other call reads `by` as the variable it
                // has always been able to be.
                let callee = match &expr {
                    Expr::Variable(name) => Some(name.as_str()),
                    Expr::Property { property, .. } => Some(property.as_str()),
                    _ => None,
                };
                let label_allowed = Self::callee_takes_argument_label(callee);

                self.enter_nesting()?;
                while self.current().map(|t| &t.kind) != Some(&TokenKind::RightParen)
                    && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof)
                {
                    // A `by` here is an argument *label*, not a variable read:
                    // `text.split("a,b", by ",")` is the spelling SPEC.md § text
                    // writes (`SPEC.md:1017`), and before this arm existed the
                    // parser read `by` as an ordinary identifier, so the
                    // analyzer refused the program with `Unknown variable 'by'`
                    // before it ever ran.
                    //
                    // It is read positionally, for the reason
                    // [`Parser::at_range_step_marker`] gives: `by` is not a
                    // reserved word and must not become one, or `to can grow(by)`
                    // and `say by` would stop working (`tests/bytecode_test.rs:614`).
                    // A label is only a label when something follows it, so
                    // `grow(by)` — passing the variable — is untouched: the
                    // `)` after `by` is not the start of an expression. That is
                    // the whole disambiguation, and it is why `by` is checked
                    // here rather than added to `KEYWORDS`.
                    if label_allowed && self.at_argument_label() {
                        self.advance();
                        // The label and the value it labels may be written on
                        // different lines. `at_argument_label` looked past the
                        // newline to decide, so this side skips it too rather
                        // than classifying one shape and parsing another.
                        self.skip_newlines();
                    }
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
            // A `to` here is a function literal, never a declaration: a
            // declaration is a statement, and [`Parser::parse_statement`] takes
            // that one before any expression is reached. `to name() ... end` at
            // the start of a line is therefore still the declaration it has
            // always been, while `to (x) ... end` and `to ... end` after an
            // operator are the value SPEC.md documents.
            TokenKind::To => self.parse_function_literal(),
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

/// Whether `kind` is a token that opens a `... end` block wherever it can begin a
/// statement.
///
/// [`Parser::opens_block`] asks the same question of the token about to be read,
/// but only after [`Parser::parse_statement_inner`] has decided what that
/// statement is — `to` opens a function body there and is a range bound in
/// `for each i from 1 to 10`. Counting from the token stream alone has no such
/// context, so it is only right where a statement can begin; see
/// [`open_block_depth`]. Both tables answer the same question for every other
/// block form, `module` included: it is a block wherever it can begin a
/// statement.
fn opens_block_at_statement_start(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::If
            | TokenKind::Unless
            | TokenKind::For
            | TokenKind::Repeat
            | TokenKind::While
            | TokenKind::To
            | TokenKind::Object
            | TokenKind::Try
            | TokenKind::Test
            | TokenKind::Module
    )
}

/// How many `... end` blocks `tokens` leaves open: positive while the program is
/// unfinished, zero once every block is closed, and negative only for source
/// that closes a block it never opened.
///
/// This is what tells a REPL whether to read another line. It is counted from
/// the token stream rather than from the text, so `say "end"` closes nothing and
/// a block keyword inside a string is not an opener — matching the last word of
/// a line instead got both wrong, and a stray `end` swallowed every line after
/// it.
///
/// An opener counts only where a statement can begin. That is what keeps the two
/// `to`s that are not function declarations out of the count: the range bound in
/// `for each i from 1 to 10` and the alias in `import MathUtils to M` are both
/// mid-statement, so neither reads as a body waiting for an `end`.
pub fn open_block_depth(tokens: &[Token]) -> i32 {
    let mut depth = 0;
    let mut at_statement_start = true;

    for token in tokens {
        match &token.kind {
            TokenKind::Newline => at_statement_start = true,
            TokenKind::End => {
                depth -= 1;
                at_statement_start = false;
            }
            // `repeat ... until <condition>` is opened by its `repeat` and closed
            // by its `until` — it has no `end` of its own, so a REPL waiting for
            // one would ask for a line the form is never written with.
            TokenKind::Until => {
                depth -= 1;
                at_statement_start = false;
            }
            // A branch of a block already counted rather than a statement of its
            // own, so the statement-start rule still holds after it: `else if x
            // then` opens one more block.
            TokenKind::Else | TokenKind::Catch | TokenKind::Finally => {
                at_statement_start = true;
            }
            kind if at_statement_start && opens_block_at_statement_start(kind) => {
                depth += 1;
                at_statement_start = false;
            }
            _ => at_statement_start = false,
        }
    }

    depth
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

    fn depth_of(source: &str) -> i32 {
        let tokens = Lexer::tokenize(source).expect("the source should lex");
        open_block_depth(&tokens)
    }

    /// The post-test loop is the one block form `end` does not close, so the
    /// depth a REPL reads to decide whether the program is finished has to be
    /// closed by its `until` — otherwise every post-test loop typed at a prompt
    /// asks for an `end` the form is never written with.
    #[test]
    fn edge_a_post_test_loop_is_closed_by_its_until_and_not_by_an_end() {
        let cases = [
            "repeat\nset n to 0\nuntil n is 1",
            "repeat\nuntil yes is yes",
            "while n is 0\nrepeat\nset n to n + 1\nuntil n is 1\nend",
        ];
        for source in cases {
            assert_eq!(
                depth_of(source),
                0,
                "{source:?} is a finished program, or a REPL asks for a line \
                 that closes nothing"
            );
        }
        assert_eq!(
            depth_of("repeat\nset n to n + 1"),
            1,
            "a `repeat` whose `until` has not been typed yet is still open"
        );
    }

    /// Every block form `docs/GRAMMAR.md` § 3.1 lists leaves a block open, and
    /// `end` closes it. A detector that answers for none of these would let a
    /// REPL run half of every block the user typed.
    #[test]
    fn open_block_depth_counts_every_block_form() {
        let openers = [
            "if x is greater than 0 then",
            "unless x is nothing then",
            "for each i from 1 to 10",
            "for each i in [1, 2]",
            "repeat 3 times",
            "while x is less than 10",
            "to greet(name)",
            "object Person",
            "try",
            "test \"a name\"",
            "module Math",
        ];

        for opener in openers {
            assert_eq!(depth_of(opener), 1, "{:?} opens exactly one block", opener);
            assert_eq!(
                depth_of(&format!("{opener}\nend")),
                0,
                "{:?} is closed by one 'end'",
                opener
            );
        }
    }

    /// An opener is counted only where a statement can begin, so the `to` that
    /// is a range bound and the `to` that is an import alias are not function
    /// declarations waiting for an `end`.
    #[test]
    fn open_block_depth_ignores_a_to_that_is_not_a_declaration() {
        assert_eq!(
            depth_of("for each i from 1 to 10"),
            1,
            "the range bound's 'to' is not a second block"
        );
        assert_eq!(
            depth_of("import MathUtils to M"),
            0,
            "an alias is not a function body"
        );
        assert_eq!(
            depth_of("import files, network to N"),
            0,
            "nor is one after a comma"
        );
        assert_eq!(depth_of("set x to 10"), 0);
    }

    /// Block keywords are counted from tokens, so one inside a string is text.
    /// Matching the text instead closed a block on `say "end"` and opened one on
    /// a string that happened to end in a keyword.
    #[test]
    fn open_block_depth_reads_tokens_not_text() {
        assert_eq!(depth_of(r#"say "end""#), 0, "a quoted 'end' closes nothing");
        assert_eq!(
            depth_of(r#"say "repeat 3 times""#),
            0,
            "a quoted opener opens nothing"
        );
        assert_eq!(
            depth_of("say 1\nend\nsay 2"),
            -1,
            "a stray 'end' closes nothing either"
        );
    }

    /// `else`, `catch` and `finally` are branches of a block already counted, so
    /// the block after one of them is still counted — `else if ... then` opens a
    /// second block needing a second `end`.
    #[test]
    fn open_block_depth_counts_the_block_inside_an_else_if() {
        assert_eq!(depth_of("if x then\nelse if y then"), 2);
        assert_eq!(depth_of("if x then\nelse if y then\nend"), 1);
        assert_eq!(depth_of("if x then\nelse if y then\nend\nend"), 0);
    }

    #[test]
    fn open_block_depth_counts_nested_blocks() {
        assert_eq!(
            depth_of("repeat 3 times\n  if x then\n    say \"a\"\n  end"),
            1,
            "the inner block is closed and the outer one is not"
        );
        assert_eq!(depth_of("repeat 3 times\nend\nsay \"a\"\nend"), -1);
    }

    /// A one-line block is already finished when the line ends, which is what
    /// keeps a REPL from asking for a second line it was never given.
    #[test]
    fn edge_a_block_whole_on_one_line_is_already_closed() {
        for source in [
            r#"if x then say "a" end"#,
            "to greet(name)\nsay name\nend",
            r#"repeat 2 times say "a" end"#,
        ] {
            assert_eq!(
                depth_of(source),
                0,
                "{:?} needs no continuation line",
                source
            );
        }
    }

    #[test]
    fn edge_source_with_no_block_at_all_is_zero() {
        for source in ["", "   ", "say \"hi\"", "2 + 3", "set x to 1"] {
            assert_eq!(depth_of(source), 0, "{:?} has no block", source);
        }
    }

    /// A block body is parsed by recursing back into `parse_statement`, so the
    /// parser's own budget is what stops a deeply nested block from taking the
    /// stack with it. `module` is one of those blocks — [`Parser::parse_module`]
    /// reads its body the same way `if` does — and it was missing from
    /// [`Parser::opens_block`], so nested modules spent no budget at all: a file
    /// of them recursed until the process aborted on a stack overflow rather
    /// than reporting anything.
    #[test]
    fn edge_blocks_deeper_than_the_budget_are_reported_whatever_their_form() {
        // A literal rather than `MAX_BLOCK_DEPTH + 1`, so that *raising* the
        // budget fails this test instead of quietly moving the target out of
        // reach. The budget is pinned behaviour; changing it is deliberate.
        let past_budget = 70;
        assert!(
            past_budget > MAX_BLOCK_DEPTH,
            "depth {} is no longer past MAX_BLOCK_DEPTH ({}); re-anchor this test \
             if the parser's budget changes",
            past_budget,
            MAX_BLOCK_DEPTH
        );

        for opener in [
            "if 1 is 1 then",
            "unless 1 is nothing then",
            "for each i from 1 to 10",
            "repeat 3 times",
            "while 1 is less than 2",
            "to greet(name)",
            "object Person",
            "try",
            "test \"a name\"",
            "module Math",
        ] {
            let mut source = String::new();
            for index in 0..past_budget {
                source.push_str(&opener.replace("Math", &format!("Math{index}")));
                source.push('\n');
            }
            source.push_str("say 1\n");
            for _ in 0..past_budget {
                source.push_str("end\n");
            }

            let tokens = Lexer::tokenize(&source).expect("the source should lex");
            let error = Parser::new(tokens).parse().err().unwrap_or_else(|| {
                panic!(
                    "{past_budget} nested {opener:?} blocks is past the budget of \
                         {MAX_BLOCK_DEPTH}, so it must be reported rather than parsed \
                         all the way down"
                )
            });
            assert!(
                error.to_string().contains("Blocks nest more than"),
                "{opener:?} past the budget must name the block budget, got: {error}"
            );
        }
    }

    /// The two tables that answer "does this open a block" have to agree about
    /// every form that opens one at a statement start, or one of them is a
    /// divergence waiting to happen — this is exactly how `module` ended up in
    /// one table and not the other, and a block form with no budget is a stack
    /// overflow rather than a diagnostic.
    #[test]
    fn edge_every_statement_start_opener_is_a_block_to_the_parser_too() {
        for kind in [
            TokenKind::If,
            TokenKind::Unless,
            TokenKind::For,
            TokenKind::Repeat,
            TokenKind::While,
            TokenKind::To,
            TokenKind::Object,
            TokenKind::Try,
            TokenKind::Test,
            TokenKind::Module,
        ] {
            assert!(
                opens_block_at_statement_start(&kind),
                "{kind:?} opens a block where a statement begins"
            );
            let parser = Parser::new(vec![Token::new(kind.clone(), 1, 1)]);
            assert!(
                parser.opens_block(),
                "{kind:?} opens a block to the parser as well, or its body is \
                 parsed without any budget"
            );
        }
    }

    /// The one-line function literal ends its body at the first token that
    /// cannot begin a statement, so the two tables have to agree: a form that
    /// `parse_statement` reads and `can_begin_statement` refuses ends the body
    /// early, and one that `can_begin_statement` accepts and `parse_statement`
    /// does not read past it swallows whatever follows the literal on the line.
    #[test]
    fn edge_the_one_line_literal_body_agrees_with_parse_statement_about_starts() {
        for kind in [
            TokenKind::Say,
            TokenKind::Print,
            TokenKind::Set,
            TokenKind::Constant,
            TokenKind::Module,
            TokenKind::Export,
            TokenKind::Import,
            TokenKind::If,
            TokenKind::Unless,
            TokenKind::For,
            TokenKind::Repeat,
            TokenKind::While,
            TokenKind::Break,
            TokenKind::Skip,
            TokenKind::Return,
            TokenKind::GiveBack,
            TokenKind::To,
            TokenKind::Object,
            TokenKind::Try,
            TokenKind::Test,
            TokenKind::Expect,
        ] {
            let parser = Parser::new(vec![Token::new(kind.clone(), 1, 1)]);
            assert!(
                parser.can_begin_statement(),
                "{kind:?} begins a statement, so it must not end a one-line \
                 literal body before the literal is finished"
            );
        }

        for kind in [
            TokenKind::Comma,
            TokenKind::RightParen,
            TokenKind::RightBracket,
            TokenKind::RightBrace,
            TokenKind::End,
            TokenKind::Newline,
            TokenKind::Eof,
        ] {
            let parser = Parser::new(vec![Token::new(kind.clone(), 1, 1)]);
            assert!(
                !parser.can_begin_statement(),
                "{kind:?} cannot begin a statement, so it must end a one-line \
                 literal body rather than being read as one"
            );
        }
    }

    /// A `to` in expression position is a literal, in both the one-line and the
    /// block form, and the body of either is the statements between them.
    #[test]
    fn a_literal_parses_in_both_forms_with_the_body_it_was_written_with() {
        let one_line = parse(
            Lexer::tokenize("set double to to (x) give back x * 2").expect("the source should lex"),
        )
        .expect("a one-line literal should parse");
        let block = parse(
            Lexer::tokenize("set double to to (x)\n    give back x * 2\nend")
                .expect("the source should lex"),
        )
        .expect("a block literal should parse");

        for program in [one_line, block] {
            assert_eq!(
                program.statements.len(),
                1,
                "the program is one `set`, whatever the literal's shape"
            );
        }
    }
}
