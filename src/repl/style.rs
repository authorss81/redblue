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
//!
//! *Which* colour is a fourth question, and one a user may answer: a
//! [`Palette`] is one SGR per role, `:themes` lists the shipped ones and
//! `:theme <name>` picks one, immediately and for the rest of the session.

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
    /// Every role, so a palette can be written and checked from the list rather
    /// than remembered.
    pub const ALL: &'static [Paint] = &[
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

/// One named palette: the SGR each [`Paint`] role wears.
///
/// A theme is a palette and a palette is data — a table of `(role, SGR)` pairs —
/// so shipping another theme is adding a value here rather than editing a
/// `match`. The table is keyed by role rather than positional, so a [`Paint`]
/// added later cannot silently take the colour meant for the role above it, and a
/// palette that says nothing about a role paints it plain: `none` is exactly
/// such a palette, and being plain *because* the table is empty is what makes the
/// byte-identical guarantee something a test can check instead of something the
/// docs claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// What `:theme` calls it and what `:themes` prints.
    name: &'static str,
    /// One line on what it is for, shown by `:themes`.
    description: &'static str,
    /// The SGR per role. Empty means no colour at all.
    sgr: &'static [(Paint, &'static str)],
}

/// The palette for a dark terminal: the colours this build has always used, kept
/// exactly as they were when they were a `match` inside `Paint::sgr`.
///
/// Deliberately dull. This is a REPL, not a light show: the value should read
/// first and the colour should only help a long transcript be scanned.
const DARK: Palette = Palette {
    name: "dark",
    description: "Dark terminal background. The default.",
    sgr: &[
        (Paint::Prompt, "1;34"),   // bold blue
        (Paint::Heading, "1;36"),  // bold cyan
        (Paint::Result, "0;32"),   // green
        (Paint::Error, "0;31"),    // red
        (Paint::Dim, "2;37"),      // faint grey
        (Paint::BrandRed, "1;31"), // bold red
        // Deliberately NOT 1;34, which is Prompt: a brand colour that matches
        // the prompt would make the wordmark look like something to type.
        (Paint::BrandBlue, "1;36"),
        (Paint::BrandMid, "1;35"), // bold magenta — the red/blue midpoint
        (Paint::Keyword, "0;35"),  // magenta — the words the language reserves
        // Green on purpose, and the same green as Paint::Result: a string
        // literal is a value, so it wears the colour values already do.
        (Paint::Str, "0;32"),
        (Paint::Number, "0;36"), // cyan — plain, where Heading is bold cyan
        // Bright black rather than Paint::Dim's faint white: a comment is
        // present but unreadable at a glance, not merely de-emphasised.
        (Paint::Comment, "0;90"),
        (Paint::Builtin, "0;33"), // yellow — and not Paint::Prompt's blue,
        // which would make a builtin look like something to type
        (Paint::Operator, "0;37"), // white
    ],
};

/// The palette for a terminal with a light background.
///
/// Every colour is out of the dark half of the ANSI range, which is the half
/// terminals render against white: cyan and white are near-invisible there, so
/// numbers lose cyan, operators lose white, and the brand half drops from bold to
/// plain so an error can be the loud red instead. Builtins are underlined rather
/// than yellow — yellow is the one colour with no readable dark equivalent, and
/// an underline reads against any background.
const LIGHT: Palette = Palette {
    name: "light",
    description: "Light terminal background: dark foreground colours.",
    sgr: &[
        (Paint::Prompt, "1;30"),
        (Paint::Heading, "1;34"),
        (Paint::Result, "0;32"),
        (Paint::Error, "1;31"),
        (Paint::Dim, "2;30"),
        (Paint::BrandRed, "0;31"),
        // Bold blue, as in the dark palette: the wordmark's blue is the headings'
        // blue there too, and what must differ is the prompt's.
        (Paint::BrandBlue, "1;34"),
        (Paint::BrandMid, "1;35"),
        (Paint::Keyword, "0;35"),
        (Paint::Str, "0;32"),
        (Paint::Number, "0;34"),
        (Paint::Comment, "0;90"),
        (Paint::Builtin, "4;34"),
        (Paint::Operator, "0;30"),
    ],
};

/// No colour at all, as a palette rather than as a flag on the [`Style`].
///
/// An empty table is what makes this the plain theme: [`Style::paint`] has no
/// colour to reach for, so the text comes back as itself and the bytes are
/// identical to what `REDBLUE_COLOR=never` produces — a fact the tests compare
/// rather than assume.
const PLAIN: Palette = Palette {
    name: "none",
    description: "No colour: exactly what colour-off prints.",
    sgr: &[],
};

impl Palette {
    /// Every shipped theme, in the order `:themes` lists them.
    pub const ALL: &'static [Palette] = &[DARK, LIGHT, PLAIN];

    /// The theme for a dark terminal.
    pub fn dark() -> Self {
        DARK
    }

    /// The theme for a light terminal background.
    pub fn light() -> Self {
        LIGHT
    }

    /// The theme that paints nothing.
    pub fn plain() -> Self {
        PLAIN
    }

    /// What `:theme` calls this one.
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// One line on what this one is for.
    pub fn description(&self) -> &'static str {
        self.description
    }

    /// Whether this theme paints nothing at all.
    pub fn is_plain(&self) -> bool {
        self.sgr.is_empty()
    }

    /// The SGR `role` wears here, or `None` when this theme says nothing about
    /// it — which is the caller leaving the text plain.
    ///
    /// A lookup for the same reason [`Theme::role`] is one: this is the part of
    /// colouring a user is expected to replace, and a theme that predates a role
    /// should leave that role plain rather than take a colour meant for another.
    pub fn sgr(&self, role: Paint) -> Option<&'static str> {
        self.sgr
            .iter()
            .find(|(candidate, _)| *candidate == role)
            .map(|(_, sgr)| *sgr)
    }

    /// The shipped theme called `name`, or `None` when there is none.
    ///
    /// Trimmed and matched without regard to case, because the name is typed by
    /// a human and `:theme Light ` is the same request as `:theme light`. A name
    /// that matches nothing resolves to nothing rather than to the first theme in
    /// the table: quietly switching a user to a palette they did not ask for is
    /// worse than refusing, and [`unknown_name_error`] says what there is.
    pub fn by_name(name: &str) -> Option<&'static Palette> {
        let wanted = name.trim().to_ascii_lowercase();
        Palette::ALL.iter().find(|palette| palette.name == wanted)
    }

    /// What to tell a user who asked for a theme that does not exist.
    pub fn unknown_name_error(name: &str) -> String {
        let names: Vec<&str> = Palette::ALL.iter().map(Palette::name).collect();
        format!(
            "Unknown theme '{}'. Type :theme <name> for one of: {}.",
            name.trim(),
            names.join(", ")
        )
    }

    /// A sample of what this theme looks like, painted by a real [`Style`]
    /// wearing *this* theme.
    ///
    /// Read out of the palette rather than written out as a literal, so the
    /// sample cannot drift from the colours the theme actually uses — and so the
    /// plain theme's sample is plain because it was painted, not because anyone
    /// remembered to leave the escapes out.
    pub fn sample(&self) -> String {
        let style = Style::with_theme(true, *self);
        let mut out = style.paint(Paint::Prompt, ">>> ");
        out.push_str(&style.paint(Paint::Result, "42"));
        out.push(' ');
        out.push_str(&style.paint(Paint::Str, "\"ok\""));
        out.push(' ');
        out.push_str(&style.paint(Paint::Error, "bad"));
        out.push(' ');
        out.push_str(&style.paint(Paint::Dim, "note"));
        out
    }

    /// One line per shipped theme, for `:themes`: its name, what it is for, and a
    /// sample of its colours. The theme in hand is marked with `*`.
    pub fn listing(current: &Style) -> Vec<String> {
        Palette::ALL
            .iter()
            .map(|palette| {
                let marker = if *palette == current.palette {
                    "*"
                } else {
                    " "
                };
                format!(
                    "  {}{:<6}  {:<46}  {}",
                    marker,
                    palette.name,
                    palette.description,
                    palette.sample()
                )
            })
            .collect()
    }
}

/// Whether colour is on, in which theme, and how to apply it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    enabled: bool,
    palette: Palette,
}

impl Style {
    /// Detect from the environment and the terminal, in the default theme.
    ///
    /// Deliberately NOT cached. Caching was tried first and had to be removed:
    /// it made detection untestable, because the first call anywhere in the
    /// process fixed the answer for every later test, and `REDBLUE_COLOR` set
    /// afterwards had no effect. It also bought nothing — this is consulted once
    /// per printed line, and the `println!` it feeds costs orders of magnitude
    /// more than the `is_terminal` syscall.
    pub fn detect() -> Self {
        Style::with_theme(resolve_enabled(), Palette::dark())
    }

    /// A style that is on or off regardless of the environment, in the default
    /// theme.
    ///
    /// The seam the tests use, and the only way to assert the coloured path
    /// without a TTY.
    pub fn forced(enabled: bool) -> Self {
        Style::with_theme(enabled, Palette::dark())
    }

    /// A style in a named theme, on or off regardless of the environment.
    ///
    /// The seam `:theme` is written in terms of: a theme says which colours, and
    /// not whether they are on, because `NO_COLOR` outranks a palette in the
    /// same way it outranks everything else.
    pub fn with_theme(enabled: bool, palette: Palette) -> Self {
        Style { enabled, palette }
    }

    /// Whether this style emits escape sequences.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The theme in hand.
    pub fn palette(&self) -> Palette {
        self.palette
    }

    /// What the theme in hand is called.
    pub fn theme_name(&self) -> &'static str {
        self.palette.name()
    }

    /// The SGR `role` wears in the theme in hand.
    ///
    /// `None` when the theme says nothing about the role — which is every role,
    /// in the theme that has no colour.
    pub fn sgr(&self, role: Paint) -> Option<&'static str> {
        self.palette.sgr(role)
    }

    /// Switch to the theme called `name`, for the rest of the session.
    ///
    /// Returns the palette now in hand, or a message naming what there is. The
    /// style is left exactly as it was when the name is refused: a session that
    /// silently fell back to the first theme would be showing the user colours
    /// they did not ask for and calling them the theme they named.
    pub fn set_theme(&mut self, name: &str) -> Result<Palette, String> {
        match Palette::by_name(name) {
            Some(palette) => {
                self.palette = *palette;
                Ok(*palette)
            }
            None => Err(Palette::unknown_name_error(name)),
        }
    }

    /// Wrap `text` in this role's colour, or return it untouched.
    ///
    /// When disabled this returns `text` itself — the same `String`, not a copy
    /// with escapes stripped — so a disabled session cannot differ from a plain
    /// one by a byte. The same is true of a role the theme in hand says nothing
    /// about, which is how the plain theme stays byte-identical to colour off.
    pub fn paint(&self, role: Paint, text: &str) -> String {
        if !self.enabled {
            return text.to_string();
        }
        match self.palette.sgr(role) {
            Some(sgr) => format!("\u{1b}[{}m{}\u{1b}[0m", sgr, text),
            None => text.to_string(),
        }
    }
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
