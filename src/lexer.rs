use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    Number(f64),
    Text(String),
    YesNo(bool),
    Nothing,
    
    // Identifiers
    Identifier(String),
    
    // Keywords
    Set,
    To,
    Is,
    Are,
    If,
    Then,
    Else,
    End,
    When,
    Unless,
    For,
    Each,
    In,
    From,
    Times,
    While,
    Repeat,
    Until,
    Break,
    Skip,
    Return,
    GiveBack,
    MightFail,
    Say,
    Print,
    Ask,
    Try,
    Catch,
    Finally,
    And,
    Or,
    Not,
    Module,
    Import,
    Export,
    Object,
    Has,
    Can,
    This,
    That,
    New,
    Extends,
    Async,
    Wait,
    Parallel,
    Done,
    
    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Mod,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    
    // Delimiters
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Comma,
    Dot,
    Colon,
    
    // Special
    Newline,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, column: usize) -> Self {
        Self { kind, line, column }
    }
}

pub struct Lexer {
    source: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }
    
    fn current(&self) -> Option<char> {
        self.source.get(self.pos).copied()
    }
    
    fn peek(&self) -> Option<char> {
        self.source.get(self.pos + 1).copied()
    }
    
    fn advance(&mut self) -> Option<char> {
        let c = self.source.get(self.pos).copied();
        self.pos += 1;
        if c == Some('\n') {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        c
    }
    
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current() {
            if c.is_whitespace() && c != '\n' {
                self.advance();
            } else {
                break;
            }
        }
    }
    
    fn skip_comment(&mut self) {
        while let Some(c) = self.current() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }
    
    fn read_number(&mut self) -> f64 {
        let mut num_str = String::new();
        while let Some(c) = self.current() {
            if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' 
               || c == '+' || c == '-' {
                num_str.push(c);
                self.advance();
            } else {
                break;
            }
        }
        num_str.parse().unwrap_or(0.0)
    }
    
    fn read_text(&mut self) -> String {
        self.advance(); // consume opening quote
        let mut text = String::new();
        while let Some(c) = self.current() {
            if c == '"' {
                self.advance();
                break;
            }
            if c == '\\' {
                self.advance();
                match self.advance() {
                    Some('n') => text.push('\n'),
                    Some('t') => text.push('\t'),
                    Some('r') => text.push('\r'),
                    Some('\\') => text.push('\\'),
                    Some('"') => text.push('"'),
                    Some(c) => text.push(c),
                    None => break,
                }
            } else {
                text.push(c);
                self.advance();
            }
        }
        text
    }
    
    fn read_identifier(&mut self) -> String {
        let mut ident = String::new();
        while let Some(c) = self.current() {
            if c.is_alphanumeric() || c == '_' {
                ident.push(c);
                self.advance();
            } else {
                break;
            }
        }
        ident
    }
    
    fn keyword(&self, ident: &str) -> TokenKind {
        match ident {
            "set" => TokenKind::Set,
            "to" => TokenKind::To,
            "is" => TokenKind::Is,
            "are" => TokenKind::Are,
            "if" => TokenKind::If,
            "then" => TokenKind::Then,
            "else" => TokenKind::Else,
            "end" => TokenKind::End,
            "when" => TokenKind::When,
            "unless" => TokenKind::Unless,
            "for" => TokenKind::For,
            "each" => TokenKind::Each,
            "in" => TokenKind::In,
            "from" => TokenKind::From,
            "times" => TokenKind::Times,
            "while" => TokenKind::While,
            "repeat" => TokenKind::Repeat,
            "until" => TokenKind::Until,
            "break" => TokenKind::Break,
            "skip" => TokenKind::Skip,
            "return" => TokenKind::Return,
            "give" => TokenKind::GiveBack, // give back
            "back" => TokenKind::GiveBack, // give back
            "might" => TokenKind::MightFail, // might fail
            "fail" => TokenKind::MightFail, // might fail
            "say" => TokenKind::Say,
            "print" => TokenKind::Print,
            "ask" => TokenKind::Ask,
            "try" => TokenKind::Try,
            "catch" => TokenKind::Catch,
            "finally" => TokenKind::Finally,
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            "yes" => TokenKind::YesNo(true),
            "no" => TokenKind::YesNo(false),
            "nothing" => TokenKind::Nothing,
            "module" => TokenKind::Module,
            "import" => TokenKind::Import,
            "export" => TokenKind::Export,
            "object" => TokenKind::Object,
            "has" => TokenKind::Has,
            "can" => TokenKind::Can,
            "this" => TokenKind::This,
            "that" => TokenKind::That,
            "new" => TokenKind::New,
            "extends" => TokenKind::Extends,
            "async" => TokenKind::Async,
            "wait" => TokenKind::Wait,
            "parallel" => TokenKind::Parallel,
            "done" => TokenKind::Done,
            _ => TokenKind::Identifier(ident.to_string()),
        }
    }
    
    pub fn tokenize(source: &str) -> Result<Vec<Token>> {
        let mut lexer = Lexer::new(source);
        let mut tokens = Vec::new();
        
        loop {
            lexer.skip_whitespace();
            
            let line = lexer.line;
            let column = lexer.column;
            
            let Some(c) = lexer.current() else {
                tokens.push(Token::new(TokenKind::Eof, line, column));
                break;
            };
            
            // Handle newline
            if c == '\n' {
                lexer.advance();
                tokens.push(Token::new(TokenKind::Newline, line, column));
                continue;
            }
            
            // Skip comments
            if c == '/' && lexer.peek() == Some('/') {
                lexer.skip_comment();
                continue;
            }
            
            // Numbers
            if c.is_ascii_digit() || (c == '.' && lexer.peek().map(|p| p.is_ascii_digit()).unwrap_or(false)) {
                let num = lexer.read_number();
                tokens.push(Token::new(TokenKind::Number(num), line, column));
                continue;
            }
            
            // Strings
            if c == '"' {
                let text = lexer.read_text();
                tokens.push(Token::new(TokenKind::Text(text), line, column));
                continue;
            }
            
            // Identifiers and keywords
            if c.is_alphabetic() || c == '_' {
                let ident = lexer.read_identifier();
                let kind = lexer.keyword(&ident);
                tokens.push(Token::new(kind, line, column));
                continue;
            }
            
            // Operators
            let kind = match c {
                '+' => { lexer.advance(); TokenKind::Plus },
                '-' => { lexer.advance(); TokenKind::Minus },
                '*' => { lexer.advance(); TokenKind::Star },
                '/' => { lexer.advance(); TokenKind::Slash },
                '%' => { lexer.advance(); TokenKind::Mod },
                '(' => { lexer.advance(); TokenKind::LeftParen },
                ')' => { lexer.advance(); TokenKind::RightParen },
                '[' => { lexer.advance(); TokenKind::LeftBracket },
                ']' => { lexer.advance(); TokenKind::RightBracket },
                '{' => { lexer.advance(); TokenKind::LeftBrace },
                '}' => { lexer.advance(); TokenKind::RightBrace },
                ',' => { lexer.advance(); TokenKind::Comma },
                '.' => { lexer.advance(); TokenKind::Dot },
                ':' => { lexer.advance(); TokenKind::Colon },
                _ => {
                    return Err(Error::Lexer(format!(
                        "Unexpected character '{}' at line {}, column {}",
                        c, line, column
                    )));
                }
            };
            
            tokens.push(Token::new(kind, line, column));
        }
        
        Ok(tokens)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_hello_world() {
        let tokens = Lexer::tokenize(r#"say "Hello, World!""#).unwrap();
        assert_eq!(tokens.len(), 4); // say, Text, Newline, Eof
        assert_eq!(tokens[1].kind, TokenKind::Text("Hello, World!".to_string()));
    }
    
    #[test]
    fn test_numbers() {
        let tokens = Lexer::tokenize("set x to 42").unwrap();
        assert_eq!(tokens[3].kind, TokenKind::Number(42.0));
    }
    
    #[test]
    fn test_keywords() {
        let tokens = Lexer::tokenize("if yes then end").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::YesNo(true));
    }
}
