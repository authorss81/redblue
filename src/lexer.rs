use std::sync::OnceLock;

use crate::error::{Error, Result, Span};

/// The byte-order mark some editors write at the start of a UTF-8 file.
const BOM: char = '\u{FEFF}';

/// Every word the lexer treats as a keyword, paired with the token it produces.
///
/// This table is the single source of truth for the keyword set: the lexer
/// matches through it (`Lexer::keyword`) and editor tooling highlights it
/// (`crate::lsp::keyword_pattern`). Keeping one table is what stops the shipped
/// TextMate grammar from drifting away from the language.
///
/// A word belongs here only while the parser has an arm that reads it. A word
/// the parser cannot read is worse than a missing feature: it cannot be used as
/// an identifier either, and the rejection names a `TokenKind` the writer has
/// never seen. Nine words (`are`, `ask`, `async`, `done`, `new`, `parallel`,
/// `that`, `wait`, `when`) sat here with no arm anywhere in `src/parser.rs` and
/// were removed for that reason. `tests/keyword_reservation_test.rs` reads the
/// table and the parser together, so adding a word here without giving the
/// parser a rule for it fails the suite.
pub const KEYWORDS: &[(&str, TokenKind)] = &[
    ("set", TokenKind::Set),
    ("constant", TokenKind::Constant),
    ("to", TokenKind::To),
    ("is", TokenKind::Is),
    ("if", TokenKind::If),
    ("then", TokenKind::Then),
    ("else", TokenKind::Else),
    ("end", TokenKind::End),
    ("unless", TokenKind::Unless),
    ("for", TokenKind::For),
    ("each", TokenKind::Each),
    ("in", TokenKind::In),
    ("from", TokenKind::From),
    ("times", TokenKind::Times),
    ("while", TokenKind::While),
    ("repeat", TokenKind::Repeat),
    ("until", TokenKind::Until),
    ("break", TokenKind::Break),
    ("skip", TokenKind::Skip),
    ("return", TokenKind::Return),
    ("give", TokenKind::GiveBack),   // give back
    ("back", TokenKind::GiveBack),   // give back
    ("might", TokenKind::MightFail), // might fail
    ("fail", TokenKind::MightFail),  // might fail
    ("say", TokenKind::Say),
    ("print", TokenKind::Print),
    ("try", TokenKind::Try),
    ("catch", TokenKind::Catch),
    ("finally", TokenKind::Finally),
    ("and", TokenKind::And),
    ("or", TokenKind::Or),
    ("not", TokenKind::Not),
    ("mod", TokenKind::Mod),
    ("yes", TokenKind::YesNo(true)),
    ("no", TokenKind::YesNo(false)),
    ("nothing", TokenKind::Nothing),
    ("module", TokenKind::Module),
    ("import", TokenKind::Import),
    ("export", TokenKind::Export),
    ("as", TokenKind::As),
    ("test", TokenKind::Test),
    ("expect", TokenKind::Expect),
    ("object", TokenKind::Object),
    ("has", TokenKind::Has),
    ("can", TokenKind::Can),
    ("this", TokenKind::This),
    ("extends", TokenKind::Extends),
];

/// The name the last value is read under, and the one identifier a sigil may
/// begin.
///
/// `$_` and not `_`: the REPL already binds `_` to each line's result, and a
/// program cannot read it — the name is not in the VM's environment, so `say _`
/// is refused as unknown however long the session has been running. `$_` is
/// spelled the way a shell spells the same idea, so it cannot be mistaken for a
/// variable the user chose, and the lexer gives it that spelling and nothing
/// else: `$name` is still the `$` it always was.
pub const LAST_VALUE_NAME: &str = "$_";

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
    Constant,
    To,
    Is,
    If,
    Then,
    Else,
    End,
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
    Try,
    Catch,
    Finally,
    And,
    Or,
    Not,
    Module,
    Import,
    Export,
    As,
    Test,
    Expect,
    Object,
    Has,
    Can,
    This,
    Extends,

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
    /// The word in the [`KEYWORDS`] table this token was lexed from, or `None`
    /// for a token that came from anywhere else.
    ///
    /// A kind on its own does not say which word the writer typed: `give` and
    /// `back` are both [`TokenKind::GiveBack`], `might` and `fail` both
    /// [`TokenKind::MightFail`], and `mod` and `%` both [`TokenKind::Mod`]. A
    /// diagnostic that named the kind would name `give` for a line that says
    /// `back`. The lexer is the only place that still knows, so it records the
    /// word here and a rejection names what was written.
    pub word: Option<&'static str>,
}

impl Token {
    pub fn new(kind: TokenKind, line: usize, column: usize) -> Self {
        Self {
            kind,
            line,
            column,
            word: None,
        }
    }

    /// A token the lexer read from a word in the [`KEYWORDS`] table.
    pub fn keyword(word: &'static str, kind: TokenKind, line: usize, column: usize) -> Self {
        Self {
            kind,
            line,
            column,
            word: Some(word),
        }
    }

    /// The same token at another position, keeping the word it was lexed from.
    ///
    /// Re-spanning a token must not cost it the word: a token that moved is
    /// still the word the writer typed, and a diagnostic about it is still
    /// about them.
    pub fn at(&self, line: usize, column: usize) -> Self {
        Self {
            kind: self.kind.clone(),
            line,
            column,
            word: self.word,
        }
    }

    /// The source position this token was lexed from.
    pub fn span(&self) -> Span {
        Span::new(self.line, self.column)
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
            // A byte-order mark is a file-formatting artefact, not a source
            // character: some editors write it at the start of the file and
            // some on every line. Left in place it would be read as a stray
            // character, or glued onto the identifier after it.
            let insignificant = c == BOM || (c.is_whitespace() && c != '\n' && c != '\r');
            if insignificant {
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

    /// Reads one numeric literal, or fails if the characters are not one.
    ///
    /// `span` is the position of the literal's first character. A literal is
    /// digits, at most one `.`, then at most one exponent: an `e`, an optional
    /// sign, and digits. A `+` or `-` anywhere else is the operator that follows
    /// the number, not part of it — `5-2` is five minus two. Text outside that
    /// shape (`1.2.3`, `2..3`, `3e`) is a mistake in the source and used to be
    /// read as the number 0 in silence.
    fn read_number(&mut self, span: Span) -> Result<f64> {
        let mut text = String::new();
        let mut has_exponent = false;
        let mut after_exponent = false;
        while let Some(c) = self.current() {
            if c.is_ascii_digit() || c == '.' {
                text.push(c);
                after_exponent = false;
            } else if (c == 'e' || c == 'E') && !has_exponent {
                has_exponent = true;
                after_exponent = true;
                text.push(c);
            } else if after_exponent && (c == '+' || c == '-') {
                after_exponent = false;
                text.push(c);
            } else {
                break;
            }
            self.advance();
        }
        text.parse()
            .map_err(|_| Error::Lexer(format!("Invalid number '{}'", text), span))
    }

    fn read_text(&mut self, span: Span) -> Result<String> {
        self.advance(); // consume opening quote
        let mut text = String::new();
        loop {
            let Some(c) = self.current() else {
                return Err(self.unterminated(span));
            };
            if c == '"' {
                self.advance();
                return Ok(text);
            }
            if c == '\\' {
                let escape = Span::new(self.line, self.column);
                self.advance();
                match self.advance() {
                    Some('n') => text.push('\n'),
                    Some('t') => text.push('\t'),
                    Some('r') => text.push('\r'),
                    Some('\\') => text.push('\\'),
                    Some('"') => text.push('"'),
                    Some('u') => text.push(self.read_unicode_escape(escape)?),
                    Some(other) => {
                        return Err(Error::Lexer(
                            format!("Unknown escape '\\{other}' in text"),
                            escape,
                        ))
                    }
                    None => return Err(self.unterminated(span)),
                }
            } else {
                text.push(c);
                self.advance();
            }
        }
    }

    /// The character named by the four hex digits of a `\u` escape.
    ///
    /// The escape names one code point and nothing else. `docs/GRAMMAR.md:80`
    /// says four `hex_digit`s, and that is exactly what it takes: a shorter run
    /// is a typo, not a number to be padded out to four digits. Everything the
    /// grammar does not spell out is refused rather than guessed at — the
    /// catch-all that used to sit in `read_text` pushed only the first character
    /// after the backslash, so `"\u00e9"` read as `u00e9` and a program written
    /// in a language's own escape printed text nobody had written.
    ///
    /// `span` is the backslash, so the refusal points at the escape rather than
    /// at the character four positions past it.
    fn read_unicode_escape(&mut self, span: Span) -> Result<char> {
        let mut value: u32 = 0;
        for _ in 0..4 {
            let Some(digit) = self.current().and_then(|c| c.to_digit(16)) else {
                return Err(Error::Lexer(
                    "Unicode escape needs four hex digits, such as \"\\u00e9\"".to_string(),
                    span,
                ));
            };
            value = value * 16 + digit;
            self.advance();
        }

        // A half of a surrogate pair — `char::from_u32` refuses it because it
        // names no character, and a lone surrogate has no UTF-8 encoding at all.
        // Substituting the replacement character here would be the same silent
        // rewrite this escape used to be, one level down.
        char::from_u32(value).ok_or_else(|| {
            Error::Lexer(
                format!("Unicode escape \\u{value:04X} names no character of its own"),
                span,
            )
        })
    }

    /// The failure for a string that ran to the end of input without closing.
    ///
    /// It spans from the opening quote to wherever the reading stopped, which for
    /// a string broken across lines is several lines and not one: the reader has
    /// to see the whole runaway string, not only the quote that opened it.
    fn unterminated(&self, span: Span) -> Error {
        let span = span.extended_to(self.line, self.column);
        Error::Lexer("Unterminated string".to_string(), span)
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

    /// The keyword `ident` is, and the word it was spelled with.
    ///
    /// The word comes back with the kind because the kind does not always carry
    /// it: `give` and `back` are one kind, so the table is the only place that
    /// knows which of the two was typed.
    fn keyword(&self, ident: &str) -> Option<(&'static str, TokenKind)> {
        KEYWORDS
            .iter()
            .find(|(word, _)| *word == ident)
            .map(|(word, kind)| (*word, kind.clone()))
    }

    /// The words this lexer recognises as keywords, sorted and deduplicated,
    /// for editor tooling.
    pub fn keywords() -> &'static [&'static str] {
        static WORDS: OnceLock<Vec<&'static str>> = OnceLock::new();

        WORDS.get_or_init(|| {
            let mut words: Vec<&'static str> = KEYWORDS.iter().map(|(word, _)| *word).collect();
            words.sort_unstable();
            words.dedup();
            words
        })
    }

    /// The word the keyword table spells a kind with, when one word does and
    /// the lexer builds that kind from nothing else.
    ///
    /// Four kinds have no single word to answer with. `Mod` and `Not` are
    /// produced by a symbol as well as by a word — `5 % 2` and `5 mod 2` are
    /// the same token, as are `!` and `not` — and `GiveBack` and `MightFail`
    /// are spelled by two words each (`give`/`back`, `might`/`fail`). For those
    /// this is `None`, because a kind cannot say which spelling the writer
    /// reached for: naming `give` for a line that says `back` is worse than
    /// naming nothing.
    ///
    /// A caller holding the token itself does not need this. [`Token::word`]
    /// records what the lexer actually read, and it is right for every kind
    /// including these four.
    pub fn word_for(kind: &TokenKind) -> Option<&'static str> {
        let mut found: Option<&'static str> = None;

        for (word, keyword_kind) in KEYWORDS {
            if keyword_kind != kind {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some(word);
        }

        // `Mod` and `Not` reach here with one table entry each, and the symbol
        // that makes them ambiguous is not in the table — so the table alone
        // cannot report that they have a second spelling.
        if matches!(kind, TokenKind::Mod | TokenKind::Not) {
            return None;
        }

        found
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

            // Handle newline, and the CR of a CRLF pair. A lone CR is how a
            // classic Mac file breaks lines; dropping it would glue two
            // statements together with no line break between them.
            if c == '\n' || c == '\r' {
                let carriage_return = c == '\r';
                lexer.advance();
                if carriage_return && lexer.current() == Some('\n') {
                    lexer.advance();
                }
                tokens.push(Token::new(TokenKind::Newline, line, column));
                continue;
            }

            // Skip comments
            if c == '/' && lexer.peek() == Some('/') {
                lexer.skip_comment();
                continue;
            }

            // Numbers
            if c.is_ascii_digit()
                || (c == '.' && lexer.peek().map(|p| p.is_ascii_digit()).unwrap_or(false))
            {
                let num = lexer.read_number(Span::new(line, column))?;
                tokens.push(Token::new(TokenKind::Number(num), line, column));
                continue;
            }

            // Strings
            if c == '"' {
                let text = lexer.read_text(Span::new(line, column))?;
                tokens.push(Token::new(TokenKind::Text(text), line, column));
                continue;
            }

            // The last value's name. Read before the operators, which is where
            // a `$` used to land and be refused as a stray character, and
            // matched exactly: only `$_` is a name, so `$foo` is still the error
            // it has always been rather than a variable spelled with a sigil.
            if c == '$' && lexer.peek() == Some('_') {
                lexer.advance();
                lexer.advance();
                tokens.push(Token::new(
                    TokenKind::Identifier(LAST_VALUE_NAME.to_string()),
                    line,
                    column,
                ));
                continue;
            }

            // Identifiers and keywords
            if c.is_alphabetic() || c == '_' {
                let ident = lexer.read_identifier();
                let token = match lexer.keyword(&ident) {
                    Some((word, kind)) => Token::keyword(word, kind, line, column),
                    None => Token::new(TokenKind::Identifier(ident), line, column),
                };
                tokens.push(token);
                continue;
            }

            // Operators
            let kind = match c {
                '+' => {
                    lexer.advance();
                    TokenKind::Plus
                }
                '-' => {
                    lexer.advance();
                    TokenKind::Minus
                }
                '*' => {
                    lexer.advance();
                    TokenKind::Star
                }
                '/' => {
                    lexer.advance();
                    TokenKind::Slash
                }
                '%' => {
                    lexer.advance();
                    TokenKind::Mod
                }
                '(' => {
                    lexer.advance();
                    TokenKind::LeftParen
                }
                ')' => {
                    lexer.advance();
                    TokenKind::RightParen
                }
                '[' => {
                    lexer.advance();
                    TokenKind::LeftBracket
                }
                ']' => {
                    lexer.advance();
                    TokenKind::RightBracket
                }
                '{' => {
                    lexer.advance();
                    TokenKind::LeftBrace
                }
                '}' => {
                    lexer.advance();
                    TokenKind::RightBrace
                }
                ',' => {
                    lexer.advance();
                    TokenKind::Comma
                }
                '.' => {
                    lexer.advance();
                    TokenKind::Dot
                }
                ':' => {
                    lexer.advance();
                    TokenKind::Colon
                }
                // The comparison symbols. `<=` and `>=` are two characters but
                // one token: the compound is consumed here so the parser sees
                // a single `LessEqual`/`GreaterEqual` rather than a `<` that a
                // separate token would then have to glue to the `=`. The
                // parser also reads a bare `Less`/`Greater` followed by
                // `Equal` as the compound, so both paths agree on what `x <= y`
                // means. `=` and `==` are the same token, and `!=` is likewise
                // read as `Not` followed by `Equal` by the parser, so `!` alone
                // stays a prefix rather than becoming a second spelling of `!=`.
                '<' => {
                    lexer.advance();
                    if lexer.current() == Some('=') {
                        lexer.advance();
                        TokenKind::LessEqual
                    } else {
                        TokenKind::Less
                    }
                }
                '>' => {
                    lexer.advance();
                    if lexer.current() == Some('=') {
                        lexer.advance();
                        TokenKind::GreaterEqual
                    } else {
                        TokenKind::Greater
                    }
                }
                '=' => {
                    lexer.advance();
                    if lexer.current() == Some('=') {
                        lexer.advance();
                    }
                    TokenKind::Equal
                }
                '!' => {
                    lexer.advance();
                    if lexer.current() == Some('=') {
                        lexer.advance();
                        TokenKind::NotEqual
                    } else {
                        TokenKind::Not
                    }
                }
                _ => {
                    // `escape_debug` keeps a NUL or a BEL readable in a
                    // terminal, where the raw character would print as
                    // nothing at all, and leaves printable ones as they are.
                    return Err(Error::Lexer(
                        format!("Unexpected character '{}'", c.escape_debug()),
                        Span::new(line, column),
                    ));
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
        assert_eq!(tokens.len(), 3); // say, Text, Eof
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
