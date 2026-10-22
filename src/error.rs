use std::fmt;

/// The most terminal cells of one drawn source line.
///
/// A drawn line is a line the reader can read, not the whole of an input file:
/// `rb run` renders whatever `fs::read_to_string` handed it
/// (`src/lib.rs`), so a file with a megabyte-long line would otherwise be drawn
/// a megabyte at a time. Past the limit the line is cut and marked with `…`, so
/// a diagnostic is bounded by this constant rather than by the program's input.
const MAX_DRAWN_CELLS: usize = 1024;

/// The most source lines one drawn range covers.
///
/// The counterpart to [`MAX_DRAWN_CELLS`] along the other axis: an unterminated
/// string runs to end of input, so without a limit its range is as long as the
/// file is.
const MAX_DRAWN_LINES: usize = 10;

/// The marker left where a drawn line or range was cut short.
const ELLIPSIS: char = '\u{2026}';

/// A source position: a 1-based line and a 1-based **character** column.
///
/// Columns count characters, not bytes, so a line that starts with an emoji
/// reports the column the caret is drawn at — and [`display_width`] is what
/// turns that character column into the cell the caret is *drawn* at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    /// Where the offending thing stops, when it covers more than one line.
    ///
    /// A span that covers one line records no end, so a span and its position
    /// are the same value and everything that compared the two keeps meaning
    /// what it meant. Only a range — an unterminated string, which runs to the
    /// end of the file — records one, because only a range has to be *drawn*.
    pub end_line: usize,
    pub end_column: usize,
}

impl Span {
    pub fn new(line: usize, column: usize) -> Self {
        Self {
            line,
            column,
            end_line: line,
            end_column: column,
        }
    }

    /// This span, extended to cover up to `end_line:end_column`.
    ///
    /// An end that is not past the start is dropped: a span that covers one line
    /// is drawn as one line, with the same caret it has always been drawn with.
    pub fn extended_to(self, end_line: usize, end_column: usize) -> Self {
        if end_line <= self.line {
            return self;
        }

        Self {
            line: self.line,
            column: self.column,
            end_line,
            end_column,
        }
    }

    /// Whether this span covers more than one line, and so has to be rendered as
    /// several.
    pub fn is_multiline(&self) -> bool {
        self.end_line > self.line
    }

    /// A position that is not attached to any source text. Only for failures
    /// that happen before a token exists, such as a module file that cannot be
    /// read.
    pub fn unknown() -> Self {
        Self {
            line: 0,
            column: 0,
            end_line: 0,
            end_column: 0,
        }
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
    /// a caret under the column. A span covering more than one line renders
    /// every line it covers. Falls back to the message alone when the line is
    /// not present in `source`.
    pub fn render(&self, source: &str, file: Option<&str>) -> String {
        let mut out = format!("{}: {}", self.label(), self.message());

        let location = match self.location(file) {
            Some(location) => location,
            None => return out,
        };

        let span = self.span().expect("location implies a known span");

        out.push_str(&format!("\n  --> {}", location));

        let lines: Vec<&str> = source.lines().collect();
        if span.line == 0 || span.line > lines.len() {
            return out;
        }

        if span.is_multiline() {
            render_range(&mut out, span, &lines);
        } else {
            render_single(&mut out, span, lines[span.line - 1]);
        }

        out
    }
}

/// Characters that take no cell of their own: combining marks, joiners,
/// variation selectors, emoji skin tones, and control characters.
///
/// They are written into the line and occupy nothing on screen, so a caret past
/// one of them must not step over a column it did not draw.
const ZERO_WIDTH: &[(u32, u32)] = &[
    (0x0300, 0x036F),
    (0x0483, 0x0489),
    (0x0591, 0x05BD),
    (0x0610, 0x061A),
    (0x064B, 0x065F),
    (0x0670, 0x0670),
    (0x06D6, 0x06DC),
    (0x0730, 0x074A),
    (0x07A6, 0x07B0),
    (0x0900, 0x0903),
    (0x093A, 0x094F),
    (0x0951, 0x0957),
    (0x1AB0, 0x1AFF),
    (0x1DC0, 0x1DFF),
    (0x200B, 0x200F),
    (0x2028, 0x202E),
    (0x2060, 0x2064),
    (0x20D0, 0x20F0),
    (0xFE00, 0xFE0F),
    (0xFE20, 0xFE2F),
    (0x1F3FB, 0x1F3FF),
    (0xE0100, 0xE01EF),
];

/// Characters a terminal draws two cells wide: East Asian Wide and Fullwidth,
/// and the emoji blocks.
///
/// Written as ranges rather than a dependency: this is the only place the
/// renderer needs to know how wide a character is, and the ranges it needs are
/// the ones a reader's line can actually contain.
const WIDE: &[(u32, u32)] = &[
    (0x1100, 0x115F),
    (0x2E80, 0x303E),
    (0x3041, 0x33FF),
    (0x3400, 0x4DBF),
    (0x4E00, 0x9FFF),
    (0xA000, 0xA4CF),
    (0xA960, 0xA97F),
    (0xAC00, 0xD7A3),
    (0xF900, 0xFAFF),
    (0xFE10, 0xFE19),
    (0xFE30, 0xFE6F),
    (0xFF00, 0xFF60),
    (0xFFE0, 0xFFE6),
    (0x1F300, 0x1F64F),
    (0x1F680, 0x1F6FF),
    (0x1F900, 0x1F9FF),
    (0x20000, 0x2FFFD),
    (0x30000, 0x3FFFD),
];

/// Whether `code` falls in one of the inclusive ranges of `table`.
fn in_ranges(code: u32, table: &[(u32, u32)]) -> bool {
    table
        .iter()
        .any(|(first, last)| code >= *first && code <= *last)
}

/// How many terminal cells `c` is drawn in.
///
/// A tab is one cell: where a tab stops is the terminal's tab stop, which a
/// renderer cannot know, and one is the honest answer rather than a guess at
/// eight.
fn char_cells(c: char) -> usize {
    // Before the control check: a tab is a control character, and answering 0
    // for it would step the caret over a column that is on the line.
    if c == '\t' {
        return 1;
    }
    let code = c as u32;
    if c.is_control() || in_ranges(code, ZERO_WIDTH) {
        0
    } else if in_ranges(code, WIDE) {
        2
    } else {
        1
    }
}

/// The terminal cells the first `chars` characters of `line` are drawn in.
///
/// This is the distance from the left of the drawn line to the column a span
/// names, which is what a caret has to step over to be *under* that column.
/// Counting characters instead — which is what a [`Span`] records — puts the
/// caret one cell left of the thing it points at for every wide character
/// before it, and a reader following a line of mixed CJK and ASCII sees a caret
/// that lands in the wrong place.
///
/// Only the prefix is measured, and only up to the drawn limit: the whole
/// point is that a caret is bounded by what is on screen.
fn display_width(line: &str, chars: usize) -> usize {
    line.chars().take(chars).map(char_cells).sum()
}

/// How many terminal cells `text` is drawn in.
///
/// The width of a text as a terminal draws it: a wide character is two cells, a
/// combining mark is none. Exposed for the REPL's table renderer, whose
/// columns are only aligned if the padding counts the same cells the terminal
/// draws — a table padded by character count is a table whose CJK columns do
/// not line up.
pub(crate) fn text_cells(text: &str) -> usize {
    text.chars().map(char_cells).sum()
}

/// One source line as it is drawn, and the cells that drawing took.
///
/// At most [`MAX_DRAWN_CELLS`] cells of the line, with [`ELLIPSIS`] in place of
/// what was left out. The cell count is the width of the text *kept*, so a
/// caret clamped to it lands where the cut was rather than under the marker.
fn drawn_line(line: &str) -> (String, usize) {
    let mut cells = 0usize;
    let mut cut = false;
    let mut drawn = String::new();

    for c in line.chars() {
        let width = char_cells(c);
        if cells + width > MAX_DRAWN_CELLS {
            cut = true;
            break;
        }
        drawn.push(c);
        cells += width;
    }

    if cut {
        drawn.push(ELLIPSIS);
    }
    (drawn, cells)
}

/// Draws one line of source and a caret under the offending column.
fn render_single(out: &mut String, span: &Span, line: &str) {
    let line_number = span.line.to_string();
    let gutter = " ".repeat(line_number.len());
    let (drawn, cells) = drawn_line(line);

    // Clamped to what is drawn, so the spaces the caret is indented by are
    // bounded by [`MAX_DRAWN_CELLS`] however large a column the failure carries,
    // and so a failure past the cut lands where the text that is not drawn
    // begins.
    let offset = display_width(line, span.column.saturating_sub(1)).min(cells);

    out.push('\n');
    out.push_str(&format!("{} | {}", line_number, drawn));
    out.push('\n');
    out.push_str(&format!("{} | {}^", gutter, " ".repeat(offset)));
}

/// Draws every line a range covers, underlined under the column it opens at.
///
/// The range is clamped to the lines the source actually has — a span that runs
/// to end of input ends one line past the last line of the file, and the lines
/// after that are not there to be printed — and then to [`MAX_DRAWN_LINES`],
/// which is what stops a string left open at the top of a long file from
/// drawing the whole file.
fn render_range(out: &mut String, span: &Span, lines: &[&str]) {
    let first = span.line;
    let last = span.end_line.min(lines.len());
    let width = last.to_string().len();
    let shown = (last - first + 1).min(MAX_DRAWN_LINES);

    for number in first..first + shown {
        let (drawn, _) = drawn_line(lines[number - 1]);
        out.push('\n');
        out.push_str(&format!("{:>width$} | {}", number, drawn, width = width));
    }

    if shown < last - first + 1 {
        out.push('\n');
        out.push_str(&format!(
            "{:>width$} | {ELLIPSIS} {} more line(s) not drawn",
            "",
            last - first + 1 - shown,
            width = width
        ));
    }

    // The underline runs from where the range opens to the end of the line it
    // opens on, in cells rather than in characters, so it covers what is drawn.
    // Where it closes is the next line's business: the lines between are shown,
    // and underlining each of them would underline the source.
    let opening = lines[first - 1];
    let (_, cells) = drawn_line(opening);
    let start = display_width(opening, span.column.saturating_sub(1)).min(cells);

    out.push('\n');
    out.push_str(&format!(
        "{} | {}^{}",
        " ".repeat(width),
        " ".repeat(start),
        "~".repeat(cells - start)
    ));
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
