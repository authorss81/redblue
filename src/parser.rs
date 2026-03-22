use crate::error::{Error, Result};
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
}

#[derive(Debug, Clone)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod,
    Equal, NotEqual,
    Less, LessEqual, Greater, GreaterEqual,
    And, Or,
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
    
    // print "Hello"
    Print(Expr),
    
    // set x to 10
    Set {
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
        then_branch: Vec<Statement>,
        else_branch: Vec<Statement>,
    },
    
    // for each x in list ... end
    ForEach {
        variable: String,
        iterable: Expr,
        body: Vec<Statement>,
    },
    
    // for each i from 1 to 10 ... end
    ForRange {
        variable: String,
        start: Expr,
        end: Expr,
        step: Option<Expr>,
        body: Vec<Statement>,
    },
    
    // repeat 10 times ... end
    Repeat {
        count: Expr,
        body: Vec<Statement>,
    },
    
    // while condition ... end
    While {
        condition: Expr,
        body: Vec<Statement>,
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
        body: Vec<Statement>,
    },
    
    // to can method(args) ... end
    Method {
        name: String,
        params: Vec<String>,
        body: Vec<Statement>,
    },
    
    // object Name ... end
    Object {
        name: String,
        extends: Option<String>,
        body: Vec<Statement>,
    },
    
    // try ... catch ... end
    Try {
        body: Vec<Statement>,
        catch_var: Option<String>,
        catch_body: Vec<Statement>,
        finally_body: Vec<Statement>,
    },
    
    // import module
    Import(String),
    
    // expression statement
    Expr(Expr),
}

#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Statement>,
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }
    
    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
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
        let token = self.advance()
            .ok_or_else(|| Error::Parser("Unexpected end of input".to_string()))?;
        
        if &token.kind != kind {
            return Err(Error::Parser(format!(
                "Expected {:?} but got {:?}",
                kind, token.kind
            )));
        }
        
        Ok(token)
    }
    
    fn skip_newlines(&mut self) {
        while let Some(Token { kind: TokenKind::Newline, .. }) = self.current() {
            self.advance();
        }
    }
    
    pub fn parse(&mut self) -> Result<Program> {
        let mut statements = Vec::new();
        
        self.skip_newlines();
        
        while self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
            if let Some(stmt) = self.parse_statement()? {
                statements.push(stmt);
            }
            self.skip_newlines();
        }
        
        Ok(Program { statements })
    }
    
    fn parse_statement(&mut self) -> Result<Option<Statement>> {
        let token = match self.current() {
            Some(t) => t.clone(),
            None => return Ok(None),
        };
        
        let stmt = match &token.kind {
            TokenKind::Say => {
                self.advance();
                let expr = self.parse_expression()?;
                Some(Statement::Say(expr))
            },
            TokenKind::Print => {
                self.advance();
                let expr = self.parse_expression()?;
                Some(Statement::Print(expr))
            },
            TokenKind::Set => self.parse_set()?,
            TokenKind::If => self.parse_if()?,
            TokenKind::For => self.parse_for()?,
            TokenKind::Repeat => self.parse_repeat()?,
            TokenKind::While => self.parse_while()?,
            TokenKind::Break => {
                self.advance();
                Some(Statement::Break)
            },
            TokenKind::Skip => {
                self.advance();
                Some(Statement::Skip)
            },
            TokenKind::Return => {
                self.advance();
                let expr = if self.is_expression_start() {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                Some(Statement::Return(expr))
            },
            TokenKind::GiveBack => {
                self.advance();
                let expr = if self.is_expression_start() {
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                Some(Statement::GiveBack(expr))
            },
            TokenKind::To => self.parse_function()?,
            TokenKind::Object => self.parse_object()?,
            TokenKind::Try => self.parse_try()?,
            TokenKind::Import => {
                self.advance();
                if let Some(Token { kind: TokenKind::Identifier(name), .. }) = self.current() {
                    self.advance();
                    Some(Statement::Import(name.clone()))
                } else {
                    return Err(Error::Parser("Expected module name".to_string()));
                }
            },
            TokenKind::Newline => {
                self.advance();
                return Ok(None);
            },
            _ => {
                let expr = self.parse_expression()?;
                Some(Statement::Expr(expr))
            },
        };
        
        Ok(stmt)
    }
    
    fn parse_set(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'set'
        
        let name = match self.current() {
            Some(Token { kind: TokenKind::Identifier(name), .. }) => {
                let n = name.clone();
                self.advance();
                n
            },
            _ => return Err(Error::Parser("Expected variable name".to_string())),
        };
        
        // Check for property access: set x.y to value
        if let Some(Token { kind: TokenKind::Dot, .. }) = self.current() {
            self.advance();
            let property = match self.current() {
                Some(Token { kind: TokenKind::Identifier(prop), .. }) => {
                    let p = prop.clone();
                    self.advance();
                    p
                },
                _ => return Err(Error::Parser("Expected property name".to_string())),
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
    
    fn parse_if(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'if'
        let condition = self.parse_expression()?;
        
        self.skip_newlines();
        self.expect(&TokenKind::Then)?;
        self.skip_newlines();
        
        let mut then_branch = Vec::new();
        while self.current().map(|t| &t.kind) != Some(&TokenKind::End) 
              && self.current().map(|t| &t.kind) != Some(&TokenKind::Else)
              && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
            if let Some(stmt) = self.parse_statement()? {
                then_branch.push(stmt);
            }
            self.skip_newlines();
        }
        
        let mut else_branch = Vec::new();
        
        if let Some(Token { kind: TokenKind::Else, .. }) = self.current() {
            self.advance();
            self.skip_newlines();
            
            while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
                  && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
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
    
    fn parse_for(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'for'
        
        match self.current() {
            Some(Token { kind: TokenKind::Each, .. }) => {
                self.advance();
                
                let variable = match self.current() {
                    Some(Token { kind: TokenKind::Identifier(name), .. }) => {
                        let n = name.clone();
                        self.advance();
                        n
                    },
                    _ => return Err(Error::Parser("Expected variable name".to_string())),
                };
                
                self.expect(&TokenKind::In)?;
                let iterable = self.parse_expression()?;
                self.skip_newlines();
                
                let mut body = Vec::new();
                while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
                      && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
                    if let Some(stmt) = self.parse_statement()? {
                        body.push(stmt);
                    }
                    self.skip_newlines();
                }
                
                self.expect(&TokenKind::End)?;
                
                Ok(Some(Statement::ForEach {
                    variable,
                    iterable,
                    body,
                }))
            },
            Some(Token { kind: TokenKind::Each, .. }) => {
                // for each i from 1 to 10
                self.advance();
                
                let variable = match self.current() {
                    Some(Token { kind: TokenKind::Identifier(name), .. }) => {
                        let n = name.clone();
                        self.advance();
                        n
                    },
                    _ => return Err(Error::Parser("Expected variable name".to_string())),
                };
                
                self.expect(&TokenKind::From)?;
                let start = self.parse_expression()?;
                self.expect(&TokenKind::To)?;
                let end = self.parse_expression()?;
                
                let step = if let Some(Token { kind: TokenKind::By, .. }) = self.current() {
                    self.advance();
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                
                self.skip_newlines();
                
                let mut body = Vec::new();
                while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
                      && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
                    if let Some(stmt) = self.parse_statement()? {
                        body.push(stmt);
                    }
                    self.skip_newlines();
                }
                
                self.expect(&TokenKind::End)?;
                
                Ok(Some(Statement::ForRange {
                    variable,
                    start,
                    end,
                    step,
                    body,
                }))
            },
            _ => Err(Error::Parser("Expected 'each' after 'for'".to_string())),
        }
    }
    
    fn parse_repeat(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'repeat'
        let count = self.parse_expression()?;
        self.expect(&TokenKind::Times)?;
        self.skip_newlines();
        
        let mut body = Vec::new();
        while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
              && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
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
              && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
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
        
        let name = match self.current() {
            Some(Token { kind: TokenKind::Identifier(name), .. }) => {
                let n = name.clone();
                self.advance();
                n
            },
            _ => return Err(Error::Parser("Expected function name".to_string())),
        };
        
        // Parse parameters
        let mut params = Vec::new();
        if let Some(Token { kind: TokenKind::LeftParen, .. }) = self.current() {
            self.advance();
            
            while let Some(Token { kind: TokenKind::Identifier(name), .. }) = self.current() {
                params.push(name.clone());
                self.advance();
                
                if let Some(Token { kind: TokenKind::Comma, .. }) = self.current() {
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
              && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
            self.skip_newlines();
        }
        
        self.expect(&TokenKind::End)?;
        
        Ok(Some(Statement::Function { name, params, body }))
    }
    
    fn parse_object(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'object'
        
        let name = match self.current() {
            Some(Token { kind: TokenKind::Identifier(name), .. }) => {
                let n = name.clone();
                self.advance();
                n
            },
            _ => return Err(Error::Parser("Expected object name".to_string())),
        };
        
        let extends = if let Some(Token { kind: TokenKind::Extends, .. }) = self.current() {
            self.advance();
            match self.current() {
                Some(Token { kind: TokenKind::Identifier(name), .. }) => {
                    let n = name.clone();
                    self.advance();
                    Some(n)
                },
                _ => return Err(Error::Parser("Expected parent object name".to_string())),
            }
        } else {
            None
        };
        
        self.skip_newlines();
        
        let mut body = Vec::new();
        while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
              && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
            self.skip_newlines();
        }
        
        self.expect(&TokenKind::End)?;
        
        Ok(Some(Statement::Object { name, extends, body }))
    }
    
    fn parse_try(&mut self) -> Result<Option<Statement>> {
        self.advance(); // consume 'try'
        self.skip_newlines();
        
        let mut body = Vec::new();
        while self.current().map(|t| &t.kind) != Some(&TokenKind::Catch)
              && self.current().map(|t| &t.kind) != Some(&TokenKind::Finally)
              && self.current().map(|t| &t.kind) != Some(&TokenKind::End)
              && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
            if let Some(stmt) = self.parse_statement()? {
                body.push(stmt);
            }
            self.skip_newlines();
        }
        
        let mut catch_var = None;
        let mut catch_body = Vec::new();
        
        if let Some(Token { kind: TokenKind::Catch, .. }) = self.current() {
            self.advance();
            if let Some(Token { kind: TokenKind::Identifier(name), .. }) = self.current() {
                catch_var = Some(name.clone());
                self.advance();
            }
            self.skip_newlines();
            
            while self.current().map(|t| &t.kind) != Some(&TokenKind::Finally)
                  && self.current().map(|t| &t.kind) != Some(&TokenKind::End)
                  && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
                if let Some(stmt) = self.parse_statement()? {
                    catch_body.push(stmt);
                }
                self.skip_newlines();
            }
        }
        
        let mut finally_body = Vec::new();
        
        if let Some(Token { kind: TokenKind::Finally, .. }) = self.current() {
            self.advance();
            self.skip_newlines();
            
            while self.current().map(|t| &t.kind) != Some(&TokenKind::End)
                  && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
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
        
        while let Some(Token { kind: TokenKind::Or, .. }) = self.current() {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_and(&mut self) -> Result<Expr> {
        let mut left = self.parse_comparison()?;
        
        while let Some(Token { kind: TokenKind::And, .. }) = self.current() {
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_comparison(&mut self) -> Result<Expr> {
        let mut left = self.parse_addition()?;
        
        loop {
            let op = match self.current() {
                Some(Token { kind: TokenKind::Is, .. }) => {
                    self.advance();
                    // Check for compound comparisons
                    if let Some(Token { kind: TokenKind::Equal, .. }) = self.current() {
                        self.advance();
                        BinaryOp::Equal
                    } else if let Some(Token { kind: TokenKind::Not, .. }) = self.current() {
                        self.advance();
                        BinaryOp::NotEqual
                    } else {
                        BinaryOp::Equal // "is" alone means equality
                    }
                },
                Some(Token { kind: TokenKind::Equal, .. }) => {
                    self.advance();
                    BinaryOp::Equal
                },
                Some(Token { kind: TokenKind::NotEqual, .. }) => {
                    self.advance();
                    BinaryOp::NotEqual
                },
                Some(Token { kind: TokenKind::Less, .. }) => {
                    self.advance();
                    if let Some(Token { kind: TokenKind::Equal, .. }) = self.current() {
                        self.advance();
                        BinaryOp::LessEqual
                    } else {
                        BinaryOp::Less
                    }
                },
                Some(Token { kind: TokenKind::Greater, .. }) => {
                    self.advance();
                    if let Some(Token { kind: TokenKind::Equal, .. }) = self.current() {
                        self.advance();
                        BinaryOp::GreaterEqual
                    } else {
                        BinaryOp::Greater
                    }
                },
                Some(Token { kind: TokenKind::LessEqual, .. }) => {
                    self.advance();
                    BinaryOp::LessEqual
                },
                Some(Token { kind: TokenKind::GreaterEqual, .. }) => {
                    self.advance();
                    BinaryOp::GreaterEqual
                },
                _ => break,
            };
            
            let right = self.parse_addition()?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        
        Ok(left)
    }
    
    fn parse_addition(&mut self) -> Result<Expr> {
        let mut left = self.parse_multiplication()?;
        
        while let Some(token) = self.current() {
            let op = match &token.kind {
                TokenKind::Plus => Some(BinaryOp::Add),
                TokenKind::Minus => Some(BinaryOp::Sub),
                _ => None,
            };
            
            if let Some(op) = op {
                self.advance();
                let right = self.parse_multiplication()?;
                left = Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        
        Ok(left)
    }
    
    fn parse_multiplication(&mut self) -> Result<Expr> {
        let mut left = self.parse_unary()?;
        
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
                left = Expr::Binary {
                    op,
                    left: Box::new(left),
                    right: Box::new(right),
                };
            } else {
                break;
            }
        }
        
        Ok(left)
    }
    
    fn parse_unary(&mut self) -> Result<Expr> {
        if let Some(token) = self.current() {
            match &token.kind {
                TokenKind::Not => {
                    self.advance();
                    let expr = self.parse_unary()?;
                    return Ok(Expr::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(expr),
                    });
                },
                TokenKind::Minus => {
                    self.advance();
                    let expr = self.parse_unary()?;
                    return Ok(Expr::Unary {
                        op: UnaryOp::Neg,
                        expr: Box::new(expr),
                    });
                },
                _ => {},
            }
        }
        
        self.parse_postfix()
    }
    
    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut expr = self.parse_primary()?;
        
        loop {
            if let Some(Token { kind: TokenKind::Dot, .. }) = self.current() {
                self.advance();
                let property = match self.current() {
                    Some(Token { kind: TokenKind::Identifier(name), .. }) => {
                        let n = name.clone();
                        self.advance();
                        n
                    },
                    _ => return Err(Error::Parser("Expected property name".to_string())),
                };
                expr = Expr::Property {
                    object: Box::new(expr),
                    property,
                };
            } else if let Some(Token { kind: TokenKind::LeftParen, .. }) = self.current() {
                self.advance();
                let mut args = Vec::new();
                
                while let Some(Token { kind: TokenKind::RightParen, .. }) = self.current() {
                    break;
                }
                
                while self.current().map(|t| &t.kind) != Some(&TokenKind::RightParen)
                      && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
                    args.push(self.parse_expression()?);
                    
                    if let Some(Token { kind: TokenKind::Comma, .. }) = self.current() {
                        self.advance();
                    }
                }
                
                self.expect(&TokenKind::RightParen)?;
                
                match *expr {
                    Expr::Variable(name) => {
                        expr = Expr::Call { name, args };
                    },
                    _ => {
                        // Method call on expression
                        return Err(Error::Parser("Expected function name".to_string()));
                    }
                }
            } else if let Some(Token { kind: TokenKind::LeftBracket, .. }) = self.current() {
                self.advance();
                let index = self.parse_expression()?;
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
        let token = self.current()
            .ok_or_else(|| Error::Parser("Unexpected end of input".to_string()))?
            .clone();
        
        match &token.kind {
            TokenKind::Number(n) => {
                self.advance();
                Ok(Expr::Number(*n))
            },
            TokenKind::Text(s) => {
                self.advance();
                Ok(Expr::Text(s.clone()))
            },
            TokenKind::YesNo(b) => {
                self.advance();
                Ok(Expr::YesNo(*b))
            },
            TokenKind::Nothing => {
                self.advance();
                Ok(Expr::Nothing)
            },
            TokenKind::Identifier(name) => {
                self.advance();
                Ok(Expr::Variable(name.clone()))
            },
            TokenKind::LeftParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(&TokenKind::RightParen)?;
                Ok(expr)
            },
            TokenKind::LeftBracket => {
                self.advance();
                let mut items = Vec::new();
                
                while self.current().map(|t| &t.kind) != Some(&TokenKind::RightBracket)
                      && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
                    items.push(self.parse_expression()?);
                    
                    if let Some(Token { kind: TokenKind::Comma, .. }) = self.current() {
                        self.advance();
                    }
                }
                
                self.expect(&TokenKind::RightBracket)?;
                Ok(Expr::List(items))
            },
            TokenKind::LeftBrace => {
                self.advance();
                let mut fields = Vec::new();
                
                while self.current().map(|t| &t.kind) != Some(&TokenKind::RightBrace)
                      && self.current().map(|t| &t.kind) != Some(&TokenKind::Eof) {
                    let key = match self.current() {
                        Some(Token { kind: TokenKind::Identifier(name), .. }) => {
                            let n = name.clone();
                            self.advance();
                            n
                        },
                        _ => return Err(Error::Parser("Expected field name".to_string())),
                    };
                    
                    self.expect(&TokenKind::Colon)?;
                    let value = self.parse_expression()?;
                    fields.push((key, value));
                    
                    if let Some(Token { kind: TokenKind::Comma, .. }) = self.current() {
                        self.advance();
                    }
                }
                
                self.expect(&TokenKind::RightBrace)?;
                Ok(Expr::Record(fields))
            },
            _ => Err(Error::Parser(format!("Unexpected token {:?}", token.kind))),
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
