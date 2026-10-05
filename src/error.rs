use std::fmt;

/// A source position: a 1-based line and a 1-based **character** column.
///
/// Columns count characters, not bytes, so a line that starts with an emoji
/// reports the column the caret is drawn at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
}

impl Span {
    pub fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }

    /// A position that is not attached to any source text. Only for failures
    /// that happen before a token exists, such as a module file that cannot be
    /// read.
    pub fn unknown() -> Self {
        Self { line: 0, column: 0 }
    }

    pub fn is_known(&self) -> bool {
        self.line > 0
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

#[derive(Debug, Clone)]
pub enum Error {
    Lexer(String, Span),
    Parser(String, Span),
    Analyzer(String, Span),
    Runtime(String, Span),
    Io(String),
}

impl Error {
    /// The source position of this failure, or `None` for failures that have
    /// no position at all (`Io`) and for failures reported as `unknown`.
    pub fn span(&self) -> Option<&Span> {
        let span = match self {
            Error::Lexer(_, span)
            | Error::Parser(_, span)
            | Error::Analyzer(_, span)
            | Error::Runtime(_, span) => span,
            Error::Io(_) => return None,
        };

        if span.is_known() {
            Some(span)
        } else {
            None
        }
    }

    /// The name of this error's kind, as it appears in a rendered failure.
    ///
    /// Public so that a caller comparing two failures — the bytecode VM against
    /// the tree-walking one — can assert on the kind without matching the whole
    /// rendered string.
    pub fn label(&self) -> &'static str {
        match self {
            Error::Lexer(_, _) => "LexerError",
            Error::Parser(_, _) => "ParserError",
            Error::Analyzer(_, _) => "AnalyzerError",
            Error::Runtime(_, _) => "RuntimeError",
            Error::Io(_) => "IoError",
        }
    }

    /// The failure message on its own, with no position.
    ///
    /// Editor tooling carries the position in its own field, so it wants the
    /// message alone rather than the multi-line [`fmt::Display`] form, which
    /// repeats the location on a second line.
    pub fn message(&self) -> &str {
        match self {
            Error::Lexer(msg, _)
            | Error::Parser(msg, _)
            | Error::Analyzer(msg, _)
            | Error::Runtime(msg, _)
            | Error::Io(msg) => msg,
        }
    }

    fn location(&self, file: Option<&str>) -> Option<String> {
        let span = self.span()?;
        Some(match file {
            Some(name) => format!("{}:{}", name, span),
            None => span.to_string(),
        })
    }

    /// Renders the message, the source position, the offending source line and
    /// a caret under the column. Falls back to the message alone when the
    /// line is not present in `source`.
    pub fn render(&self, source: &str, file: Option<&str>) -> String {
        let mut out = format!("{}: {}", self.label(), self.message());

        let location = match self.location(file) {
            Some(location) => location,
            None => return out,
        };

        let span = self.span().expect("location implies a known span");
        let line_number = span.line.to_string();
        let gutter = " ".repeat(line_number.len());

        out.push_str(&format!("\n  --> {}", location));

        let line_text = match source.lines().nth(span.line - 1) {
            Some(text) => text,
            None => return out,
        };

        out.push('\n');
        out.push_str(&format!("{} | {}", line_number, line_text));
        out.push('\n');
        out.push_str(&format!(
            "{} | {}^",
            gutter,
            " ".repeat(span.column.saturating_sub(1))
        ));

        out
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.label(), self.message())?;

        if let Some(location) = self.location(None) {
            write!(f, "\n  --> {}", location)?;
        }

        Ok(())
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;
