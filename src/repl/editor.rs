//! Where a session reads its lines, and what it offers while it reads.
//!
//! Reading a line is the only part of the REPL that owns a terminal, so it is
//! the only part that can differ between the two ways a session is started.
//! [`LineSource`] is that seam: [`PlainSource`] is the byte-for-byte reader
//! piped input has always used, and [`EditSource`] is the line editor a
//! terminal gets — TAB completion against the session's own names, Up and Down
//! through a history file that outlives the process, and Ctrl-C that abandons a
//! line rather than the session.
//!
//! Neither knows anything about Redblue. What a line *means* is decided by
//! [`crate::repl::Repl`], which reads a [`Line`] and does the same thing with it
//! whichever source produced it.

use std::borrow::Cow;
use std::cell::RefCell;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rustyline::completion::Completer;
use rustyline::config::Config;
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::{FileHistory, History};
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};

use super::completer::ReplCompleter;
use super::highlight::highlight;
use super::style::{Paint, Style, Theme};

/// The file a session keeps its history in, under the user's home directory.
/// Nothing there is created until a session has something to write.
///
/// `None` when there is no home directory to put it in, which is what keeps such
/// a session running on an in-memory history rather than refusing to start.
pub fn default_history_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let home = PathBuf::from(home);
    if !home.is_absolute() {
        // A relative `HOME` would put the history beside whatever directory the
        // session happened to be run from.
        return None;
    }
    Some(home.join(".redblue").join("history"))
}

/// What one read of a line source produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// A line the user finished, without its newline.
    Text(String),
    /// Ctrl-C. The line being written is abandoned and the session continues.
    Interrupted,
    /// End of input: Ctrl-D on an empty line, or a pipe that ran out.
    Eof,
}

/// A place a session reads its lines from.
///
/// The three outcomes are distinct on purpose. A [`Line::Text`] is run, a
/// [`Line::Interrupted`] is nothing at all and asks for another line, and a
/// [`Line::Eof`] ends the session. Reading an interrupt as end of input throws
/// away a session's work over a keypress; reading a failed read as an empty
/// line spins on a terminal that has gone away.
pub trait LineSource {
    fn read_line(&mut self, prompt: &str) -> io::Result<Line>;

    /// Every line this source can walk back through, oldest first.
    ///
    /// The default is none: a piped session's history lives in the session and
    /// is read back from nowhere.
    fn history(&self) -> Vec<String> {
        Vec::new()
    }

    /// Writes the lines read so far out, for the sources that keep a file.
    ///
    /// The default is nothing to do: a piped session keeps its history in the
    /// session and never owned a file.
    fn flush_history(&mut self) -> io::Result<()> {
        Ok(())
    }

    /// Whether this source draws the line being typed in colour.
    ///
    /// False for every source but [`EditSource`], and it is what a piped
    /// session's guarantee is made of: the highlight hook lives on the editor, so
    /// a session with no terminal never has one to call and never colours a line
    /// of input token by token.
    ///
    /// It says nothing about the prompt. A prompt is display text every reader
    /// paints with the session's [`Style`], so a piped session whose colour is
    /// forced on — `REDBLUE_COLOR=always` — *does* emit escapes, around the
    /// prompt and nowhere else. What a pipe never sees is an escape it did not
    /// ask for: with colour off, which is what a pipe gets by default, the
    /// painted prompt is the prompt, byte for byte.
    fn highlights(&self) -> bool {
        false
    }
}

/// The reader a session with no terminal gets: one line at a time off standard
/// input, with the prompt written to standard output.
///
/// Every line it reads is executed exactly as typed, with nothing echoed and
/// nothing drawn around it: the highlight hook is on the editor, and there is no
/// editor here. The prompt is the exception, and it is painted the same way both
/// readers paint it. So piped input, CI and `printf … | rb` see what they saw
/// before, byte for byte, whenever colour is off — which is what they get unless
/// `REDBLUE_COLOR=always` says otherwise.
pub struct PlainSource {
    /// How the prompt is coloured, decided by the session rather than here — so a
    /// piped session's prompt is the prompt, at the colour the rest of its output
    /// is at, rather than a second opinion formed here.
    style: Style,
}

impl PlainSource {
    /// A plain reader that paints its prompt in `style`'s colours.
    pub fn new(style: Style) -> Self {
        Self { style }
    }

    /// The prompt as this reader writes it: painted when colour is on, and the
    /// prompt itself when it is off.
    ///
    /// Split out of [`LineSource::read_line`] so the bytes a piped session puts
    /// on stdout are something a test can hold. Asserting on it matters because
    /// the guarantee is about those bytes: a piped session has no editor and so
    /// no highlighter, but it does have a `Style`, and `REDBLUE_COLOR=always`
    /// says the prompt is to be painted — so "a pipe emits no escapes" is false
    /// and "a pipe emits nothing but a painted prompt" is the thing that holds.
    fn painted_prompt(&self, prompt: &str) -> String {
        self.style.paint(Paint::Prompt, prompt)
    }
}

impl LineSource for PlainSource {
    fn read_line(&mut self, prompt: &str) -> io::Result<Line> {
        // Painted here and nowhere else on this path: `Style::paint` returns the
        // text itself when colour is off, so a session with colour off prints
        // exactly the prompt it was given.
        print!("{}", self.painted_prompt(prompt));
        if io::stdout().flush().is_err() {
            return Err(io::Error::other("standard output is closed"));
        }
        let mut input = String::new();
        match io::stdin().read_line(&mut input) {
            Ok(0) => Ok(Line::Eof),
            // `read_line` hands back the newline it stopped at, and a
            // [`Line::Text`] is the line *without* one: the terminator is how the
            // reader knows the line ended, not something anyone typed. `\r\n`
            // input carries both bytes, so both go. Trailing spaces stay — for
            // `:prompt rb> ` the space is part of the prompt.
            Ok(_) => Ok(Line::Text(input.trim_end_matches(['\r', '\n']).to_string())),
            Err(error) => Err(error),
        }
    }
}

/// A failure the line editor reported, as an ordinary I/O error.
///
/// The kind is kept where there was one, so a caller that asks *why* a history
/// could not be written is told "no such directory" rather than a string.
fn io_error(error: ReadlineError) -> io::Error {
    match error {
        ReadlineError::Io(error) => error,
        other => io::Error::other(other),
    }
}

/// Turns what the line editor reports into what the session means.
fn line_from(result: Result<String, ReadlineError>) -> io::Result<Line> {
    match result {
        Ok(line) => Ok(Line::Text(line)),
        Err(ReadlineError::Interrupted) => Ok(Line::Interrupted),
        Err(ReadlineError::Eof) => Ok(Line::Eof),
        Err(error) => Err(io_error(error)),
    }
}

/// The names a session has bound, shared with its line editor.
///
/// The editor needs the session's own names to complete, and the session cannot
/// lend them out while it is mid-read, so it keeps a snapshot here and refreshes
/// it between lines.
pub type SessionNames = Rc<RefCell<Vec<String>>>;

/// Completes a line against the language's vocabulary and the names a live
/// session has bound.
#[derive(Clone)]
struct SessionCompleter {
    completer: ReplCompleter,
    names: SessionNames,
    /// How to draw the line being typed, and what each class of it wears.
    style: Style,
    theme: Theme,
}

impl SessionCompleter {
    /// The start of the word under `pos` within `line`, and that word.
    ///
    /// Byte offsets are walked by character rather than by adding one: the space
    /// before the word may be a two-byte `U+00A0` or a three-byte `U+3000`, and
    /// stepping one byte past it would split the character after it in half.
    fn word_at(line: &str, pos: usize) -> Option<(usize, &str)> {
        let head = line.get(..pos)?;
        let start = head
            .char_indices()
            .rev()
            .find(|(_, ch)| ch.is_whitespace())
            .map_or(0, |(offset, ch)| offset + ch.len_utf8());
        Some((start, &head[start..]))
    }
}

impl Completer for SessionCompleter {
    type Candidate = String;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> Result<(usize, Vec<String>), ReadlineError> {
        // A cursor the caller put inside a character is not an answer this
        // completer can give, and slicing at it would panic.
        let Some((start, word)) = Self::word_at(line, pos) else {
            return Ok((0, Vec::new()));
        };
        // A filename first, where the line is the argument of a command that
        // takes one: the vocabulary has no business answering `:load sr`, and
        // this is the only place the filesystem is read at all.
        if let Some((path_start, candidates)) = self.completer.complete_path(line, pos) {
            return Ok((path_start, candidates));
        }
        Ok((
            start,
            self.completer.complete_with(word, &self.names.borrow()),
        ))
    }
}

// The line editor is generic over a "helper", which is completion plus the three
// hooks around it. Redblue has no inline hints and no per-line validation to
// offer, so those two are the defaults rather than a second place to keep
// state. Highlighting is the third: it is wired to Redblue's own lexer rather
// than left out, because a REPL that shows you what you are typing in plain
// black while it colours everything else around it reads as unfinished.
impl Hinter for SessionCompleter {
    type Hint = String;
}

impl Highlighter for SessionCompleter {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        // Borrowed when colour is off, which is not a micro-optimisation: it
        // is the same string the user typed, so a plain session and an editing
        // session cannot differ by a byte.
        if !self.style.enabled() {
            return Cow::Borrowed(line);
        }
        Cow::Owned(highlight(line, &self.style, &self.theme))
    }

    /// The prompt, painted — here and not by the session, because this is where
    /// the editor measures it.
    ///
    /// rustyline's own documentation says a prompt handed to `readline` should
    /// not carry escapes: it counts prompt columns to place the cursor and the
    /// line after it, and a prompt of the wrong measured width puts the cursor
    /// in the wrong place. `highlight_prompt` is the supported way to colour one,
    /// so the session hands over plain text and the colour arrives through this
    /// hook.
    fn highlight_prompt<'b, 's: 'b, 'p: 'b>(
        &'s self,
        prompt: &'p str,
        _default: bool,
    ) -> Cow<'b, str> {
        if !self.style.enabled() {
            return Cow::Borrowed(prompt);
        }
        Cow::Owned(self.style.paint(Paint::Prompt, prompt))
    }
}

impl Validator for SessionCompleter {}
impl Helper for SessionCompleter {}

/// The line editor a terminal session reads through.
pub struct EditSource {
    editor: Editor<SessionCompleter, FileHistory>,
    /// The same completer the editor holds, so what TAB offers can be asked for
    /// without a terminal.
    completer: SessionCompleter,
    history_path: Option<PathBuf>,
}

impl EditSource {
    /// Opens an editor offering `names`, remembering `history_path` if there is
    /// one to read and write.
    ///
    /// A history file that is not there yet is not a failure — it is what every
    /// first session meets — so it is not read at all. One that is there and
    /// cannot be read is reported rather than papered over with an empty
    /// history: a session whose history quietly stopped loading is worse than
    /// one that says so.
    pub fn new(names: SessionNames, history_path: Option<PathBuf>) -> io::Result<Self> {
        Self::with_style(names, history_path, Style::detect())
    }

    /// Opens an editor that draws what it reads in `style`'s colours.
    ///
    /// The seam `new` is written in terms of: a session that has already
    /// decided whether colour is on — because `NO_COLOR` or `REDBLUE_COLOR`
    /// said so, rather than because the terminal was detected here — passes it
    /// in, so the line and everything around it are coloured the same way.
    pub fn with_style(
        names: SessionNames,
        history_path: Option<PathBuf>,
        style: Style,
    ) -> io::Result<Self> {
        let completer = SessionCompleter {
            completer: ReplCompleter::new(),
            names,
            style,
            theme: Theme::standard(),
        };
        let config = Config::builder().auto_add_history(true).build();
        let mut editor = Editor::with_config(config).map_err(io_error)?;
        // Without a helper the line editor answers TAB with silence: it has
        // nothing to complete with, so it does nothing at all rather than
        // ringing. The completer has to be handed over, not merely built.
        editor.set_helper(Some(completer.clone()));
        if let Some(path) = history_path.as_deref() {
            if path.exists() {
                if let Err(error) = editor.load_history(&path) {
                    eprintln!(
                        "Warning: could not read history from {}: {}",
                        path.display(),
                        error
                    );
                }
            }
        }
        Ok(Self {
            editor,
            completer,
            history_path,
        })
    }

    /// What TAB after `line` at `pos` offers.
    pub fn completion(&self, line: &str, pos: usize) -> (usize, Vec<String>) {
        let context = Context::new(self.editor.history());
        match self.completer.complete(line, pos, &context) {
            Ok((start, candidates)) => (start, candidates),
            Err(_) => (0, Vec::new()),
        }
    }

    /// What TAB after `line` at `pos` offers.
    pub fn complete(&self, line: &str, pos: usize) -> Vec<String> {
        self.completion(line, pos).1
    }

    /// The offset within `line` at which its completions begin.
    pub fn complete_start(&self, line: &str, pos: usize) -> usize {
        self.completion(line, pos).0
    }

    /// The line as this editor would draw it while it is being typed.
    ///
    /// What the terminal asks for on every keystroke, reachable without one.
    pub fn highlight<'l>(&self, line: &'l str) -> Cow<'l, str> {
        Highlighter::highlight(&self.completer, line, line.len())
    }

    /// Records `line` as read, so Up and Down can walk back to it.
    pub fn record(&mut self, line: &str) {
        if line.trim().is_empty() {
            return;
        }
        // `false` here is a line the editor declined to keep — a blank one, or a
        // repeat of the last — which is the same answer a session gets today.
        let _ = self.editor.add_history_entry(line.to_string());
    }

    /// How many lines this session can walk back through.
    pub fn history_len(&self) -> usize {
        self.editor.history().len()
    }

    /// Whether `line` is one this session can walk back through.
    pub fn history_contains(&self, line: &str) -> bool {
        LineSource::history(self).iter().any(|entry| entry == line)
    }

    /// Writes the lines read so far to `path`, adding to what is already there.
    ///
    /// The directory is created here rather than in [`EditSource::new`] so that
    /// a first session — which has nothing to write until it ends — still
    /// succeeds, and so that a session whose history cannot be written says so
    /// at the moment it found out.
    ///
    /// A history that cannot be written is an error the caller is given rather
    /// than dropped: a session whose history silently stops recording is worse
    /// than one that says it could not save.
    pub fn save(&mut self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        self.editor.save_history(&path).map_err(io_error)
    }
}

impl LineSource for EditSource {
    fn read_line(&mut self, prompt: &str) -> io::Result<Line> {
        line_from(self.editor.readline(prompt))
    }

    fn history(&self) -> Vec<String> {
        self.editor
            .history()
            .into_iter()
            .map(String::clone)
            .collect()
    }

    fn flush_history(&mut self) -> io::Result<()> {
        let Some(path) = self.history_path.clone() else {
            return Ok(());
        };
        self.save(&path)
    }

    fn highlights(&self) -> bool {
        true
    }
}

/// The source this session reads from: the line editor when it has a terminal
/// to edit a line on, and the plain reader when it does not.
///
/// The terminal is the whole difference. Piped input and CI take the second
/// branch, so what they see is byte for byte what they saw when every session
/// took it — with colour off, which is what a pipe gets unless the user asked
/// otherwise. The case worth being careful about is a pipe under
/// `REDBLUE_COLOR=always`: it has no editor, so it never colours the line being
/// typed token by token, and it does paint its prompt, because a prompt is
/// display text that both readers draw through the session's [`Style`].
pub fn line_source(names: SessionNames, style: Style) -> Box<dyn LineSource> {
    if io::stdin().is_terminal() {
        match EditSource::with_style(names, default_history_path(), style) {
            Ok(editor) => return Box::new(editor),
            Err(error) => eprintln!(
                "Warning: no line editor ({}); reading lines without one",
                error
            ),
        }
    }
    Box::new(PlainSource::new(style))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::fs;
    use std::path::PathBuf;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use rustyline::error::ReadlineError;

    use super::{
        default_history_path, line_from, EditSource, Line, LineSource, PlainSource, SessionNames,
    };
    use crate::repl::style::{Paint, Style, Theme};

    /// A scratch name no other test in this binary has taken.
    fn scratch_name(prefix: &str) -> String {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let serial = COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("{}-{}", prefix, serial)
    }

    /// A history file path under test, inside the project checkout rather than a
    /// user's home directory. The file is removed if a previous run left one, so
    /// what a test sees does not depend on what ran before it.
    fn history_path(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/repl-editor");
        fs::create_dir_all(&dir).expect("scratch directory should be creatable");
        let path = dir.join(format!("{}-{}.history", name, scratch_name("serial")));
        if path.exists() {
            fs::remove_file(&path).expect("a leftover history file should be removable");
        }
        path
    }

    /// The live names a session would offer, shared with the editor the way the
    /// read loop shares them.
    fn live_names(names: &[&str]) -> SessionNames {
        Rc::new(RefCell::new(
            names.iter().map(|n| (*n).to_string()).collect(),
        ))
    }

    /// An editor for a test that never reads a terminal. Building one needs a
    /// terminal; using its completion does not.
    fn editor(names: SessionNames, history_path: Option<PathBuf>) -> EditSource {
        EditSource::new(names, history_path)
            .expect("an editor that never reads a line should be creatable")
    }

    #[test]
    fn tab_completes_a_builtin_and_only_the_names_the_session_has_bound() {
        let names = live_names(&["to_uppercase", "total"]);
        let source = editor(names.clone(), None);

        let upper = source.complete("upper", 5);
        assert!(
            upper.contains(&"uppercase".to_string()),
            "`upper` should offer `uppercase`, offered {:?}",
            upper
        );

        assert_eq!(
            source.complete("to_up", 5),
            vec!["to_uppercase".to_string()],
            "TAB after `to_up` offers exactly the live name it completes"
        );

        // A name the session has not bound is not offered, even though the rest
        // of the vocabulary shares its prefix.
        names.borrow_mut().retain(|n| n != "to_uppercase");
        assert_eq!(
            source.complete("to_up", 5),
            Vec::<String>::new(),
            "an unbound name stops being offered"
        );
    }

    #[test]
    fn completion_looks_at_the_word_under_the_cursor_and_not_the_whole_line() {
        let source = editor(live_names(&["counter"]), None);

        assert_eq!(
            source.complete("say coun", 8),
            vec!["counter".to_string()],
            "only the word before the cursor is completed"
        );
        assert_eq!(
            source.complete_start("say coun", 8),
            4,
            "and the completion starts where the word does, not at the line"
        );
    }

    #[test]
    fn edge_tab_on_an_empty_prefix_offers_the_whole_vocabulary() {
        let source = editor(live_names(&[]), None);

        let matches = source.complete("", 0);
        assert!(
            matches.contains(&"set".to_string()),
            "an empty prefix offers the keywords, offered nothing at all"
        );
        assert!(
            matches.contains(&":help".to_string()),
            "an empty prefix offers the commands too, which no other prefix reaches"
        );
        assert_eq!(source.complete_start("", 0), 0);
    }

    #[test]
    fn edge_tab_with_no_match_offers_nothing_and_does_not_panic() {
        let source = editor(live_names(&["counter"]), None);

        assert_eq!(
            source.complete("zqxjvk", 6),
            Vec::<String>::new(),
            "a prefix nothing starts with completes to nothing rather than panicking"
        );
        assert_eq!(source.complete_start("zqxjvk", 6), 0);
    }

    #[test]
    fn edge_completion_inside_a_multi_byte_character_offers_nothing() {
        let source = editor(live_names(&[]), None);

        // Offset 5 of `say \u{1F600}x` is one byte into the four-byte emoji, so
        // `line[..pos]` would slice mid-character if it were used unchecked.
        assert_eq!(
            source.complete("say \u{1F600}x", 5),
            Vec::<String>::new(),
            "a cursor inside a character completes to nothing rather than panicking"
        );
        assert_eq!(source.complete_start("say \u{1F600}x", 5), 0);
    }

    #[test]
    fn a_word_after_a_unicode_space_is_completed_from_its_own_start() {
        let source = editor(live_names(&["counter"]), None);

        // `say` is three bytes, `U+00A0` two, so `co` starts at byte 5 — one byte past
        // a naive `index + 1`, and the byte after it is inside the next
        // character if the space were stepped byte-wise from the wrong end.
        assert_eq!(source.complete_start("say\u{00A0}coun", 9), 5);
        assert_eq!(
            source.complete("say\u{00A0}coun", 9),
            vec!["counter".to_string()],
            "the word after a multi-byte space is the one completed"
        );
    }

    #[test]
    fn history_survives_a_restart_of_the_session() {
        let path = history_path("restart");
        let names = live_names(&[]);

        let mut first = editor(names.clone(), Some(path.clone()));
        assert_eq!(first.history_len(), 0, "a fresh session has no history");
        first.record("set restart_probe to 1");
        first.save(&path).expect("a fresh history file is writable");

        drop(first);

        let second = editor(names, Some(path.clone()));
        assert_eq!(
            second.history_len(),
            1,
            "the restarted session offers the line the previous session read"
        );
        assert!(
            second.history_contains("set restart_probe to 1"),
            "up-arrow walks the line the previous session typed"
        );
    }

    #[test]
    fn edge_a_history_file_that_does_not_exist_yet_is_not_an_error() {
        let path = history_path("absent");
        assert!(
            !path.exists(),
            "the history file under test must not exist before the session starts"
        );

        let source = editor(live_names(&[]), Some(path.clone()));
        assert_eq!(
            source.history_len(),
            0,
            "a session with no history file yet starts with an empty history, not a failure"
        );
        assert!(
            !source.history_contains("set restart_probe to 1"),
            "there is nothing to walk back through"
        );
        assert_eq!(
            LineSource::history(&source),
            Vec::<String>::new(),
            "and a session that restored nothing has an empty history to restore from"
        );
    }

    #[test]
    fn edge_a_history_file_that_is_not_writable_is_reported_rather_than_lost() {
        // A regular file where the directory has to be: neither the directory
        // nor the file can ever be created, on any user and on any machine.
        let dir = history_path("unwritable");
        let blocker = dir.with_file_name(dir.file_name().unwrap().to_string_lossy().to_string());
        fs::write(&blocker, "not a directory").expect("the blocker should be creatable");

        let mut source = editor(live_names(&[]), Some(blocker.join("history")));
        source.record("set never_saved to 1");

        let error = source
            .save(&blocker.join("history"))
            .expect_err("a history file under a regular file cannot be written");
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::AlreadyExists,
            "the failure names why the history was not written: {}",
            error
        );
    }

    #[test]
    fn the_history_directory_is_created_by_the_session_that_writes_to_it() {
        let path = history_path("first-run").with_file_name("first-run-dir/history");
        let mut source = editor(live_names(&[]), Some(path.clone()));
        source.record("set first_run to 1");

        source
            .save(&path)
            .expect("a first session has no history directory, and must make one");

        let restored = editor(live_names(&[]), Some(path));
        assert!(
            restored.history_contains("set first_run to 1"),
            "and what it wrote is there for the next session"
        );
    }

    #[test]
    fn a_plain_source_has_no_history_file_and_never_fails_to_flush() {
        let mut plain = PlainSource::new(Style::forced(false));
        assert!(
            plain.flush_history().is_ok(),
            "a piped session has no history file, so flushing one is nothing to do"
        );
    }

    #[test]
    fn the_default_history_path_is_under_the_home_directory() {
        // The path only means anything where there is a home directory to put it
        // in, and the assertion has to hold whichever that is.
        if let Some(path) = default_history_path() {
            assert_eq!(
                path.file_name(),
                Some(std::ffi::OsStr::new("history")),
                "the history file is named `history`, got {}",
                path.display()
            );
            assert_eq!(
                path.parent().and_then(|p| p.file_name()),
                Some(std::ffi::OsStr::new(".redblue")),
                "and it lives in a `.redblue` directory, got {}",
                path.display()
            );
        }
    }

    #[test]
    fn the_editor_maps_the_terminals_interrupts_onto_what_the_session_means() {
        // The three things a line editor can report that are not a line: a
        // finished line, end of input, and an interrupt. Only an interrupt is
        // neither a line nor the end, and reading it as the end throws the
        // session away over a keypress.
        assert_eq!(
            line_from(Ok("say 1".to_string())).unwrap(),
            Line::Text("say 1".to_string()),
            "a finished line is the line"
        );
        assert_eq!(
            line_from(Err(ReadlineError::Eof)).unwrap(),
            Line::Eof,
            "end of input is the end of input"
        );
        assert_eq!(
            line_from(Err(ReadlineError::Interrupted)).unwrap(),
            Line::Interrupted,
            "and an interrupt is neither a line nor the end"
        );

        assert!(
            line_from(Err(ReadlineError::Io(std::io::Error::other("no terminal")))).is_err(),
            "an I/O failure is a failure, not an empty line and not an interrupt"
        );
    }

    /// An editor that draws in `style`'s colours, built without a terminal.
    fn coloured_editor(style: Style) -> EditSource {
        EditSource::with_style(live_names(&[]), None, style)
            .expect("an editor that never reads a line should be creatable")
    }

    #[test]
    fn the_editor_hook_colours_a_line_typed_at_the_prompt() {
        let source = coloured_editor(Style::forced(true));

        let coloured = source.highlight("say \"hi\"").into_owned();
        assert!(
            coloured.contains(&Style::forced(true).paint(Paint::Keyword, "say")),
            "`say` is a keyword and takes the keyword colour through the editor's own hook, got {}",
            coloured.escape_debug()
        );
        assert!(
            coloured.contains(&Style::forced(true).paint(Paint::Str, "\"hi\"")),
            "and `\"hi\"` takes the string colour, got {}",
            coloured.escape_debug()
        );
        assert_eq!(
            coloured.matches('\u{1b}').count(),
            4,
            "the drawn line is `say` and `\"hi\"` each wrapped once and nothing else, got {}",
            coloured.escape_debug()
        );
    }

    #[test]
    fn an_editor_with_colour_off_hands_the_line_straight_back() {
        let source = coloured_editor(Style::forced(false));

        assert_eq!(
            source.highlight("say \"hi\"").into_owned(),
            "say \"hi\"",
            "with colour off the editor borrows the line rather than building a copy of it"
        );
    }

    /// What a piped session puts on stdout: no editor, so no highlighter and no
    /// token-by-token colouring of the line being typed — and the prompt painted
    /// through the session's [`Style`], which is the one thing a pipe does colour.
    ///
    /// The test asserts the bytes rather than the intent. It used to assert only
    /// `!source.highlights()`, which stayed true whatever was written: a piped
    /// session under `REDBLUE_COLOR=always` emits escape codes today, and the
    /// promise that it emits none is one the reader broke when `PlainSource`
    /// learned to paint. What still holds, and what a user of a pipe depends on,
    /// is narrower: no highlighter on this path, a prompt that is the prompt
    /// byte for byte when colour is off, and a painted prompt when it is on.
    #[test]
    fn edge_a_piped_session_paints_its_prompt_and_never_its_input() {
        // Standard input is not a terminal under `cargo test`, which is the
        // situation a pipe is in.
        let source = super::line_source(live_names(&["counter"]), Style::forced(true));
        assert!(
            !source.highlights(),
            "a session with no terminal must not get a highlighter, however colour is configured"
        );
        assert!(
            !PlainSource::new(Style::forced(false)).highlights(),
            "and the plain reader never claims to colour anything"
        );

        let plain = PlainSource::new(Style::forced(false));
        assert_eq!(
            plain.painted_prompt(">>> "),
            ">>> ",
            "with colour off — what a pipe gets unless `REDBLUE_COLOR` says \
             otherwise — the painted prompt is the prompt, byte for byte"
        );

        let on = Style::forced(true);
        let painted = PlainSource::new(on).painted_prompt(">>> ");
        assert_eq!(
            painted,
            on.paint(Paint::Prompt, ">>> "),
            "with colour forced on the prompt is painted as a prompt and nothing \
             else is, got {:?}",
            painted.escape_debug()
        );
        assert_eq!(
            painted.matches('\u{1b}').count(),
            2,
            "one open and one close for the prompt and no third escape, so the \
             escapes a forced-on pipe emits are exactly this one paint, got {:?}",
            painted.escape_debug()
        );
        assert!(
            painted.contains(">>> "),
            "and the prompt itself survives the painting, got {:?}",
            painted.escape_debug()
        );
    }

    #[test]
    fn a_shipped_theme_has_an_opinion_about_every_class_it_lists() {
        let theme = Theme::standard();
        for class in crate::repl::style::TokenClass::ALL {
            assert_eq!(
                theme.role(*class),
                Some(class.role()),
                "{class:?} is listed as a class, so the shipped theme must colour it"
            );
        }
        assert_eq!(
            crate::repl::style::TokenClass::ALL.len(),
            6,
            "a class added without a theme colour would be a class nobody can draw"
        );
    }

    /// TAB in the argument of a command that takes a path offers paths, not the
    /// language's vocabulary.
    ///
    /// `:load` and its three siblings are the only arguments that are filenames, and
    /// this is the only place TAB offers one. Before this the completer read the word
    /// under the cursor and matched it against keywords, builtins, commands and the
    /// session's names — so `:load sr` was completed against `:save` and `say` and
    /// `skip`, and every filename in the argument had to be typed whole and blind.
    #[test]
    fn edge_tab_after_a_command_that_takes_a_path_offers_paths() {
        let source = editor(live_names(&[]), None);

        // The three prefixes of the DoD: nothing typed, one letter, and one that
        // narrows. All three are answered from the directory this checkout has,
        // which is why the assertion is `contains` rather than an exact list: the
        // point is that a *directory* is in the answer at all.
        for line in [":load ", ":load s", ":load sr"] {
            let offered = source.complete(line, line.len());
            assert!(
                offered.contains(&"src/".to_string()),
                "{line:?} offers the directory this checkout has, offered \
                 {offered:?} instead"
            );
        }
        assert!(
            source
                .complete(":load ", 6)
                .contains(&"Cargo.toml".to_string()),
            "and an empty fragment offers the files as well as the directories, \
             which is what the whole of the directory means"
        );
        assert!(
            source.complete(":load sr", 8).len() < 20,
            "narrowing to `sr` narrows the answer rather than re-offering the \
             whole vocabulary: {:?}",
            source.complete(":load sr", 8)
        );

        let offered = source.complete(":load s", 7);
        assert!(
            offered.contains(&"src/".to_string()),
            "a directory is offered with the separator that means it is one: {offered:?}"
        );
        assert!(
            !offered.contains(&"say".to_string()) && !offered.contains(&":save".to_string()),
            "and the language's vocabulary is not offered in a filename argument, \
         which is what it used to offer: {offered:?}"
        );
    }

    /// The gate the other side of: a path is offered in a command that takes one and
    /// nowhere else. `say "a` is a string, and a reader typing a word into one is not
    /// asking for a directory listing — the failure mode of a completer that offered
    /// paths everywhere.
    #[test]
    fn edge_tab_outside_a_file_argument_offers_no_path() {
        let source = editor(live_names(&[]), None);

        for line in ["say \"a", "set x to \"s", "sr", "say \"src/"] {
            assert_eq!(
                source.complete(line, line.len()),
                Vec::<String>::new(),
                "{line:?} is not the argument of a command that takes a path, so \
             nothing is offered for it"
            );
        }
    }
}
