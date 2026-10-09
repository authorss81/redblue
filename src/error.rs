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
    /// One of the host's own resource limits — the step budget, the call-depth
    /// limit, or a loop's iteration cap.
    ///
    /// A variant rather than a `Runtime` whose message happens to read like one,
    /// because the question `might fail` asks of a failure — *is this the host
    /// stopping the program, or the program's own expression going wrong?* — has
    /// to be answerable from the failure itself. Matching a message prefix made
    /// the answer depend on the wording: rewording a limit message would silently
    /// turn every guard around a runaway program into one that swallows it, and
    /// any failure a program produced that began the same way would be reported as
    /// a resource error and escape the guard written for it. It renders as a
    /// `RuntimeError` and reads exactly as one — a limit is a runtime failure — but
    /// nothing has to agree on how it is spelled for it to be told apart.
    Limit(String, Span),
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
            | Error::Runtime(_, span)
            | Error::Limit(_, span) => span,
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
            // A limit is a runtime failure and is labelled as one, so a rendered
            // message and an asserted `label()` do not change when this stops
            // being spelled as a message prefix.
            Error::Runtime(_, _) | Error::Limit(_, _) => "RuntimeError",
            Error::Io(_) => "IoError",
        }
    }

    /// Whether this failure is one of the host's resource limits — the step
    /// budget, the call-depth limit, or a loop's iteration cap — rather than
    /// something the program got wrong.
    ///
    /// The three limits are the host refusing to keep going rather than a failure
    /// of the expression that was running: a program stopped at its step budget has
    /// not computed a wrong answer, it has been stopped. `might fail` is defined
    /// over a failure of its own expression and so does not take these; both VMs
    /// ask this question so the two agree, and so does the guard written around a
    /// call that hits a limit report the limit rather than yield `nothing`.
    pub fn is_resource_limit(&self) -> bool {
        matches!(self, Error::Limit(..))
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
            | Error::Limit(msg, _)
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
