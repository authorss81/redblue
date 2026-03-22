use std::fmt;

#[derive(Debug, Clone)]
pub enum Error {
    Lexer(String),
    Parser(String),
    Analyzer(String),
    Runtime(String),
    Io(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Lexer(msg) => write!(f, "LexerError: {}", msg),
            Error::Parser(msg) => write!(f, "ParserError: {}", msg),
            Error::Analyzer(msg) => write!(f, "AnalyzerError: {}", msg),
            Error::Runtime(msg) => write!(f, "RuntimeError: {}", msg),
            Error::Io(msg) => write!(f, "IoError: {}", msg),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
