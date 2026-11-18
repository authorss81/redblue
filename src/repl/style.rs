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
    /// Every role a theme has an opinion about, in the order the themes list
    /// their colours. A role added here without a colour in a theme draws plain,
    /// which is the safe direction: a missing colour is a choice, not a panic.
    pub const ALL: [Paint; 14] = [
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
    ];

    /// What this role is called in `:themes`, so a sample names what it draws.
    pub fn label(self) -> &'static str {
        match self {
            Paint::Prompt => "prompt",
            Paint::Heading => "heading",
            Paint::Result => "result",
            Paint::Error => "error",
            Paint::Dim => "dim",
            Paint::BrandRed => "brand-red",
            Paint::BrandBlue => "brand-blue",
            Paint::BrandMid => "brand-mid",
            Paint::Keyword => "keyword",
            Paint::Str => "string",
            Paint::Number => "number",
            Paint::Comment => "comment",
            Paint::Builtin => "builtin",
            Paint::Operator => "operator",
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

    /// The position of this class in a [`Theme`]'s role table.
    fn index(self) -> usize {
        match self {
            TokenClass::Keyword => 0,
            TokenClass::Str => 1,
            TokenClass::Number => 2,
            TokenClass::Comment => 3,
            TokenClass::Builtin => 4,
            TokenClass::Operator => 5,
        }
    }

    /// The role this class takes in the theme this build ships.
    pub const fn role(self) -> Paint {
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

/// The token-class half of every shipped theme: each class in its own colour.
const STANDARD_ROLES: [Option<Paint>; 6] = [
    Some(TokenClass::Keyword.role()),
    Some(TokenClass::Str.role()),
    Some(TokenClass::Number.role()),
    Some(TokenClass::Comment.role()),
    Some(TokenClass::Builtin.role()),
    Some(TokenClass::Operator.role()),
];

/// One named colour scheme for the whole REPL.
///
/// The palette is data: a theme is a name, a description and one SGR parameter
/// per [`Paint`] role. Adding a theme is adding a value to [`Theme::ALL`], not
/// editing a `match` in the painter. A role a theme leaves out draws plain,
/// which is what makes a partial theme safe rather than a compile error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    name: &'static str,
    description: &'static str,
    colors: &'static [(Paint, &'static str)],
    roles: [Option<Paint>; 6],
}

/// The default: a palette chosen for a dark terminal.
pub const DARK: Theme = Theme {
    name: "dark",
    description: "default palette for a dark terminal background",
    colors: &[
        (Paint::Prompt, "1;34"),    // bold blue
        (Paint::Heading, "1;36"),   // bold cyan
        (Paint::Result, "0;32"),    // green
        (Paint::Error, "0;31"),     // red
        (Paint::Dim, "2;37"),       // faint grey
        (Paint::BrandRed, "1;31"),  // bold red
        (Paint::BrandBlue, "1;36"), // bold cyan — deliberately NOT 1;34, which is
        // Prompt. A brand colour that matches the
        // prompt would make the wordmark look like
        // something to type.
        (Paint::BrandMid, "1;35"), // bold magenta — the red/blue midpoint
        (Paint::Keyword, "0;35"),  // magenta — the words the language reserves
        (Paint::Str, "0;32"),      // green — a value wears the value colour
        (Paint::Number, "0;36"),   // cyan — plain, where Heading is bold cyan
        (Paint::Comment, "0;90"),  // bright black — present but unreadable at a glance
        (Paint::Builtin, "0;33"),  // yellow — not Prompt's blue
        (Paint::Operator, "0;37"), // white
    ],
    roles: STANDARD_ROLES,
};

/// A palette chosen for a light terminal background: dark inks, no faint white.
pub const LIGHT: Theme = Theme {
    name: "light",
    description: "palette for a light terminal background",
    colors: &[
        (Paint::Prompt, "1;35"),   // bold magenta
        (Paint::Heading, "1;36"),  // bold cyan
        (Paint::Result, "0;32"),   // green
        (Paint::Error, "0;31"),    // red
        (Paint::Dim, "2;30"),      // faint black
        (Paint::BrandRed, "1;31"), // bold red
        (Paint::BrandBlue, "1;34"),
        (Paint::BrandMid, "1;33"),
        (Paint::Keyword, "0;35"),  // magenta
        (Paint::Str, "0;32"),      // green
        (Paint::Number, "0;36"),   // cyan
        (Paint::Comment, "0;90"),  // bright black
        (Paint::Builtin, "0;33"),  // yellow
        (Paint::Operator, "0;30"), // black
    ],
    roles: STANDARD_ROLES,
};

/// No colour at all. Every paint returns its text untouched, which is the same
/// byte-for-byte result as [`Style::forced(false)`].
pub const NONE: Theme = Theme {
    name: "none",
    description: "no colour; identical to colour off",
    colors: &[],
    roles: STANDARD_ROLES,
};

impl Theme {
    /// Every theme this build ships, in the order `:themes` lists them.
    pub const ALL: [Theme; 3] = [DARK, LIGHT, NONE];

    /// The theme this build ships by default.
    pub fn standard() -> Self {
        DARK
    }

    /// Every name `:theme` accepts, in the order `:themes` lists them.
    ///
    /// The one place the available names are read from, so the error a bad name
    /// produces cannot drift from the themes this build actually ships.
    pub fn names() -> impl Iterator<Item = &'static str> {
        Self::ALL.iter().map(|theme| theme.name)
    }

    /// Resolve a human-typed name: case-insensitive, with surrounding
    /// whitespace ignored. An unknown name lists what is available instead of
    /// silently keeping the old theme.
    pub fn resolve(input: &str) -> Result<Self, String> {
        let wanted = input.trim().to_ascii_lowercase();
        Self::ALL
            .iter()
            .copied()
            .find(|theme| theme.name == wanted)
            .ok_or_else(|| {
                let names: Vec<&str> = Self::names().collect();
                format!(
                    "Error: unknown theme '{input}'. Available themes: {}.",
                    names.join(", ")
                )
            })
    }

    /// The same theme with `class` left out, whose tokens then draw plain.
    pub fn without(mut self, class: TokenClass) -> Self {
        self.roles[class.index()] = None;
        self
    }

    /// The role `class` takes here, or `None` when this theme says nothing
    /// about it — in which case the caller leaves the text plain.
    pub fn role(&self, class: TokenClass) -> Option<Paint> {
        self.roles[class.index()]
    }

    /// This theme's name, as `:themes` prints it and `:theme` accepts it.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// One line saying what this theme is for.
    pub fn description(&self) -> &'static str {
        self.description
    }

    /// The SGR parameter for `role`, or `None` when this theme leaves it plain.
    pub fn color(&self, role: Paint) -> Option<&'static str> {
        self.colors
            .iter()
            .find(|(candidate, _)| *candidate == role)
            .map(|(_, sgr)| *sgr)
    }

    /// Whether this theme emits no escape sequences at all.
    pub fn is_colorless(&self) -> bool {
        self.colors.is_empty()
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::standard()
    }
}

/// Whether colour is on, which theme is chosen, and how to apply it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    enabled: bool,
    theme: Theme,
}

impl Style {
    /// Detect from the environment and the terminal, using the default theme.
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
            theme: Theme::standard(),
        }
    }

    /// A style that is on or off regardless of the environment.
    ///
    /// The seam the tests use, and the only way to assert the coloured path
    /// without a TTY.
    pub fn forced(enabled: bool) -> Self {
        Style {
            enabled,
            theme: Theme::standard(),
        }
    }

    /// A style with an explicit theme, which is what `:themes` uses to render
    /// every sample through the real painter rather than printing SGR codes.
    pub fn themed(enabled: bool, theme: Theme) -> Self {
        Style { enabled, theme }
    }

    /// The same on/off decision with a different theme.
    pub fn with_theme(self, theme: Theme) -> Self {
        Style {
            enabled: self.enabled,
            theme,
        }
    }

    /// Whether this style emits escape sequences.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The theme this style paints with.
    pub fn theme(&self) -> Theme {
        self.theme
    }

    /// Wrap `text` in this role's colour, or return it untouched.
    ///
    /// When disabled, or when the theme has no colour for this role, this
    /// returns `text` unchanged — the same bytes, not a copy with escapes
    /// stripped — so a plain session cannot differ from a themed one by a byte.
    pub fn paint(&self, role: Paint, text: &str) -> String {
        if !self.enabled {
            return text.to_string();
        }
        match self.theme.color(role) {
            Some(sgr) => format!("\u{1b}[{sgr}m{text}\u{1b}[0m"),
            None => text.to_string(),
        }
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
        match mode.trim().to_ascii_lowercase().as_str() {
            // An explicit value is the override: it wins even over NO_COLOR.
            "always" | "1" | "yes" | "true" => return true,
            "never" | "0" | "no" | "false" => return false,
            // "auto" and anything unrecognised fall through to the rest of the
            // detection order rather than erroring. A typo must not colour a
            // transcript unexpectedly, and it must not bypass NO_COLOR either.
            _ => {}
        }
    }
    // NO_COLOR is honoured unless REDBLUE_COLOR explicitly said "always" or
    // "never", so a user can turn colour off globally and have the REPL obey
    // while an explicit per-invocation override still wins.
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
    use super::{strip_escapes, Paint, Style, Theme, TokenClass};

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

    /// A theme name is typed by a human, so case and surrounding whitespace are
    /// not part of the identity. The resolver is the one place that decision is
    /// made, so `:theme LiGhT` and `:theme  light  ` cannot disagree.
    #[test]
    fn edge_theme_names_resolve_case_insensitively_and_ignore_whitespace() {
        for theme in Theme::ALL {
            for spoken in [
                theme.name().to_string(),
                theme.name().to_ascii_uppercase(),
                format!("  {}  ", theme.name()),
                format!("\t{}\n", theme.name()),
            ] {
                assert_eq!(
                    Theme::resolve(&spoken).expect("a shipped theme resolves"),
                    theme,
                    "{spoken:?} names {}",
                    theme.name()
                );
            }
        }
        assert_eq!(
            Theme::resolve("LiGhT").expect("mixed case resolves"),
            Theme::ALL[1],
            "the light theme is not case-sensitive"
        );
    }

    /// A name that is not a theme is a catchable error, and the error says what
    /// the user could have typed instead. Silently keeping the old palette would
    /// leave a session that asked for a theme running something else.
    #[test]
    fn edge_unknown_theme_is_an_error_naming_every_available_theme() {
        for unknown in ["", "   ", "solarized", "DARKK", "日本語", "dark\u{0}"] {
            let error = Theme::resolve(unknown).expect_err("an unknown theme is an error");
            for theme in Theme::ALL {
                assert!(
                    error.contains(theme.name()),
                    "the error for {unknown:?} names every available theme, got {error:?}"
                );
            }
        }
    }

    /// The no-colour theme's whole contract: its output is byte-for-byte what a
    /// disabled style produces, for every role and every kind of text. It is not
    /// enough that it *looks* plain — the bytes have to be identical.
    #[test]
    fn no_colour_theme_is_byte_identical_to_a_disabled_style() {
        let none = Style::themed(true, super::NONE);
        let off = Style::forced(false);
        for role in Paint::ALL {
            for text in [
                "",
                "plain text",
                "say \"héllo 日本語 ✓\"",
                "set x to 1 // comment",
            ] {
                assert_eq!(
                    none.paint(role, text).as_bytes(),
                    off.paint(role, text).as_bytes(),
                    "{role:?} with text {text:?} must be the same bytes as colour off"
                );
            }
        }
    }

    /// A theme is data, not a match arm: adding one is adding a value. Every
    /// shipped theme is named exactly once and resolves under its own name.
    #[test]
    fn every_shipped_theme_has_a_unique_name_that_resolves() {
        let mut names = Vec::new();
        for theme in Theme::ALL {
            assert!(
                !theme.name().is_empty(),
                "a theme without a name cannot be listed or selected"
            );
            assert!(
                !names.contains(&theme.name()),
                "{:?} appears in two shipped themes",
                theme.name()
            );
            names.push(theme.name());
            assert_eq!(
                Theme::resolve(theme.name()).expect("a shipped name resolves"),
                theme
            );
        }
    }

    /// Every theme this build ships is documented in both places a user reads,
    /// and every documented theme name actually resolves. A doc that names a
    /// theme the binary does not have is worse than no documentation.
    #[test]
    fn every_documented_theme_name_resolves() {
        for document in [
            include_str!("../../SPEC.md"),
            include_str!("../../README.md"),
        ] {
            for theme in Theme::ALL {
                let mention = format!(":theme {}", theme.name());
                assert!(
                    document.contains(&mention),
                    "the documentation must name {mention:?}"
                );
                assert!(
                    Theme::resolve(theme.name()).is_ok(),
                    "the documented theme {} must resolve",
                    theme.name()
                );
            }
        }
    }

    /// The names `:themes` lists are exactly the names `:theme` accepts, in a
    /// stable order, with no two themes sharing one. A duplicate would make a
    /// typed name ambiguous; a name that did not resolve would be a listing a
    /// user cannot follow.
    #[test]
    fn edge_theme_names_are_unique_and_resolve_in_listing_order() {
        let names: Vec<&'static str> = Theme::names().collect();
        assert_eq!(
            names.len(),
            Theme::ALL.len(),
            "every shipped theme contributes one name to the listing"
        );
        for (index, name) in names.iter().enumerate() {
            assert_eq!(
                *name,
                Theme::ALL[index].name(),
                "the listing reads Theme::ALL in order"
            );
            assert_eq!(
                Theme::resolve(name).expect("a listed name resolves"),
                Theme::ALL[index],
                "{name:?} selects the theme it names"
            );
        }
        for (index, name) in names.iter().enumerate() {
            for other in &names[index + 1..] {
                assert_ne!(
                    name, other,
                    "two shipped themes must not answer to the same name"
                );
            }
        }
    }

    /// A colour theme is whole: it names each [`Paint`] role exactly once. A
    /// missing role would draw plain without the theme saying so, and a repeated
    /// role would leave one of its two colours dead.
    #[test]
    fn edge_a_colour_theme_holds_one_colour_for_every_paint_role() {
        for theme in [super::DARK, super::LIGHT] {
            assert!(!theme.is_colorless(), "{} is a colour theme", theme.name());
            assert_eq!(
                theme.colors.len(),
                Paint::ALL.len(),
                "{} must have one colour per role",
                theme.name()
            );
            for role in Paint::ALL {
                assert!(
                    theme.color(role).is_some(),
                    "{} has no colour for {role:?}",
                    theme.name()
                );
                let seen = theme
                    .colors
                    .iter()
                    .filter(|(candidate, _)| *candidate == role)
                    .count();
                assert_eq!(seen, 1, "{} names {role:?} {seen} times", theme.name());
            }
        }
    }

    /// The no-colour theme keeps every token class mapped even though it has no
    /// palette: switching to it must not lose the ability to highlight source
    /// if the same style later gains colours, and it must paint every role plain.
    #[test]
    fn edge_no_colour_theme_keeps_every_token_class_but_no_palette() {
        assert!(super::NONE.is_colorless(), "none ships no SGR parameters");
        assert_eq!(super::NONE.colors.len(), 0, "none carries no palette");
        for class in TokenClass::ALL {
            assert!(
                super::NONE.role(*class).is_some(),
                "{class:?} keeps its token-class mapping under none"
            );
        }
        for role in Paint::ALL {
            assert_eq!(super::NONE.color(role), None, "none has no {role:?} colour");
            assert_eq!(
                Style::themed(true, super::NONE).paint(role, "plain"),
                "plain",
                "{role:?} under none must be the text itself"
            );
        }
    }

    /// A theme with a hole in it paints that class plain rather than borrowing
    /// another role's colour. The shipped themes fill every class; the hole is
    /// ordinary because a theme is the part a user replaces.
    #[test]
    fn edge_a_theme_may_leave_a_role_unpainted() {
        let holed = Theme::standard().without(TokenClass::Str);
        assert_eq!(holed.role(TokenClass::Str), None, "the hole stays a hole");
        assert!(
            holed.color(Paint::Str).is_some(),
            "and the role palette is still intact for the classes that remain"
        );
        let style = Style::themed(true, holed);
        assert_eq!(
            style.paint(Paint::Result, "still painted"),
            Style::themed(true, Theme::standard()).paint(Paint::Result, "still painted"),
            "a missing token class does not disturb a Paint role"
        );
    }
}
