//! The REPL's one colour system: a palette, and the gate that decides whether
//! anything is drawn with it.
//!
//! Two rules govern every byte painted from here:
//!
//! 1. A colour wraps text. Nothing in this module formats a message; it only
//!    puts an SGR sequence in front of a string another piece already wrote and
//!    a reset behind it. A session with colour off therefore prints exactly what
//!    it printed before any of this existed, which is what `tests/fixtures`
//!    holds against.
//! 2. The gate is a pure function of a [`TerminalInfo`], never of a live
//!    terminal. `cargo test` has no terminal, and a gate that could only be
//!    observed on one could not be tested on one.
//!
//! The refusal order is the ordinary one, and it is fixed: a platform that
//! cannot show an escape at all refuses first, then the user's own `Off`, then
//! `On` forcing past everything the terminal *asks* for, then `Auto` reading
//! `NO_COLOR`, `TERM` and finally whether there is a terminal at all.

use std::env;
use std::fmt;
use std::io::{self, IsTerminal};

/// What the user asked for: paint regardless, never paint, or paint only when
/// the terminal says it can.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    /// Follow the terminal. The default, and what a session starts in.
    #[default]
    Auto,
    /// Paint even where the terminal would rather not.
    On,
    /// Paint nothing at all, whatever the terminal is.
    Off,
}

impl ColorMode {
    /// The mode named by `word`, or `None` for anything that is not a mode.
    ///
    /// Case-insensitive and trimmed, because both spellings of `On` reach the
    /// same command and a refused mode is a refusal the user did not mean. A
    /// word with anything else in it — `on extra`, `yellow`, the empty string —
    /// names no mode, and guessing one would leave a session painted in a colour
    /// nobody asked for.
    pub fn parse(word: &str) -> Option<ColorMode> {
        match word.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(ColorMode::Auto),
            "on" => Some(ColorMode::On),
            "off" => Some(ColorMode::Off),
            _ => None,
        }
    }

    /// The mode as the user types it, for the report in `:help`.
    pub fn as_str(self) -> &'static str {
        match self {
            ColorMode::Auto => "auto",
            ColorMode::On => "on",
            ColorMode::Off => "off",
        }
    }
}

impl fmt::Display for ColorMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What a `TERM` says about the terminal it names.
///
/// Three answers, because the two that matter are opposites and the third is
/// "we were not told".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalKind {
    /// A terminal that renders SGR colour sequences.
    Ansi,
    /// A terminal that has been told not to be clever: no colour, no cursor
    /// movement, no clearing. `dumb` is the one name that means this.
    Dumb,
    /// No `TERM` at all, or one that cannot be read — see
    /// [`terminal_kind`].
    Unknown,
}

impl TerminalKind {
    /// Whether a terminal of this kind is one that renders colour.
    pub fn paints(self) -> bool {
        matches!(self, TerminalKind::Ansi)
    }
}

/// The longest a `TERM` may be and still be believed.
///
/// `TERM` names a terminfo entry, and no entry name approaches this. A value
/// longer than it is not a name that has been truncated by something else — it
/// is a value that will be reported back to the user by `:help`, and one that
/// carries a control character can carry an escape sequence into that report.
const MAX_TERM_LENGTH: usize = 64;

/// What `term` says about the terminal, read without trusting it.
///
/// `None`, empty and unreadable are all [`TerminalKind::Unknown`], and an
/// unknown terminal is not painted: a `TERM` that cannot be read is not
/// evidence that the terminal can show colour, and it is the one string this
/// module is obliged to print back to the user. A `TERM` carrying a control
/// character or a non-ASCII byte lands here too, which is what keeps
/// `TERM=$'\e[31m'` out of the middle of a terminal.
pub fn terminal_kind(term: Option<&str>) -> TerminalKind {
    let Some(term) = term else {
        return TerminalKind::Unknown;
    };
    if term.is_empty() || term.len() > MAX_TERM_LENGTH {
        return TerminalKind::Unknown;
    }
    if term == "dumb" {
        return TerminalKind::Dumb;
    }
    if !term.is_ascii() || term.chars().any(char::is_control) {
        return TerminalKind::Unknown;
    }
    TerminalKind::Ansi
}

/// Everything the gate reads about the terminal it is deciding for.
///
/// All fields are public and owned so a test can state a terminal outright. A
/// gate that could only be reached through the real environment would be a gate
/// that could only be tested through the real environment, and `cargo test` has
/// none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalInfo {
    /// Whether stdout is a terminal at all.
    pub is_tty: bool,
    /// The `TERM` of the terminal, as the environment spells it.
    pub term: Option<String>,
    /// `COLORTERM`, set by terminals that promise truecolour.
    pub colorterm: Option<String>,
    /// `NO_COLOR`, set by a user asking for no colour anywhere.
    pub no_color: Option<String>,
    /// Whether this is a Windows build, which has a console that needs telling.
    pub windows: bool,
    /// Whether the console can render escape sequences at all.
    pub ansi_ready: bool,
}

impl TerminalInfo {
    /// The terminal this process is actually talking to.
    ///
    /// Every variable is read once, here, so the gate below is a function of
    /// values rather than of a second trip through the environment.
    pub fn detect() -> TerminalInfo {
        let is_tty = io::stdout().is_terminal();
        TerminalInfo {
            is_tty,
            term: env::var("TERM").ok(),
            colorterm: env::var("COLORTERM").ok(),
            no_color: env::var("NO_COLOR").ok(),
            windows: cfg!(windows),
            // Asked whether or not stdout is a terminal: a redirected pipe is
            // still a stream this platform knows how to write escapes into, and
            // `:colors on` is allowed to write them. Only a console that cannot
            // be told to interpret them is a refusal, and that is answered
            // whatever stdout is.
            ansi_ready: enable_virtual_terminal_processing(),
        }
    }
}

/// Why the gate said no, when it said no.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The console cannot render an escape sequence at all.
    NoAnsiSupport,
    /// stdout is not a terminal.
    NotATerminal,
    /// `NO_COLOR` is set to something.
    NoColor,
    /// `TERM` is `dumb`.
    DumbTerminal,
    /// `TERM` names no readable terminal.
    UnreadableTerminal,
}

impl Refusal {
    /// The refusal as `:help` reports it, in a sentence that starts nowhere
    /// strange when prefixed with "Colors: auto (...)".
    pub fn as_str(self) -> &'static str {
        match self {
            Refusal::NoAnsiSupport => "this console cannot show colour",
            Refusal::NotATerminal => "output is not a terminal",
            Refusal::NoColor => "NO_COLOR is set",
            Refusal::DumbTerminal => "TERM is dumb",
            Refusal::UnreadableTerminal => "TERM names no readable terminal",
        }
    }
}

/// The palette: which colour each kind of text is drawn in.
///
/// A colour is a string, so a theme can be written down and compared. This is
/// one theme and there is no second: Redblue ships a single look, and the
/// indirection exists so that a second one can be added without moving a
/// single call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    /// The prompt a session waits at.
    pub prompt: &'static str,
    /// The `= value` a program left behind.
    pub result: &'static str,
    /// Anything the session reports as a failure.
    pub error: &'static str,
    /// A heading, and every command's name in it.
    pub heading: &'static str,
    /// A name in a listing.
    pub name: &'static str,
    /// A value in a listing.
    pub value: &'static str,
}

/// The one theme Redblue ships: a prompt you can find, results that read as
/// answers, errors that read as errors, and a bold heading.
pub const THEME: Theme = Theme {
    prompt: "\u{1b}[36m",
    result: "\u{1b}[32m",
    error: "\u{1b}[31m",
    heading: "\u{1b}[1m",
    name: "\u{1b}[36m",
    value: "\u{1b}[33m",
};

/// Ends every painted run, so a reset colour cannot leak into what a user prints.
const RESET: &str = "\u{1b}[0m";

/// One thing in the session worth drawing differently from another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Paint {
    Prompt,
    Result,
    Error,
    Heading,
    Name,
    Value,
}

impl Paint {
    /// The escape sequence this paints with, from [`THEME`].
    pub fn code(self) -> &'static str {
        match self {
            Paint::Prompt => THEME.prompt,
            Paint::Result => THEME.result,
            Paint::Error => THEME.error,
            Paint::Heading => THEME.heading,
            Paint::Name => THEME.name,
            Paint::Value => THEME.value,
        }
    }
}

/// The gate, and the only thing in the REPL that decides whether to paint.
///
/// Copyable and comparable so a test can state one outright. A `Style` holds no
/// reference to the terminal it read: everything the report needs is the
/// refusal, kept as a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    mode: ColorMode,
    enabled: bool,
    refusal: Option<Refusal>,
}

impl Style {
    /// A style that paints nothing, whatever the terminal is.
    pub fn plain() -> Style {
        Style {
            mode: ColorMode::Off,
            enabled: false,
            refusal: None,
        }
    }

    /// The style `mode` asks for on `terminal`.
    pub fn resolve(mode: ColorMode, terminal: &TerminalInfo) -> Style {
        match refuse(mode, terminal) {
            Some(refusal) => Style {
                mode,
                enabled: false,
                refusal: Some(refusal),
            },
            None => Style {
                mode,
                enabled: !matches!(mode, ColorMode::Off),
                refusal: None,
            },
        }
    }

    /// The style for `terminal` in whatever mode the session is in, asked of the
    /// real environment.
    pub fn detect(mode: ColorMode) -> Style {
        Style::resolve(mode, &TerminalInfo::detect())
    }

    /// The mode this style came from.
    pub fn mode(self) -> ColorMode {
        self.mode
    }

    /// Whether a painted string comes back wrapped.
    pub fn enabled(self) -> bool {
        self.enabled
    }

    /// The mode, and why `auto` landed where it did, for `:help`.
    ///
    /// One line, and it is the only place a refusal is reported: a user who
    /// cannot work out why their terminal is plain has no other way to ask.
    /// `Refusal::as_str` values are this module's own words, never anything read
    /// out of the environment, so a `TERM` full of escapes cannot reach here.
    pub fn report(self) -> String {
        match (self.mode, self.enabled) {
            (ColorMode::Off, _) => "Colors: off.".to_string(),
            (_, true) if self.mode == ColorMode::On => "Colors: on.".to_string(),
            (_, true) => "Colors: auto (this terminal is painted).".to_string(),
            (_, false) => match self.refusal {
                Some(refusal) => format!("Colors: {} ({}).", self.mode, refusal.as_str()),
                None => format!("Colors: {}.", self.mode),
            },
        }
    }

    /// `text` in the colour `paint` names — or `text` itself, byte for byte,
    /// when this style does not paint.
    ///
    /// Nothing is formatted here. The caller has already written the message,
    /// and this only puts a sequence in front of it and a reset behind it, so
    /// every assertion about what the REPL says holds whether colour is on or
    /// off.
    pub fn paint(self, paint: Paint, text: &str) -> String {
        if !self.enabled {
            return text.to_string();
        }
        format!("{}{}{}", paint.code(), text, RESET)
    }
}

/// Why `mode` will not paint on `terminal`, in the order that decides it.
///
/// The order is the whole design. A console that cannot render an escape
/// sequence is refused first, under every mode, because the alternative is the
/// characters of a sequence printed into the middle of a line the user is
/// reading — so no forced colour outranks the platform's ability to show one.
/// After that only `Auto` asks anything, and it asks in the order a user would:
/// is this a terminal, did they ask for no colour, and what does it call itself.
/// `On` asks the user: they have said yes, and the only thing left to overrule
/// them is the platform.
fn refuse(mode: ColorMode, terminal: &TerminalInfo) -> Option<Refusal> {
    if !terminal.ansi_ready {
        return Some(Refusal::NoAnsiSupport);
    }
    if mode != ColorMode::Auto {
        return None;
    }

    if !terminal.is_tty {
        return Some(Refusal::NotATerminal);
    }

    if terminal
        .no_color
        .as_deref()
        .is_some_and(|value| !value.is_empty())
    {
        return Some(Refusal::NoColor);
    }

    match terminal_kind(terminal.term.as_deref()) {
        TerminalKind::Dumb => Some(Refusal::DumbTerminal),
        TerminalKind::Unknown => {
            // An unknown `TERM` is a refusal only where nothing else has said
            // this is a terminal that paints. Windows sets no `TERM` of its own
            // and sets `COLORTERM` when it is a terminal that does; a Unix
            // session with no `TERM` is a session being run by something that
            // never said.
            let colorterm = terminal
                .colorterm
                .as_deref()
                .is_some_and(|value| !value.is_empty());
            if colorterm || terminal.windows {
                None
            } else {
                Some(Refusal::UnreadableTerminal)
            }
        }
        TerminalKind::Ansi => None,
    }
}

/// Whether this console can render SGR sequences, enabling them if it can be
/// told to.
///
/// True everywhere but Windows, where a console has to be asked before it
/// interprets a sequence rather than printing its characters. The declaration is
/// the console API from the platform SDK, called with the handle and flags that
/// API documents; there is no crate for it in this dependency tree and adding
/// one for two functions is not a trade worth making.
pub fn enable_virtual_terminal_processing() -> bool {
    #[cfg(windows)]
    {
        windows_console::enable()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// The Windows console, asked to interpret escape sequences.
///
/// This half of the module is never compiled on a Unix build, so nothing in the
/// test suite can reach it. What the tests *can* reach is what the REPL does
/// with the answer: a console that could not be told degrades to plain, and the
/// degrade path is the same code on every platform.
#[cfg(windows)]
mod windows_console {
    use std::ffi::c_void;
    use std::os::raw::c_int;

    /// `(DWORD)-11`: the pseudo-handle for the console's standard output.
    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    /// The console mode flag that makes stdout interpret SGR sequences.
    const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetStdHandle(nstdhandle: u32) -> *mut c_void;
        fn GetConsoleMode(hconsolehandle: *mut c_void, lpmodep: *mut u32) -> c_int;
        fn SetConsoleMode(hconsolehandle: *mut c_void, dwmode: u32) -> c_int;
    }

    pub fn enable() -> bool {
        // SAFETY: the three calls are the console API with the arguments its
        // documentation specifies. `GetStdHandle` returns a handle this process
        // owns or null, which is checked; `GetConsoleMode` writes exactly one
        // `u32` through the pointer it is given; and `SetConsoleMode` is passed
        // only a mode `GetConsoleMode` reported.
        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            if handle.is_null() {
                return false;
            }
            let mut mode: u32 = 0;
            if GetConsoleMode(handle, &mut mode) == 0 {
                return false;
            }
            if mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0 {
                return true;
            }
            SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A terminal that paints, unless a test says otherwise.
    ///
    /// Every field is a parameter because every field changes the answer, and a
    /// gate with a field no test can vary is a gate no test can hold to account.
    fn terminal(term: &str) -> TerminalInfo {
        TerminalInfo {
            is_tty: true,
            term: Some(term.to_string()),
            colorterm: None,
            no_color: None,
            windows: false,
            ansi_ready: true,
        }
    }

    #[test]
    fn a_color_terminal_in_auto_mode_paints() {
        let style = Style::resolve(ColorMode::Auto, &terminal("xterm-256color"));

        assert!(style.enabled(), "a TERM that renders colour gets colour");
        assert_eq!(style.mode(), ColorMode::Auto);
        assert_eq!(style.refusal, None);
    }

    #[test]
    fn off_paints_nothing_even_on_a_terminal_that_asks_for_color() {
        let style = Style::resolve(ColorMode::Off, &terminal("xterm-256color"));

        assert!(
            !style.enabled(),
            ":colors off is the user overriding a terminal that offers colour"
        );
        assert_eq!(
            style.paint(Paint::Result, "= 5"),
            "= 5",
            "a painted run comes back as the text and nothing else"
        );
        assert_eq!(
            style.paint(Paint::Error, "Error: no"),
            "Error: no",
            "and the same is true of every colour, not only the one under test"
        );
    }

    #[test]
    fn a_mode_is_named_by_exactly_three_words() {
        assert_eq!(ColorMode::parse("auto"), Some(ColorMode::Auto));
        assert_eq!(ColorMode::parse("on"), Some(ColorMode::On));
        assert_eq!(ColorMode::parse("off"), Some(ColorMode::Off));
        assert_eq!(ColorMode::parse("  ON  "), Some(ColorMode::On));
        assert_eq!(ColorMode::parse("On"), Some(ColorMode::On));

        for word in ["chartreuse", "on extra", "", "1", "none", "tru"] {
            assert_eq!(
                ColorMode::parse(word),
                None,
                "{word:?} names no mode, so it must be refused rather than guessed"
            );
        }
    }

    #[test]
    fn a_painted_string_is_the_same_text_between_two_escapes() {
        let style = Style::resolve(ColorMode::On, &terminal("xterm"));

        for paint in [
            Paint::Prompt,
            Paint::Result,
            Paint::Error,
            Paint::Heading,
            Paint::Name,
            Paint::Value,
        ] {
            let painted = style.paint(paint, "text");
            assert_eq!(
                painted,
                format!("{}{}{}", paint.code(), "text", RESET),
                "{paint:?} paints the text it is given and nothing else"
            );
            assert!(
                painted.contains("text"),
                "{paint:?} keeps the text it wraps: {painted:?}"
            );
            assert!(
                painted.ends_with(RESET),
                "{paint:?} resets afterwards, so a colour cannot leak into what \
                 the user prints: {painted:?}"
            );
        }
    }

    #[test]
    fn no_color_disables_colour_on_a_terminal_that_otherwise_paints() {
        let mut refused = terminal("xterm-256color");
        refused.no_color = Some("1".to_string());
        let style = Style::resolve(ColorMode::Auto, &refused);

        assert!(
            !style.enabled(),
            "NO_COLOR=1 is a user asking for no colour, on a terminal or not"
        );
        assert_eq!(style.refusal, Some(Refusal::NoColor));
        assert_eq!(
            style.paint(Paint::Prompt, ">>> "),
            ">>> ",
            "and nothing painted reaches the terminal"
        );
    }

    #[test]
    fn a_dumb_terminal_is_never_painted() {
        let style = Style::resolve(ColorMode::Auto, &terminal("dumb"));

        assert!(!style.enabled(), "dumb means dumb");
        assert_eq!(style.refusal, Some(Refusal::DumbTerminal));
    }

    /// `NO_COLOR` is honoured because it is present and not empty, so a variable
    /// set to nothing says nothing — and `TERM` is read the same way, with
    /// `dumb` the one name that refuses.
    #[test]
    fn edge_no_color_set_to_nothing_is_not_no_color() {
        let mut empty = terminal("xterm-256color");
        empty.no_color = Some(String::new());
        assert!(
            Style::resolve(ColorMode::Auto, &empty).enabled(),
            "NO_COLOR= is the variable set to nothing, and says nothing"
        );

        let mut whitespace = terminal("xterm-256color");
        whitespace.no_color = Some("   ".to_string());
        assert!(
            !Style::resolve(ColorMode::Auto, &whitespace).enabled(),
            "whitespace is not nothing: it is a user who exported the variable"
        );

        assert_eq!(
            terminal_kind(Some("")),
            TerminalKind::Unknown,
            "an empty TERM names no terminal"
        );
        assert_eq!(
            terminal_kind(None),
            TerminalKind::Unknown,
            "and an absent one does not either"
        );
        assert_eq!(
            terminal_kind(Some("dumb-color")),
            TerminalKind::Ansi,
            "only `dumb` itself is dumb; a name beginning with it is another one"
        );
    }

    /// A `TERM` that cannot be read is refused, and so is one carrying an escape.
    ///
    /// `TERM` is the one string from the environment this module puts in front
    /// of a user, in the `:help` report. One carrying a control character could
    /// put a sequence there; one that is not a terminfo name at all is not
    /// evidence that the terminal paints. Both are `Unknown`, and `Unknown` is
    /// not painted unless the platform has already said it is a colour terminal.
    #[test]
    fn edge_an_unparsable_term_is_not_believed() {
        for term in [
            "xterm\u{1b}[31m",
            "xterm\nTERM",
            "xterm\u{0}color",
            "xterm\u{7}",
            "term\u{80}",
        ] {
            assert_eq!(
                terminal_kind(Some(term)),
                TerminalKind::Unknown,
                "{term:?} is not a terminal name, so it must not be believed"
            );

            let mut refused = terminal(term);
            refused.term = Some(term.to_string());
            let style = Style::resolve(ColorMode::Auto, &refused);
            assert!(
                !style.enabled(),
                "{term:?} on a Unix session is not evidence of a colour terminal"
            );
            assert_eq!(style.refusal, Some(Refusal::UnreadableTerminal));
        }
    }

    /// A terminal that has not been told to interpret sequences is a plain
    /// terminal, whatever the mode says.
    ///
    /// This is the Windows console with virtual-terminal processing left off,
    /// which is the default on every Windows console older than Windows 10 and
    /// the state a redirect can leave any of them in. The alternative to
    /// degrading is printing `\u{1b}[31m` into a console as thirty-one readable
    /// characters in the middle of the line the user is reading.
    #[test]
    fn edge_a_console_without_ansi_support_degrades_to_plain() {
        let mut legacy = terminal("xterm-256color");
        legacy.ansi_ready = false;

        for mode in [ColorMode::Auto, ColorMode::On, ColorMode::Off] {
            let style = Style::resolve(mode, &legacy);
            assert!(
                !style.enabled(),
                "{mode:?} cannot force a colour onto a console that cannot show \
                 one"
            );
            assert_eq!(style.refusal, Some(Refusal::NoAnsiSupport));
            assert!(
                !style.paint(Paint::Result, "= 5").contains('\u{1b}'),
                "{mode:?} emits no escape into a console that cannot render one"
            );
        }
    }

    /// What the REPL builds on startup must obey the same gate as anything it
    /// is handed, or the gate is only a test fixture.
    #[test]
    fn the_detected_style_never_claims_colour_it_does_not_have() {
        for mode in [ColorMode::Auto, ColorMode::On, ColorMode::Off] {
            let style = Style::detect(mode);
            let painted = style.paint(Paint::Result, "= 5");
            assert!(
                painted == "= 5" || (painted.starts_with('\u{1b}') && painted.ends_with(RESET)),
                "a detected style either paints nothing or paints a whole run, \
                 and never a half one: {painted:?}"
            );
        }
    }

    /// The report in `:help` names the mode and, when `auto` landed somewhere
    /// other than painting, says why.
    #[test]
    fn the_help_report_names_the_mode_and_the_refusal() {
        assert_eq!(
            Style::resolve(ColorMode::Off, &terminal("xterm")).report(),
            "Colors: off.",
            "a mode the user chose is reported as itself and nothing more"
        );
        assert_eq!(
            Style::resolve(ColorMode::On, &terminal("xterm")).report(),
            "Colors: on.",
            "and so is a mode forced on"
        );
        assert_eq!(
            Style::resolve(ColorMode::Auto, &terminal("dumb")).report(),
            "Colors: auto (TERM is dumb).",
            "an auto that landed plain says which terminal fact stopped it"
        );

        let piped = TerminalInfo {
            ansi_ready: false,
            ..terminal("xterm")
        };
        assert_eq!(
            Style::resolve(ColorMode::Auto, &piped).report(),
            "Colors: auto (this console cannot show colour).",
            "a redirected stdout is not a console that cannot show colour, and \
             the report has to say which it is"
        );
    }

    /// Every refusal has a sentence, and no sentence carries a character read
    /// out of the environment — `TERM` ends up in the report's neighbourhood and
    /// must not be able to escape into a terminal through it.
    #[test]
    fn edge_every_refusal_has_a_sentence_and_none_carries_an_escape() {
        let refusals = [
            Refusal::NoAnsiSupport,
            Refusal::NotATerminal,
            Refusal::NoColor,
            Refusal::DumbTerminal,
            Refusal::UnreadableTerminal,
        ];
        for refusal in refusals {
            let sentence = refusal.as_str();
            assert!(!sentence.is_empty(), "{refusal:?} must say something");
            assert!(
                !sentence.chars().any(char::is_control),
                "{refusal:?} says {:?}, which carries a control character",
                sentence
            );
        }
    }
}
