//! Colour for the REPL.
//!
//! One rule governs everything here: **when colour is off, the output is
//! byte-identical to what it was before this module existed.** That is the
//! property the tests pin hardest, because a colour feature that silently
//! changes plain output would corrupt every golden file in the corpus and every
//! transcript a user has saved.
//!
//! Three things decide whether colour is on, in order:
//!
//! 1. `REDBLUE_COLOR` — `always` / `never` / `auto`, an explicit override.
//! 2. `NO_COLOR` — any value disables colour, the cross-platform convention.
//! 3. Whether stdout is a terminal.
//!
//! The override exists because tests cannot make stdout a TTY reliably, so
//! `REDBLUE_COLOR=always` is how a colour assertion runs at all, and
//! `REDBLUE_COLOR=never` is how a test proves the plain path is untouched.

use std::io::IsTerminal;

/// What a span of text is *for*. The theme maps this to a colour, so a call
/// site never names an escape sequence and never needs changing when the theme
/// does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    /// A prompt or other thing the user is expected to type after.
    Prompt,
    /// The banner and section headings.
    Heading,
    /// A value the program just produced. The common case.
    Result,
    /// A diagnostic: an error the user needs to see.
    Error,
    /// Supporting detail — a hint, a count, a continuation marker.
    Dim,
    /// The "red" of the wordmark, and the first half of the banner glyph.
    ///
    /// A dedicated role rather than reusing [`Paint::Error`], which happens to
    /// be red too. Colour that means "something went wrong" must never also mean
    /// "decoration", or a themed terminal shows a brand mark as an alert.
    BrandRed,
    /// The "blue" of the wordmark, and the second half of the banner glyph.
    BrandBlue,
    /// The middle of the wordmark's red-to-blue gradient.
    ///
    /// Without this the banner steps from red straight to blue and reads as two
    /// separate blocks rather than one gradient.
    BrandMid,
    /// A word the language reserves.
    Keyword,
    /// A string literal.
    Str,
    /// A numeric literal.
    Number,
    /// A `//` comment.
    Comment,
    /// A name in [`crate::stdlib::BUILTIN_NAMES`].
    Builtin,
    /// `+`, `-`, `*`, `/`, `%`, `=`, `!=`, `<`, `<=`, `>`, `>=`.
    ///
    /// An operator is a role of its own rather than more dim punctuation because
    /// operators are where a reader's eye goes next after a keyword, and the
    /// difference between `x = 1` and `x == 1` is the whole point of the line.
    Operator,
}

impl Paint {
    /// The SGR parameters for this role under the one shipped theme.
    ///
    /// Deliberately dull. This is a REPL, not a light show: the value should
    /// read first and the colour should only help a long transcript be scanned.
    fn sgr(self) -> &'static str {
        match self {
            Paint::Prompt => "1;34",    // bold blue
            Paint::Heading => "1;36",   // bold cyan
            Paint::Result => "0;32",    // green
            Paint::Error => "0;31",     // red
            Paint::Dim => "2;37",       // faint grey
            Paint::BrandRed => "1;31",  // bold red
            Paint::BrandBlue => "1;36", // bold cyan — deliberately NOT 1;34, which is
            // Prompt. A brand colour that matches the
            // prompt would make the wordmark look like
            // something to type.
            Paint::BrandMid => "1;35", // bold magenta — the red/blue midpoint
            Paint::Keyword => "0;35",  // magenta — the words the language reserves
            // Green on purpose, and the same green as Paint::Result: a string
            // literal is a value, so it wears the colour values already do.
            Paint::Str => "0;32",
            Paint::Number => "0;36", // cyan — plain, where Heading is bold cyan
            // Bright black rather than Paint::Dim's faint white: a comment is
            // present but unreadable at a glance, not merely de-emphasised.
            Paint::Comment => "0;90",
            Paint::Builtin => "0;33", // yellow — and not Paint::Prompt's blue,
            // which would make a builtin look like something to type
            Paint::Operator => "0;37", // white
        }
    }
}

/// What a run of source is, as far as a colour scheme is concerned.
///
/// The lexer knows far more than this — `give` and `back` are one token, `5`
/// and `5.0` are one token — and a theme is not interested in any of it. These
/// are the classes a theme has an opinion about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenClass {
    /// A word in [`crate::lexer::KEYWORDS`].
    Keyword,
    /// A string literal.
    Str,
    /// A numeric literal.
    Number,
    /// A `//` comment, to the end of the line.
    Comment,
    /// A name the standard library provides.
    Builtin,
    /// An operator.
    Operator,
}

impl TokenClass {
    /// Every class, so a theme can be written from the list rather than
    /// remembered. Also the guard against a new class being added here and
    /// nowhere else: [`Theme::standard`] is built from it.
    pub const ALL: &'static [TokenClass] = &[
        TokenClass::Keyword,
        TokenClass::Str,
        TokenClass::Number,
        TokenClass::Comment,
        TokenClass::Builtin,
        TokenClass::Operator,
    ];

    /// The role this class takes in the theme this build ships.
    pub fn role(self) -> Paint {
        match self {
            TokenClass::Keyword => Paint::Keyword,
            TokenClass::Str => Paint::Str,
            TokenClass::Number => Paint::Number,
            TokenClass::Comment => Paint::Comment,
            TokenClass::Builtin => Paint::Builtin,
            TokenClass::Operator => Paint::Operator,
        }
    }
}

/// Which role each token class takes.
///
/// A lookup rather than a `match`, deliberately. A theme is the one part of
/// colouring a user is expected to replace, and a theme written before
/// [`TokenClass`] grew a class should paint that class plain rather than fail
/// to compile — or, worse, take the colour of whatever a `match` fell through
/// to. The same argument makes a missing entry worth having at all: a theme
/// with a hole in it is ordinary, and the hole has to be safe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    roles: Vec<(TokenClass, Paint)>,
}

impl Theme {
    /// The theme this build ships: every class, in its own colour.
    pub fn standard() -> Self {
        Theme {
            roles: TokenClass::ALL
                .iter()
                .map(|class| (*class, class.role()))
                .collect(),
        }
    }

    /// The same theme with `class` left out, whose tokens then draw plain.
    pub fn without(mut self, class: TokenClass) -> Self {
        self.roles.retain(|(candidate, _)| *candidate != class);
        self
    }

    /// The role `class` takes here, or `None` when this theme says nothing
    /// about it — in which case the caller leaves the text plain.
    pub fn role(&self, class: TokenClass) -> Option<Paint> {
        self.roles
            .iter()
            .find(|(candidate, _)| *candidate == class)
            .map(|(_, role)| *role)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::standard()
    }
}

/// Whether colour is on, and how to apply it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    enabled: bool,
}

impl Style {
    /// Detect from the environment and the terminal.
    ///
    /// Deliberately NOT cached. Caching was tried first and had to be removed:
    /// it made detection untestable, because the first call anywhere in the
    /// process fixed the answer for every later test, and `REDBLUE_COLOR` set
    /// afterwards had no effect. It also bought nothing — this is consulted once
    /// per printed line, and the `println!` it feeds costs orders of magnitude
    /// more than the `is_terminal` syscall.
    pub fn detect() -> Self {
        Style {
            enabled: resolve_enabled(),
        }
    }

    /// A style that is on or off regardless of the environment.
    ///
    /// The seam the tests use, and the only way to assert the coloured path
    /// without a TTY.
    pub fn forced(enabled: bool) -> Self {
        Style { enabled }
    }

    /// Whether this style emits escape sequences.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Wrap `text` in this role's colour, or return it untouched.
    ///
    /// When disabled this returns `text` itself — the same `String`, not a copy
    /// with escapes stripped — so a disabled session cannot differ from a plain
    /// one by a byte.
    pub fn paint(&self, role: Paint, text: &str) -> String {
        if !self.enabled {
            return text.to_string();
        }
        format!("\u{1b}[{}m{}\u{1b}[0m", role.sgr(), text)
    }
}

/// The text `text` says, with every escape sequence taken off it.
///
/// What a document is made of. An export, a test and a `--color=never` pipe all
/// want the same thing — the text a reader reads — and each of them reaching into
/// the middle of an escape sequence is three places to get it wrong.
///
/// A sequence is `ESC [` followed by parameters and closed by a final byte in
/// `@`…`~`, which covers everything [`Style::paint`] writes and everything else
/// a terminal understands. An escape byte that opens nothing is dropped as well:
/// it is a control character, not text, and a document that carried one would
/// carry something no reader of the document can see.
pub fn strip_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        if chars.as_str().starts_with('[') {
            chars.next();
            for parameter in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&parameter) {
                    break;
                }
            }
        }
    }
    out
}

/// The detection order, isolated so it can be tested directly.
fn resolve_enabled() -> bool {
    if let Ok(mode) = std::env::var("REDBLUE_COLOR") {
        return match mode.trim().to_ascii_lowercase().as_str() {
            "always" | "1" | "yes" | "true" => true,
            "never" | "0" | "no" | "false" => false,
            // "auto" and anything unrecognised fall through to detection rather
            // than erroring. A typo must not colour a transcript unexpectedly.
            _ => detect_terminal(),
        };
    }
    // NO_COLOR is honoured whatever REDBLUE_COLOR says, so a user can turn
    // colour off globally and have the REPL obey even if something exported
    // REDBLUE_COLOR=always earlier.
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    detect_terminal()
}

fn detect_terminal() -> bool {
    std::io::stdout().is_terminal()
}

impl Default for Style {
    fn default() -> Self {
        Self::detect()
    }
}

#[cfg(test)]
mod tests {
    use super::{strip_escapes, Paint, Style};

    /// The paint a style writes, and the text it says underneath it.
    fn painted(role: Paint, text: &str) -> String {
        Style::forced(true).paint(role, text)
    }

    /// Every role this crate can paint is round-tripped: stripping what
    /// [`Style::paint`] added gives back the text that was painted, byte for byte.
    #[test]
    fn stripping_a_painted_string_gives_the_text_back() {
        for role in [
            Paint::Prompt,
            Paint::Heading,
            Paint::Result,
            Paint::Error,
            Paint::Dim,
            Paint::BrandRed,
            Paint::BrandBlue,
            Paint::BrandMid,
            Paint::Keyword,
            Paint::Str,
            Paint::Number,
            Paint::Comment,
            Paint::Builtin,
            Paint::Operator,
        ] {
            assert_eq!(
                strip_escapes(&painted(role, "hello")),
                "hello",
                "{role:?} is paint around the text, and the stripper takes it off"
            );
        }
    }

    /// Text with no escape in it comes back as itself, and a disabled style paints
    /// nothing — so a plain session and a coloured one cannot export differently
    /// because of what this function did.
    #[test]
    fn edge_stripping_a_plain_string_changes_nothing() {
        let plain = "say \"hi\" = 1\n  indented\tand tabbed";
        assert_eq!(strip_escapes(plain), plain);
        assert_eq!(strip_escapes(""), "");
        assert_eq!(Style::forced(false).paint(Paint::Result, "x"), "x");
    }

    /// An escape byte is a control character, not text. A document is read in a
    /// text editor, a diff and a code review, and a stray `ESC` in one is
    /// invisible in all three — so it is dropped, and only the text around it is
    /// kept. The parameter form ends at its final byte, so `[2J` is one sequence
    /// rather than a `[` and an orphan `J`.
    #[test]
    fn edge_an_escape_that_is_not_a_paint_sequence_leaves_only_text() {
        assert_eq!(
            strip_escapes("before\u{1b}after"),
            "beforeafter",
            "an escape that opens nothing is a control character, and dropping it \
             keeps the text on both sides of it"
        );
        assert_eq!(
            strip_escapes("\u{1b}[2J\u{1b}[H"),
            "",
            "a clear-screen and a cursor-home are terminal control, not text a \
             document should carry"
        );
        assert_eq!(
            strip_escapes("```redblue"),
            "```redblue",
            "and the backticks of a fence are text, so they stay"
        );
    }
}
